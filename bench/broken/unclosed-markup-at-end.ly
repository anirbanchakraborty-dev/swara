% expect-error-line: 8
% expect: the outer markup brace is never closed
\version "2.26.0"

\markup {
  \bold "Allegro"
  \italic { con brio }
  \column { "first line" "second line" }
