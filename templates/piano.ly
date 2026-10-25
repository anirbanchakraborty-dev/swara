\version "{{version}}"
\language "english"

\include "swara-sargam.ily"

\header {
  title = "{{title}}"
}

% Sa, the note the sargam line counts from, and the script it is written in: bengali, devanagari
% or latin.
\swaraSa c'
\swaraScript #'bengali

right = \relative c' {
  \clef treble
  \time 4/4
  c4 d e f | g1 |
}

left = \relative c {
  \clef bass
  \time 4/4
  c1 | g'1 |
}

\score {
  \new PianoStaff <<
    \new Staff = "right" \right
    % The letter names and the sargam line of the right hand, between the two staves.
    % The gap below the letter names is wider than the one between a syllable and its octave dot,
    % so a dot over a sargam syllable is read with that syllable.
    \new NoteNames \with {
      printNotesLanguage = "english"
      \override VerticalAxisGroup.nonstaff-nonstaff-spacing.padding = #2
    } \right
    \new NoteNames \with { noteNameFunction = #swara-note-name } \right
    \new Staff = "left" \left
  >>
  \layout { }
  \midi { }
}
