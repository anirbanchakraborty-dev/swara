#!/bin/sh
# A stand-in for lilypond in the process-control tests. Each run appends its pid and arguments to
# $SWARA_FAKE_LOG, then takes the comma-separated steps in $SWARA_FAKE_MODE in order:
#   sleep:N      start a grandchild that sleeps 30 s, append its pid to $SWARA_FAKE_LOG.children,
#                then sleep N seconds
#   stderr:FILE  print FILE to standard error, and exit 1 once the steps are done
echo "$$ $*" >> "$SWARA_FAKE_LOG"
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
exit $status
