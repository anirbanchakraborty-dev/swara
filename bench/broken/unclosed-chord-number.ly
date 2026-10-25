% expect-error-line: 6
% expect: a chord is left open, so the duration lands inside it
\version "2.26.0"

\relative c' {
  <c e g>4 <d f a>4 <e g b 4 r4
  <c e g>1
}
