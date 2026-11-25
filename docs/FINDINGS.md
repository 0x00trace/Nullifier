# Findings

What has been established, with evidence. Read this before re-deriving
anything: several of these cost hours, and two of them look like unrelated bugs
from the symptom.

Numbers are from an RX 9070 XT (RDNA4, 16 GB), Mesa 26.3.1, Wine 11
(GE-Proton11-10 with Proton Experimental's vkd3d-proton), 30 GB RAM, Series X
profile, 2560×1440, on 2026-11-25. Method: `tools/bench.sh`, median of world
frames (`fps < 10`).

---

## 1. Symptom → cause

| Symptom | Cause | Evidence | Fix |
|---|---|---|---|
| Black screen, UI text and audio fine | Wine's `d3dcompiler_47` rejects `InterlockedMax`/`Load`/`Store` on `RWByteAddressBuffer`; the indirect-argument clamp never compiles; **every indirect draw is dropped** | `clamp:76:10: E5005: Method 'InterlockedMax' is not defined` then `no argument clamp: indirect execution dropped`; draws/frame 312 → 2,435 after the fix | PR #127 |
| Exit 3 right after the first world mesh pipeline | RADV segfaults in `vkCreateGraphicsPipelines`; the same shader compiles standalone and on lavapipe | `err:vulkan:vkCreateGraphicsPipelines Exception 0xc0000005`, backtrace all in `libvulkan_radeon.so`, symbolized to `radv_graphics_pipeline_compile` | vkd3d-proton ≥ 5742 (Proton Experimental) |
| `device removed 0x887A0005` after minutes | submissions sent ahead of the fence | `device removed after 31 submissions sent ahead (VICTORY_LATE_WAIT=1)` | `LATE_WAIT ≤ 1` on Linux; `--safe` if needed |
| `unhandled exception 0x80000001` | guard-page violation; seen when delaying guard resets, and when PR #154's VEH copy meets newer upstream page guards | `unhandled exception 0x80000001 at victory.exe+…` | not on `reference/upstream`, no #154 there; see §3 |
| ~30 fps menus, 0.2–1 fps world | per-frame texture bookkeeping and copies, not GPU work | GPU 4–6 % busy; one busy thread of 103 | §2 |

## 2. Where the frame time goes

At a 1.2 s frame:

```
preparing        ~1.2 s   of which textures ~1.1 s
  of that, asking the write watch / guards:  1075 ms
copies           0.2-1.8 s (varies; 1961 MB moves in 7 ms of memcpy)
GPU              60-240 ms
copying back     40-530 ms
```

Two facts that keep getting rediscovered:

* **The GPU is not the problem.** It is 4–6 % busy. ~0.7 of one core of 12 is
  used, and the submitting thread spends ~2 s per frame *between* submissions
  running ~20 ms of user CPU: it is blocked, not computing.
* **Bandwidth is not the problem.** Victory's own accounting shows 1961 MB
  copied in 7.2 ms when measured directly, against hundreds of ms of wall time
  in the frame budget. The cost is coordination and per-object overhead.

## 3. The ask throttle (P0 probe)

~10,000 textures are asked every frame whether they changed. At ~0.1 ms per ask
under Wine that is ~1.1 s.

| Setting | Ask time/frame | Frame | Median | Stable |
|---|---|---|---|---|
| default (`ASK_EVERY=1`) | **1075 ms** | 1.2–3.9 s | 0.70–0.90 | yes |
| `VICTORY_TEXTURE_ASK_EVERY=4` | **145 ms** | ~1.0 s | 0.70 (n=11) | **no** — `0x80000001` guard page |
| `ASK_EVERY=2` + `LATE_WAIT=1` | — | — | **2.30** (n=4) | **no** — 2 device removals |
| `ASK_EVERY=2` + `--safe` | 203 ms | 2.4 s | 0.40 | yes |
| `ASK_EVERY=4`, write-watched only | 330 ms | 1.9 s | 0.70 (n=4) | no — 2 removals |
| `ASK_EVERY=4`, write-watched only, `--safe` | 330 ms | 1.9 s | 0.50 | yes |

The throttle works (ask up to 7× cheaper). Two things were learned refining it:

1. Throttling *all* textures delays the **guard reset** and the process dies with
   `0x80000001`. Guarded (view) memory must be polled every frame.
2. Throttling only textures whose memory is write-watched (`watched == Some(true)`)
   removes the crash and still cuts the ask 1075 → 330 ms, because only the
   successful `GetWriteWatch` calls walk page tables; guarded ones fail fast.

Defaulting the throttle on under Wine was tried (every=2): ask 1075 → 861 ms,
median 0.55 (n=4), two device removals — no gain, so it stays opt-in.

What is left is not the ask but stability of the fast path: with `LATE_WAIT=1`
the runs still end in device removals, and `--safe` (no send-ahead) is stable
but pays it back in copy time. The next target is therefore the copy/fence
coordination, not the ask.

`VICTORY_NO_WRITEWATCH=1` (diagnostic) forces the guard path so the two
mechanisms can be measured apart.

## 3.5 The real cost is page faults and per-object calls, not I/O

Measured on the live title for 20 s in the world (`tools/profile.sh`):

| | |
|---|---|
| minor page faults | **424,119 in 20 s ≈ 21,000/s** |
| major page faults | 305 (≈15/s) |
| disk read during the window | effectively zero |
| GPU | 4–6 % busy |
| CPU | ~0.7 of one core |

So the "slow asset loading" is not the NVMe and not RAM bandwidth: assets are
already resident, and the disk is idle while the frame is slow. The cost is
**per-operation overhead inside the emulation layer**:

* ~21,000 page faults a second — every guarded write traps into Wine's
  exception dispatcher, which costs far more than the write;
* ~10,000 texture bookkeeping calls a frame, each ~0.1 ms under Wine;
* 6–16 GB a frame of texture memory re-detiled, because a texture with any
  changed page is copied whole.

That is why the GPU idles: the title is not waiting for data, it is waiting for
the emulator to finish bookkeeping. `VICTORY_VIEW_GUARD=0` did not help
(0.5 fps, then a crash), so the faults are not only the view guards.

## 3.6 Our card-memory passes (a second ~1 s/frame)

`heap buffers N passes T ms` is not the descriptor heap: it is **our own passes
over copies in the card's memory** (hash passes after ranges go up, compare
passes after the lists). The code says why it is expensive: each pass walks its
64 KiB pieces 16 bytes at a time, 4,096 steps, so a pass costs about one piece
however few pieces it has — and one run sent up **514 ranges a frame = 514
passes**. Measured here: **90–1,703 ms per frame**.

Two switches target it:

| Config | Heap pass | Median | Stable |
|---|---|---|---|
| default | 90–1,703 ms | 0.70–0.90 | yes |
| `VICTORY_CARD_REHASH_RUNS=1 VICTORY_WIDE_MERGE=1` | up to 1,703 ms, then **0.0 ms** in later frames | 0.50 | **yes** (0 crashes, 0 removals, 5 min) |

The passes did collapse to zero in the later frames, but the median did not
improve, so the frame is now dominated by something else in that configuration
(copies). Worth re-measuring on a machine where copies are cheaper.

## 3.7 What upstream did about all of this (2026-11-25, `9376fd6` + `f85101b`)

Two commits landed upstream after our base, self-described as "frame rate,
processor side" and "frame rate, second night". On an RTX 4070 they measure
**3.5–4 fps before, 6–7 with all switches, ~4.5–5 by default**. What they do,
and how it maps onto our numbers:

* **Descriptor sync**: BLAKE3 over 250,000–300,000 records cost 35–42 ms a
  frame; a multiply-and-fold fingerprint replaces it and the pass went
  **70–220 ms → ~3 ms**. (Not present in our tree: our descriptor code predates
  this pass, so we have no equivalent cost to remove.)
* **`VICTORY_ASYNC_SUBMIT=1`**: submissions prepared and sent by a thread of
  the layer instead of the title's render thread — our "submitting thread
  blocked 2 s/frame".
* **`VICTORY_QUEUE2=1`**: fetches go to a copy queue — our copy/fence
  coordination.
* **Staging buffers and up to 512 idle host lists reused**; upload fills and
  read-only copies no longer wait for flights.
* **`VICTORY_PROFILE=1`**: a sampling profiler inside the layer (2,961 lines),
  which is what we lacked because `kernel.perf_event_paranoid=2` blocks `perf`.

### Tested, and it does not help us (2026-11-25)

Built `f85101b` as our tree would need it: upstream + **PR #174** (compile the
internal HLSL with DXC when Wine's `d3dcompiler_47` refuses it) instead of
rewriting 11 `ByteAddressBuffer` sites by hand. DXC 1.9.2609 x64, `VICTORY_DXC`
set. The conflict is one file (`indirect.rs`): keep upstream's
`make_from(device, source)` and compile through `host::compile`, which falls
back to DXC.

| Config | Median | n | Crashes/removals | Frame |
|---|---|---|---|---|
| `f85101b` + #174, defaults | 0.60 | 45 | 0 / 0 | 823 ms best |
| same + `ASYNC_SUBMIT=1 QUEUE2=1` | **0.50** | 40 | 0 / 0 | GPU 1,304 ms, copy-back 1,305 ms |
| our `main` | **0.70–0.90** | 20–43 | 0 / 0 | — |

The clamp failures are gone under DXC (0 occurrences), so the DXC path works —
but the new upstream switches are **slower on RADV/Wine here**, not faster: the
copy queue serialises with a 1.3 s GPU time. Upstream's 6–7 fps on an RTX 4070
does not transfer to this platform.

**Conclusion: do not take `9376fd6`/`f85101b` on Linux/AMD yet.** Re-test on
Windows and on NVIDIA before revisiting; the DXC fallback (#174) is still worth
having as a safety net, because it removes a hard dependency on the Wine
compiler accepting our HLSL.

## 3.8 Overnight sweep: 42 runs, 21 configurations, two passes

`tools/profile.sh`-style runs of 4 minutes each, driven into the world, recording
median, 10th/90th percentile, n, coefficient of variation, crashes and device
removals. Configurations that died before 15 world frames are discarded.

Kept only rows with n >= 15 and no crashes/removals, and only configurations
measured twice:

| Configuration | Median | p10 | CV | Verdict |
|---|---|---|---|---|
| `CARD_BUFFERS_MB=512` | 0.60–0.70 | **0.30–0.40** | **0.79–0.92** | **shipped** — same speed, 3-4x better 10th percentile |
| `JOIN_COPIES=1` | 0.68 | 0.15 | 0.89 | no steadiness gain, no speed gain |
| `CARD_BUFFERS_MB=2048` | 0.50 | 0.15 | 1.31 | worse |
| `LATE_WAIT=3` | 0.65 | 0.40 (one pass) | 0.94 | contradicts earlier removals; not trusted |
| `REHASH_RUNS+WIDE_MERGE+ASK4` | 0.65 | 0.30 | 1.30 | no median gain |

Rejected as invalid (n = 3–6 or 2 removals): `default` (cold cache),
`rehash`, `rehash+wide`, `wide_merge`, `ask4`, `viewguard0`, `card0`,
`widerefresh`, `pipewait24`. Their apparent 2.9–4.3 fps "medians" were three
lucky frames each.

**Conclusion.** Small card copies (512 MB) reduce stutter substantially without
losing speed, so `launcher/profiles.json` now ships 512 MB for AMD instead of
1024. No configuration in the sweep beat the baseline median by more than noise;
the frame is still bounded by the costs in 2 and 3.

## 3.9 With the shipped profile (512 MB card buffers), where the frame goes

Averaged over every world frame of the two sweep runs for `card512`:

| Phase | Run 1 (1,822 ms) | Run 2 (2,640 ms) |
|---|---|---|
| **copies** | **659 ms** | **840 ms** |
| textures | 360 ms | 660 ms |
| unexplained (waits/pacing) | 414 ms | 561 ms |
| copying back | 97 ms | 232 ms |
| GPU | 106 ms | 112 ms |

So the ordering of targets is now: **copies → textures → the ~0.5 s of
unexplained stall → copy-back**, with the GPU a rounding error. This is the same
conclusion the earlier profiling reached from a different direction: bytes move
fast (1,961 MB in 7 ms of memcpy) but the copy phase costs most of a second,
which is coordination, not bandwidth. The unexplained block is where the submit
thread is parked; that is the one that needs a stack sample, not a switch.

## 3.10 The Belobug rebase, tested (2026-11-25)

Ported the DXC fallback (#174) onto Belobug's tree (`01aadd1`) — `dxc.rs`, the
`Win32_Graphics_Direct3D_Dxc` feature, and both clamp compile sites. It builds
and the clamp no longer fails under Wine:

| Build | clamp failures | Picture | Median | GPU/frame |
|---|---|---|---|---|
| our `main` | 0 | correct | 0.70-0.90 | ~110 ms |
| Belobug + DXC | **0** | **black** | 0.40 | **1,660 ms** |
| Belobug + DXC, `AMBIENT=off HOIST_CLAMPS=0` | 0 | black | 0.55 | 821 ms |

So the DXC port did its job — indirect draws are no longer dropped — but the
tree renders **black on screen** and is **4-15x heavier on the GPU** on
RADV than on Windows. Their additions help on Windows (3-5 fps, lit) and do not
transfer here.

**Conclusion: the rebase is a regression on Linux.** `main` stays on the AMD
tree. The work is parked as a local branch (`mix-belobug`, commits `a16a1a5`,
`2bfa15c`) for whoever wants to take it to Windows or NVIDIA.

## 3.11 The Wine build itself (2026-11-25)

We had always used GE-Proton11-10's wine and only swapped its vkd3d-proton.
Tested Proton Experimental's wine (a full `wineserver -k` first, or it fails
with `version mismatch 938/931`):

| Wine | Median | n | Frame (median) | copies | textures | GPU | copy-back | unacc |
|---|---|---|---|---|---|---|---|---|
| GE-Proton11-10 | 0.50–0.70 | 74 | 1,965 ms | 695 | 328 | 62 | 141 | 44 |
| **Proton Experimental** | 0.70 | 34 | **1,493 ms** | 468 | 270 | 109 | 63 | 33 |

A second run on Proton Experimental reproduced it (frame 1,441 ms, copies 418,
textures 309) and a **third did not** (frame 2,072 ms, copies 401, textures 383).
So the wine difference is **inside the +/-40 % run-to-run noise** and must not be
quoted as a 25 % win. The launcher still prefers Proton Experimental because a
newer ntdll is the better default, but the claim is unproven until the same
build is measured several times each way on one scene.

Current baseline on the shipped configuration (one run, for comparison only):
frame 2,072 ms, copies 401, textures 383, copy-back 103, GPU 78, unaccounted 43,
33 world frames, no crash and no device removal.

Both stable, no removals. The newer wine is **~25 % cheaper per frame** and
shifts nothing else, but the median did not move because the scene mix differs
between runs. Worth a second run before changing the launcher's default; if it
holds, `VICTORY_PROTON` should prefer Proton Experimental.

Also ruled out: memory pressure (`si/so` 0/0, zram flat) and THP (already
`always`).

## 3.12 The noise floor, measured (2026-11-25)

Two runs of the **same build, same profile, same warm cache**, measured with
`tools/bench_scene.sh` (world entry at fps<2, 20 s warm-up discarded, then a
fixed 60 s window):

| Run | Median | p10 | Frame | copies | textures | GPU | copy-back | unacc |
|---|---|---|---|---|---|---|---|---|
| A | 0.50 | 0.20 | 1,976 ms | 533 | 371 | 69 | 93 | 35 |
| B | 0.70 | 0.10 | 1,464 ms | **239** | 351 | 53 | 91 | **199** |

Same run: median differs 40 %, **copies differ 2.2x**, and the unaccounted slice
moves by 164 ms. So the fixed window helps but does not make a single run
trustworthy: the camera and the streaming differ run to run, and the phases
trade time with each other.

**Rule from this:** no change is accepted on fewer than **three runs per
configuration**, and the phases are only comparable when copies *and* the
unaccounted slice both move in the same direction. A single-run "win" of less
than 40 % means nothing. A truly deterministic harness needs a fixed save plus
an exact input replay, which is the next infrastructure item.

## 3.13 Guards are not the second (correction)

`VICTORY_VIEW_GUARD=0 VICTORY_TEXTURE_GUARD=0` was expected to test the fault
theory: ~21,000 guarded writes a second at Wine's exception cost should be ~1 s
of every second, so removing the guards should have been a large win.

| config | frame | fps |
|---|---|---|
| guards on (3.12 baseline A/B) | 1,464-1,976 ms | 0.50-0.70 |
| **both guards off** | **1,358 ms** | **0.70** |

No material change, well inside the noise floor. So the "960 ms asking the
write watch and the guards" line does **not** translate into 960 ms of
reclaimable time: the cost is in the calls themselves (as copies 27 % /
textures 19 % of the frame say), not in the traps. The fault count is a
symptom of how many objects are touched, not the price of each.

This also means the next change must reduce the **number of calls or their
per-call cost**, not the fault mechanism.

## 3.14 Series S mode: measured faster, then found BLACK (retracted)

**Correction.** The 0.80-0.90 fps rows below were taken without a picture check.
Capturing frames with `VICTORY_CAPTURE` under `--series-s` produced **1,586-byte
PNGs** — a solid colour — against 1.7-4.2 MB for a real frame. So Series S
renders black on this build and the "win" was less work, not a faster renderer.
**Do not use `--series-s`.** Series X remains the only profile with a correct
picture. The numbers below are kept to show how the trap looked.

### The retracted measurement

`--console series-s` (the title's own lower-resolution profile) measured with
`bench_scene.sh`, same window and warm-up, two runs:

| config | median | p10 | frame | copies | textures | copy-back | GPU |
|---|---|---|---|---|---|---|---|
| Series X A | 0.50 | 0.20 | 1,976 ms | 533 | 371 | 93 | 69 |
| Series X B | 0.70 | 0.10 | 1,464 ms | 239 | 351 | 91 | 53 |
| **Series S A** | **0.90** | 0.20 | **1,157 ms** | 236 | **174** | **29** | 97 |
| **Series S B** | **0.80** | **0.50** | **1,204 ms** | 276 | **162** | **31** | 109 |

Consistent across both runs and outside the +/-40 % noise floor: frame time
~1,180 ms vs ~1,720, **textures roughly halved** (162-174 vs 351-371) and
**copy-back cut to a third** (29-31 vs 91-93) because the title renders fewer,
smaller targets. GPU rises slightly (97-109 vs 53-69) and stops mattering.

Trade-off: lower internal resolution and geometry — a quality setting, not a
free win. Shipped as `--series-s` (or `VICTORY_CONSOLE=series-s`), **not** as a
default.

## 3.15 `VICTORY_UNWATCHED_FRAMES` (heap rescan interval) - no effect here

VItoriaBr's newest work reduces heap-buffer CPU overhead with a 64-frame rescan
and cached spans; our tree already has the interval switch
(`VICTORY_UNWATCHED_FRAMES`, `draw.rs:1430`, default 1, with a comment that 120
once produced stale streamed geometry and crashed runs).

| config | median | p10 | frame | copies | textures | heap pass |
|---|---|---|---|---|---|---|
| default (every frame) | 0.50-0.70 | 0.10-0.20 | 1,464-1,976 | 239-533 | 351-371 | 90-1,703 ms |
| `UNWATCHED_FRAMES=4` | 0.70 | 0.10 | 1,446 | 409 | 156 | 254-469 ms |

Inside the noise floor either way, and the heap pass is still 0.25-0.5 s: most
of its shadows are write-watched, so the interval only touches a minority. The
win in VItoriaBr must be in the *other* half - cached spans and fewer COM checks
- not in the interval.

## 3.16 Wider hash loads in the card compare - tried, reverted

Port idea from VItoriaBr #33 (64 threads per 64 KiB piece). First step tried
here was smaller: keep one thread per piece but issue four 16-byte loads at a
time (64 bytes an iteration) so the fold is identical and only the load
latency overlaps.

Build fine, but the run produced **2 device removals and 4 world frames**, with
`heap buffers` at 150-767 ms. That is a failed run, not a measurement, so the
change was reverted and nothing was merged. Next attempt should take the whole
of #33 (64 threads per piece, lane fold, dispatch widened) rather than half of
it - the parallelism has to come from the group, not from unrolling inside one
thread that is already the only thing walking the piece.

## 3.17 The #33 port: 64 threads per piece (branch `hash64`, not merged)

Port of VItoriaBr #33: the card compare now runs **one group per 64 KiB piece,
64 threads on it**, lane `t` taking the 16-byte steps `t, t+64, t+128...` and
lane 0 folding the 64 lane hashes in lane order. Dispatch widened from
`pieces.div_ceil(64)` to `pieces` groups.

| run | settings | median | n | frame | heap pass | removals | picture |
|---|---|---|---|---|---|---|---|
| before | default | 0.50-0.70 | 15-35 | 1,464-1,976 | 250-470 ms | 0 | ok |
| **hash64** | default | **1.10** | 16 | **926 ms** | **120-263 ms** | 0 | **4.2 MB** |
| hash64 | default | 1.65 | 4 | 967 ms | - | **2** | ok |
| hash64 | `LATE_WAIT=0` | 0.60 | 9 | 1,578 ms | - | 0 | ok |

The shader compiles under Wine (0 "compare could not be made"), the picture is
real (4.2 MB capture) and the pass it targets is **halved at worst**. The
1.10 fps run is the best median recorded on this machine.

Full run set (five runs, branch `hash64`):

| run | median | n | frame | removals |
|---|---|---|---|---|
| 1 | **1.10** | 16 | 926 ms | 0 |
| 2 | 1.65 | 4 | 967 ms | **2** |
| 3 | 0.60 (`LATE_WAIT=0`) | 9 | 1,578 ms | 0 |
| 4 | **1.10** | 14 | 905 ms | 0 |
| 5 | 0.80 | 5 | 1,244 ms | **2** |

**Not merged.** Two of five runs lose the device, which is a correlation, not
noise: the same build without this change ran many times with no removal. The
shader halves the pass it targets and gives the best median recorded here
(1.10, twice), but a change that resets the card in 40 % of runs is not shippable.

The prime suspect is `GroupMemoryBarrierWithGroupSync` in a groupshared fold
driven through vkd3d-proton at hundreds of groups a frame, not the index range
(`which = id.x / 64` cannot reach `pieces`). Next test: keep 64 threads per piece
but replace the barrier fold with a cross-lane shuffle (`WaveReadLaneAt`), which
needs no group barrier and no groupshared memory.

## 3.18 Texture budget and VRAM (2026-11-25)

The card runs at 16,295 of 16,304 MB during play, so texture copies were the
suspect for the hash64 device losses. `VICTORY_TEXTURE_BUDGET_MB` exists in this
tree (`resource.rs:1371`) and does what it says:

| config | median | n | frame | textures | removals |
|---|---|---|---|---|---|
| hash64 | 1.10 / 0.80 | 14-16 | 905-1,244 ms | 144-191 ms | 0 / **2** |
| hash64 + `TEXTURE_BUDGET_MB=4096` | **1.00** | 17 | 1,051 ms | **64 ms** | 0 |
| hash64 + `TEXTURE_BUDGET_MB=4096` | 0.10 | 3 | 10,246 ms | 84 ms | **2** |
| hash64 + `CARD_BUFFERS_MB=256` | 0.40 | 5 | 2,506 ms | - | **2** |

The budget **halves-to-thirds the texture phase** (64 ms vs 144-191) and is
worth keeping as a setting, but it does not stop the device losses: they happen
with 256 MB card buffers, with a 4 GB texture budget, and without either. That
leaves the group barrier in hash64 as the remaining suspect - next is the
lane-shuffle fold.

## 3.19 Texture budget 4096 MB: shipped (2026-11-25)

`VICTORY_TEXTURE_BUDGET_MB=4096` caps the bytes of texture copies on the card.
On the stable tree, two runs:

| config | median | n | frame | copies | textures | copy-back | GPU | removals |
|---|---|---|---|---|---|---|---|---|
| before | 0.50-0.70 | 15-35 | 1,464-1,976 | 239-533 | **350-371** | 91-93 | 53-69 | 0 |
| **+ budget 4096** | **1.00** | 13 | **1,046** | 590 | **40** | 76 | 40 | 0 |
| + budget 4096 | 0.80 | 16 | 1,182 | - | **48** | - | - | 0 |

Third run of the shipped profile (budget applied by `profiles.json`, default
everything else): **median 1.00, n=15, frame 1,024 ms, copies 277, textures 43.7,
no removals** - the change now has the three clean runs the rule asks for
(1.00 / 0.80 / 1.00).

The texture phase drops **7-9x** in every run - far outside the +/-40 % noise -
and the frame comes down with it. Copies rise (590) because textures are
evicted and re-fetched, which is the trade the budget makes; the net is a
faster, steadier frame. Shipped in the `linux-amd` profiles.

This is the answer to "we run out of VRAM": 16,295 of 16,304 MB was in use, and
the budget is what bounds it.

## 3.20 The real profile: 44 % of the CPU is wineserver, mapping views

First working `perf record -g` profile (kernel.perf_event_paranoid=1, game run
under perf, 220 s, 7.9 MB of samples):

| share | where | symbol |
|---|---|---|
| **27.99 %** | **wineserver** | **`is_valid_view_addr`** |
| **15.32 %** | **wineserver** | **`req_unmap_view`** |
| 9.74 % | ntdll.so | `__wine_syscall_dispatcher` |
| 7.49 % | ucrtbase.dll | — |
| 1.78 % | ntdll.so | `NtProtectVirtualMemory` |
| 1.62 % | d3d12core.dll | — |

**Nearly half the process's CPU time is wineserver validating and unmapping
memory views** — `is_valid_view_addr` alone is 28 %, and it is a linear scan of
the server's view list, called from `req_unmap_view`. That is the "unaccounted"
stall, and it explains why nothing in victory's own switches moved the frame:
the time is not in our code, it is in Wine's server process handling the title's
memory windows.

The title memory is mapped as *views* (`MapViewOfFile3`, `crates/victory-hle/
src/modules/xmem.rs`); every map/unmap pays this scan. The fix direction is to
**stop mapping and unmapping views per frame** — keep windows mapped, or use
plain committed memory with a copy where a view is only an addressing trick.

This also matches `docs/LAUNCHER.md`'s note from victory-fork #340 about a
"wineserver view-tree patch": someone already identified this.

## 3.21 How often: ~500 view unmaps a frame

Counter on `View::drop` (`xmem.rs`): the title's window views are unmapped at
**~350-400 a second** measured (`unmaps=36000 -> 42000` in 6 s), which at
0.5-0.7 fps is **~500-800 unmaps per frame**. Each one makes wineserver walk
its view list (`is_valid_view_addr`, 28 % of the CPU; `req_unmap_view`, 15 %).

That is the frame's missing second, and it is not in our process.

Fixes, in order of effort:

1. **Keep views alive.** A view that is dropped and remapped for the same range
   is pure loss; caching it removes the round trip.
2. **Batch.** Unmap many ranges in one call (the server's scan is per call, not
   per range), if the runtime exposes it.
3. **Patch Wine** with the view-tree lookup from victory-fork #340, which turns
   the scan into a tree walk. Helps every title, not just this one.

## 3.22 The churn is `Slice::view()`, not the placeholder map

Instrumented `map()` (the placeholder remap): only **14-15 calls** in a whole run.
So the 350-400 unmaps a second do not come from there. By elimination they come
from `Slice::view()` (`xmem.rs:488`), the only other place that builds a `View`:

```rust
let at = MapViewOfFile(section, FILE_MAP_WRITE, ..., len);
let view = View { at, len };
commit(at.Value as usize, len).then_some(view)
```

Every call maps a view and, when the returned `View` drops, unmaps it - one
wineserver list scan each way. **That is the hot path to cache**: keep the
`View` on the `Slice`/`Physical` and hand it out again while the section lives,
instead of map/unmap per use.

## 3.23 Keeping views alive makes it WORSE - the cache plan is wrong

`VICTORY_LEAK_VIEWS=1` skips `UnmapViewOfFile` (the mapping leaks, which is fine
for a four-minute measurement) to see what the unmaps cost:

| config | median | n | frame | copies | removals |
|---|---|---|---|---|---|
| budget, normal | 0.80-1.00 | 13-16 | 1,046-1,182 | 590 | 0 |
| budget + leak views | **0.40** | 16 | **2,716** | 1,115 | 0 |

**Twice as slow.** That overturns 3.21/3.22's plan: `is_valid_view_addr` walks a
*list*, so keeping views alive makes the list longer and every remaining scan
slower. Caching views would have shipped a regression.

What this means: the cost scales with the number of live views and the number of
scans. The fixes that can work are therefore the opposite of caching:

1. **Fewer views**, not longer-lived ones - map larger ranges per view (a whole
   window instead of a page run), so both the list and the scans shrink.
2. **No views at all** where possible: committed memory plus a copy, which never
   enters the server's view table.
3. **Patch Wine** (#340's view tree): turns the scan into a lookup, which would
   make both the live count and the scan count stop mattering.

## 3.24 Ninety percent of view maps are ONE page

Histogram of `n` in `View::map` over a run (`xmem: view run lengths`):

| slices per map | count | share |
|---|---|---|
| **1** | **25,500** | **~90 %** |
| 2 | 2,285 | 8 % |
| 4 | 700 | 2.5 % |
| 3 / 5 / 6+ | 63 | <1 % |

So the title's memory is touched one page at a time and each touch costs a
map/unmap pair with a wineserver list scan. That is the 350-400 unmaps a second.

The fix follows from the number, and it is *not* a cache of indefinitely many
views (3.23 proved a longer list is worse). It is **one larger mapping per
request**: when a single slice is asked for, map the 16 slices (64 KiB) that
contain it, provided the section holds them, and hand out the page inside. The
view count drops 16x, the list stays short, and every scan gets cheaper.

## 3.25 Mapping 64 KiB for a one-page request - built, unverified

Branch `wideviews`: `View::map` maps 16 slices when one is asked for, while
`View.len` stays the requested page so nothing can reach past it (that separation
was the correctness fix; without it a caller walking `bytes()` would touch
neighbouring pages).

First run: median 0.85, n=14, frame 1,169 ms, textures 34 ms, copies 555 ms,
**no removals, no crash**. Comparable to the shipped profile (0.80-1.00).

**Not verified**: the instrumentation now disagrees with itself - 44,000 maps
logged against 5 unmaps in the same window, which cannot both be right. Until
that is explained (either the counter is misplaced or the views are being kept
somewhere), the run cannot be trusted as evidence that the view count fell, and
the change stays on its branch.

## 4. Switch matrix (same build, same scene class)

| Config | Median | n | Removals |
|---|---|---|---|
| default (`LATE_WAIT=1 WIDE_BACK=1`) | 0.70–0.90 | 20–43 | 0 |
| `LATE_WAIT=0` | 0.50 | 39 | 0 |
| `LATE_WAIT=3` | 0.60–1.40 | 20–39 | 1 in 2 runs |
| `RELAY=2` | 0.40 | 29 | 0 (no gain) |
| `JOIN_COPIES=1` | 0.90 | 11 | 2 |
| `INPLACE=1` | 0.45 | 42 | 0 |
| `VIEW_GUARD=0` | 0.90 | 7 | 2 |
| `PIPELINE_WAIT_MS=24` | 0.40 | — | 0 (worse) |

Two-run variance of one config is ±30 %. Do not chase 10 %.

## 5. Memory behaviour

* Idle with nothing running: ~1 GB VRAM, ~7 GB RAM.
* In the world: **VRAM fills to 16 GB by itself** (card copies of title memory),
  GTT grows to 9–10 GB, title RSS ~4–6 GB, system RAM 26–28 GB of 30.
* Not a leak: it climbs and plateaus. It is duplication — title memory plus a
  card copy plus GTT — for a title that uses 16 GB total on console.
* A local LLM (`llama-server -ngl 99`, gpt-oss-20b) holds ~11.4 GB VRAM and
  pushes the title into GTT, costing roughly 2–5× frame time. The launcher warns.

## 6. Driver and platform

* `RADV_DEBUG=llvm` — the LLVM backend is compiled out of release builds.
* Mesa **26.2.7** crashes on the same shader as 26.3.1: not a 26.3 regression.
* Mesa **27.0.0-devel** (mesa-git) also crashes: not fixed upstream yet.
* `ACO_DEBUG=noopt` does not help: the fault is before/in the ACO front end.
* **lavapipe (software Vulkan) does not crash** — it ran 280 s to frame 4268 at
  ~1.5 fps with `drawn=0`. This is the proof that the shaders and pipelines are
  legal and the fault is RADV's compiler.
* vkd3d-proton `GE-Proton11-10` = build 5656 (crashes); Proton Experimental =
  5742 (works). The launcher installs 5742+ into the prefix.
* A bare `wineboot` prefix lacks `d3dcompiler_47.dll`/`wined3d.dll`; the prefix
  must be created through Proton's own script.

## 7. Dead ends (do not repeat)

* **Merging `reference/upstream-2026-11-25` into this tree**: 14 conflicting
  files (`texture.rs`, `watch.rs`, `host.rs`, `resource.rs`, `xmem.rs`,
  `lower.rs`, …). Both sides implemented view guards, page marking and texture
  refresh independently. It compiles and renders wrong.
* **PR #154 on the newer upstream**: guard-page violation (`0x80000001`) because
  upstream added its own page guards.
* `VICTORY_RELAY=2`, `JOIN_COPIES=1`, `LATE_WAIT≥2` on Linux: device removals.
* The skip-list approach for crashing pipelines: it diverges. Nine runs produced
  88 hashes and each run found more, because pipeline creation is concurrent and
  whichever program crashes first kills the process. It is a driver bug, and the
  fix is the newer vkd3d-proton, not a skip list.

## 8. Useful internals

* The title's settings live in `ASAV` saves as JSON with hashed keys. The save
  name hash is **FNV-1a 64** (`"SettingsProfileSettings"` →
  `0b4e25fca06268d0`, `"Player"` → `333dc56ddffd8ea0`); the 32-bit keys inside
  are not a straight FNV-1a 32 of obvious names.
* Login/engine errors: `crashes.log` in the title's data folder; device
  removals and unhandled exceptions are appended there.
* The title renames its main thread to `main`, which is why `pgrep -x
  victory.exe` finds nothing — match the cmdline instead.
* `VICTORY_KEYS` (diagnostics builds) presses keys at given seconds; it is the
  only way to drive the game headless. `VICTORY_CAPTURE` + `VICTORY_CAPTURE_FRAMES`
  write PNGs of presented frames.
