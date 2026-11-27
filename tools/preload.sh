#!/usr/bin/env bash
# preload: warm the pipeline cache without playing.
#
#   VICTORY_GAME_DIR=DIR ./tools/preload.sh
#
# Translates the title's shaders from its archives and builds their compute
# pipelines, without starting the title. About a minute. The pipeline library
# saves itself every 20 s while it has new pipelines, so the next real run
# starts warm.
#
# Optional; a first run compiles the same pipelines during its own loading.
set -uo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE" || exit 1

GAME="${VICTORY_GAME_DIR:-}"
[ -d "$GAME" ] || { echo "preload: set VICTORY_GAME_DIR to the game folder" >&2; exit 2; }

LOG="${XDG_CACHE_HOME:-$HOME/.cache}/nullifier/preload-game.log"
mkdir -p "$(dirname "$LOG")"

# Offline: translate the shaders and make their compute pipelines. The title
# is not started, so no menu timing or world gate is involved.
./launcher/nullifier-launch.sh --game "$GAME" --warm "$@" >"$LOG" 2>&1
STATUS=$?
if [ "$STATUS" = 0 ]; then
  echo "preload: shaders warmed"
else
  echo "preload: warming failed (see $LOG)" >&2
fi
exit "$STATUS"
