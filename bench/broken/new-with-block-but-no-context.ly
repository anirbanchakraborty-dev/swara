% expect-error-line: 7
% expect: new has a with block but no context type
\version "2.26.0"

\score {
  <<
    \new \with { instrumentName = "Flute" } { c''1 }
    \new Staff { c'1 }
  >>
}
