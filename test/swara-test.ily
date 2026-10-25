% Shared by the include tests. Each check that fails prints a `swara-test: mismatch` line, and
% `swara-test-done` prints the closing `swara-test: N passed` line that `cargo xtask
% test-includes` looks for. Both go through `ly:warning`, which prints Bengali intact where a
% plain write to stderr under `LC_ALL=C` would not.

#(define swara-test-passed 0)

% The code points of TEXT, so that two strings that look the same can be told apart.
#(define (swara-test-codes text)
   (string-join
    (map (lambda (char)
           (let ((hex (string-upcase (number->string (char->integer char) 16))))
             (string-append "U+" (if (< (string-length hex) 4) (string-pad hex 4 #\0) hex))))
         (string->list text))
    " "))

#(define (swara-test-equal what got expected)
   (cond
    ((equal? got expected)
     (set! swara-test-passed (1+ swara-test-passed)))
    ((and (string? got) (string? expected))
     (ly:warning "swara-test: mismatch: ~a gave ~s (~a), expected ~s (~a)"
                 what got (swara-test-codes got) expected (swara-test-codes expected)))
    (else
     (ly:warning "swara-test: mismatch: ~a gave ~s, expected ~s" what got expected))))

#(define (swara-test-done)
   (ly:warning "swara-test: ~a passed" swara-test-passed))
