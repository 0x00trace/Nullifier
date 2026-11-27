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
CAPDIR=$(mktemp -d /tmp/opencode/cap-XXXXXX)
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
"$HERE/launcher/nullifier-launch.sh" --game "$GAME" --bin "$BIN" -- "$@" </dev/null >"$LOG" 2>&1 &
RUN=$!
set +m

# Wait for the world: the first frame line below 10 fps marks entry.
ENTRY=""
for _ in $(seq 1 "$TOTAL"); do
  sleep 1
  if grep -qE '[0-9.]+ fps' "$LOG" 2>/dev/null; then
    # Two samples below 10 fps: menus are 30, and a faster server (the view
    # tree, docs/FINDINGS.md 2.5) puts the world at 3-5 fps, so a fixed
    # "below 2" gate would never fire. One sample is a loading dip.
    if sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -oE '[0-9.]+ fps' | tr -d ' fps' \
        | tail -2 | paste -sd' ' | awk 'NF == 2 && $1 < 10 && $2 < 10 {ok = 1} END {exit !ok}'; then
      ENTRY=$(date +%s); break
    fi
  fi
  kill -0 $RUN 2>/dev/null || break
done

if [ -z "$ENTRY" ]; then
  echo "  never reached the world (see $LOG)"
  exit 1
fi
echo "  world reached; discarding ${WARMUP}s"
sleep "$WARMUP"
START=$(date +%s)
sleep "$WINDOW"
END=$(date +%s)
kill $RUN 2>/dev/null
stop_run

CLEAN=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG")
STATS=$(echo "$CLEAN" | grep -oE '^[^ ]* +INFO victory_gpu::host: [0-9.]+ fps' \
  | grep -oE '[0-9.]+ fps' | tr -d ' fps' | awk -v s="$START" -v e="$END" '$1<10{print}' \
  | sort -n | awk '{a[NR]=$1;s+=$1} END{if(!NR){print "0 0 0 0";exit}
      med=(NR%2)?a[int((NR+1)/2)]:(a[NR/2]+a[NR/2+1])/2;
      p10=a[int(NR*0.1)<1?1:int(NR*0.1)];
      printf "%.2f %.2f %d %.2f\n", med, p10, NR, s/NR}')

set -- $STATS
REM=$(grep -c 'device removed' "$LOG" 2>/dev/null); REM=${REM:-0}
CR=$(grep -c 'unhandled exception' "$LOG" 2>/dev/null); CR=${CR:-0}
CL=$(grep -c 'no argument clamp' "$LOG" 2>/dev/null); CL=${CL:-0}

printf '\n  %-30s median %5s fps   p10 %5s   mean %5s   n=%s   removals=%s crashes=%s clamp=%s\n' \
  "$LABEL" "$1" "$2" "$4" "$3" "$REM" "$CR" "$CL"
echo "  phases:"
./tools/frames.py "$LOG" 2>/dev/null | sed -n '/phase/,/^$/p' | sed 's/^/    /'
BIGGEST=$(ls -S "$CAPDIR"/*.png 2>/dev/null | head -1)
if [ -z "$BIGGEST" ]; then
  echo "  PICTURE: no frame captured (window too short, or capture failed) - NUMBER IS UNVERIFIED"
elif [ "$(stat -c%s "$BIGGEST")" -lt 50000 ]; then
  echo "  PICTURE: $(basename "$BIGGEST") is $(stat -c%s "$BIGGEST") bytes - BLACK. This result is not a renderer win."
fi
echo "  log: $LOG"
