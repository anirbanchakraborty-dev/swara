#!/usr/bin/env bash
# Throwaway driver for the LilyPond check spike.
#   run.sh check <full|a|b|nopages>  run one configuration on the seeded fixture and compare with seeded.expect
#   run.sh time                      time every configuration on bench/song-100.ly
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
fixtures=$repo/tests/fixtures/lilypond
out=${TMPDIR:-/tmp}/swara-check-spike
mkdir -p "$out"

opts() {
  case $1 in
    full) ;;
    a) echo "-dinclude-settings=$here/candidate-a.ly" ;;
    b) echo "-dinclude-settings=$here/candidate-b.ly" ;;
    nopages) echo "-dno-print-pages" ;;
    *) echo "unknown configuration: $1" >&2; exit 2 ;;
  esac
}

check() {
  local cfg=$1 err=$out/seeded.$1.stderr status=0 found=0 missing=0
  rm -f "$out/seeded.$cfg".*
  # shellcheck disable=SC2046
  LC_ALL=C lilypond -l WARNING $(opts "$cfg") -o "$out/seeded.$cfg" "$fixtures/seeded.ly" 2> "$err" || status=$?
  while IFS=: read -r line sev text; do
    if /usr/bin/grep -F "seeded.ly:$line:" "$err" | /usr/bin/grep -F ": $sev: " | /usr/bin/grep -qF "$text"; then
      found=$((found + 1))
    else
      missing=$((missing + 1)); echo "missing: $line $sev $text"
    fi
  done < "$fixtures/seeded.expect"
  echo "$cfg: exit $status, $found found, $missing missing, outputs: $(cd "$out" && /bin/ls -1 | /usr/bin/grep -E "^seeded\.$cfg\.(pdf|midi)$" | tr '\n' ' ')"
}

timeall() {
  local song=$repo/bench/song-100.ly
  hyperfine -N -w 1 -r 5 --export-json "$out/times.json" \
    -n full "env LC_ALL=C lilypond -l WARNING -o $out/song.full $song" \
    -n nopages "env LC_ALL=C lilypond -l WARNING -dno-print-pages -o $out/song.nopages $song" \
    -n a "env LC_ALL=C lilypond -l WARNING -dinclude-settings=$here/candidate-a.ly -o $out/song.a $song" \
    -n b "env LC_ALL=C lilypond -l WARNING -dinclude-settings=$here/candidate-b.ly -o $out/song.b $song"
  python3 -I -c '
import json, re, sys
for r in json.load(open(sys.argv[1]))["results"]:
    w = r["summary"]["time_wall_clock"]
    name = re.search(r"song\.(\w+) ", r["command"]).group(1)
    print("%-8s median %4.0f ms  min %4.0f  max %4.0f" % (name, w["median"] * 1e3, w["min"] * 1e3, w["max"] * 1e3))
' "$out/times.json"
}

case ${1:-} in
  check) check "${2:?configuration}" ;;
  time) timeall ;;
  *) echo "usage: run.sh check <full|a|b|nopages> | run.sh time" >&2; exit 2 ;;
esac
