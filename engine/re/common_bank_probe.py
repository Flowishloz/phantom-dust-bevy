"""Probe common.bin for the duplicate-pair question (lane-step, 2026-09-29).

Reads the STH2 bank at game_files/Assets/Data/Sound/common.bin (read-only)
and prints, for each requested sound id, the REAL entry offset, its tracks,
each playable event's clip index, and each clip's byte range + size + sha1
of the raw bytes. Verdict per pair: SAME clip index = legitimate sharing
(two REAL entries point at one WAVE clip); DIFFERENT indices with identical
bytes = data-level duplication; DIFFERENT bytes = converter bug.

Usage: python3 engine/re/common_bank_probe.py  (run from worktree root)
"""
import hashlib
import struct
import sys

BANK = ("<repo>"
        "/game_files/Assets/Data/Sound/common.bin")
IDS = [24, 25, 28, 29, 30, 31, 32, 33, 34, 35]
# material footstep table DAT_1402f6270 ids (FUN_140231010/1402311e0): one-line summary
STEP_IDS = [24, 26, 28, 30, 32, 34, 36, 38, 44, 46, 48, 50, 52, 54, 56, 58]


def u32(d, o):
    return struct.unpack_from('<I', d, o)[0]


def i32(d, o):
    return struct.unpack_from('<i', d, o)[0]


def main():
    d = open(BANK, 'rb').read()
    assert d[:4] == b'STH2', 'not an STH2 bank'
    real = u32(d, 0x24)
    wave = u32(d, 0x28)
    assert d[real:real + 4] == b'REAL' and d[wave:wave + 4] == b'WAVE'
    n_sounds = u32(d, real + 8)
    wave_end = wave + 8 + u32(d, wave + 4)
    n_clips = u32(d, wave + 8)
    starts = [wave + u32(d, wave + 16 + 4 * i) for i in range(n_clips)]
    ends = [(starts[i + 1] if i + 1 < n_clips else wave_end) for i in range(n_clips)]
    print('common.bin: %d bytes, %d sounds, %d clips' % (len(d), n_sounds, n_clips))

    def clip_info(ci):
        s, e = starts[ci], ends[ci]
        h = hashlib.sha1(d[s:e]).hexdigest()[:12]
        return s, e, e - s, h

    for sid in IDS:
        off = i32(d, real + 16 + 4 * sid)
        if off < 0:
            print('id %d: EMPTY (REAL -1)' % sid)
            continue
        r = real + off
        assert d[r:r + 4] == b'REAT', 'id %d: no REAT' % sid
        ntr = u32(d, r + 8)
        print('id %d: REAL+0x%X vol=%d tracks=%d' % (sid, off, u32(d, r + 0x10), ntr))
        for k in range(ntr):
            to = u32(d, r + 0x30 + 4 * k)
            if to == 0xFFFFFFFF:
                print('   track %d: EMPTY' % k)
                continue
            t = r + to
            assert d[t:t + 4] == b'TRAT', 'id %d tr %d: no TRAT' % (sid, k)
            nev = u32(d, t + 8)
            for e in range(nev):
                ev = t + 0x30 + e * 0x100
                assert d[ev:ev + 4] == b'EVNT'
                kind = u32(d, ev + 0x20)
                ci = u32(d, ev + 0xF0)
                sr, ch = u32(d, ev + 0x28), u32(d, ev + 0x2C)
                dur = i32(d, ev + 0x1C)
                if kind == 0 and ci != 0xFFFFFFFF:
                    s, en, size, h = clip_info(ci)
                    print(('   tr %d ev %d: clip %d file[0x%X..0x%X] %d bytes '
                           'sha1=%s %dHz %dch dur=%dms') % (k, e, ci, s, en, size, h, sr, ch, dur))
                else:
                    print('   tr %d ev %d: kind=%d clip=%s (follow-up, no clip)' %
                          (k, e, kind, 'none' if ci == 0xFFFFFFFF else ci))
    print()
    print('footstep-table ids, first playable clip each (sound -> clips):')
    for sid in STEP_IDS:
        off = i32(d, real + 16 + 4 * sid)
        if off < 0:
            print('  id %d: EMPTY' % sid)
            continue
        r = real + off
        clips = []
        for k in range(u32(d, r + 8)):
            to = u32(d, r + 0x30 + 4 * k)
            if to == 0xFFFFFFFF:
                continue
            t = r + to
            for e in range(u32(d, t + 8)):
                ev = t + 0x30 + e * 0x100
                if u32(d, ev + 0x20) == 0 and u32(d, ev + 0xF0) != 0xFFFFFFFF:
                    clips.append(u32(d, ev + 0xF0))
        uniq = sorted(set(clips))
        tags = []
        for ci in uniq:
            s, en, size, h = clip_info(ci)
            tags.append('%d(%db,%s)' % (ci, size, h))
        print('  id %d: %s' % (sid, ' '.join(tags) if tags else 'no playable events'))


if __name__ == '__main__':
    sys.exit(main())
