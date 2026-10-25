% expect-error-line: 5
% expect: music embedded in Scheme with #{ is never closed with #}
\version "2.26.0"

twice = #{ c'4 d' e' f'
\score { \twice }
