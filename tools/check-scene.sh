#!/usr/bin/env bash
# check-scene: run bench_scene.sh against a stand-in launcher.
#
#   ./tools/check-scene.sh
#
# The stand-in writes menu lines and then world lines, so the world entry,
# the window log beside the log and the median can be checked without the
# game. It also covers a log outside /tmp/opencode, which used to leave the
# capture folder empty (see issue #39).
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
fail=0

cat > "$T/stub.sh" <<'SH'
#!/usr/bin/env bash
for _ in 1 2 3; do
  echo "2026-11-27T00:00:00.000000Z  INFO victory_gpu::host: 30.0 fps, 33.3 ms/frame (1.0 preparing: copies 0.1 ms 0.1 MB, textures 0.1 ms 0.1 MB; 0.0 on the GPU; 0.1 copying back), 13 draws/frame"
  sleep 0.2
done
for i in $(seq 1 30); do
  echo "2026-11-27T00:00:00.000000Z  INFO victory_gpu::host: 4.$((i % 10)) fps, 230.0 ms/frame (40.0 preparing: copies 1.0 ms 1.0 MB, textures 1.0 ms 1.0 MB; 30.0 on the GPU; 1.0 copying back), 3000 draws/frame; heap buffers 1.0 passes 10.0 ms; textures picked by index: 1 uploaded so far (1 MB), 0 waiting, 0 passed over unmapped"
  sleep 0.2
done
SH
chmod +x "$T/stub.sh"

LOG="$T/scene.log"
OUT=$(VICTORY_LAUNCHER="$T/stub.sh" VICTORY_GAME_DIR="$T" \
  VICTORY_WINDOW=1 VICTORY_WARMUP=0 VICTORY_TOTAL=15 \
  VICTORY_BENCH_LOG="$LOG" "$HERE/tools/bench_scene.sh" check 2>&1)
STATUS=$?

[ "$STATUS" = 0 ] || { echo "FAIL: bench_scene.sh exited $STATUS"; echo "$OUT"; fail=1; }
echo "$OUT" | grep -qE 'median +4\.[0-9]+ fps' || { echo "FAIL: the stand-in world has no 4.x fps median"; echo "$OUT"; fail=1; }
echo "$OUT" | grep -qE 'n=[1-9]' || { echo "FAIL: the stand-in world has no samples"; echo "$OUT"; fail=1; }
[ -f "${LOG%.log}.window.log" ] || { echo "FAIL: no window log beside $LOG"; fail=1; }
ls -d "$T"/cap-* >/dev/null 2>&1 || { echo "FAIL: no capture folder beside the log"; fail=1; }

if [ "$fail" != 0 ]; then
  echo "--- the run:"
  echo "$OUT"
  exit 1
fi
echo "check-scene: ok"
