# Nullifier

Runs an Xbox Series title natively on AMD/NVIDIA, Linux and Windows. A
hardware-aware launcher picks measured settings; the title's own code runs in
our process with its graphics translated to D3D12.

**No game files.** Bring your own copy. Not affiliated with the game's
developer or publisher.

## Status

| | World fps |
|---|---|
| Linux · RX 9070 XT | **5.5–6.6** |
| Linux · RTX 5080 | **5.2–5.3** |
| Windows · reported | 3–5 |

About 170–180 ms a frame on the 9070 XT in the scene after the opening save
(150 ms at the opening), stable, picture correct — measured with the view-tree
wineserver and the shipped profiles.

Works: menus, cutscene, prologue, world, audio, input, saves.
Missing: the title's ray-traced light, so shadowed areas are black. Not yet
playable.

## Install

No game files are included; bring your own copy. There is no prebuilt release
yet, so the steps below build it once (about ten minutes, then starts are
instant).

### Linux

You need a Vulkan-capable AMD or NVIDIA card, `clang`, `lld`, `python3`,
`curl`, and Proton (a Steam install already has one; the launcher finds it).

```bash
git clone https://github.com/iNiKKo/Nullifier.git
cd Nullifier
./tools/setup.sh              # installs Rust in ~/.local, builds the engine
./tools/build-wineserver.sh   # optional, several times the frame rate
./Nullifier.sh                # builds and opens the launcher
```

Then, in the window:

1. Drop your game folder on the page — the folder that contains
   `game_scarlett_master.exe`.
2. Press **Play**. If something is still missing, **Set up now** builds it
   without a terminal.

### Windows

You need Windows 10/11 and a D3D12 card with shader model 6.6 and mesh shaders.

1. Install Rust from <https://rustup.rs> (the defaults are fine).
2. In PowerShell, inside the repository:

   ```powershell
   cargo build --release -p nullifier
   cargo build --release --manifest-path tools\launcher\Cargo.toml
   ```

3. Double-click **`Nullifier.bat`** (it builds the launcher once if needed),
   drop the game folder, press **Play**.

### Command line

```bash
./tools/setup.sh --game /path/to/game
./launcher/nullifier-launch.sh --game DIR
```

Windows: `.\launcher\nullifier-launch.ps1 -Game DIR`.

Your settings, saves and game data stay under `~/.local/share/nullifier`,
`~/.config/nullifier` and `~/.cache/nullifier` on Linux, and under
`%LOCALAPPDATA%\nullifier` on Windows. Deleting the repository does not delete
them.

## Contributing

One idea per PR with a bench row: **median of world frames, three runs, and a
picture over 50 KB**. The rules are in [`AGENTS.md`](AGENTS.md); what is slow
and why is in [`docs/FINDINGS.md`](docs/FINDINGS.md). Work from another fork
goes in one commit at a time, with attribution.

All docs are in the [docs index](docs/README.md). Start with
[`AGENTS.md`](AGENTS.md) for the rules and [`docs/FINDINGS.md`](docs/FINDINGS.md)
for what is slow and why.

House rules: no game files; no code from XCL (GPL-2.0, incompatible); vendor
fixes gated; a PR without a bench row is closed.

## Credits

A rebrand and consolidation, not a rewrite. Built on
[VIctory](https://github.com/Marvisak/VIctory) by Marvisak and contributors,
[fxceless/victory-fork](https://github.com/fxceless/victory-fork) and
[myokex/victory-amd](https://github.com/myokex/victory-amd), with PRs #127,
#154 and #270 carried. GPL-3.0-or-later; upstream history and notices are kept —
see [`LICENSE`](LICENSE).
