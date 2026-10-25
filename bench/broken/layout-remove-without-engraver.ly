% expect-error-line: 9
% expect: remove names no engraver before the brace
\version "2.26.0"

\layout {
  \context {
    \Staff
    \remove
  }
}
\relative c' { c1 }
