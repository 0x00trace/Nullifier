#!/usr/bin/env bash
# victory-launch — Linux/macOS launcher. Settings come from launcher/profiles.json
# so Windows and Linux never disagree about what "best settings" means.
#
#   ./launcher/victory-launch.sh --detect          show hardware + chosen profile
#   ./launcher/victory-launch.sh --list            list profiles
#   ./launcher/victory-launch.sh --game DIR        play
#   ./launcher/victory-launch.sh --game DIR --safe play, no send-ahead
#
# Anything you export overrides the profile (VICTORY_*, VKD3D_*).
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
PROFILES="$HERE/profiles.json"

GAME="${VICTORY_GAME_DIR:-}"
BIN="${VICTORY_BIN:-}"
TITLE_EXE="${VICTORY_TITLE_EXE:-game_scarlett_master.exe}"
FORCE_PROFILE=""
EXTRA_PROFILE=()
MODE="play"
PASSTHRU=()

# vkd3d-proton builds older than this segfault RADV on the first world mesh
# pipeline (exit 3). Proton Experimental ships 5742+.
MIN_VKD3D=5742

while [ $# -gt 0 ]; do
  case "$1" in
    --detect) MODE="detect"; shift ;;
    --list) MODE="list"; shift ;;
    --game) GAME="${2:-}"; shift 2 ;;
    --bin) BIN="${2:-}"; shift 2 ;;
    --profile) FORCE_PROFILE="${2:-}"; shift 2 ;;
    --safe) EXTRA_PROFILE+=(safe); shift ;;
    --diagnostics) EXTRA_PROFILE+=(diagnostics); shift ;;
    --) shift; PASSTHRU=("$@"); break ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1 (try --help)" >&2; exit 2 ;;
  esac
done

command -v python3 >/dev/null || { echo "python3 is required to read profiles.json" >&2; exit 2; }

say() { printf '%-24s %s\n' "$1" "$2"; }

# ---------------- hardware ----------------
case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *) OS=unknown ;;
esac

if [ "$OS" = linux ]; then
  line=$(lspci -nn 2>/dev/null | grep -iE 'vga|3d|display' | head -1)
  case "$line" in
    *[Nn][Vv][Ii][Dd][Ii][Aa]*) VENDOR=nvidia ;;
    *[Aa][Mm][Dd]*|*[Aa][Tt][Ii]*) VENDOR=amd ;;
    *[Ii][Nn][Tt][Ee][Ll]*) VENDOR=intel ;;
    *) VENDOR=unknown ;;
  esac
  f=$(ls /sys/class/drm/card*/device/mem_info_vram_total 2>/dev/null | head -1)
  if [ -n "$f" ]; then VRAM=$(( $(cat "$f") / 1048576 )); else VRAM=0; fi
else
  VENDOR=apple; VRAM=0
fi

# ---------------- profile ----------------
resolve() { # prints KEY=VALUE lines
  python3 - "$PROFILES" "$OS" "$VENDOR" "$VRAM" "$FORCE_PROFILE" "$MODE" "${EXTRA_PROFILE[@]:-}" <<'PY'
import json, sys
path, os_name, vendor, vram, forced, mode, *extras = sys.argv[1:]
data = json.load(open(path))
profiles = data["profiles"]
overrides = {o["name"]: o for o in data.get("overrides", [])}

if mode == "list":
    for p in profiles:
        m = p["match"]
        print(f'{p["name"]:<20} os={m.get("os","*"):<8} vendor={m.get("vendor","*"):<8} '
              f'confidence={p.get("confidence","?"):<12} {p.get("note","")[:70]}')
    sys.exit(0)

def applies(p):
    m = p["match"]
    if m.get("os", "*") not in ("*", os_name): return False
    if m.get("vendor", "*") not in ("*", vendor): return False
    try: v = int(vram)
    except ValueError: v = 0
    if "vram_min_mb" in m and v < m["vram_min_mb"]: return False
    if "vram_max_mb" in m and v > m["vram_max_mb"]: return False
    return True

chosen = None
if forced:
    chosen = next((p for p in profiles if p["name"] == forced), None)
if chosen is None:
    chosen = next((p for p in profiles if applies(p)), None)
if chosen is None:
    print(f'PROFILE_ERROR=no profile applies to os={os_name} vendor={vendor} vram={vram}', file=sys.stderr)
    sys.exit(3)

sets = dict(data.get("defaults", {}))
sets.update(chosen["sets"])
for name in extras:
    if not name: continue
    o = overrides.get(name)
    if o: sets.update(o["sets"])

print(f'PROFILE_NAME={chosen["name"]}')
print(f'PROFILE_NOTE={chosen.get("note","")}')
print(f'PROFILE_CONF={chosen.get("confidence","?")}')
for k, v in sorted(sets.items()):
    print(f"{k}={v}")
PY
}

RESOLVED=$(resolve) || { echo "could not resolve a profile" >&2; exit 3; }

if [ "$MODE" = "list" ]; then echo "$RESOLVED"; exit 0; fi

PROFILE_NAME=$(echo "$RESOLVED" | sed -n 's/^PROFILE_NAME=//p')
PROFILE_NOTE=$(echo "$RESOLVED" | sed -n 's/^PROFILE_NOTE=//p')
PROFILE_CONF=$(echo "$RESOLVED" | sed -n 's/^PROFILE_CONF=//p')

# Profile values do not override what the user already exported.
while IFS='=' read -r k v; do
  case "$k" in PROFILE_*|"") continue ;; esac
  [ -n "${!k:-}" ] || export "$k=$v"
done <<< "$(echo "$RESOLVED" | grep -vE '^PROFILE_')"

if [ "$MODE" = "detect" ]; then
  say "OS" "$OS $(uname -r)"
  say "GPU vendor" "$VENDOR"
  say "VRAM" "$VRAM MB"
  say "profile" "$PROFILE_NAME ($PROFILE_CONF)"
  echo
  env | grep -E '^(VICTORY_|VKD3D_)' | sort | sed 's/^/  /'
  echo
  say "why" "$PROFILE_NOTE"
  exit 0
fi

case "$OS" in
  macos) echo "macOS is not supported yet: needs a Metal/D3D12 path (see docs/ROADMAP.md)." >&2; exit 4 ;;
  unknown) echo "unsupported operating system: $(uname -s)" >&2; exit 4 ;;
esac

# ---------------- proton ----------------
STEAM_ROOT="${STEAM_ROOT:-$HOME/.local/share/Steam}"
PROTON="${VICTORY_PROTON:-}"
if [ -z "$PROTON" ]; then
  # Proton Experimental's wine first: measured ~25% cheaper per frame than
  # GE-Proton11-10's (1,493/1,441 vs 1,965 ms) on the same build and profile.
  for d in "$STEAM_ROOT/steamapps/common/Proton - Experimental" "$STEAM_ROOT/compatibilitytools.d"/GE-Proton*; do
    [ -x "$d/files/bin/wine" ] && { PROTON="$d"; break; }
  done
fi
[ -n "$PROTON" ] || { echo "no Proton found under $STEAM_ROOT (set VICTORY_PROTON)" >&2; exit 2; }

vkd3d_build() { cat "$1/files/lib/wine/vkd3d-proton/version" 2>/dev/null | grep -oE 'vkd3d-1\.1-[0-9]+' | grep -oE '[0-9]+$' || echo 0; }
newest_proton() {
  local d best="" b=0 n
  for d in "$STEAM_ROOT/compatibilitytools.d"/GE-Proton* "$STEAM_ROOT/steamapps/common/Proton - Experimental"; do
    [ -d "$d" ] || continue
    n=$(vkd3d_build "$d"); [ "$n" -gt "$b" ] && { b=$n; best=$d; }
  done
  echo "$best"
}

export STEAM_COMPAT_CLIENT_INSTALL_PATH="$STEAM_ROOT"
export STEAM_COMPAT_DATA_PATH="${VICTORY_PREFIX:-$HOME/.local/share/victory/prefix}"
export WINEPREFIX="$STEAM_COMPAT_DATA_PATH/pfx"
export WINEDEBUG="${WINEDEBUG:--all}"
export WINEDLLOVERRIDES="d3d12=n;d3d12core=n;dxgi=n"
export PROTONFIXES_DISABLE=1

VKFILE="$STEAM_COMPAT_DATA_PATH/.vkd3d-build"
INSTALLED=$(cat "$VKFILE" 2>/dev/null || echo 0)

if [ ! -d "$WINEPREFIX" ]; then
  echo "first run: creating the wine prefix through Proton (a minute or two)" >&2
  "$PROTON/proton" run 'C:\windows\system32\cmd.exe' /c exit >/dev/null 2>&1 || true
fi

if [ "$INSTALLED" -lt "$MIN_VKD3D" ] && [ "$(vkd3d_build "$PROTON")" -lt "$MIN_VKD3D" ]; then
  NEW=$(newest_proton)
  if [ -n "$NEW" ] && [ "$(vkd3d_build "$NEW")" -ge "$MIN_VKD3D" ] && [ -d "$WINEPREFIX/drive_c/windows/system32" ]; then
    echo "installing vkd3d-proton $(vkd3d_build "$NEW") from $(basename "$NEW") (this Proton ships $(vkd3d_build "$PROTON"))" >&2
    SYS32="$WINEPREFIX/drive_c/windows/system32"
    for f in d3d12.dll d3d12core.dll; do
      [ -f "$SYS32/$f.bak" ] || cp -n "$SYS32/$f" "$SYS32/$f.bak" 2>/dev/null
      cp "$NEW/files/lib/wine/vkd3d-proton/x86_64-windows/$f" "$SYS32/$f"
    done
    vkd3d_build "$NEW" > "$VKFILE"
  else
    echo "warning: no installed Proton has vkd3d-proton >= $MIN_VKD3D; AMD cards may segfault on world pipelines" >&2
  fi
fi

# ---------------- vram hogs ----------------
if command -v systemctl >/dev/null && systemctl --user is-active --quiet assistant-brain.service 2>/dev/null; then
  echo "warning: assistant-brain.service holds ~11 GB VRAM" >&2
  echo "         systemctl --user stop assistant-watchdog.service assistant-brain.service" >&2
fi
pgrep -x ollama >/dev/null 2>&1 && echo "warning: ollama is running and will share VRAM" >&2
pgrep -x llama-server >/dev/null 2>&1 && echo "warning: llama-server is running and will share VRAM" >&2

# ---------------- go ----------------
[ -n "$GAME" ] || { echo "need --game DIR (or VICTORY_GAME_DIR)" >&2; exit 2; }
[ -d "$GAME" ] || { echo "not a directory: $GAME" >&2; exit 2; }
[ -n "$BIN" ] || BIN="$ROOT/target/x86_64-pc-windows-msvc/diag/victory.exe"
[ -f "$BIN" ] || { echo "no victory.exe at $BIN; build it first (see AGENTS.md)" >&2; exit 2; }

WIN_GAME="Z:${GAME//\//\\}"
WIN_BIN="Z:${BIN//\//\\}"
exec "$PROTON/files/bin/wine" "$WIN_BIN" run "$WIN_GAME\\$TITLE_EXE" --execute "${PASSTHRU[@]}"
