\version "2.26.0"
\include "../swara-test.ily"
\include "../../includes/swara-lyrics.ily"

% `\swaraLyrics` reads `lyricLines` and `lyricScript` each time it is called, so one file can set
% each value in turn.

#(define words #{ \lyricmode { a -- mi } #})

#(define (lyric-lines music)
   (filter (lambda (m) (eq? (ly:music-property m 'context-type) 'Lyrics))
           (extract-typed-music music 'context-specification)))

#(define (texts music)
   (map (lambda (m) (ly:music-property m 'text)) (extract-typed-music music 'lyric-event)))

% Whether the line's `\with` block makes its text smaller.
#(define (smaller? line)
   (and (any (lambda (operation) (memq 'font-size operation))
             (ly:music-property line 'property-operations))
        #t))

#(define (in-font font texts)
   (map (lambda (text) (make-override-markup (cons 'font-name font) text)) texts))

#(swara-test-equal "lines while lyricLines is unset" (length (lyric-lines (swaraLyrics "v" words))) 2)

lyricLines = #'both
#(define both (lyric-lines (swaraLyrics "v" words)))
#(swara-test-equal "lines for both" (length both) 2)
#(swara-test-equal "the first line, in Bengali while lyricScript is unset"
   (texts (first both))
   (in-font "Kohinoor Bangla, Noto Sans Bengali" '("আ" "মি")))
#(swara-test-equal "the second line, as typed" (texts (second both)) '("a" "mi"))
#(swara-test-equal "which lines are smaller under both" (map smaller? both) '(#f #t))

lyricLines = #'script
#(define script (lyric-lines (swaraLyrics "v" words)))
#(swara-test-equal "lines for script" (length script) 1)
#(swara-test-equal "the script line" (texts (first script)) (in-font "Kohinoor Bangla, Noto Sans Bengali" '("আ" "মি")))

lyricLines = #'roman
#(define roman (lyric-lines (swaraLyrics "v" words)))
#(swara-test-equal "lines for roman" (length roman) 1)
#(swara-test-equal "the roman line" (texts (first roman)) '("a" "mi"))
#(swara-test-equal "the roman line is full size" (smaller? (first roman)) #f)

lyricLines = #'neither
#(swara-test-equal "lines for a value it does not know" (length (lyric-lines (swaraLyrics "v" words))) 2)

lyricLines = #'script
lyricScript = #'tiny
#(swara-register-script! 'tiny
   '((consonants . (("m" . "M")))
     (vowels . (("a" . ("A" . "")) ("i" . ("I" . "!"))))
     (joiner . "")
     (font . "C059")))
#(swara-test-equal "the table lyricScript names"
   (texts (first (lyric-lines (swaraLyrics "v" words))))
   (in-font "C059" '("A" "M!")))

#(swara-test-equal "the lyrics passed in" (texts words) '("a" "mi"))

#(swara-test-done)
