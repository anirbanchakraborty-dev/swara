% expect-error-line: 11
% expect: an override with no property path inside a score layout
\version "2.26.0"

\score {
  \relative c'' { c4 d e f }
  \layout {
    \context {
      \Voice
      \override
    }
  }
}
