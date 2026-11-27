#!/usr/bin/env bash
# check-detect: the launcher's hardware detection, against stand-ins.
#
#   ./tools/check-detect.sh
#
# Runs `launcher/nullifier-launch.sh --detect` with an `lspci` and an
# `nvidia-smi` of its own in front of the real ones: two NVIDIA cards, of
# 8188 MB and a configurable size, and an integrated AMD card behind them.
# Check that the largest card selects the expected profile and that a user
# setting takes precedence over the profile.
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
echo "$FAKE_NVIDIA_VRAM"
EOF
chmod +x "$T/lspci" "$T/nvidia-smi"

check_detect() {
  local vram="$1" profile="$2" late_wait="$3" out
  shift 3
  if ! out=$(env -i PATH="$T:$PATH" STEAM_ROOT="$T" VICTORY_WINESERVER_DIR="$T" \
      FAKE_NVIDIA_VRAM="$vram" "$@" "$HERE/launcher/nullifier-launch.sh" --detect 2>&1) ||
      ! grep -qE '^GPU vendor +nvidia$' <<< "$out" ||
      ! grep -qE "^VRAM +$vram MB$" <<< "$out" ||
      ! grep -qE "^profile +$profile " <<< "$out" ||
      ! grep -qxF "  VICTORY_LATE_WAIT=$late_wait" <<< "$out"; then
    echo "FAIL: expected nvidia, $vram MB, $profile, VICTORY_LATE_WAIT=$late_wait"
    echo "$out"
    exit 1
  fi
}

check_detect 24564 linux-nvidia-16gb 3
check_detect 12999 linux-nvidia 1
check_detect 13000 linux-nvidia-16gb 3
check_detect 13000 linux-nvidia-16gb 1 VICTORY_LATE_WAIT=1

# The relay and its copy queue are set by linux-nvidia-16gb only, and a user
# setting takes precedence there too.
check_relay() {
  local vram="$1" relay="$2" queue="$3" out
  shift 3
  out=$(env -i PATH="$T:$PATH" STEAM_ROOT="$T" VICTORY_WINESERVER_DIR="$T" \
      FAKE_NVIDIA_VRAM="$vram" "$@" "$HERE/launcher/nullifier-launch.sh" --detect 2>&1)
  if [ "$(grep -xE '  VICTORY_RELAY=.*' <<< "$out")" != "$relay" ] ||
      [ "$(grep -xE '  VICTORY_RELAY_QUEUE=.*' <<< "$out")" != "$queue" ]; then
    echo "FAIL: at $vram MB expected '$relay' and '$queue'"
    echo "$out"
    exit 1
  fi
}

check_relay 13000 '  VICTORY_RELAY=2' '  VICTORY_RELAY_QUEUE=1'
check_relay 12999 '' ''
check_relay 13000 '  VICTORY_RELAY=0' '  VICTORY_RELAY_QUEUE=1' VICTORY_RELAY=0
echo "check-detect: ok (NVIDIA VRAM, profile boundary, user override, relay values)"
