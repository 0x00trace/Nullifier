#!/usr/bin/env bash
# preload: warm the pipeline cache without playing.
#
#   VICTORY_GAME_DIR=DIR ./tools/preload.sh
#
# Starts the title until the world is drawing (two frame lines below 10 fps,
# as tools/bench_scene.sh gates it), lets the first world pipelines compile,
# then stops. About two minutes. The pipeline library saves itself every 20 s
# while it has new pipelines, so the next real run starts warm.
#
# Optional; a first run compiles the same pipelines during its own loading.
set -uo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE" || exit 1

GAME="${VICTORY_GAME_DIR:-}"
[ -d "$GAME" ] || { echo "preload: set VICTORY_GAME_DIR to the game folder" >&2; exit 2; }

LOG="${XDG_CACHE_HOME:-$HOME/.cache}/nullifier/preload.log"
mkdir -p "$(dirname "$LOG")"

# The same gentle opening sequence the bench uses: it only dismisses menus.
export VICTORY_KEYS="${VICTORY_KEYS:-15:Enter,22:Enter,30:Enter,38:Enter,46:Enter,55:Enter,65:Enter,75:Enter,90:Enter,110:Enter,130:Enter}"

./launcher/nullifier-launch.sh --game "$GAME" "$@" >"$LOG" 2>&1 &
RUN=$!

ENTRY=0
for _ in $(seq 1 200); do
  sleep 1
  if sed 's/\x1b\[[0-9;]*m//g' "$LOG" 2>/dev/null \
      | grep -oE '[0-9.]+ fps' | tr -d ' fps' | tail -2 | paste -sd' ' \
      | awk 'NF == 2 && $1 < 10 && $2 < 10 {ok = 1} END {exit !ok}'; then
    ENTRY=1
    break
  fi
  kill -0 "$RUN" 2>/dev/null || break
done

if [ "$ENTRY" != 1 ]; then
  echo "preload: the world was not reached (see $LOG)" >&2
  kill "$RUN" 2>/dev/null
  for p in $(pgrep -f "nullifier[.]exe run"); do kill -9 "$p" 2>/dev/null; done
  exit 1
fi

echo "preload: world reached, warming the first world pipelines"
sleep 12
kill "$RUN" 2>/dev/null
for p in $(pgrep -f "nullifier[.]exe run"); do kill -9 "$p" 2>/dev/null; done
echo "preload: done"
exit 0
