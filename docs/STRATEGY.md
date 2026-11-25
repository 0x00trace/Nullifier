# Strategy: rebase onto the most complete tree, keep the universal shell

Decision (2026-11-25): **the base changes; the fork does not.**

Our value is the combination — a universal tree with the best of every fork plus
the Wine fixes and the launcher — not the lineage. So:

| | Before | After |
|---|---|---|
| Base | `myokex/victory-amd` @ `070df42` (2 days old) | **Belobug's tree** (upstream `8e79c80` + #45 + #18 + ambient light + AMD fixes) |
| Ours on top | #127, #270, #154 | **DXC fallback (#174)**, `launcher/`, `tools/`, `docs/` |
| Reason | it was the tree that ran | it is the tree that runs **best** (3–5 fps on Windows, same GPU) |

Everything from the old base that still matters is either already in theirs, or
small enough to re-apply. The old lineage is kept as
`reference/victory-amd-070df42` for porting, never merged.

## Why not a union of all forks

Every fork rewrote the same subsystems (view guards, copies, texture refresh,
descriptors). A union merge conflicts in ~14 files per pair and, when it
compiles, renders wrong. What *can* be mixed is **policy**, not structure: one
implementation per subsystem, chosen at runtime.

## The plan, in order

1. Branch `mix/belobug` from their `main`. Public, so anyone can bench it.
2. Apply the DXC fallback (#174). One conflict, in `indirect.rs`.
3. Port `launcher/`, `tools/`, `docs/` and the 512 MB profile — mechanical.
4. Re-apply our #270 idea only if their texture path lacks it.
5. Build; run **two benches** (default profile and `--safe`) and look at the
   picture. Then, and only then, `main`.
6. Turn the two or three genuinely competing designs into switches as they are
   touched: `VICTORY_COPY_PATH`, `VICTORY_SUBMIT`, `VICTORY_TEXTURE_TRACK`.

## How the team works with this

Everyone keeps their own tree. **Nobody rebases onto ours, and we never merge
theirs.** Work crosses the boundary as ideas, one per PR.

### During the rebase (it is a freeze, not a stop)

* `main` keeps working but takes **docs only** — no renderer changes.
* `mix/belobug` is where the renderer moves. Issues and PRs that target
  rendering are labelled `mix` and land there.
* Anyone can run it: `git fetch && git checkout mix/belobug`, build, bench, and
  report a row. That is the most useful thing a non-maintainer can do — the
  rebase needs numbers on hardware we do not have.

### After the rebase

* Contributors branch `port/<name>` or `fix/<name>` **from the new `main`** and
  open one PR per idea, with the evidence table from the template.
* People whose own forks are ahead (Belobug, fxceless, VItoriaBr, Marvisak)
  keep their trees. We port from them and credit them, and they are welcome to
  PR the same idea directly — whichever lands first wins, and the other becomes
  a switch if it is cheap.
* Hardware without a profile: add one object to `launcher/profiles.json` with
  an honest `confidence`, plus a bench row. No code.

### Who decides

* The maintainer merges `main` and runs the protocol (`AGENTS.md` §5).
* When two implementations compete, the tie-breaker is **the bench row on both
  platforms** — not seniority, not tidiness. If both are close, keep both behind
  a switch and let the profiles choose.

## What "done" looks like

* One tree, one `main`, boots on AMD and NVIDIA, Linux and Windows.
* Profiles select the policy per machine; nobody edits code to get a working
  configuration.
* `docs/FINDINGS.md` says what is slow and why, with numbers, on every platform
  we claim to support.
* No branch is "the good one" that only its author can reproduce.
