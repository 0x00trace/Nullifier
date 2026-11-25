---
name: Discovery or test report
about: A measurement, a crash, a better profile, or a negative result. No code needed.
title: ""
labels: ""
assignees: ""
---

<!--
Numbers without a commit are not reproducible. Fill in what you have and write
"not measured" for the rest; that is fine and much better than a guess.
No game files, no screenshots of the title, no dumps.
-->

## Machine

- GPU + VRAM:
- CPU:
- OS + kernel/driver:
- Mesa / Proton / Wine versions:
- Game profile (Series X or S), resolution:

## Build

- Commit:
- Build: `diag` + `diagnostics` / `release`
- `./launcher/victory-launch.sh --detect` output:

```
(paste here)
```

## What happened

Menus at 30 fps / reached the world / crashed / black screen / wrong picture…
Say what you saw, not what you think caused it.

## Bench row

| commit | GPU | OS + driver | profile | median fps | n | removals | picture |
|---|---|---|---|---|---|---|---|
|  |  |  |  |  |  |  |  |

## Log excerpt

The smallest slice that shows it, for example:

```
sed 's/\x1b\[[0-9;]*m//g' <log> | grep -E "device removed|unhandled exception|no argument clamp|dropped .*not bounded" | tail
```

```
(paste here)
```

## Guess

What you think is going on, and how sure you are (e.g. "probably vkd3d-proton
version, 60 %").
