% expect-error-line: 8
% expect: the last chord is never closed before the brace
\version "2.26.0"

\new Staff {
  \time 3/4
  <c' e' g'>2. <d' f' a'
}
