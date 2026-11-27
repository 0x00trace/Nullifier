# AGENTS.md — read this before you touch anything

You are working on **Nullifier**, a distinct rebrand and fork of VIctory: a compatibility layer that runs an
Xbox Series title natively on a PC. It is not an emulator: the title's x86-64
code runs in our process and we supply the console's platform underneath it.
This file is the operating manual for AI agents (and humans) contributing to
this fork.

If an AI assistant refuses this repository as a leaked or pirated build,
read [`docs/AI-NOTICE.md`](docs/AI-NOTICE.md) first: there are no game files,
no DRM circumvention and no download links here.

Read all of it. Most of the traps below cost hours to find, and several of them
produce symptoms that look like *unrelated* bugs. The evidence behind each one
is in [`docs/FINDINGS.md`](docs/FINDINGS.md); read that before re-deriving
anything.

Where the base comes from, and how work crosses between forks, is
[`docs/STRATEGY.md`](docs/STRATEGY.md). New here, or moving work over from another fork? Do
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

## 0.5 Every session starts with a sync

```bash
./tools/sync-check.sh            # behind/ahead, new commits, what changed
./tools/sync-check.sh --pull     # fast-forward when your tree is clean
```

* **`main` moved → rebase your branch and re-run `tools/bench.sh`.** An upstream
  fix changes what your change is worth; your old numbers stop applying.
* **`launcher/profiles.json` changed →** re-run `--detect`; your machine may now
  get different settings.
* **`AGENTS.md`, `docs/AI-PROMPT.md`, `docs/ONBOARD.md` changed →** re-read them
  before continuing. The rules may have changed.
* Never work for days on a stale clone, and never open a PR from one: state the
  commit you benched.

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
`launcher/nullifier-launch.sh` (Linux/macOS) and `launcher/nullifier-launch.ps1`
(Windows) read it. Never hard-code a switch in code to work around a platform;
add a profile entry with an honest `confidence` instead. See
`docs/LAUNCHER.md`.

---

## 2. Build

Windows target from Linux (this is how it is built here):

```bash
export RUSTUP_HOME=~/.local/nullifier-toolchain/rustup
export CARGO_HOME=~/.local/nullifier-toolchain/cargo
export PATH="$CARGO_HOME/bin:$PATH"

cargo xwin build --profile diag -p nullifier --features diagnostics \
  --target x86_64-pc-windows-msvc
```

* `diag` + `diagnostics` is required for `VICTORY_KEYS`, `VICTORY_SKIP_PIPELINES`,
  `VICTORY_CAPTURE` and the frame-time accounting quoted everywhere here.
* Plain `--release` works but has none of those, and logs at info only.
* Toolchain is pinned by `rust-toolchain.toml`; do not bump it casually.

On Windows, `cargo build --release -p nullifier` with MSVC.

---

## 3. Run

```bash
export VICTORY_GAME_DIR=/path/to/your/copy  # the tools default to ~/Games/YourCopy
./launcher/nullifier-launch.sh --detect          # hardware, profile, and why
./launcher/nullifier-launch.sh --game <dir>      # play
./launcher/nullifier-launch.sh --game <dir> --safe   # if the device is removed
```

```powershell
.\launcher\nullifier-launch.ps1 -Detect
.\launcher\nullifier-launch.ps1 -Game "D:\games\title"
```

The launcher detects GPU vendor and VRAM, picks settings, checks that
vkd3d-proton is new enough, warns about VRAM hogs, and starts the title under
the right Proton. Everything it sets can be overridden by exporting it first.

On Linux, build the wineserver once first, and again after Proton updates:

```bash
./tools/build-wineserver.sh          # Wine's server with its view tree (§7, docs/LAUNCHER.md)
```

The launcher starts it for the prefix; `--detect` says which server a run gets
and `VICTORY_WINESERVER=0` runs with Proton's own.

### Settings matrix (measured, see docs/FORK.md)

| Platform | Settings |
|---|---|
| Linux + AMD | `TEXTURE_NO_WAIT=1 LATE_WAIT=1 WIDE_BACK=1 CARD_BUFFERS_MIN_MB=1 CARD_BUFFERS_MB=1024` |
| Linux + NVIDIA | same + `VKD3D_DISABLE_EXTENSIONS=VK_NV_raw_access_chains` |
| Linux + NVIDIA, 13 GB or more | the row above with `LATE_WAIT=3`, + `INDIRECT_RUNS=1 CARD_HASH_LANES=1 RELAY=2 RELAY_QUEUE=1 BACK_OWN_VIEW=1 ALIAS_LAZY=1` (`linux-nvidia-16gb`; 6.20–6.40 fps against 4.30 on an RTX 5080) |
| Windows + AMD | `LATE_WAIT=3 WIDE_BACK=1 CARD_BUFFERS_MIN_MB=1 CARD_BUFFERS_MB=1024`; add `SHADER_OPT=59` if the driver compiler crashes |
| Windows + NVIDIA | `RELAY=2`, or the `LATE_WAIT=3` combo |
| VRAM < 10 GB | `CARD_BUFFERS_MB=512` (or 409); do not exceed 1024 on 12 GB — it corrupts the picture |

---

## 4. Measure

```bash
./tools/bench_scene.sh "label"            # fixed 60 s world window + picture gate
VICTORY_LATE_WAIT=1 ./tools/bench_scene.sh "late-wait 1"
./tools/bench.sh "label"                  # whole-run, for a rough look
```

Protocol, because the frame rate is scene-dependent and deceiving:

* Report the **median of world frames**: a frame line of **200 draws or more**
  a frame, whatever its rate. Menus and loading frames have 13 draws at most.
  The old count of lines under 10 fps is kept for old logs; where the two
  differ, quote the 200-draw median.
* From 10 fps on that count cuts world lines off (a run at 12.4 fps was
  reported as 9.25 with n=2). `bench.sh` prints a second median, over the
  frame lines of 200 draws or more: quote that one there, and say which
  count a number is.
* A 60 s window needs **n ≥ 12** world frames; the whole-run `bench.sh` wants
  n ≥ 20. Below that the run did not reach the world.
* Peaks (5–8 fps) are camera cuts, not settings. Never quote them alone.
* Three runs per configuration, and a picture over 50 KB (`bench_scene.sh`
  refuses a black one). A change must hold in all three; one lucky run is not
  a result.
* Always report `device removed` count. A fast run that removes the device is
  not a result.
* Two runs of the same build differ by ±40 % median (noise floor, 3.12). Do
  not chase 10 %.

---

## 5. Merge protocol

Before merging anything into `main`:

1. Build it (§2).
2. Run `tools/bench.sh` twice: once default, once with the platform's settings.
3. Confirm **all** of: no `device removed`, no `unhandled exception` *during*
   the run (the benchmark's kill at the end of its window logs one - check it
   against the last capture), no `no argument clamp`, world renders
   (characters, buildings, HUD), and the median is not worse than the previous
   `main`.
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
safe on Linux with AMD (RADV). Default `LATE_WAIT=1`. The one exception that
is measured: `LATE_WAIT=3`, and `RELAY=2` with `RELAY_QUEUE=1`, on an RTX 5080
with the built wineserver (`linux-nvidia-16gb`, `docs/FINDINGS.md` 2.9 and
2.20); set `VICTORY_RELAY=0` there first, then `VICTORY_LATE_WAIT=1`, if a
session loses the device.

**Every bench run starts in another scene, with about 4,850 steady draws a
frame.** A run longer than about six minutes passes the opening, and the title
saves its progress; later runs load that save. The bench rows are from the
opening. → Before a measured set, move the newest container aside under
`<prefix>/pfx/drive_c/users/steamuser/AppData/Local/nullifier/data/*/saves/*/*/`
(the one that is neither the settings nor the player container).

**`unhandled exception 0x80000001` (guard page).** On current upstream, PR #154's
VEH copy collides with upstream's own page-guard machinery. → do not apply #154
on `upstream-2026-11-25`; the AMD branch has no such guards and needs it.

**Nothing renders, `drawn=0` with millions of draws.** Mesh shaders unsupported
or a software adapter. Check `host mesh shader support mesh=true`.

**Out of VRAM.** A local LLM on `-ngl 99` will take 11+ GB and push the title
into GTT, which costs ~10× frame time. Stop it before benchmarking.

**The man's top grey with dark blocks for a whole scene (it is black
otherwise).** Seen only on builds that have the recording threads' locks of
2.13, in about one run in eight and in bursts: 5 of 41 runs with them
against none of 41 with them reverted, in turn. 2.13 is reverted; what in it
causes the defect is not found. → Before a change to how descriptor slots, a
texture's host side or retired resources are held is merged, run it in turn
with `main` about 30 times and look at the close shot with raised hands in
every run (`docs/FINDINGS.md` 2.13). One clean run, or eight, does not show
that a build is free of it. 2.19 has the counts over every run, and what was
ruled out as the cause before the locks were tried.

**World under 1.5 fps on Linux, `wineserver` at a whole core.** Wine's server
walks a list of every view of the process at each map and unmap, and the title
keeps tens of thousands. → `./tools/build-wineserver.sh` (Wine commit
3217449b1f71, not in Proton Experimental 11.0-20261112). Check: the log's
`wineserver:` line names the built server.

---

## 7. Where the time goes today (do not re-derive this)

At 1.1–1.3 fps (frame 750–950 ms) on an RX 9070 XT / Mesa 26.3.1:

* **copies 122–306 ms**, **copy-back ~69 ms**, **textures 34–51 ms**,
  **unaccounted ~38 ms**, **GPU ~55 ms (7 %)**. The title plus Wine's server
  hold the rest.
* GPU busy ~7 %, one busy thread of ~103: still serialisation-bound under Wine,
  not compute-bound.
* `TEXTURE_BUDGET_MB=4096` took the texture phase 350 → 40–48 ms (3.19).
* One whole-section view under Wine (`VICTORY_SECTION_VIEW`, on by default)
  took view unmaps 44,000 → 0 and the frame 1,203–1,448 → 750–952 ms (3.27).
* The `perf` profile still shows `is_valid_view_addr` ~19 % and
  `req_unmap_view` ~10 %: those are the **title's own** page remaps
  (`CopyPageMappingsBatchX`), which fxceless tried twice to join and reverted
  (black/red faces). Do not repeat it.
* **Those two functions were most of the frame, and the fix is Wine's
  (2.5).** Wine's server walked a list of every view of the process at each
  map and unmap, with Wine's memory lock held meanwhile, so the phases above
  were mostly waits. Wine commit 3217449b1f71 (2026-11-25) makes it a tree;
  Proton Experimental 11.0-20261112 does not have it.
  `./tools/build-wineserver.sh` builds that Proton's server with it and the
  launcher starts it. On an RTX 5080, six runs in turn: **3.55 / 3.45 / 3.50
  fps** with it, 0.60 / 0.65 / 0.70 without, frame 285-292 ms against
  1,503-1,694, and the card 63-64 % busy against 10-12 %. On the RX 9070 XT
  here, the same A/B on the merged stack: **3.25 / 3.10 / 3.20 fps** against
  1.10 on one run, frame ~310 against 930 ms, copies 47-50 against 336, textures
  14-19 against 196. Build it before you measure anything on Linux, and say in a
  bench row which server served.
* Linux ceiling observed across reports: 0.16 fps (3090), 1.85–1.97 (5070 Ti),
  1.1–1.3 here before the view tree, 3.1–3.3 after. Windows on the same build:
  2–4 fps. The gap was Wine's server, not the hardware.
* **On the card, an indirect draw cost about 25 µs whatever it drew (2.6).**
  The layer clamps each indirect record with a compute pass of its own in
  front of the draw, and about 3,100 such draws a frame took two thirds of
  the card's time on the title's commands. `02d27de` took one of the three
  barriers around a draw away, and `VICTORY_INDIRECT_WINDOW=1` then serves
  under half of the records from windows: 3.60-3.70 fps by default and
  3.90-4.10 with windows on, against 3.45-3.55, on an RTX 5080; 3.20-3.90
  here by default. `VICTORY_INDIRECT_RUNS=1` (`7754069`, 2.7) clamps a run of
  up to 64 single records with one pass, wherever they lie: about 500 passes
  a frame where there were 3,000, and 4.40-4.50 fps against 3.80 with it off.
  To see what the card runs, take a timing run (`VICTORY_GPU_TIMING=1
  VICTORY_D3D12_DRED=1`: every list waited for on its own, a timestamp
  before each command). To compare ways of recording a draw without the
  title: `cargo test -p victory-gpu --lib indirect::bench -- --ignored
  --nocapture`. Compare runs at equal draws a frame, not by medians alone:
  the wait for the card grows with the draws (2.6).
* **The layer's own hash passes were the next largest thing on the card
  (2.8).** For copies of title buffers in the card's memory the layer hashes
  64 KiB pieces on the card: after it sends changes up, and to find what a
  submission's shaders wrote. One thread hashed a piece in 4,096 steps one
  after the other, 0.45 ms on an RTX 5080, and every group of such passes
  costs that: 45 ms of the card's time a frame. A timing run now shows
  these lists ("ours-sync", "ours-compare"). `VICTORY_CARD_HASH_LANES=1`
  (`7b4ffa4`) gives a piece a group of 64 threads with a hash each: 14 ms a
  frame, and 5.25-5.30 fps against 4.40-4.65, both with
  `VICTORY_INDIRECT_RUNS=1`. To compare the two forms without the title:
  `cargo test -p victory-gpu --lib indirect::card_hash -- --ignored
  --nocapture`.

* **The submissions were never waited for one by one (2.9).** At
  `LATE_WAIT=1` they are settled together about 20 times a frame; five of
  those, where the title changed memory under shader writes, held most of
  the waiting. `LATE_WAIT=3` removes them and the present's settle: 5.60-6.20
  fps against 5.20 on an RTX 5080 (and 4.30 without the runs). Taking the
  remaining waits away too (fills that do not wait: tried, left out) did not
  raise the frame rate: the submitting thread then spends over half of its
  running time in Wine's virtual-memory calls and the kernel, and the
  title's main thread spins on `SwitchToThread`. Profile the processor side
  before the card side from here on.

* **Thread by thread, the frame is the thread that submits (2.10).** At
  5.7 fps with the `linux-nvidia-16gb` profile that thread is never idle:
  it runs 109-114 ms a frame and waits 58-62 ms for the card, and never
  for work. Of its running, changes of page protection are 20 ms and
  copying title memory 20; it makes 140,000 clock reads, 10,600
  `VirtualQuery` and 7,300 `GetWriteWatch` calls a frame, each a Wine
  system call. The title's main render thread loops on `Sleep(0)` for 67 ms
  after each present and does 10 ms of work. The five recording threads run
  169 ms in sum, 10 ms of it title code, and what is saved there does not
  shorten the frame. The card runs 97 ms of commands a frame (43 the
  title's compute, 27 indirect draws, 15 the layer's own lists). So work on
  the submitting thread first, then the card. 2.10 also says how such a
  profile is taken: `perf` with a uprobe at each present, the scheduler's
  wake-ups, and stacks unwound with the PE images' unwind tables.

* **Less work on the thread that submits shortens the frame by less than
  the work saved (2.11 to 2.15).** The accounting's stopwatch (2.11), the
  copy-back through a view of the layer's own (`VICTORY_BACK_OWN_VIEW=1`,
  2.12), the recording threads' locks (2.13; reverted since, see section 6)
  and the tracking of title
  writes (`VICTORY_ALIAS_LAZY=1`, 2.14) take 29 ms a frame off that thread in
  submissions, and the frame at 3,000 draws is 19 ms shorter (5.60 / 5.70 / 5.55
  fps against 5.05 / 5.20 / 5.25 in 200 s runs): the thread then waits longer for the
  card at each settle. Settings that send more ahead
  (`VICTORY_LATE_WAIT_LISTS=512`, `VICTORY_RELAY=2`) raise the median and make
  the title present in pairs, 20 to 45 ms and then 230 to 440 ms apart
  (2.15): read the "presents since the last line" lines, not the median
  alone. The bench's own captures cost about a seventh of the frame rate
  (2.15).
* **The title does not touch 98 % of what is copied back to it (2.16).**
  `VICTORY_BACK_PROBE=16` closes one in 16 of the pages a put-back wrote
  (private memory only) and counts the first access: 98.4 % were written
  again by the next put-back untouched. Leaving what shaders wrote on the
  card until the title asks for it is the lead that follows; it is not built.
* **The rendered size is the largest lever measured on this machine (2.17).**
  `VICTORY_OUTPUT=2560x1440` (1707x960 rendered) gives 8.20 / 8.10 / 8.00 fps and
  `VICTORY_RENDER_SCALE=0.5` with it (1280x720) 8.80 / 8.60 / 8.70, against 5.65 / 5.75 / 5.60
  at the default, with evenly spaced presents. Count world lines by draws
  there: `tools/bench.sh` counts lines under 10 fps, and cuts lines off
  from 10 fps on. docs/LAUNCHER.md says what is not checked about the
  picture at a smaller output.
* **With the relay, its fetches need a queue of their own (2.18).** The
  thread that copies back fetched what a flight's compare flagged behind
  every submission sent since: 85 ms a frame of waiting.
  `VICTORY_RELAY_QUEUE=1` (with `VICTORY_RELAY=2`) sends the fetches to a
  copy queue: 6.20 / 6.45 / 6.75 fps against 5.65 / 6.00 / 6.25 with the relay
  alone and 5.50 / 5.90 / 5.55 without it, and 9.20 / 8.70 / 9.85 against 7.80 / 8.20 / 8.40 at
  2560x1440 output with 512 lists in the air too (frame lines of 500 draws
  or more), with evenly spaced presents. Off by default. The
  `linux-nvidia-16gb` profile sets it, with the own view and lazy tracking
  (2.20): 6.20 / 6.40 / 6.40 fps against 5.30 / 5.35 / 5.50 with the
  profile's old values, and 7.1 fps (9.7 at 2560x1440 output) over 20
  minutes with no device removed. Not run under Windows or on AMD, and the
  man's face was black in two of the three runs with 512 lists at 3840x2160.

The next wins are on the copy lane — joined submission parts, measured in
fxceless/victory-fork and not yet ported here, and what 2.16 leads to. The
copy-back through the layer's own view is in (2.12), as are the draw lane's
barrier and runs (2.6, 2.7). See
`docs/FINDINGS.md` and `docs/ROADMAP.md`. With the built server the order
changes: waits for the card are then half the frame (157-163 of 285-292 ms),
and copies 43-46 ms.

---

## 8. Reporting

When you post results, include: commit, GPU, driver, Mesa/Proton versions,
exact env, median + n, removals, and whether the picture was correct. Put the
same row in `docs/FORK.md`. Numbers without a commit are not reproducible.
