\version "2.26.0"

% Swara's sargam include, released under the MIT licence with Swara.
%
% Names each note with its Indian sargam syllable, for LilyPond's NoteNames context, so a lesson file
% holds notes only and builds with plain `lilypond`.
%
%   \swaraSa c'                    the note Sa stands on, c' unless it is set
%   \swaraScript #'bengali         bengali, devanagari or latin
%   \new NoteNames \with { noteNameFunction = #swara-note-name } \music
%
% A note is named by its distance in semitones from Sa, in Bhatkhande notation: a komal swara is
% underlined, tivra Ma has a short vertical stroke above it, and each octave below Sa's adds a dot
% beneath the syllable, each octave above a dot over it.
%
% Sa and the script hold for the whole file, and the last setting wins, because LilyPond names the
% notes only once it has read the file to the end.

% The syllables of the seven swaras, Sa to Ni, in each script.
#(define swara-syllables
   '((bengali . #("সা" "রে" "গা" "মা" "পা" "ধা" "নি"))
     (devanagari . #("सा" "रे" "ग" "म" "प" "ध" "नि"))
     (latin . #("Sa" "Re" "Ga" "Ma" "Pa" "Dha" "Ni"))))

% The fonts each script is set in, or #f for LilyPond's own text font.
#(define swara-fonts
   '((bengali . "Kohinoor Bangla, Noto Sans Bengali")
     (devanagari . "Kohinoor Devanagari, Noto Sans Devanagari")
     (latin . #f)))

% For each semitone above Sa: which of the seven swaras it is, whether it is komal, and whether it
% is tivra.
#(define swara-semitones
   #((0 #f #f) (1 #t #f) (1 #f #f) (2 #t #f) (2 #f #f) (3 #f #f)
     (3 #f #t) (4 #f #f) (5 #t #f) (5 #f #f) (6 #t #f) (6 #f #f)))

% The marks' sizes, in staff spaces. An octave dot keeps a whole staff space from the syllable, so
% that it is not read as part of a letter: র already has a dot beneath it, and a dot over a syllable
% looks like a nasal mark.
#(define swara-line-thickness 0.12)
#(define swara-mark-gap 0.3)
#(define swara-stroke-length 0.9)
#(define swara-dot-radius 0.17)
#(define swara-dot-padding 1)

#(define swara-current-sa (ly:make-pitch 0 0 0))
#(define swara-current-script 'bengali)

% The note Sa stands on.
#(define (swara-sa) swara-current-sa)

% The script the syllables are written in.
#(define (swara-script) swara-current-script)

swaraSa =
#(define-void-function (pitch) (ly:pitch?)
   (_i "Set Sa, the note the sargam line counts from, to @var{pitch}.")
   (set! swara-current-sa pitch))

swaraScript =
#(define-void-function (script) (symbol?)
   (_i "Write the sargam line in @var{script}: @code{bengali}, @code{devanagari} or @code{latin}.")
   (if (assq script swara-syllables)
       (set! swara-current-script script)
       (ly:input-warning (*location*)
                         "swara-sargam: no syllables for the script `~a', so the sargam line stays in ~a"
                         script swara-current-script)))

% The swara PITCH is when Sa stands on SA, in SCRIPT, as (syllable komal? tivra? octave). The octave
% is 0 from Sa up to the Ni above it, and counts down below that and up above it.
#(define (swara-name pitch sa script)
   (let* ((distance (inexact->exact (round (- (ly:pitch-semitones pitch) (ly:pitch-semitones sa)))))
          (entry (vector-ref swara-semitones (modulo distance 12))))
     (list (vector-ref (assq-ref swara-syllables script) (car entry))
           (cadr entry)
           (caddr entry)
           (floor (/ distance 12)))))

#(define-markup-command (swara-syllable layout props syllable komal? tivra? octave)
   (markup? boolean? boolean? integer?)
   "Draw @var{syllable} as one Bhatkhande swara: underlined when @var{komal?}, with a vertical
stroke above when @var{tivra?}, and with a dot for each octave @var{octave} is away from zero,
beneath the syllable below zero and over it above."
   (let* ((text (interpret-markup layout props syllable))
          (across (ly:stencil-extent text X))
          (middle (interval-center across))
          (low (car (ly:stencil-extent text Y)))
          (high (cdr (ly:stencil-extent text Y)))
          (stencil text))
     (if komal?
         (begin
           (set! low (- low swara-mark-gap))
           (set! stencil (ly:stencil-add stencil (make-line-stencil swara-line-thickness
                                                                    (car across) low (cdr across) low)))))
     (if tivra?
         (let ((top (+ high swara-mark-gap swara-stroke-length)))
           (set! stencil (ly:stencil-add stencil (make-line-stencil swara-line-thickness
                                                                    middle (+ high swara-mark-gap) middle top)))
           (set! high top)))
     ;; Dots go outward from the syllable's edge on the octave's side, the first a staff space away.
     (let ((sign (if (negative? octave) -1 1)))
       (let loop ((left (abs octave)) (edge (if (negative? octave) low high)) (gap swara-dot-padding))
         (if (positive? left)
             (let ((centre (+ edge (* sign (+ gap swara-dot-radius)))))
               (set! stencil (ly:stencil-add stencil
                                             (ly:stencil-translate
                                              (make-circle-stencil swara-dot-radius 0 #t)
                                              (cons middle centre))))
               (loop (1- left) (+ centre (* sign swara-dot-radius)) swara-mark-gap)))))
     stencil))

% The noteNameFunction for a NoteNames context: the sargam syllable of PITCH, from the Sa and the
% script set last.
#(define (swara-note-name pitch context)
   (let* ((name (swara-name pitch (swara-sa) (swara-script)))
          (font (assq-ref swara-fonts (swara-script)))
          (syllable (if font
                        (make-override-markup (cons 'font-name font) (car name))
                        (car name))))
     (make-swara-syllable-markup syllable (cadr name) (caddr name) (cadddr name))))
