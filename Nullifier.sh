#!/usr/bin/env bash
# Nullifier - opens the graphical launcher.
# Double-click in your file manager, or: ./Nullifier.sh
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")"

BIN="tools/launcher/target/release/nullifier-launcher"
if [ ! -x "$BIN" ]; then
  CARGO="${CARGO:-}"
  if [ -z "$CARGO" ] && command -v cargo >/dev/null 2>&1; then
    CARGO="$(command -v cargo)"
  elif [ -z "$CARGO" ] && [ -x "$HOME/.local/nullifier-toolchain/cargo/bin/cargo" ]; then
    export RUSTUP_HOME="$HOME/.local/nullifier-toolchain/rustup"
    export CARGO_HOME="$HOME/.local/nullifier-toolchain/cargo"
    CARGO="$CARGO_HOME/bin/cargo"
  fi
  if [ -z "$CARGO" ]; then
    echo "Rust is not installed yet. Run ./tools/setup.sh once, then start this again."
    exit 1
  fi
  echo "Building the launcher (once)..."
  "$CARGO" build --release --manifest-path tools/launcher/Cargo.toml
fi

exec "$BIN" "$@"
