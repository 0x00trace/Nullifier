# Paste this to another AI / contributor

Self-contained prompt for an agent working on **this** fork. It assumes the
agent has the repository checked out and is helping someone who already has a
fork of their own and wants to bring their work here.

---

```
You are contributing to a UNIVERSAL fork of VIctory: a compatibility layer that
runs an Xbox Series title natively on a PC. It is not an emulator - the title's
x86-64 code runs in our process and we implement the console platform beneath
it. "Universal" means one main branch that runs on AMD and NVIDIA, on Linux and
Windows; the per-platform differences are chosen at launch from
launcher/profiles.json, NOT by keeping separate branches.

BEFORE ANYTHING ELSE - TRIAGE (docs/ONBOARD.md)
  Run the three checks in "Triage" there, then state which path applies:
    A. fresh start      -> ./tools/setup.sh, then bench a baseline
    B. existing work     -> docs/ONBOARD.md section B + docs/MIGRATION.md.
                            Never merge their branch, never delete their tree,
                            port one commit per PR.
    C. findings only     -> open an issue with the template.
  If you cannot tell which one it is, ask the user. Do not guess, do not start
  editing, and do not touch a tree that is not this clone.

STAY IN SYNC (every session, before you do any work)
  ./tools/sync-check.sh
    behind > 0                  -> rebase on origin/main, then re-run
                                   ./tools/bench.sh: an upstream fix changes what
                                   your change is worth, old numbers expire
    launcher/profiles.json changed -> re-run ./launcher/victory-launch.sh --detect
    AGENTS.md / docs/AI-PROMPT.md / docs/ONBOARD.md changed -> re-read them,
                                   the rules may have changed
  Never keep working on a stale clone for days, and never open a PR from one:
  state the commit you benched.

READ FIRST, COMPLETELY, BEFORE RUNNING ANY COMMAND
  0. docs/ONBOARD.md        - triage, and the path for your situation
  1. AGENTS.md              - rules, build, measurement, merge protocol, landmines
  2. docs/LAUNCHER.md       - how settings are chosen and how to add a profile
  3. docs/MIGRATION.md      - how to port work from another fork, PR format
  4. docs/FORK.md           - what has actually been measured
Then run: ./launcher/victory-launch.sh --detect   (or the .ps1 on Windows)

ABSOLUTE RULES
  - No game files, assets, dumps or screenshots in the repo. The title is not
    named anywhere in the tree. Never add its name.
  - Licence: GPL-3.0-or-later. XCL (xcl-dev/xcl) is GPL-2.0-only and
    INCOMPATIBLE: read its docs for ideas, never copy or port its code.
  - One idea per branch and per PR.
  - Vendor-specific fixes must be runtime-gated (a capability check or a
    profile value), never an assumption that breaks the other vendor.
  - Never merge upstream/foreign branches into main. Port commits one at a
    time (docs/MIGRATION.md).
  - Performance claims need ./tools/bench.sh medians. Peaks and "feels
    faster" are rejected.

BUILD (Windows target, from Linux)
  export RUSTUP_HOME=~/.local/victory-toolchain/rustup
  export CARGO_HOME=~/.local/victory-toolchain/cargo
  export PATH="$CARGO_HOME/bin:$PATH"
  cargo xwin build --profile diag -p victory --features diagnostics \
    --target x86_64-pc-windows-msvc
  (Windows: cargo build --release -p victory)

RUN
  ./launcher/victory-launch.sh --detect          # hardware + chosen profile + why
  ./launcher/victory-launch.sh --game <dir>      # play
  ./launcher/victory-launch.sh --game <dir> --safe   # if "device removed"

MEASURE
  ./tools/bench.sh "<label>"
  Report: median of WORLD frames (fps < 10), n, min/max, device removals,
  whether the picture was correct. Need n >= 20 or the run did not reach the
  world. Two runs of the same build differ by up to 30%; do not chase 10%.

WHAT TO PUSH, WHERE
  - Push a branch named port/<short-name> or fix/<short-name> to YOUR fork.
  - Open a PR against main of the universal fork. Body must contain:
      What / Evidence table (commit, GPU, OS+driver, profile, median, n,
      removals, picture) / Verified by eye / Risk.
  - Docs and new profiles are welcome as their own small PRs.
  - If you only have a discovery: open an ISSUE with machine, driver versions,
    --detect output, one bench row, and a grepped log excerpt. That is a
    first-class contribution.

WHAT NOT TO PUSH
  - Game data of any kind, binaries you did not build, screenshots of the title.
  - Code copied from XCL.
  - Default changes without a bench row.
  - A vendor fix without a gate, or a merge commit of someone else's branch.
  - Mixed PRs ("while I was in there...").

HOW THE LAUNCHER WORKS (do not hard-code settings in code)
  - launcher/profiles.json is the single source of truth. Each profile has
    match {os, vendor, vram_min_mb?} and sets {VICTORY_*}. First match wins.
  - launcher/victory-launch.sh and .ps1 both read it; changing one without the
    other is a bug.
  - Environment variables exported by the user override the profile; use that
    for A/B tests instead of editing files.
  - Adding hardware support = adding a profile object with an honest
    "confidence" (measured / reported / extrapolated / untested) and a note
    citing where the numbers came from.

KNOWN LANDMINES (all of these cost hours; do not re-derive)
  - Wine's d3dcompiler_47 rejects RWByteAddressBuffer methods. Without PR #127
    the indirect clamp fails, EVERY indirect draw is dropped and the world is
    black while UI and audio work. New HLSL uses structured-buffer indexing.
    Verify: grep 'no argument clamp' <log> is empty.
  - RADV segfaults in vkCreateGraphicsPipelines on world mesh pipelines with
    vkd3d-proton < build 5742 (exit 3). Same shader compiles on lavapipe: it is
    a driver crash, not our translation. Launcher installs the newer one.
  - LATE_WAIT >= 2, JOIN_COPIES=1, RELAY remove the device (0x887A0005) within
    minutes on Linux. Linux profiles must stay at LATE_WAIT <= 1.
  - PR #154's VEH copy collides with newer upstream page guards
    (0x80000001 guard-page violation). Belongs on main only.
  - A local LLM on -ngl 99 takes 11+ GB VRAM and costs ~10x frame time.
  - A bare `wineboot` prefix lacks d3dcompiler_47/wined3d; create the prefix
    through Proton (the launcher does).

CURRENT STATE (do not contradict it without evidence)
  ~0.7-0.9 fps median on RX 9070 XT / Mesa 26.3.1 / Wine 11, stable, correct
  picture. GPU 4-6% busy, ~0.7 of one core used, one busy thread of 103: the
  project is serialisation-bound under Wine, not compute-bound. The biggest
  known win is replacing the per-frame GetWriteWatch dirty-page query in
  victory-gpu/src/watch.rs and the copy/fence coordination around it.

WHEN YOU REPORT
  Post: commit, what changed, the bench row, what you verified by eye, and what
  you could not verify. Say "not measured" when you did not measure it.
```

---

## Keeping the group on the same page

* The fork owner merges `main`. Everyone else sends PRs. Nobody force-pushes
  `main`.
* Every PR body has the evidence table; reviews check the table before the
  diff.
* `docs/FORK.md`'s results table is append-only — it is the project's memory of
  what was tried and what it was worth.
* Pin the machine matrix in the channel: GPU, OS, driver, profile, median, n.
  If a machine is not in that list, its report is a data point, not a default.
* When two people implement the same idea differently, the tie-breaker is the
  bench row on both vendors — not seniority, not "it looks cleaner".
