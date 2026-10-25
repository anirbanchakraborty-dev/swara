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
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Exit,
    LogMessage, Notification as _, PublishDiagnostics, ShowMessage,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, ExecuteCommand, Formatting, GotoDefinition, HoverRequest,
    Request as _,
};
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, ExecuteCommandOptions, ExecuteCommandParams, GotoDefinitionParams,
    GotoDefinitionResponse, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, LogMessageParams, MessageType, OneOf, PositionEncodingKind,
    PublishDiagnosticsParams, Range, SaveOptions, ServerCapabilities, ServerInfo,
    ShowMessageParams, TextDocumentIdentifier, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Uri,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tree_sitter::{InputEdit, Node, Parser};

use crate::build::{self, BuildFinished, Outputs};
use crate::commanddata::{self, CommandData};
use crate::completion;
use crate::definition;
use crate::diagnostics::{self, Layers, includes, names, syntax};
use crate::document::Document;
use crate::file_uri;
use crate::format::{self, FormatOutcome};
use crate::hover;
use crate::index::Index;
use crate::info;
use crate::lilypond::messages::{self, LyFile};
use crate::lilypond::run::{self, RunKind, RunResult, Running};
use crate::lilypond::to_diagnostic;
use crate::line_index::{Encoding, LineIndex};
use crate::symbols;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// The commands `workspace/executeCommand` accepts.
const COMMANDS: [&str; 2] = ["swara.build", "swara.preview"];

/// The preview's resolution in dots per inch when the command names none, and the most it allows.
const DEFAULT_DPI: u32 = 150;
const MAX_DPI: u64 = 2400;

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
        Self {
            auto_check: true,
            lilypond: None,
            include_paths: Vec::new(),
        }
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
    let offered = general
        .and_then(|g| g.position_encodings.as_deref())
        .unwrap_or_default();
    if offered.contains(&PositionEncodingKind::UTF8) {
        Encoding::Utf8
    } else {
        Encoding::Utf16
    }
}

fn capabilities(encoding: Encoding) -> ServerCapabilities {
    let position_encoding = match encoding {
        Encoding::Utf8 => PositionEncodingKind::UTF8,
        Encoding::Utf16 => PositionEncodingKind::UTF16,
    };
    ServerCapabilities {
        position_encoding: Some(position_encoding),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
                ..Default::default()
            },
        )),
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

/// A build in progress for an open file, with what it needs to place its messages once it ends.
struct Build {
    running: Running,
    out_dir: PathBuf,
    path: PathBuf,
    started: Instant,
    /// The text LilyPond read, whose lines and columns its messages count in.
    text: String,
    index: LineIndex,
    /// The edits made since it started, which move its messages, or `None` once the whole text
    /// has been replaced and nothing it says can be placed.
    edits: Option<Vec<InputEdit>>,
}

impl Build {
    fn stop(self) {
        self.running.stop();
        let _ = fs::remove_dir_all(&self.out_dir);
    }
}

/// A preview in progress, answered with its image when it ends.
struct Preview {
    running: Running,
    out_dir: PathBuf,
    png: PathBuf,
    request: RequestId,
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
    /// Diagnostics for files a run's root file included, by root and kind of run, then by path.
    /// Their positions are already in the client's encoding.
    included: HashMap<(Uri, RunKind), HashMap<PathBuf, Vec<Diagnostic>>>,
    builds: HashMap<Uri, Build>,
    previews: HashMap<Uri, Preview>,
    /// The folder of the image each file's last preview answered with, kept until the next one.
    shown: HashMap<Uri, PathBuf>,
    /// The `lilypond` on `PATH`, looked up the first time a check needs it.
    found_lilypond: Option<Option<PathBuf>>,
    /// What the installed LilyPond defines, once it has loaded.
    data: Option<CommandData>,
    /// What the open files and the files they include define, for the name checks.
    index: Index,
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
        let index = Index::new(config.include_paths.clone());
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
            builds: HashMap::new(),
            previews: HashMap::new(),
            shown: HashMap::new(),
            found_lilypond: None,
            data: None,
            index,
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
                        Ok(Ok(data)) => {
                            self.index.set_datadir(data.datadir.clone());
                            self.data = Some(data);
                            // Every name was unknown until now, so the checks run again.
                            self.refresh_open(None)?;
                        }
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
    /// or a run that has gone on too long.
    fn next_deadline(&self) -> Option<Instant> {
        let checks = self.checks.values().map(|check| check.running.deadline);
        let builds = self.builds.values().map(|build| build.running.deadline);
        let previews = self
            .previews
            .values()
            .map(|preview| preview.running.deadline);
        self.pauses
            .values()
            .copied()
            .chain(checks)
            .chain(builds)
            .chain(previews)
            .min()
    }

    fn fire_due(&mut self) -> Result<()> {
        let now = Instant::now();
        let due: Vec<Uri> = self
            .pauses
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(uri, _)| uri.clone())
            .collect();
        for uri in due {
            self.pauses.remove(&uri);
            self.start_check(&uri)?;
        }
        for uri in late(&self.checks, now, |check| &check.running) {
            self.time_out(&uri)?;
        }
        for uri in late(&self.builds, now, |build| &build.running) {
            self.time_out_build(&uri)?;
        }
        for uri in late(&self.previews, now, |preview| &preview.running) {
            self.time_out_preview(&uri)?;
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
            Completion::METHOD => {
                match serde_json::from_value::<CompletionParams>(request.params) {
                    Ok(params) => {
                        let at = params.text_document_position;
                        let items = self.documents.get(&at.text_document.uri).map(|doc| {
                            let pos =
                                doc.index
                                    .position_to_byte(&doc.text, at.position, self.encoding);
                            let data = self.data.as_ref();
                            completion::complete(doc, pos, data, &self.index, self.encoding)
                        });
                        Response::new_ok(id, items.map(CompletionResponse::Array))
                    }
                    Err(error) => invalid(id, &error.to_string()),
                }
            }
            HoverRequest::METHOD => match serde_json::from_value::<HoverParams>(request.params) {
                Ok(params) => {
                    let at = params.text_document_position_params;
                    let shown = self.documents.get(&at.text_document.uri).and_then(|doc| {
                        let pos = doc
                            .index
                            .position_to_byte(&doc.text, at.position, self.encoding);
                        hover::hover(doc, pos, self.data.as_ref())
                    });
                    Response::new_ok(id, shown)
                }
                Err(error) => invalid(id, &error.to_string()),
            },
            GotoDefinition::METHOD => {
                match serde_json::from_value::<GotoDefinitionParams>(request.params) {
                    Ok(params) => {
                        let at = params.text_document_position_params;
                        let found = self.documents.get(&at.text_document.uri).and_then(|doc| {
                            let pos =
                                doc.index
                                    .position_to_byte(&doc.text, at.position, self.encoding);
                            definition::definition(doc, pos, &self.index)
                        });
                        let found = found.map(|mut location| {
                            location.range = self.encoded(&location.uri, location.range);
                            GotoDefinitionResponse::Scalar(location)
                        });
                        Response::new_ok(id, found)
                    }
                    Err(error) => invalid(id, &error.to_string()),
                }
            }
            DocumentSymbolRequest::METHOD => {
                match serde_json::from_value::<DocumentSymbolParams>(request.params) {
                    Ok(params) => {
                        let outline = self.documents.get(&params.text_document.uri).map(|doc| {
                            DocumentSymbolResponse::Nested(symbols::document_symbols(
                                doc,
                                self.encoding,
                            ))
                        });
                        Response::new_ok(id, outline)
                    }
                    Err(error) => invalid(id, &error.to_string()),
                }
            }
            Formatting::METHOD => {
                match serde_json::from_value::<DocumentFormattingParams>(request.params) {
                    Ok(params) => {
                        let indent = params.options.tab_size as usize;
                        let edits = self
                            .documents
                            .get(&params.text_document.uri)
                            .and_then(|doc| {
                                match format::format(&doc.text, indent, &mut self.parser) {
                                    FormatOutcome::Unchanged => Some(Vec::new()),
                                    FormatOutcome::Changed(new) => Some(format::line_edits(
                                        &doc.text,
                                        &doc.index,
                                        &new,
                                        self.encoding,
                                    )),
                                    FormatOutcome::Refused(_) => None,
                                }
                            });
                        Response::new_ok(id, edits)
                    }
                    Err(error) => invalid(id, &error.to_string()),
                }
            }
            ExecuteCommand::METHOD => return self.execute(id, request.params),
            method => Response::new_err(
                id,
                ErrorCode::MethodNotFound as i32,
                format!("swara does not handle {method}"),
            ),
        };
        self.reply(response)
    }

    fn reply(&self, response: Response) -> Result<()> {
        self.connection.sender.send(response.into())?;
        Ok(())
    }

    /// Runs `swara.build` or `swara.preview` on an open file. A build is answered at once and
    /// reports through `swara/buildFinished`. A preview is answered with its image once it ends.
    fn execute(&mut self, id: RequestId, params: Value) -> Result<()> {
        let params: ExecuteCommandParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(error) => return self.reply(invalid(id, &error.to_string())),
        };
        let command = params.command.as_str();
        if !COMMANDS.contains(&command) {
            return self.reply(invalid(id, &format!("swara has no command {command}")));
        }
        let uri = params
            .arguments
            .first()
            .cloned()
            .and_then(|uri| serde_json::from_value(uri).ok());
        let open = |uri: &Uri| self.documents.contains_key(uri) && file_uri::to_path(uri).is_some();
        let Some(uri) = uri.filter(open) else {
            return self.reply(invalid(id, "the first argument must name an open file"));
        };
        let Some(lilypond) = self.lilypond() else {
            return self.reply(failed(id, NOT_FOUND.into()));
        };
        if command == "swara.build" {
            self.reply(Response::new_ok(id, Value::Null))?;
            return self.start_build(&uri, &lilypond, true);
        }
        let dpi = match params.arguments.get(1) {
            None => DEFAULT_DPI,
            Some(dpi) => match dpi.as_u64().filter(|dpi| (1..=MAX_DPI).contains(dpi)) {
                Some(dpi) => dpi as u32,
                None => return self.reply(invalid(id, "the resolution must be 1 to 2400 dpi")),
            },
        };
        self.start_preview(id, &uri, &lilypond, dpi)
    }

    fn notification(&mut self, notification: Notification) -> Result<()> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let item = params.text_document;
                let doc =
                    Document::new(item.uri.clone(), item.version, item.text, &mut self.parser);
                let changed = index_update(&mut self.index, &doc);
                self.documents.insert(item.uri.clone(), doc);
                self.layers.insert(item.uri.clone(), Layers::default());
                self.refresh(&item.uri)?;
                if changed {
                    self.refresh_open(Some(&item.uri))?;
                }
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                let Some(doc) = self.documents.get_mut(&uri) else {
                    return Ok(());
                };
                let layers = self.layers.entry(uri.clone()).or_default();
                // A running build read the old text, so its messages move with these edits too.
                let mut build = self.builds.get_mut(&uri);
                doc.version = params.text_document.version;
                for change in &params.content_changes {
                    match doc.apply(change, self.encoding, &mut self.parser) {
                        Some(edit) => {
                            layers.shift(&edit);
                            if let Some(edits) = build.as_mut().and_then(|b| b.edits.as_mut()) {
                                edits.push(edit);
                            }
                        }
                        // A whole new text leaves nothing for old positions to point at.
                        None => {
                            layers.check.clear();
                            layers.build.clear();
                            if let Some(build) = build.as_mut() {
                                build.edits = None;
                            }
                        }
                    }
                }
                let changed = index_update(&mut self.index, doc);
                self.refresh(&uri)?;
                if changed {
                    // A name this file defines or a file it includes may be what another open
                    // file was missing.
                    self.refresh_open(Some(&uri))?;
                }
                if self.config.auto_check && checkable(&uri) {
                    // A check of the old text would only report what is no longer there.
                    if let Some(check) = self.checks.remove(&uri) {
                        check.stop();
                    }
                    self.pauses.insert(uri, Instant::now() + run::PAUSE);
                }
            }
            DidSaveTextDocument::METHOD => {
                let params: DidSaveTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                // Without a LilyPond there is nothing to build with, as the start said.
                if checkable(&uri)
                    && self.documents.contains_key(&uri)
                    && let Some(lilypond) = self.lilypond()
                {
                    self.start_build(&uri, &lilypond, false)?;
                }
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = params.text_document.uri;
                self.pauses.remove(&uri);
                if let Some(check) = self.checks.remove(&uri) {
                    check.stop();
                }
                self.documents.remove(&uri);
                self.layers.remove(&uri);
                if let Some(path) = file_uri::to_path(&uri) {
                    // The file is read from disk again, which may not define what the editor did.
                    self.index.close(&path);
                    self.refresh_open(None)?;
                }
                // A build or a preview still running finishes, and reports, as the user asked.
                self.set_included(&uri, RunKind::Check, HashMap::new())?;
                self.set_included(&uri, RunKind::Build, HashMap::new())?;
                match file_uri::to_path(&uri) {
                    // Another open file's check may still have something to say about this one.
                    Some(path) => self.publish_path(&path)?,
                    None => {
                        self.send_diagnostics(PublishDiagnosticsParams::new(uri, Vec::new(), None))?
                    }
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
                self.found_lilypond
                    .get_or_insert_with(|| info::find_on_path(name))
                    .clone()
            }
        }
    }

    fn start_check(&mut self, uri: &Uri) -> Result<()> {
        // Without a LilyPond there is nothing to check with. The command data says why.
        let Some(lilypond) = self.lilypond() else {
            return Ok(());
        };
        let (Some(doc), Some(path)) = (self.documents.get(uri), file_uri::to_path(uri)) else {
            return Ok(());
        };
        let includes = self.include_paths(&path);
        let spec = run::check_spec(uri, &lilypond, &path, &doc.text, doc.version, &includes);
        let started = spec.and_then(|spec| {
            let (uri, version, out_dir) = (spec.uri.clone(), spec.version, spec.out_dir.clone());
            match run::spawn(spec, self.results_sender.clone()) {
                Ok(running) => Ok((
                    uri,
                    Check {
                        running,
                        out_dir,
                        version,
                    },
                )),
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
                self.layers
                    .entry(uri.clone())
                    .or_default()
                    .set_check(vec![notice(message)]);
                self.publish(uri)?;
            }
        }
        Ok(())
    }

    /// The configured include folders, a relative one taken from the folder of `path`.
    fn include_paths(&self, path: &Path) -> Vec<PathBuf> {
        let dir = path.parent().unwrap_or(Path::new("/"));
        self.config
            .include_paths
            .iter()
            .map(|p| dir.join(p))
            .collect()
    }

    /// Builds an open file: the file on disk after a save, or with `unsaved` its open text. A
    /// build already running for the file is stopped first, and reports nothing.
    fn start_build(&mut self, uri: &Uri, lilypond: &Path, unsaved: bool) -> Result<()> {
        if let Some(old) = self.builds.remove(uri) {
            old.stop();
        }
        let (Some(doc), Some(path)) = (self.documents.get(uri), file_uri::to_path(uri)) else {
            return Ok(());
        };
        let includes = self.include_paths(&path);
        let stdin = unsaved.then(|| doc.text.clone());
        let spec = build::build_spec(uri, lilypond, &path, stdin, doc.version, &includes);
        let out_dir = spec.out_dir.clone();
        // On a save the file on disk is the open text, so the open text places the messages.
        let (text, index) = (doc.text.clone(), doc.index.clone());
        let started = Instant::now();
        match run::spawn(spec, self.results_sender.clone()) {
            Ok(running) => {
                let edits = Some(Vec::new());
                let build = Build {
                    running,
                    out_dir,
                    path,
                    started,
                    text,
                    index,
                    edits,
                };
                self.builds.insert(uri.clone(), build);
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&out_dir);
                let message = format!("LilyPond could not be started: {error}");
                self.layers
                    .entry(uri.clone())
                    .or_default()
                    .set_build(vec![notice(message)]);
                self.set_included(uri, RunKind::Build, HashMap::new())?;
                self.publish(uri)?;
                self.send_finished(uri, Outputs::default(), false, started)
            }
        }
    }

    /// Renders the open text of a file to an image. A preview already running for the file is
    /// stopped, and its request answered with an error.
    fn start_preview(&mut self, id: RequestId, uri: &Uri, lilypond: &Path, dpi: u32) -> Result<()> {
        if let Some(old) = self.previews.remove(uri) {
            old.running.stop();
            let _ = fs::remove_dir_all(&old.out_dir);
            self.reply(failed(
                old.request,
                "a newer preview replaced this one".into(),
            ))?;
        }
        let (Some(doc), Some(path)) = (self.documents.get(uri), file_uri::to_path(uri)) else {
            return self.reply(invalid(id, "the first argument must name an open file"));
        };
        let includes = self.include_paths(&path);
        let spec =
            build::preview_spec(uri, lilypond, &path, &doc.text, doc.version, &includes, dpi);
        let spec = match spec {
            Ok(spec) => spec,
            Err(error) => {
                return self.reply(failed(id, format!("no folder for the preview: {error}")));
            }
        };
        let out_dir = spec.out_dir.clone();
        let png = build::preview_png(&out_dir, &path);
        match run::spawn(spec, self.results_sender.clone()) {
            Ok(running) => {
                self.previews.insert(
                    uri.clone(),
                    Preview {
                        running,
                        out_dir,
                        png,
                        request: id,
                    },
                );
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&out_dir);
                self.reply(failed(
                    id,
                    format!("LilyPond could not be started: {error}"),
                ))
            }
        }
    }

    /// Publishes a finished check, unless it is no longer the one wanted for its file.
    /// Takes the result of whichever run it belongs to. A run stopped since is no longer kept,
    /// and what it printed is dropped.
    fn finished(&mut self, result: RunResult) -> Result<()> {
        let id = result.id;
        if let Some(uri) = owner(&self.checks, id, |check| &check.running) {
            return self.check_finished(uri, result);
        }
        if let Some(uri) = owner(&self.builds, id, |build| &build.running) {
            return self.build_finished(uri, result);
        }
        if let Some(uri) = owner(&self.previews, id, |preview| &preview.running) {
            return self.preview_finished(uri);
        }
        Ok(())
    }

    fn check_finished(&mut self, uri: Uri, result: RunResult) -> Result<()> {
        let Some(check) = self.checks.remove(&uri) else {
            return Ok(());
        };
        let version = check.version;
        // Its standard error has closed, but whatever it started may still be running.
        check.stop();
        let (Some(doc), Some(path)) = (self.documents.get(&uri), file_uri::to_path(&uri)) else {
            return Ok(());
        };
        if doc.version != version {
            return Ok(());
        }
        let (own, others) = place(&result.stderr, &path, &doc.text, &doc.index, self.encoding);
        if let Some(layers) = self.layers.get_mut(&uri) {
            layers.set_check(own);
        }
        self.set_included(&uri, RunKind::Check, others)?;
        self.publish(&uri)
    }

    /// Moves what a build wrote beside its source, shows what it said, and tells the plugin. Its
    /// messages are placed in the text it read, then moved through every edit made since.
    fn build_finished(&mut self, uri: Uri, result: RunResult) -> Result<()> {
        let Some(build) = self.builds.remove(&uri) else {
            return Ok(());
        };
        let Build {
            running,
            out_dir,
            path,
            started,
            text,
            index,
            edits,
            ..
        } = build;
        let status = running.stop();
        let dir = path.parent().unwrap_or(Path::new("/"));
        let outputs = build::move_outputs(&out_dir, dir, &path);
        if self.documents.contains_key(&uri) {
            let (mut own, others) = place(&result.stderr, &path, &text, &index, self.encoding);
            match edits {
                Some(edits) => edits
                    .iter()
                    .for_each(|edit| diagnostics::shift_all(&mut own, edit)),
                None => own.clear(),
            }
            if let Some(layers) = self.layers.get_mut(&uri) {
                layers.set_build(own);
            }
            self.set_included(&uri, RunKind::Build, others)?;
            self.publish(&uri)?;
        }
        let success = status.is_some_and(|status| status.success());
        self.send_finished(&uri, outputs, success, started)
    }

    /// Answers a preview with its image, and lets go of the image it answered with before.
    fn preview_finished(&mut self, uri: Uri) -> Result<()> {
        let Some(preview) = self.previews.remove(&uri) else {
            return Ok(());
        };
        preview.running.stop();
        if !preview.png.is_file() {
            let _ = fs::remove_dir_all(&preview.out_dir);
            let message = "LilyPond made no image; build the file to see its errors";
            return self.reply(failed(preview.request, message.into()));
        }
        if let Some(old) = self.shown.insert(uri, preview.out_dir) {
            let _ = fs::remove_dir_all(old);
        }
        let png = preview.png.to_string_lossy();
        self.reply(Response::new_ok(preview.request, json!({ "png": png })))
    }

    fn send_finished(
        &self,
        uri: &Uri,
        outputs: Outputs,
        success: bool,
        started: Instant,
    ) -> Result<()> {
        let text = |path: Option<PathBuf>| path.map(|path| path.to_string_lossy().into_owned());
        let params = BuildFinished {
            uri: uri.clone(),
            pdf: text(outputs.pdf),
            midi: text(outputs.midi),
            success,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        };
        self.connection
            .sender
            .send(Notification::new(build::FINISHED.into(), params).into())?;
        Ok(())
    }

    fn time_out(&mut self, uri: &Uri) -> Result<()> {
        let Some(check) = self.checks.remove(uri) else {
            return Ok(());
        };
        check.stop();
        let seconds = run::check_timeout().as_secs_f64();
        let message = format!("LilyPond check took longer than {seconds} s and was stopped");
        if let Some(layers) = self.layers.get_mut(uri) {
            layers.set_check(vec![notice(message)]);
        }
        self.set_included(uri, RunKind::Check, HashMap::new())?;
        self.publish(uri)
    }

    fn time_out_build(&mut self, uri: &Uri) -> Result<()> {
        let Some(build) = self.builds.remove(uri) else {
            return Ok(());
        };
        let started = build.started;
        build.stop();
        let seconds = run::build_timeout().as_secs_f64();
        let message = format!("LilyPond build took longer than {seconds} s and was stopped");
        if let Some(layers) = self.layers.get_mut(uri) {
            layers.set_build(vec![notice(message)]);
        }
        self.set_included(uri, RunKind::Build, HashMap::new())?;
        self.publish(uri)?;
        self.send_finished(uri, Outputs::default(), false, started)
    }

    fn time_out_preview(&mut self, uri: &Uri) -> Result<()> {
        let Some(preview) = self.previews.remove(uri) else {
            return Ok(());
        };
        preview.running.stop();
        let _ = fs::remove_dir_all(&preview.out_dir);
        let seconds = run::build_timeout().as_secs_f64();
        let message = format!("LilyPond preview took longer than {seconds} s and was stopped");
        self.reply(failed(preview.request, message))
    }

    /// Replaces what one kind of run of `root` said about the files it included, and publishes
    /// every file whose diagnostics that changes.
    fn set_included(
        &mut self,
        root: &Uri,
        kind: RunKind,
        files: HashMap<PathBuf, Vec<Diagnostic>>,
    ) -> Result<()> {
        let key = (root.clone(), kind);
        let old = self.included.remove(&key).unwrap_or_default();
        let mut touched: Vec<PathBuf> = old.into_keys().chain(files.keys().cloned()).collect();
        touched.sort();
        touched.dedup();
        if !files.is_empty() {
            self.included.insert(key, files);
        }
        for path in touched {
            self.publish_path(&path)?;
        }
        Ok(())
    }

    /// What every run said about `path` from the files that include it, each message once, since
    /// a check and a build of the same file report its parse errors alike.
    fn included_for(&self, path: &Path) -> Vec<Diagnostic> {
        let mut found: Vec<Diagnostic> = Vec::new();
        for diagnostic in self
            .included
            .values()
            .filter_map(|files| files.get(path))
            .flatten()
        {
            if !found.contains(diagnostic) {
                found.push(diagnostic.clone());
            }
        }
        found
    }

    /// `range`, in UTF-8 positions in the file at `uri`, in the client's encoding. The open copy
    /// of the file is read when there is one, since the index summarised that copy.
    fn encoded(&self, uri: &Uri, range: Range) -> Range {
        if self.encoding == Encoding::Utf8 {
            return range;
        }
        let read;
        let (text, index) = match self.documents.get(uri) {
            Some(doc) => (doc.text.as_str(), &doc.index),
            None => {
                let text = file_uri::to_path(uri).and_then(|path| fs::read_to_string(path).ok());
                let Some(text) = text else { return range };
                read = (LineIndex::new(&text), text);
                (read.1.as_str(), &read.0)
            }
        };
        let convert = |p| {
            let byte = index.position_to_byte(text, p, Encoding::Utf8);
            index.byte_to_position(text, byte, self.encoding)
        };
        Range::new(convert(range.start), convert(range.end))
    }

    /// Rebuilds the syntax errors of one file and publishes all its diagnostics.
    fn refresh(&mut self, uri: &Uri) -> Result<()> {
        let (Some(doc), Some(layers)) = (self.documents.get(uri), self.layers.get_mut(uri)) else {
            return Ok(());
        };
        let mut instant = syntax::syntax_errors(doc, Encoding::Utf8);
        instant.extend(names::name_checks(
            doc,
            self.data.as_ref(),
            &self.index,
            Encoding::Utf8,
        ));
        instant.extend(includes::include_checks(
            doc,
            self.data.as_ref(),
            &self.index,
            Encoding::Utf8,
        ));
        layers.instant = instant;
        self.publish(uri)
    }

    /// Refreshes every open file but `except`.
    fn refresh_open(&mut self, except: Option<&Uri>) -> Result<()> {
        let uris: Vec<Uri> = self
            .documents
            .keys()
            .filter(|u| Some(*u) != except)
            .cloned()
            .collect();
        for uri in &uris {
            self.refresh(uri)?;
        }
        Ok(())
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
        self.send_diagnostics(PublishDiagnosticsParams::new(
            doc.uri.clone(),
            merged,
            Some(doc.version),
        ))
    }

    /// Publishes the diagnostics of a file by its path, whether it is open or not.
    fn publish_path(&self, path: &Path) -> Result<()> {
        let open = self
            .documents
            .keys()
            .find(|uri| file_uri::to_path(uri).as_deref() == Some(path));
        if let Some(uri) = open {
            return self.publish(uri);
        }
        let Some(uri) = file_uri::from_path(path) else {
            return Ok(());
        };
        self.send_diagnostics(PublishDiagnosticsParams::new(
            uri,
            self.included_for(path),
            None,
        ))
    }

    fn send_diagnostics(&self, params: PublishDiagnosticsParams) -> Result<()> {
        let notification = Notification::new(PublishDiagnostics::METHOD.into(), params);
        self.connection.sender.send(notification.into())?;
        Ok(())
    }

    /// A message the editor puts in front of the user.
    fn show(&self, typ: MessageType, message: String) -> Result<()> {
        let params = ShowMessageParams { typ, message };
        self.connection
            .sender
            .send(Notification::new(ShowMessage::METHOD.into(), params).into())?;
        Ok(())
    }

    /// A message for the editor's log, which the user reads only when looking for it.
    fn log(&self, typ: MessageType, message: String) -> Result<()> {
        let params = LogMessageParams { typ, message };
        self.connection
            .sender
            .send(Notification::new(LogMessage::METHOD.into(), params).into())?;
        Ok(())
    }
}

impl Drop for Server<'_> {
    /// Leaves no LilyPond running and nothing in the temporary folder.
    fn drop(&mut self) {
        for (_, check) in self.checks.drain() {
            check.stop();
        }
        for (_, build) in self.builds.drain() {
            build.stop();
        }
        // Previews and the images they answered with are in the work folder, removed below.
        for (_, preview) in self.previews.drain() {
            preview.running.stop();
        }
        // Before the work folder goes, so that no dump starts in it, or is left running in it.
        commanddata::stop_dump();
        run::remove_work_dir();
    }
}

/// Places a run's messages. Those about the file itself, at `path`, are in UTF-8 positions in
/// `text`, which is what the run read. Those about files it included are by path, in the
/// client's encoding, counted in each file as it is on disk, which is what LilyPond read.
fn place(
    stderr: &str,
    path: &Path,
    text: &str,
    index: &LineIndex,
    encoding: Encoding,
) -> (Vec<Diagnostic>, HashMap<PathBuf, Vec<Diagnostic>>) {
    let dir = path.parent().unwrap_or(Path::new("/"));
    let mut own = Vec::new();
    let mut others: HashMap<PathBuf, (String, LineIndex, Vec<Diagnostic>)> = HashMap::new();
    for message in messages::parse_stderr(stderr) {
        match &message.file {
            Some(LyFile::Path(named)) if dir.join(named) != path => {
                let (text, index, found) = others.entry(dir.join(named)).or_insert_with_key(|p| {
                    let text = fs::read_to_string(p).unwrap_or_default();
                    let index = LineIndex::new(&text);
                    (text, index, Vec::new())
                });
                found.push(to_diagnostic(&message, text, index));
            }
            // `-` is the file itself, and so is a message that names no file.
            _ => own.push(to_diagnostic(&message, text, index)),
        }
    }
    let others = others
        .into_iter()
        .map(|(p, (text, index, found))| (p, diagnostics::convert(found, &text, &index, encoding)))
        .collect();
    (own, others)
}

/// The open file whose run has the id `id`, among `runs` keyed by file.
fn owner<'a, T: 'a>(
    runs: impl IntoIterator<Item = (&'a Uri, &'a T)>,
    id: u64,
    running: fn(&T) -> &Running,
) -> Option<Uri> {
    runs.into_iter()
        .find(|(_, run)| running(run).id == id)
        .map(|(uri, _)| uri.clone())
}

/// The open files whose runs have passed their deadlines.
fn late<'a, T: 'a>(
    runs: impl IntoIterator<Item = (&'a Uri, &'a T)>,
    now: Instant,
    running: fn(&T) -> &Running,
) -> Vec<Uri> {
    let late = runs
        .into_iter()
        .filter(|(_, run)| running(run).deadline <= now);
    late.map(|(uri, _)| uri.clone()).collect()
}

/// Records an open file in the index, and returns whether what it defines or includes changed. A
/// file with no path cannot be included, so it is not recorded.
fn index_update(index: &mut Index, doc: &Document) -> bool {
    file_uri::to_path(&doc.uri).is_some_and(|path| index.update(&path, doc))
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

fn invalid(id: RequestId, message: &str) -> Response {
    Response::new_err(id, ErrorCode::InvalidParams as i32, message.into())
}

/// The reply to a command that was valid but could not be carried out.
fn failed(id: RequestId, message: String) -> Response {
    Response::new_err(id, ErrorCode::RequestFailed as i32, message)
}

/// The file's text and every node of its tree, for the tests to compare with a fresh parse.
fn debug_tree(doc: &Document) -> Value {
    fn dump(node: Node, out: &mut String) {
        let missing = if node.is_missing() { " MISSING" } else { "" };
        let _ = write!(
            out,
            "({}{missing} {}-{}",
            node.kind(),
            node.start_byte(),
            node.end_byte()
        );
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
