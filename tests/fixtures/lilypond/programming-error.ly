\version "2.26.0"
{ \override NoteHead.X-offset = #(lambda (grob) (ly:grob-property grob 'X-offset)) c'4 }
