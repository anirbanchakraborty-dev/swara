% expect-error-line: 6
% expect: left-margin has no equals sign
\version "2.26.0"

\paper {
  top-margin = 10\mm left-margin 15\mm
}
\relative c' { c1 }
