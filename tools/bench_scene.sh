#!/usr/bin/env bash
# bench_scene: measure the same *slice* of the world every time.
#
#   ./tools/bench_scene.sh "label" [-- extra Nullifier args]
#
# Why this and not bench.sh: averaging the whole world phase mixes the loading
# tail, the first room, camera cuts and streaming, and two runs of one build
# then differ by up to 40 % (docs/FINDINGS.md 3.11). This waits until the world
# is actually running, then measures the next fixed window only, with the same
# scripted input and a warm shader cache. Compare medians *and* p10 between
# runs; both must move for a change to count.
set -uo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE" || exit 1

LABEL="${1:-run}"; shift || true
[ "${1:-}" = "--" ] && shift

GAME="${VICTORY_GAME_DIR:-$HOME/Games/YourCopy}"
LAUNCHER="${VICTORY_LAUNCHER:-$HERE/launcher/nullifier-launch.sh}"
BIN="${VICTORY_BIN:-$HERE/target/x86_64-pc-windows-msvc/diag/nullifier.exe}"
WINDOW="${VICTORY_WINDOW:-35}"          # seconds measured once the world runs
WARMUP="${VICTORY_WARMUP:-10}"          # seconds after world entry discarded
TOTAL="${VICTORY_TOTAL:-200}"
LOG="${VICTORY_BENCH_LOG:-/tmp/opencode/scene-$(date +%H%M%S).log}"
export VICTORY_KEYS="${VICTORY_KEYS:-15:Enter,22:Enter,30:Enter,38:Enter,46:Enter,55:Enter,65:Enter,75:Enter,90:Enter,110:Enter,130:Enter}"

mkdir -p "$(dirname "$LOG")"
# A number without a picture is how a black screen got shipped once
# (docs/FINDINGS.md 3.14). Capture one world frame and check its size: a real
# frame is megabytes, a black one is ~1.5 KB.
CAPDIR=$(mktemp -d "$(dirname "$LOG")/cap-XXXXXX") || {
  echo "bench_scene: could not make a capture folder beside $LOG" >&2
  exit 1
}
export VICTORY_CAPTURE="Z:$(echo "$CAPDIR" | tr '/' '\\')"
export VICTORY_CAPTURE_FRAMES="${VICTORY_CAPTURE_FRAMES:-1024,1536}"
echo "bench_scene: $LABEL"
echo "  window ${WINDOW}s (+${WARMUP}s warm-up), log $LOG"

# Give this benchmark its own process group so cleanup also reaches its children.
RUN=""
stop_run() {
  if [ -n "$RUN" ]; then
    kill -KILL -- "-$RUN" 2>/dev/null || true
    wait "$RUN" 2>/dev/null || true
    RUN=""
  fi
}
trap stop_run EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
set -m
"$LAUNCHER" --game "$GAME" --bin "$BIN" -- "$@" </dev/null >"$LOG" 2>&1 &
RUN=$!
set +m

# Wait for a loaded world: two frame lines of 200 draws or more with the
# texture slots and heap pass settled. fps alone starts the window during
# streaming (issue #63: a 0.6 fps streaming line was taken for the world).
ENTRY=""
for _ in $(seq 1 "$TOTAL"); do
  sleep 1
  if python3 "$HERE/tools/frames.py" "$LOG" --ready 2>/dev/null; then
    ENTRY=$(date +%s); break
  fi
  kill -0 $RUN 2>/dev/null || break
done

if [ -z "$ENTRY" ]; then
  echo "  never reached the world (see $LOG)"
  exit 1
fi
echo "  world reached; discarding ${WARMUP}s"
sleep "$WARMUP"
# Snapshot complete log lines at both boundaries, before stopping the title.
START=$(wc -l < "$LOG")
sleep "$WINDOW"
END=$(wc -l < "$LOG")
kill $RUN 2>/dev/null
stop_run

WINDOW_LOG="${LOG%.log}.window.log"
awk -v start="$START" -v end="$END" 'NR > end { exit } NR > start' "$LOG" > "$WINDOW_LOG" || {
  echo "bench_scene: could not write $WINDOW_LOG" >&2
  exit 1
}
CLEAN=$(sed 's/\x1b\[[0-9;]*m//g' "$WINDOW_LOG")
STATS=$(python3 "$HERE/tools/frames.py" "$WINDOW_LOG" --stat 2>/dev/null)
[ -n "$STATS" ] || STATS="0 0 0 0"

set -- $STATS
REM=$(grep -c 'device removed' "$LOG" 2>/dev/null); REM=${REM:-0}
CR=$(grep -c 'unhandled exception' "$LOG" 2>/dev/null); CR=${CR:-0}
CL=$(grep -c 'no argument clamp' "$LOG" 2>/dev/null); CL=${CL:-0}

printf '\n  %-30s median %5s fps   p10 %5s   mean %5s   n=%s   removals=%s crashes=%s clamp=%s\n' \
  "$LABEL" "$1" "$2" "$3" "$4" "$REM" "$CR" "$CL"
echo "  phases:"
./tools/frames.py "$WINDOW_LOG" 2>/dev/null | sed -n '/phase/,/^$/p' | sed 's/^/    /'
BIGGEST=$(ls -S "$CAPDIR"/*.png 2>/dev/null | head -1)
if [ -z "$BIGGEST" ]; then
  echo "  PICTURE: no frame captured (window too short, or capture failed) - NUMBER IS UNVERIFIED"
elif [ "$(stat -c%s "$BIGGEST")" -lt 50000 ]; then
  echo "  PICTURE: $(basename "$BIGGEST") is $(stat -c%s "$BIGGEST") bytes - BLACK. This result is not a renderer win."
fi
echo "  log: $LOG"
