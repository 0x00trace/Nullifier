#!/usr/bin/env bash
# Nullifier, one command:
#   curl -fsSL https://raw.githubusercontent.com/iNiKKo/Nullifier/main/install.sh | bash
#
# Clones into ~/Nullifier (override with NULLIFIER_DIR), builds the engine,
# builds the fast graphics server (optional), builds the launcher and opens it.
# NULLIFIER_NO_LAUNCH=1 stops before opening the window.
set -euo pipefail

REPO="${NULLIFIER_REPO:-https://github.com/iNiKKo/Nullifier.git}"
DIR="${NULLIFIER_DIR:-$HOME/Nullifier}"

command -v git >/dev/null || { echo "git is required"; exit 1; }

if [ ! -d "$DIR/.git" ]; then
  echo "== cloning into $DIR"
  git clone --depth 1 "$REPO" "$DIR"
fi
cd "$DIR"

echo "== building the engine (about ten minutes the first time)"
./tools/setup.sh

echo "== building the fast graphics server (optional, several times faster)"
./tools/build-wineserver.sh || echo "   skipped; the launcher can build it later"

echo "== building the launcher"
if command -v cargo >/dev/null 2>&1; then
  cargo build --release --manifest-path tools/launcher/Cargo.toml
else
  ./Nullifier.sh >/dev/null 2>&1 || true
fi

echo
echo "Done. In the window: drop your game folder on the page and press Play."
[ "${NULLIFIER_NO_LAUNCH:-0}" = 1 ] || exec ./Nullifier.sh
