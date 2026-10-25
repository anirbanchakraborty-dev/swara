% expect-error-line: 7
% expect: a figure is closed with >> instead of >
\version "2.26.0"

\score {
  <<
    \new FiguredBass \figuremode { <6 4>2 <5 3>> }
    \new Staff \relative c { \clef bass g2 c }
  >>
}
