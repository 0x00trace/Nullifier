# The launcher

One matrix, two launchers. `launcher/profiles.json` is the only place settings
live; `launcher/victory-launch.sh` (Linux/macOS) and
`launcher/victory-launch.ps1` (Windows) both read it. If you find yourself
typing `VICTORY_*` into a shell to play, either the profile is wrong and should
be fixed, or you are testing and should use `tools/bench.sh`.

```bash
./launcher/victory-launch.sh --list       # what profiles exist
./launcher/victory-launch.sh --detect     # what this machine gets, and why
./launcher/victory-launch.sh --game DIR   # play
```

```powershell
.\launcher\victory-launch.ps1 -List
.\launcher\victory-launch.ps1 -Detect
.\launcher\victory-launch.ps1 -Game "D:\games\title"
```

## What it detects

| | Linux / macOS | Windows |
|---|---|---|
| OS | `uname -s` | always `windows` |
| Vendor | `lspci` VGA/3D line | `Win32_VideoController` name, discrete adapter preferred |
| VRAM | `/sys/class/drm/*/mem_info_vram_total` | `HardwareInformation.qwMemorySize` (AdapterRAM caps at 4 GB) |

macOS reports `vendor=apple` and exits with "not supported yet" — see
`docs/ROADMAP.md`. Intel is matched as `intel` but only by the `*-other`
profiles until someone measures it.

## Selection rules

1. `--profile NAME` (or `-Profile`) wins.
2. Otherwise the **first** profile in `profiles.json` whose `match` applies:
   `os` and `vendor` may be `"*"`, plus optional `vram_min_mb` / `vram_max_mb`.
3. `--safe` and `--diagnostics` are applied on top (see `overrides`).
4. **Anything already exported in the environment wins over the profile**, so
   you can A/B a single switch without editing the file.

`--detect` prints the profile, every variable it set, the confidence level and
the note explaining *why* those numbers.

## Adding or changing a profile

A profile is only allowed in if it has evidence:

```json
{
  "name": "linux-nvidia-16gb",
  "match": { "os": "linux", "vendor": "nvidia", "vram_min_mb": 13000 },
  "confidence": "measured",
  "sets": { "VICTORY_TEXTURE_NO_WAIT": "1", "VICTORY_LATE_WAIT": "1" },
  "note": "RTX 4080, Mesa x.y, median 1.4 fps over 3 runs"
}
```

* `confidence` is one of `measured` (you ran `tools/bench.sh` here),
  `reported` (from another person's bench row), `extrapolated` (scaled from a
  measured one) or `untested`.
* Keep the array ordered **most specific first**; selection is first-match.
* Put the measurement in `docs/FORK.md` too. A profile without a row is a
  guess and must say `untested`.

## Safety rules the launcher enforces

* `LATE_WAIT >= 2`, `JOIN_COPIES=1` and `RELAY` are never defaulted on Linux:
  they remove the device (`0x887A0005`) within minutes. `--safe` turns all
  send-ahead off if a run still dies.
* vkd3d-proton below build **5742** segfaults RADV on the first world mesh
  pipeline. The launcher installs the newest one it can find (Proton
  Experimental) into the prefix once, keeps `.bak` copies, and records the
  build in `.vkd3d-build`.
* The prefix is created through Proton itself. A bare `wineboot` produces a
  prefix without `d3dcompiler_47.dll`/`wined3d.dll` and the title will not load.
* If a local LLM (`ollama`, `llama-server`, `assistant-brain`) is running, it
  warns: `-ngl 99` models take 11+ GB of a 16 GB card and push the title into
  GTT, costing roughly 10× frame time.

## Troubleshooting

| Symptom | Do |
|---|---|
| `device removed 0x887A0005` | `--safe` |
| Black screen, UI and audio fine | the log has `no argument clamp`; the build is missing PR #127 |
| Exit 3 right after a mesh pipeline | vkd3d-proton too old; check `--detect` shows `prefix vkd3d >= 5742` |
| `no profile applies` | add/repair a profile, or pass `--profile` |
| Runs but very slow | check for an LLM, then `--detect` and compare VRAM |
