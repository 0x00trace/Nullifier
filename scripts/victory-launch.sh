#!/usr/bin/env bash
# victory-launch: pick the best-tested settings for the machine it runs on.
#
#   ./scripts/victory-launch.sh --detect            # print what it found/chose
#   ./scripts/victory-launch.sh --game DIR          # play
#   ./scripts/victory-launch.sh --game DIR -- --console series-s
#
# Everything it sets can be overridden by exporting the variable first.
# Measurements behind this matrix are in docs/FORK.md.
set -uo pipefail

STEAM_ROOT="${STEAM_ROOT:-$HOME/.local/share/Steam}"
GAME="${VICTORY_GAME_DIR:-}"
TITLE_EXE="${VICTORY_TITLE_EXE:-game_scarlett_master.exe}"
BIN="${VICTORY_BIN:-}"
MODE="play"
PASSTHRU=()

# --- the one vkd3d-proton that does not segfault RADV on the world pipelines.
# GE-Proton11-10 ships vkd3d-1.1-5656; the fix is in 5742+ (Proton Experimental).
MIN_VKD3D=5742

while [ $# -gt 0 ]; do
  case "$1" in
    --detect) MODE="detect"; shift ;;
    --game) GAME="$2"; shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    --) shift; PASSTHRU=("$@"); break ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

say() { printf '%-22s %s\n' "$1" "$2"; }

# ---------------- hardware ----------------
gpu_vendor() {
  local line
  line=$(lspci -nn 2>/dev/null | grep -iE 'vga|3d|display' | head -1)
  case "$line" in
    *[Nn][Vv][Ii][Dd][Ii][Aa]*) echo nvidia ;;
    *[Aa][Mm][Dd]*|*[Aa][Tt][Ii]*) echo amd ;;
    *[Ii][Nn][Tt][Ee][Ll]*) echo intel ;;
    *) echo unknown ;;
  esac
}

vram_mb() {
  local f
  f=$(ls /sys/class/drm/card*/device/mem_info_vram_total 2>/dev/null | head -1)
  [ -n "$f" ] && echo $(( $(cat "$f") / 1048576 )) || echo 0
}

# ---------------- proton / vkd3d ----------------
find_proton() {
  local d best=""
  for d in "$STEAM_ROOT/compatibilitytools.d"/GE-Proton* "$STEAM_ROOT/steamapps/common/Proton - Experimental"; do
    [ -x "$d/files/bin/wine" ] || continue
    best="$d"; break
  done
  echo "$best"
}

vkd3d_build() { # prints the numeric build in a proton's vkd3d-proton, or 0
  local v
  v=$(cat "$1/files/lib/wine/vkd3d-proton/version" 2>/dev/null) || { echo 0; return; }
  echo "$v" | grep -oE 'vkd3d-1\.1-[0-9]+' | grep -oE '[0-9]+$' || echo 0
}

newest_vkd3d_proton() { # the installed proton with the newest vkd3d-proton
  local d best="" bestn=0 n
  for d in "$STEAM_ROOT/compatibilitytools.d"/GE-Proton* "$STEAM_ROOT/steamapps/common/Proton - Experimental"; do
    [ -d "$d" ] || continue
    n=$(vkd3d_build "$d")
    [ "$n" -gt "$bestn" ] && { bestn=$n; best=$d; }
  done
  echo "$best"
}

# ---------------- settings matrix ----------------
VENDOR=$(gpu_vendor); VRAM=$(vram_mb)

CARD_MB=1024
case "$VRAM" in
  ""|0) CARD_MB=1024 ;;
  *) if [ "$VRAM" -lt 10000 ]; then CARD_MB=512; elif [ "$VRAM" -lt 13000 ]; then CARD_MB=768; else CARD_MB=1024; fi ;;
esac

export VICTORY_TEXTURE_NO_WAIT="${VICTORY_TEXTURE_NO_WAIT:-1}"
export VICTORY_LATE_WAIT="${VICTORY_LATE_WAIT:-1}"
export VICTORY_WIDE_BACK="${VICTORY_WIDE_BACK:-1}"
export VICTORY_CARD_BUFFERS_MIN_MB="${VICTORY_CARD_BUFFERS_MIN_MB:-1}"
export VICTORY_CARD_BUFFERS_MB="${VICTORY_CARD_BUFFERS_MB:-$CARD_MB}"
# Never default these on Linux: they remove the device (0x887A0005) within minutes.
export VICTORY_RELAY="${VICTORY_RELAY:-0}"

case "$VENDOR" in
  nvidia)
    # vkd3d-proton + NVIDIA raw access chains is a known crash path on Linux.
    export VKD3D_DISABLE_EXTENSIONS="${VKD3D_DISABLE_EXTENSIONS:-VK_NV_raw_access_chains}"
    ;;
  amd) : ;;
  *) : ;;
esac

# ---------------- report ----------------
PROTON=$(find_proton)
NEWEST=$(newest_vkd3d_proton)
CUR=$(vkd3d_build "${PROTON:-/nonexistent}")
VKFILE="${VICTORY_PREFIX:-$HOME/.local/share/victory/prefix}/.vkd3d-build"
INSTALLED=$(cat "$VKFILE" 2>/dev/null || echo 0)

if [ "$MODE" = "detect" ]; then
  say "OS"                 "$(uname -s) $(uname -r)"
  say "GPU vendor"         "$VENDOR"
  say "VRAM"               "${VRAM} MB"
  say "Proton"             "${PROTON:-not found}"
  say "proton's vkd3d"     "$CUR (need >= $MIN_VKD3D)"
  say "prefix vkd3d"        "$INSTALLED (what actually runs)"
  say "newest available"   "${NEWEST:-none} ($(vkd3d_build "${NEWEST:-/nonexistent}"))"
  echo
  say "TEXTURE_NO_WAIT"    "$VICTORY_TEXTURE_NO_WAIT"
  say "LATE_WAIT"          "$VICTORY_LATE_WAIT"
  say "WIDE_BACK"          "$VICTORY_WIDE_BACK"
  say "CARD_BUFFERS_MB"    "$VICTORY_CARD_BUFFERS_MB"
  say "CARD_BUFFERS_MIN"   "$VICTORY_CARD_BUFFERS_MIN_MB"
  [ "$VENDOR" = nvidia ] && say "VKD3D_DISABLE_EXT" "$VKD3D_DISABLE_EXTENSIONS"
  exit 0
fi

[ -n "$GAME" ] || { echo "need --game DIR (or VICTORY_GAME_DIR)" >&2; exit 2; }
[ -d "$GAME" ] || { echo "not a directory: $GAME" >&2; exit 2; }
[ -n "$PROTON" ] || { echo "no Proton install found under $STEAM_ROOT" >&2; exit 2; }

# ---------------- prefix ----------------
export STEAM_COMPAT_CLIENT_INSTALL_PATH="$STEAM_ROOT"
export STEAM_COMPAT_DATA_PATH="${VICTORY_PREFIX:-$HOME/.local/share/victory/prefix}"
mkdir -p "$STEAM_COMPAT_DATA_PATH"
export WINEPREFIX="$STEAM_COMPAT_DATA_PATH/pfx"
export WINEDEBUG="${WINEDEBUG:--all}"
export WINEDLLOVERRIDES="d3d12=n;d3d12core=n;dxgi=n"

# A vkd3d-proton older than the fix segfaults RADV while compiling the first
# world mesh pipeline. Copy in the newest one once, keeping the old as .bak.
if [ ! -d "$WINEPREFIX" ]; then
  echo "first run: creating the wine prefix through Proton (a minute or two)" >&2
  STEAM_COMPAT_CLIENT_INSTALL_PATH="$STEAM_ROOT" \
  STEAM_COMPAT_DATA_PATH="$STEAM_COMPAT_DATA_PATH" \
  PROTONFIXES_DISABLE=1 \
    "$PROTON/proton" run 'C:\windows\system32\cmd.exe' /c exit >/dev/null 2>&1 || true
fi
if [ "$INSTALLED" -lt "$MIN_VKD3D" ] && [ "$CUR" -lt "$MIN_VKD3D" ] && [ -n "$NEWEST" ] && [ "$(vkd3d_build "$NEWEST")" -ge "$MIN_VKD3D" ]; then
  SYS32="$WINEPREFIX/drive_c/windows/system32"
  if [ -d "$SYS32" ]; then
    echo "vkd3d-proton $CUR is older than $MIN_VKD3D; taking $(vkd3d_build "$NEWEST") from $(basename "$NEWEST")" >&2
    for f in d3d12.dll d3d12core.dll; do
      [ -f "$SYS32/$f.bak" ] || cp -n "$SYS32/$f" "$SYS32/$f.bak" 2>/dev/null
      cp "$NEWEST/files/lib/wine/vkd3d-proton/x86_64-windows/$f" "$SYS32/$f"
    done
    vkd3d_build "$NEWEST" > "$VKFILE"
  else
    echo "prefix not created yet: run once, then launch again to swap vkd3d-proton" >&2
  fi
fi

# ---------------- memory hogs ----------------
if command -v systemctl >/dev/null && systemctl --user is-active --quiet assistant-brain.service 2>/dev/null; then
  echo "warning: assistant-brain.service holds ~11 GB VRAM; stop it for play:" >&2
  echo "         systemctl --user stop assistant-watchdog.service assistant-brain.service" >&2
fi
if pgrep -x ollama >/dev/null 2>&1 || pgrep -x llama-server >/dev/null 2>&1; then
  echo "warning: a local LLM is running and will share VRAM" >&2
fi

# ---------------- go ----------------
[ -n "$BIN" ] || BIN="$PWD/target/x86_64-pc-windows-msvc/diag/victory.exe"
[ -f "$BIN" ] || { echo "no victory.exe at $BIN (cargo xwin build --profile diag -p victory --features diagnostics --target x86_64-pc-windows-msvc)" >&2; exit 2; }

WIN_GAME="Z:${GAME//\//\\}"
WIN_BIN="Z:${BIN//\//\\}"
exec "$PROTON/files/bin/wine" "$WIN_BIN" run "$WIN_GAME\\$TITLE_EXE" --execute "${PASSTHRU[@]}"
