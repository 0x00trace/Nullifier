#!/usr/bin/env bash
# check-bench: check the benchmark and frame reports against synthetic logs.
#
#   ./tools/check-bench.sh
#
# A log with world frames and nothing wrong must be reported with no removal,
# no fault, no failed clamp and no note about either. A log with one of each
# must be reported with them. The phase report must exclude menu frames.
# Uses `bench.sh --summary` and `frames.py`.
#
# Why this exists: every clean run was reported with "the host device was
# removed" and "the indirect clamp failed to compile" (see bench.sh).
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
fail=0

for i in 1 2 3 4 5 6 7 8 9 10; do
  echo "INFO victory_gpu::host: 3.$i fps, 300.0 ms/frame (70.0 preparing; 100.0 on the GPU), 3000 draws/frame"
done > "$T/clean.log"
OUT="$("$HERE/tools/bench.sh" --summary "$T/clean.log" 2>&1)"
echo "$OUT" | grep -qE 'removals 0 +faults 0 +clamp-fail 0 ' || { echo "FAIL: a clean log is not reported as 0 removals, 0 faults, 0 failed clamps on one line"; fail=1; }
echo "$OUT" | grep -q 'device was removed' && { echo "FAIL: a clean log gets the note about a removed device"; fail=1; }
echo "$OUT" | grep -q 'clamp failed' && { echo "FAIL: a clean log gets the note about a failed clamp"; fail=1; }
echo "$OUT" | grep -qE 'median +3\.[0-9]+ fps .* n=10' || { echo "FAIL: the median of the ten world lines is missing"; fail=1; }

cp "$T/clean.log" "$T/bad.log"
{
  echo "ERROR victory_gpu::host: host GPU device removed 0x887A0005"
  echo "ERROR victory: unhandled exception 0x80000003"
  echo "WARN victory_gpu::indirect: no argument clamp: the clamp did not compile"
} >> "$T/bad.log"
OUT2="$("$HERE/tools/bench.sh" --summary "$T/bad.log" 2>&1)"
echo "$OUT2" | grep -qE 'removals 1 +faults 1 +clamp-fail 1 ' || { echo "FAIL: one removal, one fault and one failed clamp are not counted"; fail=1; }
echo "$OUT2" | grep -q 'device was removed' || { echo "FAIL: a removal gets no note"; fail=1; }
echo "$OUT2" | grep -q 'clamp failed' || { echo "FAIL: a failed clamp gets no note"; fail=1; }

echo 'INFO victory_gpu::host: 30.0 fps, 33.3 ms/frame (5.0 preparing: copies 1.0 ms 1.0 MB, textures 1.0 ms 1.0 MB; 3.0 on the GPU; 1.0 copying back)' > "$T/frames.log"
FRAMES=$("$HERE/tools/frames.py" "$T/frames.log" --csv "$T/frames.csv" --html "$T/frames.html")
grep -q '1 frames, 0 in the world' <<< "$FRAMES" || { echo "FAIL: a menu-only log has a world sample"; fail=1; }
grep -q 'median .* fps' <<< "$FRAMES" && { echo "FAIL: a menu-only log has a world median"; fail=1; }
grep -q 'No world frames' "$T/frames.html" || { echo "FAIL: a menu-only HTML report lacks the empty state"; fail=1; }
grep -q '^30.0,' "$T/frames.csv" || { echo "FAIL: a menu-only CSV omits parsed frames"; fail=1; }

echo 'INFO victory_gpu::host: 4.0 fps, 250.0 ms/frame (40.0 preparing: copies 20.0 ms 10.0 MB, textures 10.0 ms 5.0 MB; 200.0 on the GPU; 5.0 copying back)' >> "$T/frames.log"
FRAMES=$("$HERE/tools/frames.py" "$T/frames.log")
grep -q 'median 4.00 fps .*n=1' <<< "$FRAMES" || { echo "FAIL: the mixed log has the wrong world median or sample count"; fail=1; }

if [ "$fail" != 0 ]; then
  echo "--- the report of the clean log:"; echo "$OUT"
  echo "--- the report of the other:"; echo "$OUT2"
  exit 1
fi
echo "check-bench: ok"
