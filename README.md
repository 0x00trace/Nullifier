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
./tools/setup.sh
cargo build --manifest-path tools/launcher/Cargo.toml
./tools/launcher/target/debug/nullifier-launcher
```

In the launcher: drop your game folder on the window and press **Play**. The
first run offers one **Set up now** button if the engine still needs building.
The command line below does the same without the window.

Linux, once: `./tools/build-wineserver.sh` — several times the frame rate (the
launcher offers this as one button too). Windows: `cargo build --release -p
nullifier`, then `.\launcher\nullifier-launch.ps1 -Game DIR`.

No window: `./tools/setup.sh --game /path/to/game`, then
`./launcher/nullifier-launch.sh --game DIR`.

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
