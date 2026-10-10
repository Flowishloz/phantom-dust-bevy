#!/usr/bin/env python3
"""Contact sheet of move-recorder takes (our own tool; writes a local image, never commit it).

One row per take, frames at the trace's landmarks: just before the press, the press, the cast
(press +0.5 s), the fire, 0.25 s after the fire, the first hit on the opponent (health drop) and the
end. The video and the trace are aligned by the take's wall clock (video_t0_s in the .json) and
then refined by the first big picture change near the press (the cast animation starting).

  python mr_contact_sheet.py TAKES_DIR OUT.png [--width 360]
"""
import argparse
import csv
import json
import os
import subprocess
import sys

import numpy as np
from PIL import Image, ImageDraw

FFMPEG = os.path.expandvars(r"%USERPROFILE%\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe"
          r"\ffmpeg-8.1.2-full_build\bin\ffmpeg.exe")
TICK = 1 / 30.0


def read_trace(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def landmarks(rows):
    """(label, rec_tick) pairs from the trace events; missing ones are left out."""
    def first(pred):
        for r in rows:
            ev = [e for e in r["events"].split("|") if e]
            if any(pred(e) for e in ev):
                return int(r["rec_tick"])
        return None

    press = first(lambda e: e.startswith("press:"))
    fire = first(lambda e: e.startswith("fire:"))
    hit = None
    h0 = None
    for r in rows:
        try:
            h = int(r["p1_health"])
        except ValueError:
            continue
        if h0 is None:
            h0 = h
        if h < h0:
            hit = int(r["rec_tick"])
            break
    out = []
    if press is not None:
        out += [("before press", max(0, press - 6)), ("press", press), ("press +0.5 s", press + 15)]
    if fire is not None:
        out += [("fire", fire), ("fire +0.25 s", fire + 8)]
    if hit is not None:
        out += [("first hit", hit)]
    out += [("end", len(rows) - 1)]
    return out


def video_times(meta, rows):
    """Time (s) in the cut clip of every rec_tick, from the wall clock: the take starts at
    video_t0_s in the master recording and the clip was cut from video_clip_start_s."""
    t0 = meta["video_t0_s"] - meta.get("video_clip_start_s", 0.0)
    return {int(r["rec_tick"]): t0 + float(r["ms"]) / 1000.0 for r in rows}


def decode_small(video, fps=15, width=160):
    h = width * 5 // 8
    p = subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-i", video, "-vf",
                        "fps=%d,scale=%d:%d,format=gray" % (fps, width, h), "-f", "rawvideo", "-"],
                       capture_output=True)
    a = np.frombuffer(p.stdout, dtype=np.uint8)
    n = len(a) // (width * h)
    return a[: n * width * h].reshape(n, h, width).astype(np.float32)


def refine_offset(frames, fps, predicted_press_t, search=1.5):
    """Shift (s) to add to predicted video times: the biggest frame-to-frame change within +-search s
    of the predicted press time is taken as the press."""
    if len(frames) < 3:
        return 0.0
    d = np.abs(np.diff(frames, axis=0)).mean(axis=(1, 2))
    lo = max(0, int((predicted_press_t - search) * fps))
    hi = min(len(d), int((predicted_press_t + search) * fps) + 1)
    if hi <= lo:
        return 0.0
    k = lo + int(np.argmax(d[lo:hi]))
    return (k + 1) / fps - predicted_press_t


def grab(video, t, width):
    p = subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-ss", "%.3f" % max(0.0, t), "-i", video,
                        "-frames:v", "1", "-vf", "scale=%d:-1" % width, "-f", "image2pipe", "-vcodec", "png", "-"],
                       capture_output=True)
    import io
    return Image.open(io.BytesIO(p.stdout)).convert("RGB") if p.stdout else None


def build(takes_dir, out_png, width=360, shift_all=0.0, refine=False):
    metas = sorted(f for f in os.listdir(takes_dir) if f.endswith(".json"))
    rows_img = []
    notes = []
    for jf in metas:
        meta = json.load(open(os.path.join(takes_dir, jf)))
        if not meta.get("video"):
            notes.append("%s: no video" % jf)
            continue
        video = os.path.join(takes_dir, meta["video"])
        rows = read_trace(os.path.join(takes_dir, meta["trace"]))
        vt = video_times(meta, rows)
        lm = landmarks(rows)
        press = dict(lm).get("press")
        shift = shift_all
        if refine and press is not None:
            fr = decode_small(video)
            shift += refine_offset(fr, 15, vt[press])
        tiles = []
        for label, tick in lm:
            im = grab(video, vt[min(tick, max(vt))] + shift, width)
            if im is None:
                continue
            d = ImageDraw.Draw(im)
            d.rectangle([0, 0, width, 14], fill=(0, 0, 0))
            d.text((3, 1), "%s  t%d" % (label, tick), fill=(255, 255, 0))
            tiles.append(im)
        if tiles:
            rows_img.append((meta["name"], shift, tiles))
        notes.append("%s: refine shift %+.2f s" % (meta["name"], shift))
    if not rows_img:
        return notes
    w = max(sum(t.width for t in tiles) for _n, _s, tiles in rows_img)
    th = max(t.height for _n, _s, tiles in rows_img for t in tiles)
    sheet = Image.new("RGB", (w, (th + 16) * len(rows_img)), (20, 20, 20))
    d = ImageDraw.Draw(sheet)
    for i, (name, shift, tiles) in enumerate(rows_img):
        y = i * (th + 16)
        d.text((4, y + 2), "%s  (video/trace shift %+.2f s)" % (name, shift), fill=(255, 255, 255))
        x = 0
        for t in tiles:
            sheet.paste(t, (x, y + 16))
            x += t.width
    os.makedirs(os.path.dirname(os.path.abspath(out_png)), exist_ok=True)
    sheet.save(out_png)
    return notes


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("takes_dir")
    ap.add_argument("out_png")
    ap.add_argument("--width", type=int, default=360)
    ap.add_argument("--shift", type=float, default=0.0, help="seconds added to every video time (clips cut before the epoch calibration need -0.75)")
    ap.add_argument("--refine", action="store_true", help="also snap to the biggest picture change near the press (noisy)")
    a = ap.parse_args(argv)
    for n in build(a.takes_dir, a.out_png, a.width, a.shift, a.refine):
        print(n)
    return 0


if __name__ == "__main__":
    sys.exit(main())
