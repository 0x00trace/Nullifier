#!/usr/bin/env bash
# check-ci-test: tools/ci-test.sh against stand-ins for the test command.
#
#   ./tools/check-ci-test.sh
#
# Three stand-ins: tests that pass, a test that fails, and a test that ends
# the process while the tests run one at a time. For each it checks the exit
# code that is passed on, the job summary and the error annotations, and that
# no file is left where the command ran.
#
# Why this exists: the CI step that runs victory-gpu's tests one at a time
# failed the job but left the failing test's name in the raw step log only,
# and a test that ends the process prints no result at all.
#
# CI_TEST names another script to check in place of tools/ci-test.sh.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
WRAP="${CI_TEST:-$HERE/tools/ci-test.sh}"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir "$T/cwd" "$T/tmp"

fail() {
  echo "FAIL: $1"
  echo "--- output"; cat "$T/out"
  echo "--- summary"; cat "$T/summary"
  exit 1
}
# run <case> <expected exit code>: the stand-in is the script $T/<case>.sh
run() {
  : > "$T/summary"
  (cd "$T/cwd" && RUNNER_TEMP="$T/tmp" GITHUB_STEP_SUMMARY="$T/summary" \
    bash "$WRAP" "Stand-in tests" bash "$T/$1.sh") > "$T/out" 2>&1
  local got=$?
  [ "$got" = "$2" ] || fail "$1: exit code $got, expected $2"
  [ -z "$(ls -A "$T/cwd")" ] || fail "$1: a file was left where the command ran"
  [ -z "$(ls -A "$T/tmp")" ] || fail "$1: the log was left in the temporary folder"
}
has() { grep -qF -- "$2" "$1" || fail "$3: '$2' is missing"; }
hasnt() { ! grep -qF -- "$2" "$1" || fail "$3: '$2' should not be there"; }

# 1. Tests that pass: the output is shown, and nothing is reported.
cat > "$T/pass.sh" <<'EOF'
echo 'test a::one ... ok'
echo 'test result: ok. 1 passed; 0 failed'
EOF
run pass 0
has "$T/out" 'test a::one ... ok' pass
hasnt "$T/out" '::error' pass
[ ! -s "$T/summary" ] || fail "pass: the summary is not empty"

# 2. A test that fails, with cargo's colours and a hyperlink in the output:
# its line, the panic, cargo's error and the binary are annotated.
cat > "$T/failed.sh" <<'EOF'
printf '    Finished \033]8;;https://example.invalid/profiles\033\\`test` profile\033]8;;\033\\ target(s) in 0.15s\n' >&2
echo '     Running unittests src/lib.rs (target/debug/deps/x-0123.exe)'
echo 'test a::one ... ok'
echo 'test b::two ... FAILED'
echo "thread 'b::two' panicked at src/b.rs:3:5:"
echo 'test result: FAILED. 1 passed; 1 failed; 0 ignored'
printf '\033[1m\033[91merror\033[0m: test failed, to rerun pass `-p x --lib`\n' >&2
exit 101
EOF
run failed 101
has "$T/out" '::error title=test failure::test b::two ... FAILED' failed
has "$T/out" "::error title=test failure::thread 'b::two' panicked at src/b.rs:3:5:" failed
has "$T/out" '::error title=test failure::error: test failed, to rerun pass `-p x --lib`' failed
has "$T/out" '::error title=test failure::     Running unittests src/lib.rs' failed
hasnt "$T/out" 'test ended the process' failed
has "$T/summary" '### Stand-in tests failed' failed
has "$T/summary" 'test b::two ... FAILED' failed
has "$T/summary" '    Finished `test` profile target(s) in 0.15s' failed
hasnt "$T/summary" 'The process ended while this test ran' failed
! grep -q $'\033' "$T/summary" || fail "failed: an escape code is left in the summary"

# 3. One at a time, a test that ends the process: the harness has printed its
# name and no result, and cargo's error follows on the same line.
cat > "$T/ended.sh" <<'EOF'
echo '     Running unittests src/lib.rs (target/debug/deps/x-0123.exe)'
echo 'test a::one ... ok'
printf 'test b::two ... '
printf '\033[1m\033[91merror\033[0m: test failed, to rerun pass `-p x --lib`\n\nCaused by:\n' >&2
echo "  process didn't exit successfully: \`x-0123.exe\` (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)" >&2
exit 139
EOF
run ended 139
has "$T/out" '::error title=test ended the process::b::two' ended
has "$T/out" "::error title=test failure::  process didn't exit successfully: \`x-0123.exe\` (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)" ended
has "$T/summary" 'The process ended while this test ran: `b::two`' ended
has "$T/summary" 'test a::one ... ok' ended

echo "check-ci-test: ok (passing tests, a failed test, a test that ends the process)"
