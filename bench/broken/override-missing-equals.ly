% expect-error-line: 6
% expect: the override has no equals sign before its value
\version "2.26.0"

\relative c'' {
  \override Stem.color #red
  c4 d e f
}
