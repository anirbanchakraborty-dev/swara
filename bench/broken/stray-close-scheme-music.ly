% expect-error-line: 6
% expect: a #} appears without a #{ to close
\version "2.26.0"

\relative c' {
  c4 d e f #}
}
