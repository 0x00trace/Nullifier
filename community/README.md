# Community board

`leaderboard.json` is the published board the launcher reads
(`https://raw.githubusercontent.com/iNiKKo/Nullifier/main/community/leaderboard.json`).
It is a JSON array of entries:

```json
{
  "name": "nightowl",
  "version": "0.0.12",
  "game": "v1.0.2.1 (b99f876b)",
  "profile": "linux-amd-16gb",
  "fps": 6.7,
  "frame_ms": 150.0,
  "gpu_ms": 31.7,
  "draws": 2904,
  "when": 1764172800,
  "log": "...",
  "remote": false
}
```

No personal data: the launcher's **Share last run** already replaces home
paths, the game folder, emails and IP addresses, and keeps only frame lines,
failures and resolution notes from the log. Do not add anything else.

## Submitting

1. Launcher → **Community** → choose a nickname → **Share last run**. A
   Markdown block with a `json` fence is on the clipboard.
2. **Post to GitHub** (or
   [open the form](https://github.com/iNiKKo/Nullifier/issues/new?template=share-run.yml&labels=leaderboard))
   and paste the block.
3. A maintainer moves the entry into `leaderboard.json` (or a small Action
   does it later). The launcher picks it up on **Refresh board**.

The app reads the raw file, so there is no server, token or database. Edits
are visible in git history.
