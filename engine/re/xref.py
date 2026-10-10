"""Rough cross-reference finder for PDUWP.exe (our own tool).

Scans .text for RIP-relative 32-bit displacements that land on a target
address (or within +range bytes of it), allowing 0/1/2/4 bytes of immediate
after the displacement. Prints the site and the containing function (from
functions.txt of the Ghidra export). False positives are possible; confirm
in Ghidra / decomp_all.c.

Usage: python engine/re/xref.py <hex target> [range] [functions.txt]
"""
import os
import bisect
import struct
import sys

EXE = r"../../../game_files/PDUWP.exe"
FUNCS = os.path.expandvars(r"%USERPROFILE%/.pdtools/re-out/functions.txt")
BASE = 0x140000000
TEXT_RVA, TEXT_RAW, TEXT_SIZE = 0x1000, 0x400, 0x2D1600


def main():
    target = int(sys.argv[1], 16)
    rng = int(sys.argv[2], 0) if len(sys.argv) > 2 else 1
    funcs = []
    for line in open(sys.argv[3] if len(sys.argv) > 3 else FUNCS):
        p = line.split()
        if p:
            funcs.append((int(p[0], 16), p[1]))
    funcs.sort()
    starts = [f[0] for f in funcs]
    data = open(EXE, "rb").read()
    text = data[TEXT_RAW:TEXT_RAW + TEXT_SIZE]
    seen = set()
    for i in range(len(text) - 4):
        disp = struct.unpack_from("<i", text, i)[0]
        site = BASE + TEXT_RVA + i
        for imm in (0, 1, 2, 4):
            dest = site + 4 + imm + disp
            if target <= dest < target + rng:
                k = bisect.bisect_right(starts, site) - 1
                fn = funcs[k][1] if k >= 0 else "?"
                key = (site, dest)
                if key not in seen:
                    seen.add(key)
                    print(f"{site:x} -> {dest:x} (imm {imm}) in {fn}")


if __name__ == "__main__":
    main()
