#!/usr/bin/env bash
# ci-test: run a test command for CI, and report a failure where it can be read.
#
#   tools/ci-test.sh "<heading>" <command> [args...]
#
# Runs the command, shows its output and passes its exit code on. If the
# command fails:
#
# - the last 300 lines of its output go to the job summary
#   ($GITHUB_STEP_SUMMARY), under "<heading> failed";
# - the lines that name the failure become error annotations: failed tests,
#   panics, cargo's errors, how a test binary ended and which binaries ran.
#
# A test that ends the process (an access violation, say) prints no result.
# Run one at a time (--test-threads=1), the harness prints "test <name> ... "
# before a test runs and the result after it, so a name without a result is
# the test that was running. It is named in the summary and gets an
# annotation of its own.
#
# The output is kept in a file outside the checkout: xtask's
# repository_passes_policy refuses a file in the tree whose type is not on
# the allowlist. tools/check-ci-test.sh checks this script against stand-ins.
set -uo pipefail
[ $# -ge 2 ] || { echo 'usage: ci-test.sh "<heading>" <command> [args...]' >&2; exit 2; }
heading="$1"; shift
dir="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
log="${dir//\\//}/ci-test-$$.log"

"$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}

if [ "$status" -ne 0 ]; then
  # Without carriage returns, colour codes and cargo's hyperlinks, so that
  # "error" begins its line and the summary holds text only.
  plain=$(tr -d '\r' < "$log" |
    sed -E 's/\x1b\][^\x07\x1b]*(\x07|\x1b\\)//g; s/\x1b\[[0-9;?]*[A-Za-z]//g')
  # The last test line, if it has a name and no result.
  unfinished=$(printf '%s\n' "$plain" | grep -aE '^test [^ ]+ \.\.\. ' | tail -1 |
    grep -avE ' \.\.\. (ok|FAILED|ignored.*|bench: .*)$' |
    sed -E 's/^test ([^ ]+) \.\.\. .*$/\1/')
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
      echo "### $heading failed"
      echo
      if [ -n "$unfinished" ]; then
        echo "The process ended while this test ran: \`$unfinished\`"
        echo
      fi
      echo '```'
      printf '%s\n' "$plain" | tail -300
      echo '```'
    } >> "$GITHUB_STEP_SUMMARY"
  fi
  if [ -n "$unfinished" ]; then
    echo "::error title=test ended the process::$unfinished"
  fi
  printf '%s\n' "$plain" | grep -aE 'FAILED|panicked|^error|didn.t exit successfully|Running ' | tail -12 |
    while IFS= read -r line; do
      echo "::error title=test failure::${line//%/%25}"
    done
fi
rm -f "$log"
exit "$status"
