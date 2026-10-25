\version "{{version}}"
\language "english"

% The script the words are converted to, and which lyric lines print under the melody: both (the
% words in that script, with the words as typed in smaller type beneath them), script (the
% converted words alone) or roman (the words as typed alone).
lyricScript = #'bengali
lyricLines = #'both

\include "swara-lyrics.ily"
\include "{{title}}-lyrics.ily"

\header {
  title = "{{title}}"
}

melody = \relative c' {
  \time 4/4
  c4 d e f | g a g2 |
}

harmony = \chordmode {
  c1 | g1 |
}

\score {
  <<
    \new ChordNames \harmony
    \new Staff \new Voice = "melody" \melody
    \swaraLyrics "melody" \songLyrics
  >>
  \layout { }
  \midi { }
}
