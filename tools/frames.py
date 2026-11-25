#!/usr/bin/env python3
"""frames: where a frame's time went, from victory's own frame lines.

    ./tools/frames.py <log> [--html out.html] [--csv out.csv]

Reads the per-frame accounting victory writes at info level:

    0.4 fps, 2532.7 ms/frame (819.1 preparing: copies 124.4 ms 115.5 MB,
    textures 361.6 ms 129.8 MB; 1660.5 on the GPU; 41.0 copying back)

and reports the median of every phase, the slice nothing accounts for, and the
memory rates. Writes an HTML chart (stacked phase bars + fps line) when asked.

Phases are NOT independent: preparing, GPU and copy-back overlap in principle,
so "unaccounted" is frame - preparing - GPU - copy-back and is where waits and
pacing live.
"""
import argparse, html, re, statistics as st, sys

PAT = re.compile(
    r"([\d.]+) fps, ([\d.]+) ms/frame \(([\d.]+) preparing: "
    r"copies ([\d.]+) ms ([\d.]+) MB, textures ([\d.]+) ms ([\d.]+) MB"
    r"(?:, bindless plans ([\d.]+) ms)?; "
    r"([\d.]+) on the GPU; ([\d.]+) copying back"
)

def parse(path):
    rows = []
    with open(path, errors="ignore") as f:
        for line in f:
            m = PAT.search(line)
            if not m:
                continue
            g = m.groups()
            r = dict(
                fps=float(g[0]), frame=float(g[1]), prep=float(g[2]),
                copies=float(g[3]), copies_mb=float(g[4]),
                tex=float(g[5]), tex_mb=float(g[6]),
                plans=float(g[7]) if g[7] else 0.0,
                gpu=float(g[8]), back=float(g[9]),
            )
            r["unacc"] = max(0.0, r["frame"] - r["prep"] - r["gpu"] - r["back"])
            rows.append(r)
    return rows

def med(rows, k):
    return st.median([r[k] for r in rows]) if rows else 0.0

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("log")
    ap.add_argument("--html")
    ap.add_argument("--csv")
    ap.add_argument("--world", type=float, default=10.0, help="fps below this counts as the world")
    ap.add_argument("--watch", type=float, default=0, metavar="SECONDS",
                    help="re-read the log every N seconds and repaint (live overlay in the terminal)")
    a = ap.parse_args()

    if a.watch > 0:
        import time
        while True:
            print("\x1b[H\x1b[2J", end="")
            print(f"victory frame breakdown — {a.log}  (every {a.watch:.0f}s, Ctrl-C to stop)")
            try:
                report(a.log, a.world)
            except SystemExit:
                print("waiting for frame lines…")
            sys.stdout.flush()
            time.sleep(a.watch)

    report(a.log, a.world, a.html, a.csv)

def report(path, world_fps, html_out=None, csv_out=None):
    rows = parse(path)
    if not rows:
        sys.exit("no frame lines found")
    world = [r for r in rows if r["fps"] < world_fps] or rows
    print(f"{len(rows)} frames, {len(world)} in the world")
    print(f"\n{'phase':<16}{'median ms':>11}{'share':>8}")
    order = [("copies", "copies"), ("tex", "textures"), ("plans", "bindless plans"),
             ("gpu", "GPU"), ("back", "copy-back"), ("unacc", "unaccounted")]
    frame = med(world, "frame")
    for k, label in order:
        v = med(world, k)
        if k == "plans" and v == 0:
            continue
        print(f"{label:<16}{v:>11.1f}{100*v/frame if frame else 0:>7.0f}%")
    print(f"{'frame':<16}{frame:>11.1f}")
    print(f"\nmedian {med(world,'fps'):.2f} fps   p10 {sorted(r['fps'] for r in world)[max(0,len(world)//10)]:.2f}"
          f"   n={len(world)}")
    c_mb, t_mb = med(world, "copies_mb"), med(world, "tex_mb")
    print(f"bandwidth/frame: copies {c_mb:.0f} MB, textures {t_mb:.0f} MB"
          f" -> {(c_mb+t_mb)/frame*1000 if frame else 0:.0f} MB/s at {1000/frame if frame else 0:.1f} fps")

    if csv_out:
        import csv
        with open(csv_out, "w", newline="") as f:
            w = csv.DictWriter(f, fieldnames=list(rows[0]))
            w.writeheader(); w.writerows(rows)
        print(f"csv: {csv_out}")

    if html_out:
        W, H, PAD = 1100, 320, 28
        n = len(world)
        fmax = max(r["frame"] for r in world) or 1
        bw = max(1.0, (W - 2*PAD) / n)
        parts = []
        for i, r in enumerate(world):
            x = PAD + i * bw
            y = H - PAD
            for k, col in (("copies", "#4e79a7"), ("tex", "#59a14f"), ("plans", "#b07aa1"),
                           ("gpu", "#e15759"), ("back", "#f28e2b"), ("unacc", "#bab0ac")):
                h = r[k] / fmax * (H - 2*PAD)
                y -= h
                if h > 0:
                    parts.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{bw:.2f}" height="{h:.1f}" fill="{col}"/>')
        fmaxfps = max(r["fps"] for r in world) or 1
        pts = " ".join(f"{PAD + i*bw:.1f},{H-PAD-(r['fps']/fmaxfps)*(H-2*PAD):.1f}" for i, r in enumerate(world))
        legend = "".join(
            f'<span style="color:{c}">■</span> {l} ' for c, l in
            [("#4e79a7","copies"),("#59a14f","textures"),("#b07aa1","bindless"),
             ("#e15759","GPU"),("#f28e2b","copy-back"),("#bab0ac","unaccounted")])
        open(html_out, "w").write(f"""<!doctype html><meta charset=utf-8>
<title>victory frame breakdown</title>
<style>body{{font:13px system-ui;background:#111;color:#ddd;margin:20px}}
rect{{opacity:.9}}</style>
<h2>victory frame breakdown — {html.escape(path)}</h2>
<p>median {med(world,'fps'):.2f} fps · frame {frame:.0f} ms · {n} world frames</p>
<p>{legend} <span style="color:#fff">— fps</span></p>
<svg width={W} height={H} style="background:#1b1b1b;border-radius:6px">
<polyline points="{pts}" fill="none" stroke="#fff" stroke-width="1" opacity=".7"/>{"".join(parts)}
</svg>
<p>Taller = slower. Grey is time the phases do not explain: waits and pacing.</p>""")
        print(f"html: {html_out}")

if __name__ == "__main__":
    main()
