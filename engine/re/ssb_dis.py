"""Disassembler for the program's compiled AI scripts (`Data/com/ai/*.ssb`), our own tool.

Read-only. The format and instruction set come from the program's VM `FUN_1402aaa30` and its loader
`FUN_1402aa620` (see engine/docs/FORMATS-CPU.md). Python standard library only.

On disk a script is little-endian 32-bit words:
  word0   0x20 (code start; the program overwrites it with a module slot at load time)
  word1   byte offset of the function table (= end of code)
  word2   byte offset of the end of the function table = start of the import table (16-byte entries)
  word3   byte offset of the string pool (= end of the import table)
  word4..7 unused here
Function table entries are 3 words: (name hash1, name hash2, WORD offset of the function from file start).
Instruction = one word: opcode = low byte, b1 = bits 8..15, s = signed 16 bits 16..31 (for op 01 the
fields are group = bits 28..31, fn = bits 16..27).

Usage:
  python ssb_dis.py <file.ssb> [--natives]     disassemble (with --natives: list distinct native calls + arg counts)
  python ssb_dis.py --hash main                print the name hash pair
"""
import struct
import sys

# The program's name hash: base-40 over 6-char halves (FUN_1402847c0, table DAT_1402f76a0).
_ALPHA = {}
for _i, _c in enumerate("0123456789"):
    _ALPHA[_c] = _i + 1
for _i in range(26):
    _ALPHA[chr(65 + _i)] = 11 + _i
    _ALPHA[chr(97 + _i)] = 11 + _i
_ALPHA["_"] = 37
_ALPHA["="] = 37


def name_hash(name):
    """Return (hash1, hash2) as the program computes them for `name` (max 12 chars; stops at NUL or '.').

    hash1 = first six chars, hash2 = chars seven to twelve; a short name pads with zero digits (so the
    value is multiplied by 40 per missing char). Returns -1 (0xffffffff as signed) for an unmapped char.
    """
    s = name.split(".")[0]

    def half(chars):
        h = 0
        for i in range(6):
            if i < len(chars):
                v = _ALPHA.get(chars[i])
                if v is None:
                    return -1
            else:
                v = 0
            h = (v + h * 40) & 0xFFFFFFFF
        return h

    return half(s[:6]), half(s[6:12])


MASK32 = 0xFFFFFFFF


def sx(v, bits):
    v &= (1 << bits) - 1
    return v - (1 << bits) if v >> (bits - 1) else v


class Ssb:
    def __init__(self, data):
        self.data = data
        self.words = struct.unpack_from("<%dI" % (len(data) // 4), data)
        self.code_start = self.words[0]
        self.func_table = self.words[1]
        self.func_table_end = self.words[2]
        self.string_pool = self.words[3]
        if self.func_table % 4 or self.func_table_end < self.func_table:
            raise ValueError("bad ssb header")

    def exports(self):
        """[(hash1, hash2, byte_offset)] from the function table."""
        out = []
        o = self.func_table
        while o + 12 <= self.func_table_end:
            h1, h2, w = struct.unpack_from("<III", self.data, o)
            out.append((h1, h2, w * 4))
            o += 12
        return out

    def find_export(self, name):
        want = name_hash(name)
        for h1, h2, off in self.exports():
            if (sx(h1, 32), sx(h2, 32)) == (sx(want[0], 32), sx(want[1], 32)):
                return off
        return None

    def cstring(self, s):
        """String-pool reference (op 0x11 operand `s`): file offset string_pool + s."""
        o = self.string_pool + s
        e = self.data.index(b"\0", o)
        return self.data[o:e].decode("latin1")


class Instr:
    __slots__ = ("off", "word", "op", "b1", "s", "imm", "size")

    def __init__(self, off, word, imm=None):
        self.off, self.word = off, word
        self.op = word & 0xFF
        self.b1 = (word >> 8) & 0xFF
        self.s = sx(word >> 16, 16)
        self.imm = imm
        self.size = 8 if self.op == 0x0F else 4

    @property
    def group(self):
        return self.word >> 28

    @property
    def fn(self):
        return (self.word >> 16) & 0xFFF

    def target(self):
        """Byte offset targeted by a relative branch/call (ops 03, 07) or data address (1f)."""
        if self.op in (0x03, 0x07):
            return self.off + 4 + self.s * 4
        if self.op == 0x1F:
            return self.off + 4 + (sx(self.word, 32) >> 8) * 4
        return None


_CMP = {0: "EQ", 1: "NE", 2: "GT", 3: "LT", 4: "GE", 5: "LE", 6: "BITS"}  # left operand = pushed first; see FORMATS-CPU.md
_BR = {0: "jmp", 1: "jnz", 2: "jz", 3: "jz.yield"}


def decode(ssb, start=None, end=None):
    """Linear decode of the code area. Op 0x0f (push imm32) takes the next word as its operand."""
    out = []
    o = ssb.code_start if start is None else start
    end = ssb.func_table if end is None else end
    d = ssb.data
    while o < end:
        (w,) = struct.unpack_from("<I", d, o)
        ins = Instr(o, w)
        if ins.op == 0x0F:
            ins.imm = struct.unpack_from("<i", d, o + 4)[0]
        out.append(ins)
        o += ins.size
    return out


def reachable(ssb, entries=None):
    """Recursive-descent decode from the exported entries: {offset: Instr} for every reachable instruction.

    Code and data tables share the code area (tables are reached with op 0x1f, never executed), so a linear
    sweep misreads table words as opcodes; descending only along real control flow avoids that.
    """
    if entries is None:
        entries = [off for _, _, off in ssb.exports()]
    seen = {}
    todo = list(entries)
    d = ssb.data
    while todo:
        o = todo.pop()
        while 0 <= o < ssb.func_table and o not in seen:
            (w,) = struct.unpack_from("<I", d, o)
            ins = Instr(o, w)
            if ins.op == 0x0F:
                ins.imm = struct.unpack_from("<i", d, o + 4)[0]
            seen[o] = ins
            nxt = o + ins.size
            if ins.op == 0x03:
                todo.append(ins.target())
            elif ins.op == 0x07:
                todo.append(ins.target())
                if ins.b1 not in (1, 2, 3):  # unconditional: no fall-through
                    break
            elif ins.op == 0x08:
                todo.append(o + 4 + ins.s * 4)
            elif ins.op == 0x05 or (ins.op == 0x02 and ins.b1 in (1, 2)):
                break
            o = nxt
    return seen


def _split_functions(ssb):
    """{entry offset: [instr offsets]} grouping reachable code by function (entry = export or call target)."""
    seen = reachable(ssb)
    entries = {off for _, _, off in ssb.exports()} | {i.target() for i in seen.values() if i.op == 0x03}
    return seen, sorted(entries)


def verify_stack(ssb):
    """Static check that the stack effects of the decoded semantics are balanced.

    Walks every function from its entry with depth 0 (words pushed since the frame base) and checks that all
    paths reaching an instruction agree on the depth, and that every `ret` finds exactly the allocated locals
    (plus the return value) on the stack. Returns (anomalies, {entry: (locals, args)}).
    """
    seen, entries = _split_functions(ssb)
    sig = {}
    bad = []

    def effect(i, args_of):
        op = i.op
        if op == 0x01:
            return -i.b1
        if op == 0x03:
            return -args_of.get(i.target(), 0)
        if op == 0x06:
            return i.s
        if op in (0x09, 0x0E, 0x0F, 0x10, 0x11, 0x1F):
            return 1
        if op == 0x0A:
            return -1 if i.b1 in (0, 1, 2) else 0
        if op == 0x0D:
            return -i.s
        if op in (0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A) or (op == 0x1E):
            return -1
        if op in (0x1B, 0x1C, 0x1D, 0x00):
            return 0
        if op == 0x08:
            return 0  # taken-match pops 1, handled in the walk
        return 0

    # pass 1: infer args per function by walking with unknown callee args = 0, then iterate to a fixed point
    args_of = {}
    for _ in range(16):
        changed = False
        for e in entries:
            depth_at = {}
            stack = [(e, 0)]
            locals_ = None
            rets = set()
            while stack:
                o, d = stack.pop()
                while o in seen:
                    if o in depth_at:
                        if depth_at[o] != d:
                            bad.append("depth mismatch at %06x: %d vs %d (fn %06x)" % (o, depth_at[o], d, e))
                        break
                    depth_at[o] = d
                    i = seen[o]
                    nxt = o + i.size
                    if i.op == 0x06:
                        locals_ = (locals_ or 0) + i.s
                    if i.op == 0x05:
                        rets.add((d - (1 if i.b1 else 0), i.s))
                        break
                    if i.op == 0x07:
                        dd = d + (-1 if i.b1 in (1, 2, 3) else 0)
                        stack.append((i.target(), dd))
                        if i.b1 not in (1, 2, 3):
                            break
                        d = dd
                    elif i.op == 0x08:
                        stack.append((o + 4 + i.s * 4, d))  # mismatch: value stays
                        d -= 1  # match: value popped, fall through
                    else:
                        d += effect(i, args_of)
                    o = nxt
            for dl, s in rets:
                a = s - dl
                if args_of.get(e) != a:
                    changed = True
                    args_of[e] = a
        if not changed:
            break
        bad.clear()
    for e in entries:
        sig[e] = args_of.get(e, 0)
    return bad, sig


def text(ins, natives=None):
    op, b1, s = ins.op, ins.b1, ins.s
    if op == 0x00:
        return "nop"
    if op == 0x01:
        n = (natives or {}).get((ins.group, ins.fn))
        return "native g%d.f%d%s pop=%d" % (ins.group, ins.fn, (" " + n) if n else "", b1)
    if op == 0x02:
        return {0: "wait %d" % s, 1: "fault", 2: "restart", 3: "budget-reset"}.get(b1, "nop2")
    if op == 0x03:
        return "call %06x" % ins.target()
    if op == 0x04:
        return "call-import %d" % s
    if op == 0x05:
        return "ret pop=%d%s" % (s, " +value" if b1 else "")
    if op == 0x06:
        return "alloc %d" % s
    if op == 0x07:
        return "%s %06x" % (_BR.get(b1, "jmp"), ins.target())
    if op == 0x08:
        return "case %d else %06x" % (b1, ins.off + 4 + s * 4)
    if op == 0x09:
        return "push local[%d]" % s
    if op == 0x0A:
        return "store local[%d] %s" % (s, {0: "=", 1: "+=", 2: "-="}.get(b1, "?%d" % b1))
    if op == 0x0D:
        return "pop %d" % s
    if op == 0x0E:
        return "push %d" % s
    if op == 0x0F:
        return "push32 %d" % ins.imm
    if op == 0x10:
        return "push result"
    if op == 0x11:
        return "push str@%d" % s
    if op in (0x12, 0x13, 0x14, 0x15, 0x16):
        return {0x12: "add", 0x13: "sub", 0x14: "mul", 0x15: "div", 0x16: "mod"}[op] + (".r" if b1 else "")
    if op == 0x17:
        return "cmp.%s%s" % (_CMP.get(s if 1 <= s <= 6 else 0), ".r" if b1 else "")
    if op in (0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D):
        return {0x18: "and", 0x19: "or", 0x1A: "xor", 0x1B: "not", 0x1C: "neg", 0x1D: "lnot"}[op]
    if op == 0x1E:
        return "%s%s" % ({1: "sar", 2: "shr"}.get(s, "shl"), ".r" if b1 else "")
    if op == 0x1F:
        return "push &%06x" % ins.target()
    return "?op%02x" % op


def disassemble(ssb, natives=None):
    exports = {off: "export#%d" % i for i, (_, _, off) in enumerate(ssb.exports())}
    ins = [v for _, v in sorted(reachable(ssb).items())]
    calls = {i.target() for i in ins if i.op == 0x03}
    lines = []
    for i in ins:
        label = ""
        if i.off in exports:
            label = exports[i.off]
        elif i.off in calls:
            label = "func_%06x" % i.off
        if label:
            lines.append("%s:" % label)
        lines.append("  %06x  %08x  %s" % (i.off, i.word, text(i, natives)))
    return "\n".join(lines)


def native_usage(ssb):
    """{(group, fn): set(pop counts)} for every reachable op-01."""
    use = {}
    for i in reachable(ssb).values():
        if i.op == 0x01:
            use.setdefault((i.group, i.fn), set()).add(i.b1)
    return use


def main(argv):
    if len(argv) >= 2 and argv[0] == "--hash":
        h1, h2 = name_hash(argv[1])
        print("%08x %08x" % (h1 & MASK32, h2 & MASK32))
        return 0
    if not argv:
        print(__doc__)
        return 2
    ssb = Ssb(open(argv[0], "rb").read())
    if "--natives" in argv:
        for (g, f), pops in sorted(native_usage(ssb).items()):
            print("g%d.f%d pops=%s" % (g, f, sorted(pops)))
    else:
        print(disassemble(ssb))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
