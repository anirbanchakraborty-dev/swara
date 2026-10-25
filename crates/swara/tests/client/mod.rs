//! A language client for the tests. It runs `swara lsp` with `SWARA_TEST=1` and speaks the
//! protocol to it over standard input and output.

// Each test file uses a different part of the client.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::BufReader;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use lsp_server::{Message, Notification, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidOpenTextDocument, DidSaveTextDocument, Exit, Initialized,
    Notification as _, PublishDiagnostics,
};
use lsp_types::request::{Initialize, Request, Shutdown};
use lsp_types::{
    ClientCapabilities, Diagnostic, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, GeneralClientCapabilities, InitializeParams, InitializeResult,
    InitializedParams, PositionEncodingKind, PublishDiagnosticsParams, Range,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem, Uri,
    VersionedTextDocumentIdentifier,
};
use serde_json::Value;

const REPLY_WAIT: Duration = Duration::from_secs(10);

/// A `file://` URI for an absolute path, percent-encoded as Neovim sends it, which leaves the
/// characters RFC 3986 allows in a path as they are.
pub fn uri(path: &str) -> Uri {
    let mut encoded = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&byte) {
            encoded.push(byte as char);
        } else {
            write!(encoded, "%{byte:02X}").unwrap();
        }
    }
    Uri::from_str(&encoded).unwrap()
}

pub struct TestClient {
    child: Child,
    stdin: ChildStdin,
    /// Every message from the server, with the moment it arrived.
    messages: Receiver<(Instant, Message)>,
    /// Notifications that arrived while the client waited for something else.
    pending: VecDeque<(Instant, Notification)>,
    next_id: i32,
    last_sent: Instant,
    pub initialize_result: InitializeResult,
}

impl TestClient {
    /// A client that offers UTF-8 and UTF-16 positions, as Neovim does.
    pub fn spawn(init_options: Value) -> Self {
        Self::spawn_offering(init_options, &["utf-8", "utf-16"])
    }

    pub fn spawn_offering(init_options: Value, encodings: &[&'static str]) -> Self {
        Self::start(init_options, encodings, &[])
    }

    /// A client whose server runs with extra environment variables, which the server's own
    /// children inherit.
    pub fn spawn_env(init_options: Value, env: &[(&str, &str)]) -> Self {
        Self::start(init_options, &["utf-8", "utf-16"], env)
    }

    fn start(init_options: Value, encodings: &[&'static str], env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_swara"))
            .arg("lsp")
            .env("SWARA_TEST", "1")
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start swara lsp");
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(Some(message)) = Message::read(&mut stdout) {
                if sender.send((Instant::now(), message)).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            stdin,
            messages,
            pending: VecDeque::new(),
            next_id: 0,
            last_sent: Instant::now(),
            initialize_result: InitializeResult::default(),
        };
        let encodings = encodings.iter().map(|e| PositionEncodingKind::new(e)).collect();
        let params = InitializeParams {
            capabilities: ClientCapabilities {
                general: Some(GeneralClientCapabilities {
                    position_encodings: Some(encodings),
                    ..Default::default()
                }),
                ..Default::default()
            },
            initialization_options: Some(init_options),
            ..Default::default()
        };
        client.initialize_result = client.request::<Initialize>(params);
        client.notify::<Initialized>(InitializedParams {});
        client
    }

    pub fn open(&mut self, uri: &Uri, text: &str) {
        let item = TextDocumentItem::new(uri.clone(), "lilypond".into(), 1, text.into());
        self.notify::<DidOpenTextDocument>(DidOpenTextDocumentParams { text_document: item });
    }

    pub fn change(&mut self, uri: &Uri, version: i32, range: Range, text: &str) {
        self.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(range),
                range_length: None,
                text: text.into(),
            }],
        });
    }

    pub fn save(&mut self, uri: &Uri) {
        self.notify::<DidSaveTextDocument>(DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            text: None,
        });
    }

    pub fn notify<N: lsp_types::notification::Notification>(&mut self, params: N::Params) {
        self.send(Notification::new(N::METHOD.into(), params).into());
    }

    /// Sends a request and waits for its result. An error reply fails the test.
    pub fn request<R: Request>(&mut self, params: R::Params) -> R::Result {
        match self.request_raw(R::METHOD, params).response_result {
            Ok(result) => serde_json::from_value(result).unwrap(),
            Err(error) => panic!("{} failed: {error:?}", R::METHOD),
        }
    }

    pub fn request_raw(&mut self, method: &str, params: impl serde::Serialize) -> Response {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        self.send(lsp_server::Request::new(id.clone(), method.into(), params).into());
        self.response(&id, REPLY_WAIT)
            .unwrap_or_else(|| panic!("no reply to {method} within {REPLY_WAIT:?}"))
    }

    /// Waits for a diagnostics publish for `uri` that satisfies `pred`, skipping the ones that do
    /// not. The duration runs from the last message this client sent before the publish arrived.
    pub fn wait_diagnostics(
        &mut self,
        uri: &Uri,
        timeout: Duration,
        pred: impl Fn(&[Diagnostic]) -> bool,
    ) -> (Vec<Diagnostic>, Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let Some((arrived, notification)) = self.next_notification(deadline) else {
                panic!("no matching diagnostics for {} within {timeout:?}", uri.as_str());
            };
            if notification.method != PublishDiagnostics::METHOD {
                continue;
            }
            let params: PublishDiagnosticsParams =
                serde_json::from_value(notification.params).unwrap();
            if params.uri == *uri && pred(&params.diagnostics) {
                return (params.diagnostics, arrived.saturating_duration_since(self.last_sent));
            }
        }
    }

    /// Every diagnostics publish for `uri` that arrives within `duration`.
    pub fn publishes_for(&mut self, uri: &Uri, duration: Duration) -> Vec<Vec<Diagnostic>> {
        let deadline = Instant::now() + duration;
        let mut publishes = Vec::new();
        while let Some((_, notification)) = self.next_notification(deadline) {
            if notification.method != PublishDiagnostics::METHOD {
                continue;
            }
            let params: PublishDiagnosticsParams =
                serde_json::from_value(notification.params).unwrap();
            if params.uri == *uri {
                publishes.push(params.diagnostics);
            }
        }
        publishes
    }

    fn send(&mut self, message: Message) {
        message.write(&mut self.stdin).expect("write to swara");
        self.last_sent = Instant::now();
    }

    fn next_notification(&mut self, deadline: Instant) -> Option<(Instant, Notification)> {
        if let Some(pending) = self.pending.pop_front() {
            return Some(pending);
        }
        loop {
            match self.next_message(deadline)? {
                (arrived, Message::Notification(n)) => return Some((arrived, n)),
                (_, Message::Request(r)) => self.answer(r),
                (_, Message::Response(_)) => {}
            }
        }
    }

    fn response(&mut self, id: &RequestId, timeout: Duration) -> Option<Response> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.next_message(deadline)? {
                (_, Message::Response(r)) if r.id == *id => return Some(r),
                (_, Message::Response(_)) => {}
                (arrived, Message::Notification(n)) => self.pending.push_back((arrived, n)),
                (_, Message::Request(r)) => self.answer(r),
            }
        }
    }

    fn next_message(&mut self, deadline: Instant) -> Option<(Instant, Message)> {
        let wait = deadline.saturating_duration_since(Instant::now());
        self.messages.recv_timeout(wait).ok()
    }

    /// The server's own requests, such as for progress tokens, get an empty success.
    fn answer(&mut self, request: lsp_server::Request) {
        self.send(Response::new_ok(request.id, Value::Null).into());
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.next_id += 1;
            let id = RequestId::from(self.next_id);
            self.send(lsp_server::Request::new(id.clone(), Shutdown::METHOD.into(), ()).into());
            let _ = self.response(&id, REPLY_WAIT);
            self.send(Notification::new(Exit::METHOD.into(), ()).into());
            let deadline = Instant::now() + REPLY_WAIT;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = self.child.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
