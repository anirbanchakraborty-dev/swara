% expect-error-line: 7
% expect: a block comment is opened and never closed
\version "2.26.0"

\relative c' {
  c4 d e f
  %{ a passage left out for now
  g a b c
}
