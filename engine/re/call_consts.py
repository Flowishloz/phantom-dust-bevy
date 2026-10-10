"""List the constant arguments passed at every direct call to one function (our own tool).

Purpose: label sounds and effects from the program instead of by ear. Point it at the game's
"play sound" (or "start effect") function and it prints, for each `call <target>`, the function
it sits in and the constants loaded into the first four argument registers just before the call
(rcx, rdx, r8, r9: `mov r32, imm32` and `xor r32, r32`). A constant sound id shows up as a
number you can look up in cache/sfx/common/<id>.wav, together with the calling function's address
so the context can be read in Ghidra. Heuristic, like xref.py: it scans the bytes before each call
for those instruction shapes without a full disassembler, so a few sites will show a stale or
missing value (marked "?"); confirm a finding in Ghidra. Only direct `call rel32` (E8) is found.

Usage:  python engine/re/call_consts.py <hex target address> [functions.txt] [--exe PDUWP.exe]
"""
import os
import bisect
import struct
import sys

EXE = r"../../../game_files/PDUWP.exe"
FUNCS = os.path.expandvars(r"%USERPROFILE%/.pdtools/re-out/functions.txt")
BASE = 0x140000000
TEXT_RVA, TEXT_RAW, TEXT_SIZE = 0x1000, 0x400, 0x2D1600
ARGS = {1: "rcx", 2: "rdx", 8: "r8", 9: "r9"}  # register number -> Windows x64 argument register
LOOKBACK = 48


def reg_writes(code):
    """Scan `code` left to right; return {arg register number: constant or None}.

    Recognised shapes (REX.B 0x41 selects r8-r15):
      B8+r imm32         mov r32, imm32
      [45] 31/33 /r      xor r32, r32 with the same register on both sides -> 0
    """
    out = {}
    i = 0
    while i < len(code):
        b = code[i]
        rex_b = 0
        j = i
        if b in (0x41, 0x45, 0x40, 0x44, 0x49):  # a REX prefix: take REX.B (bit 0) for the register
            rex_b = b & 1
            j = i + 1
            if j >= len(code):
                break
            b = code[j]
        if 0xB8 <= b <= 0xBF and j + 4 < len(code):
            reg = (b - 0xB8) + (8 if rex_b else 0)
            if reg in ARGS:
                out[reg] = struct.unpack_from("<I", code, j + 1)[0]
            i = j + 5  # step over the whole instruction so its immediate is not re-read as opcodes
            continue
        elif b in (0x31, 0x33) and j + 1 < len(code):
            modrm = code[j + 1]
            if modrm >> 6 == 3 and ((modrm >> 3) & 7) == (modrm & 7):
                reg = (modrm & 7) + (8 if rex_b else 0)
                if reg in ARGS:
                    out[reg] = 0
                i = j + 2
                continue
        i += 1
    return out


def find_calls(text, target):
    """Yield (call site VA, preceding bytes) for each `call rel32` that lands on target."""
    i = text.find(b"\xe8")
    while i != -1 and i + 5 <= len(text):
        rel = struct.unpack_from("<i", text, i + 1)[0]
        site = BASE + TEXT_RVA + i
        if site + 5 + rel == target:
            yield site, text[max(0, i - LOOKBACK):i]
        i = text.find(b"\xe8", i + 1)


def main():
    argv = sys.argv[1:]
    exe = EXE
    if "--exe" in argv:
        k = argv.index("--exe")
        exe = argv[k + 1]
        del argv[k:k + 2]
    if not argv:
        sys.exit(__doc__)
    target = int(argv[0], 16)
    funcs = []
    for line in open(argv[1] if len(argv) > 1 else FUNCS):
        p = line.split()
        if p:
            funcs.append((int(p[0], 16), p[1]))
    funcs.sort()
    starts = [f[0] for f in funcs]
    data = open(exe, "rb").read()
    text = data[TEXT_RAW:TEXT_RAW + TEXT_SIZE]
    rows = []
    for site, before in find_calls(text, target):
        k = bisect.bisect_right(starts, site) - 1
        fn = funcs[k][1] if k >= 0 else "?"
        w = reg_writes(before)
        rows.append((site, fn, [w.get(r) for r in (1, 2, 8, 9)]))
    print(f"{len(rows)} call sites to {target:x}")
    print("site        function              rcx      rdx      r8       r9")
    for site, fn, vals in rows:
        cells = "  ".join(f"{('?' if v is None else v):>7}" for v in vals)
        print(f"{site:x}  {fn:<20}  {cells}")
    # Which constant values appear most (a sound id column stands out).
    for name, col in (("rcx", 0), ("rdx", 1), ("r8", 2), ("r9", 3)):
        vals = sorted({r[2][col] for r in rows if r[2][col] is not None})
        print(f"distinct {name}: {len(vals)}" + (f"  {vals[:40]}" if vals else ""))


if __name__ == "__main__":
    main()
