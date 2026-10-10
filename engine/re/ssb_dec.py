"""Goto-style pseudo-C decompiler for the AI scripts (`Data/com/ai/*.ssb`), our own tool.

Read-only helper for reading `eneprog006` logic. It symbolically executes each function with an expression stack,
turns native calls into named calls (`NATIVE_NAMES`, our names from FORMATS-CPU.md section 4), and prints
`if (...) goto Lxxxx;` / labels at branch targets. The output is a reading aid, not a recompilable program.

Usage:  python ssb_dec.py <file.ssb> [function-offset-hex ...]
"""
import struct
import sys

import ssb_dis as D

# name(arity) for group-4 natives used by eneprog006 (a0 first). Names are ours (FORMATS-CPU.md section 4).
NATIVE_NAMES = {
    0: "setvar", 1: "getvar", 2: "vcopy", 16: "dist", 18: "node_at", 19: "MOVE_TO", 20: "nav_step", 21: "vadd",
    23: "heading", 24: "ground_ok", 28: "vx", 29: "vy", 30: "vz", 31: "node_at_strict", 32: "rand", 33: "angdiff",
    37: "nodedist", 48: "PRESS", 49: "path49", 51: "path51", 53: "probe", 56: "stuck", 60: "path_step", 82: "style0",
    85: "style2", 96: "set_target", 97: "get_target", 112: "self", 113: "vpos_of", 114: "vstart_of", 115: "yaw_of",
    117: "pairflag", 118: "hp", 119: "maxaura", 120: "aura", 121: "has_status", 122: "team_of", 123: "f123",
    124: "cond", 125: "cast_skill", 134: "dodge_skill", 135: "pickable_object", 144: "slot_skill", 146: "slot_ctype",
    147: "slot_cost", 148: "slot_ready", 150: "slot_state", 161: "sk_ctype", 162: "sk_sel", 165: "sk_class",
    166: "sk_motion", 168: "sk_s44", 169: "sk_hpcost", 175: "sk_aitype", 176: "scan_reset", 177: "scan_next",
    178: "obj_skill", 179: "obj_pos", 186: "obj186", 192: "cap_id", 193: "cap_pos", 194: "contact_state",
    195: "contact_index", 197: "cap_skill", 198: "cap_due", 199: "cap_pos_global", 200: "cap_index", 208: "TICK",
    210: "nodecount", 229: "heal", 234: "hopeless", 235: "respawn", 425: "stage", 426: "stage10",
}
LIB_NAMES = {16: "tbl"}
_CMP = {0: "==", 1: "!=", 2: ">", 3: "<", 4: ">=", 5: "<="}


def _label(off):
    return "L%04x" % off


def native_name(group, fn):
    if group == 0:
        return LIB_NAMES.get(fn, "lib%d" % fn)
    if group == 4:
        return NATIVE_NAMES.get(fn, "g4_f%d" % fn)
    return "g%d_f%d" % (group, fn)


def decompile_function(ssb, entry, instrs=None, arg_count=None):
    """Returns the pseudo-code lines of the function starting at byte offset `entry`."""
    seen = instrs if instrs is not None else D.reachable(ssb, [entry])
    # find the function's instruction extent: follow control flow from entry (calls are not followed)
    body = {}
    todo = [entry]
    while todo:
        o = todo.pop()
        while o in seen and o not in body:
            ins = seen[o]
            body[o] = ins
            if ins.op == 0x05:
                break
            if ins.op == 0x07:
                todo.append(ins.target())
                if ins.b1 not in (1, 2, 3):
                    break
            elif ins.op == 0x08:
                todo.append(o + 4 + ins.s * 4)
            o += ins.size
    labels = set()
    for ins in body.values():
        if ins.op == 0x07:
            labels.add(ins.target())
        elif ins.op == 0x08:
            labels.add(ins.off + 4 + ins.s * 4)
    out = []
    stack = []
    last = None
    ntemp = [0]
    state_at = {}   # label -> depth of materialised stack
    locals_ = [0]

    def tmp():
        ntemp[0] += 1
        return "t%d" % ntemp[0]

    def flush():
        """Materialise the expression stack into named slots (stack-0 is the deepest)."""
        for i, e in enumerate(stack):
            name = "s%d" % i
            if e != name:
                out.append("    %s = %s;" % (name, e))
                stack[i] = name

    for o in sorted(body):
        ins = body[o]
        if o in labels:
            flush()
            out.append("%s:" % _label(o))
            depth = state_at.get(o)
            if depth is not None and len(stack) != depth:
                stack[:] = ["s%d" % i for i in range(depth)]
        op = ins.op
        if op == 0x06:
            locals_[0] += ins.s
            out.append("    // alloc %d" % ins.s)
        elif op == 0x01:
            args = [stack.pop() for _ in range(ins.b1)]
            t = tmp()
            out.append("    %s = %s(%s);" % (t, native_name(ins.group, ins.fn), ", ".join(args)))
            last = t
        elif op == 0x03:
            tgt = ins.target()
            nargs = arg_count.get(tgt, 0) if arg_count else 0
            args = [stack.pop() for _ in range(nargs)]
            t = tmp()
            out.append("    %s = func_%06x(%s);" % (t, tgt, ", ".join(args)))
            last = t
        elif op == 0x10:
            stack.append(last if last else "result")
        elif op == 0x0E:
            stack.append(str(ins.s))
        elif op == 0x0F:
            fl = struct.unpack("<f", struct.pack("<i", ins.imm))[0]
            note = "" if abs(fl) < 1e-20 or abs(fl) > 1e20 else "/*%gf*/" % fl
            stack.append("%d%s" % (ins.imm, note))
        elif op == 0x09:
            stack.append(("arg%d" % ins.s) if ins.s >= 0 else ("loc%d" % -ins.s))
        elif op == 0x0A:
            v = stack.pop()
            name = ("arg%d" % ins.s) if ins.s >= 0 else ("loc%d" % -ins.s)
            out.append("    %s %s %s;" % (name, {0: "=", 1: "+=", 2: "-="}.get(ins.b1, "?="), v))
        elif op in (0x12, 0x13, 0x14, 0x15, 0x16, 0x18, 0x19, 0x1A, 0x1E):
            s0 = stack.pop()
            s1 = stack.pop()
            if ins.b1 and op in (0x13, 0x15, 0x16, 0x1E):
                s0, s1 = s1, s0
            sym = {0x12: "+", 0x13: "-", 0x14: "*", 0x15: "/", 0x16: "%", 0x18: "&", 0x19: "|", 0x1A: "^"}.get(op)
            if op == 0x1E:
                sym = {1: ">>", 2: ">>>"}.get(ins.s, "<<")
            stack.append("(%s %s %s)" % (s1, sym, s0))
        elif op == 0x17:
            s0 = stack.pop()
            s1 = stack.pop()
            if ins.b1:
                s0, s1 = s1, s0
            if ins.s == 6:
                stack.append("((%s & %s) == %s)" % (s0, s1, s0))
            else:
                stack.append("(%s %s %s)" % (s1, _CMP.get(ins.s if 1 <= ins.s <= 5 else 0), s0))
        elif op == 0x1B:
            stack.append("~%s" % stack.pop())
        elif op == 0x1C:
            stack.append("-%s" % stack.pop())
        elif op == 0x1D:
            stack.append("!%s" % stack.pop())
        elif op == 0x1F:
            stack.append("&data_%04x" % ins.target())
        elif op == 0x11:
            stack.append('str@%d' % ins.s)
        elif op == 0x0D:
            for _ in range(ins.s):
                if stack:
                    stack.pop()
            out.append("    // pop %d" % ins.s)
        elif op == 0x07:
            tgt = ins.target()
            if ins.b1 == 0:
                flush()
                state_at[tgt] = len(stack)
                out.append("    goto %s;" % _label(tgt))
            else:
                c = stack.pop()
                flush()
                state_at[tgt] = len(stack)
                cond = c if ins.b1 == 1 else "!(%s)" % c
                out.append("    if (%s) goto %s;%s" % (cond, _label(tgt), "  // then yield" if ins.b1 == 3 else ""))
        elif op == 0x08:
            tgt = o + 4 + ins.s * 4
            v = stack[-1]
            flush()
            state_at[tgt] = len(stack)
            out.append("    if (%s != %d) goto %s;  // case %d" % (v, ins.b1, _label(tgt), ins.b1))
            stack.pop()  # the matching arm pops the value
        elif op == 0x05:
            val = ""
            if ins.b1:
                val = " " + stack.pop()
            out.append("    return%s;  // pops %d" % (val, ins.s))
        else:
            out.append("    // %s" % D.text(ins))
    return out


def decompile(ssb, only=None):
    seen = D.reachable(ssb)
    _, sig = D.verify_stack(ssb)
    entries = {off for _, _, off in ssb.exports()} | {i.target() for i in seen.values() if i.op == 0x03}
    lines = []
    for e in sorted(entries):
        if only and e not in only:
            continue
        lines.append("func_%06x(%d args) {" % (e, sig.get(e, 0)))
        lines += decompile_function(ssb, e, seen, sig)
        lines.append("}")
        lines.append("")
    return "\n".join(lines)


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    ssb = D.Ssb(open(argv[0], "rb").read())
    only = {int(x, 16) for x in argv[1:]} or None
    print(decompile(ssb, only))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
