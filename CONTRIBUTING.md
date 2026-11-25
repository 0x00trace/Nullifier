# Contributing

Short version: **one idea per PR, with a bench row, or it is closed with "needs
a bench row".** Medians of world frames, never peaks.

## Where to start

| You are… | Go to |
|---|---|
| New, nothing built | [`docs/ONBOARD.md`](docs/ONBOARD.md) § A — one command: `./tools/setup.sh` |
| Coming from your own fork | [`docs/ONBOARD.md`](docs/ONBOARD.md) § B and [`docs/MIGRATION.md`](docs/MIGRATION.md) |
| Reporting a result, no code | open an issue — the template asks for everything needed |
| Working with an AI assistant | paste [`docs/AI-PROMPT.md`](docs/AI-PROMPT.md) |

Read [`AGENTS.md`](AGENTS.md) before changing code: it lists the rules and every
landmine this project has already hit (a wrong guess costs hours, not minutes).

## The bar

* **No game files, assets, dumps, screenshots of the title, or its name.** The
  repository must stay clean.
* **No code from XCL** (GPL-2.0-only, incompatible with this GPL-3.0 tree).
* **Vendor-specific behaviour must be runtime-gated**, never assumed.
* **Evidence:** a `./tools/bench.sh` row with median, n, device removals and
  whether the picture was correct. PRs and issues have templates for it.
* **One idea per PR.** Four small PRs beat one 3,000-line diff.
* Tests for shader/GPU work: `cargo test -p victory-shader -p victory-gpu`.

## What gets merged

What passes the protocol in `AGENTS.md` § 5 and improves (or at least does not
regress) the median on the reporter's machine. Negative results are welcome as
issues: "this switch bought nothing, here is the row" saves the next person the
same week.

## Licence

By contributing you agree your work is licensed GPL-3.0-or-later, like the rest
of the tree.
