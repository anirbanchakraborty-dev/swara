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

music = \relative c' {
  \time 4/4
  c4 d e f | g a b c |
}

\score {
  <<
    \new Staff \with { \clef "treble_8" } \music
    \new TabStaff \with { stringTunings = \stringTuning <e, a, d g b e'> } \music
    % The gap below the letter names is wider than the one between a syllable and its octave dot,
    % so a dot over a sargam syllable is read with that syllable.
    \new NoteNames \with {
      printNotesLanguage = "english"
      \override VerticalAxisGroup.nonstaff-nonstaff-spacing.padding = #2
    } \music
    \new NoteNames \with { noteNameFunction = #swara-note-name } \music
  >>
  \layout { }
  \midi { }
}
