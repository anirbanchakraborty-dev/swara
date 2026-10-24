\version "2.26.0"
%% Swara check, candidate A. Loaded with -dinclude-settings.
%% Every score is interpreted against a MIDI output definition and never engraved. Parse errors,
%% Scheme errors and interpretation warnings such as bar checks still appear; nothing is laid out
%% and nothing is written.

#(define (swara-midi-def? def)
   (let ((score (assq 'Score (ly:output-find-context-def def))))
     (and score
          (eq? (ly:context-def-lookup (cdr score) 'group-type) 'Score_performer))))

#(define (swara-check-score score)
   (if (and (ly:score? score) (not (ly:score-error? score)))
       (ly:run-translator (ly:score-music score)
                          (or (find swara-midi-def? (ly:score-output-defs score))
                              $defaultmidi))))

#(define (swara-check-book book)
   (for-each swara-check-score (ly:book-scores book))
   (for-each swara-check-book (ly:book-book-parts book)))

#(define toplevel-book-handler swara-check-book)
