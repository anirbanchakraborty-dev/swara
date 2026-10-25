\version "2.26.0"
\language "english"

% Checks the swara that `swara-sargam.ily` gives each note: every semitone of the octave with Sa on
% C and on D, the notes either side of that octave, and each of the three scripts.

\include "../swara-test.ily"
\include "../../includes/swara-sargam.ily"

% The octave above c', one spelling per semitone, as written in a lesson.
#(define chromatic
   (list (ly:make-pitch 0 0 0) (ly:make-pitch 0 1 FLAT) (ly:make-pitch 0 1 0)
         (ly:make-pitch 0 2 FLAT) (ly:make-pitch 0 2 0) (ly:make-pitch 0 3 0)
         (ly:make-pitch 0 3 SHARP) (ly:make-pitch 0 4 0) (ly:make-pitch 0 5 FLAT)
         (ly:make-pitch 0 5 0) (ly:make-pitch 0 6 FLAT) (ly:make-pitch 0 6 0)))

% Whether each of those semitones above Sa is komal, and whether it is tivra.
#(define marks
   '((#f #f) (#t #f) (#f #f) (#t #f) (#f #f) (#f #f)
     (#f #t) (#f #f) (#t #f) (#f #f) (#t #f) (#f #f)))

% The syllable each semitone above Sa is written with, in each script.
#(define syllables
   '((bengali "সা" "রে" "রে" "গা" "গা" "মা" "মা" "পা" "ধা" "ধা" "নি" "নি")
     (devanagari "सा" "रे" "रे" "ग" "ग" "म" "म" "प" "ध" "ध" "नि" "नि")
     (latin "Sa" "Re" "Re" "Ga" "Ga" "Ma" "Ma" "Pa" "Dha" "Dha" "Ni" "Ni")))

#(define sa-on-c (ly:make-pitch 0 0 0))
#(define sa-on-d (ly:make-pitch 0 1 0))

#(for-each
  (lambda (sa-name sa)
    (for-each
     (lambda (script)
       (for-each
        (lambda (semitone pitch mark syllable)
          (swara-test-equal
           (format #f "~a semitones above Sa on ~a, in ~a" semitone sa-name script)
           (swara-name (ly:pitch-transpose pitch sa) sa script)
           (list syllable (car mark) (cadr mark) 0)))
        (iota 12) chromatic marks (cdr (assq script syllables))))
     '(bengali devanagari latin)))
  '("C" "D")
  (list sa-on-c sa-on-d))

% The notes either side of Sa's octave, and two octaves down.
#(swara-test-equal "b with Sa on C" (swara-name (ly:make-pitch -1 6 0) sa-on-c 'bengali) '("নি" #f #f -1))
#(swara-test-equal "c'' with Sa on C" (swara-name (ly:make-pitch 1 0 0) sa-on-c 'bengali) '("সা" #f #f 1))
#(swara-test-equal "c, with Sa on C" (swara-name (ly:make-pitch -2 0 0) sa-on-c 'bengali) '("সা" #f #f -2))
#(swara-test-equal "c' with Sa on D" (swara-name (ly:make-pitch 0 0 0) sa-on-d 'bengali) '("নি" #t #f -1))
#(swara-test-equal "cs' with Sa on D" (swara-name (ly:make-pitch 0 0 SHARP) sa-on-d 'bengali) '("নি" #f #f -1))
#(swara-test-equal "d'' with Sa on D" (swara-name (ly:make-pitch 1 1 0) sa-on-d 'bengali) '("সা" #f #f 1))
#(swara-test-equal "cs'' with Sa on D" (swara-name (ly:make-pitch 1 0 SHARP) sa-on-d 'bengali) '("নি" #f #f 0))

% The same note spelt two ways is the same swara.
#(swara-test-equal "cs' and df' with Sa on C"
                   (swara-name (ly:make-pitch 0 0 SHARP) sa-on-c 'latin)
                   (swara-name (ly:make-pitch 0 1 FLAT) sa-on-c 'latin))

% Sa and the script start as C and Bengali, and the two commands change them.
#(swara-test-equal "Sa before \\swaraSa" (ly:pitch-semitones (swara-sa)) 0)
#(swara-test-equal "the script before \\swaraScript" (swara-script) 'bengali)
\swaraSa d'
\swaraScript #'latin
#(swara-test-equal "Sa after \\swaraSa d'" (ly:pitch-semitones (swara-sa)) 2)
#(swara-test-equal "the script after \\swaraScript" (swara-script) 'latin)
% Every string inside a markup, in order.
#(define (strings-in m)
   (cond ((string? m) (list m))
         ((pair? m) (append (strings-in (car m)) (strings-in (cdr m))))
         (else '())))
#(swara-test-equal "the note name of d' after both commands"
                   (filter (lambda (s) (member s '("Sa" "সা"))) (strings-in (swara-note-name (ly:make-pitch 0 1 0) #f)))
                   '("Sa"))
\swaraScript #'tamil
#(swara-test-equal "the script after \\swaraScript names a script with no table" (swara-script) 'latin)
\swaraSa c'
\swaraScript #'bengali

#(swara-test-done)
