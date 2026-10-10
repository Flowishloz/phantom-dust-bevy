"""Reference interpreter for the program's AI scripts (`Data/com/ai/*.ssb`), our own tool.

A small, readable implementation of the VM described in engine/docs/FORMATS-CPU.md section 2, meant as a
test oracle for the Rust VM (pd_formats) and for exploring what a script does against a fake world. It is NOT
a game component. `Host` supplies the native calls (group 0 library calls are built in). Read-only.

  vm = Vm(open("eneprog006.ssb","rb").read(), host)
  vm.tick()      # one 30 Hz entry: call `main`, run until it returns

Semantics follow the doc: 64-bit signed values, call frame base = sp at the call, arguments pushed right to left
(a0 = top of stack), natives read the low 32 bits of each argument slot and their result goes to the result
register, `ret pop=N` pops args + locals. Pointers pushed by op 0x1f are plain byte offsets into the file image.
"""
import struct

import ssb_dis

MASK64 = (1 << 64) - 1


def s64(v):
    v &= MASK64
    return v - (1 << 64) if v >> 63 else v


def s32(v):
    v &= 0xFFFFFFFF
    return v - (1 << 32) if v >> 31 else v


class VmFault(Exception):
    pass


class Host:
    """Native-call provider. `native(group, fn, args)` gets the argument words a0, a1, ... as signed 64-bit ints."""

    def native(self, group, fn, args):
        raise NotImplementedError


class Vm:
    def __init__(self, image, host, budget=50000):
        self.ssb = ssb_dis.Ssb(image)
        self.image = image
        self.host = host
        self.budget = budget
        self.entry = self.ssb.find_export("main")
        if self.entry is None:
            raise VmFault("no main export")
        self.calls_log = []  # (group, fn, args, result) of the last tick

    # ---- memory / natives ------------------------------------------------------------------
    def _lib(self, fn, a):
        if fn == 16:  # g0.f16 tbl(ptr, idx): sign-extended i32 at ptr + 4*idx
            return s32(struct.unpack_from("<i", self.image, a[0] + 4 * a[1])[0])
        raise VmFault("group 0 function %d not implemented" % fn)

    # ---- one tick ---------------------------------------------------------------------------
    def tick(self):
        """Run `main` once from the top (the script is re-entered every tick; state lives in the host's registers)."""
        self.calls_log = []
        img = self.image
        code_end = self.ssb.func_table
        stack = []                    # top = end of list
        calls = []                    # (return pc, frame base, ) ; main's return ends the tick
        pc = self.entry
        base = 0
        result = 0
        steps = self.budget
        calls.append((None, 0))
        base = 0
        while True:
            if not 0 <= pc < code_end:
                raise VmFault("pc %#x outside the code area" % pc)
            if steps <= 0:
                return "budget"
            steps -= 1
            (w,) = struct.unpack_from("<I", img, pc)
            op, b1, s = w & 0xFF, (w >> 8) & 0xFF, ssb_dis.sx(w >> 16, 16)
            nxt = pc + 4
            if op == 0x00:
                pass
            elif op == 0x01:
                group, fn = w >> 28, (w >> 16) & 0xFFF
                args = [stack[-1 - i] for i in range(b1)] if b1 else []
                if group == 0:
                    result = self._lib(fn, args)
                else:
                    a32 = [s32(x) for x in args]
                    result = self.host.native(group, fn, a32)
                result = s64(result if result is not None else 0)
                self.calls_log.append((group, fn, tuple(s32(x) for x in args), result))
                if b1:
                    del stack[len(stack) - b1:]
            elif op == 0x03:
                calls.append((nxt, base))
                base = len(stack)
                nxt = nxt + 4 * s
            elif op == 0x05:
                if b1:
                    result = stack.pop()
                if s:
                    del stack[len(stack) - s:]
                ret_pc, base = calls.pop()
                if ret_pc is None:
                    return "returned"
                nxt = ret_pc
            elif op == 0x06:
                stack.extend([0] * s)
            elif op == 0x07:
                if b1 == 0:
                    nxt = nxt + 4 * s
                else:
                    v = stack.pop()
                    if (b1 == 1 and v != 0) or (b1 == 2 and v == 0):
                        nxt = nxt + 4 * s
                    elif b1 == 3:
                        raise VmFault("yielding branch not supported")
            elif op == 0x08:
                if stack[-1] == b1:
                    stack.pop()
                else:
                    nxt = nxt + 4 * s
            elif op == 0x09:
                stack.append(stack[base - 1 - s])
            elif op == 0x0A:
                v = stack.pop()
                i = base - 1 - s
                if b1 == 0:
                    stack[i] = v
                elif b1 == 1:
                    stack[i] = s64(stack[i] + v)
                elif b1 == 2:
                    stack[i] = s64(stack[i] - v)
                else:
                    stack.append(v)
            elif op == 0x0D:
                if s:
                    del stack[len(stack) - s:]
            elif op == 0x0E:
                stack.append(s)
            elif op == 0x0F:
                stack.append(struct.unpack_from("<i", img, nxt)[0])
                nxt += 4
            elif op == 0x10:
                stack.append(result)
            elif op == 0x11:
                stack.append(self.ssb.string_pool + s)
            elif op in (0x12, 0x13, 0x14, 0x15, 0x16, 0x18, 0x19, 0x1A):
                s0 = stack.pop()
                s1 = stack.pop()
                if op in (0x13, 0x15, 0x16) and b1:
                    s0, s1 = s1, s0
                if op == 0x12:
                    r = s1 + s0
                elif op == 0x13:
                    r = s1 - s0
                elif op == 0x14:
                    r = s1 * s0
                elif op == 0x15:
                    if s0 == 0:
                        raise VmFault("division by zero")
                    r = abs(s1) // abs(s0) * (1 if (s1 < 0) == (s0 < 0) else -1)
                elif op == 0x16:
                    if s0 == 0:
                        raise VmFault("division by zero")
                    r = s1 - s0 * (abs(s1) // abs(s0) * (1 if (s1 < 0) == (s0 < 0) else -1))
                elif op == 0x18:
                    r = s1 & s0
                elif op == 0x19:
                    r = s1 | s0
                else:
                    r = s1 ^ s0
                stack.append(s64(r))
            elif op == 0x17:
                s0 = stack.pop()
                s1 = stack.pop()
                if b1:
                    s0, s1 = s1, s0
                k = s if 1 <= s <= 6 else 0
                r = {0: s1 == s0, 1: s1 != s0, 2: s1 > s0, 3: s1 < s0, 4: s1 >= s0, 5: s1 <= s0,
                     6: (s0 & s1) == s0}[k]
                stack.append(1 if r else 0)
            elif op == 0x1B:
                stack[-1] = s64(~stack[-1])
            elif op == 0x1C:
                stack[-1] = s64(-stack[-1])
            elif op == 0x1D:
                stack[-1] = 1 if stack[-1] == 0 else 0
            elif op == 0x1E:
                s0 = stack.pop()
                s1 = stack.pop()
                if b1:
                    s0, s1 = s1, s0
                n = s0 & 63
                if s == 1:
                    r = s1 >> n
                elif s == 2:
                    r = (s1 & MASK64) >> n
                else:
                    r = s1 << n
                stack.append(s64(r))
            elif op == 0x1F:
                stack.append(pc + 4 + 4 * (ssb_dis.sx(w, 32) >> 8))
            else:
                raise VmFault("invalid opcode %#x at %#x" % (op, pc))
            pc = nxt
