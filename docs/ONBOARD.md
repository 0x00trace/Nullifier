# Start here

Give your AI this file first. It is written so an agent can work out which
situation you are in and then follow the right path without guessing.

```
Read docs/ONBOARD.md and follow it. Do not delete or reset any existing
directory. Ask me before anything destructive. When you are done, tell me:
which path you took, what you changed, and the bench row.
```

---

## Triage — which one are you?

An agent should answer these three questions before touching anything:

```bash
# 1. Do you have an existing tree with work in it?
for d in ~/victory/victory-amd ~/victory/victory-fork ~/victory; do
  [ -d "$d/.git" ] && echo "$d: $(git -C "$d" log --oneline -1 2>/dev/null)"
done

# 2. Does this clone have your work already?
cd <victory-universal> && git log --oneline -1

# 3. Do you have findings but no code?
ls docs/ notes/ 2>/dev/null
```

| Situation | Path |
|---|---|
| Nothing built yet, no prior work | **[A. Fresh start](#a-fresh-start)** |
| You have your own tree with commits (`victory-amd`, `victory-fork`, another fork) | **[B. Bring your work across](#b-bring-your-work-across)** |
| You only have notes/results, no code | **[C. Report a discovery](#c-report-a-discovery)** |

**Never delete the old tree.** It is your reference and your evidence. Keep it
until the ported PR is merged and your numbers reproduce here.

---

## A. Fresh start

One command, no sudo, idempotent:

```bash
git clone git@github.com:iNiKKo/victory-universal.git
cd victory-universal
./tools/setup.sh                 # toolchain into ~/.local, build, detect
./tools/setup.sh --game <dir>    # ... and start it once
```

It checks `git/python3/curl/clang/lld-link`, installs a private Rust toolchain
if you have none, installs `cargo-xwin`, builds the diagnostics binary, and
prints the profile the launcher picked for your hardware.

Then:

```bash
./launcher/victory-launch.sh --detect    # profile, values, and why
./launcher/victory-launch.sh --game DIR  # play
./tools/bench.sh "baseline"              # the number you will compare against
```

Keep that baseline row. It is your control for every future change.

If a run dies with `device removed`, launch with `--safe`.

---

## B. Bring your work across

You have commits in another tree. They are **not** merged wholesale — they are
ported one idea at a time, each with a bench row. This is what keeps the tree
from turning into the 14-file conflict mess that upstream and the AMD fork
already are.

### B1. Find out what you actually have

```bash
cd <your old tree>
git log --oneline -20
git status --short                    # uncommitted work? commit it on a wip/ branch first
git remote -v
```

Then compare against this fork without touching anything:

```bash
git remote add universal git@github.com:iNiKKo/victory-universal.git
git fetch universal
git log --oneline universal/main..HEAD      # your commits not in the fork
git diff --stat universal/main..HEAD
```

If that list is empty, you have no unique work — use path A and keep your tree
as a reference.

### B2. Classify each commit

| Kind | Example | Rule |
|---|---|---|
| Portability | Wine/Proton fix, driver workaround | highest value; must not change behaviour on the platform it was not written for |
| Vendor | AMD/NVIDIA/Intel specific | must be **runtime-gated** (capability check or a profile value), never an assumption that breaks the other vendor |
| Shaders | translator, HLSL/DXIL emission | never use `ByteAddressBuffer` methods (AGENTS.md §6); run the shader tests |
| GPU/runtime | copies, textures, barriers, memory | bench on every vendor you can reach |
| Settings | a new `VICTORY_*` switch or a better default | a switch is not a result; it needs a bench row |
| Launcher | hardware profile, detection | edit `launcher/profiles.json` only; both launchers read it |
| Docs | anything under `docs/` | always welcome |

Discard nothing yet. A change you think is vendor-specific is often the
interesting one — the AMD view-guard work fixed rendering on NVIDIA too.

### B3. Port one idea per PR

In the **universal clone**, not your old tree:

```bash
cd <victory-universal>
git checkout main && git pull
git checkout -b port/<short-name>

# bring one commit across:
git cherry-pick <sha>
# ... or reimplement it if it conflicts badly
```

If a cherry-pick conflicts because both trees built the same idea differently
(common: view guards, page marking, texture refresh), **do not keep both
halves**. Pick the one that benches better on your machine, delete the other,
and say which you dropped and why in the PR.

Test before opening:

```bash
./tools/bench.sh "<what changed>"          # profile defaults
./tools/bench.sh "<what changed> (safe)" -- --safe
cargo test -p victory-shader -p victory-gpu  # GPU tests need a card
```

Open a PR against `main`. The template asks for the evidence table; a PR
without it is closed with "needs a bench row", not reviewed.

### B4. Repeat

One idea per branch, one branch per PR. Four good PRs beat one 3,000-line diff
that nobody can review.

---

## C. Report a discovery

If you only have a result — "this switch gives +20 %", "this crashes on my
card", "this profile is wrong for 8 GB" — that is a first-class contribution.

Open an issue with the fields in the template:

* commit you tested;
* GPU, driver, Mesa/Proton versions;
* `./launcher/victory-launch.sh --detect` output;
* one `tools/bench.sh` row (median, n, removals, picture);
* the smallest log excerpt that shows it (grep it down; never attach game data
  or screenshots of the title);
* what you think it means and how sure you are.

A good report often becomes a profile entry or a two-line fix.

---

## Do not do

* Do not delete, reset or force-push anyone's tree — including your own old one.
* Do not merge a foreign branch into `main` (see AGENTS.md; it conflicts in 14
  files and breaks rendering).
* Do not copy code from **XCL** (GPL-2.0-only, incompatible with this GPL-3.0
  tree).
* Do not add game files, dumps, screenshots of the title, or its name in the
  tree.
* Do not change a default without a bench row.
* Do not report peaks; report the median of world frames with `n`.
