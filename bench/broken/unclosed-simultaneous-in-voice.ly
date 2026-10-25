% expect-error-line: 10
% expect: the << for two voices is never closed
\version "2.26.0"

\new Staff \relative c'' {
  \time 2/4
  c4 d << { e f } \\ { c d }
  g2
  a2
}
