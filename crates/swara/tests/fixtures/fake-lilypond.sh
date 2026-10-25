#!/bin/sh
# A stand-in for lilypond in the process-control tests. `--version` prints
# "GNU LilyPond $SWARA_FAKE_VERSION" when that is set, and nothing otherwise, and is not logged.
# A command-data dump, which is a run with $SWARA_DUMP_OUT set, appends its pid and arguments to
# $SWARA_FAKE_LOG.dumps and copies $SWARA_FAKE_DUMP, when set, to $SWARA_DUMP_OUT. Every other run
# appends its pid and arguments to $SWARA_FAKE_LOG. Dumps and other runs then take the
# comma-separated steps in $SWARA_FAKE_MODE in order:
#   sleep:N      start a grandchild that sleeps 30 s, append its pid to $SWARA_FAKE_LOG.children,
#                then sleep N seconds
#   stderr:FILE  print FILE to standard error, and exit 1 once the steps are done
if [ "$1" = --version ]; then
  [ -n "$SWARA_FAKE_VERSION" ] && echo "GNU LilyPond $SWARA_FAKE_VERSION"
  exit 0
fi
if [ -n "$SWARA_DUMP_OUT" ]; then
  echo "$$ $*" >> "$SWARA_FAKE_LOG.dumps"
else
  echo "$$ $*" >> "$SWARA_FAKE_LOG"
fi
status=0
set -f
IFS=,
for step in $SWARA_FAKE_MODE; do
  case $step in
    sleep:*)
      sleep 30 </dev/null >/dev/null 2>&1 &
      echo "$!" >> "$SWARA_FAKE_LOG.children"
      sleep "${step#sleep:}"
      ;;
    stderr:*)
      cat "${step#stderr:}" >&2
      status=1
      ;;
  esac
done
if [ -n "$SWARA_DUMP_OUT" ] && [ -n "$SWARA_FAKE_DUMP" ]; then
  cp "$SWARA_FAKE_DUMP" "$SWARA_DUMP_OUT"
fi
exit $status
