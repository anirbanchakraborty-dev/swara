#!/usr/bin/env bash
# Re-records every stderr fixture under tests/fixtures/lilypond. Each case runs through the check
# command with its text on stdin, except the cases that need engraving, which run as a full build.
# The fixtures folder is written as /fixture and the scratch folder LilyPond runs in as /scratch,
# so recordings do not depend on where the repository or the temporary folder lives.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
fixtures=$repo/tests/fixtures/lilypond
settings=$repo/assets/lilypond/check-settings.ly
out=${TMPDIR:-/tmp}
out=${out%/}/swara-check-record
mkdir -p "$out"
out=$(cd "$out" && pwd -P)
full_build=" tab-no-string programming-error "

check() { # case, source
  (cd "$out" && LC_ALL=C lilypond -l WARNING -dinclude-settings="$settings" -I "$fixtures" -o "$out/$1" - < "$2") 2> "$out/$1.raw" || true
}
full() { # case, source
  (cd "$out" && LC_ALL=C lilypond -l WARNING -o "$out/$1" "$2") 2> "$out/$1.raw" || true
}
keep() { # case, fixture name
  sed -e "s|$fixtures|/fixture|g" -e "s|$out|/scratch|g" "$out/$1.raw" > "$fixtures/$2.stderr"
}

for src in "$fixtures"/*.ly; do
  case=$(basename "$src" .ly)
  if [ "$case" = seeded ]; then
    full seeded "$src"; keep seeded seeded.full
    check seeded "$src"; keep seeded seeded.check
  elif [[ $full_build == *" $case "* ]]; then
    full "$case" "$src"; keep "$case" "$case"
  else
    check "$case" "$src"; keep "$case" "$case"
  fi
done
