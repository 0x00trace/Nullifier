# Contributing

One idea per PR, with a bench row — **median of world frames, three runs, and a
picture over 50 KB** — or it is closed.

| You are… | Start at |
|---|---|
| New, nothing built | [`./tools/setup.sh`](tools/setup.sh), then [ONBOARD](docs/ONBOARD.md) |
| Coming from another fork | [MIGRATION](docs/MIGRATION.md) — one commit at a time |
| Reporting a result | open a discovery issue with its template |
| Using an AI assistant | paste [AI-PROMPT](docs/AI-PROMPT.md) |

Read [`AGENTS.md`](AGENTS.md) before changing code: it has the rules, the merge
protocol and every landmine already hit.

House rules: no game files or the title's name in the repository; no code from
XCL (GPL-2.0, incompatible); vendor fixes must be runtime-gated; shader or GPU
changes run `cargo test -p victory-shader -p victory-gpu`.

Everything else is in the [docs index](docs/README.md).
