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
| World under 1.5 fps under Wine, the frame thread asleep most of the time, `wineserver` at a whole core | Wine's server walks a list of every view of the process at each map and unmap, and the title keeps tens of thousands | `perf`: `is_valid_view_addr` and `req_unmap_view` are all of `wineserver`'s time; 3.55 / 3.45 / 3.50 fps with the tree against 0.60 / 0.65 / 0.70 | `tools/build-wineserver.sh` (2.5), until Proton has Wine commit 3217449b1f71 |

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

## 2.5 Most of that was waiting for Wine's server, which walked a list of every view (2026-11-26)

RTX 5080 (16 GB), NVIDIA 620.73.04, Ryzen 9 9900X, kernel 7.3.6, Proton
Experimental 11.0-20261112, `linux-nvidia` profile, build `9e8bcd8`.

The budget above is what the layer's clocks show, and it is wrong about why.
3.20 found `is_valid_view_addr` and `req_unmap_view` on top of the profile,
and 3.27 took the layer's own views out of them. What stayed was put down to
"the title's own page remaps" and left. It was most of the frame.

**What was measured.** One run, in the world: `perf record -a -g -F 499` for
32 s, then `perf trace -s` on the title for 20 s, then the scheduler's switch
and wake-up events for 12 s.

* `wineserver` used a whole core for the whole window: 32.6 s of processor
  time in 32.2 s. Those two functions are 16.13 of its 16.52 % of the machine's
  samples.
* In the 12 s of scheduler events it ran 84 % of the time and was woken 43,489
  times, 39,265 of them by two threads of the title's process, the ones that
  map and unmap its views: 3,270 requests a second, 231 µs of the server's
  time each.
* The thread that prepares and submits the frame ran 10 % of the time. It
  slept 10.8 s of the 12, and 8.5 s of that were 30,709 sleeps of about 277 µs,
  each one ended by one of those two threads. 0.9 s were waits for the card.
* The process had 38,999 mappings, 27,646 of them views of the title's pages.

**Why that is the frame.** The title maps its tiled heap one 64 KiB page per
view (`map_alias` in `xmem.rs`) and keeps tens of thousands of views. Wine's server
kept a process's views in a list: a map walked all of it to look for an
overlap, an unmap walked it to find the view. Wine also holds its
virtual-memory lock while the server answers either request
(`virtual_map_section` and `unmap_view_of_section` in ntdll), and
`GetWriteWatch` and `VirtualProtect` take that lock. A frame of ours makes
about 2,900 askings and 4,400 protection changes, and each one waited for the
remap then in progress. That is the thread section 2 calls "blocked, not
computing" and 3.9 "parked", and the reason no switch of ours moved the
phases: they were waiting.

**The fix is Wine's.** Upstream commit 3217449b1f71 of 2026-11-25, "server:
Store per-process memory views in an rbtree" (Wine bug 60868), makes both
walks a tree search. Proton Experimental 11.0-20261112 does not have it.
`tools/build-wineserver.sh` builds that Proton's server from Valve's source,
at the commit that Proton was built from, with this one commit carried over
(`tools/wine/server-view-tree.patch`). It checks that the result speaks the
protocol of Proton's own server and installs it outside Proton; the launcher
starts it before wine (`docs/LAUNCHER.md`). Nothing of Proton's is changed.

**Six runs of one build, the two servers in turn** (`tools/bench.sh`, 300 s
each, `VICTORY_CAPTURE_WORLD=1`; phases are medians over a run's world frames,
in ms):

| server | median fps | n | frame | copies | textures | GPU | copy-back | removals |
|---|---|---|---|---|---|---|---|---|
| Proton's own | 0.60 / 0.65 / 0.70 | 41 / 40 / 42 | 1,694 / 1,571 / 1,503 | 492-698 | 314-364 | 100-116 | 80-93 | 0 |
| **built, views in a tree** | **3.55 / 3.45 / 3.50** | 46 / 46 / 47 | **285 / 292 / 287** | 43-46 | 11-14 | 157-163 | 21-24 | 0 |

No device removal, no unhandled exception and no failed clamp in any of the
six. Picture, from 72 captures of each run with the built server (30 spread
over the run, 42 from its first 300 world frames): both leads' faces have
their textures from their first shot, and the street, the car, the interior,
the title cards and the video panels are drawn. With Proton's own server a
lead's face is black in the first shot of run 1, and late shots of run 3 are
drawn before their parts have streamed in (a figure as a glowing outline, the
things of a room hanging in the air over the city): the title streams by the
clock, and at 0.6 fps the frames do not keep up with it. One difference
among the runs with the built server: in the third the man's beard is missing
for his first 290 world frames and there from the car scene on, and it is
there from the start in the other two. The cause was not looked for; the
pipelines dropped are the same in all six runs.

The same three measurements with the built server (one run, 3.50 fps): the
server ran 1.3 % of the time for as many requests (44,070 wake-ups in 12 s,
under 4 µs each); the frame thread ran 35 % of the time, and 6.98 s of the
7.71 s it slept were waits for the card.

**What this changes in the rest of this file.**

* The phases on the frame line were waits. With no change to the layer,
  copies fell to a twelfth, textures to a twenty-fifth and copy-back to a
  quarter (table), the write-watch askings from 517-668 ms a frame to 32-35 ms
  for the same 2,900 to 3,050 askings, the pass over the heap from 261-303 ms
  to 28 ms, and the time from a present to the title's first list call from
  374-506 ms to 11-12 ms. Every other phase time in this file was measured
  under a server that walked the list: it says how long the phase waited
  there, not what it costs.
* The run-to-run noise of 3.12 was largely the server's too: three runs
  within 3 % of each other here, against ±40 %.
* The card is now the larger part. 157-163 ms of a 285-292 ms frame are waits
  for it, and by the layer's own count it is busy 63-64 % of the time at the
  median of a run (10-12 % under Proton's server). The next step is on that
  side: what the card runs, and how many of the frame's 62 submissions wait
  for it one by one.
* `bench.sh` counts frame lines under 10 fps as the world. The fastest lines
  of these runs are 7.7 to 8.6 fps: the limit will need raising soon.

## 2.6 What the card runs: two thirds of its time on the title's commands went to indirect draws, about 25 µs each (2026-11-26)

Same machine and settings as 2.5, with the built server. Build `9e8bcd8` for
the measurements, `02d27de` for the change.

**What was measured.** One run with `VICTORY_GPU_TIMING=1 VICTORY_D3D12_DRED=1`:
every list is waited for on its own and has a timestamp in front of every
command the title recorded (median 2.90 fps, n=47, no removal). Medians over
its 47 world lines, per frame:

| the title's commands, by first word | on the card | commands | each |
|---|---|---|---|
| indirect (one record a call) | 78.3 ms | 3,110 | 25.2 µs |
| dispatch | 41.8 ms | 1,262 | 33.1 µs |
| alias copy | 6.3 ms | 78 | 80.8 µs |
| mesh (direct) | 2.4 ms | 378 | 6.3 µs |
| indexed, draw (direct) | 2.4 ms | 210 | 11 µs |
| all | 117.5 ms | | |

The card was waited for 188 ms of a 344 ms frame in that run. What the
commands do not account for is the layer's own lists (the passes over copies
in the card's memory, preludes) and the waits themselves.

* The dispatches are the title's work: in one line of that run thirteen
  compute pipelines took 0.7 to 9.3 ms a frame each, 29 ms together.
* The indirect commands are not. A direct mesh draw costs the card 6 µs and an
  indirect one 25, whatever it draws: two pipelines that draw depth only (44
  and 72 commands a frame in that line) cost 16 and 11 µs a command. The
  difference is what the layer records around each one.

**Why.** Before an indirect command the layer clamps its record into a
scratch buffer with a compute pass of its own (`indirect.rs`), so that a
record of garbage cannot run the card for seconds. Around one draw that was:
a wait for every earlier shader write and a state change, the pass, a second
wait and state change, the draw, and a third state change that moved the
scratch buffer back to the common state. The card leaves drawing for compute
and comes back, and each of the three barriers ends the pass the draw was
in.

**The benchmark.** `indirect::bench` draws 3,200 one-record indirect draws of
a tiny triangle each way and times them on the card
(`cargo test -p victory-gpu --lib indirect::bench -- --ignored --nocapture`;
no title files). On the RTX 5080, through the vkd3d-proton of Proton
Experimental 11.0-20261112:

| the 3,200 draws | on the card | a draw |
|---|---|---|
| read straight from the title's records (not something the layer may do) | 0.06 ms | 0.02 µs |
| a pass for each, the scratch buffer back to common after each (before) | 26.9 ms | 8.4 µs |
| a pass for each, the scratch buffer left readable (now) | 20.3 ms | 6.3 µs |
| a pass for every 32, back to common after each draw (windows, before) | 12.7 ms | 4.0 µs |
| a pass for every 32, left readable (windows, now) | 2.7 ms | 0.9 µs |

So the third barrier was a quarter of the cost of a draw clamped alone, and
nearly all of the cost of a draw served from a window: a window
(`VICTORY_INDIRECT_WINDOW=1`) took the pass away and left a barrier after
every draw. One run of `9e8bcd8` with windows on shows it: 1,734 passes a
frame where there were about 3,000, and the same frame rate (3.50 fps, n=47;
3.45 to 3.55 without).

**The change** (`02d27de`, ported by hand from fxceless/victory-fork
`f85101b`): the scratch buffer stays readable as indirect arguments after a
clamp. The next clamp moves it straight back to unordered access, and the
list moves it to the common state once, when it closes. With it comes that
tree's rule for windows, which it lost the device without: a draw record that
asks for 2^23 vertices or indices an instance or more runs empty and charges
nothing. A window charges the list's budget once for a record however often
it is drawn, and upstream found a list drawing 90 records of 2^24 vertices
from one.

**Six runs of `02d27de`, windows off and on in turn** (`tools/bench.sh`,
300 s each, `VICTORY_CAPTURE_WORLD=1`), against the three runs of 2.5 with
the built server. A run's frame lines stand in different scenes (1,000 to
4,700 draws a frame), so beside the medians a line is fitted to each set's
world frame lines, of frame time and of the wait for the card against draws
a frame, and read at 3,000 draws:

| | median fps | n | frame at 3,000 draws | wait for the card at 3,000 draws | per 1,000 draws | removals |
|---|---|---|---|---|---|---|
| `9e8bcd8` (2.5) | 3.55 / 3.45 / 3.50 | 46 / 46 / 47 | 279 ms | 152.6 ms | 28.2 ms | 0 |
| `02d27de`, default | 3.65 / 3.70 / 3.60 | 46 / 47 / 47 | 262 ms | 141.6 ms | 24.4 ms | 0 |
| `02d27de`, `VICTORY_INDIRECT_WINDOW=1` | **4.00 / 3.90 / 4.10** | 47 / 47 / 47 | **243 ms** | **122.5 ms** | 19.6 ms | 0 |

The fit is steady: the three runs of the first row give 152.6, 153.1 and
152.1 ms one by one. No device removal, no unhandled exception and no failed
clamp in the six. With windows on, 1,727 indirect commands a frame still
have a pass of their own (the median of the first such run): the title keeps
its mesh records scattered through one large argument buffer, and a window
of records one step apart serves under half of them.

Picture, from 72 captures of each run (30 spread over the run and its first
42): the title cards, the street, the car, the interior and the video panels
are drawn, and both leads have their face textures from their first shots.
In the first captures of the shot with raised hands the woman's top is dark
grey in two of the three default runs, as it is in the three runs of 2.5,
and white in every capture of that shot looked at in the three runs with
windows.

## 2.7 One clamp pass for a run of draws: about 500 passes a frame where there were 3,000 (2026-11-26)

Same machine and settings as 2.6. Build `7754069`.

**Where the records lie.** With windows on (2.6), 1,727 indirect commands a
frame still had a pass of their own. A run with `VICTORY_CAPTURE_WORLD=1`
saves a listing of four world frames, one line per command and list. In
eight such frames (two runs of `9e8bcd8`): 1,292 to 3,403 single draw records
a frame, eight or nine in ten of them mesh records. In two of the frames,
counted list by list:

* Most lists begin with compute passes and then draw. Only 26 and 34 % of
  the records lie before any dispatch of their list. That is the part a
  clamp in the list's prelude can serve (the "gather" of
  fxceless/victory-fork `465b94f`, which reports that it serves about 600
  of the 1,600 draws that miss a window there).
* The draws come in long runs: 34 and 49 runs a frame between two commands
  that may write arguments, the longest of 401 draws. Within a run the
  records are scattered through one argument buffer, which is why a window
  of records one step apart misses half of them.

**The change** (`VICTORY_INDIRECT_RUNS=1`, off by default). The first single
record after a command that may write arguments gets a pass that clamps a
run: that record and the ones the draws after it read, wherever they lie in
the same argument buffer, up to 64. The pass cannot know those records when
it is recorded, so it reads them from a table in the list's upload memory:
a count and, for each slot, the offset of its record. The table is filled
as the draws are recorded and is complete when the list is submitted. The
draws read their records from the run's slots with no pass and no barrier.
A run ends at a compute dispatch, at a draw whose shaders can store to
buffers, at an indirect call with a count buffer or more than one record,
at a record of another kind or argument buffer, at another host list, and
when it is full; the next single record begins another.

* What a draw reads is what a pass in front of it would have clamped: the
  run's pass runs behind everything recorded before it, and nothing that
  may write arguments lies between it and the run's draws.
* Every draw has a slot of its own, also a draw of a record drawn before
  (91 to 412 draws a frame in the saved frames). The pass charges the
  list's budget for each slot, so the guard against garbage records counts
  a record once for each draw of it, as before.

**The budget word.** Every thread of a pass charges the same word of the
list's budget with a compare and swap, and the list kept that word in memory
the CPU maps. With one record a pass that costs nothing that can be seen.
With many it is most of the pass (`indirect::bench`, 3,200 draws, on the
card):

| the 3,200 draws | budget in mapped memory | budget in the card's memory |
|---|---|---|
| a pass for each | 21.1 ms | 21.1 ms |
| windows of 32 | 2.8 ms | 1.2 ms |
| runs of 64 | 4.8 ms | 1.0 ms |
| one pass for all | 95.8 ms | 1.4 ms |

So a list now keeps its budget in the card's own memory when runs or windows
are on (`VICTORY_INDIRECT_CARD_BUDGET`, after fxceless/victory-fork, where
it is always there): the list zeroes it before its first clamp and copies
the clamp's notes out when it closes. It is also why a run stops at 64
records, one group of the clamp's threads, though runs of 256 would make
somewhat fewer passes.

**Six runs of `7754069`, the switch off and on in turn**, as in 2.6:

| | median fps | n | frame at 3,000 draws | wait for the card at 3,000 draws | per 1,000 draws | passes of ours a frame | removals |
|---|---|---|---|---|---|---|---|
| `9e8bcd8` (2.5) | 3.55 / 3.45 / 3.50 | 46 / 46 / 47 | 279 ms | 152.6 ms | 28.2 ms | about 3,000 | 0 |
| `7754069`, default | 3.80 / 3.80 / 3.80 | 47 / 47 / 47 | 253 ms | 138.0 ms | 23.8 ms | 3,026 / 2,997 / 3,007 | 0 |
| `7754069`, `VICTORY_INDIRECT_RUNS=1` | **4.50 / 4.40 / 4.40** | 47 / 47 / 47 | **215 ms** | **101.3 ms** | 10.3 ms | 496 / 495 / 492 | 0 |

(The passes are the frame line's "indirect commands bounded".) With the
switch, per frame at the median of a run, by the line that says what became
of each command's clamp: 2,598 to 2,637 commands read their record from a
run, 103 to 105 commands opened one, and 397 to 414 others had a pass of
their own (388 to 405 dispatches, 4 draws whose shaders can store). 96 of
100 single records are served without a pass of their own. The card is busy 57 % of the time by the layer's count (62 to 64 %
without). No device removal, no unhandled exception and no failed clamp in
the six, and the NVIDIA module logged nothing new.

**Checked in the title.** One more run with
`VICTORY_INDIRECT_WINDOW_CHECK=1`: every draw served from a run has its
record clamped again in front of it, and the two are compared on the card.
2,210,845 draws in 300 s, none differed (3.70 fps with the check's passes,
n=47, no removal).

Picture, from 72 captures of each of the six runs (30 spread over the run
and its first 42): the title cards, the street, the car, the interior and
the video panels are drawn, and both leads have their face textures from
their first shots, with the switch off and on. The woman's top in the shot
with raised hands (2.6) is white in the three runs with the switch, and
white, grey and dark in the three without.

**What is left on the card.** At 3,000 draws the frame is 215 ms: 101 ms of
waiting for the card, 70 ms of preparing, about 20 ms of copying back. The
wait no longer grows much with the draws (10.3 ms per 1,000); 70 ms of it
are there at any number of draws. The 400 passes left are in front of the
title's indirect dispatches, each of which may write the next one's record.

## 2.8 The layer's own hash passes: a group of threads to a piece (2026-11-26)

Same machine and settings as 2.6 and 2.7. Builds `a881feb` (the timing) and
`7b4ffa4` (the change), and `44f17e3` for the benchmark.

**What they are.** For each copy of a title buffer in the card's memory the
layer keeps hashes of the copy's 64 KiB pieces on the card (3.6). A list of
ours in front of a submission's lists sends up what the title changed and
hashes those pieces again. Another behind them hashes the ranges the
submission's shaders may write and flags the pieces whose hash is another;
those are fetched. In the title's opening, medians of a run's world frame
lines: 934 to 975 hash passes a frame over 2,051 to 2,253 pieces, and 119
to 123 compare passes over about 50,000 pieces, of which 913 to 1,088 are
flagged and fetched.

**What they cost.** A timing run (2.6) did not show these lists: it timed
the title's commands. `a881feb` waits for each of the two lists on its own
in a timing run and logs its time from submission to its end ("ours-sync",
"ours-compare"). By the clock, which is about 0.05 ms a list more than the
card's time. (Timestamps around the lists were tried first and read 0.0 ms
for the compare: a timestamp behind a list's last compute passes was taken
while they still ran. A title list that ends in dispatches may be short in
the timing run's figures in the same way.) With `VICTORY_INDIRECT_RUNS=1`,
medians of 47 world frame lines each:

| the card's time a frame | `a881feb` | `7b4ffa4`, `VICTORY_CARD_HASH_LANES=1` |
|---|---|---|
| the title's dispatches | 41.8 ms in 1,373 | 41.4 ms in 1,343 |
| **ours-sync** | **29.9 ms in 43 lists** | **9.4 ms in 41 lists** |
| the title's indirect commands | 25.4 ms in 3,075 | 25.4 ms in 3,121 |
| **ours-compare** | **15.5 ms in 31 lists** | **4.8 ms in 31 lists** |
| the title's alias copies | 6.9 ms in 79 | 6.9 ms in 78 |
| the rest of the title's commands | 4.3 ms | 4.2 ms |
| all | 124.3 ms | 91.5 ms |

The layer's own two lists were 45.4 ms of the card's time in a frame: more
than the indirect draws after 2.6 and 2.7, and second only to the title's
own compute.

**Why.** The compare's shader hashed a piece with one thread, in 4,096
steps one after the other, each step waiting for the loads of the one
before it: about 0.45 ms a piece on this card, with next to nothing of the
card in use. Passes recorded one after the other run side by side, so a
group of them takes about as long as one piece. But the list that brings
copies up to date has a state transition between one copy's passes and the
next copy's, the card waits there, and so every copy with pieces to hash
costs that time, and every compare list costs it once more. Joining the
passes of a copy (`VICTORY_CARD_REHASH_RUNS=1`) leaves a quarter of them
and changes nothing: 332 hash passes a frame and 4.60 fps, against 1,238
and 4.70 fps, in two trial runs of a later build that differed in that
switch only. The cost is per group, not per pass.

**The change** (`VICTORY_CARD_HASH_LANES=1`, off by default). A piece gets a
group of 64 threads. Thread `t` hashes the piece's 16-byte steps `t`,
`t + 64`, `t + 128`... into a hash of its own, 64 steps deep, and stores or
compares that hash: 64 hashes a piece, a KiB of hashes for 64 KiB of a copy
(13 MB for the 945 MB of copies here). A thread whose hash differs writes 1
to the piece's dword of the results and nothing else is written there, so
the layer zeroes a compare's dwords when it plans it. fxceless/victory-fork
`5557224` gives a piece a group too and folds the 64 hashes into one,
through group-shared memory behind a group barrier; that form lost the
device in two runs of five on RADV in this tree (3.17, the barrier
suspected), so this one has no memory the threads share and no barrier.

A benchmark of one frame's passes with both forms, no title needed
(`cargo test -p victory-gpu --lib indirect::card_hash -- --ignored
--nocapture`, `44f17e3`), the card's time on this RTX 5080:

| a list of | a thread a piece | a group a piece |
|---|---|---|
| nothing | 0.00 ms | 0.00 ms |
| 65 MB sent up in 520 ranges | 1.61 ms | 1.60 ms |
| 520 hash passes of 2 pieces, one after the other | 3.6 ms | 0.40 ms |
| the same passes as 40 copies' (a transition between them) | 17.8 ms | 1.95 ms |
| 80 compare passes of 144 pieces each (737 MB) | 1.92 ms | 0.84 ms |
| 768 MB hashed in one pass | 1.11 ms | 0.89 ms |

The fourth row is the case of the list in front of a submission: 0.45 ms a
group with a thread a piece, 0.05 ms with a group a piece. The last two are
bound by how fast the card reads its memory (about 380 and 880 GB/s).

**Six runs of `7b4ffa4`, the switch off and on in turn**, all with
`VICTORY_INDIRECT_RUNS=1`, fitted against draws a frame as in 2.6:

| | median fps | n | frame at 3,000 draws | wait for the card at 3,000 draws | card busy | removals |
|---|---|---|---|---|---|---|
| `VICTORY_INDIRECT_RUNS=1` | 4.40 / 4.60 / 4.65 | 47 / 48 / 48 | 213 ms | 100.1 ms | 58 / 58 / 57 % | 0 |
| and `VICTORY_CARD_HASH_LANES=1` | **5.30 / 5.25 / 5.30** | 47 / 48 / 47 | **190 ms** | **75.5 ms** | 50 / 50 / 51 % | 0 |

The wait for the card falls by 24.6 ms at 3,000 draws, and the frame by as
much. Both forms compare the same pieces and flag as many: 995 to 1,088
pieces a frame fetched (64.7 to 70.5 MB) with a thread a piece, 913 to
1,039 (59.3 to 67.1 MB) with a group a piece. No device removal, no
unhandled exception, no failed clamp and no compare that could not be made
in the six, and the NVIDIA module logged nothing new.

**Tests.** Each form finds every changed byte in the piece it is in and in
no other, on the host GPU, with a copy that does not start on a piece
boundary, with spans that do and do not start at piece 0, told to keep the
hashes and told to store them
(`indirect::card_hash::card_compare_finds_each_changed_byte_in_both_forms`).
The older tests of the compare need the software adapter, which a Wine
prefix does not give, so they never ran here.

Picture, from 100 captures of each of the six runs (30 spread over the run,
its first 42, and 28 around the car scene): the title cards, the street,
the car, the interior and the video panels are drawn, and both leads have
their face textures from their first shots, with the switch off and on. In
the third run without the switch the man's face is black in the car
interior, a defect seen before these changes that comes and goes from run
to run; it is right in the other five. The woman's top in the shot with
raised hands (2.6) is grey in the first run of each kind and white in the
other four.

**What is left.** At 3,000 draws the frame is 190 ms: 76 ms of waiting for
the card, 70 ms of preparing, about 20 ms of copying back. The card is busy
half of the time. The submitting thread still stops for the card about 20
times a frame, so the two rarely work at the same time.

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

**Rejected, and the premise was wrong.** The map/unmap counters never
disagreed: the run logged 44,000 one-page maps, 5,058 two-page maps and
**52,000 unmaps**, and the earlier reading of "5 unmaps" was my grep cutting
the line off at 95 characters (`unmaps=52` plus the next digits). Both counters
agree, and they say the opposite of 3.24: **a view is created per call, so
widening each view leaves the number of MapViewOfFile/UnmapViewOfFile pairs
exactly where it was.** 44,000 calls still cost 44,000 round trips. The branch
is deleted and the next measurement is reuse distance (3.26), which is the only
thing that says whether calls can be *skipped* rather than made larger.

## 3.26 Reuse distances: 56 % of view maps repeat, but mostly while still live

Instrumented `View::map` with the reuse distance of each `(section, index, n)`
in map order (45,000 maps in a run):

| repeat within | hits | share of all maps |
|---|---|---|
| 16 maps | 18,667 | 41 % |
| 64 | 2,035 | 4.5 % |
| 256 | 537 | 1.2 % |
| 1024 | 359 | 0.8 % |
| 4096 | 703 | 1.6 % |
| 16384 | 1,963 | 4.4 % |
| further | 1,016 | 2.3 % |
| **any repeat** | **25,280** | **56 %** |

So a pool of recently dropped views should have been worth ~41 % of the calls.
It was not: a bounded pool (`VICTORY_VIEW_POOL`) only served 6,000-8,000 of
45,000 maps, because most repeats are of a range that is *still mapped* by
another window (concurrent users), which a pool of dropped views cannot serve.
Measured medians, three runs each:

| pool | median fps (3 runs) | unmaps/run | hits/run |
|---|---|---|---|
| off | 0.80 / 0.60 / 0.90 | 42-44k | - |
| 16 | 0.60 | 34k | 4k |
| 64 | 0.90 / 0.90 / 0.60 | 36-44k | 6-8k |
| 128 | 0.40 / 0.90 / 0.70 | 30-34k | 6k |
| 1024 | 0.90 | 36k | 8k |
| 4096 | 0.70 | 30k | 8k |

Every pool size is inside the +/-40 % noise, and larger pools trend slower -
consistent with 3.23 (a longer live-view list makes each scan dearer). The pool
stays in the tree, off by default, for Windows where the section-view fix (3.27)
does not apply.

## 3.27 One view per section instead of one per page - the 25 % scan is gone

A section is committed whole under Wine (`commit` is a no-op there), so the
whole section can be mapped once and every page addressed as `base + index *
64 KiB`. `VICTORY_SECTION_VIEW` does that (default on under Wine, opt out with
`VICTORY_NO_SECTION_VIEW=1`); a section's view is created on its first use and
unmapped when the section dies. Windows is untouched (SEC_RESERVE sections are
not committed whole there).

| config | median fps (3 runs) | frame | copies | textures | unmaps/run | removals |
|---|---|---|---|---|---|---|
| before (per-page views) | 0.70 / 0.80 / 0.80 | 1,448 / 1,203 / 1,251 ms | 348-564 | 40-68 | 44,000 | 0 |
| **section views** | **1.10 / 1.30 / 1.10** | **937 / 750 / 952 ms** | **122-306** | 34-51 | **0** | 0 |
| section views (first build) | 1.10 / 1.75 / 1.25 | 919 / 585 / 813 ms | 200-294 | 14-44 | 0 | 0 |

Six section-view runs, 53,000-60,000 maps per run served by **one whole-section
mapping**, zero unmaps, zero device removals, real frames (1.85 MB at 1024,
3.9 MB at 512). The copies phase drops with it (348-564 -> 122-306 ms) because a
copy no longer pays a map/unmap around the memcpy. This is the first change that
removes the cost the profile found rather than working around it, and it is the
answer to 3.21/3.22: the churn was `Slice::view()` and the fix was to stop
calling it per page.

Two of the three clean-build runs log `unhandled exception` - that is the
benchmark killing the title at the end of its window (the timestamp is directly
after the last capture, and the same marker appears in runs without this change),
not a gameplay crash; `removals=0` throughout.

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
