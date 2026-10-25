% expect-error-line: 5
% expect: a >> closes nothing inside the music
\version "2.26.0"

\relative c' { c4 d >> e f }
