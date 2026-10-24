# Swara: a LilyPond grammar, language server and Neovim plugin

Design document, draft of 2026-10-24. Status: awaiting the owner's approval. Nothing described here has been built yet.

## What this is and why it exists

LilyPond is a music typesetter: a plain-text file of notes goes in, and an engraved PDF and a MIDI file come out. Editing those files in Neovim today is poor. Neovim 0.12 recognises `.ly` and `.ily` files but ships no highlighting for them. The one maintained Neovim plugin, `martineausimon/nvim-lilypond-suite`, is GPL-3.0 and built on the old regex syntax files. The only LilyPond language server found, `tymbalodeon/lilypond-language-server`, has not moved since August 2024. The best Tree-sitter grammar, `nwhetsell/tree-sitter-lilypond` (MIT), targets Helix, Nova and Zed rather than Neovim.

Swara is three things built from the ground up and published as public MIT projects:

- **`tree-sitter-lilypond`**, a new Tree-sitter grammar. Tree-sitter is a parsing library that editors run on every keystroke to get a syntax tree of the file, which drives colours, folding and structural checks.
- **`swara lsp`**, a language server written in Rust. A language server is a separate program that an editor talks to over the Language Server Protocol (LSP) to get error reports, completions, hover text and formatting.
- **The Swara Neovim plugin**, written in Lua, which loads the grammar, starts the server, builds the PDF on save, shows it in Skim, previews it inside the terminal and plays it back.

The first user is a beginner starting music theory, guitar and piano from zero. A typical exercise has a standard staff, a guitar tablature staff, letter note names under both, a line of Indian sargam syllables (Sa Re Ga Ma, written in Bengali script) computed from the notes, and later song lyrics. The tools have to make writing such an exercise quick and the feedback immediate, because a beginner learns from seeing and hearing the result of each change.

Speed is the requirement that defines the project. The owner asked for tools that are "fast and very responsive and built from the ground up". Every speed claim in this document is a numeric target with a test that measures it.

## Goals

- Colours and syntax errors appear on every keystroke, with no visible lag, on files up to a full song.
- LilyPond's own errors and warnings appear shortly after typing pauses, without saving.
- Saving builds the PDF and MIDI, tidies the spacing, and the PDF in Skim updates on its own.
- Completion offers every command, music function, markup command, context, grob and property the installed LilyPond knows, plus the variables defined in the file and its includes.
- A new lesson file can be started from a template that already carries the staff, tablature, note names and sargam lines.
- Bengali song lyrics can be typed in English letters and come out in the PDF as two lines, Bengali script with the typed spelling beneath it, and a switch keeps only one of them.
- Playback of the current file starts in about a second the first time and almost at once after that.
- Installation needs no manual steps beyond having LilyPond: the plugin fetches its own program and grammar.
- "Better than the existing grammar" is measured on a corpus of real files and reported honestly, whichever way it comes out.

## Non-goals for the first release

- A Mason package. Mason's official registry is the only route worth having and is left for later.
- LilyPond embedded in LaTeX or Texinfo (`.lytex`, `.tely`). Neovim assigns those files no filetype and they need a two-language design.
- Clicking a note in the PDF to jump to the source line. LilyPond writes `textedit://` links into the PDF, but following them needs a URL handler outside Neovim.
- Moving the PDF view to follow the cursor. Skim's sync works for TeX through SyncTeX, which LilyPond does not produce.
- MusicXML import or export.
- Rename across files, code actions and semantic tokens.
- Clients for editors other than Neovim. The server speaks standard LSP and should work elsewhere, but only Neovim is shipped and tested.
- Rewriting LilyPond's error messages into friendlier wording.
- Lyric tables for scripts other than Bengali. The engine takes them later as data.

## Anti-goals

These would make the project a failure even if every feature worked.

- Any lag while typing. Nothing slow may run on the path from a keystroke to the screen, and no LilyPond run may block the editor.
- Skim or any other window taking focus away from the terminal while the user types.
- The formatter changing anything that matters: a note, a duration, an octave mark, a comment or a line break.
- Shipping text copied from GPL projects, including LilyPond's own docstrings and word lists, inside an MIT release.
- Claude or Anthropic credited anywhere: no co-author trailer in any commit, no "Generated with" line in any pull request, no author credit in any README or release note.
- Silent failure. When LilyPond, FluidSynth or Skim is missing, the plugin says so once, in one line, and keeps every feature that does not need it.

## Constraints

- **Licence.** MIT for all three projects. No code, query, word list or docstring is copied from GPL projects: LilyPond, its bundled Vim files, nvim-lilypond-suite, Frescobaldi and python-ly. Facts about the language, such as which commands switch the lexer into lyric mode, are not copyrightable and may be used. LilyPond's docstrings are GPL text and are read from the user's own LilyPond at run time, kept on the user's machine and never shipped.
- **LilyPond version.** Built and tested against LilyPond 2.26.0, the current Homebrew release. Other versions should work because command data is read from whichever LilyPond is installed, but only 2.26 is tested.
- **Neovim version.** 0.12 or later, the version the plugin is tested on.
- **Platforms.** macOS on Apple silicon is the tested platform. Release builds also cover macOS on Intel, Linux and Windows. Skim is macOS-only, so other platforms open the PDF in the system's default viewer.
- **Third-party libraries** are reviewed before adoption: licence, maintenance and the dependencies each one pulls in.
- **Repository rules.** Each repository installs a commit-message hook that refuses a commit crediting Claude, and that hook is never bypassed.

## Decisions the owner made

These were settled in conversation on 2026-10-24 and are not open.

- The server is written in **Rust**.
- **Two repositories**: `swara`, holding the server and the Neovim plugin, and `tree-sitter-lilypond`, holding the grammar on its own so that nvim-treesitter and other editors can consume it. A third, a Homebrew tap, follows from the install decision below.
- **MIT** licence for both.
- The project is called **Swara**.
- The **LilyPond check runs on each pause in typing**, not only on save.
- **Skim opens automatically** with the PDF. The **in-editor preview**, an image drawn inside Ghostty, is **on a key**.
- **FluidSynth**, installed through Homebrew, plays the file for quick listening. **MuseScore** opens it on a key for slower practice.
- **Tidy on save**, the way other languages are formatted on save.
- **Install** by the plugin downloading its own `swara` program and grammar, with a **Homebrew tap**, `anirbanchakraborty-dev/tap/swara`, as the other route. **Mason is left for later.**
- **Lesson files hold notes only.** The sargam line is computed from the notes by a shared include file, `swara-sargam.ily`, so no Bengali appears in the lesson source. `:Swara new` copies the include into the lesson folder, so lessons still build with plain `lilypond`. Templates exist for guitar, piano and song. Song lyrics live in a separate file per song.
- **The speed targets** in the table below, and **the comparison plan** against the existing grammar.
- **One complete first release.** Everything under Goals ships together, rather than lesson tools first and the grammar later. *(Answered after the builder check.)*
- **Bengali lyrics are typed in English letters and converted to Bengali script.** *(Answered after the builder check.)*
- **Both lyric lines, and a general converter.** The song template prints the Bengali line and the typed English-letter line, with a switch for either one alone. The converter is one engine for Indian scripts that takes a letter table per script, and the first release ships only the Bengali table. *(Answered on 2026-10-24, after the build plan was written.)*

## How the parts fit together

Neovim loads the grammar as a shared library and runs it in-process for colours, folds and indentation. The same grammar is compiled into the `swara` program, which runs as the language server in a separate process. Neovim sends the server each edit; the server keeps its own up-to-date copy of the text and tree, answers completion and hover requests from memory, and runs LilyPond in the background for the slower checks. On save the server also runs the full build that produces the PDF and MIDI, so that the warnings only engraving produces land in the same diagnostics list, and then tells the plugin the build finished. The plugin opens Skim and drives FluidSynth for playback. *(The server owning the build is the designer's call.)*

```
Neovim ── grammar (shared library, in-process): colours, folds, indent
   │
   └─ LSP over stdin/stdout ── swara lsp ── grammar (compiled in): instant checks, completion, hover, format
                                   │
                                   ├─ lilypond (child process, on pause): LilyPond's errors and warnings
                                   └─ lilypond (child process, on save): PDF + MIDI + engraving warnings
Plugin ── Skim (PDF, after each build), FluidSynth (sound), MuseScore (on a key)
```

## The grammar: `tree-sitter-lilypond`

### What it has to get right

LilyPond's own lexer is unusually dependent on context, and the grammar's quality comes down to reproducing that faithfully. The facts below were verified against LilyPond 2.26.0's lexer and by running LilyPond on test files.

- **Modes.** The lexer has 13 exclusive states besides its initial one. The top level of a file is in note mode. The plain initial state applies only inside `\paper`, `\layout` and `\midi` blocks. Drum mode is note mode with drum pitch names, not a lexer state of its own.
- **Mode-switching commands.** `\notemode`, `\drummode` and `\drums`, `\figuremode` and `\figures`, `\chordmode` and `\chords`, `\lyricmode` and `\lyrics`, `\markup` and `\markuplist`, and `\addlyrics` and `\lyricsto`. The last two switch only for a following braced or `<< >>` block, so an identifier after them is not re-read in lyric mode. Inside markup, `\score { }` returns to note mode.
- **Lyric words.** A word starts with any character other than `| * . = $ # { } " \`, whitespace or a digit, and continues with characters other than `$ # { } " \`, whitespace or a digit, so a digit ends a word. `--` and `__` are hyphen and extender tokens only when they stand as whole words. An `_` inside a word is printed as a space.
- **Markup words** are any run of characters other than `$ # { } " \` and whitespace.
- **Comments.** `%` starts a line comment and `%{ ... %}` a block comment, which does not nest. `%%{` is a line comment and `%%%}` closes a block. Inside markup words and lyric words, `%` is a literal character.
- **Embedded Scheme.** `#` reads exactly one Guile datum, evaluated later. `$` reads one datum evaluated immediately by the lexer. `#{ ... #}` embeds LilyPond inside Scheme and nests to any depth. After `#`, a `}` counts as part of a symbol, so `#red}` is the symbol `red}`. Inside `#{ #}`, every `%` starts a comment and `%{` ends at the first `%`, so `\markup { 50% off }` breaks there.
- **Identifiers** are letters, or any byte from 0x80 to 0xFF, with single `-` or `_` allowed between letters and no digits. `\foo2` is the command `\foo` followed by the duration `2`. Assignments also accept `"any text" =`, `\"any text"` and dotted names such as `verse.1 =`.
- **`\language`** defaults to nederlands, persists at the top level, is undone at the end of `\notemode { }`, is ignored in chord mode and in the initial state, and in drum mode replaces the drum names.
- **Runtime-dependent tokens.** How many arguments a music function takes, and of which kind, comes from its signature, which LilyPond reads at run time. Pitch names and chord modifiers come from tables that `\language` swaps.

### Design of the grammar

The grammar reproduces LilyPond 2.26 as it is, quirks included, because its errors must agree with the compiler's. A file LilyPond accepts should parse without error, and a file LilyPond rejects for a syntax reason should show an error at the same place. *(Designer's call.)*

One grammar covers both LilyPond and the Scheme embedded in it, including `#{ #}` nested inside Scheme nested inside LilyPond. The existing grammar splits Scheme into a second grammar joined by injection. A single grammar keeps one tree, one parse per keystroke and one set of queries, and handles the nesting directly. *(Designer's call.)*

Tree-sitter lexes by asking which tokens are valid at the current point of the parse, so lyric words, markup words and note-mode tokens can be separate token types that only appear inside the matching mode's rules. The parts plain rules cannot express go in an external scanner written in C: reading one Scheme datum after `#` or `$` with LilyPond's symbol rules, block comments, and the cases where the meaning of a character depends on the mode. Mode switches are recognised by the command names listed above. *(Designer's call.)*

The grammar does not decide what it cannot know without running LilyPond. It does not bind arguments to music functions, because arity comes from signatures known only at run time; it parses a command and the expressions after it as a flat sequence, and the server applies arity using the command data it reads from LilyPond. It does not decide whether a word is a valid pitch, because pitch names depend on `\language`; it parses a note-shaped word, and the server checks it against the active language's table.

The grammar's node names, token boundaries and scanner internals are left to the builder. The highlight queries need distinct nodes for at least: notes with their accidental and octave marks, rests and spacers, durations, chords `< >`, sequential `{ }` and simultaneous `<< >>` music, commands, assignments, strings, both comment kinds, Scheme expressions, embedded LilyPond, markup blocks, lyric words with hyphens and extenders, chord-mode and figure-mode items, bar checks, ties, slurs, beams, articulations, fingerings and dynamics, context creation with `\new` and `\with`, `\override` and `\set` with their property paths, and `\header`, `\paper`, `\layout` and `\midi` blocks.

### Queries and tests

The grammar repository holds `highlights.scm`, `folds.scm`, `indents.scm` and `locals.scm`. None of them may contain LilyPond's command word lists, which are GPL. Highlighting is by node kind, not by a list of known names, and the server supplies the names at run time for completion and checks.

The repository commits `src/parser.c`, because nvim-treesitter downloads a tarball of the repository and builds from it. It is generated for Tree-sitter ABI 15, which the `tree-sitter` 0.27 CLI produces by default and which Neovim 0.12 and the `tree-sitter` 0.27 Rust crate both load.

Tests run under `tree-sitter test`: syntax-tree tests in `test/corpus/`, and highlight assertions in `test/highlight/`. Every quirk in the list above gets at least one corpus test. Incremental reparsing is tested through the Rust crate, because the CLI's `parse --edits` option fails in 0.27.1.

## The language server: `swara lsp`

### One program, several commands

The server is one program, `swara`, with subcommands: `swara lsp` runs the language server, `swara format` tidies a file from standard input to standard output for use by conform.nvim and other formatters, and `swara info` prints its version and where its grammar library is. *(Designer's call.)*

It is built on `lsp-server` 0.10, the synchronous LSP transport that rust-analyzer uses and maintains, with `lsp-types` 0.97 for the message types. `lsp-types` has not had a release since 2024 but covers LSP 3.17 and is the most used crate of its kind. `tower-lsp` is unmaintained and its community fork is mid-way through an API change. There is no async runtime: one main thread handles messages, parsing is incremental and done on that thread because it takes well under a millisecond on lesson-sized files, and LilyPond runs in child processes whose output a helper thread sends back over a channel. *(Designer's call.)*

The grammar is compiled into `swara` through a Cargo git dependency pinned to a commit of `tree-sitter-lilypond`. *(Designer's call.)*

The server negotiates UTF-8 positions, which Neovim 0.12 offers alongside UTF-16 and UTF-32. Tree-sitter works in bytes, so UTF-8 avoids converting every position. *(Designer's call.)*

### Diagnostics in three layers

Errors come from three sources that differ in speed, and the server merges them into one list per file. LSP's push model replaces the whole list on every publish, so the server always publishes the union of the three. *(Designer's call.)*

- **Instant checks**, run by the server on every edit, under 50 ms from keystroke to display. Syntax errors from the tree (Tree-sitter's error and missing nodes, reported in words such as "missing `}`"); a backslash command that is neither built into LilyPond nor defined in the file or its includes; a note name that is not valid in the active `\language`; and an `\include` naming a file that does not exist. The unknown-command check stays off until the command data has loaded, so it never reports a false error on start-up. *(The list is the designer's call.)*
- **The LilyPond check**, run when typing pauses for about a third of a second. The server runs LilyPond on the unsaved text with engraving skipped, which reports parse errors, unknown commands, Scheme errors and bar-check failures. *(The owner chose a check on each pause; the pause length is the designer's call.)*
- **Engraving warnings**, collected from the full build on save. Some warnings, such as a tablature note with no string to play it on, arise only while LilyPond engraves, which the pause check skips. These stay on display after later edits, move with the lines they belong to, and are replaced by the next save's build. *(Designer's call.)*

Neovim's client waits 150 ms by default before sending an edit to a server. The plugin lowers that to about 10 ms for Swara's client so the instant checks can meet their target, and the server applies the one-third-second pause itself. *(Designer's call.)*

Neovim ignores the version number LSP attaches to diagnostics, so a slow LilyPond result can arrive after a newer edit and point at the wrong lines. The server tags each LilyPond run with the document version it was started for, kills a run once a newer edit makes it stale, and drops any result whose version is no longer current. *(Designer's call.)*

An `.ily` include file cannot be compiled on its own. It gets the instant checks, plus any LilyPond diagnostics that name it when an open `.ly` file that includes it is checked. *(Designer's call.)*

### Running LilyPond

LilyPond has no server mode, so each check is a fresh process. The fixed cost is about 155 ms, of which about 74 ms is starting Guile, its Scheme interpreter. A warm loop that keeps one LilyPond running and feeds it files through `ly:parse-file` works and costs about 70 ms per file, but leans on behaviour LilyPond does not promise. It stays an optional later step, adopted only if a measurement shows the fresh-process path missing its target. *(Designer's call.)*

Every run sets `LC_ALL=C`, because LilyPond translates its severity words ("error", "warning") and the parser keys on them. Every run passes `-l WARNING`, because the quieter `-s` setting hides warnings, bar-check failures among them. Unsaved text goes in on standard input (`-`), with the file's own folder passed as an absolute `-I` path so that `\include` still finds neighbouring files. Paths are always absolute, because `-o dir/name` makes LilyPond change into the output folder, after which relative `-I` paths resolve from the wrong place. Check output goes to a temporary folder. Options come before the file name, which LilyPond requires. If standard input turns out to break include resolution, the fallback is a temporary file with the same `-I`. *(Designer's call; the builder verifies include resolution from standard input.)*

LilyPond writes diagnostics to standard error as `FILE:LINE:COL: error|warning: MESSAGE` followed by two lines of context. The column is 1-based and counts characters, with a tab advancing to the next multiple of 8, so the server converts it to a byte offset in the line. The parser also handles the variants seen in testing: a warning with a line and no column (`file:1: warning: no \version ...`), Guile errors, reader errors with no severity word, messages with no location, and `programming error` lines. Standard input appears as the file name `-` and is mapped back to the buffer. LilyPond exits with status 1 and `fatal error: failed files:` when any error occurred, and usually still writes a PDF.

**Skipping engraving is the one unproven mechanism in the design.** For a file without `\layout`, LilyPond only builds MIDI, and a 100-bar song checks in 237 ms. For a file with `\layout`, the fastest proven option is `-dno-print-pages`, at 910 ms for the same song, which misses the target. The candidates are to override the score handler so it interprets the music without engraving, or to run only the MIDI translators through `ly:run-translator`, injected before the user's file with LilyPond's `-dinclude-settings` option so the buffer text is untouched. Proving or rejecting this is the first task of the build. If neither works, the LilyPond check uses `-dno-print-pages` and the speed table records the miss.

Compiling a LilyPond file runs any Scheme in it, and Scheme can run shell commands and read or write files as the user. LilyPond 2.26 has no safe mode. So **opening a file never runs LilyPond.** The first LilyPond run for a file comes only after the user types in it, saves it or asks for a build, each of which is a deliberate act on that file, the same as pressing compile. A downloaded piece opened to read is never executed. A setting turns the automatic check off altogether. *(Designer's call.)*

### Completion, hover, definitions and outline

Completion offers, by position: commands and music functions after `\`, with their argument kinds from the signature; markup commands inside markup; context names after `\new`, `\context` and `\set`; grob names and their properties after `\override` and `\revert`; context properties after `\set` and `\unset`; pitch names for the active `\language`; and the variables defined in the file and in the files it includes. Hover shows LilyPond's own description of a command, grob or property, read from the local cache. Go-to-definition jumps to a variable's assignment, in the same file or an included one. The outline lists variables, `\score`, `\book` and `\bookpart` blocks. *(Hover, definitions and the outline are the designer's call; completion was agreed.)*

Includes are followed by parsing each included file with the grammar, recursively, with a guard against include cycles. The index updates as files change.

### Command data from the installed LilyPond

LilyPond can describe itself. A Scheme program run inside a `.ly` file lists every music function with its signature and docstring, every markup command, grob, interface, property, context, translator, music type and event class. Measured on 2.26.0: 199 music functions, 194 markup commands plus 9 markup-list commands, 167 grobs, 172 interfaces, 484 backend properties, 43 contexts, 163 translators, 277 context properties, 131 music types and 122 event classes. Event classes come from a private binding, `(@@ (lily) all-event-classes)`, and may move between versions. The program must run inside a file rather than through `-e`, which runs before music functions and contexts exist. Each dump takes 0.19 to 0.31 s.

The server runs this program in the background on first start, writes the result to the user's cache folder keyed by LilyPond's version and path, and reads the cache on later starts, so it is ready in under 100 ms. The program itself is Swara's own MIT code. Its output contains LilyPond's GPL docstrings, so it stays in the cache and is never committed, bundled or uploaded. *(Designer's call.)*

### Formatting

The formatter changes spacing only. It indents the contents of `{ }`, `<< >>` and `\with { }` blocks, puts one space between notes and other items, and puts spaces around bar checks `|`. It never changes a note, a duration, an octave mark, a comment, the text of a string or the position of a line break. Embedded Scheme is only re-indented as a block. *(Agreed with the owner.)*

Every result is checked before it is applied: the server re-parses the new text and compares the sequence of non-whitespace tokens with the original. If anything other than whitespace differs, or the new tree has more errors, the formatter refuses and leaves the text alone. It also refuses on a file that already has syntax errors, because spacing around a broken construct is guesswork; the save still happens. Indent width comes from the editor's settings, which LSP sends with each formatting request, and `swara format` takes `--indent`. *(Agreed contract; refusing on broken files and the indent source are the designer's call.)*

The formatter is reachable two ways: as LSP formatting, which the plugin runs on save, and as `swara format`, which conform.nvim can run instead. When conform already has a formatter for LilyPond, the plugin leaves on-save formatting to it, so a file is never formatted twice. A setting turns tidy-on-save off. *(Designer's call.)*

## The Neovim plugin

### Loading

The plugin finds the `swara` program on `PATH` first, otherwise uses the copy it downloaded. It asks `swara info` where the grammar library is, loads it under the language name `lilypond`, and starts Tree-sitter on every LilyPond buffer itself, because Neovim does not start highlighting for a language on its own. It ships the queries from the grammar repository, copied at each release from the pinned grammar commit, with a test that fails if the copies drift. *(Designer's call.)*

Neovim core uses `highlights.scm`, `injections.scm` and `folds.scm` but nothing reads `indents.scm`, so the plugin sets its own indent function, driven by the tree, so that a new line starts at the right depth. *(Designer's call.)*

nvim-treesitter is optional. A user who prefers `:TSInstall` can register the grammar with nvim-treesitter, and the README shows how. If some other grammar is already installed under the name `lilypond`, such as the existing one, Swara's queries would not match it, so the health check looks for a node type only Swara's grammar has and reports the conflict.

The server is started with Neovim's own `vim.lsp.config` and `vim.lsp.enable`.

### Building, viewing and previewing

Saving a `.ly` file makes the server build it in the background with LilyPond, writing the PDF and MIDI next to the source, which is where plain `lilypond file.ly` puts them. The first successful build opens the PDF in Skim with `open -g`, which keeps Skim behind the terminal so typing is not interrupted, and later builds update the same window. Whether that update comes from Skim watching the file or from the plugin re-sending the file to Skim is for the builder to measure. Skim's auto-reload preference must be on for the first route, and the health check reports it, along with the command that turns it on, rather than changing the setting itself. On other platforms the PDF opens in the system's default viewer. *(Owner: Skim opens automatically. Placement next to the source and `open -g` are the designer's call.)*

The in-editor preview renders the current file to a PNG image with LilyPond and draws it inside Ghostty with the image support in snacks.nvim when that plugin is present. Without it, the image opens in the system viewer. *(Owner: preview on a key. The rendering route is the designer's call.)*

### Playback

Playback runs FluidSynth on the MIDI file from the last build with a General MIDI soundfont. The plugin uses the soundfont set in its options, otherwise MuseScore 4's bundled `MS Basic.sf3` (MIT licence) when MuseScore is installed. Starting FluidSynth with that soundfont and `-o synth.dynamic-sample-loading=1` takes 1.1 s, against 5.1 s without the option. After the first play, FluidSynth stays running with the soundfont loaded so later plays start in under 0.2 s. How a running FluidSynth is handed a new file, through its command shell or by another route, is unproven and for the builder to settle; if no warm route works, each play starts afresh and the speed table records the miss. A stop command ends playback. *(Owner: FluidSynth for quick play. Soundfont choice and keeping it warm are the designer's call.)*

MuseScore, on a key, opens the MIDI file for slower practice with its tempo and loop controls.

### Commands and keys

`:Swara` takes subcommands: `new`, `build`, `open`, `preview`, `play`, `stop` and `musescore`. The plugin sets buffer-local keys in LilyPond buffers only, all under the local leader key (`<localleader>`, which is `\` unless the user's configuration changes it), and a setting turns them off. *(Designer's call.)*

| Key | What it does |
| --- | --- |
| `<localleader>o` | Opens the PDF in Skim, or brings it back if closed |
| `<localleader>v` | Shows the in-editor preview image |
| `<localleader>p` | Plays the file through FluidSynth |
| `<localleader>s` | Stops playback |
| `<localleader>m` | Opens the file's MIDI in MuseScore |
| `<localleader>b` | Builds now without saving first |

### Health check

`:checkhealth swara` reports the `swara` program's version and whether it matches the plugin's, where it came from (`PATH` or download), the grammar's version and whether a conflicting `lilypond` grammar is loaded, the LilyPond version, whether the command cache is built, FluidSynth and the soundfont, MuseScore, Skim and its auto-reload setting. Without LilyPond, highlighting and the instant checks still work, and the LilyPond check, build, preview and playback are switched off with a single message.

## Lessons: templates, the sargam line and Bengali lyrics

`:Swara new guitar <name>`, `:Swara new piano <name>` and `:Swara new song <name>` create `<name>.ly` in the current folder from a template, and copy the include files it needs (`swara-sargam.ily`, and for songs `swara-lyrics.ily`) beside it unless a copy is already there. An existing copy is never overwritten, so a lesson folder keeps working even if the user edits it. *(Template folder and the no-overwrite rule are the designer's call.)*

- The **guitar** template has a treble staff written an octave above sounding pitch (`treble_8`), a tablature staff with standard tuning `<e, a, d g b e'>`, letter note names under each, and the sargam line.
- The **piano** template has a two-staff piano system with letter names and the sargam line.
- The **song** template has a melody, chord names and lyrics attached with `\lyricsto`, the lyrics read from a separate `<name>-lyrics.ily` file and typed in English letters, as described below. Two lyric lines print under the melody: the Bengali script, and beneath it the same words as typed, in smaller type. One setting at the top of the template, `lyricLines`, chooses `both`, `script` or `roman`. *(Owner: both lines with a switch. Smaller type for the typed line is the designer's call.)*

Templates use `\language "english"`, so that sharps and flats are written `cs` and `bf`, matching English-language theory lessons rather than LilyPond's default Dutch names (`cis`, `bes`). *(Designer's call.)*

`swara-sargam.ily` is written fresh under MIT. It supplies a note-name function for LilyPond's `NoteNames` context, which LilyPond calls with each note's pitch and which may return formatted text. The function maps each note by its distance in semitones from a movable Sa, set per piece, to the twelve swaras, and draws them in Bhatkhande notation: komal (flat) swaras underlined, tivra (sharp) Ma with a vertical stroke above, a dot below for the lower octave and a dot above for the upper. Bengali is the default script (সা রে গা মা পা ধা নি), with Devanagari and Latin as options. A test on 2026-10-24 rendered this correctly in the PDF with the Kohinoor Bangla font, vowel signs in the right order. Two refinements came out of that test: র already carries a dot below, so a lower-octave রে shows two dots, and a dot above can be misread as the nasal sign anusvara, so the octave dots get one staff-space of padding.

Bengali stays out of lesson sources because the terminal draws it badly. Tested in Ghostty 1.3.1: a consonant with a vowel sign draws correctly, but joined letters such as ক্ষ and স্ত্র fall apart, the reph in র্ক lands in the wrong place, and Neovim's cursor and highlighting drift on lines holding Bengali. The only Bengali on disk is inside the two include files, which are correct even where the terminal misdraws them, and the PDF shows everything truly.

### Lyrics from English letters

Song lyrics are typed in English letters and appear in Bengali script in the PDF, with the typed spelling printed beneath unless the template's switch turns it off. *(The owner chose conversion from English letters, and both lines.)* The typed line is there for a reader who does not read Bengali, such as a teacher; the Bengali line is there because the English-letter spelling is a typing code rather than a way of reading. The lyrics file therefore stays plain ASCII text, which the terminal draws perfectly, Neovim's cursor stays aligned, and the grammar, formatter and diagnostics never meet Bengali in a source line.

The conversion happens inside LilyPond, not in the editor. `swara-lyrics.ily`, written fresh under MIT, defines a music function, `\transliterate`, that takes a script name and is wrapped around a block of lyrics; `\bangla` is its shorthand for Bengali. While LilyPond engraves, it converts the text of each lyric syllable from English letters to the script and sets it in that script's font (for Bengali, Kohinoor Bangla on macOS and Noto Sans Bengali elsewhere). It works on a copy, so the same lyrics can also print as typed on the line beneath. Because the converter is an include file copied into the lesson folder, a song still builds with plain `lilypond`, with or without Swara. Lyrics without `\transliterate` are left as typed, so English songs need nothing special. *(Converting at engraving time inside LilyPond is the designer's call.)*

The converter is one engine and a letter table per script. *(The owner's decision.)* Most Indian scripts are built the way Bengali is: a consonant, a vowel sign after it, and clusters joined by a silent mark, so Devanagari (Hindi, Marathi), Gurmukhi, Gujarati, Odia, Telugu, Kannada and Malayalam each need only their own table, plus the few letters that change shape after a consonant, such as Bengali's য-ফলা. The first release ships the Bengali table alone, and adding a script later means adding a table and its test word list, not changing the engine. Tamil, which joins few letters and usually shows the silent mark, and Urdu, which is written right to left in Arabic script, would need more than a table and are not planned.

```lilypond
songLyrics = \lyricmode { a -- mi tO -- may bha -- lO -- ba -- si }

% In the song template: the lines that lyricLines asks for.
\swaraLyrics "melody" \songLyrics

% By hand: the Bengali line alone.
\new Lyrics \lyricsto "melody" { \bangla \songLyrics }
```

The song template calls `\swaraLyrics`, which prints the converted line, the typed line or both, as `lyricLines` says. Written by hand, the braces after `\lyricsto` are required: LilyPond 2.26.0 rejects `\lyricsto "melody" \bangla ...` with a syntax error, because `\lyricsto` takes its lyrics only inside braces. *(Measured on 2026-10-24.)*

The spelling follows the style of Avro phonetic typing, which most Bengali speakers who type in English letters already know. For example, `ami` gives আমি and `bhalO` gives ভালো: lower-case `o` is the inherent vowel and capital `O` is the ো sign, a vowel after a consonant becomes its vowel sign, a vowel at the start of a syllable takes its full form, and consonants written together join with the virama (হসন্ত) automatically, so `kSh` and `str` give ক্ষ and স্ত্র. Each syllable is converted on its own, so LilyPond's `--` hyphens between syllables are unaffected. The rules are written fresh from that spelling convention; no code or rule table is copied from Avro Keyboard or any other implementation. *(The Avro-style spelling is the designer's call.)*

Correctness is checked two ways. A test file of words with their expected Bengali runs through LilyPond in CI and compares the output text. And a one-page sample sheet of common song words, engraved to PDF, goes to the owner to read, because whether a conversion is right is a judgement a native reader makes faster and better than any test. *(Designer's call.)*

## Release plan

The first release is complete: every goal above ships together, on all the platforms listed under Constraints, and nothing is cut to ship sooner. *(The owner's decision.)* A speed target that is missed does not hold the release back; the release notes record it, as the speed section says.

Inside the build, two measurements come first because their results shape later work: proving or rejecting the way to skip engraving, and measuring the existing grammar's baseline on the corpus. The rest of the order is the builder's.

## Installing

The plugin installs in the usual way through a plugin manager. On first opening a LilyPond file with no `swara` on `PATH`, it downloads the release archive for the platform that matches its own version, checks the archive's SHA-256 checksum against the release's `SHA256SUMS` file, and unpacks the program and grammar library into Neovim's data folder. It says so in one line, and highlighting starts as soon as the download finishes. Offline, it says so once and the buffer stays plain text. *(Owner: plugin download. Details are the designer's call.)*

Each release attaches one archive per platform, holding both `swara` and the compiled grammar: macOS on Apple silicon and on Intel, Linux on x86-64 and ARM, and Windows on x86-64. They are built by GitHub Actions on a version tag. *(Designer's call.)*

Building from source needs Rust. A Cargo workspace builds both the `swara` program and a small shared-library crate that exposes the grammar for Neovim, so one `cargo build --release` produces everything a plugin manager's build hook needs. *(Designer's call.)*

The Homebrew route is a formula in a tap repository, `anirbanchakraborty-dev/homebrew-tap`, installed as `brew install anirbanchakraborty-dev/tap/swara`. It builds from the release source with Rust as a build dependency and installs the program and the grammar library. That tap repository does not exist yet; creating it is a public action confirmed with the owner at build time.

## Speed targets

The owner approved these targets. Each has an automated timing test on the primary development Mac, an Apple-silicon MacBook Pro, and each release's notes record any target missed.

| What | Target | How it is measured |
| --- | --- | --- |
| Colours after a keystroke | under 16 ms | Headless Neovim: time from a buffer edit to the reparsed tree, on the 100-bar test song |
| Instant checks after a keystroke | under 50 ms | A test LSP client times from sending an edit to receiving diagnostics |
| LilyPond check after typing stops | under 0.6 s, up to about 100 bars | Same client, from the last edit to diagnostics from LilyPond |
| Tidy | under 10 ms | `swara format` and LSP formatting on the 100-bar song |
| PDF in Skim after save | about 0.3 s for a lesson, about 1.5 s for a 100-bar song | Save to PDF written; LilyPond's own engraving time is the floor |
| Completion | under 20 ms | Request to response, cache loaded |
| Server ready | under 100 ms | Start to answering `initialize`, cache present; the cache builds in the background otherwise |
| Play | about 1.5 s first time, under 0.2 s after | Command to first sound |

The 0.6 s LilyPond-check target is tight by construction: about 0.33 s of pause plus a 0.24 s run. It holds only if engraving can be skipped for files with `\layout`.

### What the targets rest on

Measured on 2026-10-24 with `hyperfine`, five runs each, `LC_ALL=C`, LilyPond 2.26.0, on the 100-bar test song (a treble staff, a tablature staff, two note-name lines and lyrics) and a 4-bar exercise.

| Measurement | Time |
| --- | --- |
| 4-bar exercise to PDF | 261 ms |
| 100-bar song to PDF | 1,493 ms |
| 100-bar song with `-dno-print-pages` | 910 ms |
| 100-bar song with no `\layout` (sound only) | 237 ms |
| LilyPond fixed start-up | about 155 ms, about 74 ms of it starting Guile |
| Each extra file in one LilyPond run | about 68 ms |
| Command-data dump | 0.19 to 0.31 s |
| FluidSynth start with `MS Basic.sf3` | 1.1 s with dynamic sample loading, 5.1 s without |

## Measuring "better"

The new grammar is compared with `nwhetsell/tree-sitter-lilypond` on the same corpus, and the first step of the build measures that grammar's baseline before any new grammar code exists.

The corpus is LilyPond's regression tests from the v2.26.0 source, the `.ly` init files Homebrew ships with LilyPond, pieces from the Mutopia Project, snippets from the LilyPond Snippet Repository, and the owner's own lessons. The first two are GPL and the lessons are private, so the corpus is fetched to a local folder by a script and never committed; only the results are. *(Designer's call.)*

"Better" means two things together: fewer files that parse with any error, and parsing at least as fast. *(Agreed with the owner.)* Three refinements keep that measure honest, because the new grammar is built to report errors wherever LilyPond does and a naive count would reward a grammar that hides them. *(All three are the designer's call.)*

- **Only files LilyPond 2.26 accepts count towards "fewer errors".** The corpus script first compiles every file with LilyPond 2.26. Files written for older versions are upgraded with LilyPond's own `convert-ly` in the local copy and kept if they then compile. Files that still fail are set aside and reported separately. On the files that count, any parse error from either grammar is a false error.
- **Caught errors count too.** A second, smaller corpus of deliberately broken files, each with one known error, is part of the verdict: a grammar must report each error within a line of where LilyPond reports it. A grammar that wins on clean files by swallowing errors loses here.
- **Speed is compared like for like.** `tree-sitter parse --stat -q` reports success percentage and bytes per millisecond, but it runs one grammar and does not follow injections. The existing grammar hands Scheme to a second grammar by injection, so its time is measured with a small harness that also parses every injected Scheme region with that second grammar, and the two times are added.

If the new grammar does not beat the old one on all of this, the README says so plainly. The 100-bar test song used for this and for the speed table is produced by a script committed in the repository, so anyone can regenerate it. *(Designer's call.)*

## Licensing in practice

- All Swara code, queries, templates and `swara-sargam.ily` are MIT.
- LilyPond's docstrings reach the user only through the local cache built from their own LilyPond.
- LilyPond's regression tests are used as a local test corpus and never redistributed.
- Grammar queries contain no LilyPond word lists.
- `cargo-deny` checks crate licences and advisories in CI. *(Designer's call.)*

## Risks

- **Skipping engraving may not work.** Then the LilyPond check takes about 0.9 s on a 100-bar song with `\layout`, missing the 0.6 s target. Shorter lessons stay well inside it.
- **Grammar fidelity.** LilyPond's syntax depends on run-time information a grammar cannot have, so some valid files will show errors. The comparison will show how many, and the result may be that the new grammar is not better on every count.
- **Warm playback is unproven.** If no route keeps FluidSynth warm, each play takes about 1.1 s.
- **Name clash.** Two grammars named `lilypond` cannot both be loaded. If nvim-treesitter adds the existing grammar under that name, users of both see a conflict; the health check reports it.
- **GPL leakage.** A careless commit of the command cache, or of a corpus file, would put GPL text in an MIT repository. The cache lives outside the repository and the corpus folder is ignored by git.
- **LilyPond version drift.** Private bindings, such as the event-class list, and message formats can change between versions. The cache is keyed by version and the parser is tested against recorded LilyPond output.
- **Stale crate.** `lsp-types` has had no release since 2024. If it falls behind something Swara needs, the types can be vendored or replaced.
- **Spelling ambiguity in lyrics.** English letters cannot say everything Bengali script says, which is why Avro-style spelling uses capitals such as `O` for distinctions. Words typed loosely will sometimes convert to the wrong spelling, which the PDF and preview show at once. The sample sheet catches systematic mistakes before release.
- **One long release.** Shipping everything together means nothing is usable for lessons until the whole build is done.

## Calls made by the designer

These were not discussed with the owner. Each is marked where it appears above.

- One program, `swara`, with `lsp`, `format` and `info` subcommands.
- `lsp-server` and `lsp-types`, no async runtime, parsing on the main thread, LilyPond in child processes.
- A single grammar including Scheme and nested `#{ #}`, faithful to LilyPond 2.26's quirks, with mode switches recognised by command name and argument counts left to the server.
- The grammar consumed by `swara` as a pinned git dependency, and its queries copied into the plugin with a drift test.
- UTF-8 positions.
- The instant-check list; diagnostics merged into one published list; a pause of about a third of a second; Neovim's send delay lowered to about 10 ms; stale LilyPond runs killed and their results dropped; engraving warnings kept and moved until the next save; `.ily` files given instant checks plus results attributed from including files.
- Fresh LilyPond process per check, warm loop only if measured necessary; `LC_ALL=C`, `-l WARNING`, absolute paths, unsaved text on standard input.
- Opening a file never runs LilyPond; the first run follows typing, saving or an explicit build. A setting turns the automatic check off.
- The server, not the plugin, runs the on-save build, so engraving warnings join the same diagnostics list.
- Hover, go-to-definition and outline in the first release.
- Command data cached per LilyPond version in the user's cache folder.
- The formatter refuses on files with syntax errors and takes indent width from the editor; on-save formatting defers to conform when conform handles LilyPond.
- PDF and MIDI next to the source; Skim opened with `open -g`; the health check reports Skim's auto-reload setting rather than changing it.
- In-editor preview through snacks.nvim's image support when present.
- MuseScore's `MS Basic.sf3` as the default soundfont; FluidSynth kept warm.
- Buffer-local keys under `<localleader>`, listed above, with a setting to turn them off.
- Templates written to the current folder, the include never overwritten, `\language "english"` in templates.
- Bengali lyrics converted inside LilyPond by an MIT include, `swara-lyrics.ily`, at engraving time, so lyric files stay ASCII and songs build with plain `lilypond`.
- The typed lyric line set in smaller type beneath the script line.
- Avro-style phonetic spelling for lyrics, rules written fresh, checked by a CI word list and a sample sheet the owner reads.
- One release archive per platform holding program and grammar; a Cargo workspace whose source build produces both.
- Corpus fetched locally, never committed; `cargo-deny` in CI.
- The "better" verdict counts only files LilyPond 2.26 accepts (after `convert-ly` where needed), includes a broken-file corpus that checks errors are caught at the right line, and times the existing grammar with its injected Scheme grammar included. This refines the comparison plan the owner approved without changing its two tests.
- The 100-bar test song is generated by a committed script.
- Neovim 0.12 as the minimum version.
- This document's final home: `docs/design/2026-10-24-swara-design.md` in the `swara` repository, once it exists.

## Left to the builder

- The grammar's node names, token boundaries and the external scanner's internals.
- How engraving is skipped, within the candidates above, or the fallback.
- How a running FluidSynth receives a new file.
- How Skim picks up a rebuilt PDF.
- The exact text of templates and messages.
- Crate layout inside the workspace, the CI matrix and the release workflow.
