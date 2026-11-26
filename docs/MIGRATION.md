# Moving your work into this fork

This fork exists so the work stops being scattered across vendor-specific
branches. It is the **universal** fork: one `main` that boots on AMD and NVIDIA,
on Linux and Windows, with the per-platform differences handled by the launcher
(`launcher/profiles.json`) instead of by separate branches.

You do not have to throw your fork away. Keep it as your reference, and bring
the work across one measurable change at a time.

---

## 0. Before anything

* Read [`AGENTS.md`](../AGENTS.md) and [`docs/LAUNCHER.md`](LAUNCHER.md).
* Build this fork and confirm you can reach the world on **your** machine
  *before* porting anything. Otherwise you cannot tell your change from a
  pre-existing problem:

  ```bash
  ./launcher/nullifier-launch.sh --detect
  ./tools/bench.sh "baseline, upstream main"
  ```

  Keep that baseline row. It is your control for every later measurement.

---

## 1. Classify what you have

```bash
# against this fork's main (add it as a remote, do not merge)
git remote add universal <url-of-this-fork>
git fetch universal
git log --oneline universal/main..HEAD          # your commits, newest first
git diff --stat universal/main..HEAD
```

Put each commit in one of these buckets — they have different rules:

| Bucket | Examples | Rule |
|---|---|---|
| **Portability** | Wine/Proton fixes, driver workarounds, vkd3d changes | highest value; must not change behaviour on the platform it was not written for |
| **Vendor** | AMD/NVIDIA/Intel specific fixes | must be **runtime-gated** (a check, or a profile value), never `#if`-style assumptions that break the other vendor |
| **Shaders** | translator, HLSL/DXIL emission | needs the shader tests; must not use `ByteAddressBuffer` methods (see AGENTS.md §6) |
| **GPU/runtime** | copies, textures, barriers, memory | needs bench numbers on both vendors if you have access |
| **Settings** | new `VICTORY_*` switches, better defaults | a switch alone is not a result: needs a bench row showing what it buys |
| **Launcher** | new hardware profile, better detection | edit `launcher/profiles.json` only; both launchers read it |
| **Docs** | anything in `docs/` | always welcome |

Discard nothing yet — a change you think is vendor-specific is often the
interesting one (the AMD view-guard work fixed rendering on NVIDIA too).

---

## 2. Port it as one PR per idea

```bash
git checkout -b port/<short-name> universal/main
git cherry-pick <commit>          # or reimplement it if it conflicts badly
```

If the cherry-pick conflicts because both trees implemented the same idea
differently (this is common: view guards, page marking, texture refresh), **do
not** resolve it by keeping both halves. Pick the one that tests better on your
machine, delete the other, and say so in the PR.

Test before opening:

```bash
./tools/bench.sh "<what you changed>"        # twice: default profile, then --safe
cargo test -p victory-shader -p victory-gpu  # GPU tests need a card
```

Open the PR against `main` with exactly this body:

```
## What
One paragraph. What was wrong, what you changed.

## Evidence
| commit | GPU | OS/driver | profile | median | n | removals | picture |
|---|---|---|---|---|---|---|---|
| <sha> | RX 9070 XT | Arch, Mesa 26.3.1 | linux-amd-16gb | 0.78 fps | 34 | 0 | correct |

Before: 0.70 fps, n=31. Same scene, same profile.

## Verified by eye
Characters, buildings, HUD, lighting. (Say what you actually looked at.)

## Risk
Which platform could this break, and what did you do about it?
```

A PR without the table is closed with "needs a bench row", not reviewed.

---

## 3. If you only have a discovery, not code

That is still useful. Open an **issue** with:

* commit you tested, GPU, driver/Mesa/Proton versions;
* exact environment (`./launcher/nullifier-launch.sh --detect`);
* the bench row;
* the smallest log excerpt that shows the behaviour (`grep` it down, never
  paste a whole log, never attach game data or screenshots of the title);
* what you think it means, and how sure you are.

A well-written report often turns into a profile entry or a two-line fix.

---

## 4. Hardware profiles

If your machine runs better with different values, the change is a JSON object,
not code:

```json
{
  "name": "linux-nvidia-24gb",
  "match": { "os": "linux", "vendor": "nvidia", "vram_min_mb": 20000 },
  "confidence": "measured",
  "sets": { "VICTORY_TEXTURE_NO_WAIT": "1", "VICTORY_LATE_WAIT": "1" },
  "note": "RTX 4090, Mesa 26.3, median 2.1 fps, n=28, three runs"
}
```

Order matters: profiles are matched first-to-last, so a more specific one must
come before the generic one. `confidence` must be honest — `untested` is a
valid answer and gets merged only as a request for reports.

---

## 5. What gets rejected

* Anything containing game files, assets, dumps, or the title's name.
* Code copied or ported from **XCL** (GPL-2.0-only, incompatible with this
  GPL-3.0-or-later tree).
* PRs that change defaults with no bench row.
* Vendor fixes that are not runtime-gated and break the other vendor.
* "It feels faster" / screenshots / peaks instead of medians.
* Giant mixed PRs: one idea per PR, always.

---

## 6. Working with an AI assistant

Give your agent the block in [`docs/AI-PROMPT.md`](AI-PROMPT.md). It is written
to stop the four failure modes we have already hit: copying XCL code, merging
branches blind, claiming speed without medians, and adding vendor-specific
assumptions that break everyone else.
