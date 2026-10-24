# Swara server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The `swara` program: a language server with layered diagnostics, completion, hover, definitions and an outline; a spacing-only formatter; the save build with its notification to the plugin; and, if the playback spike chose streaming, the MIDI player. All of it meets the speed table in the design.

**Architecture:** A synchronous `lsp-server` loop on one thread selects over three sources: client messages, results from LilyPond child processes, and one timer. Documents are parsed incrementally with the grammar compiled in. LilyPond runs as a fresh child process per check or build; one helper thread per child feeds stdin and collects stderr. Diagnostics live in three layers per file, `instant`, `check` and `build`, and every publish sends their union. A second workspace crate, `swara-parser`, is a C-ABI shared library that exposes the same grammar to Neovim.

**Tech Stack:** Rust 1.98 (edition 2024), `lsp-server` 0.10.0, `lsp-types` 0.97.0, `crossbeam-channel` (through `lsp-server`), `serde` and `serde_json` 1, `tree-sitter` 0.27.1, `streaming-iterator` (the `tree-sitter` query API), `libc` (Unix process groups only), and the grammar as a git dependency pinned to `v0.1.0`'s commit.

**Spec:** `docs/design/2026-10-24-swara-design.md`, sections "The language server", "Speed targets" and "Installing".

**Plan order:** After `2026-10-24-swara-grammar.md`'s node contract is committed. It consumes the spike results and fixtures from `2026-10-24-swara-foundations.md`.

## Global Constraints

Everything under Global Constraints in the foundations plan applies. In addition:

- No async runtime. Parsing happens on the main thread; only child-process I/O runs on helper threads.
- Positions are UTF-8 when the client offers it, and UTF-16 otherwise.
- Every LilyPond run sets `LC_ALL=C`, passes `-l WARNING`, uses absolute paths, puts options before the file name, and sends check and preview output to a temporary folder.
- Opening a file never runs LilyPond. The first check needs an edit, and the first build needs a save or an explicit build command.
- Test fixtures for command data are hand-written and MIT. Real dump output holds LilyPond's GPL docstrings, so it is never committed, even as a fixture.
- Diagnostics from Swara use `source = "swara"`, and diagnostics from LilyPond use `source = "lilypond"`.
- The test LilyPond for process-control tests is `tests/fixtures/fake-lilypond.sh`, passed as `initializationOptions.lilypond`. Speed tests use the real LilyPond 2.26.0.

## Review Focus

- **Fast typing.** Fifty edits 20 ms apart must start exactly one LilyPond check, after the pause, and leave no child process behind. The test lives in the check task.
- **Scheme that never finishes,** such as `#(let loop () (loop))` typed into a lesson. The check stops after 10 s, shows one information diagnostic and leaves no process. The test lives in the check task.
- **Folder and file names with spaces or Bengali,** such as `~/Music/আমার গান/lesson 1.ly`. Checks, builds, include resolution and message parsing must all work. Tests live in the messages task and the build task.
- **Saving twice in quick succession.** The older build is killed, only the newer one reports, and the PDF is replaced in one step, so Skim never reads a half-written file. The test lives in the build task.
- **No LilyPond on `PATH`, or a different version.** Highlighting, syntax errors and formatting still work, one message says why the rest is off, and the command cache is keyed by version, so 2.24 and 2.26 never share one. The test lives in the command data task.

---

### Task: Workspace crates, CLI and `swara info`

**Files:**
- Modify: `Cargo.toml` (members `crates/swara`, `crates/swara-parser`, `xtask`)
- Create: `crates/swara/Cargo.toml`, `crates/swara/src/main.rs`, `crates/swara/src/info.rs`
- Create: `crates/swara-parser/Cargo.toml` (`crate-type = ["cdylib"]`), `crates/swara-parser/src/lib.rs`, `crates/swara-parser/tests/load.rs`
- Create: `deny.toml`, `docs/dependencies.md` (add every crate above with licence, last release and what it pulls in)
- Create: `.github/workflows/ci.yml` (`cargo test` and `cargo deny check` on macOS and Ubuntu, LilyPond 2.26.0 installed the way the grammar CI installs it, and `--include-ignored` on macOS only; every action pinned to a commit)

**Interfaces:**
- Produces: `swara lsp`, `swara format [--indent N]`, `swara info`, `swara --version`, and, when streaming won, `swara play --port P FILE`. Arguments are parsed by hand, with no CLI crate.
- Produces: `swara info` prints one JSON object: `{"version","grammar","grammar_symbol":"lilypond_swara","grammar_rev","abi":15,"lilypond":{"path","version"}|null,"cache":{"path","present"}}`. `grammar` is the first of `<exe dir>/../lib/<libname>` and `<exe dir>/<libname>` that exists, where `<exe dir>` comes from the canonicalised `current_exe`, so a Homebrew symlink in `/opt/homebrew/bin` still finds the library in the Cellar, and where `<libname>` is `libswara_parser.dylib`, `libswara_parser.so` or `swara_parser.dll`.
- Produces: in `swara-parser`, `#[unsafe(no_mangle)] pub extern "C" fn tree_sitter_lilypond_swara() -> *const ()`, which returns `tree_sitter_lilypond::LANGUAGE.into_raw()()`. Neovim loads it with `symbol_name = 'lilypond_swara'`.

- [ ] **Write the failing tests.**
  - `info_reports_grammar_and_abi` runs `CARGO_BIN_EXE_swara info`, parses the JSON, and asserts `abi == 15`, that `grammar` is an existing file, and that `version` equals `CARGO_PKG_VERSION`.
  - `info_through_symlink` copies the binary and library into a temporary `prefix/bin` and `prefix/lib`, runs it through a symlink in another folder, and asserts `grammar` points into `prefix/lib`.
  - `unknown_subcommand_exits_2` checks that an unknown subcommand exits 2 and prints usage to stderr.
  - `load.rs::exported_symbol_gives_abi_15` opens the built cdylib with `libloading` (a dev-dependency, already reviewed), calls the symbol, wraps the result in `tree_sitter::Language`, and asserts `abi_version() == 15` and `node_kind_is_named` for `embedded_lilypond`.
- [ ] **Run them to see them fail.** Run `cargo test -p swara -p swara-parser`. Expected: compile failure.
- [ ] **Implement `main.rs`, `info.rs` and the cdylib.** `lilypond` is resolved from `PATH` and canonicalised, and its version is read from the first line of `lilypond --version`.
- [ ] **Run them to see them pass.** Run `cargo test -p swara -p swara-parser && cargo deny check`. Expected: PASS.
- [ ] **Commit:** `feat(swara): add the workspace, CLI and grammar library`.

### Task: Positions

**Files:**
- Create: `crates/swara/src/line_index.rs`

**Interfaces:**
- Produces: `pub enum Encoding { Utf8, Utf16 }` and `pub struct LineIndex`, with `LineIndex::new(text: &str)`, `fn position_to_byte(&self, text: &str, p: lsp_types::Position, enc: Encoding) -> usize`, `fn byte_to_position(&self, text: &str, byte: usize, enc: Encoding) -> lsp_types::Position`, and `fn lilypond_column_to_byte(&self, text: &str, line0: u32, col1: u32) -> usize`.

- [ ] **Write the failing tests.**
  - `utf8_round_trip` and `utf16_round_trip` run on `"a\tগান c4\n\"আমি\" d"` at every character boundary.
  - `lilypond_tab_column`: on the line `"\tc4 \\foo"`, LilyPond's column 10 is the byte of `\`, because the tab advances to column 9.
  - `lilypond_bengali_column`: on `"\"গান\" \\foo"`, column 7 is the byte of `\`, because LilyPond counts characters.
  - `crlf_lines`: a `\r\n` file gives the same line starts as a `\n` file, plus one byte per line.
  - The tab and Bengali expectations are cross-checked against `tests/fixtures/lilypond/tab-column.stderr` and `utf8-column.stderr` from the foundations plan.
- [ ] **Run them to see them fail.** Run `cargo test -p swara line_index`. Expected: compile failure.
- [ ] **Implement `LineIndex`.**
- [ ] **Run them to see them pass.** Run `cargo test -p swara line_index`. Expected: PASS.
- [ ] **Commit:** `feat(swara): convert LSP and LilyPond positions`.

### Task: Documents and the server loop

**Files:**
- Create: `crates/swara/src/document.rs`, `crates/swara/src/server.rs`, `crates/swara/src/diagnostics/mod.rs`, `crates/swara/src/diagnostics/syntax.rs`
- Create: `crates/swara/tests/client/mod.rs`, `crates/swara/tests/lsp_basics.rs`

**Interfaces:**
- Produces: `pub struct Document { pub uri: Uri, pub version: i32, pub text: String, pub tree: tree_sitter::Tree, pub index: LineIndex }` with `fn apply(&mut self, change: &TextDocumentContentChangeEvent, enc: Encoding, parser: &mut Parser) -> Option<InputEdit>`.
- Produces: `pub struct Layers { pub instant: Vec<Diagnostic>, pub check: Vec<Diagnostic>, pub build: Vec<Diagnostic> }`, with `fn merged(&self) -> Vec<Diagnostic>` and `fn shift(&mut self, edit: &InputEdit, old: &LineIndex, new: &LineIndex)`. `shift` moves `check` and `build` entries after an edit by its line and column delta, and pins an entry inside the edited range to the range's start.
- Produces: `pub fn syntax_errors(doc: &Document, enc: Encoding) -> Vec<Diagnostic>`. A `MISSING` node gives "missing `<kind>`". An `ERROR` node gives "unexpected `<first token, at most 20 characters>`".
- Produces: `pub struct TestClient` in `tests/client/mod.rs`, with `spawn(init_options: serde_json::Value) -> Self`, `open(uri, text)`, `change(uri, version, range, text)`, `save(uri)`, `request<R: Request>(params) -> R::Result`, and `wait_diagnostics(uri, timeout, pred) -> (Vec<Diagnostic>, Duration)`.
- Produces: the initialize result. It advertises `positionEncoding`, incremental sync with `save`, completion with trigger characters `\` and `.`, hover, definition, document symbols, formatting, and `executeCommandProvider` with `swara.build` and `swara.preview`. `initializationOptions` is `{"autoCheck": bool, "lilypond": string|null, "includePaths": [string]}`.

- [ ] **Write the failing tests** in `lsp_basics.rs`:
  - `negotiates_utf8` checks that a client offering `["utf-8","utf-16"]` gets `utf-8`, and one offering only `utf-16` gets `utf-16`.
  - `syntax_error_published`: opening `{ c d` publishes one diagnostic, "missing `}`", with `source == "swara"`.
  - `edit_fixes_error`: appending `}` publishes an empty list.
  - `incremental_equals_fresh` applies 200 random single-character edits through the client, then asserts that `swara`'s tree equals a fresh parse. The server exposes a test-only request, `swara/debugTree`, enabled with `SWARA_TEST=1`.
  - `layers_shift` is a unit test: a `check` diagnostic on line 10 moves to line 12 after a two-line insert on line 3.
- [ ] **Run them to see them fail.** Run `cargo test -p swara --test lsp_basics`. Expected: FAIL.
- [ ] **Implement** the document, the loop with `crossbeam_channel::select!` over the connection, a results channel and `crossbeam_channel::at(next_deadline)`, and syntax diagnostics.
- [ ] **Run them to see them pass.** Run `cargo test -p swara`. Expected: PASS.
- [ ] **Commit:** `feat(swara): serve documents with instant syntax diagnostics`.

### Task: LilyPond message parser

**Files:**
- Create: `crates/swara/src/lilypond/messages.rs`

**Interfaces:**
- Produces: `pub enum LyFile { Stdin, Path(PathBuf) }`, `pub enum Severity { Error, Warning, ProgrammingError }`, `pub struct LyMessage { pub file: Option<LyFile>, pub line: Option<u32>, pub col: Option<u32>, pub severity: Severity, pub text: String }`, and `pub fn parse_stderr(stderr: &str) -> Vec<LyMessage>`. Lines are 1-based as LilyPond prints them, and the two context lines after each message are consumed, not reported.

- [ ] **Write the failing tests,** one per recorded fixture under `tests/fixtures/lilypond/`: parse error, unknown command, bar check, no version (a line and no column), Guile error, reader error (no severity word, so `Error`), no location, include error (an absolute path), stdin (`-` gives `LyFile::Stdin`), tab and Bengali columns, and `programming error` if it was recorded. Each asserts the exact file, line, column, severity and text prefix. Add `path_with_spaces_and_bengali`: `/Users/x/আমার গান/lesson 1.ly:3:5: error: syntax error` gives that path, line 3 and column 5. Add `fatal_summary_ignored`: `fatal error: failed files: "-"` gives no message.
- [ ] **Run them to see them fail.** Run `cargo test -p swara messages`. Expected: compile failure.
- [ ] **Implement `parse_stderr`.** Split from the right on `: error: `, `: warning: ` and `: programming error: `, so colons and spaces in paths survive. Then take the trailing `:LINE[:COL]` from the location part.
- [ ] **Run them to see them pass.** Run `cargo test -p swara messages`. Expected: PASS.
- [ ] **Commit:** `feat(swara): parse LilyPond diagnostics`.

### Task: The LilyPond check on each pause

**Files:**
- Create: `crates/swara/src/lilypond/mod.rs`, `crates/swara/src/lilypond/run.rs`, `crates/swara/assets/lilypond/check-settings.ly` (from the spike, if a candidate won)
- Create: `crates/swara/tests/fixtures/fake-lilypond.sh`, `crates/swara/tests/lsp_check.rs`

**Interfaces:**
- Produces: `pub struct RunSpec { pub kind: RunKind, pub uri: Uri, pub version: i32, pub stdin: Option<String>, pub args: Vec<OsString>, pub out_dir: PathBuf }`, `pub enum RunKind { Check, Build, Preview }`, `pub fn check_spec(lilypond: &Path, file: &Path, text: &str, version: i32, include_paths: &[PathBuf]) -> RunSpec` (the exact argument list recorded in `docs/spikes/lilypond-check.md`), and `pub fn spawn(spec: RunSpec, results: Sender<RunResult>) -> Running`.
- Produces: `pub struct Running { pub id: u64, pub child: Child, pub deadline: Instant }` and `pub struct RunResult { pub id: u64, pub stderr: String, pub status: Option<ExitStatus> }`.
- Produces: the constants `PAUSE = 330 ms` and `CHECK_TIMEOUT = 10 s`.
- Behaviour: on Unix the child gets its own process group, and stopping it kills the group with `libc::killpg`. On Windows it is `Child::kill`. A helper thread writes stdin, then reads stderr to the end; the main thread reaps with `wait`.
- Behaviour: a result whose id is not the current run for that document is dropped. Messages naming `-` map to the document. Messages naming another file go to that file's `check` layer, keyed by the root document, so `.ily` files receive their own diagnostics and they clear when the root's next check replaces them.

- [ ] **Write the fake LilyPond.** `fake-lilypond.sh` appends `$$ $*` to `$SWARA_FAKE_LOG`. In mode `sleep:N` it starts `sleep 30 &`, records that child's pid, then sleeps N seconds. In mode `stderr:FILE` it prints that file to stderr and exits 1.
- [ ] **Write the failing tests** in `lsp_check.rs`:
  - `open_never_runs_lilypond`: after `open` and 1 s of waiting, the log is empty.
  - `one_check_after_typing_stops`: 50 edits 20 ms apart give exactly one log line, starting at least 330 ms after the last edit. Then every recorded pid, including the `sleep 30` grandchild, fails `kill -0`.
  - `stale_check_killed`: with `sleep:5`, an edit 100 ms after a check starts kills it, the grandchild is gone, and its result is never published.
  - `runaway_check_times_out`: with `sleep:60` and `CHECK_TIMEOUT` overridden to 1 s through `SWARA_TEST_CHECK_TIMEOUT_MS`, one `Information` diagnostic says "LilyPond check took longer than 1 s and was stopped", and no process is left.
  - `check_errors_published`: with `stderr:tests/fixtures/lilypond/parse-error.stderr`, the diagnostics hold the parse error at the converted position with `source == "lilypond"`, merged with the instant layer.
  - `ily_gets_its_own_diagnostics`: `main.ly` includes `part.ily`, and the fake prints an error naming `part.ily`'s absolute path. A publish arrives for `part.ily`'s URI, and it clears after a clean check.
  - `auto_check_off`: with `autoCheck: false`, edits never run LilyPond.
  - `real_check_finds_seeded_errors` is `#[ignore]` and uses the real LilyPond. On `seeded.ly`, the four non-engraving errors in `seeded.expect` appear at their lines.
- [ ] **Run them to see them fail.** Run `cargo test -p swara --test lsp_check`. Expected: FAIL.
- [ ] **Implement `run.rs` and the check scheduling** in `server.rs`. Each edit resets the pause deadline, and a running check for an older version is stopped at once.
- [ ] **Run them to see them pass.** Run `cargo test -p swara --test lsp_check -- --include-ignored`. Expected: PASS.
- [ ] **Commit:** `feat(swara): run the LilyPond check when typing pauses`.

### Task: Command data from the installed LilyPond

**Files:**
- Create: `crates/swara/src/commanddata/mod.rs`, `crates/swara/src/commanddata/dump.ly`, `crates/swara/src/commanddata/cache.rs`
- Create: `crates/swara/tests/fixtures/commanddata/mini.jsonl` (hand-written, about 40 records)

**Interfaces:**
- Produces: `dump.ly`, Swara's own Scheme, embedded with `include_str!`, written to a temporary file and run as `LC_ALL=C lilypond -l ERROR <tmp>/dump.ly` with `SWARA_DUMP_OUT=<tmp>/out.jsonl`. It writes one JSON record per line:

```text
{"k":"meta","lilypond":"2.26.0","datadir":"/opt/homebrew/share/lilypond/2.26.0"}
{"k":"music-function","name":"relative","args":[{"pred":"ly:pitch?","optional":true},{"pred":"ly:music?","optional":false}],"doc":"..."}
{"k":"markup","name":"bold","args":[...],"doc":"..."}      also "markup-list"
{"k":"identifier","name":"fermata","kind":"music"}          kind: music | markup | number | string | context-def | scheme
{"k":"grob","name":"NoteHead","interfaces":[...],"doc":"..."}
{"k":"interface","name":"...","props":[...],"doc":"..."}
{"k":"property","name":"color","scope":"backend","pred":"color?","doc":"..."}   scope: backend | context
{"k":"context","name":"Staff","doc":"..."}
{"k":"translator","name":"...","doc":"..."}
{"k":"music-type","name":"..."} {"k":"event-class","name":"..."}
{"k":"language","name":"english","pitches":["c","cs",...]}  {"k":"drums","names":["bd","sn",...]}
```

- Produces: `pub struct CommandData` with lookup methods `music_function(&str)`, `markup(&str)`, `identifier(&str)`, `grob(&str)`, `property(&str, Scope)`, `context(&str)`, `pitches(&str) -> Option<&HashSet<String>>`, `drums()`, and `fn is_known_command(&self, name: &str, in_markup: bool) -> bool`. Also `pub enum ArgKind { Pitch, Duration, Music, Word, Markup, Number, Scheme }` and `fn arg_kind(pred: &str) -> ArgKind`.
- Produces: `cache::path(lilypond_canonical: &Path, version: &str) -> PathBuf`, which is `<cache dir>/swara/commands-<16 hex digits of FNV-1a 64 over "version\0path">.jsonl`. The cache dir is `~/Library/Caches` on macOS, `$XDG_CACHE_HOME` or `~/.cache` on Linux, and `%LOCALAPPDATA%` on Windows.
- Behaviour: at start the server reads the cache if present. Otherwise it runs the dump on a helper thread and loads the result when the dump finishes. Until then, `CommandData` is absent and every check that needs it stays off.

- [ ] **Write the failing tests.**
  - `parses_mini_fixture` checks counts per kind and `arg_kind("ly:pitch?") == Pitch`, `"string?"` gives `Word` and `"markup?"` gives `Markup`.
  - `cache_key_differs_by_version` compares the same path at 2.24.4 and 2.26.0.
  - `no_lilypond_degrades` starts the server with `lilypond` pointing at a missing path. Syntax diagnostics and formatting still work, and exactly one `window/showMessage` reads "LilyPond not found: the LilyPond check, builds, preview and playback are off".
  - `real_dump_counts` is `#[ignore]` and runs the real dump. It asserts at least 190 music functions, 190 markup commands, 160 grobs, 40 contexts and 270 context properties (the measured counts, rounded down), a `language` record for `english` holding `cs`, `identifier` records for `fermata`, `f` and `major`, and a `meta.datadir` that exists.
- [ ] **Run them to see them fail.** Run `cargo test -p swara commanddata`. Expected: FAIL.
- [ ] **Write `dump.ly`.** Find each binding first in a scratch `.ly` file and probe its LilyPond 2.26 name before relying on it: the music-function signature accessor, the markup command list, `all-grob-descriptions`, `all-interfaces`, the property docstrings, the context and translator lists, `(@@ (lily) all-event-classes)`, the per-language pitch tables, the drum pitch names, the data folder, and the bindings in the parser's current module for `identifier` records.
- [ ] **Implement the loader and the cache.**
- [ ] **Run them to see them pass.** Run `cargo test -p swara commanddata -- --include-ignored`. Expected: PASS.
- [ ] **Commit:** `feat(swara): read and cache LilyPond's own command data`.

### Task: Include index, names checks and argument binding

**Files:**
- Create: `crates/swara/src/index.rs`, `crates/swara/src/arity.rs`, `crates/swara/src/diagnostics/names.rs`, `crates/swara/src/diagnostics/includes.rs`

**Interfaces:**
- Produces: `pub struct Index`, with `fn update(&mut self, path: &Path, doc: &Document)`, `fn definitions_visible_from(&self, root: &Path) -> Vec<Definition>` (`Definition { name, path, range }`), and `fn resolve_include(&self, from: &Path, text: &str) -> Option<PathBuf>`. Resolution searches the including file's folder, then `includePaths`, then `<datadir>/ly`. Included files that are not open are read from disk and re-read when their modification time changes, checked on use. Recursion stops at any path already on the current chain.
- Produces: `pub fn bind_args(cmd: &str, siblings: &[Node], data: &CommandData) -> Vec<(Node, ArgKind)>`. It consumes siblings in order against the signature, skips an optional argument whose kind does not match, and stops at the first required mismatch.
- Produces: `pub fn name_checks(doc, data, index, enc) -> Vec<Diagnostic>`.
  - An unknown command gives "unknown command `\foo`". Known means grammar keywords, `data.is_known_command`, and definitions visible from the file.
  - An invalid pitch gives "`cis` is not a note name in english". It applies to `note` nodes that are not bound as `Word` arguments. The active language starts as nederlands, a top-level `\language` changes it, it is undone at the end of `\notemode { }`, it is ignored inside chord mode and paper, layout and midi blocks, and drum mode checks against the drum names.
  - `includes.rs` gives "cannot find `x.ily`".
  - All of these return nothing while `CommandData` is absent.

- [ ] **Write the failing tests,** using `mini.jsonl`:
  - `unknown_command_flagged` and `defined_variable_not_flagged`: `melody = { c }` then `\melody` is fine, and `\melodyy` is flagged.
  - `variable_from_include_not_flagged` and `include_cycle_terminates`: `a.ily` includes `b.ily`, which includes `a.ily`, and indexing finishes with both files indexed.
  - `clef_word_not_a_pitch`: `\clef treble` gives no pitch error.
  - `english_pitch_rules`: under `\language "english"`, `cs` is fine and `cis` is flagged. Inside `\notemode { \language "nederlands" cis }` it is fine, and after the block `cis` is flagged again.
  - `chordmode_ignores_language` and `drum_names_checked`.
  - `missing_include_flagged` and `datadir_include_found`: `\include "english.ly"` resolves through `meta.datadir`, and the test uses a temporary datadir.
  - `bengali_folder_include`: an include under `আমার গান/` resolves.
  - `nothing_before_data_loads`.
- [ ] **Run them to see them fail.** Run `cargo test -p swara names index arity`. Expected: FAIL.
- [ ] **Implement** `index.rs`, `arity.rs`, `names.rs` and `includes.rs`, and add them to the `instant` layer.
- [ ] **Run them to see them pass.** Run `cargo test -p swara`. Expected: PASS.
- [ ] **Commit:** `feat(swara): check commands, note names and includes`.

### Task: Completion and hover

**Files:**
- Create: `crates/swara/src/completion.rs`, `crates/swara/src/hover.rs`

**Interfaces:**
- Produces: `pub fn complete(doc, pos_byte, data, index) -> Vec<CompletionItem>`. Each item's `textEdit` replaces from the `\` (or from the word start) to the cursor. `detail` holds the argument kinds from the signature, such as `pitch? music`. The context decides the list:

| Where | Items |
| --- | --- |
| after `\` in music or at top level | music functions, music identifiers, visible variables, grammar keywords |
| after `\` inside `markup` | markup and markup-list commands |
| after `\new ` / `\context ` | contexts |
| after `\set ` / `\unset ` | contexts, then context properties after `.` |
| after `\override ` / `\revert ` | contexts and grobs, then grob properties from the grob's interfaces after `.` |
| a word in note mode | pitch names for the active language, or drum names in drum mode |

- Produces: `pub fn hover(doc, pos_byte, data) -> Option<Hover>`, Markdown holding the name, the signature and LilyPond's description from the cache, for commands, markup commands, grobs, properties and contexts.

- [ ] **Write the failing tests,** using `mini.jsonl`:
  - `backslash_offers_functions_and_variables` (`\rel` offers `relative` with detail `pitch? music`, plus the file's `melody`);
  - `markup_offers_markup_commands`, `new_offers_contexts`;
  - `override_path` (`\override Staff.Not` offers `NoteHead`, and `\override NoteHead.co` offers `color`);
  - `set_path`, `pitches_follow_language`;
  - `hover_shows_doc`, `hover_on_unknown_is_none`.
- [ ] **Run them to see them fail.** Run `cargo test -p swara completion hover`. Expected: FAIL.
- [ ] **Implement `complete` and `hover`.**
- [ ] **Run them to see them pass.** Run `cargo test -p swara`. Expected: PASS.
- [ ] **Commit:** `feat(swara): complete and describe LilyPond names`.

### Task: Definitions and the outline

**Files:**
- Create: `crates/swara/src/definition.rs`, `crates/swara/src/symbols.rs`

**Interfaces:**
- Produces: `pub fn definition(doc, pos_byte, index) -> Option<Location>`, from a `command` whose name without `\` matches a visible definition.
- Produces: `pub fn document_symbols(doc) -> Vec<DocumentSymbol>`. Assignments are `Variable`. `score_block`, `book_block` and `bookpart_block` are `Module` with nested children, named by the block keyword plus its `\header` `title` or `piece` when present.

- [ ] **Write the failing tests:** `goes_to_same_file_assignment`, `goes_to_included_assignment`, `string_named_variable` (`"my tune" = { }` with `\"my tune"`), and `outline_lists_variables_and_scores`.
- [ ] **Run them to see them fail.** Run `cargo test -p swara definition symbols`. Expected: FAIL.
- [ ] **Implement `definition` and `document_symbols`.**
- [ ] **Run them to see them pass.** Run `cargo test -p swara`. Expected: PASS.
- [ ] **Commit:** `feat(swara): add go-to-definition and the outline`.

### Task: The formatter

**Files:**
- Create: `crates/swara/src/format.rs`, `crates/swara/tests/format.rs`

**Interfaces:**
- Produces: `pub enum FormatOutcome { Unchanged, Changed(String), Refused(&'static str) }` and `pub fn format(text: &str, indent: usize, parser: &mut Parser) -> FormatOutcome`. The rules, applied line by line without adding or removing line breaks:
  - Indent each line by `indent` times the number of enclosing `{ }`, `<< >>`, block and `markup_block` nodes, where a line starting with a closer counts one fewer.
  - Collapse runs of spaces and tabs between two tokens on one line to one space. Add one space on each side of `|`, after `{` and `<<`, and before `}` and `>>` on the same line. Remove trailing whitespace.
  - Never insert a space between tokens that touch elsewhere, because `c4-.`, `c'4~` and `a--b` belong together.
  - Leave strings, comments, lyric words, markup words and the inside of Scheme as they are. A multi-line Scheme datum moves as a block by its first line's change in indent.
  - Before returning `Changed`, check that the sequence of non-whitespace leaf texts is identical and that the error count did not rise. Refuse a file that already has errors.
- Produces: LSP formatting that returns one `TextEdit` per changed line. `swara format --indent N` reads stdin and writes stdout. A refusal prints the input unchanged, writes the reason to stderr and exits 0, so conform never blanks a broken buffer.

- [ ] **Write the failing tests** in `tests/format.rs`:
  - `indents_blocks`: `\score {\n{\nc4  d |e\n}\n}` gives `\score {\n  {\n    c4 d | e\n  }\n}`.
  - `keeps_attached_tokens` (`c4-. d'8~ d`), `keeps_line_breaks`, `keeps_comments_and_strings` (`"a  b"` and `%  x`).
  - `scheme_moves_as_block`, `with_block_indents`.
  - `refuses_on_syntax_error` (`{ c` gives `Refused`), `idempotent` (formatting twice gives `Unchanged`).
  - `token_guard_catches_change`: the guard rejects a forced output whose tokens differ.
  - `cli_refusal_passes_input_through`.
  - `every_corpus_case_survives` runs `format` on every accepted file under `bench/corpus/accepted` when it exists, and asserts the result is never `Changed` with a different token sequence.
- [ ] **Run them to see them fail.** Run `cargo test -p swara --test format`. Expected: FAIL.
- [ ] **Implement `format`,** the LSP handler and the CLI.
- [ ] **Run them to see them pass.** Run `cargo test -p swara --test format`. Expected: PASS.
- [ ] **Commit:** `feat(swara): tidy spacing without changing music`.

### Task: The save build, preview and engraving warnings

**Files:**
- Create: `crates/swara/src/build.rs`, `crates/swara/tests/lsp_build.rs`

**Interfaces:**
- Produces: on `didSave` of a `.ly` file, and on `workspace/executeCommand` `swara.build` with `[uri]` (which uses the unsaved text on stdin), a build: `LC_ALL=C lilypond -l WARNING -I <abs dir> -o <abs dir>/.swara-build-<id>/<stem> <abs file or ->`. On exit, every output file moves into `<abs dir>` with `rename`, which is atomic on one filesystem, and the temporary folder is removed. A newer build for the same file stops the older one first.
- Produces: the notification `swara/buildFinished` with params `{"uri": string, "pdf": string|null, "midi": string|null, "success": bool, "durationMs": number}`. `midi` is whichever of `<stem>.midi` and `<stem>.mid` was written.
- Produces: `swara.preview` with `[uri, dpi]`, which runs `--png -dcrop -dresolution=<dpi>` into a temporary folder and returns `{"png": "<tmp>/<stem>.cropped.png"}`.
- Behaviour: warnings and errors from a build replace the file's `build` layer. `Layers::shift` keeps them on their lines through later edits until the next build.

- [ ] **Write the failing tests** in `lsp_build.rs`:
  - `save_builds_next_to_source` is `#[ignore]` and uses the real LilyPond. Saving a 4-bar lesson gives `buildFinished` with `success`, and the PDF and MIDI exist beside the source, with no `.swara-build-*` folder left.
  - `bengali_and_spaces_in_path` repeats that under `আমার গান/lesson 1.ly`.
  - `second_save_supersedes_first` uses the fake with `sleep:3`. Two saves 100 ms apart give exactly one `buildFinished`, for the second, and the first build's processes are gone.
  - `pdf_replaced_atomically` uses the real LilyPond. A watcher thread polling the PDF every 5 ms during ten rebuilds never sees a file that fails to start with `%PDF-` and end with `%%EOF`.
  - `engraving_warning_kept_and_moved` uses the real LilyPond on `tab-no-string.ly`. The warning appears after the save, moves down two lines after a two-line insert above it, and is cleared by the next clean build.
  - `build_command_uses_unsaved_text` and `preview_returns_png` (both real, ignored).
  - `no_build_on_open`.
- [ ] **Run them to see them fail.** Run `cargo test -p swara --test lsp_build -- --include-ignored`. Expected: FAIL.
- [ ] **Implement `build.rs`.**
- [ ] **Run them to see them pass.** Run `cargo test -p swara --test lsp_build -- --include-ignored`. Expected: PASS.
- [ ] **Commit:** `feat(swara): build on save and report to the editor`.

### Task: `swara play` (only if the playback spike chose streaming)

**Files:**
- Create: `crates/swara/src/play.rs` (moving `smf.rs` from `xtask`, with its tests)
- Modify: `xtask/src/spike_play.rs` (delete)

**Interfaces:**
- Consumes: `read_smf`, `TimedEvent`, `Event` and `shell_line` from the foundations plan.
- Produces: `swara play --port P FILE`, which connects to `127.0.0.1:P`, writes each event's shell line at its time measured from a monotonic start, writes `reset` after the last event, and exits 0. It exits 3 when the port refuses. Stopping is the plugin's job: it kills the process and sends `reset` itself.

- [ ] **Move the tests,** then add `connection_refused_exits_3` and `streams_in_order_on_time`. The second uses a local `TcpListener` in the test to check that events arrive in order, that the last is `reset`, and that each arrives no earlier than its time.
- [ ] **Run them to see them fail.** Run `cargo test -p swara play`. Expected: FAIL.
- [ ] **Implement `play.rs`.**
- [ ] **Run them to see them pass.** Run `cargo test -p swara play`. Expected: PASS.
- [ ] **Commit:** `feat(swara): stream MIDI to a running FluidSynth`.

### Task: The server speed suite

**Files:**
- Create: `crates/swara/tests/speed.rs`, `docs/bench/speed.md`

**Interfaces:**
- Consumes: `TestClient`, `bench/song-100.ly` and the real LilyPond.

- [ ] **Write the timing tests,** each `#[ignore]` and run in release, each the median of 20 runs after a warm-up:
  - `ready_under_100ms`: spawn to the `initialize` reply, with the cache present.
  - `instant_under_50ms`: a one-character edit to the publish of instant diagnostics on the song.
  - `check_under_600ms`: from the last edit to the `lilypond` diagnostics, with the 330 ms pause included.
  - `tidy_under_10ms`: both LSP formatting and `swara format` on the song.
  - `completion_under_20ms`: after `\`.
  - `pdf_after_save`: the 4-bar lesson around 0.3 s and the song around 1.5 s. This test reports the times and fails only above 1.5 times the target.
- [ ] **Run them.** Run `cargo test -p swara --release --test speed -- --include-ignored --nocapture`. Expected: PASS. Any miss is recorded, not hidden.
- [ ] **Record** every median against its target in `docs/bench/speed.md`, then run the prose gate.
- [ ] **Commit:** `test(swara): measure the server against the speed targets`.
