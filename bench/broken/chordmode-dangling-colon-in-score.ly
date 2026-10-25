% expect-error-line: 7
% expect: a chord modifier list ends with a colon and no modifier
\version "2.26.0"

\score {
  <<
    \new ChordNames \chordmode { c1 g:7sus4 a:m7: }
    \new Staff \relative c'' { e1 d c }
  >>
}
