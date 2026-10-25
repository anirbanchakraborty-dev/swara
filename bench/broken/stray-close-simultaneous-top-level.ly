% expect-error-line: 9
% expect: a >> at the top level closes nothing
\version "2.26.0"

\score {
  \new Staff \relative c'' { e4 d c2 }
  \layout { }
}
>>
