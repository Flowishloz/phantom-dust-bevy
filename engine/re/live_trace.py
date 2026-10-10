#!/usr/bin/env python3
"""Live "ground-truth recorder" for the ORIGINAL Phantom Dust PC game (our own code).

A minimal Win32 debugger (Python stdlib + ctypes only). It attaches to PDUWP.exe, plants
software breakpoints (0xCC) at functions you name with their GHIDRA addresses, logs the
arguments every time one is called, and lets the game carry on. Nothing is sent to the game.

Usage (run from anywhere):
  python live_trace.py --bp 0x140276330:sound --seconds 60 --summary
  python live_trace.py --bp 0x140276330:sound --bp 0x140261a00:capture --bp 0x14014fd10:erase
  python live_trace.py --bp 0x14024ec80:tick:rcx,rdx --quiet --seconds 5
  python live_trace.py --restore --bp 0x140276330     (repair a game left with a planted 0xCC)

--bp ADDR:NAME[:ARGS]   ADDR is a Ghidra address (image base 0x140000000). ARGS is a comma list
                        of rcx/edx/r8d/... registers (e* / r*d = low 32 bits) or stack slots
                        [rsp+0x28] (qword) / d[rsp+0x28] (dword); a trailing * marks a summary key.
                        With no ARGS, NAME picks a preset (sound, capture, erase) or rcx,rdx,r8,r9.
--pid N / auto-finds PDUWP.exe     --seconds N (else until Ctrl+C / console closed)
--out FILE.csv  (default %USERPROFILE%/.pdtools/trace/sound_<timestamp>.csv, outside the repo)
--summary       count per (bp, key args) at exit      --quiet  do not echo hits to the console

Safety: DebugSetProcessKillOnExit(False) straight after attaching, so ending never kills the
game. On any exit path (finish, Ctrl+C, closing the console, exception) every original byte is
written back, queued breakpoint events are drained, then DebugActiveProcessStop detaches.
"""
import argparse
import csv
import ctypes
import ctypes.wintypes as wt
import datetime
import os
import struct
import sys
import threading
import time

GHIDRA_BASE = 0x140000000
DEFAULT_OUT_DIR = os.path.expandvars(r"%USERPROFILE%\.pdtools\trace")

# --------------------------------------------------------------------------- pure parts

# CONTEXT (x64) offsets of the registers we log.
REG64 = {
    "rax": 0x78, "rcx": 0x80, "rdx": 0x88, "rbx": 0x90, "rsp": 0x98, "rbp": 0xA0,
    "rsi": 0xA8, "rdi": 0xB0, "r8": 0xB8, "r9": 0xC0, "r10": 0xC8, "r11": 0xD0,
    "r12": 0xD8, "r13": 0xE0, "r14": 0xE8, "r15": 0xF0, "rip": 0xF8,
}
REG32 = {"eax": "rax", "ecx": "rcx", "edx": "rdx", "ebx": "rbx", "esp": "rsp",
         "ebp": "rbp", "esi": "rsi", "edi": "rdi"}
for _n in range(8, 16):
    REG32["r%dd" % _n] = "r%d" % _n

CTX_SIZE = 1232
CTX_FLAGS_OFF = 0x30
CTX_EFLAGS_OFF = 0x44
CTX_RSP_OFF = 0x98
CTX_RIP_OFF = 0xF8
CONTEXT_AMD64 = 0x100000
CONTEXT_CONTROL = CONTEXT_AMD64 | 0x1
CONTEXT_INTEGER = CONTEXT_AMD64 | 0x2
CONTEXT_FLOATING_POINT = CONTEXT_AMD64 | 0x8
XMM_OFF = 0x1A0                 # CONTEXT.Xmm0 (16 bytes each)
TRAP_FLAG = 0x100

PRESETS = {
    # FUN_140276330(bank, id, id, 0x1000, 0x1000, &pos|0)
    "sound": "ecx*,edx*,r8d,r9d,d[rsp+0x28],[rsp+0x30]",
    # FUN_140261a00(button, skill, 1, play_sound)
    "capture": "ecx,edx*,r8d,r9d",
    # FUN_14014fd10(mgr, player, slot, mode)
    "erase": "rcx,edx*,r8d*,r9d*",
}
DEFAULT_ARGS = "rcx,rdx,r8,r9"


def ghidra_to_runtime(addr, base):
    return base + (addr - GHIDRA_BASE)


def runtime_to_ghidra(addr, base, size=None):
    """Runtime address -> Ghidra address; None when outside [base, base+size)."""
    if addr < base or (size is not None and addr >= base + size):
        return None
    return addr - base + GHIDRA_BASE


class ArgSpec:
    def __init__(self, text):
        t = text.strip()
        self.key = t.endswith("*")
        if self.key:
            t = t[:-1]
        self.text = t
        self.kind = None
        if t.lower().startswith("xmm") and t[3:].isdigit() and int(t[3:]) < 16:
            # low 32-bit float lane of xmmN (needs the thread context taken with the floating-point flag)
            self.kind, self.reg, self.size, self.fmt = "xmm", int(t[3:]), 4, "f"
        elif t.lower() in REG64:
            self.kind, self.reg, self.size = "reg", t.lower(), 8
        elif t.lower() in REG32:
            self.kind, self.reg, self.size = "reg", REG32[t.lower()], 4
        else:
            # [reg+off] / [reg-off]: a qword at that address; d[..] dword, w[..] word, b[..] byte,
            # f[..] a 32-bit float (printed as a decimal). Any 64-bit register; [rsp+off] is the stack.
            size, fmt, body = 8, "u", t
            if body[:2] in ("d[", "w[", "b[", "f["):
                size = {"d": 4, "w": 2, "b": 1, "f": 4}[body[0]]
                fmt = "f" if body[0] == "f" else "u"
                body = body[1:]
            if body.startswith("[") and body.endswith("]"):
                inner = body[1:-1].replace(" ", "")
                for reg in sorted(REG64, key=len, reverse=True):
                    if inner.startswith(reg):
                        rest = inner[len(reg):]
                        off = 0 if rest == "" else (int(rest, 0) if rest[0] in "+-" else None)
                        if off is not None:
                            self.fmt = fmt
                            if reg == "rsp":
                                self.kind, self.off, self.size = "stack", off, size
                            else:
                                self.kind, self.reg, self.off, self.size = "deref", reg, off, size
                        break
        if self.kind is None:
            raise ValueError("bad arg spec %r (use rcx, edx, r8d, [rsp+0x28], d[rsp+0x28])" % text)
        self.name = t.replace(" ", "")

    def read(self, ctx, read_mem):
        """ctx: bytes of a CONTEXT; read_mem(addr, n) -> bytes or None."""
        if self.kind == "xmm":
            return struct.unpack_from("<f", ctx, XMM_OFF + 16 * self.reg)[0]
        if self.kind == "reg":
            v = struct.unpack_from("<Q", ctx, REG64[self.reg])[0]
            return v & 0xFFFFFFFF if self.size == 4 else v
        base = struct.unpack_from("<Q", ctx, CTX_RSP_OFF if self.kind == "stack" else REG64[self.reg])[0]
        raw = read_mem(base + self.off, self.size)
        if raw is None or len(raw) < self.size:
            return None
        if getattr(self, "fmt", "u") == "f":
            return struct.unpack("<f", raw)[0]
        return int.from_bytes(raw, "little")


class BpSpec:
    def __init__(self, text):
        parts = text.split(":", 2)
        if len(parts) < 2:
            parts.append("bp_%s" % parts[0])
        self.addr = int(parts[0], 0)
        self.name = parts[1]
        argtext = parts[2] if len(parts) == 3 and parts[2] else PRESETS.get(self.name, DEFAULT_ARGS)
        self.args = [ArgSpec(a) for a in argtext.split(",") if a.strip()]
        if not any(a.key for a in self.args):
            for a in self.args[:2]:
                a.key = True
        if not (GHIDRA_BASE <= self.addr < GHIDRA_BASE + 0x10000000):
            raise ValueError("address %#x is not a Ghidra address (base %#x)" % (self.addr, GHIDRA_BASE))


def fmt_val(v):
    if v is None:
        return ""
    if isinstance(v, float):
        return "%.3f" % v
    return "0x%x" % v if v >= 0x10000 else str(v)


def csv_header(n_args):
    return ["ms", "tid", "bp", "ret"] + ["a%d" % i for i in range(n_args)]


def csv_row(ms, tid, bp_name, ret, values, n_args):
    vals = [fmt_val(v) for v in values] + [""] * (n_args - len(values))
    return ["%.1f" % ms, str(tid), bp_name, ret] + vals


def fmt_ret(ret_runtime, base, size):
    g = runtime_to_ghidra(ret_runtime, base, size) if ret_runtime is not None else None
    if g is not None:
        return "0x%x" % g
    return "ext:0x%x" % ret_runtime if ret_runtime is not None else ""


def summary_key(bp, values):
    return (bp.name,) + tuple(fmt_val(v) for a, v in zip(bp.args, values) if a.key)


def summary_lines(counts):
    lines = []
    for key, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])):
        lines.append("%6d  %s  %s" % (n, key[0], " ".join(key[1:])))
    return lines


def pe_rva_to_offset(pe_bytes, rva):
    """File offset of an RVA inside a PE image (None if not in a section)."""
    e_lfanew = struct.unpack_from("<I", pe_bytes, 0x3C)[0]
    nsec = struct.unpack_from("<H", pe_bytes, e_lfanew + 6)[0]
    opt_size = struct.unpack_from("<H", pe_bytes, e_lfanew + 20)[0]
    sec = e_lfanew + 24 + opt_size
    for i in range(nsec):
        vsize, va, rawsize, rawptr = struct.unpack_from("<IIII", pe_bytes, sec + 40 * i + 8)
        if va <= rva < va + max(vsize, rawsize):
            return rawptr + (rva - va)
    return None


# --------------------------------------------------------------------------- win32 plumbing

EXCEPTION_DEBUG_EVENT, CREATE_THREAD_DEBUG_EVENT, CREATE_PROCESS_DEBUG_EVENT = 1, 2, 3
EXIT_THREAD_DEBUG_EVENT, EXIT_PROCESS_DEBUG_EVENT, LOAD_DLL_DEBUG_EVENT = 4, 5, 6
EXC_BREAKPOINT, EXC_SINGLE_STEP = 0x80000003, 0x80000004
DBG_CONTINUE, DBG_NOT_HANDLED = 0x00010002, 0x80010001
PROCESS_ALL_ACCESS = 0x1FFFFF
THREAD_ALL_ACCESS = 0x1FFFFF


def _k32():
    k = ctypes.WinDLL("kernel32", use_last_error=True)
    V, B, D, H = ctypes.c_void_p, wt.BOOL, wt.DWORD, wt.HANDLE
    sig = {
        "OpenProcess": (H, [D, B, D]), "OpenThread": (H, [D, B, D]), "CloseHandle": (B, [H]),
        "ReadProcessMemory": (B, [H, V, V, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)]),
        "WriteProcessMemory": (B, [H, V, V, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)]),
        "VirtualProtectEx": (B, [H, V, ctypes.c_size_t, D, ctypes.POINTER(D)]),
        "FlushInstructionCache": (B, [H, V, ctypes.c_size_t]),
        "DebugActiveProcess": (B, [D]), "DebugActiveProcessStop": (B, [D]),
        "DebugSetProcessKillOnExit": (B, [B]),
        "WaitForDebugEvent": (B, [V, D]), "ContinueDebugEvent": (B, [D, D, D]),
        "GetThreadContext": (B, [H, V]), "SetThreadContext": (B, [H, V]),
        "SuspendThread": (D, [H]), "ResumeThread": (D, [H]),
        "CreateToolhelp32Snapshot": (H, [D, D]),
        "Process32FirstW": (B, [H, V]), "Process32NextW": (B, [H, V]),
        "Module32FirstW": (B, [H, V]), "GetExitCodeProcess": (B, [H, ctypes.POINTER(D)]),
    }
    for name, (res, args) in sig.items():
        f = getattr(k, name)
        f.restype, f.argtypes = res, args
    return k


class ProcEntry(ctypes.Structure):
    _fields_ = [("dwSize", wt.DWORD), ("cntUsage", wt.DWORD), ("th32ProcessID", wt.DWORD),
                ("th32DefaultHeapID", ctypes.c_size_t), ("th32ModuleID", wt.DWORD),
                ("cntThreads", wt.DWORD), ("th32ParentProcessID", wt.DWORD),
                ("pcPriClassBase", ctypes.c_long), ("dwFlags", wt.DWORD),
                ("szExeFile", ctypes.c_wchar * 260)]


class ModEntry(ctypes.Structure):
    _fields_ = [("dwSize", wt.DWORD), ("th32ModuleID", wt.DWORD), ("th32ProcessID", wt.DWORD),
                ("GlblcntUsage", wt.DWORD), ("ProccntUsage", wt.DWORD),
                ("modBaseAddr", ctypes.c_void_p), ("modBaseSize", wt.DWORD),
                ("hModule", wt.HANDLE), ("szModule", ctypes.c_wchar * 256),
                ("szExePath", ctypes.c_wchar * 260)]


class AlignedCtx:
    """A 1232-byte CONTEXT buffer whose address is 16-byte aligned (x64 requirement)."""

    def __init__(self):
        self._raw = ctypes.create_string_buffer(CTX_SIZE + 16)
        addr = ctypes.addressof(self._raw)
        self.off = (-addr) % 16
        self.addr = addr + self.off

    def clear(self, flags):
        ctypes.memset(self.addr, 0, CTX_SIZE)
        struct.pack_into("<I", self._raw, self.off + CTX_FLAGS_OFF, flags)

    def bytes(self):
        return bytes(self._raw[self.off:self.off + CTX_SIZE])

    def set_u64(self, off, v):
        struct.pack_into("<Q", self._raw, self.off + off, v)

    def set_u32(self, off, v):
        struct.pack_into("<I", self._raw, self.off + off, v)

    def get_u32(self, off):
        return struct.unpack_from("<I", self._raw, self.off + off)[0]


def find_pdu_process(k, name="PDUWP.exe"):
    snap = k.CreateToolhelp32Snapshot(0x2, 0)
    pids = []
    e = ProcEntry()
    e.dwSize = ctypes.sizeof(e)
    ok = k.Process32FirstW(snap, ctypes.byref(e))
    while ok:
        if e.szExeFile.lower() == name.lower():
            pids.append(e.th32ProcessID)
        ok = k.Process32NextW(snap, ctypes.byref(e))
    k.CloseHandle(snap)
    return pids


def module_base(k, pid):
    """(base, size) of the process's main module (first module in the snapshot)."""
    snap = k.CreateToolhelp32Snapshot(0x8 | 0x10, pid)
    if snap in (None, ctypes.c_void_p(-1).value):
        raise OSError("module snapshot failed (error %d)" % ctypes.get_last_error())
    e = ModEntry()
    e.dwSize = ctypes.sizeof(e)
    if not k.Module32FirstW(snap, ctypes.byref(e)):
        k.CloseHandle(snap)
        raise OSError("Module32First failed (error %d)" % ctypes.get_last_error())
    base, size = e.modBaseAddr, e.modBaseSize
    k.CloseHandle(snap)
    return base, size


class Bp:
    def __init__(self, spec, addr, orig):
        self.spec, self.addr, self.orig = spec, addr, orig
        self.armed = False
        self.stepping = set()  # tids currently stepping over this bp
        self.hits = 0


class Tracer:
    def __init__(self, pid, specs, out_path, exe_path, echo=True, summary=False):
        self.k = _k32()
        self.pid, self.specs, self.out_path, self.exe_path = pid, specs, out_path, exe_path
        self.echo, self.want_summary = echo, summary
        self.hproc = None
        self.attached = False
        self.bps = {}          # runtime addr -> Bp
        self.threads = {}      # tid -> handle
        self.suspended = []    # handles suspended for the current step
        self.step_owner = {}   # tid -> Bp list stepping
        self.ctx = AlignedCtx()
        self.evbuf = ctypes.create_string_buffer(256)
        self.t0 = time.monotonic()
        self.counts = {}
        self.ev_counts = {}
        self.stop = threading.Event()
        self.done = threading.Event()
        self.writer = None
        self.fh = None
        self.base = self.size = None
        self.first_break_seen = False

    # ---- memory
    def read(self, addr, n):
        buf = ctypes.create_string_buffer(n)
        got = ctypes.c_size_t(0)
        if not self.k.ReadProcessMemory(self.hproc, addr, buf, n, ctypes.byref(got)):
            return None
        return buf.raw[:got.value]

    def write(self, addr, data):
        got = ctypes.c_size_t(0)
        ok = self.k.WriteProcessMemory(self.hproc, addr, data, len(data), ctypes.byref(got))
        if not ok:
            old = wt.DWORD(0)
            if not self.k.VirtualProtectEx(self.hproc, addr, len(data), 0x40, ctypes.byref(old)):
                raise OSError("cannot write process memory at %#x (error %d)" % (addr, ctypes.get_last_error()))
            try:
                ok = self.k.WriteProcessMemory(self.hproc, addr, data, len(data), ctypes.byref(got))
            finally:
                self.k.VirtualProtectEx(self.hproc, addr, len(data), old.value, ctypes.byref(wt.DWORD(0)))
            if not ok:
                raise OSError("WriteProcessMemory failed at %#x" % addr)
        self.k.FlushInstructionCache(self.hproc, addr, len(data))

    # ---- threads
    def thread_handle(self, tid):
        h = self.threads.get(tid)
        if h is None:
            h = self.k.OpenThread(THREAD_ALL_ACCESS, False, tid)
            if h:
                self.threads[tid] = h
        return h

    ctx_flags = CONTEXT_CONTROL | CONTEXT_INTEGER      # a subclass adds CONTEXT_FLOATING_POINT to read xmm args

    def get_ctx(self, tid, flags=None):
        flags = self.ctx_flags if flags is None else flags
        h = self.thread_handle(tid)
        if not h:
            return None
        self.ctx.clear(flags)
        if not self.k.GetThreadContext(h, self.ctx.addr):
            return None
        return h

    def suspend_others(self, tid):
        for t, h in list(self.threads.items()):
            if t != tid and h:
                if self.k.SuspendThread(h) != 0xFFFFFFFF:
                    self.suspended.append(h)

    def resume_others(self):
        for h in self.suspended:
            self.k.ResumeThread(h)
        self.suspended = []

    # ---- breakpoints
    def arm(self, bp):
        if not bp.armed and not bp.stepping:
            self.write(bp.addr, b"\xcc")
            bp.armed = True

    def disarm(self, bp):
        if bp.armed:
            self.write(bp.addr, bytes([bp.orig]))
            bp.armed = False

    def plant_all(self):
        exe = None
        if self.exe_path and os.path.exists(self.exe_path):
            exe = open(self.exe_path, "rb").read()
        else:
            print("warning: %s not found - cannot cross-check the image base" % self.exe_path)
        for s in self.specs:
            addr = ghidra_to_runtime(s.addr, self.base)
            cur = self.read(addr, 1)
            if cur is None:
                raise OSError("cannot read %#x (ghidra %#x)" % (addr, s.addr))
            orig = cur[0]
            if exe is not None:
                off = pe_rva_to_offset(exe, s.addr - GHIDRA_BASE)
                if off is None:
                    raise ValueError("ghidra %#x is not inside a file section" % s.addr)
                want = exe[off]
                if orig != want and orig != 0xCC:
                    raise ValueError("byte at ghidra %#x is %02x, exe says %02x: wrong base/address" % (s.addr, orig, want))
                orig = want
            elif orig == 0xCC:
                raise ValueError("%#x already holds 0xCC (stale trace?) and no exe to restore from" % s.addr)
            bp = Bp(s, addr, orig)
            self.bps[addr] = bp
        for bp in self.bps.values():
            self.arm(bp)

    def restore_all(self):
        for bp in self.bps.values():
            try:
                self.disarm(bp)
            except Exception as e:  # keep going: restore as many as possible
                print("restore failed at %#x: %s" % (bp.addr, e))

    # ---- logging
    def log_hit(self, bp, tid, ctx_bytes):
        ms = (time.monotonic() - self.t0) * 1000.0
        vals = [a.read(ctx_bytes, self.read) for a in bp.spec.args]
        rsp = struct.unpack_from("<Q", ctx_bytes, CTX_RSP_OFF)[0]
        raw = self.read(rsp, 8)
        ret = struct.unpack("<Q", raw)[0] if raw and len(raw) == 8 else None
        row = csv_row(ms, tid, bp.spec.name, fmt_ret(ret, self.base, self.size), vals, self.nargs)
        self.writer.writerow(row)
        self.fh.flush()
        bp.hits += 1
        k = summary_key(bp.spec, vals)
        self.counts[k] = self.counts.get(k, 0) + 1
        if self.echo:
            print("%9.1f ms  t%-6d %-8s ret=%-12s %s" % (ms, tid, bp.spec.name, row[3],
                  " ".join("%s=%s" % (a.name, fmt_val(v)) for a, v in zip(bp.spec.args, vals))))

    # ---- event loop
    def cont(self, ev_pid, tid, status):
        self.k.ContinueDebugEvent(ev_pid, tid, status)

    def handle_exception(self, tid, draining):
        b = self.evbuf.raw
        code = struct.unpack_from("<I", b, 16)[0]
        addr = struct.unpack_from("<Q", b, 32)[0]
        if code == EXC_BREAKPOINT:
            bp = self.bps.get(addr)
            if bp is not None:
                h = self.get_ctx(tid)
                if h:
                    c = self.ctx.bytes()
                    if not draining:
                        self.log_hit(bp, tid, c)
                        redirect = getattr(self, "redirect", None)
                        if redirect is not None:
                            # a subclass asked to run something else on this thread (move_recorder's
                            # remote calls): it has changed self.ctx; keep the breakpoint armed and go
                            self.redirect = None
                            self.k.SetThreadContext(h, self.ctx.addr)
                            return DBG_CONTINUE
                    self.ctx.set_u64(CTX_RIP_OFF, addr)
                    if draining:
                        self.k.SetThreadContext(h, self.ctx.addr)
                        return DBG_CONTINUE
                    # step over: original byte back, others frozen, trap flag on
                    bp.stepping.add(tid)
                    self.disarm(bp)
                    self.suspend_others(tid)
                    self.ctx.set_u32(CTX_EFLAGS_OFF, self.ctx.get_u32(CTX_EFLAGS_OFF) | TRAP_FLAG)
                    self.k.SetThreadContext(h, self.ctx.addr)
                    self.step_owner.setdefault(tid, []).append(bp)
                    return DBG_CONTINUE
            if not self.first_break_seen:
                self.first_break_seen = True  # the attach breakpoint
                return DBG_CONTINUE
            return DBG_NOT_HANDLED
        if code == EXC_SINGLE_STEP and tid in self.step_owner:
            h = self.get_ctx(tid)
            if h:
                self.ctx.set_u32(CTX_EFLAGS_OFF, self.ctx.get_u32(CTX_EFLAGS_OFF) & ~TRAP_FLAG)
                self.k.SetThreadContext(h, self.ctx.addr)
            self.finish_step(tid, draining)
            return DBG_CONTINUE
        return DBG_NOT_HANDLED

    def finish_step(self, tid, draining):
        for bp in self.step_owner.pop(tid, []):
            bp.stepping.discard(tid)
            if not draining and self.attached:
                self.arm(bp)
        if not self.step_owner:
            self.resume_others()

    def pump(self, timeout_ms, draining=False):
        """Handle one debug event. Returns False on timeout, None when the process exited."""
        if not self.k.WaitForDebugEvent(self.evbuf, timeout_ms):
            return False
        b = self.evbuf.raw
        code, pid, tid = struct.unpack_from("<III", b, 0)
        self.ev_counts[code] = self.ev_counts.get(code, 0) + 1
        status = DBG_CONTINUE
        if code == EXCEPTION_DEBUG_EVENT:
            status = self.handle_exception(tid, draining)
        else:
            if tid in self.step_owner and code == EXIT_THREAD_DEBUG_EVENT:
                self.finish_step(tid, draining)
            if code in (CREATE_PROCESS_DEBUG_EVENT, LOAD_DLL_DEBUG_EVENT):
                hfile = struct.unpack_from("<Q", b, 16)[0]
                if hfile:
                    self.k.CloseHandle(hfile)
            if code == CREATE_PROCESS_DEBUG_EVENT:
                self.thread_handle(tid)
            elif code == CREATE_THREAD_DEBUG_EVENT:
                self.thread_handle(tid)
            elif code == EXIT_THREAD_DEBUG_EVENT:
                h = self.threads.pop(tid, None)
                if h:
                    self.k.CloseHandle(h)
            elif code == EXIT_PROCESS_DEBUG_EVENT:
                self.attached = False
                self.cont(pid, tid, DBG_CONTINUE)
                return None
        self.cont(pid, tid, status)
        return True

    # ---- run
    def run(self, seconds):
        os.makedirs(os.path.dirname(os.path.abspath(self.out_path)), exist_ok=True)
        self.nargs = max(len(s.args) for s in self.specs)
        self.fh = open(self.out_path, "w", newline="")
        self.writer = csv.writer(self.fh)
        self.writer.writerow(csv_header(self.nargs))
        self.hproc = self.k.OpenProcess(PROCESS_ALL_ACCESS, False, self.pid)
        if not self.hproc:
            raise OSError("OpenProcess failed (error %d)" % ctypes.get_last_error())
        self.base, self.size = module_base(self.k, self.pid)
        print("pid %d  image base %#x  (ghidra 0x140000000)" % (self.pid, self.base))
        if not self.k.DebugActiveProcess(self.pid):
            raise OSError("DebugActiveProcess failed (error %d)" % ctypes.get_last_error())
        self.attached = True
        self.k.DebugSetProcessKillOnExit(False)  # FIRST thing: never take the game down with us
        self.t0 = time.monotonic()
        try:
            # drain the attach events (create process/threads/dlls + initial breakpoint)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                r = self.pump(300)
                if r is False and self.first_break_seen:
                    break
                if r is None:
                    raise RuntimeError("process exited during attach")
            self.plant_all()
            print("tracing %d breakpoint(s) -> %s" % (len(self.bps), self.out_path))
            end = time.monotonic() + seconds if seconds else None
            while not self.stop.is_set() and (end is None or time.monotonic() < end):
                r = self.pump(200)
                if r is None:
                    print("the game exited")
                    break
        finally:
            self.shutdown()

    def shutdown(self):
        self.stop.set()
        try:
            self.restore_all()
            # events queued before the bytes went back: fix up RIP and let the threads go
            quiet = 0
            t_end = time.monotonic() + 3.0
            while self.attached and quiet < 3 and time.monotonic() < t_end:
                r = self.pump(100, draining=True)
                if r is False:
                    quiet += 1
                elif r is None:
                    break
                else:
                    quiet = 0
            self.resume_others()
        finally:
            if self.attached:
                self.k.DebugActiveProcessStop(self.pid)
                self.attached = False
                print("detached; original bytes restored")
            for h in self.threads.values():
                self.k.CloseHandle(h)
            self.threads.clear()
            if self.hproc:
                self.k.CloseHandle(self.hproc)
                self.hproc = None
            if self.fh:
                self.fh.close()
            if self.want_summary:
                print("\nSummary (hits per bp / key args):")
                for ln in summary_lines(self.counts) or ["  (no hits)"]:
                    print(ln)
                print("total hits:", sum(b.hits for b in self.bps.values()))
                print("debug events by code:", dict(sorted(self.ev_counts.items())))
            self.done.set()


def restore_only(pid, specs, exe_path):
    """Write the original byte back at each bp address if it still holds 0xCC."""
    k = _k32()
    h = k.OpenProcess(PROCESS_ALL_ACCESS, False, pid)
    base, _ = module_base(k, pid)
    exe = open(exe_path, "rb").read()
    t = Tracer.__new__(Tracer)
    t.k, t.hproc = k, h
    for s in specs:
        addr = ghidra_to_runtime(s.addr, base)
        cur = t.read(addr, 1)
        want = exe[pe_rva_to_offset(exe, s.addr - GHIDRA_BASE)]
        if cur and cur[0] == 0xCC:
            t.write(addr, bytes([want]))
            print("restored %#x -> %02x" % (s.addr, want))
        else:
            print("%#x is fine (%s)" % (s.addr, cur.hex() if cur else "unreadable"))
    k.CloseHandle(h)


def install_ctrl_handler(tracer):
    """Ctrl+C / Ctrl+Break / closing the console: stop, then hold the OS off until we detached."""
    HandlerRoutine = ctypes.WINFUNCTYPE(wt.BOOL, wt.DWORD)

    def handler(ctrl_type):
        tracer.stop.set()
        tracer.done.wait(4.0)
        return True

    cb = HandlerRoutine(handler)
    ctypes.windll.kernel32.SetConsoleCtrlHandler(cb, True)
    return cb  # keep a reference


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bp", action="append", required=True, help="ADDR[:NAME[:ARGS]] (ghidra address)")
    ap.add_argument("--pid", type=int)
    ap.add_argument("--seconds", type=float, default=0)
    ap.add_argument("--out")
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--quiet", action="store_true")
    ap.add_argument("--restore", action="store_true", help="only repair stale 0xCC bytes, then exit")
    here = os.path.dirname(os.path.abspath(__file__))
    ap.add_argument("--exe", default=os.path.join(here, "..", "..", "game_files", "PDUWP.exe"))
    a = ap.parse_args(argv)
    specs = [BpSpec(t) for t in a.bp]
    k = _k32()
    pid = a.pid
    if not pid:
        pids = find_pdu_process(k)
        if not pids:
            print("PDUWP.exe is not running (launch it first)")
            return 2
        pid = pids[0]
    if a.restore:
        restore_only(pid, specs, a.exe)
        return 0
    out = a.out or os.path.join(DEFAULT_OUT_DIR, "sound_%s.csv" % datetime.datetime.now().strftime("%Y%m%d_%H%M%S"))
    tr = Tracer(pid, specs, out, os.path.normpath(a.exe), echo=not a.quiet, summary=a.summary)
    _keep = install_ctrl_handler(tr)
    try:
        tr.run(a.seconds)
    except KeyboardInterrupt:
        pass
    except Exception as e:
        print("trace failed: %s" % e)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
