\version "2.26.0"
\include "../swara-test.ily"
\include "../../includes/swara-lyrics.ily"

% Checks every line of `bengali.tsv`, which sits beside this file: English letters, a tab, then the
% Bengali they must give. Both sides pass through NFC, so a letter typed as one code point and the
% same letter typed as two compare equal.

#(define here (dirname (car (ly:input-file-line-char-column (*location*)))))

#(for-each
  (lambda (line)
    (let ((tab (string-index line #\tab)))
      (cond
       ((string-null? line))
       ((not tab) (ly:warning "swara-test: mismatch: no tab in the line ~s" line))
       (else
        (let ((ascii (substring line 0 tab)))
          (swara-test-equal ascii
                            (swara-transliterate 'bengali ascii)
                            (string-normalize-nfc (substring line (1+ tab)))))))))
  (string-split (ly:gulp-file-utf8 (string-append here "/bengali.tsv")) #\newline))

#(swara-test-done)
