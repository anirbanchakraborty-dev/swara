% expect-error-line: 9
% expect: the brace that opens the melody is never closed, so the score block lands inside it
\version "2.26.0"

melody = \relative c' {
  c4 d e f
  g2 g

\score {
  \new Staff \melody
}
