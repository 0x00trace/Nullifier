# Nullifier

A compatibility layer that runs an Xbox Series title natively on a Linux or Windows PC, for players who own a copy of the title and for the contributors who make it faster.

## What it is

Nullifier is a Rust program and a launcher. It is not an emulator: the title's own x86-64 code runs in Nullifier's process, Nullifier supplies the console's platform underneath it, and the title's graphics are translated to D3D12. On Linux it runs under Proton, where vkd3d-proton runs D3D12 on Vulkan. The launcher detects the card and its memory, picks measured settings from `launcher/profiles.json`, and starts the title: the player drops the game folder in the window and presses Play. Nullifier is a fork of VIctory that combines work from several of its forks.

## The problem

The title runs only on the Xbox Series console, so people who own a copy cannot play it on their PC. Several forks of VIctory run it, and each one runs best on its author's hardware, so a player has to find the fork and the settings that work on their machine.

## Who it's for

- Players who own a copy of the title. On Linux they need a Vulkan-capable AMD or NVIDIA card and Proton; on Windows 10 or 11, a D3D12 card with shader model 6.6 and mesh shaders.
- Contributors who measure on their own hardware and port ideas from other VIctory forks.

It is not for people without their own copy of the title. macOS is wanted but has no contributors yet (see Goals).

## Goals

1. The title can be played in the world on Linux and Windows, on AMD and NVIDIA cards.
2. The picture is complete, including the title's ray-traced light.
3. One `main` runs on every supported platform. `launcher/profiles.json` picks each machine's settings, and nobody edits code to get a working setup.
4. A player can start without a terminal: drop the game folder in the launcher and press Play.
5. Every performance claim is measured and recorded in `docs/FINDINGS.md` and `docs/FORK.md`.
6. macOS and Apple silicon, when contributors with that hardware join. This needs a D3D12 path on Metal or MoltenVK.

## Non-goals

- Shipping game files, or a build that contains them. Why: a legal requirement.
- Merging other forks' branches. Why: every fork rewrote the same subsystems, so a merge conflicts in about 14 files and, when it compiles, renders wrong. Ideas are ported one at a time instead.

## Success looks like

- On the reference cards (RX 9070 XT and RTX 5080), the median of world frames reaches the playable rate (see Open questions) in three `tools/bench_scene.sh` runs, with no device removed and a correct picture.
- A new player gets from a clone to the world with `tools/setup.sh` and the launcher, without editing code.

## Failure looks like

- The repository is taken down because it holds game files, the title's name, or code under an incompatible licence (XCL).
- The tree splits into branches for each vendor that only their authors can run.
- Changes that look right but remove the device or render wrong get merged.

## Priorities

1. Stability: no device removal and no crash.
2. A correct picture for what is drawn.
3. Frame rate.
4. Easy setup.
5. The complete picture, including the ray-traced light.
6. Support for more hardware, including macOS.

## Constraints

- Licence: GPL-3.0-or-later, inherited from VIctory. Upstream history and notices are kept.
- No code from XCL (`xcl-dev/xcl`), which is GPL-2.0-only. Its docs may be read for ideas.
- No game files in the repository: no assets, binaries, dumps or screenshots of the title, and not the title's name.
- Rust 1.98.1, pinned in `rust-toolchain.toml`; edition 2024; target `x86_64-pc-windows-msvc`. On Linux it is built with `cargo xwin`.
- Settings live only in `launcher/profiles.json`. A fix for one vendor is switched on at run time, not built in.
- A performance change needs a bench row: median of world frames, three runs, a picture over 50 KB, and the device removals counted (`AGENTS.md` §4). A pull request without one is closed.
- Work from another fork comes in one commit at a time, with attribution.
- The community board (`community/leaderboard.json`) holds no personal data.

## Decisions

- 2026-11-23: Do not join the title's own page remaps (`CopyPageMappingsBatchX`). Why: fxceless/victory-fork tried twice and reverted both times; faces rendered black and red.
- 2026-11-25: `main` is one tree for every platform, and `launcher/profiles.json` picks the settings, not a branch per vendor. Why: no branch should work only for its author.
- 2026-11-25: Ideas from other forks are ported one per pull request; their branches are never merged. Why: merges compile and then render wrong.
- 2026-11-25: Belobug's tree was tested as a new base and not adopted; `main` kept its base. Why: on RADV the world was black and the card took 4 to 15 times as long (`docs/FINDINGS.md` 3.10).
- 2026-11-25: The ray-traced light waits until the frame rate is usable. Why: a correct picture at 1 fps is no more useful than a dark one.
- 2026-11-26: `tools/build-wineserver.sh` builds Wine's server with Wine commit 3217449b1f71, and the launcher uses it when it is built. Why: Proton's own server walks a list of every view at each map and unmap; on an RTX 5080 the built server gave 3.45-3.55 fps against 0.60-0.70.
- 2026-11-26: macOS is wanted and waits for contributors. Why: nobody on the project works on it yet.

## Open questions

- Is running other Xbox titles a non-goal, or may the layer grow to more titles?
- What median of world frames counts as playable? Suggested: 30 fps, the rate of the menus.
- `tools/bench.sh` and `tools/bench_scene.sh` count a frame line as a world frame when it is under 10 fps, so world frames above 10 fps are left out. The launcher already uses 200 or more draws a frame instead. Which rule should the bench scripts use?

## Glossary

- **Title**: the one Xbox Series game that Nullifier runs. Its name is not written in the repository.
- **Layer**: Nullifier itself, the code between the title and the PC.
- **World frame**: a frame in the game world, not in a menu or a loading screen.
- **Bench row**: one measured result: commit, hardware, driver, settings, median of world frames with n, device removals, and whether the picture was correct (`docs/FORK.md`).
- **Profile**: an entry in `launcher/profiles.json` with the settings for one kind of machine and a `confidence`.
- **Device removed**: the D3D12 error `0x887A0005`, after which the card can no longer be used. A run with one is not a result.
- **Built wineserver**: Wine's server built by `tools/build-wineserver.sh`, which keeps the process's views in a tree.
- **VIctory forks**: VIctory (Marvisak), `fxceless/victory-fork` and `myokex/victory-amd`, which Nullifier is built on, and the other forks that ideas are ported from.

## Status

- The measured frame rates are in the README's Status table: about 5 to 7 fps in the world on Linux, and 3 to 5 reported on Windows. Runs are stable and the picture is correct apart from the missing light.
- Works: menus, cutscene, prologue, world, audio, input, saves. Missing: the ray-traced light, so shadowed areas are black. Not yet playable.
- Next: the submitting thread's processor time (`docs/FINDINGS.md` 2.10), then the copy lane. Next mark: about twice the current frame rate on Linux.
