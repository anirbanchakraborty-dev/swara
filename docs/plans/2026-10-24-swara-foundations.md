# Swara foundations and spikes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the repositories with their guard hooks, and settle the measured unknowns the later plans branch on: how the LilyPond check skips engraving and reads unsaved text, how a running FluidSynth plays a new file, how Skim picks up a rebuilt PDF, and how the existing grammar performs on the comparison corpus.

**Architecture:** The `swara` repository starts as a Cargo workspace with one member, `xtask`, a development-only binary that generates the 100-bar test song, fetches and filters the corpus, scores grammars, and later syncs queries and runs the speed suite. Spikes are throwaway; each leaves a results note under `docs/spikes/`, the fixtures later plans test against, and, for the LilyPond check, the settings file the server embeds.

**Tech Stack:** Rust 1.98 (xtask; `libloading` for loading grammars at run time), LilyPond 2.26.0, FluidSynth 2.6.1, Skim, tree-sitter CLI 0.27.1, hyperfine 2.0.0.

**Spec:** `docs/design/2026-10-24-swara-design.md` in the `swara` repository.

**Plan order:** This plan runs first. Then `2026-10-24-swara-grammar.md`, then `2026-10-24-swara-server.md`, then `2026-10-24-swara-plugin.md`. Server work may start once the grammar plan's node contract is committed, using the grammar as it stands.

## Global Constraints

- MIT for every Swara project. No code, query, word list or docstring copied from LilyPond, its bundled Vim files, nvim-lilypond-suite, Frescobaldi or python-ly, and nothing copied from Avro Keyboard. Facts about the language may be used.
- LilyPond 2.26.0 is the tested version. Neovim 0.12 is the minimum. Tree-sitter ABI 15, from the `tree-sitter` 0.27 CLI.
- Every LilyPond run sets `LC_ALL=C`, passes `-l WARNING`, uses absolute paths, and puts options before the file name.
- Opening a file never runs LilyPond.
- LilyPond's docstrings stay in the user's cache folder: never committed, bundled or uploaded. The corpus folder is git-ignored and only results are committed.
- No Claude or Anthropic credit anywhere. Every repository gets the `commit-msg` hook from `Utilities/no-claude-credit.mjs`, and `--no-verify` is never used.
- Commits use Conventional Commits with a scope and `git commit --only <paths>`. One logical change per commit.
- Documentation has no numbered headings or numbered lists, is written impersonally, and is not hard-wrapped. Every prose file gets `clean_text.py --in-place` (never `--strip-emoji-glue`, which breaks Bengali joiners), then `node Utilities/sweep-baks.mjs`.
- Each third-party crate is reviewed (licence, maintenance, what it pulls in) and recorded in `docs/dependencies.md` before it is added.
- Downloads need the owner's yes first, naming the file, its source and its size. Public actions, such as creating a GitHub repository, need the owner's yes first, naming the repository, its visibility and its licence.
- Any wait longer than about two minutes runs in the background with a `Monitor` that prints one changing line a minute.

## Review Focus

- **Corpus files that are not valid UTF-8** (older Mutopia pieces are Latin-1). They must be copied, classified and parsed as bytes, never skipped silently. The test lives in the corpus task.
- **Corpus files that include neighbours** (`\include "../common.ily"`). Acceptance must reflect the file's real include context, so LilyPond runs from the file's own folder. The test lives in the corpus task.
- **Corpus files that hang or run for minutes.** A per-file timeout classifies them as `timeout`, reported apart from the rest. The test lives in the corpus task.
- **A busy FluidSynth port**, such as 9800 already held by another FluidSynth. The port strategy must still start a player. The test lives in the playback spike.
- **Typing a character that LilyPond counts differently from bytes**, a tab or a Bengali letter before an error. Recorded fixtures must cover both, because the server converts columns from them. The test lives in the LilyPond check spike.

---

### Task: Repositories, licence and the credit hook

**Files:**
- Create: `Projects/swara/LICENSE`, `Projects/swara/.gitignore`, `Projects/swara/README.md` (one paragraph: what Swara is, status "in development")
- Create: `Projects/swara/docs/design/2026-10-24-swara-design.md` (moved from the shared notes folder)
- Create: `Projects/swara/docs/plans/2026-10-24-swara-{foundations,grammar,server,plugin}.md` (these four plans)
- Create: `Projects/tree-sitter-lilypond/LICENSE`, `Projects/tree-sitter-lilypond/.gitignore`
- Modify (hub, after remotes exist): `.gitmodules`

**Interfaces:**
- Produces: two local repositories on branch `main`, each with the credit hook, later registered as hub submodules at `Projects/swara` and `Projects/tree-sitter-lilypond`.

- [ ] **Confirm the public repositories with the owner.** Ask, in one message: `anirbanchakraborty-dev/swara` and `anirbanchakraborty-dev/tree-sitter-lilypond`, both public, both MIT. The Homebrew tap is asked about separately in the plugin plan. Wait for a yes.
- [ ] **Initialise both repositories.** Run `git init -b main` in each. The LICENSE is MIT, `Copyright (c) 2026 Anirban Chakraborty`. `.gitignore` for `swara`: `/target`, `/bench/corpus/`, `/bench/third-party/`, `/.cargo/local.toml`, `*.bak`, `.DS_Store`. For the grammar: `/target`, `/build`, `*.dylib`, `*.so`, `*.dll`, `node_modules`, `*.bak`, `.DS_Store`.
- [ ] **Install the credit hook.** From the hub root, for each absolute repository path: `node -e "import('./Utilities/no-claude-credit.mjs').then(m=>console.log(m.installHooks('<abs path>')))"`.
- [ ] **Prove the hook refuses a credit line.** In `Projects/swara`, run `git commit --allow-empty -m "test" -m "Co-Authored-By: Claude <noreply@anthropic.com>"`. Expected: a non-zero exit with the hook's refusal message, and `git log` still empty. Repeat in the grammar repository.
- [ ] **Move the design and plans, then run the prose gate on them.** Run `python3 .claude/skills/remove-ai-marks/scripts/clean_text.py --in-place <file>` for each file, then `node Utilities/sweep-baks.mjs`. Delete `.bak` files outside the hub only after `cmp -s` confirms each is identical to its original.
- [ ] **Commit.** In `swara`: `git add LICENSE .gitignore README.md docs && git commit --only LICENSE .gitignore README.md docs -m "docs: add design, build plans and licence"`. In the grammar repository: `chore: add licence and ignore rules`.
- [ ] **Create the remotes and push.** Run `gh repo create anirbanchakraborty-dev/swara --public --source Projects/swara --remote origin --push`, and the same for `tree-sitter-lilypond`.
- [ ] **Register both as hub submodules.** From the worktree root: `git submodule add https://github.com/anirbanchakraborty-dev/swara Projects/swara` (git adopts the existing repository at that path), the same for the grammar, then `git commit --only .gitmodules Projects/swara Projects/tree-sitter-lilypond -m "chore(swara): register swara and tree-sitter-lilypond submodules"`. Expected: `node Utilities/no-claude-credit.mjs --check` exits 0.

### Task: Workspace and the 100-bar test song

**Files:**
- Create: `Projects/swara/Cargo.toml` (workspace, `resolver = "3"`, `members = ["xtask"]`)
- Create: `Projects/swara/.cargo/config.toml` (`[alias] xtask = "run --quiet -p xtask --"`)
- Create: `Projects/swara/xtask/Cargo.toml`, `xtask/src/main.rs`, `xtask/src/gen_song.rs`
- Create: `Projects/swara/bench/song-100.ly` (generated, committed)
- Create: `Projects/swara/docs/spikes/speed-basis.md`

**Interfaces:**
- Produces: `pub fn generate_song(bars: u32) -> String` in `xtask/src/gen_song.rs`, and `cargo xtask gen-song --bars <n>`, which prints to stdout. `bench/song-100.ly` is the 100-bar song every later speed test uses.
- Produces: `.cargo/local.toml` (git-ignored) as the place for `[patch."https://github.com/anirbanchakraborty-dev/tree-sitter-lilypond"] tree-sitter-lilypond = { path = "../tree-sitter-lilypond" }`, used as `cargo --config .cargo/local.toml <cmd>` during local grammar work.

- [ ] **Write the failing tests** in `xtask/src/gen_song.rs`:

```rust
#[test]
fn song_has_requested_bars_and_parts() {
    let s = generate_song(100);
    assert!(s.starts_with("\\version \"2.26.0\""));
    assert!(s.contains("\\language \"english\""));
    assert_eq!(s.matches(" |").count() >= 100, true);
    for part in ["\\new Staff", "\\new TabStaff", "\\new NoteNames", "\\lyricsto", "\\layout", "\\midi", "treble_8"] {
        assert!(s.contains(part), "missing {part}");
    }
    assert_eq!(generate_song(100), generate_song(100)); // deterministic
}

#[test]
fn song_compiles_cleanly_when_lilypond_present() {
    // Skips with a printed note when `lilypond` is not on PATH.
    // Runs `LC_ALL=C lilypond -l WARNING -o <tmp>/song <tmp>/song.ly` and asserts exit 0 and empty stderr.
}
```

- [ ] **Run them to see them fail.** Run `cargo test -p xtask gen_song`. Expected: compile failure, `generate_song` not found.
- [ ] **Implement `generate_song(bars: u32) -> String`.** It writes a melody in 4/4, C major, cycling a fixed 16-note pattern inside guitar range. It has a `\new Staff` with `\clef "treble_8"`, a `\new TabStaff` with `\set TabStaff.stringTunings = \stringTuning <e, a, d g b e'>`, two `\new NoteNames` lines and lyrics attached with `\lyricsto`. It ends with `\layout { }` and `\midi { }`, and uses no randomness.
- [ ] **Run the tests to see them pass.** Run `cargo test -p xtask gen_song`. Expected: both PASS.
- [ ] **Check the generated song weighs the same as the measured one.** Run `cargo xtask gen-song --bars 100 > bench/song-100.ly && hyperfine -w 1 -r 5 'LC_ALL=C lilypond -l WARNING -o /tmp/s bench/song-100.ly'`. Expected: a median within ±25% of 1,493 ms. Record the result in `docs/spikes/speed-basis.md`.
- [ ] **Commit:** `feat(xtask): generate the 100-bar test song`.

### Task: LilyPond check spike

**Files:**
- Create: `Projects/swara/spikes/lilypond-check/` (throwaway: `candidate-a.ly`, `candidate-b.ly`, `run.sh`)
- Create: `Projects/swara/assets/lilypond/check-settings.ly` (only if a candidate wins)
- Create: `Projects/swara/tests/fixtures/lilypond/*.ly` and the matching `*.stderr` recordings
- Create: `Projects/swara/docs/spikes/lilypond-check.md`

**Interfaces:**
- Produces: the check command the server runs, recorded in `docs/spikes/lilypond-check.md` as one exact argument list. When a candidate wins it is `LC_ALL=C lilypond -l WARNING -dinclude-settings=<abs>/check-settings.ly -I <abs dir> -o <abs tmp>/<stem> -`, with the text on stdin. Otherwise `-dno-print-pages` takes the place of `-dinclude-settings`.
- Produces: recorded stderr fixtures, each named `<case>.stderr` beside `<case>.ly`, which the server's message-parser tests read: `parse-error`, `unknown-command`, `barcheck`, `no-version`, `guile-error`, `reader-error`, `no-location`, `include-error`, `stdin-include`, `tab-column`, `utf8-column`, `tab-no-string` (full build only), and `programming-error` if it can be reproduced.

- [ ] **Write the seeded fixture.** Copy `bench/song-100.ly` to `tests/fixtures/lilypond/seeded.ly` and plant exactly five problems at known lines: an extra `}`, an unknown command `\fooBar`, a bar-check failure, the Scheme error `#(car '())`, and a tablature note below the low string (`e,,4` in the tab staff's music), which only engraving reports. Record them in `seeded.expect`, one per line as `LINE:error|warning:substring`.
- [ ] **Record the full build.** Run `LC_ALL=C lilypond -l WARNING -o <abs tmp>/seeded <abs>/seeded.ly 2> seeded.full.stderr`. Expected: all five messages at the lines in `seeded.expect`.
- [ ] **Try candidate A.** `candidate-a.ly` redefines the top-level book and score handlers so that each score is interpreted with `ly:run-translator` against a MIDI-only output definition and never engraved. Run it with `-dinclude-settings`. Expected: the four messages that do not need engraving appear at the same lines, no PDF is written, and the exit status is 1.
- [ ] **Try candidate B.** `candidate-b.ly` overrides the score handler so that it interprets the music under its own layout definition with every engraver removed. Same expectations as candidate A.
- [ ] **Time both, plus the fallback.** Run `hyperfine -w 1 -r 5` on `bench/song-100.ly` for candidate A, candidate B and `-dno-print-pages`, and record the medians.
- [ ] **Decide.** Choose the fastest candidate that reports all four messages at the right lines and has a median of 0.27 s or less: the 0.6 s target minus the 0.33 s pause. Copy it to `assets/lilypond/check-settings.ly`. If neither candidate qualifies, record the `-dno-print-pages` fallback and its time as a missed target.
- [ ] **Check unsaved text and includes.** In a temporary folder, put `main.ly` (which does `\include "part.ily"`) and a `part.ily` holding one planted error. Feed `main.ly` on stdin with `-` and `-I <abs folder>`. Expected: the include resolves with no "cannot find file", lines in the main file are named `-`, and lines in the include are named by its absolute path. Save the output as `stdin-include.stderr`. If the include does not resolve, record the temporary-file fallback and capture with it instead.
- [ ] **Record the message fixtures.** Write one tiny `.ly` per case listed under Interfaces, each with a single problem, and capture its stderr with the chosen check command. For `tab-no-string`, use the full build. `tab-column` puts a tab before the error column; `utf8-column` puts `"গান"` before it on the same line. Mark any case that cannot be reproduced as synthetic in `lilypond-check.md`.
- [ ] **Write `docs/spikes/lilypond-check.md`.** It records the candidates, the medians, the verdict, the exact command and the include result.
- [ ] **Commit:** `test(lilypond): record check spike results and stderr fixtures`.

### Task: Warm playback spike

FluidSynth 2.6.1's shell has no command that loads a MIDI file into a running player: `help all` on 2026-10-24 listed `player_start`, `player_stop`, `player_cont`, `player_seek`, `player_next`, `player_loop` and two tempo commands, and nothing that loads. It does take live events: `noteon chan key vel`, `noteoff chan key`, `cc chan ctrl value`, `prog chan num`, `pitch_bend chan value` and `reset`. The only warm route is therefore to stream a file's events into a running FluidSynth.

**Files:**
- Create: `Projects/swara/xtask/src/smf.rs` (a minimal Standard MIDI File reader, formats 0 and 1)
- Create: `Projects/swara/xtask/src/spike_play.rs`
- Create: `Projects/swara/docs/spikes/playback.md`

**Interfaces:**
- Produces: `pub fn read_smf(bytes: &[u8]) -> Result<Vec<TimedEvent>, SmfError>`, with `pub struct TimedEvent { pub at: std::time::Duration, pub event: Event }` and `pub enum Event { NoteOn { ch: u8, key: u8, vel: u8 }, NoteOff { ch: u8, key: u8 }, Control { ch: u8, ctrl: u8, value: u8 }, Program { ch: u8, num: u8 }, PitchBend { ch: u8, value: u16 } }`. Also `pub fn shell_line(e: &Event) -> String`. The server plan moves both into `crates/swara` if streaming wins.
- Produces: `cargo xtask spike-play --port <p> <file>`, and the verdict "stream" or "fresh process" in `docs/spikes/playback.md`.

- [ ] **Write the failing tests** in `smf.rs`: `reads_tempo_and_notes` builds a format-0 file in the test (tempo 500000 µs per quarter, a 480-tick division, note 60 on at tick 0 and off at tick 480). It asserts two events, `NoteOn{ch:0,key:60,vel:100}` at 0 ms and `NoteOff{ch:0,key:60}` at 500 ms. `note_on_zero_velocity_is_note_off` and `shell_line_formats` assert `shell_line(&Event::NoteOn{ch:0,key:60,vel:100}) == "noteon 0 60 100"`.
- [ ] **Run them to see them fail.** Run `cargo test -p xtask smf`. Expected: compile failure.
- [ ] **Implement `read_smf` and `shell_line`.** Merge the tracks by absolute tick and apply tempo changes in order.
- [ ] **Run them to see them pass.** Run `cargo test -p xtask smf`. Expected: PASS.
- [ ] **Measure the cold start.** Run `fluidsynth -s -i -o shell.port=<p> -o synth.dynamic-sample-loading=1 "/Applications/MuseScore 4.app/Contents/Resources/sound/MS Basic.sf3"` and time how long until the TCP port accepts a connection. Expected: about 1.1 s.
- [ ] **Test a busy port.** Hold 9800 with `nc -l 9800`. The strategy under test: try ports from `9800 + (pid % 200)` upward, and treat a refused connection after 2 s, or a bind error on FluidSynth's stderr, as busy before moving to the next. Expected: a player starts on another port. Record the strategy.
- [ ] **Stream the song.** Build `bench/song-100.ly` to MIDI, then run `cargo xtask spike-play --port <p> <midi>` against the warm FluidSynth. Expected: the first `noteon` is written within 0.2 s of the command, and p99 lateness across all events is under 5 ms, both printed by the spike.
- [ ] **Check the result offline.** Run a second FluidSynth with `-a file -o audio.file.name=<tmp>/out.wav`, stream into it, and compare the non-silent length of `out.wav` with the MIDI's length. Expected: within ±2%. Then send `reset` partway through. Expected: silence within 50 ms of the reset.
- [ ] **Decide and record** "stream" or "fresh process" in `docs/spikes/playback.md`, with the numbers.
- [ ] **Commit:** `test(xtask): measure warm FluidSynth playback by event streaming`.

### Task: Skim refresh spike

Skim is installed, and on 2026-10-24 `defaults read -app Skim` showed `SKAutoCheckFileUpdate = 1` and `SKAutoReloadFileUpdate = 1`. The spike does not change those preferences.

**Files:**
- Create: `Projects/swara/docs/spikes/skim.md`

**Interfaces:**
- Produces: the refresh route the plugin's viewer uses, either "Skim watches the file" or "the plugin asks Skim to revert", with its latency. Also the one-line command the health check reads: `defaults read -app Skim SKAutoReloadFileUpdate`.

- [ ] **Check focus on open.** Record the frontmost app with `osascript -e 'tell application "System Events" to name of first process whose frontmost is true'`, run `open -g -a Skim <abs pdf>`, and read the frontmost app again. Expected: unchanged.
- [ ] **Check the watch route.** Rebuild the same PDF with enough extra bars to add a page. Within 1 s, `osascript -e 'tell application "Skim" to count pages of document 1'` should return the new count, and the frontmost app should be unchanged.
- [ ] **Check the revert route.** Rebuild again, then run `osascript -e 'tell application "Skim" to revert document 1'`. Measure how long it takes and check that the frontmost app is unchanged.
- [ ] **Record the chosen route** (watch first, revert as the fallback when the preference is off) with both latencies in `docs/spikes/skim.md`.
- [ ] **Commit:** `docs(spikes): record how Skim picks up a rebuilt PDF`.

### Task: Corpus fetch and the LilyPond filter

**Files:**
- Create: `Projects/swara/xtask/src/corpus.rs`
- Create: `Projects/swara/bench/mutopia.txt` (a pinned list of raw URLs at one commit of `MutopiaProject/MutopiaProject`, about 60 pieces across eras)
- Create: `Projects/swara/bench/results/corpus-manifest.json` (committed: path, SHA-256, size, source and class for every file, with no content)

**Interfaces:**
- Produces: `cargo xtask corpus fetch` and `cargo xtask corpus filter [--jobs N] [--timeout-secs 60]`.
- Produces: `bench/corpus/accepted/<source>/…`, `bench/corpus/rejected/<source>/…` and `bench/corpus/timeout/<source>/…`, where `<source>` is one of `regression`, `snippets`, `init`, `mutopia` or `lessons`.
- Produces: `pub enum Class { Accepted, Converted, Rejected, Timeout }` and `pub trait Runner { fn run(&self, file: &Path, cwd: &Path, timeout: Duration) -> Outcome; }`, so the logic is testable without LilyPond.

- [ ] **Ask the owner before downloading.** Name the files: `lilypond-2.26.0.tar.gz` from `https://lilypond.org/download/sources/v2.26/` (size read with `curl -sI` first; it supplies `input/regression/*.ly` and `Documentation/snippets/*.ly`), and the pieces in `bench/mutopia.txt` (count and total size stated). Local sources need no download: `/opt/homebrew/share/lilypond/2.26.0/ly/*.ly`, and the owner's lessons from `$SWARA_LESSONS_DIR` when set.
- [ ] **Write the failing tests** in `corpus.rs`, using a fake `Runner`:
  - `accepted_when_exit_zero`.
  - `converted_when_convert_ly_fixes_it`: the fake fails first and succeeds after conversion, and the original bytes are kept in the manifest's SHA-256.
  - `timeout_is_its_own_class`.
  - `non_utf8_file_is_classified_not_skipped`: the input holds the byte `0xE9`.
  - `runs_from_the_files_own_folder`: the fake asserts that `cwd` equals the file's parent.
- [ ] **Run them to see them fail.** Run `cargo test -p xtask corpus`. Expected: compile failure.
- [ ] **Implement fetch and filter.** Fetch shells out to `curl -fsSL` and `tar`. Filter runs the chosen check command from the LilyPond spike in the file's folder. On failure, it runs `convert-ly -e` on the local copy and retries. A pool of `std::thread`s sized by `--jobs` (default `available_parallelism`) does the work.
- [ ] **Run them to see them pass.** Run `cargo test -p xtask corpus`. Expected: PASS.
- [ ] **Run the real filter in the background,** with a `Monitor` printing the counts per class each minute. Expected: the manifest is written, and its summary of counts per source and class goes into `docs/bench/corpus.md`.
- [ ] **Commit:** `feat(xtask): fetch and filter the comparison corpus` (the manifest and doc only; `bench/corpus/` stays ignored).

### Task: Broken-file corpus

**Files:**
- Create: `Projects/swara/bench/broken/*.ly` (at least 30 files, written fresh, MIT)
- Modify: `Projects/swara/xtask/src/corpus.rs` (add `broken verify`)

**Interfaces:**
- Produces: each file starts `% expect-error-line: N` and then `% expect: <what is wrong>`. Also `cargo xtask broken verify`, which exits 0 only if LilyPond's first error in every file is on line `N`.

- [ ] **Write the files.** Each holds exactly one syntax error, spread across: unclosed `{`, extra `}`, unclosed `<<`, unclosed chord `<`, unterminated string, unterminated `%{`, unbalanced `#(`, `#{` without `#}`, `\override` missing `=`, `\new` with no context type, a stray `>>`, a stray `#}`, a lyric block left unclosed, a markup block left unclosed, a `\header` with a missing value, and errors inside `\paper`, `\layout`, chord mode and figure mode. The error sits at varied depths and lines.
- [ ] **Write the failing check.** Run `cargo xtask broken verify` before it is implemented. Expected: the subcommand is unknown.
- [ ] **Implement `broken verify`** with the check command, comparing the first `error:` line number with `N`.
- [ ] **Run it.** Expected: exit 0. Fix the `N` of any file where LilyPond places the error elsewhere, because LilyPond's line is the truth.
- [ ] **Commit:** `test(bench): add the broken-file corpus`.

### Task: Comparison harness and the existing grammar's baseline

**Files:**
- Create: `Projects/swara/xtask/src/compare.rs`, `xtask/src/dyn_grammar.rs`
- Create: `Projects/swara/bench/results/2026-10-24-baseline-nwhetsell.json`, `docs/bench/baseline.md`
- Modify: `Projects/swara/docs/dependencies.md` (add `libloading`, used by xtask only)

**Interfaces:**
- Produces: `cargo xtask compare --name <n> --lib <dylib> --symbol <sym> [--inject-lib <dylib> --inject-symbol <sym> --inject-query <injections.scm>] --out <json>`.
- Produces: JSON with the shape `{"grammar":..,"commit":..,"files":[{"path":..,"bytes":..,"error":bool,"first_error_line":u32|null,"parse_us":u64,"inject_parse_us":u64}],"broken":[{"path":..,"expected":u32,"caught":bool}],"summary":{"accepted_files":..,"files_with_error":..,"bytes_per_ms":..,"broken_caught":..,"broken_total":..}}`.
- Produces: `pub fn caught(first_error_line: Option<u32>, expected: u32) -> bool`, true when the line is within ±1.

- [ ] **Ask the owner before cloning.** Name both repositories: `nwhetsell/tree-sitter-lilypond` and `nwhetsell/tree-sitter-lilypond-scheme`, cloned into `bench/third-party/`, with sizes from `gh api repos/<r> --jq .size`. Record both HEAD commits.
- [ ] **Write the failing tests:** `caught_within_one_line` (`caught(Some(11),12)` and `caught(Some(13),12)` are true, `caught(Some(14),12)` and `caught(None,12)` are false), and `timing_takes_median_of_five_after_warmup`, which uses a fake parse function that returns fixed durations.
- [ ] **Run them to see them fail.** Run `cargo test -p xtask compare`. Expected: compile failure.
- [ ] **Implement the harness.** Load each grammar with `libloading` and wrap it in `tree_sitter::Language`. For grammars with injections, run the injection query on the primary tree and parse every `@injection.content` range with the injected grammar. A file has an error when any `ERROR` or `MISSING` node appears in its primary or injected trees. Time each file as the median of five parses after one warm-up.
- [ ] **Run them to see them pass.** Run `cargo test -p xtask compare`. Expected: PASS.
- [ ] **Build the baseline libraries.** Run `tree-sitter build -o bench/third-party/nw-lilypond.dylib bench/third-party/tree-sitter-lilypond`, and the same for the Scheme grammar.
- [ ] **Run the baseline** over `bench/corpus/accepted` and `bench/broken`, writing `bench/results/2026-10-24-baseline-nwhetsell.json`.
- [ ] **Confirm the detection node.** Check `bench/third-party/tree-sitter-lilypond/src/node-types.json` for `embedded_lilypond`. Expected: absent. The health check relies on that node existing only in Swara's grammar, so record the result.
- [ ] **Write `docs/bench/baseline.md`** with the summary numbers and both commits.
- [ ] **Commit:** `test(bench): measure the nwhetsell grammar baseline`.
