\version "2.26.0"
\language "english"

% A sheet for reading the Bengali table by eye: forty words common in songs, each printed in
% Bengali with the spelling typed for it beneath. `cargo xtask test-includes` skips it, because
% only a reader can say whether a word looks right. A word marked wrong becomes a line of
% `bengali.tsv` before the table is fixed.

\include "../../includes/swara-lyrics.ily"

\header {
  title = "Swara lyrics: Bengali sample sheet"
  tagline = ##f
}

lyricLines = #'both

melody = \relative c' { \repeat unfold 10 { c4 d e f } \bar "|." }

\score {
  <<
    \new Staff \new Voice = "melody" \melody
    \swaraLyrics "melody" \lyricmode {
      ami tumi tOmay bhalO basi gan mon praN alO akash
      rat din ca^d phul nodI jol megh brriShTi swopn hrridoy
      bhalObasa jIbon maTi ma desh sOnar bangla bhOr pakhi ghor
      bondhu hasi kanna ekhon shudhu sondhya tara nIl shanti kotha
    }
  >>
  \layout {
    \context { \Lyrics \override LyricSpace.minimum-distance = #2 }
  }
}
