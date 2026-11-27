# Website

A single static page: two questions (Play / Contribute, then AI or not) and the
answers. No build step, no framework, no tracking.

Serve it locally with any static server:

```bash
python3 -m http.server 8000 --directory site
```

GitHub Pages: `.github/workflows/pages.yml` publishes `site/` on every push to
`main`. The repository setting **Settings → Pages → Source** must be
**GitHub Actions** once.
