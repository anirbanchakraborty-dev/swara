% expect-error-line: 8
% expect: the override inside the context block has no equals sign
\version "2.26.0"

\layout {
  \context {
    \Staff
    \override StaffSymbol.line-count 4
  }
}
\relative c' { c1 }
