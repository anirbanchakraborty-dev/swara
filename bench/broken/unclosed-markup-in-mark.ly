% expect-error-line: 10
% expect: the markup after mark is never closed, so it swallows the rest of the music
\version "2.26.0"

\relative c'' {
  c4 d e f
  \mark \markup { \box A
  g1
  f1
}
