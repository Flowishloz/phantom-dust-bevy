"""List or unpack the game's packed effect archive (our own tool).

Layout (read from the program, docs/PROGRAM-RE.md "Phase 3 / Packed effects"):
  Effect/e.i   u16 archive count (8), u16 entry count (496), then per entry
               u16 effect number, u16 archive index, u32 byte offset, u32 byte size
               (sorted by effect number; offsets and sizes are multiples of 0x800).
  Effect/a<k>.e  the archives. An entry's bytes are the effect's ALR, padded to
               0x800, followed by ONE 0x800 block whose first 412 bytes are its
               .eft event table (identical to the loose eftNNNN.eft for 330 of
               433 effects that have one; 103 differ in a few fields).

The program loads every effect this way (FUN_14018cd10 -> FUN_14018d0b0); the
loose eftNNNN.alr files are not on its normal path.

Usage:
  python engine/re/effect_archive.py <Effect dir>                 list entries
  python engine/re/effect_archive.py <Effect dir> <num> <out dir> write eftNNNN.alr + eftNNNN.eft
Output is game data: write it outside the repo (e.g. engine/cache or the scratchpad).
"""
import os
import struct
import sys

EFT_SIZE = 412


def entries(effect_dir):
    ei = open(os.path.join(effect_dir, "e.i"), "rb").read()
    _archives, count = struct.unpack_from("<HH", ei, 0)
    for i in range(count):
        yield struct.unpack_from("<HHII", ei, 4 + 12 * i)  # number, archive, offset, size


def main():
    d = sys.argv[1]
    if len(sys.argv) == 2:
        loose = {int(f[3:7]) for f in os.listdir(d) if f.startswith("eft") and f.endswith(".alr")}
        for num, arc, off, size in entries(d):
            print(f"{num:5d} a{arc}.e off 0x{off:07x} size 0x{size:06x} {'' if num in loose else 'packed only'}")
        return
    want, out = int(sys.argv[2]), sys.argv[3]
    for num, arc, off, size in entries(d):
        if num == want:
            with open(os.path.join(d, f"a{arc}.e"), "rb") as f:
                f.seek(off)
                blob = f.read(size)
            os.makedirs(out, exist_ok=True)
            open(os.path.join(out, f"eft{num:04d}.alr"), "wb").write(blob[: size - 0x800])
            # Every loose .eft is 412 bytes; the rest of the last block is padding (not zeroed).
            open(os.path.join(out, f"eft{num:04d}.eft"), "wb").write(blob[size - 0x800:size - 0x800 + EFT_SIZE])
            print(f"wrote eft{num:04d}.alr ({size - 0x800} bytes incl. padding) and eft{num:04d}.eft")
            return
    print(f"effect {want} not in e.i")


if __name__ == "__main__":
    main()
