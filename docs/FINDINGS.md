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
| `unhandled exception 0x80000001` | guard-page violation; seen when delaying guard resets, and when PR #154's VEH copy meets newer upstream page guards | `unhandled exception 0x80000001 at nullifier.exe+…` | not on `reference/upstream`, no #154 there; see §3 |
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

**On the RX 9070 XT / Mesa 26.3.1 here** (the same switch, two runs of 15 s of
world against the shipped profile):

| | median fps | p10 | frame | GPU time | removals |
|---|---|---|---|---|---|
| profile (window + runs on) | 4.45 | 3.9 | 226 ms | ~88-110 ms | 0 |
| **+ `VICTORY_CARD_HASH_LANES=1`** | **6.60 / 6.70** | 4.5 / 4.6 | **151 / 150 ms** | **31.7 ms** | 0 |

The GPU phase fell by two thirds and the frame by a third; copies 41 ms,
textures 15, copy-back 22, unaccounted 8. Real frames (3.7 MB), no device
removal and the same three untranslated pixel shaders as before. Enabled for
the Linux AMD and NVIDIA profiles.

## 2.9 Where the submitting thread stops for the card, and `VICTORY_LATE_WAIT=3` on NVIDIA under Linux (2026-11-26)

Same machine as 2.6 to 2.8. Build `6f3375d` (`main`, version 0.003), with the
`linux-nvidia` profile as shipped there (`VICTORY_CARD_HASH_LANES=1`,
`VICTORY_LATE_WAIT=1`).

**The question was why about 62 submissions a frame are each waited for.**
They are not. With `VICTORY_LATE_WAIT=1` a submission is sent and the thread
goes on; the submissions in the air are settled together at the points where
the title or the layer can see what they did (`late` in `host.rs` lists
them). The frame line counts those points. Per frame, medians of each run's
world lines:

| settled for | profile | and `VICTORY_INDIRECT_RUNS=1` | and `VICTORY_LATE_WAIT=3` |
|---|---|---|---|
| a fill of upload memory read title memory that a flight's shaders may write | 13.0 to 14.2 | 14.0 to 14.2 | 13.6 to 14.5 |
| the title changed memory under a flight's shader writes | 4.5 to 4.6 | 4.4 to 4.6 | 0 |
| a present (or another submission that is waited for) | 1.0 | 1.0 | 0.2 |
| 128 lists in the air | 0 | 0 | 1.0 |
| all | 19.8 to 20.5 | 20.4 to 20.6 | 14.9 to 15.7 |
| time waited for the card, a frame | 114 to 117 ms | 77 to 78 ms | 58 to 59 ms |

Most of the waiting was at the second kind: four or five settles a frame with 84 to
86 ms of the card's work still to run (53 to 55 ms with runs on). Stage 2 of
the switch puts the title's changed bytes into the copy beside the flights'
writes and does not settle; its log line says it looked at about eight pages
a frame and found no changed run in any of them, in every line of the three
runs. Stage 3 also sends the present as a flight. The fills still settle, but
by then little of the card's work is left to wait for (17 to 20 ms a frame
for 14 settles), and the one settle a frame at 128 lists in the air waits 28
to 29 ms.

**Nine runs of `6f3375d`, three settings in turn**, fitted against draws a
frame as in 2.6:

| | median fps | n | frame at 3,000 draws | wait for the card at 3,000 draws | card busy | removals |
|---|---|---|---|---|---|---|
| `linux-nvidia` as shipped | 4.30 / 4.30 / 4.30 | 47 / 47 / 47 | 226 ms | 110.3 ms | 57 / 59 / 60 % | 0 |
| and `VICTORY_INDIRECT_RUNS=1` | 5.20 / 5.20 / 5.20 | 47 / 47 / 46 | 189 ms | 75.7 ms | 51 / 51 / 51 % | 0 |
| and `VICTORY_LATE_WAIT=3` | **6.20 / 5.85 / 5.75** | 48 / 46 / 46 | **169 ms** | **57.5 ms** | 58 / 57 / 58 % | 0 |

No device removal, no unhandled exception and no failed clamp in the nine,
and the NVIDIA module logged nothing new. A tenth run, 20 minutes long, with
the profile chosen by the launcher itself, gave 6.30 fps (n=208) and no
removal; after the opening it stands in the first scene the player controls,
with no input sent. Six more runs of 300 s with stage 3 followed (below), one
of them served by Proton's own wineserver, and none lost the device. After
`main` had moved to `b91fd2e` (lint changes and a keyboard pad in `crates/`),
three runs with the profile chosen by the launcher gave 5.60 / 5.70 / 5.90
fps (n = 47 each, 175 ms at 3,000 draws), again with no removal.

**`VICTORY_LATE_WAIT=2` and `3` were not safe on Linux** where they were
tried before: on an RX 9070 XT under RADV they remove the device within
minutes (section 4 and `launcher/profiles.json`). That was with Proton's own
wineserver, at under 1 fps. Here, on NVIDIA, stage 3 ran for 300 s nine
times on `6f3375d`, three times on `b91fd2e` and nine times on the builds
before them that night, and for 20 minutes once, without a removal. What
differs between the two cases is not known, so the value goes into a profile
for this kind of machine only
(`linux-nvidia-16gb`), and `VICTORY_LATE_WAIT=1` is the first thing to try if
a session there loses the device.

**Tried and left out: fills that do not wait.** A port of the fills-ahead of
fxceless/victory-fork `f85101b` (a fill of upload memory reads title memory
without settling the flights; what the flights then write is compared with
what it read) takes the first row of the table to 0 and the wait for the
card to about 26 ms a frame, with no fill ever found to have read bytes a
flight then changed. The frame rate did not follow: 5.40, 5.40 and 5.45 fps
with it against 5.40 and 5.60 without (single runs of work-in-progress
builds, with stage 3, 512 lists in the air and the hash lanes off). With
fewer waits inside its submissions the title waits between them instead
(below). The port is not in this tree.

**What holds the frame when the card is not waited for** (`perf`, one run of
a work-in-progress build with the runs, stage 3, the hash lanes and the
fills that do not wait: 6.40 fps without captures; a 12 s window of
scheduler events, 30 s of samples, 20 s of system calls):

* The thread that submits runs 41 % of the time and waits for the card 5 %.
  It waits for the title's main thread 42 %. Of its samples, 41 % are in the
  layer (`victory_try_copy_op` 10 %, BLAKE3 2 %, `watch::writes` 2 %) and
  59 % outside it, in Wine's `ntdll.so`, libc, the driver and the kernel. It
  makes 68,000 `rt_sigprocmask` calls a second and 7,700 `mprotect`. My
  reading, not checked in Wine's source: Wine blocks signals around each of
  its virtual-memory calls, so those are about 34,000 such calls a second,
  5,300 a frame (`NtGetWriteWatch`, `get_basic_memory_info` and
  `NtProtectVirtualMemory` are in its samples). Reading the clock is about
  7 % of its samples.
* The title's main thread runs 56 % of the time, and 77 % of its samples
  are in Wine and the kernel: it calls `SwitchToThread` 12,500 times a
  second, which under Wine is `sched_yield` between two `getrusage` calls.
  What it waits for in that loop was not found. It also sleeps 29 % of the
  time waiting for its five recording threads.
* The five recording threads run 20 % each. Of one such thread's samples,
  46 % are in the layer, and 7.7 % of all of them in a contended mutex of
  the layer's (which one was not found).

So taking the remaining waits for the card away does not shorten the frame:
the card is no longer what it waits for most. The next gains are on the
processor's side of the layer: fewer virtual-memory calls in the submit
path, the lock the recording threads
share, and what the title's main thread spins for.

**The scene after the opening, and what was tried there.** The 20-minute run
passed the opening, and the title saved its progress. Every run after that
started from the save, in a scene that stands still: about 4,850 draws a
frame, and frame-rate lines within a few tenths of each other. The bench rows
of this tree are from the opening, so that save has to be moved aside before
a measured set (`AGENTS.md` §6); it was, after the runs below. One run each
in that scene, medians of the last 30 lines:

| | fps | removals |
|---|---|---|
| the profile | 6.2 to 6.3 | 0 |
| and `VICTORY_LATE_WAIT_LISTS=512` | 5.4 | 0 |
| and `VICTORY_RELAY=1` | 6.6 to 6.7 | 0 |
| and `VICTORY_RELAY=2` | 6.8 | 0 |
| the profile, served by Proton's own wineserver | 3.8 over the run, with lines down to 0.2 | 0 |

These are single runs. The relay looks worth a measured set, and more lists
in the air does not. The profile sets neither.

Picture, from 100 captures of each of the nine runs (30 spread over the run,
its first 42, and 28 around the car scene): the title cards, the street, the
car, the interior and the video panels are drawn, and both leads have their
face textures from their first shots, in all nine. The man's face is black
in the car interior (the defect of 2.8) in the first run with runs on, for
the first shots of that scene, and in the second run with stage 3, for the
whole scene; it is right in the other seven, and in the three runs on
`b91fd2e`. With the six runs of 2.8 that is 3 runs of 18 where it shows:
3 of the 15 with `VICTORY_INDIRECT_RUNS=1`, and none of the 3 without. The
defect was seen before that switch existed, and three runs without it are
too few to say that the switch makes it more frequent; it is the first thing
to look at if it does.

## 2.10 Where a frame goes, thread by thread: the thread that submits is never idle (2026-11-26)

Same machine as 2.6 to 2.9, with the built wineserver. Build `ecd008e`
(`main` at `b91fd2e` and the `linux-nvidia-16gb` profile, which the launcher
chose), a diagnostics build. Two runs of the opening: one with `perf`
(5.70 fps, n=47, no removal), one timing run (`VICTORY_GPU_TIMING=1
VICTORY_D3D12_DRED=1`, 3.70 fps in that mode, n=47, no removal).

**How it was taken.** One `perf record -a -g` session of 10 s from second
100 of the run (3,000 to 4,500 draws a frame, 49 frames) held processor
samples, every scheduler switch and wake-up, and a mark at every present: a
uprobe on `thunk64_vkQueuePresentKHR` in Proton's `winevulkan.so`, which
keeps its symbols. With the marks a frame can be cut out of the scheduler
data, and with the wake-ups it can be said for whom a thread slept. A second
window had a uprobe on Wine's `__wine_syscall_dispatcher` with the call
number, which counts every Wine system call by thread and, from the frame
pointer chain, by the layer function that made it. A third took samples
with the registers and 16 KiB of stack each (`--call-graph dwarf`); the
Windows side of those stacks unwinds with the unwind tables of the PE
images (the layer's, the title's, Wine's), which `perf` itself cannot read.
The frames under the first window took 191 to 197 ms, the frame lines
beside it 175 to 232 ms; the window with the dispatcher probe slows the
frame to about 310 ms, so of its counts only those that follow the work are
used.

**The frame in three parts** (medians of the 49 frames, in ms):

| | from a present to the first list call | recording | from the end of recording to the present |
|---|---|---|---|
| length | 74 | 65 | 52 |
| the thread that submits: running | 39 | 39 | 30 |
| waiting for the card | 29 | 8 | 21 |
| other waits | 3 | 14 | 1 |
| the title's main render thread | spins 67 | asleep 64 | asleep 51 |
| the five recording threads, in sum | 1 | run 161 | 0 |
| host queue submits | 22 | 97 | 26 |

* **The thread that submits never waits for work.** Per frame it runs 109
  to 114 ms, sleeps 58 to 62 ms woken by `vkd3d_fence` (the card: about 40
  sleeps, 53 ms of it in sleeps over 2 ms) and about 17 ms in short sleeps
  (locks it shares with the recording threads, Wine's server). No sleep of
  it ends with a wake-up from the title's main thread that hands it work.
  It is what the frame's length is made of.
* **The title's main render thread does about 10 ms of work a frame.** After
  each present it loops on `Sleep(0)` for a median of 67 ms (image offset
  `+0x4fb0c03` to `+0x4fb0c27`; under Proton each call is two `getrusage`
  and a `sched_yield`, and 30 ms of the 73 ms it runs are in Wine's
  dispatcher). In most frames the loop ends 10 to 15 ms after the first long
  wait for the card of the thread that submits. My reading, from the timing
  alone: the title waits for the card to finish the last frame before it
  records the next.
* **The five recording threads run 169 ms a frame in sum, 10 ms of it title
  code.** By the unwound stacks 100 ms are under `draw::State::bind` (78
  under `tables`: making the slots of tables made afresh 25, table entries
  21, `readable` 12), nearly all of it from `execute_indirect` (75) and
  `dispatch` (21). By kind: mutex lock and unlock 40 ms and the waits in the
  kernel 14, shared reference counts and other atomics 23, the NVIDIA driver
  16. Each of the five sleeps about 14 ms a frame woken by another of them.
  The locks they spin for, by caller: the slots' kept resources 4.7 ms, the
  slots' records (`mirror`) 1.8, the buffer table 1.8, the views table 1.7.

**The processor time of the thread that submits** (113.9 ms a frame, by
samples):

| | ms a frame |
|---|---|
| changes of page protection (`VirtualProtect`: `mprotect` in the kernel 16.6) | 19.8 |
| copying title memory (`victory_try_copy_op`, `copy_mapped`, their page faults) | 19.5 |
| `BTreeMap` and `BTreeSet` lookups | 6.0 |
| `memcpy`, `memset`, `memcmp` in Wine's DLLs | 5.9 |
| BLAKE3 | 5.2 |
| the NVIDIA driver | 4.9 |
| `GetWriteWatch` | 4.4 |
| entering and leaving Wine system calls, not attributed to a call | 4.2 |
| mutex lock and unlock | 4.2 |
| reading the clock | 3.4 |
| the rest, each under 3.5 | 36.4 |

Its Wine system calls a frame, with the layer function that makes them:
140,000 `NtQueryPerformanceCounter` from `Instant::now` (62,000 more on
each recording thread: `QueryPerformanceCounter` is a system call under
Wine here, and the layer's accounting reads the clock around nearly
everything it does); 10,600 `NtQueryVirtualMemory` from
`watch::uncommitted_end` (a question for every run of blocks noted
uncommitted, at every asking); 7,300 `NtGetWriteWatch`; about 11,000
`NtProtectVirtualMemory`, of which about 3,000 reach `mprotect` (5.8 µs
each: the kernel flushes the page tables of every processor that runs a
thread of the process, and the main thread spins on one). The title's main
thread makes 61,000 `NtDelayExecution` a frame in its loop.

**The card** (timing run, medians of 47 world lines at 3,331 draws): 97.4 ms
of commands a frame.

| | ms a frame | commands |
|---|---|---|
| dispatches (the title's compute) | 42.8 | 1,355 |
| indirect draws | 27.5 | 3,054 (9.0 µs each; 25 before 2.6 and 2.7) |
| the layer's own lists (2.8) | 15.2 | 73 |
| alias copies | 7.4 | 78 |
| all other draws and copies | 4.5 | |

Two single commands take about 10.5 ms each: a dispatch of compute program
`5cf00dc39042edaf` (the shadow masks) and an indirect dispatch of
`982f3cccd5c04294` (a ray-march of the scattering); the next is
`cb6adcc6da2613ff` (screen probes, 3.7 ms). `nullifier translate
<shaders> --list` with `VICTORY_TRANSLATE_ONLY=<hash>` names a program
and makes its pipeline without the title. A timestamp can be taken while the dispatch
before it still runs (2.8), so the split by command is approximate and the
sum is the better number. `nvidia-smi` shows the card 58 % busy in the
window above and 77 % in a later scene of 1,100 to 1,600 draws, where the
thread that submits waits 80 ms a frame for it and runs 70.

**What follows.** Three amounts each take several times the 33 ms of 30
fps: the processor work of the thread that submits (about 110 ms), the
card's work (about 97 ms), and recording (65 ms). The first two run mostly
in turn (114 ms running and 62 ms waiting for the card make the frame), so
a millisecond less of the submitting thread's work is a millisecond less of
the frame until the card's 97 ms are the limit. A millisecond less of
recording is not: a build in which the recording threads spend 105 ms a
frame in list calls, where they spent 170, gives the same frame rate. No
single item is larger than 20 ms.

## 2.11 The accounting's clock reads were system calls: a stopwatch on the time-stamp counter (2026-11-26)

Same machine and scene as 2.10. The diagnostics build reads the clock around
nearly everything it does, and under Wine here each read
(`QueryPerformanceCounter`) is a system call: 140,000 a frame on the thread
that submits and 62,000 on each recording thread (2.10).

**The change.** `crate::clock::Stamp` takes the place of `std::time::Instant`
for those reads. It reads the processor's time-stamp counter, with a rate
measured once against the system's clock, where the processor says the
counter runs at a constant rate; elsewhere, and with
`VICTORY_STOPWATCH=system`, it reads the system's clock as before. The
frame-rate cap and the coarse clock keep the system's clock.

**Six runs in turn** (200 s each, one build of this change):

| | median fps | n | frame at 3,000 draws | the submitting thread in submissions, user time | removals |
|---|---|---|---|---|---|
| `VICTORY_STOPWATCH=system` | 5.10 / 5.00 / 4.90 | 29 / 29 / 29 | 184.5 / 188.0 / 189.5 ms | 57.0 / 57.2 / 58.1 ms | 0 |
| the counter (the default) | 5.40 / 5.00 / 5.30 | 29 / 28 / 28 | 182.5 / 182.4 / 183.6 ms | 49.3 / 53.7 / 52.3 ms | 0 |

About 5 ms a frame of the submitting thread's time, and 2 to 6 ms of the
frame. The frame-rate medians of the two rows are within the spread of runs
of one setting.

## 2.12 What shaders wrote goes back through a view of our own (2026-11-26)

Ported by hand from fxceless/victory-fork `3973179` (`VICTORY_BACK_OWN_VIEW=1`,
off by default).

**What it was.** What a submission's shaders wrote is put back into title
memory at the title's own address. For view memory the write opens the guard
on the 64 KiB block, and on every other view of the page. The block then
counts as written by the title, so the host copy that has just put the bytes
there copies all 64 KiB of it in again at its next refresh, guards it again,
and sends it up to the card again. The memory line counted 3,586 "views
opened by a write through another": 3,720 to 3,790 a frame (the increase
between two memory lines over the frames between them), and nearly all of
them were this.

**The change.** The bytes are stored through a view of the layer's own on the
same section, which no guard covers. The title's views stay read-only (a
write of the title's still faults and is seen), the write-watch journal is
told that the page was written, so that every other copy of it takes it
again, and the copy that wrote it notes the page current.

**Six runs in turn** (200 s each, one build of this change):

| | median fps | n | frame at 3,000 draws | copies, a frame | sent up to the card, a frame | copying back, a frame | views opened by a write through another, a frame | kernel time of the submitting thread in submissions | removals |
|---|---|---|---|---|---|---|---|---|---|
| off | 4.70 / 5.10 / 4.80 | 28 / 29 / 29 | 191.4 / 182.7 / 192.5 ms | 42.3 / 41.9 / 43.6 ms | 192 / 184 / 184 MB | 26.7 / 25.2 / 27.9 ms | 3,789 / 3,724 / 3,739 | 17.6 / 15.7 / 17.9 ms | 0 |
| `VICTORY_BACK_OWN_VIEW=1` | 5.50 / 5.60 / 4.90 | 29 / 28 / 29 | 177.4 / 173.7 / 185.1 ms | 32.0 / 30.6 / 33.9 ms | 141 / 151 / 151 MB | 19.6 / 19.1 / 19.9 ms | 425 / 416 / 444 | 9.7 / 9.8 / 10.0 ms | 0 |

A run with `VICTORY_BACK_OWN_CHECK=1` compared each
store with what the title's address then showed: 540,672 stretches, 24,205
MB, no store that the title's address did not show, and no page noted
current whose copy differed from title memory. In the same run 475,866
stretches went the old way: what is put back into the title's private memory
(its heaps) is not in a section, and is still copied in again (2.16 counts
it).

## 2.13 The recording threads' locks: 170 ms of list calls became 105, and the frame rate did not move (2026-11-26)

A draw brings the descriptor slots of its tables up to date, and the five
threads the title records with each took the same locks for every slot. The
profile (2.10) showed each of them asleep for about 14 ms a frame, woken by
another of them, and a third of their processor time in mutexes and shared
counters.

**The change** (`1dc0143`): a slot keeps the resource it views in
an atomic pointer; a slot made from the record it already holds is answered
from an atomic word, and only a slot that changes takes a lock; the views
table, the buffer tables and a texture's host side are read under read-write
locks; a lookup of a texture's host side clones nothing unless the caller
keeps it; what is retired while a submission runs goes to a list per thread;
the use stamp of host copies reads the tick count.

**Runs in turn** (300 s each; a third run with the change is void: a
controller button was pressed and the opening was skipped):

| | median fps | n | frame at 3,000 draws | time in list calls, all recording threads | the process, user time a frame |
|---|---|---|---|---|---|
| before | 5.80 / 5.60 / 5.70 | 47 / 47 / 47 | 177.3 / 175.1 / 177.7 ms | 171.1 / 169.5 / 169.5 ms | 365 / 372 / 374 ms |
| after | 5.60 / 5.80 | 47 / 47 | 172.7 / 174.6 ms | 105.8 / 102.8 ms | 340 / 330 ms |

The frame rate is the same within the runs' spread: recording is not what the
frame waits for on this machine (2.10). The change frees about a third of a
processor.

**Later: reverted (`dede94c`), because of the picture.** With this change the
man's top is drawn grey with dark blocks from the close shot with raised
hands to the end of the run, in about one run in eight; it is black
otherwise. `main` @ `25f6ae1` and the same with this change reverted, run in
turn:

| | runs with the grey top, with the change | with it reverted |
|---|---|---|
| 8 rounds of 110 s runs at 2560x1440 output (2026-11-26, from 23:50) | 0 of 8 | 0 of 8 |
| 30 rounds of 110 s runs at 2560x1440 output (2026-11-27, from 00:50) | 4 of 30 | 0 of 30 |
| 3 rounds of 200 s runs at 3840x2160 (the rows in `docs/FORK.md`) | 1 of 3 | 0 of 3 |
| sum | 5 of 41 | 0 of 41 |

A 110 s run ends after the shot. The runs were found with a measure of the
area of his top over every capture of the shot (the share of it that is
neutral grey of middle brightness: 4.5 to 8.6 % in the five runs, 2.1 % or
less in the others), and eight captures of the shot were looked at for each
run of the two later sets. If the change made no difference, five such runs
would all be on its side one time in 36. Over every run of 2026-11-25 to
2026-11-27 whose captures were kept and in which the shot was captured, it
is 18 of the 179 runs of builds with the change and none of the 127 runs of
builds without it.

The frame rate is the same with and without the change, as it was when the
change was made (the three rounds of 200 s runs):

| | median fps | n | frame at 3,000 draws | time in list calls, all recording threads | the process, user time a frame |
|---|---|---|---|---|---|
| with the change | 5.55 / 5.50 / 5.50 | 28 / 28 / 27 | 167.4 / 166.7 / 169.1 ms | 108.9 / 109.5 / 108.4 ms | 339 / 340 / 355 ms |
| reverted | 5.50 / 5.50 / 5.65 | 28 / 28 / 28 | 166.5 / 166.0 / 166.7 ms | 182.4 / 181.2 / 183.7 ms | 379 / 381 / 386 ms |

So the revert costs about a quarter of a processor and no frame rate.

**Not known:** what in the change causes the defect. It looks like a texture
of his top that keeps an early state. The change has six parts (the list
above), and none was tried alone. To apply a part again, run it in turn with
`main` as above and look at the shot in every run: the defect occurs in
bursts (three of the four runs of the long set within five rounds), and the
first set of eight rounds had none on either side.

## 2.14 Tracking the title's writes: other views stay read-only, block sets as bits, refused askings not repeated (2026-11-26)

After 2.12 the thread that submits still spent about 19 ms a frame in the
guards (the frame line's "guards") and 24 ms in all in the questions about
written pages ("watches"). Three changes:

- **`VICTORY_ALIAS_LAZY=1`** (off by default). The views of a guarded page are
  guarded together, and a write through one of them opened them all: a change
  of protection for each view, and another for each when the page was next
  taken. Only the view that is written through has to be open. With the
  switch the other views stay read-only and are noted not clean; the next
  check at such a view reports the block written, as before, and makes the
  page's open views read-only again before it notes the block clean, so that
  a later write through any view still faults and is noted at every view.
- **The guards' sets of blocks are bit sets** (65,536 blocks to a chunk). A
  frame asks about some 13 GB of view memory, 200,000 blocks, nearly all
  clean; as ordered sets those questions were 6 ms a frame in lookups (2.10).
  A range that is all clean is now a count over a few words.
- **A range the kernel refused a write-watch asking for is not asked about
  again for 4,096 askings**, and a range whose first block the guards hold is
  not asked about at all. A submission's ranges are asked about together, and
  by halves where the kernel refuses; a view among them was refused at every
  level, in every submission: about 4,400 of a frame's 7,300
  `GetWriteWatch` calls (2.10).

The census for the memory line also worked out the console layout of every
title texture without a host copy, on the thread that submits, with every
memory line (about 90 ms each, 3 ms a frame in the profile); it now works a
layout out once per description.

**Every step in turn.** Three rounds of six configurations, 200 s a run, with
captures, each row adding to the one above (the wineserver of 2.5; no run
lost the device, and none logged an unhandled exception or a failed clamp;
three more runs, made between the first round and the second, are not
counted: another program started the title on the machine during them):

| | median fps | n | frame at 3,000 draws | the submitting thread in submissions, user + kernel | guards | list calls, all recording threads | views opened by a write through another, a frame | removals |
|---|---|---|---|---|---|---|---|---|
| `main` @ `a0cb06a` | 5.05 / 5.20 / 5.25 | 28 / 29 / 30 | 179.7 / 188.9 / 186.0 ms | 59.3 + 16.4 / 61.5 + 17.9 / 58.2 + 16.4 ms | 25.4 / 27.5 / 26.1 ms | 192.6 / 194.6 / 190.4 ms | 3867 / 3540 / 3780 | 0 |
| + the stopwatch (`8b1b582`, 2.11) | 5.10 / 4.90 / 4.85 | 28 / 28 / 28 | 182.3 / 184.1 / 182.2 ms | 52.3 + 16.5 / 51.5 + 17.0 / 50.7 + 15.8 ms | 26.1 / 25.9 / 25.1 ms | 190.8 / 191.4 / 191.3 ms | 3795 / 3819 / 3819 | 0 |
| + `VICTORY_BACK_OWN_VIEW=1` (`fdd5d3f`, 2.12) | 5.50 / 5.40 / 5.40 | 29 / 27 / 29 | 172.6 / 171.0 / 174.5 ms | 48.3 + 10.3 / 48.1 + 10.6 / 47.2 + 10.3 ms | 19.2 / 19.4 / 18.0 ms | 183.8 / 190.8 / 188.1 ms | 413 / 427 / 443 | 0 |
| + the recording threads' locks (`1dc0143`, 2.13) | 5.30 / 5.60 / 5.40 | 29 / 29 / 29 | 178.4 / 173.8 / 170.6 ms | 45.8 + 10.6 / 46.0 + 9.8 / 45.4 + 10.3 ms | 19.9 / 18.8 / 19.5 ms | 109.5 / 109.4 / 107.3 ms | 415 / 409 / 396 | 0 |
| + block sets as bits, refused askings, the census (`ed2c469`) | 5.50 / 5.40 / 5.65 | 27 / 26 / 28 | 167.7 / 166.1 / 164.5 ms | 39.7 + 9.0 / 39.5 + 9.1 / 40.0 + 9.4 ms | 10.3 / 10.8 / 10.9 ms | 106.4 / 110.8 / 107.6 ms | 441 / 432 / 442 | 0 |
| + `VICTORY_ALIAS_LAZY=1` | 5.60 / 5.70 / 5.55 | 28 / 28 / 28 | 167.7 / 165.9 / 163.5 ms | 39.2 + 7.7 / 40.8 + 8.2 / 39.4 + 7.8 ms | 9.4 / 9.1 / 9.4 ms | 106.0 / 108.2 / 105.4 ms | 187 / 177 / 189 | 0 |

The guards take 10.3 to 10.9 ms a frame with the bit sets against 18.8 to 19.9
before them, and 9.1 to 9.4 with `VICTORY_ALIAS_LAZY=1`, which also takes the
views opened by a write through another from between 432 and 442 a frame to
between 177 and 189.
From `main` to the last row the thread that submits runs 29 ms less a
frame in submissions (59.7 + 16.9 ms of user and kernel time, then 39.8 + 7.9),
and the frame at 3,000 draws is 19 ms shorter (184.9 against 165.7 ms,
means of three).

**Picture.** For each of the 18 runs, a sheet of 20 captures spread over the
world and a sheet of 24 captures of the shots where a face fills the picture:
the title cards, the street, the car, its interior and the credits are drawn
in every run. A black face in some shots of some runs is on record for
builds of `main` (docs/FORK.md, rows of `a0d014d` and `6f3375d`). To count it,
the brightness of the face areas was measured over every capture of two
shots, both faces in the close shot with raised hands and the man's in his
close shot in the car. Runs with a black face, of three each: `main` 0, the stopwatch 1, own view 1, the locks 2, bit sets and refused askings 0, alias lazy 0.
The last two rows are the build that holds every change, and the three
configurations of 2.17 are that build too: 1 of their 15 runs has a black
face (seen on a sheet, in a shot the measure does not cover), and 4 of the 12
runs of the rows above them. Single captures of one
shot differ in how complete the characters' textures are in every row (the
woman's top white or grey, grey patches on the man's top).

## 2.15 Sending more ahead makes the title present in pairs, and two ways to hand shader writes back sooner did not help (2026-11-26)

**A frame rate does not say how evenly the frames come.** The frame-rate
lines now give the shortest and the longest time between two presents in
each second (a frame that was read back first, to be saved, is left out).
Medians of the world lines of 200 s runs, each row adding to the one above
(two runs a row with captures, in turn, then one run a row without them):

| | median fps | presents, shortest to longest gap | time waited for the card in settles |
|---|---|---|---|
| the profile | 5.00 / 5.20 | 140 to 193 ms / 143 to 180 ms | 76 ms |
| + `VICTORY_BACK_OWN_VIEW=1 VICTORY_ALIAS_LAZY=1` | 5.80 / 5.00 | 118 to 174 ms / 144 to 188 ms | 78 to 88 ms |
| + fills of upload memory that do not wait for the submissions in the air (a change that is not proposed) | 5.80 / 5.25 | 74 to 215 ms / 95 to 224 ms | 77 to 90 ms |
| + `VICTORY_LATE_WAIT_LISTS=512` | 5.70 / 5.15 | 24 to 289 ms / 25 to 345 ms | 22 to 25 ms |
| + `VICTORY_RELAY=2`, no captures | 6.20 | 24 to 307 ms | 19 ms |
| the profile, no captures | 5.90 | 153 to 201 ms | 73 ms |
| + `VICTORY_BACK_OWN_VIEW=1 VICTORY_ALIAS_LAZY=1`, no captures | 6.20 | 144 to 184 ms | 77 ms |

With fills not waiting, more lists allowed in the air, or the relay, the
median goes up a little and the presents come in pairs: two of them 20 to
45 ms apart, then none for 230 to 440 ms. They are not in the
`linux-nvidia-16gb` profile for that reason. A Windows profile sets
`VICTORY_RELAY=2`; how its presents are spaced was not measured.

**Why pairs.** A profile of the relay configuration, taken as in 2.10, shows
it. The title lets two frames be under way: its main
render thread waits, in the loop of 2.10, until the card has finished the
frame before the last one, and then records. With the relay the thread that
copies back takes a submission when the card has run it, and fetches the
pieces its compare flagged with a list of its own. That list waits on the
queue behind every submission sent since. The title sends a frame's
submissions in one burst of about 60 ms, so the fetch stands behind all of
them: the thread that copies back waited 52 to 86 ms a frame for fetches,
about 8 ms each, and the results of two frames reached the title together.
It then recorded two frames one after the other and waited again. Without
the relay the same happens whenever nothing settles the submissions in the
middle of a frame: they are all settled when the card has run every one of
them.

**The bench's own captures cost a seventh of the frame rate.** With
`VICTORY_CAPTURE_WORLD=1` two frames of every eight are saved, each presented
with a wait for the card, a read-back and a picture file to write. The
profile's median is 5.0 to 5.2 fps with captures and 5.90 without; what a
player sees is the second number. Rows are still compared
with captures, as before, but a configuration that works ahead loses more to
them.

**Tried: the compare hands the bytes down itself** (not proposed). A thread
of the compare whose
hash differs copied its bytes to a twin of the copy in the PC's memory and
stored a stamp; the thread that copies back merged them with no fetch. The
fetch waits went to zero, and the card became the limit: its shader stores
into the PC's memory, 1,210 pieces and 54 MB a frame, cost it more than the
fetches had. In three runs in turn, with captures, the
thread that copies back waited 163 ms a frame for the card where it had
waited 94 ms for fetches, and the median was 4.10 fps (frame 244 ms) against
5.70 (175 ms) with fetches and 4.90 with no relay. It is the same driver
behaviour as 2.7 found for a word that every thread stores to.

**Tried: the compare's list copies down the pieces it is expected to flag**
(not proposed). The pieces that the
same compare of a copy flagged in the last three frames were copied to the
PC's memory by plain copies at the end of the compare's list, 120 to 140 MB
a frame. The expectation covered 87 to 98 % of the 1,150 to 1,550 pieces
flagged a frame; 25 to 200 a frame were not expected, and one of those is
enough for a fetch. Fetch waits stayed at about 100 ms a frame, the presents
stayed in pairs, and the median was 5.45 fps against 6.20.

**Also tried on top of the even configuration, without captures:**
`VICTORY_WIDE_REFRESH=1` with a submission's texture refreshes on several
threads and a guarded texture hashed every 64th frame instead of every 8th
(two changes that are not proposed): 6.10 fps against 6.20, the copies no
shorter (31 ms against 25). `VICTORY_DISPLAY_MODE=2560x1440` changes the
mode reported to the title and not the size it renders at (5.90 fps against
5.80 for the run before it); `VICTORY_OUTPUT` and
`VICTORY_RENDER_SCALE` (docs/LAUNCHER.md) do change it.

**What the numbers of 2.11 to 2.14 come to.** In the table of 2.14 the
thread that submits does 29 ms less work a frame in submissions than at `main`,
and the frame at 3,000 draws is 19 ms shorter. Where that thread now arrives
sooner at a point where the submissions are settled, it waits longer there
for the card: 65 ms a frame in settles at `main`, 79 in the last row. The
frame is a chain of the two, and what shortens it from here is not more of
either alone but letting the card run one frame while the next is prepared,
with the title told of each submission when the card has run it. 2.16 says
what stands in the way, and 2.17 what a smaller rendered size does to the
card's part.

## 2.16 The title does not touch 98 % of what is copied back to it (2026-11-26)

What a submission's shaders wrote to host copies is put back into title
memory after it: 65 MB a frame in the opening (the frame line's "copying
back"), because on the console the card writes the title's memory itself and
the title may read it. Every way of settling submissions, and both attempts
of 2.15, are about getting those bytes to the title in time.

**A probe** (`VICTORY_BACK_PROBE=N`, diagnostics, off by default) asks whether
the title reads them. One in N of the 4 KiB pages that a put-back has just
written whole is made inaccessible (the title's private memory only: the
write guards own the protection of views). The first access to such a page
faults into a handler that opens the page and counts who it was. A copy of
the layer's own opens the page for the length of the copy and closes it
again. A page that is still closed when the next put-back writes it was not
touched in between.

**One run** (N=16, with `VICTORY_BACK_OWN_VIEW=1 VICTORY_ALIAS_LAZY=1`,
no captures; 5.70 fps, n=27, no unhandled fault). The probe's lines give
counts per frame for each second; multiplied by that second's presents and
summed over the 870 frames of the world:

| per frame | |
|---|---|
| pages closed after a put-back | 380 |
| put back again while still closed | 373, after 1.9 frames on average |
| first touched by a read that is not the layer's | 2.7 |
| first touched by a write that is not the layer's | 3.5 |
| first touched by code of the layer's outside its copies | 0 |
| opened for a copy of the layer's own and closed again | 799 |

Of the 330,076 sampled pages whose fate is known, 98.36 % were written again
by the next put-back without having been touched, 0.71 % were first read and
0.92 % first written by code that is not the layer's. Each closed page was
read 2.1 times by the layer itself in between: the put-back marks the page
written, and the copy that wrote it copies it in again (2.12 took that away
for views, not for private memory).

**What follows.** Nearly all of the copy-back, the fetches behind it, the
second copy in and the second send up serve bytes the title does not look
at. Leaving what shaders wrote on the card, with the title's pages closed,
and fetching a piece when the title first touches it would take those away
and with them the reason to settle submissions at all. Not built. What it
needs, as far as this run shows: a fetch from a fault handler on any title
thread; the same for views, whose protection the guards own; the runtime's
system calls on title memory told to it first (a call that is handed a
closed page fails); and the title's own buffer copies, which read title
memory on the processor.

## 2.17 The size the title renders at: 2560x1440 output on an RTX 5080 (2026-11-26)

`0c4c2e3` gave the loader `VICTORY_OUTPUT` and `VICTORY_RENDER_SCALE`
(docs/LAUNCHER.md): the title's output size, and the scale of its own
upscaler. By default the output is 3840x2160 and the scale 2/3, so the title
renders 2560x1440. docs/LAUNCHER.md has the numbers of the machine the switches
were written on. These are from the RTX 5080 of 2.6 to 2.16. Its monitor is
2560x1440, so a 3840x2160 output is scaled down again on the way to it.

**Twelve runs in turn** (three rounds, 200 s a run, with captures, build
`ed2c469` with `VICTORY_BACK_OWN_VIEW=1 VICTORY_ALIAS_LAZY=1`; the first row
is the last row of 2.14's table):

| output | render scale | rendered | median fps (frame lines of 500 draws or more) | n | frame at 3,000 draws | wait for the card at 3,000 draws | copying back, a frame | removals |
|---|---|---|---|---|---|---|---|---|
| 3840x2160 (default) | 2/3 (default) | 2560x1440 | 5.65 / 5.75 / 5.60 | 30 / 30 / 30 | 167.7 / 165.9 / 163.5 ms | 73.8 / 72.5 / 67.5 ms | 18.5 / 19.5 / 19.4 ms, 63 / 64 / 63 MB | 0 |
| 2560x1440 | 1.0 | 2560x1440 | 5.95 / 6.00 / 6.15 | 30 / 30 / 30 | 159.8 / 158.6 / 152.8 ms | 72.4 / 73.3 / 70.7 ms | 19.6 / 19.8 / 19.2 ms, 63 / 64 / 65 MB | 0 |
| 2560x1440 | 2/3 (default) | 1707x960 | 8.20 / 8.10 / 8.00 | 30 / 31 / 31 | 113.9 / 118.4 / 120.6 ms | 42.3 / 41.1 / 44.9 ms | 14.3 / 13.5 / 14.3 ms, 42 / 39 / 41 MB | 0 |
| 2560x1440 | 0.5 | 1280x720 | 8.80 / 8.60 / 8.70 | 31 / 31 / 31 | 109.1 / 108.1 / 99.0 ms | 34.2 / 36.0 / 36.1 ms | 12.5 / 12.3 / 13.2 ms, 32 / 31 / 33 MB | 0 |

A frame line counts here when it has 500 draws or more. `tools/bench.sh`
counts a line as world when it is under 10 fps, which cuts lines off where a
setting reaches 10 fps: its medians for the four rows are 5.60 / 5.70 / 5.55;
5.90 / 5.80 / 5.85; 8.05 / 7.75 / 7.90; and 8.70 / 8.40 / 8.70 (n = 26 / 26 / 27 in the last row).

**What changes with the size.** At the same rendered size the smaller output
shortens the frame at 3,000 draws by 9 ms (157.1 against 165.7 ms); the
wait for the card is 72.1 against 71.3 ms there. With two thirds of the
pixels in each direction (1707x960) the wait for the card at 3,000 draws is
42.8 ms, and what shaders write back is 40 MB a frame where it was 63. At
1280x720 the frame is 105.4 ms at 3,000 draws, 35.4 of them waiting for the
card. Preparing the submissions takes 43.8 ms a frame there and 48.4 in the
first row: the work of the thread that submits (2.10) is what is left, and
it changes little with the size.

One run each on the build of 2.15 and 2.16, which logs the presents (the
first under load from other programs on the card, the second partly):
141 to 193 ms apart by default, 107 to 145 ms at 2560x1440 output, 82 to
117 ms at scale 0.5 there, and 80 to 114 ms at 1920x1080 output (1280x720
rendered, 9.80 fps by draws). The presents stay evenly spaced, as nothing is sent further ahead.

**Picture.** Two sheets of captures for each of the twelve runs, as in 2.14:
the title cards, the street, the car, its interior and the credits are drawn
at every size. On the sheets a face is black in one of the twelve runs (the
woman's, in the last capture of one run at 1280x720 rendered); the measure of
2.14 finds none in its two shots.
Captures are saved at half the output's size (1280x720 for a 2560x1440
output), so they do not show how sharp the output is. A measure of a regular
pattern (the autocorrelation of the picture's gradients, 37 to 39 captures
of one run each) is the same at scale 0.5 as at the default. The opening has
no HUD and no menu in play; docs/LAUNCHER.md reports those wrong at a smaller
output, and they were not looked at here.

## 2.18 The relay's fetches on a copy queue of their own (2026-11-26)

2.15 found what the thread that copies back waits for with the relay: it
fetches the pieces that a flight's compare flagged with a list of its own on
the host's queue, and that list runs behind every submission sent since.
`VICTORY_RELAY_QUEUE=1` (`8c1774b`, off by default, needs `VICTORY_RELAY`)
gives that thread a D3D12 copy queue of its own for the fetches. On this
card under vkd3d-proton the fetches then no longer wait behind the host
queue's submissions: the wait for them goes from about 85 ms a frame to
about 16. How vkd3d-proton maps a copy queue to the card's queues was not
looked up; the wait is what was measured.

**What it changes for the merge.** On the host's queue a fetch ran behind
every send of title bytes that was on the queue before it, and the merge
left out the ranges of later sends only. On its own queue a fetch is behind
the sends ahead of the compares of the flights being copied back (the card
has run those) and may or may not be behind later ones. The merge is told
the newest send ahead of those compares, and leaves out the ranges of every
later send, which is right either way: there the title's bytes are the newer
ones. A fetch may also bring bytes that a later flight's shaders wrote, some
of them while the copy ran; that flight's own compare flags the piece again
and it is fetched again.

**Eighteen runs in turn** (three rounds of six configurations, 200 s a run,
with captures; build `8c1774b`, which is the stack of 2.11 to 2.17 and this
change; `VICTORY_BACK_OWN_VIEW=1 VICTORY_ALIAS_LAZY=1` in every row; the
second, third and fourth row each add to the row above, and the last row
adds those three switches to the fifth):

| | median fps (lines under 10 fps) | n | median fps (lines of 500 draws or more) | frame at 3,000 draws | presents apart | times a frame what shaders wrote is copied back | the thread that copies back waits for fetches, a frame | the submitting thread waits in settles, a frame |
|---|---|---|---|---|---|---|---|---|
| the relay off | 5.40 / 5.70 / 5.50 | 27 / 26 / 28 | 5.50 / 5.90 / 5.55 | 168.1 / 159.7 / 168.9 ms | 122 to 170 / 119 to 170 / 124 to 172 ms | with each settle (17.0 / 16.6 / 16.7) | none: fetched in the settle | 79.5 / 78.5 / 79.2 ms |
| + `VICTORY_RELAY=2` | 5.50 / 5.90 / 5.90 | 28 / 27 / 26 | 5.65 / 6.00 / 6.25 | 165.9 / 159.2 / 149.4 ms | 133 to 172 / 97 to 185 / 95 to 162 ms | 24.0 / 24.1 / 24.5 | 91.5 / 85.4 / 78.5 ms | 97.3 / 90.9 / 86.5 ms |
| + `VICTORY_RELAY_QUEUE=1` | 6.20 / 6.40 / 6.70 | 27 / 27 / 27 | 6.20 / 6.45 / 6.75 | 150.9 / 149.4 / 138.0 ms | 70 to 154 / 70 to 150 / 63 to 140 ms | 52.9 / 52.5 / 50.4 | 13.1 / 15.7 / 17.7 ms | 80.2 / 77.4 / 70.5 ms |
| + `VICTORY_LATE_WAIT_LISTS=512` | 6.30 / 6.30 / 6.75 | 27 / 27 / 28 | 6.40 / 6.35 / 6.80 | 153.5 / 147.8 / 137.9 ms | 69 to 154 / 67 to 148 / 64 to 135 ms | 53.0 / 50.2 / 50.7 | 12.4 / 15.4 / 16.2 ms | 81.6 / 77.1 / 70.7 ms |
| 2560x1440 output: the relay off | 7.70 / 8.10 / 8.15 | 28 / 27 / 26 | 7.80 / 8.20 / 8.40 | 119.0 / 114.3 / 106.6 ms | 93 to 130 / 91 to 122 / 85 to 124 ms | with each settle (16.8 / 17.1 / 16.4) | none: fetched in the settle | 47.3 / 46.0 / 42.4 ms |
| 2560x1440 output: the relay, its queue and 512 lists | 8.60 / 8.40 / 9.45 | 22 / 22 / 16 | 9.20 / 8.70 / 9.85 | 102.6 / 105.0 / 93.3 ms | 58 to 116 / 61 to 117 / 54 to 108 ms | 53.2 / 53.4 / 51.6 | 8.6 / 8.1 / 9.3 ms | 46.6 / 48.8 / 43.6 ms |

With the queue the flights are copied back 52 times a frame where they
were copied back 24 times, the thread that copies back waits 16 ms a frame
for fetches where it waited 85, and the frame at 3,000 draws is 146 ms
against 158 with the relay alone and 166 without it. At 2560x1440 output
the last row is 100 ms against 113. The presents stay evenly spaced: with
the queue the shortest gap of a second is 54 to 70 ms and the longest 108 to
154 (medians of each run), unlike the pairs of 2.15, which came where fills
did not wait. `VICTORY_LATE_WAIT_LISTS=512` on top changes nothing that
these runs can show at 3840x2160 (the third and the fourth row).

**Without the other switches** (twelve more runs in turn, three rounds, the
same build; no `VICTORY_BACK_OWN_VIEW` and no `VICTORY_ALIAS_LAZY`):

| | median fps (lines under 10 fps) | n | median fps (lines of 500 draws or more) | frame at 3,000 draws | presents apart | the thread that copies back waits for fetches, a frame |
|---|---|---|---|---|---|---|
| the profile | 5.55 / 5.50 / 5.75 | 28 / 27 / 28 | 5.60 / 5.55 / 5.80 | 168.2 / 163.7 / 164.7 ms | 128 to 168 / 127 to 171 / 129 to 164 ms | none: fetched in the settle |
| + `VICTORY_RELAY=2 VICTORY_RELAY_QUEUE=1` | 6.50 / 6.40 / 6.30 | 27 / 27 / 28 | 6.50 / 6.55 / 6.30 | 146.7 / 144.6 / 150.2 ms | 78 to 142 / 79 to 148 / 78 to 149 ms | 17.8 / 14.1 / 15.9 ms |
| 2560x1440 output | 7.90 / 7.80 / 8.00 | 26 / 26 / 28 | 8.00 / 7.90 / 8.10 | 114.8 / 116.1 / 121.8 ms | 98 to 128 / 94 to 131 / 95 to 128 ms | none: fetched in the settle |
| 2560x1440 output + `VICTORY_RELAY=2 VICTORY_RELAY_QUEUE=1` | 8.60 / 8.80 / 8.75 | 23 / 23 / 20 | 8.95 / 8.90 / 9.00 | 100.9 / 100.0 / 100.4 ms | 65 to 117 / 67 to 112 / 65 to 114 ms | 8.4 / 8.8 / 8.6 ms |

The queue's part is the same without them: the frame at 3,000 draws is 147 ms
against 166 at 3840x2160 and 100 against 118 at 2560x1440 output. No run lost the
device. In these twelve runs the man's top is grey with dark blocks in three
(two with the profile at 2560x1440 output, one with the queue there), his
face is black in the car in one, and dark patches lie around his eyes in
two; `docs/FORK.md` names the runs.

**On top of the revert of 2.13** (twelve more runs in turn, three rounds,
the merge `5aa7b69`: `main` @ `25f6ae1`, the revert `dede94c` and this
change; no other switch). A browser playing video and a music player ran
beside this set, and its level is about 5 % under the sets before it: the
frame at 3,000 draws with the profile alone is 175 ms here, and 166 ms for
the same code an hour earlier (2.13). So compare inside the set only:

| | median fps (lines under 10 fps) | n | median fps (lines of 500 draws or more) | frame at 3,000 draws | presents apart | the thread that copies back waits for fetches, a frame |
|---|---|---|---|---|---|---|
| the profile | 5.10 / 5.40 / 5.35 | 27 / 28 / 28 | 5.20 / 5.45 / 5.50 | 175.3 / 170.6 / 178.7 ms | 139 to 181 / 133 to 178 / 136 to 178 ms | none: fetched in the settle |
| + `VICTORY_RELAY=2 VICTORY_RELAY_QUEUE=1` | 6.00 / 5.90 / 5.70 | 27 / 27 / 27 | 6.10 / 6.05 / 5.85 | 156.1 / 157.6 / 157.5 ms | 88 to 158 / 97 to 153 / 90 to 157 ms | 16.1 / 17.6 / 16.3 ms |
| 2560x1440 output | 7.40 / 7.40 / 7.45 | 28 / 28 / 28 | 7.60 / 7.60 / 7.60 | 124.0 / 123.3 / 123.8 ms | 104 to 131 / 102 to 135 / 102 to 138 ms | none: fetched in the settle |
| 2560x1440 output + `VICTORY_RELAY=2 VICTORY_RELAY_QUEUE=1` | 8.40 / 8.60 / 8.00 | 27 / 27 / 27 | 8.50 / 8.75 / 8.10 | 80.6 / 108.3 / 114.5 ms | 69 to 119 / 72 to 125 / 72 to 123 ms | 9.2 / 9.4 / 9.5 ms |

The queue's part is the same with 2.13 reverted: the frame at 3,000 draws is
157 ms against 175 at 3840x2160, and the median by draws 8.45 fps against
7.60 at 2560x1440 output (the fit of the last row spreads from 81 to 115 ms,
so its medians are the better number). No run lost the device, and the man's
top is black in all twelve runs.

**Without captures** (one run each, 2560x1440 output, a build that also
holds a change that is not proposed, fills of upload memory that do not
wait; frame lines of 500 draws or more): the relay off 8.40 fps, presents
100 to 135 ms apart; the relay with its queue and 512 lists 10.00 fps, 84 to
120 ms apart; with fills that do not wait 10.90 fps, 23 to 155 ms apart;
with `VICTORY_RENDER_SCALE=0.5` on top 12.40 fps, 56 to 111 ms apart. The
profile's 3840x2160 without captures was 5.90 (2.15).

**What is left.** With the relay, its queue and 512 lists the thread that
submits still waits 76 ms a frame in settles at 3840x2160 (46 at 2560x1440
output): about three quarters of it where a fill of upload memory reads
what a flight's shaders may write (58 and 34 ms), the rest at the presents
that the bench's captures make wait for the card (18 and 11 ms; two
frames of eight). The thread that copies back waits for the card with
nothing to copy back for 78 ms a frame (54): the card is what both wait for.
Fills that do not wait (not proposed) take those settles away and bring
others, for title memory that changed under shader writes, and the pairs of
2.15 with them.

**Picture.** Two sheets of captures for each of the eighteen runs, as in
2.14: the title cards, the street, the car, its interior and the credits
are drawn in every run, and the measure of the face areas of 2.14 finds no
black face. That measure takes a fixed area over the first 70 % of a shot's
captures. A stricter look at the close shot with raised hands, eight
captures of each run cut to the man, finds more: in the fourth row his face
is black in the last capture of the second run and in three captures of the
third, and in the second run his top is grey with dark blocks from that shot
on (it is black in the other seventeen runs); in the fifth row dark patches
lie around his eyes in three captures of the third run. The other fifteen
runs show none of these, the three with the queue alone among them. The
same look at 18 runs of builds from before 2.11 (the sets of 2.8 and 2.9,
and `main` @ `b91fd2e`) shows the dark patches around his eyes in three of
them and the grey top in none.

**Not measured:** Windows, where a profile sets `VICTORY_RELAY=2`; AMD and
RADV, where a copy queue may not run apart from the host's queue; runs
longer than 200 s; scenes after the opening. A copy queue reads the buffers
of copies in the card's memory in whatever state the host's queue left them,
as the relay's fetch on the host's queue does; vkd3d-proton keeps no state
for buffers.

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
stall, and it explains why nothing in Nullifier's own switches moved the frame:
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

## 3.28 Black frames at shot changes are the title's own (2026-11-25)

**Symptom.** One or two wholly black frames at some shot changes of the
opening scene (`a black frame after a frame that was not` in the log, 8 to
26 a run on an RTX 5080 at about one frame a second).

**What the title draws there.** A run with the draws of every world frame
listed (`VICTORY_CAPTURE_WORLD=1` and `VICTORY_CAPTURE_WORLD_ALSO=1,2,…,300`:
285 listings) had four such places. All four are the start of a video the
title lays over the picture, and all four go the same way:

| frame | the video's target (512x512, format 29) | the overlay draw | picture |
|---|---|---|---|
| first | nothing draws into it | samples it | black |
| second | the title fills it with a full-screen triangle of its own (`dfafaf385b2f94e8` + `06295fb1d1aefb78`, blending off) | samples it | black |
| third | the video's planes are converted into it for the first time (`f7f331c8c5b3a0d8` + `3bfe730f7e24ee1a`) | samples it | the video |

The overlay draw is `80cb958cde505e15` + `f9bf13c6c099ccec`, alpha-blended
(source alpha, inverse source alpha) into the 3840x2160 interface layer. No
draw of those frames is dropped. The planes are not late either: no list
samples them in the first two frames, and when they are first read they hold
a picture (luma mean 29 and 46 of 255 at two of the places; logged with
`VICTORY_FILM_LOG=1` on a diagnostics branch). The 3D picture under the
overlay is drawn in those frames, and `VICTORY_TEXTURE_NO_WAIT` changes
nothing.

**So.** The title shows its overlay two frames before the video's first
picture, and it counts frames, not time. At 30 frames a second that is 66 ms;
at one frame a second it is two seconds of black. The layer has nothing to
bring up to date there, and the black gets shorter as the frame does. Do not
look for a late upload here.

**Open.** What the target holds in the first frame (it was never drawn to,
or it holds an earlier video's last picture), and the colour the title's
fill writes in the second: the listing does not show the fill's constants.
A video panel is also drawn green in some frames, which is what planes of
all zeros convert to; in one capture the picture's luma is there under the
green. Not looked into.

## 3.29 The game's settings: plain XML, and a save container behind it (2026-11-26)

The title keeps two stores, and both are readable.

**`settings.xml`** — plain XML with real names, 112 lines, written on every
settings change:
`<prefix>/pfx/drive_c/users/steamuser/AppData/Local/nullifier/data/<title>/storage/settings/settings.xml`
(the title opens it as `Y:\settings\settings.xml` at boot; a `VICTORY_LOG=debug`
run logs the host path). Sections and examples:

| section | settings |
|---|---|
| graphics | `hdr`, `hdrIntensity`, `gamma`, `anisotropicFiltering`, `desiredFPS` (30), `fsrQuality` (2), `graphicsQualityPreset` |
| advancedGraphics | `asyncComputeEnabled`, `damageModelsEnabled`, `directionalScreenSpaceShadowQuality`, `fsrSharpen`, `grassLod`, `lodScale`, `texLodScale_*` (Global/6 materials/SLOD1-4), `maxTexUpgradesPerFrame`, `motionBlur*`, `rtLODLightShadows`, `rtDynamicShadows`, `shadow*`, `ssroSampleCount`, `transferQueuesEnabled`, `treeTessellationEnabled`, `vehicleLodBias`, `volumetricsRaymarchResolutionUnclamped`, `waterReflectionResX/Y`, `waterReflectionSSR`, `worldHeightShadowQuality`, `mapMSAA`, `vrsEnabled`, `manualMipBias` |
| video | `resolutionFullscreen` 3840x2160, `refreshRate` 60/1, `frameRateLimit` 30, `vSync` 2, `tripleBuffered`, `uiResolution`, `adapterIndex`, `outputIndex` |

Two writes to it were read but not applied: `frameRateLimit`/`desiredFPS` 60
(menus stayed 30 fps) and `resolutionFullscreen` 1280x720 (the swap chain still
presented 3840x2160). So the XML is read, but the graphics state that takes
effect comes from the save container, or the console video settings are fixed.

**The save container** — `XGameSave` emulation, JSON with 32-bit hashed keys:
`.../data/<title>/saves/0009000000000001/<xuid>/SettingsProfileSettings~0b4e25fca06268d0/v<N>/0`
(`manifest`, `0`, `1` per version; the title keeps v1, v3, ... and a
`Player~333dc56ddffd8ea0` sibling). After changing three settings in the in-game
Display menu and diffing v1 -> v3:

| path in the JSON | value | setting (from the menu) |
|---|---|---|
| `/$0x8c1c3948/$0x25584db6/$0xcb20b040/$0xcb0db478` | 3 -> 0 | **Motion Blur Strength** |
| `/$0x8c1c3948/$0x25584db6/$0xcb20b040/$0x2e4a3f2c` | 5 -> 0 | Hit Effects Intensity (menu showed 1; unresolved) |
| `/$0x8c1c3948/$0x17d82a47/$0x944fa9d9/$0xc89952df` | 0 -> 1 | **Dismemberment** (0 on, 1 off) |

That is the mapping method: change one setting in the menu, save, diff the
containers. The pause menu is reachable with scripted keys: `Esc` opens it,
`Up` wraps to Settings, `E` (right bumper) moves tabs to DISPLAY. Settings are
committed when the container reaches a new version, not when Back is pressed.

### 3.29b What the LOAD ERROR means, and the ASAV header

The title's own banner — "Some settings changes couldn't be loaded and may have
been reverted. Your saved games haven't been affected." — appeared after an
edited container version was tried. It then wrote a fresh version (v5) itself.

The blob is an **ASAV** container, not bare JSON:

```
offset  size  field
0       4     "ASAV"
4       4     version (2)
8       4     2
12      4     1
16      4     0
20      4     0x170   JSON offset
24      4     0x1cb0  file size
32      4     Unix time of the save (e.g. 0x6b082ed2 = 13:06:26)
36      4     0
40      ...   UTF-16 "Profile Settings", zero padding
```

and blob `1` is its metadata: a count (6), the file size, a **64-bit value that
changes with every save** (`0x19f3e566cebaddbf` at 03:59, `0x603a43fd289d0b92`
at 13:06), the same Unix time, the size again and the name.

Tests: an **identical copy** of the newest version as a new version loaded with
no error; a copy with one value changed was rejected and reverted. So there is a
content or revision check we have not reproduced yet. The 64-bit value is not
xxHash64, FNV-1a-64, MD5/SHA-1/SHA-256/BLAKE2b (first 8 bytes) of the JSON or of
the file, nor CRC32/CRC32C/Adler-32/Jenkins lookup3-32 (also 64 not found). It
may be a revision id generated per save rather than a digest, with the loader
matching it against an index elsewhere (`storage/cloudcache`, the `Player`
container, or the save index).

**The XML is honoured for graphics.** With `graphics/gamma` set from 15 to 1
through the launcher's own writer, a frame at 55 s measures mean luma **4.9**
against **12.7** on the same scene before the change: the picture goes almost
black. So `settings.xml` is the writable store for graphics (and probably the
advanced graphics list); the video section's `frameRateLimit`/`resolution` were
the exceptions tested earlier, not the rule.

Next: (1) test an XML-only advanced setting with a visible effect (e.g.
`lodScale` 0.05) to see whether the XML store is honoured for graphics; (2) find
which file records the current version/revision (rename one at a time and see
whether the identical copy is accepted); (3) only then write the container.

### 3.29a The hash is joaat, and the boot path is not the newest version

The container's section keys are **joaat** (Jenkins one-at-a-time, lowercased)
of the menu category names, which confirms the hash family:

| key | joaat of |
|---|---|
| `0x17d82a47` | `Display` |
| `0x25584db6` | `Camera` |
| `0xb80c6ae8` | `Audio` |
| `0x1b2d32db` | `UI` |
| `0x1723e64b` | `Gameplay` |

The setting keys under them do **not** match simple labels or the XML names
(tested `Motion Blur Strength`, `motionBlurScale`, `hitEffectsIntensity`, with
spaces, underscores, camel case, path prefixes and UTF-16); they are probably
localisation or widget ids, not the visible text.

A copy of v3 as **v4** with one key changed (the Motion Blur key set to 5) was
not loaded: the DISPLAY menu came up at defaults (5/5/5/3/On), while v3 held
1/0/off. So the boot path is not "the newest version directory", and the
settings the menu shows come from another store or another profile (the account
pickers's player). Next: rename or remove each store in turn (settings.xml,
the container version, `storage/cloudcache`, the Player container) and see
which one the menu follows; then map keys by changing several menu values to
distinct numbers per run.

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
  nullifier.exe` finds nothing — match the cmdline instead.
* `VICTORY_KEYS` (diagnostics builds) presses keys at given seconds; it is the
  only way to drive the game headless. `VICTORY_CAPTURE` + `VICTORY_CAPTURE_FRAMES`
  write PNGs of presented frames.
