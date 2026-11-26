# This fork

A **universal** fork of the VIctory work as of **2026-11-25**: one `main` that
runs on AMD and NVIDIA, Linux and Windows. Platform differences are chosen at
launch by `launcher/profiles.json`, not by branch. Kept honest: what runs, what
was measured, what is deliberately not merged.

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
| `main` | `myokex/victory-amd` @ `070df42` + PRs #127, #270, #154 | **the universal fork; what users run** |
| `reference/upstream-2026-11-25` | `fxceless/victory-fork` @ `a26859f` + PR #127 | read-only reference: newer, faster textures, slower in the world on AMD |
| `pr127` … `pr270` | individual PR heads | for rebasing |

We do not merge `reference/*`. Where a commit there is worth having, it is
ported as one PR with a bench row (`docs/MIGRATION.md`). Merging it wholesale
conflicts in 14 files because both sides independently implemented view guards,
page marking and texture refresh.

## Platform support

| Platform | State | Evidence |
|---|---|---|
| Linux + AMD | works, 0.7–0.9 fps | measured here (RX 9070 XT, Mesa 26.3.1) |
| Linux + NVIDIA | works, 3.5 fps with the wineserver from `tools/build-wineserver.sh`, 0.6–1.4 with Proton's own | measured here (RTX 5080, below); 1.85–1.97 fps in issue #6 (RTX 5070 Ti); profile is `reported` |
| Windows + AMD | works, ~1 fps | issue #3 (RX 9070 XT, needs skips) |
| Windows + NVIDIA | works, 2–4 fps | upstream README, issue #417 |
| Linux + Intel | untested | profile `untested` |
| macOS / Apple silicon | not supported | needs a Metal path; see roadmap |


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

### RTX 5080 (16 GB), NVIDIA 620.73.04, Arch, Proton Experimental 11.0-20261112
(wine 11.0, vkd3d-proton build 5742), `linux-nvidia` profile, 3840×2160

`tools/bench.sh`, three runs each, medians of world frames in run order.

| Build | Settings | Median | n | Device removals | Picture |
|---|---|---|---|---|---|
| `main` @ `3c34f8f` | profile | 0.60 / 1.00 / 0.95 | 9 / 12 / 8 | 0 | none taken: every run ended 17–20 s into the world |
| + raw reads made again when refused (`a0d014d`) | profile | 1.30 / 0.95 / 1.35 | 39 / 38 / 12 | 0 | world renders (captures of 2.0 and 1.5 MB); some faces black, some frames black |
| + guard pages left unread by copies (`c7e6f5f`) | profile | 0.90 / 1.20 / 0.70 | 40 / 41 / 40 | 0 | world renders (eight captures of 1.0–2.3 MB, one black); one sampled frame looks right, the others have a black face or a low-detail figure |
| + loops given one latch each (`fa7ea8d`) | profile | 0.90 / 0.80 / 0.80 | 35 / 37 / 40 | 0 | world renders (four world captures of 1.5–1.7 MB); defects as before: in one, hair drawn as a ball of strands over a face, in another a figure lit in a dot pattern |
| + an empty pipeline library asked for with no blob (`42466d5`) | profile, library kept from an earlier run | 0.85 / 1.30 / 1.40 | 42 / 44 / 44 | 0 | world renders (159 world captures of over 1 MB); faces and hair right in the 22 looked at; left: low-detail figures in one shot, one or two black frames at shot changes, arm hair drawn like fur |
| + a streamed texture's host copy made with the mips its view shows (`d038121`) | profile, library kept | 1.00 / 1.30 / 0.70 | 44 / 44 / 40 | 0 | world renders (201 world captures, every second one looked at); both leads' faces have their textures from their first shots in all three runs; left: a dark patch around one eye in one shot of the third run, video panels drawn green in some frames, and the defects of the row above |
| same build with `VICTORY_SHOWN_MIPS=0` (every mip, as before), run in turn with the row above | profile, library kept | 0.90 / 0.90 / 1.15 | 42 / 41 / 44 | 0 | world renders (232 world captures, every second one of the first 180 frames looked at); a lead's face black for the whole of the second run and dark in the first shots of the third; right in the first |
| the same build (`9e8bcd8`), served by the wineserver from `tools/build-wineserver.sh` (Wine's view tree) | profile, library kept | **3.55 / 3.45 / 3.50** | 46 / 46 / 47 | 0 | world renders (704 captures of over 50 KB; 72 of each run looked at: 30 spread over the run, 42 from its first 300 world frames); both leads' faces have their skin from their first shot in all three runs; the street, the car, the interior, the title cards and the video panels are drawn; in the third run the man's beard is missing for his first 290 world frames and there from the car scene on (there from the start in the other two) |
| the same build served by Proton's own wineserver, run in turn with the row above | profile, library kept | 0.60 / 0.65 / 0.70 | 41 / 40 / 42 | 0 | world renders (146 captures of over 50 KB; the first 30 to 42 world captures of each run looked at); a lead's face black in the first shot of the first run; in the third, late shots drawn before their parts have streamed in (a figure as a glowing outline, the things of a room in the air over the city) |
| + an indirect draw's scratch buffer left readable, no barrier after each draw (`02d27de`), built server | profile, library kept | 3.65 / 3.70 / 3.60 | 46 / 47 / 47 | 0 | world renders (758 captures of over 50 KB; 72 of each run looked at: 30 spread over the run and its first 42); both leads' faces have their textures from their first shots; the street, the car, the interior, the title cards and the video panels are drawn; the woman's top is dark grey in the first captures of the shot with raised hands in two of the runs, as in the three runs two rows up |
| the same build with `VICTORY_INDIRECT_WINDOW=1`, run in turn with the row above | profile + `VICTORY_INDIRECT_WINDOW=1`, library kept | **4.00 / 3.90 / 4.10** | 47 / 47 / 47 | 0 | world renders (817 captures of over 50 KB; 72 of each run looked at, as above); faces, street, car, interior, title cards and video panels as in the row above; the woman's top is white in every capture of that shot looked at |
| + a run of single indirect records clamped by one pass (`7754069`), switch off, built server | profile, library kept | 3.80 / 3.80 / 3.80 | 47 / 47 / 47 | 0 | world renders (783 captures of over 50 KB; 72 of each run looked at: 30 spread over the run and its first 42); faces, street, car, interior, title cards and video panels as in the rows above; the woman's top in the shot with raised hands is white, grey and dark in the three runs |
| the same build with `VICTORY_INDIRECT_RUNS=1`, run in turn with the row above | profile + `VICTORY_INDIRECT_RUNS=1`, library kept | **4.50 / 4.40 / 4.40** | 47 / 47 / 47 | 0 | world renders (924 captures of over 50 KB; 72 of each run looked at, as above); faces, street, car, interior, title cards and video panels as in the rows above; the woman's top is white in the three runs |

| the merged stack (#1–#5, #6 docs) on this AMD machine | `linux-amd-16gb`, library kept | 1.70 / 1.40 / 1.40 | 15 / 16 / 15 | 0 | world renders (captures 3.2–4.2 MB); card 6.1–6.3 GB of 15.2; texture refresh 94–281 ms, mostly the write-watch ask |
| + the wineserver from `tools/build-wineserver.sh` (Wine's view tree) | same | **3.25 / 3.10 / 3.20** | 10 / 9 / 9 | 0 | world renders (captures 3.7–4.1 MB); frame ~310 ms; copies 47–50, textures 14–19; card 6.3–6.4 GB |
| the same binary, served by Proton's own wineserver (A/B, one run) | same | 1.10 | 11 | 0 | world renders (capture 3.2 MB); frame 930 ms; copies 336, textures 196 |
* **Before:** each run ended on a breakpoint of the title's own
  (`+0x23d8f03` twice, `+0x22106e0` once), at most 0.3 s after
  `dstorage: request failed: copying … hr=0x800703e6`: the kernel would not
  write a raw read's destination, because a copy had guarded the block again
  between the opening and the read. Under 20 world frames, so these are not
  bench rows; they are what the change is measured against.
* **After:** no request failed in three runs. Five reads were refused and
  each landed on the second try. Runs 1 and 2 lasted the 300 s window.
* **Run 3 ended after 134 s** on `unhandled exception 0x80000001` in
  `victory_try_copy_op` (`copy_mapped` under `Shadow::update`, the submit
  path): the guarded copy's handler takes access violations and in-page
  errors, not guard-page violations. No read had been refused in that run.
  Cause and fix: the third row.
* **The third row (`c7e6f5f`):** the title describes buffer views at
  near-null addresses (`0x1` to `0x9`), and the copy of one reads from
  address 0 up. They are at most 456 bytes in every logged run but one (ten
  runs); in the run that stopped, one of 131072 bytes at `0x5` was logged
  half a second before. Its copy reached `0x11000`, the guard page of the
  first thread's stack in a Wine process. Copies now leave a guard page
  unread, and put back one they touch. Three runs lasted the 300 s window
  with no stop, and 12 more refused reads all landed on the second try, but
  no near-null view in them was long enough to reach the page: the fix is
  shown by its test, not yet in the title.
* **The fourth row (`fa7ea8d`):** five compute pipelines were never made
  under Wine: prop instance culling (two) and ray-traced shadow traces
  (three). vkd3d-proton's DXIL converter calls itself without end on a loop
  with more than one back edge in a shader that reads another wave lane; the
  worker making the pipeline ran out of stack, and Wine ends such a thread
  without a word (`WINEDEBUG=-all` hides its one line). Every earlier run
  listed them as `being made` for its whole length. These three runs list
  none of them and log no stack overflow (run with `WINEDEBUG=fixme-all`).
  The frame rate is within the noise of the row above and the picture
  defects are the same. The runs also saved every target of three frames
  (`VICTORY_DUMP_TARGETS=1`), which costs some seconds.
* Every run of the first four rows logs `no host pipeline library: pipelines
  are not kept`: the host refuses even an empty library here, so each run
  makes its pipelines again and drops hundreds to thousands of draws a frame
  meanwhile.
* **The fifth row (`42466d5`):** that refusal was ours to avoid. The empty
  library was asked for with a pointer and no length, which vkd3d-proton
  refuses (`E_INVALIDARG`) and Windows takes; with no library nothing was
  saved, the list of title pipelines to start early included, so every run
  began cold. Asked for with no blob, the library is made. Cold, the three
  runs of the fourth row made about 1,800 pipelines each on first use and
  dropped 74, 36 and 64 compute pipelines (212, 141 and 181 graphics ones) at
  least once as `being made`: hair simulation and strand generation,
  character skinning and the face rig's graph among them, passes that keep
  state from frame to frame. Warm, these three runs loaded 1,834 / 1,956 /
  1,984 pipelines, started 1,843 / 1,966 / 1,998 early, made 113 / 26 / 15,
  and dropped no compute pipeline that way (56, 3 and 5 graphics ones). The
  hair that hid a face is hair again and faces have their skin. The medians
  are inside the range of the rows above (two runs of the same build differ
  by ±40 %): this is a picture fix, not a speed claim. Run after a restart
  of the machine (kernel 7.3.5 to 7.3.6, same driver), with
  `VICTORY_CAPTURE_WORLD=1` (a picture of two frames in every eight).
* **The sixth row (`d038121`) and the seventh, its baseline:** the copy on
  the card of a texture the title streams was made with every mip the title
  describes, while the title has memory for the small ones first and its
  view says from which mip on. In one run of this scene the texture pass
  uploaded 30,931 such textures whose views showed 2,563 MB, into copies of
  11,205 MB (counted on a diagnostics branch). The copies took the card to
  nine tenths of its budget within seconds of the world's start, and past
  that the pass makes no more: thousands of texture views waited for
  minutes, a lead's face among them. A view from a later mip on now gets a
  copy of its own that holds the mips from there. The two rows are six runs
  of one build, the change on and off in turn (`VICTORY_SHOWN_MIPS=0` is
  off). Off, the card stood at 74–98, 90–99 and 90–97 % of its budget from
  second 13 to 200 of the world, 228, 260 and 334 passes made no copy for
  lack of room, no texture was uploaded for 57, 162 and 68 s at a stretch,
  and 12,225, 11,760 and 9,835 views were still waiting at the end; the
  NVIDIA kernel module logged three mapping errors
  (`intermapRegisterDmaMapping`). On, the card stood at 52–63, 50–61 and
  49–62 %, no pass lacked room, uploads never paused, 1,017, 1,118 and 4,263
  views waited at the end, and the module logged nothing. The medians
  overlap: this is a picture fix, not a speed claim. What a frame's time
  goes to did change: the texture phase takes 155–321 ms of it (38–52 ms
  off), because there is now something to upload in every frame. The layer
  also keeps more texture resources (109–124 thousand after 245 s of world,
  39–58 thousand off): one for each first mip a copy holds.

* **The eighth row and the ninth, its baseline:** nothing in the layer
  differs between them. Wine's server kept a process's views in a list and
  walked it at each map and unmap; the title keeps tens of thousands of views
  and remaps about a thousand a second. Measured in a run of the ninth kind
  (`perf`, in the world): the server at a whole core, all of it in
  `is_valid_view_addr` and `req_unmap_view`, and the thread that prepares the
  frame asleep 71 % of the time behind the two threads whose maps and unmaps
  the server was answering, because Wine holds its memory lock meanwhile. Wine commit
  3217449b1f71 of 2026-11-25 makes the list a tree; Proton Experimental
  11.0-20261112 does not have it. `tools/build-wineserver.sh` builds that
  Proton's server with it, and the launcher starts it. The two rows are six
  runs of one build, the two servers in turn. With Proton's own: frame
  1,694 / 1,571 / 1,503 ms, copies 492–698, textures 314–364, 100–116 ms
  waiting for the card, which is busy 10–12 % of the time. With the built
  one: frame 285 / 292 / 287 ms, copies 43–46, textures 11–14, 157–163 ms
  waiting for the card, busy 63–64 %; the server ran 1.3 % of the time for as
  many requests. No removal, no unhandled exception and no failed clamp in
  the six, and the NVIDIA module logged nothing new. In the six runs the
  server was started by a script outside the tree, as the launcher now
  starts it; one run each way through the launcher gave 3.70 fps (n=47) and
  0.90 (n=43). The three runs with the built server are within 3 % of each
  other, where two runs of a build used to differ by ±40 %. Evidence and
  method: `docs/FINDINGS.md` 2.5.

* **The tenth row and the eleventh:** with the built server the card is the
  larger part of a frame, and a timing run (`VICTORY_GPU_TIMING=1`) shows what
  it runs: of 117.5 ms a frame on the title's commands, 78.3 ms go to about
  3,100 indirect commands of one record each, 25 µs a command whatever it
  draws, against 6 µs for a direct mesh draw. The layer clamps each record
  with a compute pass of its own in front of the draw, between two waits,
  and moved its scratch buffer back to the common state behind the draw: a
  third barrier, and the only thing left between two draws served from a
  window (`VICTORY_INDIRECT_WINDOW=1`), which is why windows alone gave
  nothing on `9e8bcd8` (3.50 fps, n=47, 1,734 passes a frame of about 3,000).
  `02d27de`, ported from fxceless/victory-fork `f85101b`, leaves the scratch
  buffer readable until the next clamp, and takes that tree's rule that a
  draw record of 2^23 vertices an instance or more runs empty (it lost the
  device with windows on before it). A benchmark of the clamp's barriers
  (`indirect::bench`, no title needed) gives for 3,200 draws on this card:
  26.9 ms before and 20.3 ms now clamped one at a time, 12.7 ms before and
  2.7 ms now in windows of 32. The two rows are six runs of one build,
  windows off and on in turn. A run's frame lines stand in different scenes,
  so a line was fitted to each set's world frame lines against draws a
  frame: at 3,000 draws the frame is 279 ms on `9e8bcd8`, 262 ms with the
  change and 243 ms with windows on, and the wait for the card 152.6, 141.6
  and 122.5 ms. No removal, no unhandled exception and no failed clamp in
  the six, and the NVIDIA module logged nothing new. Windows stay off by
  default: under half of the title's records lie one step apart, and 1,727
  commands a frame still had a pass of their own. Evidence and method:
  `docs/FINDINGS.md` 2.6.

* **The twelfth row and the thirteenth:** with windows on, 1,727 indirect
  commands a frame still had a clamp pass of their own. Saved frame
  listings show why: the title draws in runs of up to some hundreds of
  single records between two of its compute passes, each record somewhere
  else in one argument buffer, and most lists begin with compute passes, so
  neither a window of records one step apart nor a clamp ahead of the list
  serves them. `VICTORY_INDIRECT_RUNS=1` gives the first single record after
  a command that may write arguments a pass that clamps a run: that record
  and the ones the draws after it read, up to 64, found by a table that is
  filled as those draws are recorded. Per frame, 2,598 to 2,637 commands
  then read their record from a run, and about 500 passes are left where
  there were about 3,000 (400 of them in front of the title's indirect
  dispatches). With it a list's budget moves to the card's own memory: all
  the threads of a pass charge that one word, and in memory the CPU maps
  that cost 4.8 ms for 3,200 records in runs of 64 against 1.0 ms on the
  card (95.8 ms against 1.4 ms in one pass). The two rows are six runs of
  one build, the switch off and on in turn: at 3,000 draws the frame is
  253 ms off and 215 ms on, the wait for the card 138.0 and 101.3 ms. One
  more run compared every draw served from a run with a fresh clamp of its
  record, on the card: 2,210,845 draws, none differed. No removal, no
  unhandled exception and no failed clamp in the seven, and the NVIDIA
  module logged nothing new. Off by default: one machine and one scene are
  not enough to change a profile. Evidence and method:
  `docs/FINDINGS.md` 2.7.

### The texture ask (P0 probe, 2026-11-25)

~10,000 textures are asked whether they changed, every frame, at ~0.1 ms each:
**1075 ms of a 1134 ms refresh budget**. Throttling the ask to every 4th frame
(`VICTORY_TEXTURE_ASK_EVERY=4`, off by default) drops it to 145 ms and the frame
from 1.2–3.9 s to ~1.0 s, but currently ends in a guard-page fault
(`0x80000001`); see `docs/FINDINGS.md` §3 and `docs/ROADMAP.md` P0.

Every cause and its evidence lives in [`docs/FINDINGS.md`](FINDINGS.md).

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
| RX 9070 XT | **this fork, Linux, view-tree wineserver** | **3.1–3.3** |
| RX 9070 XT | this fork, Linux, Proton's wineserver | 1.1–1.3 |

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

The open problem is the wall-clock cost of Nullifier's memory tracking and
copy/fence coordination under Wine, not bandwidth and not the GPU.

---

## Launcher

`launcher/nullifier-launch.sh` / `.ps1` detect OS, GPU vendor and VRAM, pick a
profile from `launcher/profiles.json`, create the Proton prefix if needed,
install a vkd3d-proton new enough for RADV, warn about VRAM hogs, and start the
title. `--detect` prints the chosen profile, every variable, the confidence and
the reason. See `docs/LAUNCHER.md`.

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
