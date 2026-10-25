% expect-error-line: 12
% expect: the outer brace is never closed, so the file ends inside the music
\version "2.26.0"

\relative c'' {
  a4 b c d
  \repeat volta 2 {
    e4 d c b
  }
  a1
  g4 f e d
  c1
