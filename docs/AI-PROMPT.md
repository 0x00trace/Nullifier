# Paste this to another AI / contributor

The block below is a self-contained prompt. Paste it into an agent session that
has the repository checked out. It is deliberately narrow: the failure modes in
this project are subtle and a "helpful" rewrites cost days.

---

```
You are contributing to a GPL-3.0-or-later fork of VIctory, a compatibility
layer that runs an Xbox Series title natively on a PC (it is not an emulator:
the title's x86-64 code runs in our process and we implement the console
platform under it).

FIRST: read AGENTS.md at the repository root, completely, before any command.
It contains the build, the measurement protocol, the merge protocol and the
landmines. Then read docs/FORK.md for the current measured state.

Absolute rules
- No game files, assets, dumps or screenshots in the repo. The title is not
  named in the tree. Do not add its name.
- GPL-3.0-or-later. XCL (xcl-dev/xcl, GPL-2.0-only) is licence-incompatible:
  read its docs for ideas, never copy its code.
- Never merge a branch or PR without running the test protocol below.
- Report medians, never peaks, and always report device removals.

Build (Windows target, from Linux)
  export RUSTUP_HOME=~/.local/victory-toolchain/rustup
  export CARGO_HOME=~/.local/victory-toolchain/cargo
  export PATH="$CARGO_HOME/bin:$PATH"
  cargo xwin build --profile diag -p victory --features diagnostics \
    --target x86_64-pc-windows-msvc

Run (hardware-aware)
  ./scripts/victory-launch.sh --detect
  ./scripts/victory-launch.sh --game <game dir>

Measure
  ./tools/bench.sh "<label>"
  Report: median of world frames (fps < 10), n, min/max, device removals,
  whether the picture was correct. n >= 20 world frames or the run does not
  count. Two runs of one build differ by up to 30 %; do not chase 10 %.

Merge protocol (all four must hold)
  1. Builds.
  2. Two bench runs (default + platform settings).
  3. No "device removed", no "unhandled exception", no "no argument clamp",
     world renders (characters, buildings, HUD), median not worse than main.
  4. Row added to docs/FORK.md with commit, GPU, driver, env, median, n.

Known landmines (do not re-learn these)
  - Wine's d3dcompiler_47 rejects RWByteAddressBuffer methods. Without PR #127
    the indirect clamp fails, every indirect draw is dropped and the world is
    black while UI and audio work. New HLSL must use structured-buffer
    indexing. Verify: grep 'no argument clamp' <log> is empty.
  - RADV segfaults in vkCreateGraphicsPipelines on world mesh pipelines when
    driven by vkd3d-proton older than build 5742 (exit 3). The launcher copies
    the newer one from Proton Experimental. Same shader compiles on lavapipe:
    it is a driver crash, not our translation.
  - LATE_WAIT >= 2, JOIN_COPIES=1 and RELAY remove the device (0x887A0005)
    within minutes on Linux. Default LATE_WAIT=1.
  - PR #154's VEH copy collides with upstream's page guards on
    upstream-2026-11-25 (unhandled exception 0x80000001). It belongs on main.
  - main and upstream-2026-11-25 cannot be merged mechanically: 14 files,
    both sides implement view guards and page marking.
  - A local LLM with -ngl 99 takes 11+ GB VRAM and pushes the title into GTT,
    costing ~10x frame time. Stop it before benchmarking; the launcher warns.

Current state
  main = victory-amd + #127 + #270 + #154: ~0.7-0.9 fps median on RX 9070 XT /
  Mesa 26.3.1 / Wine 11, stable, picture correct. The GPU is 4-6 % busy and one
  thread of 103 does all the work: the project is serialisation-bound under
  Wine, not compute-bound. The biggest known win is replacing the per-frame
  GetWriteWatch dirty-page query in victory-gpu/src/watch.rs.

When you are done, post: commit, what changed, the bench row, and what you
verified by eye. If you could not verify something, say so.
```

---

## Running a Discord group on this

- One person owns `main`; everyone else works on branches and opens PRs.
- Every PR description needs a bench row. Reviews check the row, not the diff
  prose.
- Keep `docs/FORK.md`'s table append-only so history is visible.
- Pin the machine matrix (GPU / OS / driver / fps) in the channel so people
  know which configurations actually have evidence.

---

## Windows hardware matrix (from the issue tracker, not measured here)

The launcher picks these for Linux. On Windows, set them by hand:

| GPU | Settings |
|---|---|
| NVIDIA | `VICTORY_RELAY=2`, or `VICTORY_LATE_WAIT=3 VICTORY_WIDE_BACK=1` |
| AMD (RDNA4, driver compiler crashes) | `VICTORY_SHADER_OPT=59`, then `VICTORY_SKIP_PIPELINES` with the hashes from issue #3 |
| 8–12 GB VRAM | `VICTORY_CARD_BUFFERS_MB=1024` at most; 4096 corrupts the picture |
