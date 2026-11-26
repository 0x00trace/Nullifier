# Nullifier

Runs an Xbox Series title natively on AMD/NVIDIA, Linux and Windows. A
hardware-aware launcher picks measured settings; the title's own code runs in
our process with its graphics translated to D3D12.

**No game files.** Bring your own copy. Not affiliated with the game's
developer or publisher.

## Status

| | World fps |
|---|---|
| Linux · RX 9070 XT | **6.6–6.7** |
| Linux · RTX 5080 | **5.2–5.3** |
| Windows · reported | 3–5 |

About 150 ms a frame on the 9070 XT, stable, picture correct — measured with
the view-tree wineserver and the shipped profiles.

Works: menus, cutscene, prologue, world, audio, input, saves.
Missing: the title's ray-traced light, so shadowed areas are black. Not yet
playable.

## Get started

```bash
git clone git@github.com:iNiKKo/Nullifier.git
cd Nullifier
./tools/setup.sh --game /path/to/game
```

Linux, once: `./tools/build-wineserver.sh` — several times the frame rate.
Later: `./launcher/nullifier-launch.sh --game DIR`.
Windows: `cargo build --release -p nullifier`, then
`.\launcher\nullifier-launch.ps1 -Game DIR`.

Graphical launcher:

```bash
cargo build --manifest-path tools/launcher/Cargo.toml
./tools/launcher/target/debug/nullifier-launcher
```

## Contributing

One idea per PR with a bench row: **median of world frames, three runs, and a
picture over 50 KB**. The rules are in [`AGENTS.md`](AGENTS.md); what is slow
and why is in [`docs/FINDINGS.md`](docs/FINDINGS.md). Work from another fork
goes in one commit at a time, with attribution.

| Read | For |
|---|---|
| [`docs/ONBOARD.md`](docs/ONBOARD.md) | setup, or bringing an existing fork across |
| [`docs/PIPELINE.md`](docs/PIPELINE.md) | how a frame is assembled and what it costs |
| [`docs/LAUNCHER.md`](docs/LAUNCHER.md) | profiles, the launcher and its settings |
| [`docs/AI-PROMPT.md`](docs/AI-PROMPT.md) | paste-ready prompt for an AI contributor |

House rules: no game files; no code from XCL (GPL-2.0, incompatible); vendor
fixes gated; a PR without a bench row is closed.

## Credits

A rebrand and consolidation, not a rewrite. Built on
[VIctory](https://github.com/Marvisak/VIctory) by Marvisak and contributors,
[fxceless/victory-fork](https://github.com/fxceless/victory-fork) and
[myokex/victory-amd](https://github.com/myokex/victory-amd), with PRs #127,
#154 and #270 carried. GPL-3.0-or-later; upstream history and notices are kept —
see [`LICENSE`](LICENSE).
