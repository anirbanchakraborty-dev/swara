% expect-error-line: 8
% expect: line-width has no value, so the next setting becomes its value
\version "2.26.0"

\paper {
  indent = 0\mm
  line-width =
  ragged-last = ##t
}
\relative c' { c1 }
