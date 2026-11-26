#!/usr/bin/env bash
# check-bench: the report tools/bench.sh prints, against logs made here.
#
#   ./tools/check-bench.sh
#
# A log with world frames and nothing wrong must be reported with no removal,
# no fault, no failed clamp and no note about either. A log with one of each
# must be reported with them. Uses `bench.sh --summary`, so nothing is run.
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

if [ "$fail" != 0 ]; then
  echo "--- the report of the clean log:"; echo "$OUT"
  echo "--- the report of the other:"; echo "$OUT2"
  exit 1
fi
echo "check-bench: ok"
