//! What the installed LilyPond knows about itself: its music functions, markup commands, grobs,
//! properties and contexts, dumped once per installation and cached.

pub mod cache;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::info::Lilypond;
use crate::lilypond::run;

/// Swara's own Scheme, which has the installed LilyPond describe itself.
const DUMP: &str = include_str!("dump.ly");

/// The record layout `dump.ly` writes. A cache in any other layout is dumped again.
const FORMAT: u32 = 1;

/// How long a dump may run. It takes well under a second.
const DUMP_TIMEOUT: Duration = Duration::from_secs(60);

static NEXT_DUMP: AtomicU64 = AtomicU64::new(1);

/// The dump running now, so that a server shutting down can stop it.
static DUMP_STATE: Mutex<DumpState> = Mutex::new(DumpState::Idle);

enum DumpState {
    Idle,
    /// The id of the dump's process, which leads its own group.
    Running(u32),
    /// The server is shutting down, so no dump may start.
    Stopped,
}

/// What an argument takes, which decides what in the source binds to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    Pitch,
    Duration,
    Music,
    Word,
    Markup,
    Number,
    /// Anything at all, so it binds whatever comes.
    Scheme,
}

/// The kind of argument one of LilyPond's type predicates accepts. A predicate that accepts more
/// than one kind, such as `symbol-list-or-music?`, is `Scheme`.
pub fn arg_kind(pred: &str) -> ArgKind {
    match pred {
        "ly:pitch?" => ArgKind::Pitch,
        "ly:duration?" => ArgKind::Duration,
        "ly:music?" | "ly:event?" => ArgKind::Music,
        "string?" | "symbol?" | "symbol-list?" | "symbol-list-or-symbol?" | "key?" | "key-list?"
        | "key-list-or-symbol?" => ArgKind::Word,
        "markup?" | "cheap-markup?" | "markup-list?" => ArgKind::Markup,
        "number?" | "integer?" | "real?" | "rational?" | "index?" | "positive-number?"
        | "non-negative-number?" | "exact-rational?" | "fraction?" | "ly:dir?" => ArgKind::Number,
        _ => ArgKind::Scheme,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentifierKind {
    Music,
    Markup,
    Number,
    String,
    /// A context definition or a context modification, such as `RemoveEmptyStaves`.
    ContextDef,
    Scheme,
}

/// Whether a property belongs to grobs or to contexts. Some names are both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Backend,
    Context,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Arg {
    /// The predicate's name, such as `ly:pitch?`, or empty when it has none.
    pub pred: String,
    pub optional: bool,
}

impl Arg {
    pub fn kind(&self) -> ArgKind {
        arg_kind(&self.pred)
    }
}

/// A music function, or a markup or markup-list command.
#[derive(Debug, Deserialize)]
pub struct Function {
    pub name: String,
    pub args: Vec<Arg>,
    pub doc: Option<String>,
    /// A markup-list command, such as `\wordwrap-lines`.
    #[serde(skip)]
    pub list: bool,
}

#[derive(Debug, Deserialize)]
pub struct Grob {
    pub name: String,
    pub interfaces: Vec<String>,
    pub doc: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Interface {
    pub name: String,
    pub props: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Property {
    pub name: String,
    pub scope: Scope,
    pub pred: String,
    pub doc: Option<String>,
}

/// A context or a translator.
#[derive(Debug, Deserialize)]
pub struct Described {
    pub name: String,
    pub doc: Option<String>,
}

/// One line of the dump.
#[derive(Deserialize)]
#[serde(tag = "k", rename_all = "kebab-case")]
enum Record {
    Meta { format: u32, lilypond: String, datadir: Option<PathBuf> },
    MusicFunction(Function),
    Markup(Function),
    MarkupList(Function),
    Identifier { name: String, kind: IdentifierKind },
    Grob(Grob),
    Interface(Interface),
    Property(Property),
    Context(Described),
    Translator(Described),
    MusicType { name: String },
    EventClass { name: String },
    Language { name: String, pitches: HashSet<String> },
    Drums { names: HashSet<String> },
    /// The last line, which says the dump finished.
    End,
    /// A record from a newer dump, which this version skips.
    #[serde(other)]
    Unknown,
}

/// Everything one LilyPond installation defines, by name.
#[derive(Debug, Default)]
pub struct CommandData {
    pub lilypond: String,
    /// The installation's data folder, whose `ly` folder holds the files `\include` finds there.
    pub datadir: Option<PathBuf>,
    music_functions: HashMap<String, Function>,
    markups: HashMap<String, Function>,
    identifiers: HashMap<String, IdentifierKind>,
    grobs: HashMap<String, Grob>,
    interfaces: HashMap<String, Interface>,
    backend_properties: HashMap<String, Property>,
    context_properties: HashMap<String, Property>,
    contexts: HashMap<String, Described>,
    translators: HashMap<String, Described>,
    music_types: HashSet<String>,
    event_classes: HashSet<String>,
    languages: HashMap<String, HashSet<String>>,
    drums: HashSet<String>,
}

impl CommandData {
    pub fn music_function(&self, name: &str) -> Option<&Function> {
        self.music_functions.get(name)
    }

    pub fn music_functions(&self) -> impl Iterator<Item = &Function> {
        self.music_functions.values()
    }

    /// A markup or markup-list command.
    pub fn markup(&self, name: &str) -> Option<&Function> {
        self.markups.get(name)
    }

    pub fn identifier(&self, name: &str) -> Option<IdentifierKind> {
        self.identifiers.get(name).copied()
    }

    pub fn grob(&self, name: &str) -> Option<&Grob> {
        self.grobs.get(name)
    }

    pub fn interface(&self, name: &str) -> Option<&Interface> {
        self.interfaces.get(name)
    }

    pub fn property(&self, name: &str, scope: Scope) -> Option<&Property> {
        match scope {
            Scope::Backend => self.backend_properties.get(name),
            Scope::Context => self.context_properties.get(name),
        }
    }

    pub fn context(&self, name: &str) -> Option<&Described> {
        self.contexts.get(name)
    }

    /// The markup and markup-list commands.
    pub fn markups(&self) -> impl Iterator<Item = &Function> {
        self.markups.values()
    }

    pub fn identifiers(&self) -> impl Iterator<Item = (&str, IdentifierKind)> {
        self.identifiers.iter().map(|(name, kind)| (name.as_str(), *kind))
    }

    pub fn contexts(&self) -> impl Iterator<Item = &Described> {
        self.contexts.values()
    }

    pub fn grobs(&self) -> impl Iterator<Item = &Grob> {
        self.grobs.values()
    }

    pub fn properties(&self, scope: Scope) -> impl Iterator<Item = &Property> {
        match scope {
            Scope::Backend => self.backend_properties.values(),
            Scope::Context => self.context_properties.values(),
        }
    }

    /// The note names of one language, such as `english`.
    pub fn pitches(&self, language: &str) -> Option<&HashSet<String>> {
        self.languages.get(language)
    }

    pub fn drums(&self) -> &HashSet<String> {
        &self.drums
    }

    /// Whether `\name` means something LilyPond defines. In markup that is a markup or
    /// markup-list command, and elsewhere a music function or a context, which `\Staff` names
    /// inside `\context { }`. A variable LilyPond defines counts in both.
    pub fn is_known_command(&self, name: &str, in_markup: bool) -> bool {
        self.identifiers.contains_key(name)
            || if in_markup {
                self.markups.contains_key(name)
            } else {
                self.music_functions.contains_key(name) || self.contexts.contains_key(name)
            }
    }

    /// How many of each kind there are, for the tests.
    pub fn summary(&self) -> Value {
        let lists = self.markups.values().filter(|m| m.list).count();
        json!({
            "lilypond": self.lilypond,
            "musicFunctions": self.music_functions.len(),
            "markups": self.markups.len() - lists,
            "markupLists": lists,
            "identifiers": self.identifiers.len(),
            "grobs": self.grobs.len(),
            "interfaces": self.interfaces.len(),
            "backendProperties": self.backend_properties.len(),
            "contextProperties": self.context_properties.len(),
            "contexts": self.contexts.len(),
            "translators": self.translators.len(),
            "musicTypes": self.music_types.len(),
            "eventClasses": self.event_classes.len(),
            "languages": self.languages.len(),
            "drums": self.drums.len(),
        })
    }
}

/// Reads a dump, or a cache of one. Fails on a line that is not a record, and on a dump that
/// lacks its `meta` record, is in another layout, or stops before its `end` record.
pub fn parse(text: &str) -> Result<CommandData, String> {
    let mut data = CommandData::default();
    let (mut meta, mut end) = (false, false);
    for (number, line) in text.lines().enumerate() {
        let record: Record =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", number + 1))?;
        match record {
            Record::Meta { format, lilypond, datadir } => {
                if format != FORMAT {
                    return Err(format!("records in layout {format}, not {FORMAT}"));
                }
                data.lilypond = lilypond;
                data.datadir = datadir;
                meta = true;
            }
            Record::MusicFunction(f) => {
                data.music_functions.insert(f.name.clone(), f);
            }
            Record::Markup(f) => {
                data.markups.insert(f.name.clone(), f);
            }
            Record::MarkupList(f) => {
                data.markups.insert(f.name.clone(), Function { list: true, ..f });
            }
            Record::Identifier { name, kind } => {
                data.identifiers.insert(name, kind);
            }
            Record::Grob(g) => {
                data.grobs.insert(g.name.clone(), g);
            }
            Record::Interface(i) => {
                data.interfaces.insert(i.name.clone(), i);
            }
            Record::Property(p) => {
                let properties = match p.scope {
                    Scope::Backend => &mut data.backend_properties,
                    Scope::Context => &mut data.context_properties,
                };
                properties.insert(p.name.clone(), p);
            }
            Record::Context(c) => {
                data.contexts.insert(c.name.clone(), c);
            }
            Record::Translator(t) => {
                data.translators.insert(t.name.clone(), t);
            }
            Record::MusicType { name } => {
                data.music_types.insert(name);
            }
            Record::EventClass { name } => {
                data.event_classes.insert(name);
            }
            Record::Language { name, pitches } => {
                data.languages.insert(name, pitches);
            }
            Record::Drums { names } => data.drums = names,
            Record::End => end = true,
            Record::Unknown => {}
        }
    }
    match (meta, end) {
        (false, _) => Err("no meta record".into()),
        (true, false) => Err("the records stop before the end".into()),
        (true, true) => Ok(data),
    }
}

/// The command data of `lilypond`: from the cache when there is one, and otherwise from a dump,
/// which is then cached.
pub fn load(lilypond: &Path) -> Result<CommandData, String> {
    let found = Lilypond::at(lilypond)
        .ok_or_else(|| format!("{} did not report its version", lilypond.display()))?;
    let path = cache::path(&found.path, &found.version);
    if let Ok(text) = fs::read_to_string(&path)
        && let Ok(data) = parse(&text)
    {
        return Ok(data);
    }
    let text = dump(&found.path)?;
    let data = parse(&text)?;
    // A cache that cannot be written only means dumping again at the next start.
    let _ = cache::store(&path, &text);
    Ok(data)
}

/// Runs `dump.ly` with `lilypond` and returns what it wrote. The script, its output and its log
/// are kept in this server's private work folder, and removed afterwards.
fn dump(lilypond: &Path) -> Result<String, String> {
    // Held until the dump has started, so a shutdown either comes first and no dump starts, or
    // comes after and stops it. Either way the work folder is not made again once it is removed.
    let mut state = DUMP_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if matches!(*state, DumpState::Stopped) {
        return Err("the server is shutting down".into());
    }
    let dir = run::work_dir().map_err(|error| format!("no work folder: {error}"))?;
    let n = NEXT_DUMP.fetch_add(1, Ordering::Relaxed);
    let [script, out, log] = ["ly", "jsonl", "log"].map(|ext| dir.join(format!("dump-{n}.{ext}")));
    let dumped = start_dump(lilypond, &dir, &script, &out, &log).and_then(|mut child| {
        *state = DumpState::Running(child.id());
        drop(state);
        let finished = wait_for_dump(&mut child);
        let said = fs::read_to_string(&log).unwrap_or_default();
        let said = said.lines().last().unwrap_or("").trim().to_owned();
        match finished {
            Some(status) if status.success() => fs::read_to_string(&out)
                .map_err(|error| format!("the dump wrote nothing: {error}")),
            Some(status) => Err(format!("the dump failed ({status}): {said}")),
            None => {
                let limit = DUMP_TIMEOUT.as_secs();
                Err(format!("the dump took longer than {limit} s and was stopped"))
            }
        }
    });
    for file in [script, out, log] {
        let _ = fs::remove_file(file);
    }
    dumped
}

fn start_dump(
    lilypond: &Path,
    dir: &Path,
    script: &Path,
    out: &Path,
    log: &Path,
) -> Result<Child, String> {
    let failed = |what: &str, error: io::Error| format!("{what}: {error}");
    create_private(script)
        .and_then(|mut file| file.write_all(DUMP.as_bytes()))
        .map_err(|error| failed("the dump script could not be written", error))?;
    let stderr =
        create_private(log).map_err(|error| failed("the dump log could not be made", error))?;
    let mut command = Command::new(lilypond);
    run::english(&mut command)
        .args(["-l", "ERROR"])
        .arg(script)
        .current_dir(dir)
        .env("SWARA_DUMP_OUT", out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command
        .spawn()
        .map_err(|error| failed(&format!("{} could not be started", lilypond.display()), error))
}

/// Waits for the dump to exit, and stops it once `DUMP_TIMEOUT` is up, which is the one case that
/// gives None.
fn wait_for_dump(child: &mut Child) -> Option<ExitStatus> {
    let deadline = Instant::now() + DUMP_TIMEOUT;
    loop {
        {
            // Reaped only while the lock is held, so `stop_dump` never signals a reused id.
            let mut state = DUMP_STATE.lock().unwrap_or_else(PoisonError::into_inner);
            let done = match child.try_wait() {
                Ok(Some(status)) => Some(Some(status)),
                Ok(None) if Instant::now() < deadline => None,
                _ => {
                    run::kill_group(child);
                    Some(None)
                }
            };
            if let Some(status) = done {
                if matches!(*state, DumpState::Running(_)) {
                    *state = DumpState::Idle;
                }
                return status;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Stops the dump that is running, with everything it started, and any dump from starting after
/// it. A server calls this as it shuts down.
pub fn stop_dump() {
    let mut state = DUMP_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    // Elsewhere a dump runs to its end, which takes under a second.
    #[cfg(unix)]
    if let DumpState::Running(id) = *state {
        // SAFETY: killpg only sends a signal, and takes no pointers. The dump leads its own group,
        // and is not reaped while the lock is held, so the id is still its own.
        unsafe { libc::killpg(id as libc::pid_t, libc::SIGKILL) };
    }
    *state = DumpState::Stopped;
}

/// A new file that only this user can open.
fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    const MINI: &str = include_str!("../../tests/fixtures/commanddata/mini.jsonl");

    #[test]
    fn parses_mini_fixture() {
        let data = parse(MINI).unwrap();
        assert_eq!(
            data.summary(),
            json!({
                "lilypond": "2.26.0",
                "musicFunctions": 8,
                "markups": 5,
                "markupLists": 1,
                "identifiers": 7,
                "grobs": 3,
                "interfaces": 3,
                "backendProperties": 5,
                "contextProperties": 3,
                "contexts": 4,
                "translators": 2,
                "musicTypes": 2,
                "eventClasses": 2,
                "languages": 2,
                "drums": 3,
            })
        );
        assert_eq!(data.datadir.as_deref(), Some(Path::new("/usr/share/lilypond/2.26.0")));

        assert_eq!(arg_kind("ly:pitch?"), ArgKind::Pitch);
        assert_eq!(arg_kind("string?"), ArgKind::Word);
        assert_eq!(arg_kind("markup?"), ArgKind::Markup);

        let relative = data.music_function("relative").unwrap();
        let args: Vec<(ArgKind, bool)> =
            relative.args.iter().map(|a| (a.kind(), a.optional)).collect();
        assert_eq!(args, [(ArgKind::Pitch, true), (ArgKind::Music, false)]);
        assert!(relative.doc.as_deref().is_some_and(|doc| doc.contains("octave")));
        assert!(data.music_function("bold").is_none());

        assert!(!data.markup("bold").unwrap().list);
        assert!(data.markup("wordwrap-lines").unwrap().list);
        assert!(data.markup("relative").is_none());

        assert_eq!(data.identifier("fermata"), Some(IdentifierKind::Music));
        assert_eq!(data.identifier("major"), Some(IdentifierKind::Scheme));
        assert_eq!(data.identifier("up"), Some(IdentifierKind::Number));
        assert_eq!(data.identifier("default-language"), Some(IdentifierKind::String));
        assert_eq!(data.identifier("whiteTriangleMarkup"), Some(IdentifierKind::Markup));
        assert_eq!(data.identifier("RemoveEmptyStaves"), Some(IdentifierKind::ContextDef));
        assert_eq!(data.identifier("melody"), None);

        let note_head = data.grob("NoteHead").unwrap();
        assert!(note_head.interfaces.iter().any(|i| i == "note-head-interface"));
        assert_eq!(data.interface("grob-interface").unwrap().props, ["color", "transparent"]);

        let backend = data.property("color", Scope::Backend).unwrap();
        let context = data.property("color", Scope::Context).unwrap();
        assert_eq!(backend.pred, "color?");
        assert_ne!(backend.doc, context.doc);
        assert!(data.property("instrumentName", Scope::Backend).is_none());
        assert!(data.property("instrumentName", Scope::Context).is_some());

        assert!(data.context("Staff").is_some());
        assert!(data.context("NoteHead").is_none());

        let english = data.pitches("english").unwrap();
        assert!(english.contains("cs") && !english.contains("cis"));
        assert!(data.pitches("nederlands").unwrap().contains("cis"));
        assert!(data.pitches("klingon").is_none());
        assert!(data.drums().contains("bd") && !data.drums().contains("c"));
    }

    #[test]
    fn known_commands_depend_on_markup() {
        let data = parse(MINI).unwrap();
        assert!(data.is_known_command("relative", false));
        assert!(!data.is_known_command("relative", true));
        assert!(data.is_known_command("bold", true));
        assert!(!data.is_known_command("bold", false));
        assert!(data.is_known_command("wordwrap-lines", true));
        // Variables LilyPond defines work in both.
        assert!(data.is_known_command("fermata", false));
        assert!(data.is_known_command("whiteTriangleMarkup", true));
        // `\Staff` names a context inside `\context { }` and `\with { }`.
        assert!(data.is_known_command("Staff", false));
        assert!(!data.is_known_command("melodyy", false));
        assert!(!data.is_known_command("melodyy", true));
    }

    #[test]
    fn arg_kinds() {
        let cases = [
            ("ly:duration?", ArgKind::Duration),
            ("ly:music?", ArgKind::Music),
            ("ly:event?", ArgKind::Music),
            ("symbol?", ArgKind::Word),
            ("symbol-list-or-symbol?", ArgKind::Word),
            ("cheap-markup?", ArgKind::Markup),
            ("markup-list?", ArgKind::Markup),
            ("number?", ArgKind::Number),
            ("index?", ArgKind::Number),
            ("fraction?", ArgKind::Number),
            // A predicate that takes music or something else binds whatever comes.
            ("symbol-list-or-music?", ArgKind::Scheme),
            ("color?", ArgKind::Scheme),
            ("scheme?", ArgKind::Scheme),
        ];
        for (pred, kind) in cases {
            assert_eq!(arg_kind(pred), kind, "{pred}");
        }
    }

    #[test]
    fn rejects_a_cache_it_cannot_trust() {
        let without_meta: String = MINI.lines().skip(1).map(|l| format!("{l}\n")).collect();
        assert!(parse(&without_meta).is_err());
        let other_format = MINI.replacen("\"format\":1", "\"format\":2", 1);
        assert!(parse(&other_format).is_err());
        let cut = &MINI[..MINI.len() / 2];
        assert!(parse(cut).is_err());
        // Whole lines, but the dump stopped before its last record.
        let lines: Vec<&str> = MINI.lines().collect();
        let unfinished = lines[..lines.len() - 1].join("\n");
        assert!(parse(&format!("{unfinished}\n")).is_err());
        assert!(parse(MINI).is_ok());
        assert!(parse("").is_err());
    }

    /// The dump of the LilyPond on `PATH`, run for real.
    #[test]
    #[ignore = "runs the installed LilyPond"]
    fn real_dump_counts() {
        let lilypond = crate::info::find_on_path("lilypond").expect("lilypond on PATH");
        let text = dump(&lilypond).unwrap();
        let data = parse(&text).unwrap();
        let summary = data.summary();
        let count = |key: &str| summary[key].as_u64().unwrap();
        assert!(count("musicFunctions") >= 190, "{summary}");
        assert!(count("markups") >= 185, "{summary}");
        assert!(count("grobs") >= 160, "{summary}");
        assert!(count("contexts") >= 40, "{summary}");
        assert!(count("contextProperties") >= 270, "{summary}");
        assert!(data.pitches("english").unwrap().contains("cs"));
        for name in ["fermata", "f", "major"] {
            assert!(data.identifier(name).is_some(), "{name}");
        }
        assert!(data.datadir.as_deref().is_some_and(Path::is_dir), "{:?}", data.datadir);
        // Every predicate has a name, so none falls back to the catch-all by accident.
        let unnamed = data.music_functions().flat_map(|f| &f.args).filter(|a| a.pred.is_empty());
        assert_eq!(unnamed.count(), 0);
    }
}
