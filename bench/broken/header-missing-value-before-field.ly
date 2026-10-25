% expect-error-line: 8
% expect: the subtitle field has no value, so the next field name becomes its value
\version "2.26.0"

\header {
  title = "River"
  subtitle =
  composer = "Traditional"
}
\relative c' { c1 }
