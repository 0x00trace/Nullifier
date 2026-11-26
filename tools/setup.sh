#!/usr/bin/env bash
# setup: get a fresh machine from "git clone" to "the launcher works".
#
#   ./tools/setup.sh                # toolchain + build + detect
#   ./tools/setup.sh --game DIR     # ... and start the title once
#
# Idempotent, no sudo, no system packages changed: Rust goes into
# ~/.local/victory-toolchain. If a system rustup is already set up for the
# pinned toolchain, that one is used instead.
set -uo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE" || exit 1

GAME=""
while [ $# -gt 0 ]; do
  case "$1" in
    --game) GAME="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

say() { printf '\n== %s\n' "$1"; }
ok()  { printf '   ok: %s\n' "$1"; }
die() { printf '\n!! %s\n' "$1" >&2; exit 1; }

case "$(uname -s)" in
  Linux) ;;
  Darwin) die "macOS is not supported yet (docs/ROADMAP.md)." ;;
  *) die "unsupported OS: $(uname -s). On Windows use the .ps1 launcher; see docs/ONBOARD.md." ;;
esac

say "checking prerequisites"
for c in git python3 curl; do
  command -v "$c" >/dev/null || die "missing '$c'"
done
ok "git, python3, curl"

if ! command -v clang >/dev/null || ! command -v lld-link >/dev/null; then
  die "cargo-xwin needs clang and lld-link (on Arch: pacman -S clang lld; Debian: apt install clang lld)"
fi
ok "clang, lld-link"

say "rust toolchain"
TOOLCHAIN_ENV=(RUSTUP_HOME="$HOME/.local/victory-toolchain/rustup" CARGO_HOME="$HOME/.local/victory-toolchain/cargo")
LOCAL_CARGO="$HOME/.local/victory-toolchain/cargo/bin/cargo"

if [ -x "$LOCAL_CARGO" ]; then
  ok "using $LOCAL_CARGO"
elif command -v cargo >/dev/null && command -v rustup >/dev/null; then
  ok "using system cargo/rustup"
  TOOLCHAIN_ENV=()
elif command -v cargo >/dev/null && ! command -v rustup >/dev/null; then
  die "a system cargo without rustup cannot add the Windows target; install rustup or remove 'cargo' from PATH to let this script install one into ~/.local"
else
  echo "   installing rustup into ~/.local/victory-toolchain (no sudo)"
  mkdir -p "$HOME/.local/victory-toolchain"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/victory-rustup-init.sh || die "rustup download failed"
  RUSTUP_HOME="$HOME/.local/victory-toolchain/rustup" \
  CARGO_HOME="$HOME/.local/victory-toolchain/cargo" \
    sh /tmp/victory-rustup-init.sh -y --profile minimal --no-modify-path \
      --default-toolchain "$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)" \
      --target x86_64-pc-windows-msvc || die "rustup install failed"
  TOOLCHAIN_ENV=(RUSTUP_HOME="$HOME/.local/victory-toolchain/rustup" CARGO_HOME="$HOME/.local/victory-toolchain/cargo")
  ok "installed"
fi

export "${TOOLCHAIN_ENV[@]:-}"
export PATH="$HOME/.local/victory-toolchain/cargo/bin:$PATH"

# pinned toolchain + Windows target (rustup handles the toolchain file itself)
rustup target add x86_64-pc-windows-msvc >/dev/null 2>&1 && ok "x86_64-pc-windows-msvc target" \
  || echo "   note: could not add the target (already present, or rustup is not managed here)"

say "cargo-xwin"
if cargo xwin --version >/dev/null 2>&1; then
  ok "$(cargo xwin --version | head -1)"
else
  echo "   installing cargo-xwin (a few minutes)"
  cargo install cargo-xwin --locked >/dev/null 2>&1 || die "cargo install cargo-xwin failed"
  ok "installed"
fi

say "building victory.exe (diag, diagnostics)"
cargo xwin build --profile diag -p victory --features diagnostics \
  --target x86_64-pc-windows-msvc || die "build failed"
BIN="$HERE/target/x86_64-pc-windows-msvc/diag/victory.exe"
[ -f "$BIN" ] || die "build produced no binary"
ok "$(du -h "$BIN" | cut -f1) $BIN"

say "hardware and profile"
./launcher/victory-launch.sh --detect || die "launcher --detect failed"

if [ -n "$GAME" ]; then
  say "starting the title (Ctrl-C to stop)"
  ./launcher/victory-launch.sh --game "$GAME"
else
  say "next"
  cat <<EOF
   ./tools/build-wineserver.sh     (once: Wine's server with its view tree,
                                    several times the frame rate; docs/LAUNCHER.md)
   ./launcher/victory-launch.sh --game <your game folder>

   If a run dies with "device removed", use --safe.
   Contributions: docs/ONBOARD.md, then AGENTS.md and docs/MIGRATION.md.
EOF
fi
