# AGENTS.md — read this before you touch anything

You are working on **a fork of VIctory**, a compatibility layer that runs an
Xbox Series title natively on a PC. It is not an emulator: the title's x86-64
code runs in our process and we supply the console's platform underneath it.
This file is the operating manual for AI agents (and humans) contributing to
this fork.

Read all of it. Most of the traps below cost hours to find, and several of them
produce symptoms that look like *unrelated* bugs.

New here, or moving work over from another fork? Do
[`docs/ONBOARD.md`](docs/ONBOARD.md) first — it triages which situation you are
in before you touch anything.

---

## 0. Absolute rules

1. **No game files, ever.** No assets, no binaries from the title, no dumps, no
   screenshots of the title in the repo. The title's name is not written in the
   tree; keep it that way. This is a licence/legal requirement, not a style
   preference.
2. **Licence.** This fork is GPL-3.0-or-later, inherited from VIctory.
   * **XCL (xcl-dev/xcl) is GPL-2.0-only and is INCOMPATIBLE.** You may read
     its docs for ideas; you must not copy or port its code. XCL's own spec
     says the same in reverse. If you are about to copy a line from XCL, stop.
   * Upstream VIctory and its forks are GPL-3.0 — merging those is fine.
3. **Never merge blind.** See §5. Two branches here independently implement
   the same features; a naive merge compiles and then renders wrong or crashes.
4. **Measurement or it did not happen.** Every performance claim needs
   `tools/bench.sh` output (median of world frames), not a peak, not a
   screenshot, and not "feels faster". See §4.

---

## 1. What this fork is

`main` is the **universal stack**: it boots on AMD and NVIDIA, Linux and
Windows. The per-platform differences are chosen at launch from
`launcher/profiles.json` — not by keeping a branch per vendor. The tree is
`myokex/victory-amd` (AMD fixes on top of `fxceless/victory-fork`) plus three
PRs required under Wine, and it is the fastest configuration measured on AMD:

| Applied | Why it is required |
|---|---|
| PR #127 | Wine's `d3dcompiler_47` refuses `RWByteAddressBuffer` methods. Without this the indirect-argument clamp fails to compile, **every indirect draw is dropped**, and the world/cutscene renders as a black screen with working UI and audio. |
| PR #270 | Texture refresh waited on the title-memory remap lock: ~688 ms/frame. Now ~24 ms. |
| PR #154 | Title-memory copies went through `ReadProcessMemory` (~360 MB/s). Now an in-process `memcpy` with a VEH fallback (~1.1 GB/s). |

`reference/upstream-2026-11-25` is read-only reference: `fxceless/victory-fork`
at `a26859f` + PR #127, no AMD fork. It is newer (texture path: 548 MB in
169 ms) but slower overall on AMD (0.4–0.55 vs 0.7–0.9 fps). **We do not merge
it.** If a commit there is worth having, port that one idea as a PR
(`docs/MIGRATION.md`) and prove it with a bench row.

```
main                            the universal fork (this is what users run)
reference/upstream-2026-11-25   read-only; cherry-pick ideas, never merge
pr127 pr154 pr270 ...           the individual PRs, for rebasing
```

Why not one merged branch: merging upstream into this tree conflicts in 14
files (`texture.rs`, `watch.rs`, `host.rs`, `resource.rs`, `xmem.rs`,
`lower.rs`, …) because both sides independently implemented view guards, page
marking and texture refresh. A mechanical merge compiles and then renders
wrong. Port concepts, not branches.

### Launcher and profiles

Settings live in exactly one place: `launcher/profiles.json`. Both
`launcher/victory-launch.sh` (Linux/macOS) and `launcher/victory-launch.ps1`
(Windows) read it. Never hard-code a switch in code to work around a platform;
add a profile entry with an honest `confidence` instead. See
`docs/LAUNCHER.md`.

---

## 2. Build

Windows target from Linux (this is how it is built here):

```bash
export RUSTUP_HOME=~/.local/victory-toolchain/rustup
export CARGO_HOME=~/.local/victory-toolchain/cargo
export PATH="$CARGO_HOME/bin:$PATH"

cargo xwin build --profile diag -p victory --features diagnostics \
  --target x86_64-pc-windows-msvc
```

* `diag` + `diagnostics` is required for `VICTORY_KEYS`, `VICTORY_SKIP_PIPELINES`,
  `VICTORY_CAPTURE` and the frame-time accounting quoted everywhere here.
* Plain `--release` works but has none of those, and logs at info only.
* Toolchain is pinned by `rust-toolchain.toml`; do not bump it casually.

On Windows, `cargo build --release -p victory` with MSVC.

---

## 3. Run

```bash
./launcher/victory-launch.sh --detect          # hardware, profile, and why
./launcher/victory-launch.sh --game <dir>      # play
./launcher/victory-launch.sh --game <dir> --safe   # if the device is removed
```

```powershell
.\launcher\victory-launch.ps1 -Detect
.\launcher\victory-launch.ps1 -Game "D:\games\title"
```

The launcher detects GPU vendor and VRAM, picks settings, checks that
vkd3d-proton is new enough, warns about VRAM hogs, and starts the title under
the right Proton. Everything it sets can be overridden by exporting it first.

### Settings matrix (measured, see docs/FORK.md)

| Platform | Settings |
|---|---|
| Linux + AMD | `TEXTURE_NO_WAIT=1 LATE_WAIT=1 WIDE_BACK=1 CARD_BUFFERS_MIN_MB=1 CARD_BUFFERS_MB=1024` |
| Linux + NVIDIA | same + `VKD3D_DISABLE_EXTENSIONS=VK_NV_raw_access_chains` |
| Windows + AMD | `LATE_WAIT=3 WIDE_BACK=1 CARD_BUFFERS_MIN_MB=1 CARD_BUFFERS_MB=1024`; add `SHADER_OPT=59` if the driver compiler crashes |
| Windows + NVIDIA | `RELAY=2`, or the `LATE_WAIT=3` combo |
| VRAM < 10 GB | `CARD_BUFFERS_MB=512` (or 409); do not exceed 1024 on 12 GB — it corrupts the picture |

---

## 4. Measure

```bash
./tools/bench.sh "label"                  # uses the env you exported
VICTORY_LATE_WAIT=1 ./tools/bench.sh "late-wait 1"
```

Protocol, because the frame rate is scene-dependent and deceiving:

* Report the **median of world frames** (`fps < 10`), with n. Menus are 30 fps
  and loading dips to 5–20 fps; including them flatters or wrecks a run.
* A run needs **n ≥ 20** world frames to mean anything.
* Peaks (5–8 fps) are camera cuts, not settings. Never quote them alone.
* Always report `device removed` count. A fast run that removes the device is
  not a result.
* Two runs of the same build differ by ±30 % median. Do not chase 10 %.

---

## 5. Merge protocol

Before merging anything into `main`:

1. Build it (§2).
2. Run `tools/bench.sh` twice: once default, once with the platform's settings.
3. Confirm **all** of: no `device removed`, no `unhandled exception`, no
   `no argument clamp`, world renders (characters, buildings, HUD), and the
   median is not worse than the previous `main`.
4. If it touches shaders or the GPU crate, also run the crate's tests
   (`cargo test -p victory-shader -p victory-gpu` — GPU tests need a card).
5. Record the number and the config in `docs/FORK.md`'s results table.

Do not merge a PR that only "looks right". Three of the four bugs in §6 were
introduced by plausible-looking changes.

---

## 6. Landmines (symptom → cause → fix)

**Black screen, UI and audio fine.** Wine's `d3dcompiler_47` rejects
`InterlockedMax`/`Load`/`Store` on `RWByteAddressBuffer`; the indirect clamp
does not build; every indirect draw is dropped. → PR #127 (in `main`). If you
add HLSL to `victory-gpu`, use structured-buffer indexing, not
`ByteAddressBuffer` methods. Check: `grep 'no argument clamp' <log>` must be 0.

**Segfault in `vkCreateGraphicsPipelines` / `libvulkan_radeon.so`, exit 3.**
RADV crashes compiling certain world mesh pipelines when driven by
vkd3d-proton older than build 5742. → use the vkd3d-proton from Proton
Experimental (the launcher copies it in). The same shader compiles fine
standalone and fine on lavapipe: it is a driver crash, not our translation.

**`device removed 0x887A0005` after minutes.** Submissions sent ahead of the
fence outrun the driver. → `LATE_WAIT>=2`, `JOIN_COPIES=1` and `RELAY` are not
safe on Linux. Default `LATE_WAIT=1`.

**`unhandled exception 0x80000001` (guard page).** On current upstream, PR #154's
VEH copy collides with upstream's own page-guard machinery. → do not apply #154
on `upstream-2026-11-25`; the AMD branch has no such guards and needs it.

**Nothing renders, `drawn=0` with millions of draws.** Mesh shaders unsupported
or a software adapter. Check `host mesh shader support mesh=true`.

**Out of VRAM.** A local LLM on `-ngl 99` will take 11+ GB and push the title
into GTT, which costs ~10× frame time. Stop it before benchmarking.

---

## 7. Where the time goes today (do not re-derive this)

At ~0.7 fps on an RX 9070 XT / Mesa 26.3.1:

* GPU busy **4–6 %**, ~0.7 of one core of 12 used, 103 threads with **one**
  busy. The title is *not* CPU-bound or GPU-bound; it is serialisation-bound.
* The submitting thread spends ~2 s per frame between submissions running only
  ~20 ms of user CPU — it is blocked in memory tracking / copies / fences.
* Copies: fast in throughput (1961 MB in 7.2 ms) but slow in wall time
  (hundreds of ms), i.e. coordination, not bandwidth.
* `GetWriteWatch` under Wine is the reason texture refresh was the top cost
  (now mitigated by #270, still visible).
* Linux ceiling observed across reports: 0.16 fps (3090), 1.85–1.97 (5070 Ti),
  0.5–1.4 here. Windows on the same build: 2–4 fps. The gap is Wine, not the
  hardware.

The next real win is replacing the per-frame `GetWriteWatch` with cheaper dirty
tracking, or amortising it. That is in `victory-gpu/src/watch.rs` and the
texture lane.

---

## 8. Reporting

When you post results, include: commit, GPU, driver, Mesa/Proton versions,
exact env, median + n, removals, and whether the picture was correct. Put the
same row in `docs/FORK.md`. Numbers without a commit are not reproducible.
