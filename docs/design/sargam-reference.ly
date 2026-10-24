\version "2.26.0"

%% Bhatkhande sargam in Bengali script, Sa = C (fixed).
%% komal = underline, tivra Ma = short vertical stroke above,
%% mandra (octave below c') = dot below, tar (octave above b') = dot above.

#(define sargam-font "Kohinoor Bangla")

#(define-markup-command (swara layout props syl komal tivra octave)
   (string? boolean? boolean? integer?)
   "Typeset one Bhatkhande swara."
   (let* ((fonts (chain-assoc-get 'fonts props '()))
          (bprops (cons `((fonts . ,(acons 'serif sargam-font fonts))
                          (font-family . serif))
                        props))
          (t (interpret-markup layout bprops syl))
          (xe (ly:stencil-extent t X))
          (ye (ly:stencil-extent t Y))
          (cx (interval-center xe))
          (th 0.12) (gap 0.3) (r 0.17)
          (low (car ye))
          (high (cdr ye))
          (dot (lambda (y) (ly:stencil-translate (make-circle-stencil r 0 #t) (cons cx y))))
          (s t))
     (if komal
         (let ((y (- low gap)))
           (set! s (ly:stencil-add s (make-line-stencil th (car xe) y (cdr xe) y)))
           (set! low y)))
     (if tivra
         (let ((y0 (+ high gap)) (y1 (+ high gap 0.9)))
           (set! s (ly:stencil-add s (make-line-stencil th cx y0 cx y1)))
           (set! high y1)))
     (do ((i 0 (1+ i))) ((>= i (max 0 (- octave))))
       (set! low (- low gap r))
       (set! s (ly:stencil-add s (dot low)))
       (set! low (- low r)))
     (do ((i 0 (1+ i))) ((>= i (max 0 octave)))
       (set! high (+ high gap r))
       (set! s (ly:stencil-add s (dot high)))
       (set! high (+ high r)))
     s))

%% semitone above C -> (syllable komal? tivra?)
#(define sargam-table
   (vector '("সা" #f #f) '("রে" #t #f) '("রে" #f #f) '("গা" #t #f)
           '("গা" #f #f) '("মা" #f #f) '("মা" #f #t) '("পা" #f #f)
           '("ধা" #t #f) '("ধা" #f #f) '("নি" #t #f) '("নি" #f #f)))

#(define sargam-arg-check-done #f)

#(define (sargam-note-name pitch context)
   (if (not sargam-arg-check-done)
       (begin
         (set! sargam-arg-check-done #t)
         (ly:message "noteNameFunction called with pitch? ~a context? ~a context-name ~a"
                     (ly:pitch? pitch) (ly:context? context) (ly:context-name context))))
   (let* ((semi (round (ly:pitch-semitones pitch)))
          (oct (floor (/ semi 12)))
          (entry (vector-ref sargam-table (modulo semi 12))))
     (make-swara-markup (car entry) (cadr entry) (caddr entry) oct)))

melody = {
  \time 4/4
  g4 aes a bes | b c' des' d' | ees' e' f' fis' | g' aes' a' bes' | b' c'' d'' fis'' |
}

words = \lyricmode {
  Pa Dha Dha Ni | Ni Sa Re Re | Ga Ga Ma Ma | Pa Dha Dha Ni | Ni Sa Re Ma |
}

\score {
  <<
    \new Staff \new Voice = "mel" \melody
    \new Lyrics \lyricsto "mel" \words
    \new NoteNames \with {
      printNotesLanguage = "english"
      printOctaveNames = #'scientific
    } \melody
    \new NoteNames \with {
      noteNameFunction = #sargam-note-name
      \override VerticalAxisGroup.nonstaff-relatedstaff-spacing.padding = #1
      \override VerticalAxisGroup.nonstaff-nonstaff-spacing.padding = #1
    } \melody
    \new TabStaff \melody
    \new NoteNames \with {
      noteNameFunction = #sargam-note-name
      \override VerticalAxisGroup.nonstaff-relatedstaff-spacing.padding = #1
    } \melody
  >>
  \layout { }
}
