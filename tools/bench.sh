#!/usr/bin/env bash
# bench: run the title once, then report the world frame rate.
#
#   ./tools/bench.sh "label" [-- extra Nullifier args]
#   ./tools/bench.sh --summary LOG     # the report of a run's log again; runs nothing
#
# Environment you export is passed through (VICTORY_* switches), so a run is:
#
#   VICTORY_LATE_WAIT=1 ./tools/bench.sh "amd linux, late-wait 1"
#
# Reports the median of the world-phase frame lines (fps < 10), which is the
# only number comparable between runs: menu/loading frames are 30 fps and
# would flatter any configuration. Peaks are reported but are scenes, not
# settings. See docs/FORK.md for the protocol and why it is medians.
set -uo pipefail

# --summary LOG: report on an existing log and run nothing.
SUMMARY=""
if [ "${1:-}" = "--summary" ]; then
  SUMMARY="${2:-}"
  [ -f "$SUMMARY" ] || { echo "bench: --summary needs a log file" >&2; exit 2; }
  set -- "$(basename "$SUMMARY")"
fi

LABEL="${1:-run}"; shift || true
[ "${1:-}" = "--" ] && shift

HERE="$(cd "$(dirname "$0")/.." && pwd)"
GAME="${VICTORY_GAME_DIR:-$HOME/Games/YourCopy}"
BIN="${VICTORY_BIN:-$HERE/target/x86_64-pc-windows-msvc/diag/nullifier.exe}"
LOG="${VICTORY_BENCH_LOG:-/tmp/victory-bench-$(date +%H%M%S).log}"
[ -n "$SUMMARY" ] && LOG="$SUMMARY"

KEYS="${VICTORY_KEYS:-15:Enter,22:Enter,30:Enter,38:Enter,46:Enter,55:Enter,65:Enter,75:Enter,90:Enter,110:Enter,130:Enter}"
SECONDS_TO_RUN="${VICTORY_BENCH_SECONDS:-300}"

echo "bench: $LABEL"
echo "  log $LOG"

if [ -z "$SUMMARY" ]; then
  VICTORY_BIN="$BIN" VICTORY_GAME_DIR="$GAME" VICTORY_KEYS="$KEYS" \
    timeout "$SECONDS_TO_RUN" "$HERE/launcher/nullifier-launch.sh" --game "$GAME" --bin "$BIN" -- "$@" \
    >"$LOG" 2>&1
  CODE=$?
else
  CODE=-
fi

CLEAN=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG")

# grep -c prints 0 and fails when nothing matches: with "|| echo 0" behind it
# the count of a clean log was "0" twice, on two lines, which is not "0", so
# every clean run got the notes below about a removed device and a failed clamp.
REMOVALS=$(grep -c 'device removed' "$LOG" 2>/dev/null); REMOVALS=${REMOVALS:-0}
FAULTS=$(grep -c 'unhandled exception' "$LOG" 2>/dev/null); FAULTS=${FAULTS:-0}
CLAMP=$(grep -c 'no argument clamp' "$LOG" 2>/dev/null); CLAMP=${CLAMP:-0}
INDIRECT=$(grep -E "dropped .*not bounded" "$LOG" 2>/dev/null | tail -1 | grep -oE '^.*dropped [0-9.]+' | grep -oE '[0-9.]+' | head -1)

STATS=$(echo "$CLEAN" | grep -oE '[0-9.]+ fps' | tr -d ' fps' | awk '$1 < 10 {print}' | sort -n | awk '
  {a[NR]=$1}
  END {
    if (!NR) { print "0 0 0 0"; exit }
    med = (NR%2) ? a[(NR+1)/2] : (a[NR/2]+a[NR/2+1])/2
    printf "%.2f %.2f %.2f %d\n", med, a[1], a[NR], NR
  }')

set -- $STATS
MED=$1 MIN=$2 MAX=$3 N=$4

printf '\n  %-28s median %5s fps   min %5s   max %5s   n=%s\n' "$LABEL" "$MED" "$MIN" "$MAX" "$N"
printf '  %-28s removals %s   faults %s   clamp-fail %s   indirect-dropped %s   exit %s\n' "" "$REMOVALS" "$FAULTS" "$CLAMP" "${INDIRECT:-0}" "$CODE"

if [ "$N" -lt 8 ]; then
  echo "  note: fewer than 8 world frames sampled; the run probably never reached the world"
fi
if [ "$REMOVALS" != "0" ]; then
  echo "  note: the host device was removed - this configuration is not stable"
fi
if [ "$CLAMP" != "0" ]; then
  echo "  note: the indirect clamp failed to compile; indirect draws (the scene) are dropped"
fi
