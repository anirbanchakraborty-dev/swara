//! The language server. One thread reads the protocol, keeps the open files and answers, waiting
//! on the client, on results from work it started, and on its next deadline, whichever comes first.

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender, at, never, select};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit,
    Notification as _, PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, ExecuteCommand, Formatting, GotoDefinition, HoverRequest,
    Request as _,
};
use lsp_types::{
    CompletionOptions, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, ExecuteCommandOptions, HoverProviderCapability, InitializeParams,
    InitializeResult, OneOf, PositionEncodingKind, PublishDiagnosticsParams, SaveOptions,
    ServerCapabilities, ServerInfo, TextDocumentIdentifier, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Uri,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tree_sitter::{Node, Parser};

use crate::diagnostics::{self, Layers, syntax};
use crate::document::Document;
use crate::line_index::Encoding;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// The commands `workspace/executeCommand` accepts.
const COMMANDS: [&str; 2] = ["swara.build", "swara.preview"];

/// The client's `initializationOptions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
// The check reads these, in a later task.
#[allow(dead_code)]
pub struct Config {
    /// Run the LilyPond check after each pause in typing.
    pub auto_check: bool,
    /// The `lilypond` to run, when not the one on `PATH`.
    pub lilypond: Option<String>,
    /// Folders `\include` looks in, besides the file's own.
    pub include_paths: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self { auto_check: true, lilypond: None, include_paths: Vec::new() }
    }
}

/// What finished work sends back to the loop. Nothing runs in the background yet.
enum Outcome {}

pub fn run() -> ExitCode {
    let (connection, io_threads) = Connection::stdio();
    let served = serve(&connection);
    drop(connection);
    let joined = io_threads.join();
    match served.and(joined.map_err(Into::into)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("swara: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve(connection: &Connection) -> Result<()> {
    let (id, params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(params)?;
    let encoding = negotiate(&params);
    let config = match params.initialization_options {
        Some(options) => serde_json::from_value(options).unwrap_or_else(|error| {
            eprintln!("swara: ignoring initializationOptions: {error}");
            Config::default()
        }),
        None => Config::default(),
    };
    let result = InitializeResult {
        capabilities: capabilities(encoding),
        server_info: Some(ServerInfo {
            name: "swara".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
    };
    connection.initialize_finish(id, serde_json::to_value(result)?)?;
    Server::new(connection, encoding, config).main_loop()
}

/// UTF-8 when the client offers it, since that is how the text is stored, and otherwise UTF-16,
/// which every client must accept.
fn negotiate(params: &InitializeParams) -> Encoding {
    let general = params.capabilities.general.as_ref();
    let offered = general.and_then(|g| g.position_encodings.as_deref()).unwrap_or_default();
    if offered.contains(&PositionEncodingKind::UTF8) { Encoding::Utf8 } else { Encoding::Utf16 }
}

fn capabilities(encoding: Encoding) -> ServerCapabilities {
    let position_encoding = match encoding {
        Encoding::Utf8 => PositionEncodingKind::UTF8,
        Encoding::Utf16 => PositionEncodingKind::UTF16,
    };
    ServerCapabilities {
        position_encoding: Some(position_encoding),
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::INCREMENTAL),
            save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                include_text: Some(false),
            })),
            ..Default::default()
        })),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["\\".into(), ".".into()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: COMMANDS.map(String::from).to_vec(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

struct Server<'c> {
    connection: &'c Connection,
    encoding: Encoding,
    #[allow(dead_code)]
    config: Config,
    parser: Parser,
    documents: HashMap<Uri, Document>,
    layers: HashMap<Uri, Layers>,
    /// Kept so the results channel stays open while nothing is running.
    _results_sender: Sender<Outcome>,
    results: Receiver<Outcome>,
    /// `SWARA_TEST=1` turns on requests that only the tests use.
    test_mode: bool,
}

impl<'c> Server<'c> {
    fn new(connection: &'c Connection, encoding: Encoding, config: Config) -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lilypond::LANGUAGE.into())
            .expect("the grammar's ABI matches the tree-sitter crate");
        let (sender, results) = crossbeam_channel::unbounded();
        Self {
            connection,
            encoding,
            config,
            parser,
            documents: HashMap::new(),
            layers: HashMap::new(),
            _results_sender: sender,
            results,
            test_mode: std::env::var_os("SWARA_TEST").is_some_and(|v| v == "1"),
        }
    }

    fn main_loop(&mut self) -> Result<()> {
        let connection = self.connection;
        let results = self.results.clone();
        loop {
            let deadline = self.next_deadline().map_or_else(never, at);
            select! {
                recv(connection.receiver) -> message => {
                    // The client went away without saying goodbye.
                    let Ok(message) = message else { return Ok(()) };
                    match message {
                        Message::Request(request) => {
                            if connection.handle_shutdown(&request)? {
                                return Ok(());
                            }
                            self.request(request)?;
                        }
                        Message::Notification(notification) if notification.method == Exit::METHOD => {
                            return Ok(());
                        }
                        Message::Notification(notification) => self.notification(notification)?,
                        Message::Response(_) => {}
                    }
                }
                recv(results) -> outcome => {
                    if let Ok(outcome) = outcome {
                        match outcome {}
                    }
                }
                recv(deadline) -> _ => {}
            }
        }
    }

    /// When the loop next has something to do without being asked. Nothing is scheduled yet.
    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn request(&mut self, request: Request) -> Result<()> {
        let id = request.id.clone();
        let response = match request.method.as_str() {
            "swara/debugTree" if self.test_mode => {
                match serde_json::from_value::<TextDocumentIdentifier>(request.params) {
                    Ok(params) => match self.documents.get(&params.uri) {
                        Some(doc) => Response::new_ok(id, debug_tree(doc)),
                        None => invalid(id, "no such open file"),
                    },
                    Err(error) => invalid(id, &error.to_string()),
                }
            }
            Completion::METHOD
            | HoverRequest::METHOD
            | GotoDefinition::METHOD
            | DocumentSymbolRequest::METHOD
            | Formatting::METHOD
            | ExecuteCommand::METHOD => Response::new_ok(id, Value::Null),
            method => Response::new_err(
                id,
                ErrorCode::MethodNotFound as i32,
                format!("swara does not handle {method}"),
            ),
        };
        self.connection.sender.send(response.into())?;
        Ok(())
    }

    fn notification(&mut self, notification: Notification) -> Result<()> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams = serde_json::from_value(notification.params)?;
                let item = params.text_document;
                let doc = Document::new(item.uri.clone(), item.version, item.text, &mut self.parser);
                self.documents.insert(item.uri.clone(), doc);
                self.layers.insert(item.uri.clone(), Layers::default());
                self.refresh(&item.uri)?;
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                let Some(doc) = self.documents.get_mut(&uri) else { return Ok(()) };
                let layers = self.layers.entry(uri.clone()).or_default();
                doc.version = params.text_document.version;
                for change in &params.content_changes {
                    match doc.apply(change, self.encoding, &mut self.parser) {
                        Some(edit) => layers.shift(&edit),
                        // A whole new text leaves nothing for old positions to point at.
                        None => {
                            layers.check.clear();
                            layers.build.clear();
                        }
                    }
                }
                self.refresh(&uri)?;
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams = serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                self.documents.remove(&uri);
                self.layers.remove(&uri);
                self.send_diagnostics(PublishDiagnosticsParams::new(uri, Vec::new(), None))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Rebuilds the syntax errors of one file and publishes all its diagnostics.
    fn refresh(&mut self, uri: &Uri) -> Result<()> {
        let (Some(doc), Some(layers)) = (self.documents.get(uri), self.layers.get_mut(uri)) else {
            return Ok(());
        };
        layers.instant = syntax::syntax_errors(doc, Encoding::Utf8);
        let merged = diagnostics::in_encoding(layers.merged(), doc, self.encoding);
        self.send_diagnostics(PublishDiagnosticsParams::new(doc.uri.clone(), merged, Some(doc.version)))
    }

    fn send_diagnostics(&self, params: PublishDiagnosticsParams) -> Result<()> {
        let notification = Notification::new(PublishDiagnostics::METHOD.into(), params);
        self.connection.sender.send(notification.into())?;
        Ok(())
    }
}

fn invalid(id: lsp_server::RequestId, message: &str) -> Response {
    Response::new_err(id, ErrorCode::InvalidParams as i32, message.into())
}

/// The file's text and every node of its tree, for the tests to compare with a fresh parse.
fn debug_tree(doc: &Document) -> Value {
    fn dump(node: Node, out: &mut String) {
        let missing = if node.is_missing() { " MISSING" } else { "" };
        let _ = write!(out, "({}{missing} {}-{}", node.kind(), node.start_byte(), node.end_byte());
        for child in node.children(&mut node.walk()) {
            out.push(' ');
            dump(child, out);
        }
        out.push(')');
    }
    let mut tree = String::new();
    dump(doc.tree.root_node(), &mut tree);
    json!({ "text": doc.text, "tree": tree })
}
