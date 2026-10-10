"""Print the game's hit-reaction / player-state table (our own tool).

gsdata_en.dat holds a second table of 0x72-byte rows (57 i16, same row size as
the action table) at byte 0x2B00C. The program keeps a pointer to it
(DAT_1404bdc58, set in FUN_14017dcb0) and indexes it with the reaction id a hit
chooses (skill fields s57..s61) and with a few fixed ids (26 = brushed, 27 =
hit in mid-air). FUN_140258690 plays a row. Meanings are in docs/PROGRAM-RE.md
("Hit reactions").

Columns used (w = i16 index): w2 effect, w3 sound, w4 battle-voice group,
w38 kind, w39 knock-back speed (1/10.8 u per tick), w40 launch angle (degrees),
w41..w48 animation ids / tick counts (meaning depends on kind).

Usage:  python engine/re/state_table.py <gsdata_en.dat> [row ...]
"""
import struct
import sys

from action_table import ANIM

TABLE = 0x2B00C
ROW = 0x72

KINDS = {
    0: "play w41 (if set), hold for the clip",
    1: "play w41, timer w42, then lie down (state 18) 30 ticks",
    2: "4-way stagger by hit direction: w41 front, w42 right, w43 back, w44 left; hold for the clip",
    3: "2-way stagger: w41 hit from the front half, w42 from behind; hold for the clip",
    4: "4-way knock-down chains (4 entries per direction from w41)",
    5: "knock-down: launch w39 at w40 degrees; chain w41..w44 (front) / w45..w48 (back)",
    6: "knock-down, one chain w41..",
    7: "pinned (state 18): w41 or w43 by current pose",
    8: "bind: w41, held w42 + w43 + 1 ticks",
    9: "lifted: rise to w45 x 10 u, gravity 0.12 u/tick^2",
    10: "w41 then w42",
    11: "w41 held w42 ticks",
}


def name(i):
    return ANIM[i] if 0 <= i < len(ANIM) else str(i)


def main():
    data = open(sys.argv[1], "rb").read()
    rows = [int(x) for x in sys.argv[2:]] or range(0, 75)
    for i in rows:
        w = struct.unpack_from("<57h", data, TABLE + i * ROW)
        if not any(w[1:]):
            continue
        k = w[38]
        anims = [name(x) if x > 0 else "-" for x in w[41:49]]
        print(f"row {i}: kind {k} ({KINDS.get(k, '?')})")
        print(f"   effect {w[2]} sound {w[3]} voice group {w[4]}  w39 {w[39]} w40 {w[40]}  w41..w48 {list(w[41:49])}")
        if k in (0, 1, 2, 3, 5, 6, 7, 10):
            print(f"   as animations: {anims}")
        elif k in (8, 11):
            print(f"   animation {anims[0]}")


if __name__ == "__main__":
    main()
