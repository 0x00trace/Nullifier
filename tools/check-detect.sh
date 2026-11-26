#!/usr/bin/env bash
# check-detect: the launcher's hardware detection, against stand-ins.
#
#   ./tools/check-detect.sh
#
# Runs `launcher/nullifier-launch.sh --detect` with an `lspci` and an
# `nvidia-smi` of its own in front of the real ones: two NVIDIA cards, of
# 8188 and 24564 MB, and an integrated AMD card behind them. The launcher
# must name the vendor nvidia and the larger card's 24564 MB.
#
# Why this exists: the NVIDIA driver has no mem_info_vram_total file, so the
# launcher took the video memory from whichever other card had one (512 MB
# of an integrated card, on the machine this was found on) or reported 0,
# and a profile with vram_min_mb never matched an NVIDIA card on Linux.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
[ "$(uname -s)" = Linux ] || { echo "check-detect: Linux only"; exit 0; }
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT

cat > "$T/lspci" <<'EOF'
#!/bin/sh
echo '01:00.0 VGA compatible controller [0300]: NVIDIA Corporation GB203 [GeForce RTX 5080] [10de:2c02] (rev a1)'
echo '79:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. [AMD/ATI] Granite Ridge [Radeon 610M] [1002:13c0] (rev c2)'
EOF
cat > "$T/nvidia-smi" <<'EOF'
#!/bin/sh
echo 8188
echo 24564
EOF
chmod +x "$T/lspci" "$T/nvidia-smi"

OUT="$(PATH="$T:$PATH" "$HERE/launcher/nullifier-launch.sh" --detect 2>&1)"
fail=0
echo "$OUT" | grep -qE '^GPU vendor +nvidia$' || { echo "FAIL: vendor is not nvidia"; fail=1; }
echo "$OUT" | grep -qE '^VRAM +24564 MB$' || { echo "FAIL: video memory is not the larger NVIDIA card's 24564 MB"; fail=1; }
if [ "$fail" != 0 ]; then
  echo "--- the launcher said:"
  echo "$OUT" | sed -n '1,6p'
  exit 1
fi
echo "check-detect: ok (nvidia, 24564 MB)"
