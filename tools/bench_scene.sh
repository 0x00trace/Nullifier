#!/usr/bin/env bash
# bench_scene: measure the same *slice* of the world every time.
#
#   ./tools/bench_scene.sh "label" [-- extra victory args]
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

GAME="${VICTORY_GAME_DIR:-$HOME/Downloads/GTA6.Xbox.v1.0.2.1-CYBERLEEK}"
BIN="${VICTORY_BIN:-$HERE/target/x86_64-pc-windows-msvc/diag/victory.exe}"
WINDOW="${VICTORY_WINDOW:-60}"          # seconds measured once the world runs
WARMUP="${VICTORY_WARMUP:-20}"          # seconds after world entry discarded
TOTAL="${VICTORY_TOTAL:-260}"
LOG="${VICTORY_BENCH_LOG:-/tmp/opencode/scene-$(date +%H%M%S).log}"
export VICTORY_KEYS="${VICTORY_KEYS:-15:Enter,22:Enter,30:Enter,38:Enter,46:Enter,55:Enter,65:Enter,75:Enter,90:Enter,110:Enter,130:Enter}"

mkdir -p "$(dirname "$LOG")"
echo "bench_scene: $LABEL"
echo "  window ${WINDOW}s (+${WARMUP}s warm-up), log $LOG"

"$HERE/launcher/victory-launch.sh" --game "$GAME" --bin "$BIN" -- "$@" >"$LOG" 2>&1 &
RUN=$!

# Wait for the world: the first frame line below 10 fps marks entry.
ENTRY=""
for _ in $(seq 1 "$TOTAL"); do
  sleep 1
  if grep -qE '[0-9.]+ fps' "$LOG" 2>/dev/null; then
    if sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -oE '[0-9.]+ fps' | tr -d ' fps' | awk '$1 < 2 {f=1} END{exit !f}'; then
      ENTRY=$(date +%s); break
    fi
  fi
  kill -0 $RUN 2>/dev/null || break
done

if [ -z "$ENTRY" ]; then
  echo "  never reached the world (see $LOG)"; kill $RUN 2>/dev/null; exit 1
fi
echo "  world reached; discarding ${WARMUP}s"
sleep "$WARMUP"
START=$(date +%s)
sleep "$WINDOW"
END=$(date +%s)
kill $RUN 2>/dev/null
for p in $(pgrep -f "victory[.]exe run"); do kill -9 "$p" 2>/dev/null; done

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
echo "  log: $LOG"
