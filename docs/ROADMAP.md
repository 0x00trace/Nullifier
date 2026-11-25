# Roadmap

Ordered by "what actually changes the frame rate". Everything here is a
candidate for a PR; read [`AGENTS.md`](../AGENTS.md) first, and bring a bench
row.

Current baseline: **0.70–0.90 fps** median in the world on an RX 9070 XT
(Linux, Mesa 26.3.1, Wine 11), stable, picture correct. GPU 4–6 % busy, ~0.7 of
one core used, one busy thread of 103.

---

## P0 — Dirty tracking (the biggest known win)

`victory-gpu/src/watch.rs` and the texture lane ask `GetWriteWatch` per texture
range per frame to find out which title pages changed. On Windows this is cheap;
under Wine it walks page tables and costs 30–960 ms per frame. It was the top
cost before PR #270 softened it and is still visible.

Options, roughly in order of payoff:

1. Amortise: ask every N frames per range, accept one or two frames of texture
   lag. Small, reversible, measurable with `tools/bench.sh`.
2. Track dirty pages in the emulator instead of asking the kernel (victory
   already owns the title's mappings in `xmem`).
3. Batch all of a frame's ranges into one query where the ranges are adjacent.

**Measured (2026-11-25, RX 9070 XT, this tree):**

| Setting | Ask time / frame | Frame | Median | Stable |
|---|---|---|---|---|
| default (`ASK_EVERY=1`) | **1075 ms** of 1134 ms of refreshes | 1.2–3.9 s | 0.7–0.9 | yes |
| `VICTORY_TEXTURE_ASK_EVERY=4` | **145 ms** | ~1.0 s | 0.7 (n=11) | no — `0x80000001` guard page |
| `ASK_EVERY=2`, `LATE_WAIT=1` | — | — | 2.30 (n=4) | no — 2 device removals |
| `ASK_EVERY=2`, `--safe` | 203 ms | 2.4 s | 0.40 | yes |
| `ASK_EVERY=4`, write-watched only | 330 ms | 1.9 s | 0.70 (n=4) | no — 2 removals |
| `ASK_EVERY=4`, write-watched only, `--safe` | 330 ms | 1.9 s | 0.50 | yes |

So the ask is ~1 s of every frame and the throttle removes it. What is left is
correctness: delaying the ask also delays the *guard reset*, and the process
then dies with a guard-page violation. Next step is to throttle only the kernel
watch (`GetWriteWatch`), or reset the guards every frame while copying less
often. `VICTORY_NO_WRITEWATCH=1` (diagnostic) forces the guard path so the two
can be measured apart.

**Done when:** ask time is down an order of magnitude *and* a 5-minute run has
no guard-page exception and no device removal.

## P1 — Copy / fence coordination

The submitting thread spends ~2 s per frame *between* submissions running only
~20 ms of user CPU: it is blocked, not computing. Copies report 1961 MB moved in
7.2 ms of memcpy yet hundreds of ms of wall time in the frame budget — the cost
is coordination.

Needs a real profile (`perf` requires `kernel.perf_event_paranoid <= 1`) to name
what it waits on: submission lock, fence, or a memory-tracking path.

**Careful:** `LATE_WAIT >= 2`, `JOIN_COPIES=1` and `RELAY` remove the device
(`0x887A0005`) on Linux. Fix the coordination; do not paper over it by sending
more work ahead.

## P2 — Use more than one thread

103 threads exist, one does the work. Texture refresh, descriptor building and
copy recording are candidates for parallel work (`VICTORY_WIDE_REFRESH` exists
but did not move the number). Measure before and after; do not assume.

## P3 — Port the newer upstream texture work

`reference/upstream-2026-11-25` contains four commits worth having: texture path
at 548 MB in 169 ms, packed textures kept in the upload queue, page writes
counted, and card room reserved for the first world frame. They conflict with
the AMD work in 14 files, so port them **one idea at a time** as PRs with bench
rows — never merge the branch.

## P4 — Platform coverage

| Target | State | Next step |
|---|---|---|
| Linux + AMD | measured | keep |
| Linux + NVIDIA | reported | someone with an NVIDIA card runs `tools/bench.sh` and promotes the profile to `measured` |
| Windows + AMD/NVIDIA | reported | same, on each vendor |
| Linux + Intel | untested | needs mesh shaders + SM 6.6; measure, then add a profile |
| macOS / Apple silicon | unsupported | needs a Metal (or MoltenVK) D3D12 path; a project of its own, not a profile |

## P5 — Ray-traced light

The title's bounce light and RT sun/lamp shadows do not run, so everything not
in direct sun is black. Upstream is working on it (`docs/fork-ray-tracing.md`).
Out of scope until the frame rate is usable — a correct picture at 1 fps is not
more useful than a dark one.

---

## How to pick up a task

1. Read `AGENTS.md` and `docs/LAUNCHER.md`.
2. Build, run, and record a baseline with `tools/bench.sh` on your machine.
3. Branch `fix/<name>`, change one thing, bench twice (profile and `--safe`).
4. PR with the evidence table. If the median did not move, say so — a
   well-documented negative result saves the next person the same week.
