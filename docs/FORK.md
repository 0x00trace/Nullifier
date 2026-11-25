# This fork

A consolidation of the VIctory fork work as of **2026-11-25**, kept honest: what
runs, what was measured, what is deliberately not merged.

Read [`AGENTS.md`](../AGENTS.md) before changing anything.

---

## What this is

VIctory runs the Xbox Series build of the title natively on a PC: the title's
x86-64 code runs in our process and we implement the console platform beneath
it, translating its GPU programs to the host's D3D12. This fork collects the
AMD-specific work (`myokex/victory-amd`) with the Wine/Proton fixes that are
not merged upstream, and adds a hardware-aware launcher.

No game files are here. You need your own copy.

---

## Branches

| Branch | Base | State |
|---|---|---|
| `main` | `myokex/victory-amd` @ `070df42` + PRs #127, #270, #154 | **the Linux/AMD stack that runs today** |
| `upstream-2026-11-25` | `fxceless/victory-fork` @ `a26859f` + PR #127 | newer, faster textures, slower in the world on AMD |
| `pr127` … `pr270` | individual PR heads | for rebasing |

Upstream and the AMD tree **cannot be merged mechanically**: 14 files conflict
because both sides independently implemented view guards, page marking and
texture refresh. Two tested branches beat one untested merge.

---

## Measured results

Method: `tools/bench.sh`, median of world frames (`fps < 10`), n ≥ 20 unless
noted. Menus are 30 fps; loading dips to 5–20; peaks are camera cuts.

### RX 9070 XT (RDNA4), Mesa 26.3.1, Arch, Wine 11 (GE-Proton11-10 + Proton
Experimental's vkd3d-proton), Series X profile, 2560×1440, 30 GB RAM

| Build | Settings | Median | n | Device removals | Picture |
|---|---|---|---|---|---|
| base + #127 + #270 + #154 (`main`) | `LATE_WAIT=1 WIDE_BACK=1` | **0.70–0.90** | 20–43 | 0 | correct |
| same, `LATE_WAIT=0` | no send-ahead | 0.50 | 39 | 0 | correct |
| same, `LATE_WAIT=3` | aggressive | 0.60–1.40 | 20–39 | 1 in 2 runs | correct |
| same + `JOIN_COPIES=1` | — | 0.90 | 11 | 2 | correct |
| same + `INPLACE=1` | — | 0.45 | 42 | 0 | correct |
| same + `VIEW_GUARD=0` | — | 0.90 | 7 | 2 | correct |
| `upstream-2026-11-25` + #127 | `LATE_WAIT=1 TEXTURE_NO_WAIT=1` | 0.55 | 44 | 0 | correct |
| `upstream-2026-11-25` + #127 + #154 | same | — | — | crash `0x80000001` | — |

Perf switches that made no difference or hurt: `PIPELINE_WAIT_MS=24` (0.40),
`RELAY=2` (0.40).

### What each fix was worth

| Fix | Before | After |
|---|---|---|
| PR #127 (Wine HLSL) | 312 draws/frame, black world | 2,435 draws/frame, world renders |
| PR #270 (remap lock) | textures 688 ms/frame | 24–54 ms |
| PR #154 (VEH copy) | copies ~500 ms, 360 MB/s | 177 ms, 1.1 GB/s |
| Proton Experimental vkd3d-proton | segfault at first world mesh pipeline | no crash |

### For comparison, from the issue tracker

| Machine | OS | fps |
|---|---|---|
| RTX 3090 | Linux/Proton | 0.16 |
| RTX 5070 Ti | Linux/Proton | 1.85–1.97 |
| RTX 5070 Ti laptop / 4060 Ti | Windows | 2–4 |
| RX 9070 XT | Windows (issue #3) | ~1.0 with pipeline skips |
| RX 9070 XT | **this fork, Linux** | **0.7–0.9** |

---

## Where the time goes

At 0.7 fps the GPU is 4–6 % busy and the process uses ~0.7 of one core out of
12. One thread is busy; the other 102 idle. The submitting thread spends ~2 s
per frame between submissions running ~20 ms of CPU: it is blocked, not
computing.

Frame budget at ~1.7 s/frame:

```
preparing       ~1.2-1.6 s   (copies, texture refresh, heap passes)
  copies         200-750 ms  wall, though 1961 MB moves in 7 ms of memcpy
  textures       170-900 ms  GetWriteWatch under Wine dominates
GPU             60-240 ms
copy back       100-380 ms
```

The open problem is the wall-clock cost of victory's memory tracking and
copy/fence coordination under Wine, not bandwidth and not the GPU.

---

## Roadmap

1. **Dirty tracking** — replace/a mortise the per-frame `GetWriteWatch`
   (`victory-gpu/src/watch.rs`, texture lane). Biggest known win.
2. **Serialisation** — the submitting thread is blocked ~2 s/frame; find what
   it waits on (needs `perf`; `kernel.perf_event_paranoid` must be ≤ 1).
3. **Upstream rebase** — take `a26859f`'s texture work into the AMD tree
   properly (it needs the 14-file reconciliation, tested on both vendors).
4. **Ray-traced light** — shadowed areas are black until the RT path runs. Out
   of scope until the frame rate is usable.

---

## Credits and licence

GPL-3.0-or-later. Built on:

* [VIctory](https://github.com/Marvisak/VIctory) by Marvisak and contributors —
  the loader, runtime services, D3D12 back end, shader recompiler.
* [victory-fork](https://github.com/fxceless/victory-fork) by fxceless — the
  fork this one follows, plus PRs #127, #154, #270.
* [victory-amd](https://github.com/myokex/victory-amd) by myokex — the AMD
  fixes in `main`.

**Not** used, and not licence-compatible: [XCL](https://github.com/xcl-dev/xcl)
(GPL-2.0-only).
