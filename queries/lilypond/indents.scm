; nvim-treesitter's indent captures. A block indents what it holds, and its
; closing bracket returns to the block's own column.

[
  (header_block)
  (paper_block)
  (layout_block)
  (midi_block)
  (context_definition)
  (score_block)
  (book_block)
  (bookpart_block)
  (sequential_music)
  (simultaneous_music)
  (alternative)
  (with_block)
  (markup_block)
  (scheme_list)
  (scheme_vector)
  (embedded_lilypond)
] @indent.begin

[
  "}"
  ">>"
  "#}"
] @indent.branch @indent.end

; Matched under the parent, since a bare ")" would also match `slur_end`.
(scheme_list
  ")" @indent.branch @indent.end)

(scheme_vector
  ")" @indent.branch @indent.end)
