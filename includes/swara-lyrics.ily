\version "2.26.0"

% Swara's lyrics include, released under the MIT licence with Swara.
%
% Lyrics are typed in English letters and converted to an Indian script while LilyPond engraves,
% so a lyrics file stays plain ASCII and a song builds with plain `lilypond`. One engine serves
% every script, and each script is a table of letters added with `swara-register-script!`.
%
%   \bangla \lyricmode { a -- mi }                 the lyrics in Bengali script
%   \swaraLyrics "melody" \songLyrics              the lines that lyricLines asks for
%
% `lyricLines` is `both` (the script, with the typed line smaller beneath it), `script` or
% `roman`, and `lyricScript` names the table, `bengali` unless it is set.

% Each script's table, by the script's name.
#(define swara-scripts '())

% Adds TABLE as the script NAME, or replaces the table already registered under that name.
#(define (swara-register-script! name table)
   (set! swara-scripts (acons name table (alist-delete name swara-scripts eq?))))

% Every key of TABLE as (key kind value), longest first. Keys of the same length keep the order
% of kinds below, so a letter with its own form after a consonant or a vowel comes first.
#(define (swara-keys table)
   (stable-sort
    (append-map
     (lambda (kind)
       (map (lambda (entry) (list (car entry) kind (cdr entry)))
            (or (assq-ref table kind) '())))
     '(after-consonant after-vowel consonants vowels marks))
    (lambda (a b) (> (string-length (car a)) (string-length (car b))))))

% Converts ASCII by TABLE. At each position the longest key that applies wins. A vowel straight
% after a consonant writes its sign and anywhere else its full form. Two consonants that touch
% get the joiner between them, and a consonant before anything else gets nothing. A letter from
% `after-consonant` or `after-vowel` counts as a consonant for what follows. A character no key
% matches is copied as it is and counts as neither consonant nor vowel.
#(define (swara-convert table ascii)
   (let ((keys (swara-keys table))
         (joiner (assq-ref table 'joiner))
         (end (string-length ascii)))
     ;; `consonant?` says whether what was written last counts as a consonant.
     (let loop ((at 0) (consonant? #f) (out '()))
       (define (applies? key)
         (and (string-prefix? (car key) ascii 0 (string-length (car key)) at end)
              (case (cadr key)
                ((after-consonant) consonant?)
                ((after-vowel) (not consonant?))
                (else #t))))
       (if (= at end)
           (string-concatenate-reverse out)
           (let ((key (find applies? keys)))
             (if (not key)
                 (loop (1+ at) #f (cons (string (string-ref ascii at)) out))
                 (let ((next (+ at (string-length (car key))))
                       (value (caddr key)))
                   (case (cadr key)
                     ((consonants)
                      (loop next #t (cons value (if consonant? (cons joiner out) out))))
                     ((vowels)
                      (loop next #f (cons (if consonant? (cdr value) (car value)) out)))
                     ((marks)
                      (loop next #f (cons value out)))
                     (else
                      (loop next #t (cons value out)))))))))))

% ASCII in the script SCRIPT, in NFC. A script with no table is reported and ASCII is returned.
#(define (swara-transliterate script ascii)
   (let ((table (assq-ref swara-scripts script)))
     (if table
         (string-normalize-nfc (swara-convert table ascii))
         (begin
           (ly:warning "swara-lyrics: no letter table for the script `~a', so the lyrics stay as typed" script)
           ascii))))

% The value of the top-level variable NAME, or DEFAULT while it is unset.
#(define (swara-setting name default)
   (let ((value (ly:parser-lookup name)))
     (if (null? value) default value)))

transliterate =
#(define-music-function (script lyrics) (symbol? ly:music?)
   (_i "Convert each syllable of a copy of @var{lyrics} from English letters to the script
@var{script}, set in that script's font.")
   (let ((table (assq-ref swara-scripts script))
         (copy (ly:music-deep-copy lyrics)))
     (if table
         (let ((font (assq-ref table 'font)))
           (for-each
            (lambda (syllable)
              (let ((text (ly:music-property syllable 'text)))
                (if (string? text)
                    (ly:music-set-property!
                     syllable 'text
                     (make-override-markup (cons 'font-name font)
                                           (swara-transliterate script text))))))
            (extract-typed-music copy 'lyric-event)))
         (ly:input-warning (*location*)
                           "swara-lyrics: no letter table for the script `~a', so the lyrics stay as typed"
                           script))
     copy))

bangla = \transliterate #'bengali \etc

swaraLyrics =
#(define-music-function (voice lyrics) (string? ly:music?)
   (_i "The lines of @var{lyrics} that @code{lyricLines} asks for, each set to the voice
@var{voice}: the script named by @code{lyricScript}, the lyrics as typed, or both.")
   (let* ((asked (swara-setting 'lyricLines 'both))
          (lines (if (memq asked '(both script roman))
                     asked
                     (begin
                       (ly:input-warning (*location*)
                                         "swara-lyrics: lyricLines is ~s, not both, script or roman, so both lines print"
                                         asked)
                       'both)))
          (script (swara-setting 'lyricScript 'bengali)))
     (define (converted)
       #{ \new Lyrics \lyricsto #voice { #(transliterate script lyrics) } #})
     (case lines
       ((script)
        #{ << #(converted) >> #})
       ((roman)
        #{ << \new Lyrics \lyricsto #voice { #(ly:music-deep-copy lyrics) } >> #})
       (else
        #{
          <<
            #(converted)
            \new Lyrics \with { \override LyricText.font-size = #-2 }
            \lyricsto #voice { #(ly:music-deep-copy lyrics) }
          >>
        #}))))

#(swara-register-script!
  'bengali
  '((consonants
     . (("k" . "ক") ("kh" . "খ") ("g" . "গ") ("gh" . "ঘ") ("Ng" . "ঙ")
        ("c" . "চ") ("ch" . "ছ") ("j" . "জ") ("jh" . "ঝ") ("NG" . "ঞ")
        ("T" . "ট") ("Th" . "ঠ") ("D" . "ড") ("Dh" . "ঢ") ("N" . "ণ")
        ("t" . "ত") ("th" . "থ") ("d" . "দ") ("dh" . "ধ") ("n" . "ন")
        ("p" . "প") ("ph" . "ফ") ("f" . "ফ") ("b" . "ব") ("bh" . "ভ") ("v" . "ভ")
        ("m" . "ম") ("z" . "য") ("r" . "র") ("l" . "ল")
        ("sh" . "শ") ("Sh" . "ষ") ("s" . "স") ("h" . "হ")
        ("R" . "ড়") ("Rh" . "ঢ়")))
    (vowels
     . (("o" . ("অ" . "")) ("a" . ("আ" . "া"))
        ("i" . ("ই" . "ি")) ("I" . ("ঈ" . "ী"))
        ("u" . ("উ" . "ু")) ("U" . ("ঊ" . "ূ"))
        ("e" . ("এ" . "ে")) ("O" . ("ও" . "ো"))
        ("OI" . ("ঐ" . "ৈ")) ("OU" . ("ঔ" . "ৌ"))
        ("rri" . ("ঋ" . "ৃ"))))
    (marks . (("t`" . "ৎ") ("ng" . "ং") (":" . "ঃ") ("^" . "ঁ")))
    (after-consonant . (("y" . "্য") ("w" . "্ব")))
    (after-vowel . (("y" . "য়")))
    (joiner . "্")
    (font . "Kohinoor Bangla, Noto Sans Bengali")))
