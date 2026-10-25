; Highlights by node kind. The grammar already says what each word is, so no
; pattern lists LilyPond's commands: a `\word` the grammar does not treat as a
; keyword is a `command`, and the language server decides what it names. The
; one pattern on text is the shape of a dynamic such as `\ff`.
;
; Each node gets one capture. Where a general pattern and a specific one both
; match a node, the specific one comes later in this file, because
; `tree-sitter highlight` and Neovim both keep the last pattern that matches.
; A bare string such as "(" also matches a named token with the same text,
; such as `slur_start`, so brackets are matched under their parent node.

; Comments and text

[
  (line_comment)
  (block_comment)
  (scheme_comment)
  (scheme_block_comment)
  (scheme_datum_comment)
] @comment

[
  (string)
  (scheme_string)
  (markup_word)
  (lyric_word)
  (text_script)
] @string

(escape_sequence) @string.escape

; Notes, durations and numbers

[
  (number)
  (fraction)
  (duration)
  (scheme_number)
  (figure)
] @number

[
  (note_name)
  (scheme_symbol)
] @variable

[
  (octave)
  (accidental_mark)
  (octave_check)
  "="
] @operator

[
  (rest)
  (skip)
  (multi_measure_rest)
  (chord_repetition)
] @constant.builtin

(command) @function.call

; Keywords. `\breve`, `\longa` and `\maxima` are durations, so they stay
; numbers.

[
  "\\accepts"
  "\\addlyrics"
  "\\alias"
  "\\alternative"
  "\\book"
  "\\bookpart"
  "\\change"
  "\\chordmode"
  "\\chords"
  "\\consists"
  "\\context"
  "\\defaultchild"
  "\\denies"
  "\\description"
  "\\drummode"
  "\\drums"
  "\\figuremode"
  "\\figures"
  "\\header"
  "\\layout"
  "\\lyricmode"
  "\\lyrics"
  "\\lyricsto"
  "\\markup"
  "\\markuplist"
  "\\midi"
  "\\name"
  "\\new"
  "\\notemode"
  "\\override"
  "\\paper"
  "\\remove"
  "\\repeat"
  "\\revert"
  "\\score"
  "\\sequential"
  "\\set"
  "\\simultaneous"
  "\\tempo"
  "\\type"
  "\\unset"
  "\\with"
] @keyword

[
  "\\version"
  "\\include"
  "\\language"
] @keyword.import

; Post-events

[
  (hairpin)
  (hairpin_end)
] @attribute.builtin

[
  (articulation)
  (fingering)
  (string_number)
  (tremolo)
] @attribute

; Contexts and properties

(new_context
  type: (identifier) @type)

(change
  type: (identifier) @type)

(lyrics_to
  type: (identifier) @type)

(chord_modifiers) @type

(property_path) @property

; Scheme atoms

(scheme_boolean) @boolean

(scheme_character) @character

(scheme_keyword) @string.special.symbol

; Punctuation

[
  (tie)
  (slur_start)
  (slur_end)
  (phrasing_slur_start)
  (phrasing_slur_end)
  (beam_start)
  (beam_end)
  (ligature_start)
  (ligature_end)
] @punctuation.special

(scheme
  [
    "#"
    "$"
  ] @punctuation.special)

(scheme_quote
  "'" @punctuation.special)

(scheme_quasiquote
  "`" @punctuation.special)

(scheme_unquote
  "," @punctuation.special)

(scheme_unquote_splicing
  ",@" @punctuation.special)

(embedded_lilypond
  [
    "#{"
    "#}"
  ] @punctuation.special)

[
  "{"
  "}"
  "<<"
  ">>"
] @punctuation.bracket

(scheme_list
  [
    "("
    ")"
  ] @punctuation.bracket)

(scheme_vector
  [
    "#("
    ")"
  ] @punctuation.bracket)

(chord
  [
    "<"
    ">"
  ] @punctuation.bracket)

(figure_group
  [
    "<"
    ">"
    "["
    "]"
  ] @punctuation.bracket)

[
  (bar_check)
  (voice_separator)
  (lyric_hyphen)
  (lyric_extender)
] @punctuation.delimiter

; Specific patterns, which override the general ones above

(context_definition
  .
  (command) @type)

(articulation
  (command) @attribute)

(fingering
  (number) @attribute)

(include
  path: (string) @string.special.path)

(assignment
  name: (_) @variable.member)

(scheme_list
  .
  (scheme_symbol) @function.call)

; A dynamic written as an articulation, `-\ff`, is still a dynamic.
((command) @attribute.builtin
  (#match? @attribute.builtin "^\\\\[fmprsz]+$"))
