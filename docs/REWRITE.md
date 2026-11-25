# Rewriting the foundation: one shared heap instead of two copies

Authorised direction: change the foundation if it removes the performance wall.
This is the design I would build. It targets the two costs that dominate every
measurement — **copies (27 %)** and **textures (19 %)** — at their root: the
duplication of the title's memory.

## The problem with the current foundation

```
title memory (system RAM)          card memory (copy)
        |                                  ^
        |  every changed range             |
        +------ explicit copy -------------+
        |                                  |
   per-texture write-watch ask      per-texture detile + upload
```

Victory keeps the title's memory *and* a copy of it in the card's memory, then
spends the frame figuring out what changed and re-sending it. Every object
touched costs a call, and under Wine a call costs 10–100× a native one. Ten
thousand textures and 25–45 submissions a frame is the entire budget.

The data is small (117 MB/s at 0.5 fps), so the copies are not moving bytes —
they are *managing* them.

## The foundation to build

**Make the title's memory the GPU's memory.** One allocation per title heap,
created as a resource the GPU reads directly, with the CPU writing into the
same bytes. No shadow, no per-range copy, no detile on the critical path.

Vulkan already supports this and RADV exposes it:

* `VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | DEVICE_LOCAL` with ReBAR, or
  host-visible system memory with `VK_EXT_external_memory_host` for the rest;
* D3D12 surfaces it as a *custom* heap / reserved resource that vkd3d-proton can
  map (`ID3D12Device::CreateHeap` + `CreateReservedResource`, or a placed
  resource over a shared allocation).

Victory already has **stage one** of this under `VICTORY_INPLACE`
(`crates/victory-gpu/src/inplace.rs`): title memory opened as a graphics heap,
buffers used in place. Completing it is the rewrite.

## What has to change

1. **Allocation** (`hle/xmem.rs`, `gpu/inplace.rs`): every title heap that the
   GPU can see becomes one graphics allocation; title pages map to it. Keep a
   fallback to the copy model for heaps that cannot be shared (small BAR,
   unsupported driver, protected memory).
2. **Coherency** (`gpu/shadow.rs`, `gpu/texture.rs`): with CPU and GPU on the
   same bytes, correctness depends on barriers and cache maintenance, not on
   copies. On RADV: `vkFlushMappedMemoryRanges` after CPU writes and
   `vkInvalidateMappedMemoryRanges` before CPU reads, or non-coherent memory
   with explicit flush. This replaces the write-watch/detile machinery for
   shared heaps entirely.
3. **Textures**: tiled console layouts cannot be read by the GPU in place — the
   console's swizzle must either be expressed in the resource layout
   (`VK_IMAGE_TILING_OPTIMAL` with a matching block layout is not exposed) or
   detiled once at upload and kept in a host-visible *linear* image. The honest
   answer is textures stay on the existing path; the rewrite targets **buffer**
   traffic, which is the copies.
4. **Fallback**: keep today's path behind a runtime switch; profiles choose.

## Why this is the right target

* It removes **copies** entirely for shared heaps — 27 % of the frame, the
  largest single line in every measurement.
* It removes the per-range write-watch question for those heaps — the ask was
  measured at ~1 s/frame before throttling.
* It is the only change that reduces *calls*, which is what the evidence says
  matters (`docs/FINDINGS.md` 3.13: guards off changed nothing; per-call cost
  dominates).

## Risks, honestly

* **ReBAR size.** A 16 GB card with a 256 MB BAR cannot map 10 GB of title
  memory. Profiles must detect `mem_info_vram`/BAR size and fall back.
* **CPU write speed.** Write-combined memory is slow for CPU *reads*; the title
  reads its own memory constantly. Host-visible *system* memory (GTT) is
  readable at normal speed but slower for the GPU — this is the trade the
  design has to measure, not assume.
* **Coherency bugs are silent.** Wrong flush ordering gives stale data, not a
  crash. Every milestone needs a picture check.
* **vkd3d-proton may not expose what we need.** It is a D3D12 implementation;
  a custom heap mapped onto title memory may have to go through
  `VK_EXT_external_memory_host` at the vkd3d level, which is a patch to
  vkd3d-proton, not to victory. That is the first thing to verify — a one-day
  spike, before any rewrite.

## Milestones

| # | Deliverable | Gate |
|---|---|---|
| 0 | Spike: can vkd3d-proton place a resource over externally-hosted memory on RADV? | a triangle drawn from title memory |
| 1 | One shared buffer heap, everything else on the copy path | same picture, `copies` phase down for that heap |
| 2 | All buffer heaps shared, fallback intact | `bench_scene.sh` × 3: median up, p10 up, picture correct |
| 3 | Remove dead copy paths | no regression on NVIDIA / Windows |

## What it does not fix

Textures, pipeline creation, and the per-submission overhead remain. Expect the
copies line (27 %) to shrink to near zero and textures (19 %) to stay; a frame
might go from ~1,500 ms to ~1,000 ms. That is the honest ceiling of this
rewrite: **roughly 1.5×, not 30 fps**.
