# victory-universal

**Runs an Xbox Series title natively on your PC — AMD or NVIDIA, Linux or Windows.**

Not an emulator: the title's x86-64 code runs in our process, and this project
supplies the console platform underneath it — the runtime services, the files,
the input, the audio, and the GPU (its RDNA programs are recompiled to DXIL and
run on your card's D3D12 driver).

**This repository contains no game files.** You need your own copy of the title.
Not affiliated with or endorsed by its developer or publisher.

> **Status: the world renders, but it is not playable yet.** It boots, plays the
> opening cutscene with audio, and enters the prologue with characters,
> environment, HUD and working input. Frame rate is the open problem (see below).

---

## Status (2026-11-25)

| Stage | State |
|---|---|
| Boot, first-boot screens, account picker | works, 30 fps |
| Saves and settings | work; survive a kill |
| Opening cutscene | plays start to finish with audio and subtitles |
| Prologue | renders characters, buildings, sky, HUD, pad/keyboard input |
| Frame rate in the world | **0.70–0.90 fps** on RX 9070 XT + Mesa 26.3.1 (Linux); 2–4 fps reported on Windows |
| Light in shadow | missing — the title's ray-traced bounce/sun/lamp shadows do not run, so anything not in direct sun is black |
| Stability | 6+ minutes in the world, no device loss, no crash (Linux, `LATE_WAIT=1`) |

### Why it is slow, honestly

The GPU is **4–6 % busy** and the process uses **~0.7 of one core out of 12**.
It is not compute-bound: it is serialisation-bound under Wine, where victory's
memory tracking (per-frame dirty-page queries) and copy/fence coordination cost
far more wall time than on Windows. Same build, same class of card, Windows:
2–4 fps. The roadmap in [`docs/ROADMAP.md`](docs/ROADMAP.md) names the fix.

---

## Platform support

| Platform | State | Evidence |
|---|---|---|
| **Linux + AMD** | works, 0.70–0.90 fps | measured here (RX 9070 XT, Mesa 26.3.1, Wine 11) |
| **Linux + NVIDIA** | works, 1.85–1.97 fps | reported (RTX 5070 Ti) |
| **Windows + AMD** | works, ~1 fps | reported (RX 9070 XT; needs the pipeline skips noted in `AGENTS.md`) |
| **Windows + NVIDIA** | works, 2–4 fps | reported (RTX 4070 / 5070 Ti) |
| Linux + Intel | untested | profile exists, marked `untested` |
| macOS / Apple silicon | not supported yet | needs a Metal path; see [`docs/ROADMAP.md`](docs/ROADMAP.md) |

Requirements: x86-64 CPU with AVX2, a D3D12 GPU with **shader model 6.6 and mesh
shaders** (RTX 20 series+, RX 6000 series+), ~24–28 GB RAM in play, and a lot of
patience. Linux needs Proton and a vkd3d-proton recent enough — the launcher
handles that.

---

## Quick start

### Linux

```bash
git clone git@github.com:iNiKKo/victory-universal.git
cd victory-universal

# one-time: a Rust toolchain that can build the Windows target
# (rustup with the pinned toolchain, plus cargo-xwin, clang and lld)
cargo xwin build --profile diag -p victory --features diagnostics \
  --target x86_64-pc-windows-msvc

./launcher/victory-launch.sh --detect            # what it found, and why
./launcher/victory-launch.sh --game /path/to/game
```

### Windows

```powershell
git clone https://github.com/iNiKKo/victory-universal.git
cd victory-universal
cargo build --release -p victory

.\launcher\victory-launch.ps1 -Detect
.\launcher\victory-launch.ps1 -Game "D:\path\to\game"
```

The launcher starts the title; there is no separate install step.

---

## The launcher

One matrix, two launchers, no per-platform branches. `launcher/profiles.json`
is the only place settings live; the bash and PowerShell launchers both read it.

```
detect OS -> detect vendor + VRAM -> pick the first matching profile
          -> apply --safe / --diagnostics -> your exported vars win
          -> Linux: prefix + vkd3d-proton >= 5742 -> run the title
```

```bash
./launcher/victory-launch.sh --list       # profiles, confidence and why
./launcher/victory-launch.sh --detect     # this machine's profile and values
./launcher/victory-launch.sh --game DIR   # play
./launcher/victory-launch.sh --game DIR --safe   # if a run dies: device removed
```

| Profile | Confidence | Key settings |
|---|---|---|
| `linux-amd-16gb` | measured | `TEXTURE_NO_WAIT=1 LATE_WAIT=1 WIDE_BACK=1 CARD_BUFFERS_MB=1024` |
| `linux-amd-8gb` | extrapolated | same, `CARD_BUFFERS_MB=512` |
| `linux-nvidia` | reported | same + `VKD3D_DISABLE_EXTENSIONS=VK_NV_raw_access_chains` |
| `linux-other` | untested | conservative card buffers |
| `windows-amd` | reported | `LATE_WAIT=3 WIDE_BACK=1 CARD_BUFFERS_MB=1024` |
| `windows-nvidia` | reported | `RELAY=2` + late-wait |
| `windows-other` | untested | conservative |

Every profile states where its numbers came from. `untested` means exactly
that. See [`docs/LAUNCHER.md`](docs/LAUNCHER.md) for how to add your hardware.

The launcher also creates the Proton prefix, installs a vkd3d-proton new enough
for RADV (older ones segfault on world mesh pipelines), and warns you if a local
LLM is holding 11 GB of your VRAM.

---

## Contributing

This fork exists so the work stops being scattered across vendor-specific
branches. One `main`, contributions as PRs, one idea each.

| Read | For |
|---|---|
| [`AGENTS.md`](AGENTS.md) | the rules, the merge protocol, every landmine |
| [`docs/MIGRATION.md`](docs/MIGRATION.md) | moving your own fork's work here, PR format |
| [`docs/AI-PROMPT.md`](docs/AI-PROMPT.md) | paste-ready prompt for your AI assistant |
| [`docs/FORK.md`](docs/FORK.md) | measured results and the platform table |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | what to work on, in order |
| [`docs/LAUNCHER.md`](docs/LAUNCHER.md) | detection, profiles, troubleshooting |

House rules, short version: **no game files**; **no code from XCL** (GPL-2.0,
incompatible with this GPL-3.0 tree); **one idea per PR**; **a bench row or it
does not get reviewed**; medians of world frames, never peaks.

Measure with `./tools/bench.sh "<label>"`.

---

## Credits

This is a consolidation, not a from-scratch implementation:

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its
  contributors — the loader, runtime services, the D3D12 back end and the
  shader recompiler.
- **[victory-fork](https://github.com/fxceless/victory-fork)** by fxceless —
  the fork this one follows, plus the Wine fixes carried here.
- **[victory-amd](https://github.com/myokex/victory-amd)** by myokex — the
  AMD-specific fixes in this tree.
- **Carried PRs:** #127 (Wine-compatible internal HLSL), #270 (texture refresh
  without the remap lock), #154 (VEH-guarded in-process copy).

Licensed **GPL-3.0-or-later**; see [`LICENSE`](LICENSE). The upstream READMEs
are kept, unchanged, in [`docs/README.upstream.md`](docs/README.upstream.md).
