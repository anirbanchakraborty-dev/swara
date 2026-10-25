% expect-error-line: 5
% expect: the #{ inside the music function body has no #}
\version "2.26.0"

echo = #(define-music-function (m) (ly:music?)
  #{ $m \transpose c c' $m )
\relative c' { \echo { c4 d e f } }
