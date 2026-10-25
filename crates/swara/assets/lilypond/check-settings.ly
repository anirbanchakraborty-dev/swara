\version "2.26.0"
%% Swara check settings, loaded with -dinclude-settings (candidate B of the check spike).
%% Every score is interpreted under its own layout definition, or under its MIDI definition when
%% that is its only output, with every engraver taken out of every context. Contexts the score
%% defines still exist and timing still runs, so bar checks are reported; nothing is laid out and
%% nothing is written.

#(define (swara-score-group? def group)
   (let ((score (assq 'Score (ly:output-find-context-def def))))
     (and score
          (eq? (ly:context-def-lookup (cdr score) 'group-type) group))))

#(define (swara-layout-def? def) (swara-score-group? def 'Score_engraver))

#(define (swara-midi-def? def) (swara-score-group? def 'Score_performer))

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
                           (let ((defs (ly:score-output-defs score)))
                             (or (find swara-layout-def? defs)
                                 (find swara-midi-def? defs)
                                 $defaultlayout))))))

#(define (swara-check-book book)
   (for-each swara-check-score (ly:book-scores book))
   (for-each swara-check-book (ly:book-book-parts book)))

#(define toplevel-book-handler swara-check-book)
