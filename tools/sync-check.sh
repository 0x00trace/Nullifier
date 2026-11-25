#!/usr/bin/env bash
# sync-check: is this clone behind the universal fork, and what changed?
#
#   ./tools/sync-check.sh          # report; exit 10 if behind, 0 if current
#   ./tools/sync-check.sh --pull   # fast-forward if the tree is clean, then report
#
# Run it at the start of every session. If main moved, rebase your work on it
# and re-run tools/bench.sh before you trust an old number: a fix upstream can
# change what your change is worth.
set -uo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE" || exit 1

PULL=0
[ "${1:-}" = "--pull" ] && PULL=1

command -v git >/dev/null || { echo "git missing"; exit 2; }
git remote get-url origin >/dev/null 2>&1 || { echo "no 'origin' remote; add it: git remote add origin git@github.com:iNiKKo/victory-universal.git"; exit 2; }

BRANCH=$(git rev-parse --abbrev-ref HEAD)
if ! git rev-parse --abbrev-ref '@{u}' >/dev/null 2>&1; then
  echo "branch '$BRANCH' has no upstream. If it is your work branch, compare with main instead:"
  echo "  git fetch origin && git log --oneline -10 origin/main"
  exit 2
fi

echo "fetching origin..."
git fetch --quiet origin || { echo "fetch failed (network or SSH key)"; exit 1; }

BEHIND=$(git rev-list --count "HEAD..@{u}")
AHEAD=$(git rev-list --count "@{u}..HEAD")
DIRTY=$(git status --porcelain | wc -l | tr -d ' ')

echo
echo "branch   $BRANCH -> @{u}"
echo "behind   $BEHIND commit(s)"
echo "ahead    $AHEAD commit(s) of your own"
echo "dirty    $DIRTY file(s)"

if [ "$DIRTY" != "0" ]; then
  echo
  echo "uncommitted work:"
  git status --short | head -10
  echo "  commit it on a wip/ branch before pulling or rebasing."
fi

if [ "$BEHIND" = "0" ]; then
  echo
  echo "up to date."
  [ "$AHEAD" != "0" ] && echo "you have $AHEAD local commit(s) to push."
  exit 0
fi

echo
echo "new commits on the fork:"
git log --oneline "HEAD..@{u}" | head -20

CHANGED=$(git diff --name-only "HEAD..@{u}")
echo
echo "files touched:"
echo "$CHANGED" | head -25
[ "$(echo "$CHANGED" | wc -l | tr -d ' ')" -gt 25 ] && echo "  ... and more"

echo
case "$CHANGED" in
  *launcher/profiles.json*) echo "!! launcher/profiles.json changed: re-run ./launcher/victory-launch.sh --detect" ;;
esac
case "$CHANGED" in
  *AGENTS.md*|*docs/AI-PROMPT.md*|*docs/ONBOARD.md*) echo "!! the rules changed: re-read AGENTS.md before continuing" ;;
esac
case "$CHANGED" in
  *crates/*) echo "!! engine code changed: rebuild and re-benchmark; your old numbers may not apply" ;;
esac

if [ "$PULL" = "1" ] && [ "$DIRTY" = "0" ] && [ "$AHEAD" = "0" ]; then
  echo
  echo "fast-forwarding..."
  git merge --ff-only '@{u}' || { echo "not a fast-forward; rebase by hand"; exit 1; }
  echo "done. rebuild: ./tools/setup.sh  (or cargo xwin build ...)"
  exit 0
fi

echo
echo "to update: git pull --ff-only   (or rebase your branch onto origin/main)"
exit 10
