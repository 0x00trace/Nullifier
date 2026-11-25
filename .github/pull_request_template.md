## What

One paragraph: what was wrong, what this changes, and which platform it was
written for.

## Evidence

| commit | GPU | OS + driver | profile | median fps | n | removals | picture |
|---|---|---|---|---|---|---|---|
|  |  |  |  |  |  |  |  |

Before this change, same scene and profile: **__ fps**, n=__.

Method: `./tools/bench.sh "<label>"` — median of world frames (fps < 10).
Peaks are camera cuts and are not evidence.

## Verified by eye

What you actually looked at: characters, buildings, HUD, lighting, menus.

## Risk

Which platform could this break? If the change is vendor-specific, where is the
runtime gate that keeps the other vendor on the old path?

## Checklist

- [ ] One idea in this PR (no "while I was in there")
- [ ] No game files, assets, dumps, screenshots of the title, or its name
- [ ] No code copied or ported from XCL (GPL-2.0, incompatible)
- [ ] Vendor-specific behaviour is runtime-gated, not assumed
- [ ] Bench row above is from this commit, medians of world frames, with n
- [ ] `cargo test -p victory-shader -p victory-gpu` if shaders or GPU changed
- [ ] `docs/FORK.md` results table updated (append-only)
- [ ] If this adds a switch or profile: documented and measured
