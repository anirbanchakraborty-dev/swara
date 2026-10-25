% expect-error-line: 9
% expect: the << is never closed before the score brace
\version "2.26.0"

\score {
  <<
    \new Staff { c''1 }
    \new Staff { \clef bass c1 }
}
