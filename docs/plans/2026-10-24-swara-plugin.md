# Swara plugin, lessons and release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Neovim plugin that loads Swara's grammar and server, opens and refreshes the PDF, previews and plays a score; the lesson templates with the sargam line; lyrics typed in English letters and printed in Bengali script with the typed line beneath, through an engine that takes other scripts as tables; and the first complete release, with archives for five platforms and a Homebrew tap.

**Architecture:** Pure Lua under `lua/swara/` at the root of the `swara` repository, so a plugin manager installs the repository as it stands. The plugin talks to the server through Neovim's built-in LSP client and to everything else (Skim, FluidSynth, MuseScore, `curl`, `tar`) through `vim.system`. The include files are plain LilyPond and Scheme, so a lesson folder builds with `lilypond` alone.

**Tech Stack:** Neovim 0.12 Lua API, LilyPond 2.26.0 Scheme (Guile 3.0), GitHub Actions, Homebrew.

**Spec:** `docs/design/2026-10-24-swara-design.md`, sections "The Neovim plugin", "Lessons", "Release plan" and "Installing".

**Plan order:** Last. It needs `swara info`, `swara/buildFinished`, `swara.build`, `swara.preview` and, if streaming won, `swara play` from `2026-10-24-swara-server.md`, and the query files from `2026-10-24-swara-grammar.md`.

## Global Constraints

Everything under Global Constraints in the foundations plan applies. In addition:

- Neovim 0.12 is the floor. snacks.nvim, conform.nvim and nvim-treesitter are optional, and the plugin must work without any of them.
- Nothing is copied from nvim-lilypond-suite, LilyPond's own editor files, Frescobaldi or Avro Keyboard.
- The plugin never changes a Skim preference. The health check reports it and prints the command.
- Downloads come only from `https://github.com/anirbanchakraborty-dev/swara/releases/download/v<plugin version>/`, and an archive is used only after its SHA-256 matches `SHA256SUMS`.
- Bengali appears in a lesson folder only inside `swara-sargam.ily` and `swara-lyrics.ily`. The repository's test data for those two files is the only other place.
- Every template starts with `\language "english"` and builds with `-l WARNING` and an empty stderr.
- Plugin tests run with `nvim --clean --headless -l tests/run.lua`, never through the user's configuration.

## Review Focus

- **First open while offline, or a download cut off half-way.** The buffer stays plain text, one message says why, the half-written download is never used, and the next Neovim start tries again. The test lives in the binary task.
- **Another `lilypond` grammar already loaded,** such as nwhetsell's through nvim-treesitter. Swara does not start its queries on that grammar, one message points to `:checkhealth swara`, and the health check names the conflict. The test lives in the Tree-sitter task.
- **A LilyPond buffer with no file name,** such as `:enew` followed by `:setfiletype lilypond`. Build, preview, play and MuseScore each say "Save the file first", with no Lua error. The test lives in the commands task.
- **Pressing play twice, pressing stop while FluidSynth is still loading, or quitting Neovim mid-song.** One FluidSynth at most, the older song stops, and quitting leaves no FluidSynth or `swara play` process. The test lives in the playback task.
- **`:Swara new` where the lesson file already exists, or where the user has edited `swara-sargam.ily`.** Nothing is overwritten, and the message names the file that was kept. Names with spaces work. The test lives in the templates task.

---

### Task: Plugin skeleton, settings and the test runner

**Files:**
- Create: `plugin/swara.lua`, `lua/swara/init.lua`, `lua/swara/config.lua`, `lua/swara/version.lua`
- Create: `tests/run.lua`, `tests/helpers.lua`, `tests/config_spec.lua`
- Modify: `.github/workflows/ci.yml` (a job that installs Neovim 0.12 from its release archive, checked against the published checksum, builds `swara`, and runs the plugin tests on macOS and Ubuntu)

**Interfaces:**
- Produces: `require('swara').setup(opts)`, which is optional. `plugin/swara.lua` registers a `FileType lilypond` autocommand that runs with the defaults when `setup` was never called.
- Produces: the defaults in `config.lua`: `{ auto_check = true, format_on_save = true, keys = true, soundfont = nil, debounce_ms = 10, preview_dpi = 120, lilypond = nil, include_paths = {}, release_base_url = 'https://github.com/anirbanchakraborty-dev/swara/releases/download' }`. `config.get()` returns the merged table, and an unknown key gives one warning naming it.
- Produces: `lua/swara/version.lua` returning the string `'0.1.0'`.
- Produces: `tests/run.lua`, which runs every `tests/*_spec.lua` file. Each file returns a table of named functions; the runner calls each with a fresh scratch buffer, prints `ok` or `FAIL <name>: <message>`, and exits 1 on any failure. `tests/helpers.lua` gives `wait_for(pred, timeout_ms)` (built on `vim.wait`), `tmpdir()`, `write(path, text)`, `fake_bin(name, script)`, which puts a shell script on a `PATH` used only by the test, and `child(lua) -> { code, stdout, stderr }`, which runs the code in a fresh `nvim --clean --headless -l`. A test that registers a language or changes global state runs through `child`, so it cannot leak into the tests after it.

- [ ] **Write the failing tests** in `config_spec.lua`: `defaults_without_setup`, `setup_merges`, `unknown_key_warns_once`, and `version_matches_cargo`, which reads `crates/swara/Cargo.toml` and compares its `version` with `version.lua`.
- [ ] **Run them to see them fail.** Run `nvim --clean --headless -l tests/run.lua`. Expected: FAIL.
- [ ] **Implement the skeleton.**
- [ ] **Run them to see them pass.** Run `nvim --clean --headless -l tests/run.lua`. Expected: PASS.
- [ ] **Commit:** `feat(plugin): add the plugin skeleton and test runner`.

### Task: Finding or downloading `swara`

**Files:**
- Create: `lua/swara/binary.lua`, `tests/binary_spec.lua`
- Create: `xtask/src/dist.rs` (`cargo xtask dist --target <triple> --out <dir>`)

**Interfaces:**
- Produces: `cargo xtask dist`, which builds `swara` and `swara-parser` in release mode and writes `swara-<version>-<triple>.tar.gz` (`.zip` on Windows) holding `swara-<version>-<triple>/bin/swara` and `lib/libswara_parser.<dylib|so>`, or `bin/swara.exe` and `bin/swara_parser.dll` on Windows. It also appends that archive's line to `<dir>/SHA256SUMS`. The five triples are `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`.
- Produces: `binary.resolve(callback)`, which calls back with `{ path = string, source = 'path' | 'download', info = table }` or `nil, message`. It tries `swara` on `PATH` first, then `stdpath('data')/swara/<version>/bin/swara`, then a download. `info` is the decoded output of `swara info`.
- Download steps:
  - Fetch `SHA256SUMS` and the archive with `curl -fsSL` into `stdpath('data')/swara/<version>.partial/`.
  - Compare `vim.fn.sha256(vim.fn.readblob(archive))` with the archive's line in `SHA256SUMS`.
  - Unpack with `tar -xf`, which also reads `.zip` on Windows 10 and later.
  - Rename `<version>.partial` to `<version>`. A leftover `.partial` folder is deleted before any new attempt.
- Behaviour: one line, "Swara: downloading swara 0.1.0 for aarch64-apple-darwin", then "Swara: ready" or "Swara: download failed (<reason>); LilyPond files stay plain text until the next start". A failure is not retried in the same Neovim session.

- [ ] **Write the failing tests** in `binary_spec.lua`. Each test builds a fake release folder with a small fake `swara` script that prints a fixed `info` JSON, and points `release_base_url` at it as a `file://` URL, which `curl` reads:
  - `prefers_path`
  - `downloads_and_verifies`: after the download, `source == 'download'` and the binary sits under the data folder.
  - `bad_checksum_refused`: neither `0.1.0` nor `0.1.0.partial` is left under the data folder, and the message says "checksum".
  - `offline_message_once`: an unreachable URL gives one message, and a second LilyPond buffer gives none.
  - `partial_never_used`: a pre-made `0.1.0.partial` holding a binary is deleted, not run.
- [ ] **Write the failing dist test.** `xtask/tests/dist.rs::dist_layout` runs `cargo xtask dist` for the host triple, unpacks the archive, runs `bin/swara info`, and asserts that `grammar` points into the unpacked `lib/` folder and that `SHA256SUMS` matches the archive.
- [ ] **Run them to see them fail.** Run `nvim --clean --headless -l tests/run.lua && cargo test -p xtask dist`. Expected: FAIL.
- [ ] **Implement `binary.lua` and `dist.rs`.**
- [ ] **Run them to see them pass.** Run the same command. Expected: PASS.
- [ ] **Commit:** `feat(plugin): find or download the swara program`.

### Task: Tree-sitter loading, queries and indent

**Files:**
- Create: `lua/swara/treesitter.lua`, `lua/swara/indent.lua`
- Create: `queries/lilypond/highlights.scm`, `folds.scm`, `indents.scm`, `locals.scm` (copied from the pinned grammar commit)
- Create: `xtask/src/check_queries.rs`, `tests/treesitter_spec.lua`

**Interfaces:**
- Consumes: `info.grammar` and `info.grammar_symbol` from `swara info`, and `HIGHLIGHTS_QUERY`, `FOLDS_QUERY`, `INDENTS_QUERY` and `LOCALS_QUERY` from the `tree-sitter-lilypond` crate.
- Produces: `treesitter.attach(buf, info)`. It calls `vim.treesitter.language.add('lilypond', { path = info.grammar, symbol_name = info.grammar_symbol })` once, checks that the loaded language has the node kind `embedded_lilypond`, and only then calls `vim.treesitter.start(buf, 'lilypond')` and sets `indentexpr` and `foldexpr = 'v:lua.vim.treesitter.foldexpr()'` for the window.
- Produces: `indent.get(lnum) -> integer`, used as `indentexpr = "v:lua.require'swara.indent'.get(v:lnum)"`. It counts the `@indent.begin` captures that enclose the line's first non-blank character in `indents.scm`, takes one away when the line starts with an `@indent.branch` or `@indent.end` token, and multiplies by `shiftwidth()`.
- Produces: `cargo xtask check-queries`, which fails when a file under `queries/lilypond/` differs by a single byte from the matching constant.

- [ ] **Probe two Neovim facts and record them.** First, in `nvim --clean --headless`, load the cdylib with `symbol_name = 'lilypond_swara'` and confirm that Neovim looks up `tree_sitter_lilypond_swara`. If it does not, change `grammar_symbol` in the server's `info.rs` and its test to the name that loads. Second, find the shape of `vim.treesitter.language.inspect(lang).symbols` in 0.12, and confirm that `.ly` and `.ily` already map to the filetype `lilypond`. Record all three answers in `docs/neovim-facts.md`.
- [ ] **Write the failing tests** in `treesitter_spec.lua`:
  - `highlights_start`: in a buffer holding `\relative c' { c4\f d }`, `vim.treesitter.get_captures_at_pos` on `\relative` includes `function.builtin` or the capture the mapping table gives it, and on `\f` includes the dynamics capture.
  - `other_grammar_refused` runs through `helpers.child`. It registers Neovim's bundled C parser under the name `lilypond`, found with `nvim_get_runtime_file('parser/c.*', false)`. Then `attach` does not start highlighting, and one message mentions `:checkhealth swara`.
  - `indent_inside_blocks`: the line after `\score {` indents by `shiftwidth()`, and a line holding `}` returns to the outer depth.
  - `unclosed_brace_keeps_colours`: after deleting the final `}` of a 20-line score, line 10 still has a note capture.
  - `colours_under_16ms`: on `bench/song-100.ly`, the median of 20 single-character edits, timed from `nvim_buf_set_text` to the end of `get_parser(buf):parse()`, is under 16 ms. The test prints the median.
- [ ] **Run them to see them fail.** Run `nvim --clean --headless -l tests/run.lua && cargo xtask check-queries`. Expected: FAIL.
- [ ] **Copy the queries** from the grammar commit pinned in `Cargo.lock`, then implement `treesitter.lua`, `indent.lua` and `check_queries.rs`.
- [ ] **Run them to see them pass.** Run the same command. Expected: PASS.
- [ ] **Commit:** `feat(plugin): load Swara's grammar, queries and indent`.

### Task: The language server, diagnostics and tidy on save

**Files:**
- Create: `lua/swara/lsp.lua`, `lua/swara/format.lua`, `tests/lsp_spec.lua`

**Interfaces:**
- Consumes: `bin`, the table `binary.resolve` calls back with, and from the server the notification `swara/buildFinished` with `{ uri, pdf, midi, success, durationMs }`.
- Produces: `lsp.setup(bin)`, which calls `vim.lsp.config('swara', { cmd = { bin.path, 'lsp' }, filetypes = { 'lilypond' }, root_markers = { '.git' }, init_options = { autoCheck = cfg.auto_check, lilypond = cfg.lilypond, includePaths = cfg.include_paths }, flags = { debounce_text_changes = cfg.debounce_ms }, handlers = { ['swara/buildFinished'] = M.on_build } })` and then `vim.lsp.enable('swara')`. A file outside any repository starts the server with no root.
- Produces: `lsp.on_build(err, params, ctx)`. It stores `vim.b[buf].swara_pdf` and `vim.b[buf].swara_midi` for the buffer of `params.uri`, then calls `require('swara.viewer').built(buf, params)`.
- Produces: `format.attach(buf)`. When `format_on_save` is true, it adds a buffer-local `BufWritePre` autocommand that calls `vim.lsp.buf.format({ bufnr = buf, name = 'swara', timeout_ms = 200 })`. It does nothing when conform is loaded and lists a formatter for the buffer; the exact conform call is probed against its current release and noted in a comment.

- [ ] **Write the failing tests** in `lsp_spec.lua`, using the `swara` built by `cargo build`:
  - `syntax_error_shows`: a buffer holding `{ c d` gets a diagnostic "missing `}`" within 1 s.
  - `debounce_is_10ms`: the client's `flags.debounce_text_changes == 10`.
  - `tidy_on_save`: writing `c4  d |e` saves `c4 d | e`.
  - `tidy_off`: with `format_on_save = false` the file is saved as typed.
  - `conform_takes_over`: a fake `package.loaded.conform` that lists a formatter for the buffer means the text is saved as typed.
  - `build_notification_stored`: calling `lsp.on_build` with a fake `swara/buildFinished` payload sets `vim.b.swara_pdf` on the buffer that `uri` names.
- [ ] **Run them to see them fail.** Run `nvim --clean --headless -l tests/run.lua`. Expected: FAIL.
- [ ] **Implement `lsp.lua` and `format.lua`.** Wire `init.lua` so that a `FileType lilypond` buffer runs `binary.resolve`, then `treesitter.attach(buf, bin.info)`, `lsp.setup(bin)` (once per session) and `format.attach(buf)`. A download that finishes later attaches every LilyPond buffer already open.
- [ ] **Run them to see them pass.** Run `nvim --clean --headless -l tests/run.lua`. Expected: PASS.
- [ ] **Commit:** `feat(plugin): start the server and tidy on save`.

### Task: Opening the PDF and the in-editor preview

**Files:**
- Create: `lua/swara/viewer.lua`, `lua/swara/preview.lua`, `tests/viewer_spec.lua`

**Interfaces:**
- Consumes: the refresh route chosen in `docs/spikes/skim.md`.
- Produces: `viewer.built(buf, params)`. After the first successful build of a PDF in this session, it runs `open -g -a Skim <pdf>` when Skim is installed, `open -g <pdf>` on macOS without Skim, and `vim.ui.open(pdf)` elsewhere. Later builds of the same PDF do nothing when both of Skim's watch preferences read 1, and otherwise run the by-path revert from `docs/spikes/skim.md`: `{ 'osascript', '-e', 'on run argv', '-e', 'tell application "Skim" to revert (documents whose path is (item 1 of argv))', '-e', 'end run', realpath }`, where `realpath` is `vim.uv.fs_realpath(pdf)`, because Skim holds resolved paths and `document 1` is whichever PDF happens to be in front. A failed build opens nothing and echoes "Swara: build failed, see the diagnostics".
- Produces: `viewer.open(buf)` for `<localleader>o`. It runs `open -a Skim <pdf>` without `-g`, so Skim comes forward, or `vim.ui.open` elsewhere.
- Produces: `preview.show(buf)`, which sends `workspace/executeCommand` `swara.preview` with `{ uri, cfg.preview_dpi }` and shows the returned PNG with snacks.nvim's image module when it is loaded, otherwise with `vim.ui.open`. The snacks call is probed against its current release and noted in a comment.

- [ ] **Write the failing tests** in `viewer_spec.lua`, replacing `vim.system` and `vim.ui.open` with recorders:
  - `first_build_opens_in_background`: argv is exactly `{ 'open', '-g', '-a', 'Skim', pdf }`.
  - `second_build_refreshes_per_spike`: nothing when both preferences read 1, otherwise exactly the by-path `osascript` argv above, with the PDF's resolved path as its last element.
  - `revert_names_the_document_by_path`: with two PDFs built in turn, each later build's revert carries its own PDF's path, never `document 1`.
  - `failed_build_opens_nothing`.
  - `path_with_spaces_and_bengali`: the path `/tmp/x/আমার গান/lesson 1.pdf` is one argv element.
  - `preview_uses_snacks_when_present` and `preview_falls_back`.
- [ ] **Run them to see them fail.** Run `nvim --clean --headless -l tests/run.lua`. Expected: FAIL.
- [ ] **Implement `viewer.lua` and `preview.lua`.**
- [ ] **Run them to see them pass.** Run `nvim --clean --headless -l tests/run.lua`. Expected: PASS.
- [ ] **Commit:** `feat(plugin): open the PDF and preview the score`.

### Task: Playback and MuseScore

**Files:**
- Create: `lua/swara/playback.lua`, `lua/swara/musescore.lua`, `tests/playback_spec.lua`

**Interfaces:**
- Consumes: the route recorded in `docs/spikes/playback.md`. With streaming: `swara play --port P FILE` and the FluidSynth start options and port rule (`9800 + (pid % 200)`, upward to the first free port) from that spike. Without it: the one-shot FluidSynth command from the same file.
- Produces: `playback.play(buf)`. It uses `vim.b[buf].swara_midi`. The soundfont is `cfg.soundfont`, else MuseScore 4's `/Applications/MuseScore 4.app/Contents/Resources/sound/MS Basic.sf3` when it exists, else the message "Swara: set `soundfont` in Swara's options". With streaming, the first play starts FluidSynth exactly as `docs/spikes/playback.md` gives it, `fluidsynth -s -i -o shell.port=<p> -o shell.prompt=<token> -o synth.dynamic-sample-loading=1 <soundfont>`, with the token `swara-<pid>-<port>>`. It connects with `vim.uv` TCP, retried every 50 ms, and takes the port only when the first bytes it receives are that token. The port counts as busy, and the player launched for it is killed before the next port up is tried, when something else answers first, when FluidSynth exits, when its stderr contains `bind`, or when 2 s pass without the token. Only then does it start `swara play`. A play while another is running stops the older one first.
- Produces: `playback.stop()`, which kills the `swara play` process, then sends `gain 0` and `reset` on the shell port, in that order, because `reset` alone left sound for 122 ms on the song and 580 ms on a held organ note. The gain stays at zero until the next `swara play` sets it again. Without streaming it kills the one-shot FluidSynth.
- Produces: a `VimLeavePre` autocommand that kills `swara play` and FluidSynth.
- Produces: `musescore.open(buf)`, which runs `open -a 'MuseScore 4' <midi>` on macOS, and elsewhere the first of `mscore4`, `mscore` and `musescore` found on `PATH`.

- [ ] **Write the failing tests** in `playback_spec.lua`. Fake `fluidsynth` and `swara` scripts record their argv and pid in a log, the fake FluidSynth's port is a `vim.uv` TCP server started by the test, and the server records each line it receives:
  - `first_play_starts_fluidsynth_once` and `second_play_reuses_it`.
  - `play_twice_stops_first`: the first `swara play` pid is gone when the second starts.
  - `stop_while_loading`: `stop()` before the port opens leaves no `swara play` started and no Lua error.
  - `stop_sends_gain_zero_then_reset`: the fake port's last two lines are `gain 0` and `reset`, in that order.
  - `port_answered_by_another_program_is_skipped`: the test's server on the first port sends `hello` on connect, so the plugin kills that player and starts the next one on the next port up.
  - `fluidsynth_is_launched_with_its_token`: the logged argv carries `-o shell.prompt=swara-<pid>-<port>>` for the port it was given.
  - `quit_leaves_nothing`: a child `nvim --clean --headless` that plays and then quits leaves every logged pid failing `kill -0`.
  - `no_build_yet` says "Build the file first (save it)".
  - `real_play_times` runs only when `SWARA_REAL=1`. With the real FluidSynth and MS Basic, it prints the time from `play()` to the start of `swara play`, first and second time, for `docs/bench/speed.md`.
- [ ] **Run them to see them fail.** Run `nvim --clean --headless -l tests/run.lua`. Expected: FAIL.
- [ ] **Implement `playback.lua` and `musescore.lua`.**
- [ ] **Run them to see them pass.** Run `nvim --clean --headless -l tests/run.lua`, then `SWARA_REAL=1 nvim --clean --headless -l tests/run.lua`. Expected: PASS, with times printed.
- [ ] **Commit:** `feat(plugin): play scores through FluidSynth`.

### Task: Lyrics from English letters: the engine and the Bengali table

**Files:**
- Create: `includes/swara-lyrics.ily`, `xtask/src/test_includes.rs`
- Create: `test/lyrics/engine.ly`, `test/lyrics/lines.ly`, `test/lyrics/words.ly`, `test/lyrics/bengali.tsv`, `test/lyrics/sample-sheet.ly`

**Interfaces:**
- Produces: `cargo xtask test-includes`, which runs `LC_ALL=C lilypond -l WARNING -dno-print-pages` on each `test/*/*.ly` file whose name does not start with `sample-`, and fails on any line starting `swara-test: mismatch`, or when the closing `swara-test: N passed` line is missing.
- Produces, in `swara-lyrics.ily`:
  - `swara-scripts`, an alist from a script symbol to its table, and `(swara-register-script! name table)`, which adds or replaces one entry.
  - `(swara-transliterate script ascii) -> string`, a pure function whose result is in NFC, through Guile's `string-normalize-nfc`. An unknown script calls `ly:warning` naming it and returns `ascii` unchanged.
  - `\transliterate`, a music function `(symbol? ly:music?) -> ly:music?`. It works on `ly:music-deep-copy` of its argument, finds the syllables with `(extract-typed-music copy 'lyric-event)`, and replaces each `text` that is a plain string with `(make-override-markup (cons 'font-name font) converted)`, where `font` is the table's. Hyphens and extenders are separate events and stay as they are.
  - `\bangla`, which is `\transliterate #'bengali`.
  - `\swaraLyrics`, a music function `(string? ly:music?) -> ly:music?` taking a voice name and the lyrics. It reads `lyricLines` and `lyricScript` with `ly:parser-lookup` each time it is called, defaulting to `both` and `bengali` when either is unbound, and returns `<< >>` holding one `\new Lyrics \lyricsto #voice { }` per line. For `both`, the converted line comes first and the typed line second, with `\override LyricText.font-size = #-2` in its `\with` block. For `script`, the converted line prints alone. For `roman`, the typed line prints alone at full size. Any other value calls `ly:warning` and is treated as `both`.
- Produces: the table format, an alist with these keys.
  - `consonants`: an ASCII string to a script string.
  - `vowels`: an ASCII string to a pair of full form and sign, where a sign of `""` marks the inherent vowel.
  - `marks`: an ASCII string to a script string, for signs that never join and never take a vowel sign.
  - `after-consonant`: letters with their own form straight after a consonant. The value is the whole string written, joiner included.
  - `after-vowel`: letters with their own form anywhere except straight after a consonant.
  - `joiner`: the string written between two consonants that touch.
  - `font`: the `font-name` value.
- Produces: the engine's rules, the same for every script.
  - Matching is case-sensitive and takes the longest key at each position, across every key in the table.
  - A vowel straight after a consonant writes its sign. Anywhere else it writes its full form.
  - Two consonants that touch get the joiner between them. A consonant followed by anything else, or by the end of the text, gets nothing, so a consonant at the end of a syllable carries no hasanta.
  - An `after-consonant` or `after-vowel` result counts as a consonant for what follows it, so a vowel after it writes its sign.
  - A character that no key matches is copied unchanged and counts as neither consonant nor vowel.
- Produces: the Bengali table, registered as `bengali`, with the joiner ্ (the hasanta) and the font `"Kohinoor Bangla, Noto Sans Bengali"`.

| Letters | Bengali | Letters | Bengali | Letters | Bengali |
| --- | --- | --- | --- | --- | --- |
| `k` `kh` | ক খ | `g` `gh` `Ng` | গ ঘ ঙ | `c` `ch` | চ ছ |
| `j` `jh` `NG` | জ ঝ ঞ | `T` `Th` | ট ঠ | `D` `Dh` `N` | ড ঢ ণ |
| `t` `th` | ত থ | `d` `dh` `n` | দ ধ ন | `p` `ph` `f` | প ফ ফ |
| `b` `bh` `v` | ব ভ ভ | `m` `z` | ম য | `r` `l` | র ল |
| `sh` `Sh` `s` `h` | শ ষ স হ | `R` `Rh` | ড় ঢ় | | |

| Vowel | Full form | Sign after a consonant |
| --- | --- | --- |
| `o` | অ | none (the inherent vowel) |
| `a` | আ | া |
| `i` `I` | ই ঈ | ি ী |
| `u` `U` | উ ঊ | ু ূ |
| `e` | এ | ে |
| `O` | ও | ো |
| `OI` `OU` | ঐ ঔ | ৈ ৌ |
| `rri` | ঋ | ৃ |

- The marks are `` t` `` ৎ, `ng` ং, `:` ঃ and `^` ঁ.
- `after-consonant` holds `y` as the য-ফলা ্য and `w` as the ব-ফলা ্ব. `after-vowel` holds `y` as য়.
- So `kSh` gives ক্ষ, `str` gives স্ত্র, `rk` gives র্ক, and `bangla` gives বাংলা, with no joiner before or after the mark.

Measured on LilyPond 2.26.0 on 2026-10-24, so these need no probe step: `transliterate`, `bangla`, `swaraLyrics`, `lyricLines` and `lyricScript` are all unbound, so no built-in is shadowed. `ly:parser-lookup` reads a top-level variable assigned earlier in the file. `font-name` falls back along a comma list, since `"Nonexistent Font, Bangla MN"` printed in Bangla MN where the system's own fallback for Bengali is Kohinoor Bangla. `\lyricsto` takes its lyrics only inside braces. `extract-typed-music` wants the type `'lyric-event`, and the event name `'LyricEvent` matches nothing. ড় written as one code point and as ড followed by ় compare unequal until both pass through `string-normalize-nfc`.

- [ ] **Write the failing tests.**
  - `engine.ly` registers a made-up script, `test-latin`, with the consonants `k`→`K` and `t`→`T`, the vowels `o`→`("O" . "")` and `a`→`("A" . "-a")`, the mark `n`→`N`, `after-consonant` `y`→`+Y`, `after-vowel` `y`→`Y`, the joiner `+` and the font `"C059"`. Through `swara-transliterate` it expects `kta` to give `K+T-a`, `ok` to give `OK`, `kot` to give `KT`, `kn` to give `KN`, `kya` to give `K+Y-a`, `ya` to give `Y-a` and `k!` to give `K!`, and an unknown script to return its input. A new script therefore needs only a table.
  - `lines.ly` calls `\swaraLyrics "v" \lyricmode { a -- mi }` four times and counts the `Lyrics` context specifications in each result: 2 before `lyricLines` is set at all, then 2, 1 and 1 after setting it to `both`, `script` and `roman` in turn. Because the function reads the variable each time it is called, one file can set each value in turn. In the `both` result, the first line's syllables are markups and the second line's are the strings `a` and `mi`. After all four calls, the music passed in still holds `a` and `mi`.
  - `bengali.tsv` holds at least 80 lines of ASCII, a tab, then the expected Bengali, covering every row of both tables, every mark, both `after-` letters, every rule, and the design's examples: `ami` আমি, `tOmay` তোমায়, `bhalO` ভালো, `basi` বাসি, `kShoma` ক্ষমা, `strI` স্ত্রী. Two of its lines are typed with ড় and য় as single code points, so the NFC rule is tested.
  - `words.ly` reads `bengali.tsv` from the folder it sits in, passes each word through `(swara-transliterate 'bengali word)`, compares it with the expected text after `string-normalize-nfc`, warns `swara-test: mismatch` on a difference, and ends with `swara-test: N passed`.
- [ ] **Run them to see them fail.** Run `cargo xtask test-includes`. Expected: FAIL.
- [ ] **Write `test_includes.rs` and `swara-lyrics.ily`.**
- [ ] **Run them to see them pass.** Run `cargo xtask test-includes`. Expected: PASS, with a `swara-test: N passed` line from each of the three files.
- [ ] **Engrave the sample sheet.** `sample-sheet.ly` sets 40 common song words through `\swaraLyrics` with `lyricLines = #'both`, so each word prints in Bengali with its typed spelling beneath. Run `lilypond -o test/lyrics/sample-sheet test/lyrics/sample-sheet.ly`, check that `pdffonts` lists `KohinoorBangla`, and send the PDF to the owner to read. Every word the owner marks wrong becomes a `bengali.tsv` line before the fix.
- [ ] **Commit:** `feat(lessons): convert English-letter lyrics through a script table`.

### Task: Lesson templates, `:Swara new` and the sargam line

**Files:**
- Create: `templates/guitar.ly`, `templates/piano.ly`, `templates/song.ly`, `templates/song-lyrics.ily`
- Create: `includes/swara-sargam.ily`, `test/sargam/names.ly`
- Create: `lua/swara/templates.lua`, `tests/templates_spec.lua`

**Interfaces:**
- Consumes: `cargo xtask test-includes`, and `includes/swara-lyrics.ily` with `\swaraLyrics`, `lyricLines` and `lyricScript`, all from the lyrics task.
- Produces: `templates.new(kind, name)` for `kind` in `guitar`, `piano` and `song`. It writes `<name>.ly` in the current folder, with `{{title}}` replaced by `name` and `{{version}}` by `info.lilypond.version` or `2.26.0`. It copies `swara-sargam.ily`, plus `swara-lyrics.ily` for songs, unless a file of that name is already there. A song also gets `<name>-lyrics.ily`. An existing `<name>.ly` stops everything, and nothing is written.
- Produces, in `swara-sargam.ily`, all names prefixed `swara-`:
  - `(swara-sa)`, a pitch set with `\swaraSa c'` and defaulting to C.
  - `(swara-script)`, set with `\swaraScript #'bengali`, `#'devanagari` or `#'latin`.
  - `(swara-name semitones octave)`, a pure function returning `(syllable komal? tivra? octave)`.
  - `swara-note-name`, the `noteNameFunction`.
  - The markup follows `docs/design/sargam-reference.ly`: komal underlined, tivra Ma with a vertical stroke, octave dots below and above with one staff-space of padding, and the font Kohinoor Bangla.
- Produces: the templates.
  - guitar: `\new Staff \with { \clef "treble_8" }`, then `\new TabStaff \with { stringTunings = \stringTuning <e, a, d g b e'> }`, then letter `NoteNames`, then the sargam `NoteNames`.
  - piano: a `PianoStaff` with two staves, letter names and the sargam line.
  - song: at the top, `lyricScript = #'bengali` and `lyricLines = #'both`, with a comment naming the three values `both`, `script` and `roman`; then `\include "swara-lyrics.ily"` and `\include "{{title}}-lyrics.ily"`, which defines `songLyrics = \lyricmode { }`; then a score holding `melody`, `\new ChordNames`, and `\swaraLyrics "melody" \songLyrics`. `templates/song-lyrics.ily` holds a two-bar sample in English letters.

- [ ] **Write the failing tests.**
  - `test/sargam/names.ly` checks all twelve semitones with Sa on C and on D, both octave neighbours, and all three scripts, through `swara-name`. On a mismatch it calls `ly:warning "swara-test: mismatch ~a"`.
  - `templates_spec.lua`:
    - `guitar_creates_files`
    - `existing_lesson_untouched`: the message names the file, and its bytes are unchanged.
    - `edited_include_kept`: an edited `swara-sargam.ily` is byte-identical afterwards.
    - `name_with_space`: `my song` gives `my song.ly` and `my song-lyrics.ily`.
    - `missing_name_shows_usage`.
    - `templates_build_clean`, which runs only when LilyPond is present: each new lesson builds with exit 0 and an empty stderr, and the song does so with `lyricLines` set to each of `both`, `script` and `roman`.
- [ ] **Run them to see them fail.** Run `cargo xtask test-includes && nvim --clean --headless -l tests/run.lua`. Expected: FAIL.
- [ ] **Write `swara-sargam.ily`, the templates and `templates.lua`.**
- [ ] **Run them to see them pass.** Run the same command. Expected: PASS.
- [ ] **Commit:** `feat(lessons): add lesson templates and the sargam line`.

### Task: Commands, keys and the health check

**Files:**
- Create: `lua/swara/commands.lua`, `lua/swara/keys.lua`, `lua/swara/health.lua`, `tests/commands_spec.lua`

**Interfaces:**
- Produces: `:Swara {new|build|open|preview|play|stop|musescore}`, with completion of the subcommands and, after `new`, of `guitar`, `piano` and `song`. `build` sends `swara.build` with the buffer's URI, so unsaved text is built.
- Produces: buffer-local keys in `lilypond` buffers when `keys` is true, each with a `desc`: `<localleader>o` (open), `v` (preview), `p` (play), `s` (stop), `m` (MuseScore) and `b` (build).
- Produces: `:checkhealth swara`, with one `vim.health` line per item: the `swara` version and whether it matches `version.lua`, its source, the grammar commit (`info.grammar_rev`) and any conflict (from the `embedded_lilypond` check), the LilyPond version, whether the command cache exists (`info.cache.present`), FluidSynth and the soundfont, MuseScore, and, on macOS, Skim with `defaults read -app Skim SKAutoCheckFileUpdate` and `SKAutoReloadFileUpdate`. When either is off, it shows `defaults write -app Skim SKAutoCheckFileUpdate -bool true` and the same for `SKAutoReloadFileUpdate`, and runs neither. Without LilyPond it gives one warning saying the check, builds, preview and playback are off.

- [ ] **Write the failing tests** in `commands_spec.lua`:
  - `completes_subcommands`: all seven, in the order above.
  - `keys_only_in_lilypond`: the keys exist in a `lilypond` buffer, not in a `markdown` buffer, and not at all with `keys = false`.
  - `unnamed_buffer_says_save_first`: `build`, `preview`, `play` and `musescore` each give "Save the file first".
  - `health_without_lilypond`: one warning, and the grammar line is still OK.
  - `health_reports_conflict`: through `helpers.child`, with the C parser registered as `lilypond` as in `other_grammar_refused`.
  - `health_never_writes_defaults`: a recorder on `vim.system` sees no `defaults write`.
- [ ] **Run them to see them fail.** Run `nvim --clean --headless -l tests/run.lua`. Expected: FAIL.
- [ ] **Implement `commands.lua`, `keys.lua` and `health.lua`.**
- [ ] **Run them to see them pass.** Run `nvim --clean --headless -l tests/run.lua`. Expected: PASS.
- [ ] **Commit:** `feat(plugin): add commands, keys and the health check`.

### Task: Release workflow, READMEs and v0.1.0

**Files:**
- Create: `.github/workflows/release.yml`, `docs/release-notes/v0.1.0.md`
- Modify: `README.md`, `docs/bench/speed.md`

**Interfaces:**
- Produces: on a `v*` tag, a matrix running `cargo xtask dist`.
  - `macos-latest` builds `aarch64-apple-darwin`, and `x86_64-apple-darwin` by cross-compiling.
  - `ubuntu-latest` builds `x86_64-unknown-linux-gnu`.
  - `ubuntu-24.04-arm` builds `aarch64-unknown-linux-gnu`.
  - `windows-latest` builds `x86_64-pc-windows-msvc`.
  - A final job merges the five `SHA256SUMS` lines and runs `gh release create` with `GITHUB_TOKEN`, the notes file and every archive. Every action is pinned to a commit, and each is named to the owner before the workflow is first pushed.
- Produces: the README, in impersonal prose with no credit to Claude, covering:
  - what Swara is;
  - installing with lazy.nvim (`{ 'anirbanchakraborty-dev/swara', ft = 'lilypond', opts = {} }`), and with Homebrew once the tap exists;
  - registering the grammar with nvim-treesitter, checked against nvim-treesitter's current README;
  - the commands, and the keys table naming each key and what it does;
  - the settings and `:checkhealth swara`;
  - the speed table from `docs/bench/speed.md`, with any misses;
  - the licence, and the two GPL boundaries (LilyPond's docstrings stay in the user's cache, and no code from GPL projects).

- [ ] **Run the whole suite** on this Mac: `cargo test --workspace -- --include-ignored`, `cargo xtask test-includes`, `cargo xtask check-queries`, `nvim --clean --headless -l tests/run.lua` and `cargo deny check`. Expected: PASS.
- [ ] **Fill in `docs/bench/speed.md`** with the colours, server and playback medians against every row of the design's speed table. **Write the release notes** listing what ships and every missed target. Run the prose gate on both files and the README.
- [ ] **Commit** the workflow, notes and README: `docs: prepare the v0.1.0 release`. Push, wait for CI with a `Monitor`, then tag `v0.1.0` and push the tag.
- [ ] **Check the release from a clean data folder.** After the owner agrees to the download, start Neovim with an empty `XDG_DATA_HOME` and no `swara` on `PATH`, open `bench/song-100.ly`, and confirm that the download message appears, highlighting starts, and `:checkhealth swara` shows `source: download`.
- [ ] **Bump the hub pointer.** From the hub worktree: `git commit --only Projects/swara -m "chore(swara): bump submodule to <short-sha>"`.

### Task: Homebrew tap (after the owner confirms it)

**Files:**
- Create, in a new repository `anirbanchakraborty-dev/homebrew-tap` at `Projects/homebrew-tap`: `Formula/swara.rb`, `README.md`, `LICENSE` (MIT)
- Modify, in the hub: `Utilities/Post-Mac-Setup/mac-setup.sh`, `Utilities/Post-Mac-Setup/Brewfile`; then the public `anirbanchakraborty-dev/Post-Mac-Setup` repository and the website's `content/blog/post-mac-setup.mdx`

**Interfaces:**
- Produces: `Formula/swara.rb`.
  - `url` points at the `v0.1.0` source tarball, with its `sha256`.
  - `depends_on "rust" => :build`.
  - `install` runs `cargo build --release --locked -p swara -p swara-parser`, installs `target/release/swara` into `bin`, and the library into `lib`.
  - `test` asserts that `swara info` contains `"abi":15` and a `grammar` path under `lib`.

- [ ] **Ask the owner** to confirm the repository name `homebrew-tap`, public visibility and the MIT licence. Stop here until they answer.
- [ ] **Create the repository** with `git init`, install the credit hook, write the three files and commit. Recent Homebrew refuses a formula outside a tap, so tap the local repository: `brew tap anirbanchakraborty-dev/tap Projects/homebrew-tap`, then `brew install --build-from-source anirbanchakraborty-dev/tap/swara && brew test anirbanchakraborty-dev/tap/swara && brew audit --strict --new anirbanchakraborty-dev/tap/swara`. Expected: PASS.
- [ ] **Publish it.** Run `gh repo create anirbanchakraborty-dev/homebrew-tap --public --source Projects/homebrew-tap --push` and register it as a hub submodule. Then `brew untap anirbanchakraborty-dev/tap`, tap it again from GitHub, and `brew install anirbanchakraborty-dev/tap/swara`. Expected: `swara info` works.
- [ ] **Run the Mac-setup checklist,** because this Mac now has a new Homebrew formula. In `mac-setup.sh`, add `anirbanchakraborty-dev/tap` to `TAPS`, `anirbanchakraborty-dev/tap/swara` to `TRUST_FORMULAE` (the script's comment says the two stay in step) and to `FORMULAE` beside `lilypond`. Add `tap` and `brew` lines to the `Brewfile`, copy both byte for byte into the public Post-Mac-Setup repository and push it, then update the package tables and counts in the blog post. Commit the website inside its submodule, push, and bump its pointer from the hub.
- [ ] **Add the Homebrew line to the Swara README** and commit: `docs: add the Homebrew install`. Push, then bump the hub pointers for `swara` and `homebrew-tap`.
