//! The language server. One thread reads the protocol, keeps the open files and answers, waiting
//! on the client, on results from work it started, and on its next deadline, whichever comes first.

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender, at, never, select};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, LogMessage,
    Notification as _, PublishDiagnostics, ShowMessage,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, ExecuteCommand, Formatting, GotoDefinition, HoverRequest,
    Request as _,
};
use lsp_types::{
    CompletionOptions, Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, ExecuteCommandOptions,
    HoverProviderCapability, InitializeParams, InitializeResult, LogMessageParams, MessageType,
    OneOf, PositionEncodingKind, PublishDiagnosticsParams, SaveOptions, ServerCapabilities,
    ServerInfo, ShowMessageParams, TextDocumentIdentifier,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, Uri,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tree_sitter::{Node, Parser};

use crate::commanddata::{self, CommandData};
use crate::diagnostics::{self, Layers, syntax};
use crate::document::Document;
use crate::file_uri;
use crate::info;
use crate::line_index::{Encoding, LineIndex};
use crate::lilypond::messages::{self, LyFile};
use crate::lilypond::run::{self, RunResult, Running};
use crate::lilypond::to_diagnostic;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// The commands `workspace/executeCommand` accepts.
const COMMANDS: [&str; 2] = ["swara.build", "swara.preview"];

/// The client's `initializationOptions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Run the LilyPond check after each pause in typing.
    pub auto_check: bool,
    /// The `lilypond` to run, when not the one on `PATH`.
    pub lilypond: Option<String>,
    /// Folders `\include` looks in, besides the file's own. A relative one is taken from the
    /// file's folder.
    pub include_paths: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self { auto_check: true, lilypond: None, include_paths: Vec::new() }
    }
}

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

/// Shown once at start when there is no LilyPond to run. Everything else still works.
const NOT_FOUND: &str =
    "LilyPond not found: the LilyPond check, builds, preview and playback are off";

/// A LilyPond check in progress for one version of an open file.
struct Check {
    running: Running,
    out_dir: PathBuf,
    version: i32,
}

impl Check {
    fn stop(self) {
        self.running.stop();
        let _ = fs::remove_dir_all(&self.out_dir);
    }
}

struct Server<'c> {
    connection: &'c Connection,
    encoding: Encoding,
    config: Config,
    parser: Parser,
    documents: HashMap<Uri, Document>,
    layers: HashMap<Uri, Layers>,
    results_sender: Sender<RunResult>,
    results: Receiver<RunResult>,
    /// When each file's check is due, once typing has paused.
    pauses: HashMap<Uri, Instant>,
    checks: HashMap<Uri, Check>,
    /// Diagnostics for files a check's root file included, by root and then by path. Their
    /// positions are already in the client's encoding.
    included: HashMap<Uri, HashMap<PathBuf, Vec<Diagnostic>>>,
    /// The `lilypond` on `PATH`, looked up the first time a check needs it.
    found_lilypond: Option<Option<PathBuf>>,
    /// What the installed LilyPond defines, once it has loaded.
    data: Option<CommandData>,
    /// The command data from its loading thread, until it arrives.
    data_results: Receiver<std::result::Result<CommandData, String>>,
    /// `SWARA_TEST=1` turns on requests that only the tests use.
    test_mode: bool,
}

impl<'c> Server<'c> {
    fn new(connection: &'c Connection, encoding: Encoding, config: Config) -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lilypond::LANGUAGE.into())
            .expect("the grammar's ABI matches the tree-sitter crate");
        let (results_sender, results) = crossbeam_channel::unbounded();
        Self {
            connection,
            encoding,
            config,
            parser,
            documents: HashMap::new(),
            layers: HashMap::new(),
            results_sender,
            results,
            pauses: HashMap::new(),
            checks: HashMap::new(),
            included: HashMap::new(),
            found_lilypond: None,
            data: None,
            data_results: never(),
            test_mode: std::env::var_os("SWARA_TEST").is_some_and(|v| v == "1"),
        }
    }

    fn main_loop(&mut self) -> Result<()> {
        let connection = self.connection;
        let results = self.results.clone();
        self.load_command_data()?;
        loop {
            let deadline = self.next_deadline().map_or_else(never, at);
            let data_results = self.data_results.clone();
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
                recv(results) -> result => {
                    if let Ok(result) = result {
                        self.finished(result)?;
                    }
                }
                recv(data_results) -> loaded => {
                    // There is one answer at most, or none when the thread went away.
                    self.data_results = never();
                    match loaded {
                        Ok(Ok(data)) => self.data = Some(data),
                        Ok(Err(error)) => {
                            let message =
                                format!("LilyPond's command data could not be read: {error}");
                            self.log(MessageType::WARNING, message)?;
                        }
                        Err(_) => {}
                    }
                }
                recv(deadline) -> _ => {}
            }
            self.fire_due()?;
        }
    }

    /// Loads LilyPond's command data on a thread of its own, since a dump takes a moment, or says
    /// once that there is no LilyPond.
    fn load_command_data(&mut self) -> Result<()> {
        let Some(lilypond) = self.lilypond() else {
            return self.show(MessageType::WARNING, NOT_FOUND.into());
        };
        let (sender, receiver) = crossbeam_channel::bounded(1);
        thread::spawn(move || {
            let _ = sender.send(commanddata::load(&lilypond));
        });
        self.data_results = receiver;
        Ok(())
    }

    /// When the loop next has something to do without being asked: a check whose pause is over,
    /// or one that has run too long.
    fn next_deadline(&self) -> Option<Instant> {
        let checks = self.checks.values().map(|check| check.running.deadline);
        self.pauses.values().copied().chain(checks).min()
    }

    fn fire_due(&mut self) -> Result<()> {
        let now = Instant::now();
        let due: Vec<Uri> =
            self.pauses.iter().filter(|(_, at)| **at <= now).map(|(uri, _)| uri.clone()).collect();
        for uri in due {
            self.pauses.remove(&uri);
            self.start_check(&uri)?;
        }
        let late: Vec<Uri> = self
            .checks
            .iter()
            .filter(|(_, check)| check.running.deadline <= now)
            .map(|(uri, _)| uri.clone())
            .collect();
        for uri in late {
            self.time_out(&uri)?;
        }
        Ok(())
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
            "swara/debugCommandData" if self.test_mode => {
                Response::new_ok(id, self.data.as_ref().map(CommandData::summary))
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
                if self.config.auto_check && checkable(&uri) {
                    // A check of the old text would only report what is no longer there.
                    if let Some(check) = self.checks.remove(&uri) {
                        check.stop();
                    }
                    self.pauses.insert(uri, Instant::now() + run::PAUSE);
                }
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams = serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                self.pauses.remove(&uri);
                if let Some(check) = self.checks.remove(&uri) {
                    check.stop();
                }
                self.documents.remove(&uri);
                self.layers.remove(&uri);
                self.set_included(&uri, HashMap::new())?;
                match file_uri::to_path(&uri) {
                    // Another open file's check may still have something to say about this one.
                    Some(path) => self.publish_path(&path)?,
                    None => self.send_diagnostics(PublishDiagnosticsParams::new(uri, Vec::new(), None))?,
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The `lilypond` to run: the configured one, or the one on `PATH`. A configured path has to
    /// lead to a file, and a configured bare name is looked up on `PATH`.
    fn lilypond(&mut self) -> Option<PathBuf> {
        match self.config.lilypond.as_deref() {
            Some(path) if path.contains(std::path::is_separator) => {
                Some(PathBuf::from(path)).filter(|path| path.is_file())
            }
            name => {
                let name = name.unwrap_or("lilypond");
                self.found_lilypond.get_or_insert_with(|| info::find_on_path(name)).clone()
            }
        }
    }

    fn start_check(&mut self, uri: &Uri) -> Result<()> {
        // Without a LilyPond there is nothing to check with. The command data says why.
        let Some(lilypond) = self.lilypond() else { return Ok(()) };
        let (Some(doc), Some(path)) = (self.documents.get(uri), file_uri::to_path(uri)) else {
            return Ok(());
        };
        let dir = path.parent().unwrap_or(Path::new("/"));
        let includes: Vec<PathBuf> = self.config.include_paths.iter().map(|p| dir.join(p)).collect();
        let spec = run::check_spec(uri, &lilypond, &path, &doc.text, doc.version, &includes);
        let started = spec.and_then(|spec| {
            let (uri, version, out_dir) = (spec.uri.clone(), spec.version, spec.out_dir.clone());
            match run::spawn(spec, self.results_sender.clone()) {
                Ok(running) => Ok((uri, Check { running, out_dir, version })),
                Err(error) => {
                    let _ = fs::remove_dir_all(&out_dir);
                    Err(error)
                }
            }
        });
        match started {
            Ok((uri, check)) => {
                self.checks.insert(uri, check);
            }
            Err(error) => {
                let message = format!("LilyPond could not be started: {error}");
                self.layers.entry(uri.clone()).or_default().check = vec![notice(message)];
                self.publish(uri)?;
            }
        }
        Ok(())
    }

    /// Publishes a finished check, unless it is no longer the one wanted for its file.
    fn finished(&mut self, mut result: RunResult) -> Result<()> {
        let found = self.checks.iter().find(|(_, check)| check.running.id == result.id);
        let Some(uri) = found.map(|(uri, _)| uri.clone()) else { return Ok(()) };
        let Some(check) = self.checks.remove(&uri) else { return Ok(()) };
        let version = check.version;
        // Its standard error has closed, but whatever it started may still be running.
        result.status = check.running.stop();
        let _ = fs::remove_dir_all(&check.out_dir);
        let (Some(doc), Some(path)) = (self.documents.get(&uri), file_uri::to_path(&uri)) else {
            return Ok(());
        };
        if doc.version != version {
            return Ok(());
        }
        let dir = path.parent().unwrap_or(Path::new("/"));
        let mut own = Vec::new();
        let mut others: HashMap<PathBuf, (String, LineIndex, Vec<Diagnostic>)> = HashMap::new();
        for message in messages::parse_stderr(&result.stderr) {
            match &message.file {
                Some(LyFile::Path(named)) if dir.join(named) != path => {
                    // LilyPond read the included file from disk, so its positions are counted
                    // in the text on disk.
                    let (text, index, found) = others.entry(dir.join(named)).or_insert_with_key(|p| {
                        let text = fs::read_to_string(p).unwrap_or_default();
                        let index = LineIndex::new(&text);
                        (text, index, Vec::new())
                    });
                    found.push(to_diagnostic(&message, text, index));
                }
                // `-` is the file itself, and so is a message that names no file.
                _ => own.push(to_diagnostic(&message, &doc.text, &doc.index)),
            }
        }
        let encoding = self.encoding;
        let others = others
            .into_iter()
            .map(|(p, (text, index, found))| (p, diagnostics::convert(found, &text, &index, encoding)))
            .collect();
        if let Some(layers) = self.layers.get_mut(&uri) {
            layers.check = own;
        }
        self.set_included(&uri, others)?;
        self.publish(&uri)
    }

    fn time_out(&mut self, uri: &Uri) -> Result<()> {
        let Some(check) = self.checks.remove(uri) else { return Ok(()) };
        check.stop();
        let seconds = run::check_timeout().as_secs_f64();
        let message = format!("LilyPond check took longer than {seconds} s and was stopped");
        if let Some(layers) = self.layers.get_mut(uri) {
            layers.check = vec![notice(message)];
        }
        self.set_included(uri, HashMap::new())?;
        self.publish(uri)
    }

    /// Replaces what `root`'s check said about the files it included, and publishes every file
    /// whose diagnostics that changes.
    fn set_included(&mut self, root: &Uri, files: HashMap<PathBuf, Vec<Diagnostic>>) -> Result<()> {
        let old = self.included.remove(root).unwrap_or_default();
        let mut touched: Vec<PathBuf> = old.into_keys().chain(files.keys().cloned()).collect();
        touched.sort();
        touched.dedup();
        if !files.is_empty() {
            self.included.insert(root.clone(), files);
        }
        for path in touched {
            self.publish_path(&path)?;
        }
        Ok(())
    }

    fn included_for(&self, path: &Path) -> Vec<Diagnostic> {
        self.included.values().filter_map(|files| files.get(path)).flatten().cloned().collect()
    }

    /// Rebuilds the syntax errors of one file and publishes all its diagnostics.
    fn refresh(&mut self, uri: &Uri) -> Result<()> {
        let (Some(doc), Some(layers)) = (self.documents.get(uri), self.layers.get_mut(uri)) else {
            return Ok(());
        };
        layers.instant = syntax::syntax_errors(doc, Encoding::Utf8);
        self.publish(uri)
    }

    /// Publishes every diagnostic of an open file: its layers, and what checks of other files
    /// said about it.
    fn publish(&self, uri: &Uri) -> Result<()> {
        let (Some(doc), Some(layers)) = (self.documents.get(uri), self.layers.get(uri)) else {
            return Ok(());
        };
        let mut merged = diagnostics::in_encoding(layers.merged(), doc, self.encoding);
        if let Some(path) = file_uri::to_path(uri) {
            merged.extend(self.included_for(&path));
        }
        self.send_diagnostics(PublishDiagnosticsParams::new(doc.uri.clone(), merged, Some(doc.version)))
    }

    /// Publishes the diagnostics of a file by its path, whether it is open or not.
    fn publish_path(&self, path: &Path) -> Result<()> {
        let open = self.documents.keys().find(|uri| file_uri::to_path(uri).as_deref() == Some(path));
        if let Some(uri) = open {
            return self.publish(uri);
        }
        let Some(uri) = file_uri::from_path(path) else { return Ok(()) };
        self.send_diagnostics(PublishDiagnosticsParams::new(uri, self.included_for(path), None))
    }

    fn send_diagnostics(&self, params: PublishDiagnosticsParams) -> Result<()> {
        let notification = Notification::new(PublishDiagnostics::METHOD.into(), params);
        self.connection.sender.send(notification.into())?;
        Ok(())
    }

    /// A message the editor puts in front of the user.
    fn show(&self, typ: MessageType, message: String) -> Result<()> {
        let params = ShowMessageParams { typ, message };
        self.connection.sender.send(Notification::new(ShowMessage::METHOD.into(), params).into())?;
        Ok(())
    }

    /// A message for the editor's log, which the user reads only when looking for it.
    fn log(&self, typ: MessageType, message: String) -> Result<()> {
        let params = LogMessageParams { typ, message };
        self.connection.sender.send(Notification::new(LogMessage::METHOD.into(), params).into())?;
        Ok(())
    }
}

impl Drop for Server<'_> {
    /// Leaves no LilyPond running and nothing in the temporary folder.
    fn drop(&mut self) {
        for (_, check) in self.checks.drain() {
            check.stop();
        }
        // Before the work folder goes, so that no dump starts in it, or is left running in it.
        commanddata::stop_dump();
        run::remove_work_dir();
    }
}

/// Only a `.ly` file on disk is checked. An `.ily` file is checked through the files that
/// include it.
fn checkable(uri: &Uri) -> bool {
    file_uri::to_path(uri).is_some_and(|path| path.extension().is_some_and(|ext| ext == "ly"))
}

/// A note from the check about the check itself, at the top of the file.
fn notice(message: String) -> Diagnostic {
    Diagnostic {
        severity: Some(DiagnosticSeverity::INFORMATION),
        source: Some("lilypond".into()),
        message,
        ..Default::default()
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
