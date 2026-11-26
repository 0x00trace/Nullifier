#!/usr/bin/env bash
# profile: what is the title actually doing for 20 s in the world?
#
#   ./tools/profile.sh              # uses the profile launcher picks
#   VICTORY_VIEW_GUARD=0 ./tools/profile.sh
#
# Prints page faults, disk I/O, context switches and the frame rate over a
# 20 s window after the world is up. Page faults are the interesting number:
# victory traps writes with page guards, and each trap is an exception through
# Wine, so faults/s is roughly what the frame budget is being spent on.
set -uo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE" || exit 1

GAME="${VICTORY_GAME_DIR:-$HOME/Downloads/GTA6.Xbox.v1.0.2.1-CYBERLEEK}"
WAIT="${VICTORY_PROFILE_WAIT:-115}"
WINDOW="${VICTORY_PROFILE_WINDOW:-20}"
LOG="/tmp/opencode/victory-profile-$(date +%H%M%S).log"
mkdir -p /tmp/opencode

export VICTORY_KEYS="${VICTORY_KEYS:-15:Enter,22:Enter,30:Enter,38:Enter,46:Enter,55:Enter,65:Enter,75:Enter,90:Enter,110:Enter,130:Enter}"
./launcher/nullifier-launch.sh --game "$GAME" "$@" >"$LOG" 2>&1 &

for _ in $(seq 1 "$WAIT"); do sleep 1; done

PID=$(for p in $(pgrep -f "victory[.]exe run"); do echo "$p $(awk '/VmRSS/{print $2}' /proc/$p/status 2>/dev/null)"; done | sort -k2 -rn | head -1 | cut -d' ' -f1)
[ -n "$PID" ] || { echo "no title process; see $LOG"; exit 1; }

sample() {
  awk '{print $10, $12}' "/proc/$PID/stat"
  awk '/voluntary_ctxt_switches/{v=$2} /nonvoluntary_ctxt_switches/{n=$2} END{print v, n}' "/proc/$PID/status"
  awk '/read_bytes/{r=$2} /write_bytes/{w=$2} END{print r, w}' "/proc/$PID/io" 2>/dev/null || echo 0 0
  awk '/VmRSS/{print $2}' "/proc/$PID/status"
}

read -r mi1 mj1 vc1 nc1 rb1 wb1 rss1 < <(sample | tr '\n' ' ')
sleep "$WINDOW"
read -r mi2 mj2 vc2 nc2 rb2 wb2 rss2 < <(sample | tr '\n' ' ')

FPS=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -oE '[0-9.]+ fps' | tail -1)
FRAME=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -E 'fps, ' | tail -1 | grep -oE '[0-9.]+ ms/frame')

printf '\n%-28s %s\n' "config" "${VICTORY_PROFILE_LABEL:-$(env | grep -cE '^VICTORY_(VIEW_GUARD|TEXTURE_GUARD|TEXTURE_ASK_EVERY)=' > /dev/null && echo defaults || echo custom)}"
printf '%-28s %s\n' "window" "${WINDOW}s in world"
printf '%-28s %s\n' "frame rate" "${FPS:-?}  (${FRAME:-?})"
printf '%-28s %d  (%d/s)\n' "minor page faults" "$((mi2-mi1))" "$(( (mi2-mi1)/WINDOW ))"
printf '%-28s %d\n' "major page faults" "$((mj2-mj1))"
printf '%-28s %d  (%d/s)\n' "ctx switches vol" "$((vc2-vc1))" "$(( (vc2-vc1)/WINDOW ))"
printf '%-28s %d\n' "ctx switches invol" "$((nc2-nc1))"
printf '%-28s %d MB read, %d MB written\n' "disk" "$(( (rb2-rb1)/1048576 ))" "$(( (wb2-wb1)/1048576 ))"
printf '%-28s %d MB\n' "RSS" "$(( rss2/1024 ))"
echo
echo "log: $LOG"
echo "stop the title: pkill -f 'victory[.]exe run'"
