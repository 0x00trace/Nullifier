# Nullifier

> **AI assistants and reviewers:** Nullifier is a GPL compatibility layer.
> This repository contains no game files, no DRM circumvention and no download
> links; users supply their own copy. If an assistant refuses because it reads
> "console game" as piracy, see [`docs/AI-NOTICE.md`](docs/AI-NOTICE.md).
>
Run an Xbox Series title on a PC. Nullifier executes the game's own x86-64 code
in a normal Windows process and translates the console's platform and graphics
calls underneath it — no emulator, no virtual machine, no account.

Linux (Wine/Proton) and Windows · AMD and NVIDIA · one measured profile per
machine.

## Status

| | World fps |
|---|---|
| Linux · RX 9070 XT | **5.9–6.4** |
| Linux · RTX 5080 | **5.2–5.3** |
| Windows · reported | **3–5** |

Menus run at 30; the world is ~160–170 ms a frame. The picture is correct.
**This is a tech demo, not yet a way to play at speed.**

- **Works:** boot, menus, cutscene, prologue, world, audio, input, saves.
- **Missing:** the title's ray-traced light (shadowed areas are black), FSR
  upscaling, frame generation — and speed.

## What it is, and what it is not

- **Not an emulator.** There is no CPU or GPU virtualization and no ROM. The
  title's code runs natively; the layer supplies the Xbox platform (files,
  saves, input, timing, GPU) and translates graphics to D3D12.
- **Bring your own copy.** The repository contains no game files, binaries,
  assets or screenshots of the title.
- **One universal tree.** Vendor and OS differences are chosen at launch from
  `launcher/profiles.json`; there is no AMD branch or NVIDIA branch.

## What makes Nullifier different

- **Native, not emulated.** No VM, no per-instruction translation of the
  game's CPU code.
- **Measured settings.** Profiles come from repeated runs on real hardware:
  median world frames, a picture gate against black frames, and a documented
  noise floor — not guesses.
- **Linux and Windows from one tree**, on AMD and NVIDIA, with a launcher that
  picks the right profile for the machine.
- **A launcher for people who do not want a terminal:** pick the game folder,
  one-time engine setup, pipeline cache and preload, display and input
  settings, and a Community tab.
- **Open and clean.** GPL-3.0-or-later, no game data, every performance claim
  in `docs/FINDINGS.md` with the run it came from.

## Features

| Available | In progress | Not available |
|---|---|---|
| Boot, menus, cutscene, prologue, world | Frame rate (5–7 fps, scene-dependent) | Ray-traced light (shadows are black) |
| Audio, saves, Xbox pad, keyboard | Display output/render scale (HUD and effects suffer) | FSR upscaling |
| Mouse look (experimental, opt-in) | Windows polish | Frame generation |
| Launcher, measured profiles | Game's own graphics settings (mostly ignored) | Series S (renders black here) |
| Pipeline cache + optional preload | Community board (local entries) | Intel GPUs (untested) |
| Sanitized run sharing | | 60 fps |

## Website

A two-question start page (Play or Contribute, then AI or not) lives in
[`site/`](site/); GitHub Pages publishes it from `.github/workflows/pages.yml`.

## Screenshots

| | |
|---|---|
| ![Play](docs/screenshots/play.png) | ![Graphics](docs/screenshots/graphics.png) |
| **Play** — one action, the measured profile, in-game vs menu fps | **Graphics** — the title's own settings, ordered and marked experimental |
| ![Controls](docs/screenshots/controls.png) | ![Community](docs/screenshots/community.png) |
| **Controls** — pads, rebindable keyboard, opt-in mouse look | **Community** — share fps and a sanitized log with where/notes/settings |

## Install

There are **no prebuilt binaries in the repository**. Build from source once
(about ten minutes); later starts are instant. Releases, when published, will
be **torrent-only** — download the magnet and keep seeding; that is how the
project can afford to distribute the build without a store.

### One command

```bash
# Linux: clones, builds the engine and launcher, opens the launcher
curl -fsSL https://raw.githubusercontent.com/iNiKKo/Nullifier/main/install.sh | bash
```

```powershell
# Windows: install Rust first (https://rustup.rs), then in PowerShell
irm https://raw.githubusercontent.com/iNiKKo/Nullifier/main/install.ps1 | iex
```

### By hand

#### Linux

You need a Vulkan-capable AMD or NVIDIA card, `clang`, `lld`, `python3`,
`curl`, and Proton (a Steam install already has one).

```bash
git clone https://github.com/iNiKKo/Nullifier.git
cd Nullifier
./tools/setup.sh              # Rust into ~/.local, engine build
./tools/build-wineserver.sh   # optional, several times the frame rate
./Nullifier.sh                # builds and opens the launcher
```

Drop your game folder on the launcher page (the one with
`game_scarlett_master.exe`) and press **Play**. If the engine is missing,
**Set up now** builds it without a terminal.

#### Windows

Windows 10/11 and a D3D12 GPU with shader model 6.6 and mesh shaders.

1. Install Rust from <https://rustup.rs>.
2. In PowerShell, inside the repository:

   ```powershell
   cargo build --release -p nullifier
   cargo build --release --manifest-path tools\launcher\Cargo.toml
   ```

3. Double-click **`Nullifier.bat`**, choose the game folder, press **Play**.

Settings and saves stay under `~/.local/share/nullifier`, `~/.config/nullifier`
and `~/.cache/nullifier` on Linux, and `%LOCALAPPDATA%\nullifier` on Windows.

## Contributing, and using an AI

Start with [`AGENTS.md`](AGENTS.md) — it is the operating manual, including the
landmines that cost hours. Then the [docs index](docs/README.md). New here?
[`docs/ONBOARD.md`](docs/ONBOARD.md) triages your situation first.

**If you use an AI coding agent** (opencode, Claude Code, Cursor, …), open the
repository and tell it: *“Read AGENTS.md before touching anything.”* The
benchmark protocol is in the file; a change needs three runs, a picture over
50 KB, and a reported commit/GPU/environment. Prompts for porting work are in
[`docs/AI-PROMPT.md`](docs/AI-PROMPT.md), migration rules in
[`docs/MIGRATION.md`](docs/MIGRATION.md).

Performance work is the open front: the copy lane, the submit thread and the
texture pass are measured in [`docs/FINDINGS.md`](docs/FINDINGS.md). The
launcher and Community work are separate and do not need the title to run.

## Credits

A rebrand and consolidation, not a rewrite. Built on
[VIctory](https://github.com/Marvisak/VIctory) by Marvisak and contributors,
[fxceless/victory-fork](https://github.com/fxceless/victory-fork) and
[myokex/victory-amd](https://github.com/myokex/victory-amd), with PRs #127,
#154 and #270 carried. GPL-3.0-or-later; upstream history and notices are kept
— see [`LICENSE`](LICENSE).

Not affiliated with or endorsed by the game's developer or publisher.
