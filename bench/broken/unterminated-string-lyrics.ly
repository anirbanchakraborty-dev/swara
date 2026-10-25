% expect-error-line: 8
% expect: a quoted lyric syllable never ends
\version "2.26.0"

<<
  \new Voice = "tune" \relative c'' { a4 b c2 }
  \new Lyrics \lyricsto "tune" {
    Sing "a -- long
  }
>>
