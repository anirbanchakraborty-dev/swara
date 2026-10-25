% expect-error-line: 8
% expect: the lambda inside the override is missing a closing parenthesis
\version "2.26.0"

\score {
  \relative c'' { c4 d e f }
  \layout {
    \override Staff.TimeSignature.stencil = #(lambda (grob) (ly:time-signature::print grob)
  }
}
