% expect-error-line: 7
% expect: the lyric block is never closed, so the file ends inside it
\version "2.26.0"

\new Voice = "tune" \relative c'' { a4 b c d e1 }
\new Lyrics \lyricsto "tune" {
  Wa -- ter flows a -- way
  
