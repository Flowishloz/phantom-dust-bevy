"""Look up functions in a Ghidra decompile export (our own tool).

The export (decomp_all.c, written by ExportDecomp.java) is decompiled game
code: it stays outside the repo (%USERPROFILE%/.pdtools/re-out). Never paste
its contents into the repo; describe logic in words with the address.

Usage:
  python engine/re/decomp_fn.py <decomp_all.c> <addr> [...]        print functions
  python engine/re/decomp_fn.py <decomp_all.c> --uses <text> [...]  list functions containing text
"""
import re
import sys


def load(path):
    src = open(path, encoding="utf-8", errors="replace").read()
    parts = re.split(r"^(?=//==== )", src, flags=re.M)
    return {p.split()[1]: p for p in parts if p.startswith("//==== ")}


def main():
    fns = load(sys.argv[1])
    args = sys.argv[2:]
    if args and args[0] == "--uses":
        for addr, body in fns.items():
            if all(t in body for t in args[1:]):
                print(addr, body.count("\n"), "lines")
        return
    for a in args:
        print(fns.get(a.lower().replace("fun_", ""), f"not found: {a}"))


if __name__ == "__main__":
    main()
