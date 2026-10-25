% expect-error-line: 10
% expect: the lyric block is never closed before the >>
\version "2.26.0"

\score {
  <<
    \new Voice = "v" \relative c' { e4 f g2 }
    \new Lyrics \lyricsto "v" {
      Rain falls soft
  >>
  \layout { }
}
