% expect-error-line: 8
% expect: the composer field has no value before the closing brace
\version "2.26.0"

\header {
  title = "Morning Song"
  composer =
}
