% expect-error-line: 10
% expect: a second closing brace ends the staff and then meets the open <<
\version "2.26.0"

\score {
  <<
    \new Staff \relative c'' {
      g4 a b c
      d1 }
    }
    \new Staff \relative c { \clef bass c1 c1 }
  >>
}
