# Swara grammar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tree-sitter grammar for LilyPond 2.26 that agrees with the compiler on every file the comparison corpus holds, covers embedded Scheme and `#{ #}` in one tree, ships highlight, fold, indent and locals queries free of GPL word lists, and beats or honestly reports against `nwhetsell/tree-sitter-lilypond`.

**Architecture:** `grammar.js` plus an external scanner in `src/scanner.c` for the tokens plain rules cannot express. Commands are parsed flat: a command and the expressions after it are siblings, because arity is known only at run time. One word token is aliased by context into `note_name`, `identifier` and the like, because Tree-sitter cannot lex two identical patterns differently. Rust integration tests in the grammar crate check agreement with LilyPond, incremental reparsing, termination on every prefix, query validity and speed.

**Tech Stack:** tree-sitter CLI 0.27.1 (ABI 15), C11 for the scanner, Rust 1.98 with `tree-sitter` 0.27.1 as a dev-dependency, LilyPond 2.26.0 for the agreement test.

**Spec:** `docs/design/2026-10-24-swara-design.md` in the `swara` repository, sections "The grammar" and "Measuring better".

**Plan order:** After `2026-10-24-swara-foundations.md`. The comparison corpus, the broken-file corpus and `cargo xtask compare` come from there.

## Global Constraints

Everything under Global Constraints in the foundations plan applies. In addition:

- Every corpus example is a complete file. LilyPond 2.26.0 must accept it, unless the example is marked `:error`, in which case LilyPond must report an error. The agreement test enforces this, so every syntax fact the grammar encodes is checked against the compiler rather than taken from memory.
- No LilyPond command names in queries beyond the grammar's own keywords. No `#any-of?` and no word-list `#match?`. The one allowed regex is the dynamics pattern `^\\\\[fmprsz]+$`.
- `src/parser.c`, `src/grammar.json` and `src/node-types.json` are committed and regenerated with `tree-sitter generate` in the same commit as any `grammar.js` change.
- Node names follow the contract below. A rename is a contract change: it updates `docs/nodes.md`, the queries and the server in the same release.

## Review Focus

- **Typing a file from scratch.** Every prefix of a file, including an open string, `#(` or `%{` at the end, must parse to completion without hanging or crashing the scanner. The test lives in the incremental task.
- **An unclosed `{` partway through a song.** The rest of the file must stay coloured as music: one missing `}` at the end, and every note keeps its kind. The test lives in the error placement task.
- **Windows line endings** from Frescobaldi or a shared lesson. A line comment must stop before `\r`, and nothing may show as an error. The test lives in the scaffold task.
- **Bengali in names and lyrics,** such as `গান = \relative { c' }` and a Bengali lyric syllable. Non-ASCII characters count as letters in identifiers and as word characters in lyrics. The tests live in the commands task and the lyric task.
- **A very large score** of 10,000 bars. Parse time must grow in step with size, and a one-character edit must reparse within the colour budget. The test lives in the incremental task.

## Node contract

The server and the queries depend on these names. The scaffold task writes this section to `docs/nodes.md`; the queries task adds a test that every name listed there exists in `src/node-types.json`.

- **File and text:** `source_file`, `line_comment`, `block_comment`, `string` (children `escape_sequence`), `number`, `fraction` (fields `numerator`, `denominator`), `version` (field `value: string`), `include` (field `path: string`), `language` (field `name: string`).
- **Words and commands:** `command` is a leaf for a backslash word, and includes the `\"any text"` form. `identifier` is a word in a position that names something. `assignment` has fields `name` (`identifier`, `string` or `dotted_name`) and `value`. When the value is flat, such as a number and its unit command, or a command and its arguments, `value` holds each node in order.
- **Blocks:** `header_block`, `paper_block`, `layout_block`, `midi_block`, `context_definition` (`\context { }` inside `\layout` or `\midi`), `score_block`, `book_block`, `bookpart_block`.
- **Contexts and properties:** `new_context` (`\new` or `\context` in music, fields `type: identifier`, optional `name`, optional `with: with_block`; the music follows as the next sibling), `with_block`, `context_modification` (`\consists`, `\remove`, `\accepts`, `\denies`, `\alias`, `\name`, `\type`, `\defaultchild`, `\description` with its argument), `override` and `set` (fields `path: property_path`, `value`), `revert` and `unset` (field `path`), `property_path`.
- **Keyword constructs:** `repeat` (fields `type: identifier`, `count: number`), `alternative`, `tempo`, `change`.
- **Music:** `sequential_music` (`{ }`, and `\sequential { }`), `simultaneous_music` (`<< >>`, and `\simultaneous { }`), `note` (field `pitch: pitch`, whose children are `note_name` and an optional `octave`; then optional `accidental_mark`, optional `octave_check`, optional field `duration`), `rest`, `skip`, `multi_measure_rest`, `chord_repetition`, `chord` (`< >` holding notes, field `duration`), `duration` (a number or `\breve`, `\longa`, `\maxima`, with dots and `*` multipliers). A lone duration in music (`c4 8 8`) is a `duration` node in the sequence.
- **Post-events** are siblings of the event they follow, never children, because a command such as `\f` or `\fermata` can itself be a post-event and is parsed flat: `tie`, `slur_start`, `slur_end`, `phrasing_slur_start`, `phrasing_slur_end`, `beam_start`, `beam_end`, `ligature_start`, `ligature_end`, `hairpin` (`\<` and `\>`), `hairpin_end` (`\!`), `articulation` (a direction `-`, `^` or `_` with a shorthand or a command), `fingering`, `string_number`, `tremolo`, `text_script` (a direction with a string or markup), `bar_check`, `voice_separator`.
- **Modes:** `note_mode`, `drum_mode`, `chord_mode`, `figure_mode`, `lyric_mode`, `add_lyrics`, `lyrics_to` (field `voice`), `markup`, `markup_list`.
- **Mode content:** `chord_symbol` (fields `root`, optional `duration`, optional `modifiers: chord_modifiers`, optional `inversion`, optional `bass`), `figure_group`, `figure`, `lyric_word`, `lyric_hyphen`, `lyric_extender`, `markup_word`, `markup_block`. Inside lyric mode, braces produce `sequential_music`.
- **Scheme:** `scheme` (prefix `#` or `$`, then one datum), `scheme_list`, `scheme_vector`, `scheme_quote`, `scheme_quasiquote`, `scheme_unquote`, `scheme_unquote_splicing`, `scheme_symbol`, `scheme_number`, `scheme_string`, `scheme_boolean`, `scheme_character`, `scheme_keyword`, `scheme_comment`, `scheme_block_comment`, `scheme_datum_comment`, `embedded_lilypond`.

Two consequences of parsing flat are accepted and documented in `docs/nodes.md`. A note-shaped word is a `note` even where LilyPond reads it as a word argument, as in `\clef treble`, and the server tells the two apart using signatures. An assignment whose value ends in a music function's last argument may absorb a following top-level music expression into its `value`; this changes no colour and raises no error.

---

### Task: Scaffold, comments, strings and the agreement test

**Files:**
- Create: `tree-sitter.json` (name `lilypond`, scope `source.lilypond`, file types `ly` and `ily`, `"highlights": "queries/highlights.scm"`, bindings `c` and `rust` only), `grammar.js`, `src/scanner.c`, `Cargo.toml`, `bindings/rust/lib.rs`, `bindings/rust/build.rs`, `docs/nodes.md`, `README.md`
- Create: `test/corpus/comments.txt`, `test/corpus/strings.txt`, `test/corpus/toplevel.txt`
- Create: `tests/common/mod.rs`, `tests/lilypond_agrees.rs`
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: the Rust crate `tree-sitter-lilypond` 0.1.0 exporting `pub const LANGUAGE: tree_sitter_language::LanguageFn`, `NODE_TYPES`, `HIGHLIGHTS_QUERY`, `FOLDS_QUERY`, `INDENTS_QUERY` and `LOCALS_QUERY` (the last four start as empty files under `queries/`).
- Produces: in `tests/common/mod.rs`, `pub struct CorpusCase { pub file: String, pub name: String, pub input: String, pub expects_error: bool, pub skip: bool }` and `pub fn corpus_cases() -> Vec<CorpusCase>`, which reads every `test/corpus/*.txt`.
- Produces: the external token list, fixed in this order for the rest of the plan: `block_comment`, `_embedded_start`, `_embedded_end`, `scheme_block_comment`, `_lyric_word`, `_markup_word`, `_error_sentinel`. The scanner keeps one serialised byte, the `#{ #}` nesting depth.

- [ ] **Generate the skeleton.** Write `tree-sitter.json`, then run `tree-sitter init` in `Projects/tree-sitter-lilypond`. Expected: `bindings/c` and `bindings/rust` exist, and no other bindings. Add the three missing query constants to `bindings/rust/lib.rs` beside `HIGHLIGHTS_QUERY`.
- [ ] **Write the failing corpus tests.**
  - `comments.txt`: a line comment, `%` alone at the end of a line, `%%{ still a line comment`, a block comment over three lines, a block closed by `%%%}`, a block comment holding `%{` (no nesting), an unterminated block comment (`:error`), and a two-line file with CRLF endings whose expected tree has two `line_comment` nodes and no error.
  - `strings.txt`: `\header { title = "a \"b\" c" }` with an `escape_sequence`, a string spanning two lines, and an unterminated string (`:error`).
  - `toplevel.txt`: `\version "2.26.0"`, `\include "x.ily"` (marked skip for the agreement test: its first line is `% agree: skip, needs the include`), and `\language "english"`.
- [ ] **Write the agreement test** in `tests/lilypond_agrees.rs`. For each `CorpusCase` that is not skipped, it writes the input to a temporary folder and runs `LC_ALL=C lilypond -l ERROR -dno-print-pages -o <tmp>/out <tmp>/case.ly`, using a thread pool. It asserts exit 0 when `expects_error` is false and non-zero when it is true, and prints the corpus file, the case name and LilyPond's stderr on failure. It skips, with one printed line, when `lilypond --version` is absent or does not report 2.26.0.
- [ ] **Run them to see them fail.** Run `tree-sitter generate && tree-sitter test`. Expected: the new cases FAIL. Run `cargo test --test lilypond_agrees`. Expected: PASS, because it checks only that LilyPond agrees with the examples.
- [ ] **Implement** `source_file`, `line_comment` (stops before `\r` and `\n`), `string`, `escape_sequence`, `number`, `version`, `include` and `language` in `grammar.js`, and `block_comment` in `src/scanner.c`, which closes at the first `%}` and runs to the end of the file when unterminated. Set `extras` to whitespace and both comment kinds, and `word: $ => $.command`.
- [ ] **Run them to see them pass.** Run `tree-sitter generate && tree-sitter test && cargo test`. Expected: PASS.
- [ ] **Write CI.** On macOS and Ubuntu, CI runs `tree-sitter generate` and fails on `git diff --exit-code src/`, then runs `tree-sitter test` and `cargo test`. The Ubuntu job installs LilyPond 2.26.0 from the official Linux x86-64 release archive on GitLab, cached by version, so the agreement test runs in CI.
- [ ] **Commit:** `feat: scaffold the grammar with comments, strings and the agreement test`.

### Task: Embedded Scheme and the scanner

The tests in this task settle the reader's rules for `%` inside `#{ #}` by asking LilyPond first. Before encoding a case, write it as a corpus example and run the agreement test; LilyPond's verdict decides whether it is `:error`.

**Files:**
- Modify: `grammar.js`, `src/scanner.c`
- Create: `test/corpus/scheme.txt`, `test/corpus/embedded.txt`

**Interfaces:**
- Produces: the `scheme` node and every `scheme_*` node in the contract, plus `embedded_lilypond`. `scheme` is valid wherever a value, a music expression, a markup argument or a post-event argument may appear. `embedded_lilypond` is valid as a Scheme datum (`#(list #{ c #})`, `##{ c #}`) and directly after LilyPond's own `#` (`x = #{ c #}`).

- [ ] **Write the failing corpus tests.**
  - `scheme.txt` covers: `##t`, `##f`, `#'sym`, `#'(1 . 2)`, `#(define x 1)`, `$(+ 1 2)`, `##:key`, `##\a`, `##(1 2)` as a vector, `#"str"` with an escape, `#-1.5`, `#1/2`, `#'1+` (a symbol), and a datum comment `#(list #;(skipped) 1)`.
  - It also covers `; line` and nested `#| a #| b |# c |#` inside a list.
  - The `#red}` quirk is a pair of tests. `{ c \override NoteHead.color = #red }` parses with `(scheme (scheme_symbol))`. `{ c \override NoteHead.color = #red}` is `:error`, because `red}` is one symbol and the brace never closes.
- [ ] **Write the failing embedded tests.** `embedded.txt` holds:
  - `x = #{ c d #}` and `x = ##{ c #}`;
  - three-deep nesting `x = #(list #{ c #(list #{ d #}) #})`;
  - `#{ \markup { 50% off } #}` on one line, followed on the next line by `#}`;
  - `#{ c % comment #}` on one line, followed on the next by `#}`;
  - a string holding `%` inside `#{ #}`.
- [ ] **Run the agreement test on the new examples alone.** Run `cargo test --test lilypond_agrees -- --nocapture`. Mark `:error` exactly where LilyPond reports one, then fill in each expected tree to match. Record the reader rules the verdicts reveal in `docs/nodes.md`, under "Reader rules inside `#{ #}`".
- [ ] **Run the tree tests to see them fail.** Run `tree-sitter test`. Expected: the new cases FAIL.
- [ ] **Implement.** Scheme datums are grammar rules; symbols, numbers, booleans and characters are regex tokens, with the number pattern taking precedence over the symbol pattern at equal length. A Scheme symbol excludes whitespace, `(`, `)`, `"`, `;`, `'`, `` ` `` and `,`, so `}` belongs to it. The scanner emits `_embedded_start` and `_embedded_end`, raising and lowering the depth byte, and emits `scheme_block_comment` with nesting. While the depth is above zero, the scanner applies the reader rules recorded above.
- [ ] **Run them to see them pass.** Run `tree-sitter generate && tree-sitter test && cargo test`. Expected: PASS.
- [ ] **Commit:** `feat: parse embedded Scheme and nested #{ #}`.

### Task: Note-mode music

**Files:**
- Modify: `grammar.js`
- Create: `test/corpus/music.txt`, `test/corpus/post_events.txt`

**Interfaces:**
- Produces: `sequential_music`, `simultaneous_music`, `note`, `pitch`, `note_name`, `octave`, `accidental_mark`, `octave_check`, `duration`, `rest`, `skip`, `multi_measure_rest`, `chord_repetition`, `chord`, every post-event node, and the hidden rule `_music`, which later tasks extend.
- Produces: the hidden word token `_word` (letters or any non-ASCII character, with single `-` or `_` between letters, no digits), aliased to `note_name` here and to `identifier` in the next task.

- [ ] **Write the failing corpus tests.**
  - `music.txt` covers:
    - `{ c4 d8. e16 f2*2/3 g\breve a\longa b\maxima }`;
    - octaves `{ c' d,, e }`, accidentals `{ cis! des? }` and an octave check `{ c='4 }`;
    - rests and skips `{ r4 s8 R1*4 q4 }`, and a word that starts like a rest, `{ res4 }`, as a note;
    - a lone duration `{ c4 8 8 }`, and chords `{ <c e g>4 <c-1 e-3>2 }`;
    - nesting `<< { c } \\ { e } >>`, and `\sequential { c }` and `\simultaneous { c e }`.
  - `post_events.txt` covers:
    - `{ c4~ c( d) e\( f\) g[ a] b\[ c\] }`;
    - `{ c\< d\> e\! }`, `{ c-. d-^ e-> f-_ g-- a-! b-+ }`;
    - `{ c^"up" d_\markup { down } e-\fermata f-1 g\2 a:16 b4 | }`.
- [ ] **Run them to see them fail.** Run `cargo test --test lilypond_agrees` (expected: PASS, which confirms the examples are valid LilyPond), then `tree-sitter test` (expected: the new cases FAIL).
- [ ] **Implement.** Strings beat regexes at equal length, which makes `r`, `s`, `R` and `q` rests and repeats while `res` stays a note. Directions belong to `articulation`, `fingering` and `text_script`. Post-events are siblings in `_music` sequences.
- [ ] **Run them to see them pass.** Run `tree-sitter generate && tree-sitter test && cargo test`. Expected: PASS.
- [ ] **Commit:** `feat: parse note-mode music and post-events`.

### Task: Commands, assignments, keyword constructs and blocks

**Files:**
- Modify: `grammar.js`
- Create: `test/corpus/commands.txt`, `test/corpus/assignments.txt`, `test/corpus/contexts.txt`, `test/corpus/blocks.txt`

**Interfaces:**
- Produces: `command`, `identifier`, `assignment`, `dotted_name`, every block node, `new_context`, `with_block`, `context_modification`, `override`, `set`, `revert`, `unset`, `property_path`, `repeat`, `alternative`, `tempo` and `change`.

- [ ] **Write the failing corpus tests.**
  - `commands.txt` covers:
    - flat arguments `{ \relative c' { c d } \time 3/4 \key g \major \clef treble }`, where `treble` is a `note`;
    - `\foo2` as `command` then `duration`, and keyword boundaries `{ \newSpacingSection }` as one `command`;
    - the `\"any text"` reference.
  - `assignments.txt` covers:
    - `melody = { c }`, `"my tune" = { c }` and `verse.1 = \lyricmode { a }`;
    - a Bengali name `গান = \relative { c' }` used later as `\গান`;
    - `\paper { indent = 10\mm }`, whose value is `number` then `command`.
    - Use whichever form of `\verse.1` reference LilyPond's verdict accepts; write the probe first.
  - `contexts.txt` covers:
    - `\new Staff = "up" \with { \consists "Ambitus_engraver" \remove "Time_signature_engraver" } { c }`;
    - `\context Voice = "one" { c }`;
    - `\override Staff.TimeSignature.stencil = ##f`, the old form `\override Staff.TimeSignature #'stencil = ##f`, and `\set Staff.instrumentName = "Guitar"`;
    - `\revert Stem.details.beamed-lengths` and `\unset Staff.keyAlterations`;
    - `\once \override NoteHead.color = #red`, where `\once` is a `command`.
  - `blocks.txt` covers:
    - `\header { title = "T" }`, `\paper { #(set-paper-size "a4") }`;
    - `\layout { \context { \Staff \remove "Time_signature_engraver" } }`, `\midi { }`;
    - `\score { { c } \layout { } \midi { } \header { piece = "P" } }`, and `\book { \bookpart { \score { { c } } } }`;
    - `\repeat volta 2 { c } \alternative { { d } { e } }`, `\repeat unfold 4 c4`;
    - `\tempo 4 = 120`, `\tempo "Allegro" 4 = 100-120`, `\tempo "Andante"`;
    - `\change Staff = "up"` inside `\new Staff = "up" { }` and `\new Staff = "down" { }` siblings.
- [ ] **Run them to see them fail.** Run the agreement test (expected: PASS), then `tree-sitter test` (expected: FAIL).
- [ ] **Implement.** One `_word` token is aliased to `identifier` or `note_name` by position. The `=` that follows decides assignment, which is an LR(1) choice, with no second word token. Keywords are string tokens, and `word: $ => $.command` keeps `\newSpacingSection` whole.
- [ ] **Run them to see them pass.** Run `tree-sitter generate && tree-sitter test && cargo test`. Expected: PASS.
- [ ] **Commit:** `feat: parse commands, assignments, contexts and blocks`.

### Task: Lyric mode

**Files:**
- Modify: `grammar.js`, `src/scanner.c` (`_lyric_word`)
- Create: `test/corpus/lyrics.txt`

**Interfaces:**
- Produces: `lyric_mode`, `add_lyrics`, `lyrics_to`, `lyric_word` (aliased from `_lyric_word`), `lyric_hyphen` and `lyric_extender`.

- [ ] **Write the failing corpus tests.** `lyrics.txt` covers:
  - `\lyricmode { a4 -- mi tO -- may bha -- lO __ si }`;
  - a digit ending a word, `\lyricmode { ab4cd }`, which gives `lyric_word duration lyric_word`;
  - `--` inside a word, `\lyricmode { a--b }`, as one `lyric_word`, and `_` inside a word;
  - `%` inside a word, `\lyricmode { 50%off }`, as one word, while `\lyricmode { % comment` + newline + `a }` has a comment;
  - Bengali syllables `\lyricmode { আমি তোমায় }`;
  - `\addlyrics { a b }` after `{ c d }`, and `\addlyrics \words` with a `command` child that is not re-read as lyrics;
  - `\new Lyrics \lyricsto "melody" { a b }`, and `\lyricsto "melody" \lyricmode { a }`;
  - `\lyricmode { \set stanza = "1." a \skip 4 "quoted" #"scheme" }`.
- [ ] **Run them to see them fail.** Run the agreement test (expected: PASS), then `tree-sitter test` (expected: FAIL).
- [ ] **Implement `_lyric_word`** in the scanner. A word starts with any character other than `| * . = $ # { } " \`, whitespace or a digit, and continues until one of `$ # { } " \`, whitespace or a digit. A `%` mid-word is literal. A `%` at the start of a token is left to `line_comment`, and while the `#{ #}` depth is above zero the reader rules apply. `--` and `__` standing alone are the hyphen and extender tokens.
- [ ] **Run them to see them pass.** Run `tree-sitter generate && tree-sitter test && cargo test`. Expected: PASS.
- [ ] **Commit:** `feat: parse lyric mode`.

### Task: Markup mode

**Files:**
- Modify: `grammar.js`, `src/scanner.c` (`_markup_word`)
- Create: `test/corpus/markup.txt`

**Interfaces:**
- Produces: `markup`, `markup_list`, `markup_block` and `markup_word`. A `markup` holds `\markup`, then any commands and Scheme arguments, then one `markup_block`, `markup_word`, `string` or `scheme`. That rule is right for every markup command with one markup argument, and is documented as the flat-parse limit for commands with two.

- [ ] **Write the failing corpus tests.** `markup.txt` covers:
  - `\markup { \bold "x" word \line { a b } }`, `\markup \bold \italic foo`, and `\markup \fontsize #3 \bold "x"`;
  - `\markuplist { \column-lines { a } }`, and `\markup { 50% off }`, where `50%` is one `markup_word`;
  - `\markup { \score { { c } \layout { } } }`, which returns to note mode;
  - a markup as a post-event `{ c^\markup { up } }`, and in a header `\header { title = \markup { \bold T } }`.
- [ ] **Run them to see them fail.** Run the agreement test (expected: PASS), then `tree-sitter test` (expected: FAIL).
- [ ] **Implement `_markup_word`.** It is any run of characters other than `$ # { } " \` and whitespace, with the same `%` rule as lyric words.
- [ ] **Run them to see them pass.** Run `tree-sitter generate && tree-sitter test && cargo test`. Expected: PASS.
- [ ] **Commit:** `feat: parse markup mode`.

### Task: Chord, figure and drum modes

**Files:**
- Modify: `grammar.js`
- Create: `test/corpus/chords.txt`, `test/corpus/figures.txt`, `test/corpus/drums.txt`

**Interfaces:**
- Produces: `chord_mode`, `chord_symbol`, `chord_modifiers`, `figure_mode`, `figure_group`, `figure`, `drum_mode` and `note_mode`. Drum mode contains `note` nodes, because its names are runtime tables.

- [ ] **Write the failing corpus tests.**
  - `chords.txt` covers `\chordmode { c1:m7 d:7/fis e:sus4 f:9^7 g:maj7.11+ c/+e }`, `\chords { c2 g }` and `\new ChordNames \chordmode { c }`.
  - `figures.txt` covers `\figuremode { <6 4>4 <7+ 5->8 <_>4 <6\\> <[6 4]> }` and `\figures { <6> }`.
  - `drums.txt` covers `\drummode { bd4 sn hh8 hh }`, `\drums { bd }` and `\notemode { c }`.
- [ ] **Run them to see them fail.** Run the agreement test (expected: PASS), then `tree-sitter test` (expected: FAIL).
- [ ] **Implement.** `chord_modifiers` is one token after `:`, made of letters, digits, `.`, `+`, `-` and `^`. `/` introduces `inversion` and `/+` introduces `bass`.
- [ ] **Run them to see them pass.** Run `tree-sitter generate && tree-sitter test && cargo test`. Expected: PASS.
- [ ] **Commit:** `feat: parse chord, figure and drum modes`.

### Task: Queries, highlight tests and the query lint

**Files:**
- Create: `queries/highlights.scm`, `queries/folds.scm`, `queries/indents.scm`, `queries/locals.scm`
- Create: `test/highlight/music.ly`, `test/highlight/scheme.ly`, `test/highlight/lyrics_markup.ly`, `test/highlight/blocks.ly`
- Create: `tests/queries.rs`

**Interfaces:**
- Produces: the capture names the plugin and nvim-treesitter use. The highlight mapping:

| Nodes | Capture |
| --- | --- |
| all comment kinds | `@comment` |
| `string`, `scheme_string`, `markup_word`, `lyric_word`, `text_script` | `@string` |
| `escape_sequence` | `@string.escape` |
| `include` path | `@string.special.path` |
| `number`, `fraction`, `duration`, `scheme_number`, `figure` | `@number` |
| `note_name`, `scheme_symbol`, `chord_symbol` root and bass | `@variable` |
| assignment name | `@variable.member` |
| `octave`, `accidental_mark`, `octave_check`, `=` | `@operator` |
| `rest`, `skip`, `multi_measure_rest`, `chord_repetition` | `@constant.builtin` |
| `command`, first symbol of a `scheme_list` | `@function.call` |
| grammar keywords (`\new`, `\score`, `\override` and the rest) | `@keyword` |
| `\version`, `\include`, `\language` | `@keyword.import` |
| dynamics (regex above), `hairpin`, `hairpin_end` | `@attribute.builtin` |
| `articulation`, `fingering`, `string_number`, `tremolo` | `@attribute` |
| `new_context` type, `context_definition` command, `chord_modifiers` | `@type` |
| `property_path` | `@property` |
| `scheme_boolean` / `scheme_character` / `scheme_keyword` | `@boolean` / `@character` / `@string.special.symbol` |
| ties, slurs, beams, ligatures, `#`, `$`, quote marks, `#{`, `#}` | `@punctuation.special` |
| `{ } << >> < > ( )` | `@punctuation.bracket` |
| `bar_check`, `voice_separator`, `lyric_hyphen`, `lyric_extender` | `@punctuation.delimiter` |

- Produces: `@fold` on every block and music node, `with_block`, `markup_block`, multi-line `scheme_list`, `embedded_lilypond` and `block_comment`. `indents.scm` uses nvim-treesitter's captures: `@indent.begin` on the same nodes, and `@indent.branch` plus `@indent.end` on `}`, `>>`, `)` and `#}`. `locals.scm` has `@local.scope` on `source_file` and `@local.definition.var` on assignment names.

- [ ] **Write the failing tests.**
  - The highlight files carry assertions in `%` comments, for example `c'4 -. ~` followed by `% <- variable` and `%    ^ attribute`, covering every row of the mapping.
  - `tests/queries.rs` holds four tests. `queries_compile` checks that `Query::new` succeeds for all four queries, which proves every node and anonymous token they name exists. `captures_are_allowed` checks every capture name against the mapping's list. `no_word_lists` checks that there is no `#any-of?` and no `#match?` other than the dynamics pattern. `contract_nodes_exist` checks every name in `docs/nodes.md` against `NODE_TYPES`.
- [ ] **Run them to see them fail.** Run `tree-sitter test && cargo test --test queries`. Expected: FAIL.
- [ ] **Write the four query files.**
- [ ] **Run them to see them pass.** Run `tree-sitter test && cargo test`. Expected: PASS.
- [ ] **Commit:** `feat: add highlight, fold, indent and locals queries`.

### Task: Incremental reparsing, termination and speed

**Files:**
- Create: `tests/incremental.rs`, `tests/prefixes.rs`, `tests/speed.rs`
- Create: `test/fixtures/song-100.ly` (copied from `swara/bench/song-100.ly`), `test/fixtures/everything.ly` (every construct from the corpus in one file, about 3 KB)

**Interfaces:**
- Produces: `pub fn edit_and_reparse(src: &str, at: usize, delete: usize, insert: &str) -> (String, String)` in `tests/common/mod.rs`. It returns the S-expression of the incremental reparse and of a fresh parse of the edited text.

- [ ] **Write the failing tests.**
  - `incremental_matches_fresh` runs over `song-100.ly` and `everything.ly`. At 500 deterministic positions spread evenly, including inside strings, comments, Scheme and `#{ #}`, it inserts and deletes one character, then inserts `{`, `"`, `%{`, `#{` and `#(`. Each time it asserts the two S-expressions are equal.
  - `every_prefix_terminates` parses each character-boundary prefix of `everything.ly` with a progress callback that cancels after 200 ms, and asserts that every parse completes.
  - `speed_targets` is `#[ignore]` and runs in release. It asserts a full parse of `song-100.ly` with a median of 5 ms or less, and a one-character reparse with a median of 1 ms or less, both over 20 runs after a warm-up. It builds a 10,000-bar file by repeating the song's music and asserts that its full parse takes no more than 110 times the 100-bar time, and that a one-character reparse in it stays at 2 ms or less.
- [ ] **Run them to see them fail.** Run `cargo test --test incremental --test prefixes`. Expected: FAIL, from the missing helper or a real mismatch.
- [ ] **Implement the helper, then fix any scanner state bug the tests expose.** The depth byte must be serialised and restored exactly.
- [ ] **Run them to see them pass.** Run `cargo test && cargo test --release --test speed -- --include-ignored`. Expected: PASS. Record the medians in `README.md`.
- [ ] **Commit:** `test: check incremental reparsing, termination and speed`.

### Task: Error placement and the corpus verdict

**Files:**
- Create: `tests/recovery.rs`
- Modify: `README.md` (the verdict section)
- Create (in `swara`): `bench/results/<date>-swara-grammar.json`, `docs/bench/verdict.md`

**Interfaces:**
- Consumes: `cargo xtask compare` and `bench/broken/` from the foundations plan, and the baseline JSON.

- [ ] **Write the failing recovery tests.**
  - `unclosed_brace_keeps_music` inserts `{` after line 10 of `song-100.ly`. It asserts exactly one `MISSING "}"` node, no `ERROR` node, and the same count of `note` nodes as the unedited file.
  - `deleted_brace_keeps_music` deletes the first `}`. It asserts the `note` count still equals the unedited count, and that every `ERROR` node lies after the deletion point.
- [ ] **Run them to see them fail.** Run `cargo test --test recovery`. Expected: FAIL, or PASS if recovery already holds. A PASS is fine; keep the tests either way.
- [ ] **Improve recovery** where the tests fail, by giving `}` and `>>` a sensible precedence. Do not add rules that accept invalid LilyPond.
- [ ] **Run the comparison.** Build `tree-sitter build -o ../swara/bench/third-party/swara-lilypond.dylib .`, then in `swara` run `cargo xtask compare --name swara --lib bench/third-party/swara-lilypond.dylib --symbol tree_sitter_lilypond --out bench/results/<date>-swara-grammar.json`.
- [ ] **Judge and write the verdict.** Swara is better only if all of the following hold. Fewer accepted corpus files show any error than with the baseline. `bytes_per_ms` is at least the baseline's, where the baseline is timed with its injected Scheme grammar. At least as many broken files are caught within ±1 line. Write the figures, both commits and every broken file Swara misses, with a reason, into `docs/bench/verdict.md` and the grammar README, stating plainly if any criterion fails. Run the prose gate on both files.
- [ ] **Commit** in each repository: `docs: report the corpus verdict against nwhetsell/tree-sitter-lilypond`.

### Task: Release the grammar

**Files:**
- Modify: `README.md` (install, nvim-treesitter registration, node contract link, licence)
- Modify: `Cargo.toml` (version)

**Interfaces:**
- Produces: the tag `v0.1.0`, whose commit hash the server plan pins as `tree-sitter-lilypond = { git = "https://github.com/anirbanchakraborty-dev/tree-sitter-lilypond", rev = "<sha>" }`.

- [ ] **Check that the committed parser is current.** Run `tree-sitter generate && git diff --exit-code src/`. Expected: exit 0.
- [ ] **Write the nvim-treesitter snippet.** Read nvim-treesitter's current README on its `main` branch for how a custom parser is registered, and write the matching snippet using `queries = 'queries'` and `revision = 'v0.1.0'`. Test it in a scratch config: `nvim --clean -u <scratch>/init.lua +'TSInstall lilypond' +q`, then open `test/fixtures/everything.ly` and run `:lua print(vim.treesitter.get_parser():lang())`. Expected: `lilypond`.
- [ ] **Run the prose gate on the README,** then commit `chore: release v0.1.0`, tag `v0.1.0`, and push with tags. Run `gh release create v0.1.0 --notes-file <notes>`, where the notes name what the grammar covers and the verdict, with no credit line.
- [ ] **Bump the hub pointer.** From the hub worktree: `git commit --only Projects/tree-sitter-lilypond -m "chore(tree-sitter-lilypond): bump submodule to <short-sha>"`.
