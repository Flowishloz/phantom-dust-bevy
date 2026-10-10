#!/usr/bin/env python3
"""Effect coverage on film_compare sheets, plus the original take's effect-request ticks (our own tool; local output only).

  python fx_measure.py [SHEET_DIR]      default ~/.pdtools/compare3

Per sheet (top row = original, bottom row = ours; tiles are TILE_W 400 wide, strip height 213 at the fair framing):
  cov    = share of pixels that differ from the same row's first ("before press") tile by more than 90 (sum of RGB),
           max and mean over the other tiles. Camera moves (the original's camera swings and pulls in) also count, so
           read ratios as a first filter, not a measurement.
  calls  = effect requests in the original trace (move recorder `effect:<player>.<id>` events), ticks relative to the press.
READ (2026-10-02, lane fxbright): the program drops an effect request whose effect is not resident yet -- `FUN_14018a040`
returns -1 and only asks for the load (`FUN_140189e80` -> `FUN_14018cd10`, asynchronous); the effect is preloaded when the hand
selection moves onto the skill (`FUN_14012d150`). A take whose first request comes within a few ticks of a cold selection has an
original with NO cast effect on screen, which looks like "ours is bigger / brighter". Read the `calls` column with the picture.
"""
import csv
import glob
import os
import re
import sys

import numpy as np
from PIL import Image

HOME = os.path.expanduser("~")
TAKES = os.path.join(HOME, ".pdtools", "moverec", "takes_full")
TH = 213
LABEL = 18


def tile(im, row, i):
    y0 = 22 + row * TH + LABEL
    return im[y0:y0 + TH - LABEL, i * 400:(i + 1) * 400]


def coverage(im):
    out = []
    for row in (0, 1):
        base = tile(im, row, 0)
        cov = [(np.abs(tile(im, row, i) - base).sum(2) > 90).mean() for i in range(1, im.shape[1] // 400)]
        out.append((max(cov), float(np.mean(cov))))
    return out


def calls(skill):
    fs = glob.glob(os.path.join(TAKES, "%03d_*_trace.csv" % skill))
    if not fs:
        return None
    press, ev = None, []
    for r in csv.DictReader(open(fs[0], newline="")):
        for e in r["events"].split("|"):
            if e.startswith("press:") and press is None:
                press = int(r["rec_tick"])
            if e.startswith("effect:") and not e.endswith(".600"):
                ev.append((int(r["rec_tick"]), e[7:]))
    return [(t - (press or 0), a) for t, a in ev]


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HOME, ".pdtools", "compare3")
    for f in sorted(glob.glob(os.path.join(src, "0*.png"))):
        m = re.match(r"(\d+)_(\d+)_(.*)\.png", os.path.basename(f))
        im = np.asarray(Image.open(f).convert("RGB")).astype(float)
        if not m or im.shape[1] < 2000:
            continue
        (co, mo), (cu, mu) = coverage(im)
        c = calls(int(m.group(2)))
        print("%-26s cov orig %.3f ours %.3f ratio %5.1f | mean %.3f %.3f | calls %s" % (m.group(3), co, cu, cu / max(co, 1e-3), mo, mu, c[:8] if c else c))


if __name__ == "__main__":
    main()
