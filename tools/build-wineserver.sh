#!/usr/bin/env bash
# build-wineserver: build the installed Proton's wineserver with Wine's view tree.
#
#   ./tools/build-wineserver.sh                    # for the Proton the launcher picks
#   ./tools/build-wineserver.sh --proton DIR       # for another Proton
#   ./tools/build-wineserver.sh --wine-commit SHA  # when the commit cannot be looked up
#   ./tools/build-wineserver.sh --check            # say what is installed, build nothing
#
# Why: the title maps its tiled heap one 64 KiB page per view, tens of
# thousands of views, and maps and unmaps about a thousand of them a second.
# Wine's server kept a process's views in a list and walked it at every map
# and unmap. On an RTX 5080 that held the server at a whole core and the
# thread that prepares the frame asleep 71 % of the time behind it
# (docs/FINDINGS.md 2.5). Wine fixed it on 2026-11-25 (commit 3217449b1f71,
# "server: Store per-process memory views in an rbtree"); Proton
# Experimental 11.0-20261112 does not have that commit.
#
# This builds the server of the Proton you have: Valve's source at the commit
# that Proton was built from, with that one commit applied
# (tools/wine/server-view-tree.patch). The result goes where the launcher
# looks, ~/.local/share/nullifier/wineserver. Nothing of Proton's is changed,
# and VICTORY_WINESERVER=0 makes the launcher leave the built server unused.
#
# Needs git, gcc, make, autoconf, bison, flex, perl and python3 (Arch:
# pacman -S --needed base-devel git), a network connection, and about 600 MB
# under ~/.cache/nullifier. Run it again after Proton updates: the launcher
# does not use a server built for another Proton version.
set -uo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
PATCH="$HERE/tools/wine/server-view-tree.patch"
STEAM_ROOT="${STEAM_ROOT:-$HOME/.local/share/Steam}"
PROTON="${VICTORY_PROTON:-}"
OUT="${VICTORY_WINESERVER_DIR:-$HOME/.local/share/nullifier/wineserver}"
WORK="${XDG_CACHE_HOME:-$HOME/.cache}/nullifier/wineserver"
REPO="https://github.com/ValveSoftware/wine.git"
COMMIT=""
CHECK=0

while [ $# -gt 0 ]; do
  case "$1" in
    --proton) PROTON="${2:-}"; shift 2 ;;
    --wine-commit) COMMIT="${2:-}"; shift 2 ;;
    --wine-repo) REPO="${2:-}"; shift 2 ;;
    --out) OUT="${2:-}"; shift 2 ;;
    --check) CHECK=1; shift ;;
    -h|--help) sed -n '2,27p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1 (try --help)" >&2; exit 2 ;;
  esac
done

say() { printf '\n== %s\n' "$1"; }
ok()  { printf '   ok: %s\n' "$1"; }
die() { printf '\n!! %s\n' "$1" >&2; exit 1; }

# The Proton the launcher picks (launcher/nullifier-launch.sh, same order).
if [ -z "$PROTON" ]; then
  for d in "$STEAM_ROOT/steamapps/common/Proton - Experimental" "$STEAM_ROOT/compatibilitytools.d"/GE-Proton*; do
    [ -x "$d/files/bin/wine" ] && { PROTON="$d"; break; }
  done
fi
THEIRS="$PROTON/files/bin/wineserver"
{ [ -n "$PROTON" ] && [ -x "$THEIRS" ]; } || die "no Proton with files/bin/wineserver found (set VICTORY_PROTON or pass --proton DIR)"
VERSION_LINE=$(cat "$PROTON/version" 2>/dev/null)
[ -n "$VERSION_LINE" ] || die "no version file in $PROTON"

# Whether the binary $1 has a function named $2. Not "grep -q" in a pipe:
# it leaves at its first match, nm then dies writing, and with pipefail the
# match counts as a failure. A stripped binary has no names: "cannot tell".
has_sym() { [ -n "$(nm "$1" 2>/dev/null | grep " $2\$")" ]; }
# Whether a wineserver binary keeps views in a tree: upstream's commit adds
# this function.
has_tree() { has_sym "$1" compare_memory_views; }

say "Proton"
ok "$PROTON"
ok "version: $VERSION_LINE"
if has_tree "$THEIRS"; then
  ok "its wineserver already keeps views in a tree: nothing to build"
  exit 0
fi
if [ -x "$OUT/bin/wineserver" ]; then
  if [ "$(sed -n 1p "$OUT/built-for" 2>/dev/null)" = "$VERSION_LINE" ]; then
    ok "a server built for this version is installed: $OUT/bin/wineserver"
  else
    echo "   the installed server was built for: $(sed -n 1p "$OUT/built-for" 2>/dev/null)"
    echo "   the launcher will not use it with this Proton"
  fi
else
  echo "   no built server in $OUT"
fi
[ "$CHECK" = 1 ] && exit 0

say "checking prerequisites"
for c in git gcc make autoreconf bison flex perl python3 curl nm strip; do
  command -v "$c" >/dev/null || die "missing '$c' (Arch: pacman -S --needed base-devel git)"
done
[ -f "$PATCH" ] || die "missing $PATCH"
ok "git, gcc, make, autoconf, bison, flex, perl, python3"

say "the Wine commit this Proton was built from"
if [ -z "$COMMIT" ]; then
  NAME=${VERSION_LINE#* }
  TAG=${NAME%-x86_64}; TAG=${TAG%-aarch64}
  # Proton's tag names the commit of its wine submodule. Only the public
  # tag name is sent.
  COMMIT=$(curl -fsSL --max-time 30 "https://api.github.com/repos/ValveSoftware/Proton/contents/wine?ref=$TAG" 2>/dev/null \
    | python3 -c 'import json, sys
d = json.load(sys.stdin)
print(d["sha"] if d.get("type") == "submodule" else "")' 2>/dev/null)
  [ -n "$COMMIT" ] || die "could not look up the Wine commit of Proton tag '$TAG' (not a build of Valve's, or no network). Pass --wine-commit SHA, and --wine-repo URL if the source is not ValveSoftware/wine."
fi
ok "$REPO at $COMMIT"

say "source"
SRC="$WORK/src"; BUILD="$WORK/build"
mkdir -p "$WORK" || die "cannot make $WORK"
[ -d "$SRC/.git" ] || git init -q "$SRC" || die "git init failed"
cd "$SRC" || exit 1
case "$(git rev-parse -q --verify HEAD 2>/dev/null)" in
  "$COMMIT"*) ok "already fetched" ;;
  *) echo "   fetching (about 400 MB)"
     git fetch -q --depth 1 "$REPO" "$COMMIT" || die "git fetch of $COMMIT from $REPO failed"
     git checkout -q -f FETCH_HEAD || die "git checkout failed" ;;
esac
# What an earlier build generated or patched goes.
{ git reset -q --hard && git clean -q -fdx; } || die "could not reset $SRC"
if grep -q 'views_tree' server/process.h; then
  ok "this Wine already has the view tree, so the Proton built from it does too: nothing to build"
  exit 0
fi
git apply "$PATCH" 2>/dev/null || patch -p1 -s --fuzz=3 < "$PATCH" \
  || die "tools/wine/server-view-tree.patch does not apply to this Wine. Upstream commit 3217449b1f71 has to be carried to it by hand."
ok "patched: $(git diff --shortstat | sed 's/^ *//')"

say "generated files (as Proton's own build makes them)"
autoreconf -f > "$WORK/autoreconf.log" 2>&1 || die "autoreconf failed (see $WORK/autoreconf.log)"
perl tools/make_requests > /dev/null || die "tools/make_requests failed"
# The server needs neither of these two; the build system reads every
# module's sources to write its Makefile, and stops at one that is missing.
if [ -f tools/make_specfiles ]; then
  perl tools/make_specfiles > /dev/null || die "tools/make_specfiles failed"
fi
if [ -f dlls/winevulkan/vk.xml ] && [ ! -f dlls/winevulkan/loader_thunks.c ]; then
  (cd dlls/winevulkan && python3 ./make_vulkan -x vk.xml -X video.xml > "$WORK/make_vulkan.log" 2>&1) \
    || die "dlls/winevulkan/make_vulkan failed (see $WORK/make_vulkan.log)"
fi
ok "protocol version $(sed -n 's/^#define SERVER_PROTOCOL_VERSION //p' include/wine/server_protocol.h)"

say "building wineserver"
rm -rf "$BUILD"; mkdir -p "$BUILD"; cd "$BUILD" || exit 1
# Only the server is built, so everything it does not use is left out.
"$SRC/configure" --enable-win64 --disable-tests --without-mingw --without-x --without-freetype \
  --without-wayland --without-gstreamer --without-alsa --without-pulse --without-cups \
  --without-opengl --without-vulkan --without-sdl > configure.log 2>&1 \
  || { tail -5 configure.log >&2; die "configure failed (see $BUILD/configure.log)"; }
make -j"$(nproc)" server/wineserver > make.log 2>&1 \
  || { tail -15 make.log >&2; die "the build failed (see $BUILD/make.log)"; }
OURS="$BUILD/server/wineserver"
[ -x "$OURS" ] || die "the build produced no wineserver"
has_tree "$OURS" || die "the built server has no view tree: the patch did not take"
ok "$(du -h "$OURS" | cut -f1) $OURS"

say "checking it against Proton's own"
# What Proton's server was built with, the built one must have too: a server
# without them answers the same protocol and then refuses what wine asks.
for sym in inproc_sync_ops fsync_init esync_init; do
  if has_sym "$THEIRS" "$sym" && ! has_sym "$OURS" "$sym"; then
    die "Proton's server has '$sym' and the built one does not (kernel headers too old for it?)"
  fi
done
ok "same synchronisation back ends"

# The protocol version a wineserver speaks: it is the first thing the server
# sends a client. Asked of each server on an empty prefix of its own.
protocol() {
  local dir pid version served
  dir=$(mktemp -d) || return 1
  # Where a server keeps the socket of a prefix: by the prefix's device and inode.
  served=$(printf '/tmp/.wine-%s/server-%x-%x' "$(id -u)" "$((0x$(stat -c %D "$dir")))" "$(stat -c %i "$dir")")
  WINEPREFIX="$dir" WINEDEBUG=-all "$1" -f > /dev/null 2>&1 &
  pid=$!
  version=$(python3 - "$served" <<'PY'
import os, socket, struct, sys, time
server = sys.argv[1]
for _ in range(100):
    if os.path.exists(server + "/socket"):
        break
    time.sleep(0.05)
os.chdir(server)
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect("socket")
data, _, _, _ = s.recvmsg(4, socket.CMSG_SPACE(4))
print(struct.unpack("I", data)[0])
PY
  )
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  rm -rf "$dir" "$served"
  echo "$version"
}
P_THEIRS=$(protocol "$THEIRS"); P_OURS=$(protocol "$OURS")
{ [ -n "$P_THEIRS" ] && [ "$P_THEIRS" = "$P_OURS" ]; } \
  || die "Proton's server speaks protocol '${P_THEIRS:-?}' and the built one '${P_OURS:-?}': not the same Wine. Nothing was installed."
ok "both speak protocol $P_OURS"

say "installing"
mkdir -p "$OUT/bin" || die "cannot make $OUT/bin"
# Function names stay (a profile can name them); debug data goes.
{ strip --strip-debug -o "$OUT/bin/wineserver.new" "$OURS" && mv -f "$OUT/bin/wineserver.new" "$OUT/bin/wineserver"; } \
  || die "could not install into $OUT/bin"
# The server looks for its data (case tables) beside itself, as in Proton.
ln -sfn "$PROTON/files/share" "$OUT/share"
printf '%s\n%s %s\n%s\n' "$VERSION_LINE" "$REPO" "$COMMIT" "upstream Wine 3217449b1f71 (tools/wine/server-view-tree.patch)" > "$OUT/built-for"
ok "$OUT/bin/wineserver"
cat <<EOF

   The launcher now starts this server for its prefix. To compare:
     ./tools/bench.sh "built wineserver"
     VICTORY_WINESERVER=0 ./tools/bench.sh "Proton's wineserver"
   Each log's "wineserver:" line says which server served the run.
EOF
