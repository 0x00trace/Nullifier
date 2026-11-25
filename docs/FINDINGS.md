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
