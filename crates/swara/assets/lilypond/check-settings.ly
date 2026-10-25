\version "2.26.0"
%% The settings for the Swara check, loaded with -dinclude-settings. Chosen in docs/spikes/lilypond-check.md.
%% Every score is interpreted under its own layout definition with every engraver taken out of
%% every context. Contexts the score defines still exist and timing still runs, so bar checks are
%% reported; nothing is laid out and nothing is written.

#(define (swara-layout-def? def)
   (let ((score (assq 'Score (ly:output-find-context-def def))))
     (and score
          (eq? (ly:context-def-lookup (cdr score) 'group-type) 'Score_engraver))))

#(define (swara-engraver? name)
   (or (not (symbol? name))
       (string-suffix? "_engraver" (symbol->string name))))

#(define (swara-without-engravers layout)
   (let ((def (ly:output-def-clone layout)))
     (for-each
      (lambda (entry)
        (let ((doomed (filter swara-engraver?
                              (ly:context-def-lookup (cdr entry) 'consists))))
          (ly:output-def-set-variable!
           def (car entry)
           (ly:context-def-modify
            (cdr entry)
            (ly:make-context-mod (map (lambda (name) (list 'remove name)) doomed))))))
      (ly:output-find-context-def def))
     def))

#(define (swara-check-score score)
   (if (and (ly:score? score) (not (ly:score-error? score)))
       (ly:run-translator (ly:score-music score)
                          (swara-without-engravers
                           (or (find swara-layout-def? (ly:score-output-defs score))
                               $defaultlayout)))))

#(define (swara-check-book book)
   (for-each swara-check-score (ly:book-scores book))
   (for-each swara-check-book (ly:book-book-parts book)))

#(define toplevel-book-handler swara-check-book)
