\version "2.26.0"
\include "../swara-test.ily"
\include "../../includes/swara-lyrics.ily"

% A made-up script whose letters show which rule wrote what. The rules are the same for every
% script, so a new script needs only a table.
#(define test-latin
   '((consonants . (("k" . "K") ("t" . "T")))
     (vowels . (("o" . ("O" . "")) ("a" . ("A" . "-a"))))
     (marks . (("n" . "N")))
     (after-consonant . (("y" . "+Y")))
     (after-vowel . (("y" . "Y")))
     (joiner . "+")
     (font . "C059")))

#(swara-register-script! 'test-latin test-latin)

#(for-each
  (lambda (pair)
    (swara-test-equal (car pair) (swara-transliterate 'test-latin (car pair)) (cdr pair)))
  '(("kta" . "K+T-a")
    ("ok" . "OK")
    ("kot" . "KT")
    ("kn" . "KN")
    ("kya" . "K+Y-a")
    ("ya" . "Y-a")
    ("k!" . "K!")))

% The result is in NFC even where a table's own text is not: U+09DC is ড় as one code point, which
% NFC writes as ড followed by the nukta.
#(swara-register-script! 'test-nfc '((consonants . (("R" . "\u09dc"))) (joiner . "") (font . "C059")))
#(swara-test-equal "a table letter that is not in NFC" (swara-transliterate 'test-nfc "R") "\u09a1\u09bc")

#(swara-test-equal "an unknown script" (swara-transliterate 'no-such-script "kta") "kta")

% Registering a script again replaces its table rather than adding a second one.
#(swara-register-script! 'test-latin (acons 'joiner "*" test-latin))
#(swara-test-equal "kt once the joiner is *" (swara-transliterate 'test-latin "kt") "K*T")
#(swara-test-equal "tables named test-latin"
   (length (filter (lambda (entry) (eq? (car entry) 'test-latin)) swara-scripts))
   1)
#(swara-register-script! 'test-latin test-latin)

% `\transliterate` converts each syllable of a copy and sets it in the table's font. Hyphens and
% extenders are events of their own and stay as they are.
#(define (texts music)
   (map (lambda (m) (ly:music-property m 'text)) (extract-typed-music music 'lyric-event)))

#(define typed #{ \lyricmode { kta -- ok __ ya } #})
#(define converted (transliterate 'test-latin typed))

#(swara-test-equal "the converted syllables"
   (texts converted)
   (map (lambda (text) (make-override-markup '(font-name . "C059") text)) '("K+T-a" "OK" "Y-a")))
#(swara-test-equal "hyphens and extenders"
   (map (lambda (type) (length (extract-typed-music converted type))) '(hyphen-event extender-event))
   '(1 1))
#(swara-test-equal "the lyrics passed in" (texts typed) '("kta" "ok" "ya"))
#(swara-test-equal "an unknown script's lyrics"
   (texts (transliterate 'no-such-script typed))
   '("kta" "ok" "ya"))

#(swara-test-done)
