% expect-error-line: 6
% expect: the string in the markup never ends
\version "2.26.0"

\relative c' { c4 d e f }
\markup { "unfinished text }
