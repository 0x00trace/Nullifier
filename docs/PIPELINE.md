# How the frame pipeline works, and where it loses time

A map of the path from the title's draw calls to the card, with the measured
cost at each stage. Numbers are from an RX 9070 XT under Proton
(`docs/FINDINGS.md`); the shape is the same everywhere, the constants differ.

## The path

```
title threads (game code)
  |  D3D12 calls: PSO binds, draws, copies, barriers, map/unmap
  v
victory-hle  (crates/victory-hle/src/modules/*)
  |  the console's API surface; D3D12 is forwarded to victory-gpu
  v
victory-gpu
  |-- recorder: turns draws/dispatches into command lists (list.rs, draw.rs)
  |-- resource/shadow: a copy of title memory in the card's memory (shadow.rs)
  |-- texture: staging buffers, detiling (texture.rs)
  |-- watch: which title pages changed (watch.rs)
  |-- pipeline: DXIL translation, PSO creation, cache (pipeline/*)
  v
vkd3d-proton (DXIL -> SPIR-V, D3D12 -> Vulkan)
  v
RADV (shader compile, submission) -> the card
```

The title itself runs unchanged; everything between "title wrote memory" and
"card has the data" is ours.

## Where a frame actually goes

Median of 33–74 world frames, `tools/frames.py`:

| Stage | Cost | Share | What it is |
|---|---|---|---|
| copies | 400–695 ms | 19–35 % | title memory -> its copy in card memory |
| textures | 270–383 ms | 17–22 % | write-watch ask + detile + upload, per texture |
| copy-back | 63–141 ms | 4–7 % | what shaders wrote, back to title memory |
| GPU | 62–109 ms | 3–7 % | the card's own work |
| unaccounted | 28–700 ms | 2–35 % | waits: fences, submission lock, pacing |

The GPU is idle 93–97 % of the frame. Nothing here is bandwidth-bound: at
0.5 fps a frame moves ~230 MB, i.e. **117 MB/s**, against tens of GB/s of
memory bandwidth and 16 GB/s of PCIe.

## The four mechanisms, in the order they cost

### 1. Per-object cost on one thread

The submitting thread walks a list of steps per submission (`host.rs`, the
`Step` enum: `Host`, `GpuWrite`, `Texture`), one after another. Each step is
cheap on Windows and expensive under Wine, because each is one or more kernel
crossings:

* `Step::Texture` -> `Staging::refresh` -> `watch::writes` -> `GetWriteWatch`
  (or a guard page walk) then `copy_mapped` + `detile` for the **whole**
  texture;
* `Step::Host` -> `copy_mapped` (a kernel or VEH-guarded copy per range);
* `Step::GpuWrite` -> `shadow.writing`.

A world frame has ~10,000 textures and 25–45 submissions. Ten thousand asks at
~0.1 ms each is ~1 s. That is the single largest identified cost.

### 2. Whole-texture detiling

`refresh_timed` (`crates/victory-gpu/src/texture.rs:646`) decides *whether* a
texture changed, then copies and detiles **all of it** (`detile`,
`texture.rs:891`). One dirty 4 KiB page in a 16 MB texture costs 16 MB of
detiling; a frame detiles 6–16 GB.

### 3. Copy coordination

`copy_mapped` moves 1,961 MB in 7 ms when measured as `memcpy`, yet the copy
phase costs 0.4–0.7 s of wall time. The bytes are not the cost; the per-call
setup, the fence waits around the copy, and the fact that it all happens on the
submitting thread are.

### 4. The unaccounted stall

Up to 0.7 s a frame is not in any phase. Traced (partly) to the submitting
thread sitting between submissions: ~2 s of wall time for ~20 ms of CPU. It is
a wait, and identifying it needs a stack sample (`perf` is blocked by
`kernel.perf_event_paranoid=2`; `VICTORY_PROFILE=1` upstream has an in-process
sampler we do not).

## What to fix, and how

### A. Page-granular refresh (biggest, safest)

Read only the pages `watch::writes` reports, and detile only the blocks they
cover. The inverse mapping is defined by the console tiling
(`crates/victory-formats/src/texture.rs`: block size, swizzle, mip placement,
mip tails) and by `Staging::part`/`subresource` (`texture.rs:623`). It cannot
stale a texture: it copies *more precisely* what is copied wholesale today.
Behind `VICTORY_TEXTURE_PAGES=1`. Expected: the 6–16 GB/frame of detiling drops
with the changed-page count, likely an order of magnitude.

### B. Batch the ask (second biggest)

One `GetWriteWatch` per frame over the whole texture heap, then attribute the
returned pages to textures, instead of one call per texture. The journal
(`watch.rs`) already has an "asked ahead" path for exactly this; the work is to
make the texture lane use it. The throttle (`VICTORY_TEXTURE_ASK_EVERY`) proved
the ask is ~1.1 s/frame; batching removes it rather than spreading it.

### C. Parallel steps (structural)

The steps of a submission are independent per object except for ordering into
the command list. Sharding the journal by address range and refreshing textures
on N threads would use the 11 idle cores. `VICTORY_WIDE_REFRESH` and
`VICTORY_WIDE_MERGE` are the first attempts at this and did not move the median
alone; parallelism has to cover the ask and the detile, not just the copy.

### D. Measure properly first

None of the above can be judged at ±40 % run-to-run noise. A deterministic
benchmark (fixed save, replayed input, a fixed frame window, warm shader cache)
is a prerequisite for claiming any of them works. `VICTORY_KEYS` and the
autosave checkpoint are enough to build it.

## What is *not* the problem

* GPU power: 3–7 % busy.
* VRAM: 16 GB fills, but the card is not the limit.
* RAM bandwidth: 117 MB/s used.
* Disk: ~15 major faults/s in the world; the NVMe sleeps.
* Swapping: `si/so` 0/0, zram flat during play.
* The Wine build: three runs spanned 1,441–2,072 ms, i.e. noise.
