"""Read constants and small tables out of PDUWP.exe by virtual address (our own tool).

Usage:
  python engine/re/exe_const.py f32 1402f7d60 1402f7d30 ...   floats
  python engine/re/exe_const.py f64 1402f7f70                 doubles
  python engine/re/exe_const.py i32 1403faed0 [count]         ints (count consecutive)
  python engine/re/exe_const.py rows 1403faed0 11 28          i32 rows: <addr> <ints per row> <rows>
  python engine/re/exe_const.py frows 1403fb000 4 10          same, as floats
  python engine/re/exe_const.py hex 1403faed0 64              raw bytes

Only initialised sections are in the file: .text, .rdata (RVA 0x2D3000..) and
.data (RVA 0x3D4000..0x4BA800). Addresses above that (DAT_1404b....) are
runtime-only (.bss); this tool says so instead of printing garbage.
"""
import struct
import sys

EXE = r"../../../game_files/PDUWP.exe"
BASE = 0x140000000
# (rva, file offset, raw size)
SECS = [(0x1000, 0x400, 0x2D1600), (0x2D3000, 0x2D1A00, 0x100400), (0x3D4000, 0x3D1E00, 0xE6800)]

_data = open(EXE, "rb").read()


def va2f(va):
    r = va - BASE
    for rva, raw, size in SECS:
        if rva <= r < rva + size:
            return r - rva + raw
    raise ValueError(f"{va:x} is not in an initialised section (runtime .bss?)")


def get(fmt, va):
    return struct.unpack_from(fmt, _data, va2f(va))[0]


def main():
    kind, args = sys.argv[1], sys.argv[2:]
    if kind in ("f32", "f64", "i32", "i16", "u16"):
        fmt = {"f32": "<f", "f64": "<d", "i32": "<i", "i16": "<h", "u16": "<H"}[kind]
        size = struct.calcsize(fmt)
        if kind in ("i32", "i16", "u16") and len(args) == 2 and len(args[1]) < 5:
            va, n = int(args[0], 16), int(args[1])
            print(f"{va:x}:", [get(fmt, va + size * k) for k in range(n)])
            return
        for a in args:
            va = int(a, 16)
            try:
                print(f"{va:x} = {get(fmt, va)!r}")
            except ValueError as e:
                print(e)
    elif kind in ("rows", "frows"):
        va, per, n = int(args[0], 16), int(args[1]), int(args[2])
        fmt = "<i" if kind == "rows" else "<f"
        for r in range(n):
            row = [get(fmt, va + 4 * (r * per + k)) for k in range(per)]
            if kind == "frows":
                row = [round(x, 4) for x in row]
            print(f"{r:3d} {va + 4 * r * per:x}: {row}")
    elif kind == "hex":
        va, n = int(args[0], 16), int(args[1])
        o = va2f(va)
        for k in range(0, n, 16):
            print(f"{va + k:x}: {_data[o + k:o + k + 16].hex(' ')}")


if __name__ == "__main__":
    main()
