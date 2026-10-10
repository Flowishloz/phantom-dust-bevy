#!/usr/bin/env python3
"""Move recorder for the ORIGINAL Phantom Dust PC game (our own code, Python stdlib + ctypes).

Records controlled, repeatable footage of one skill at a time from the Store version
(PDUWP.exe) without sending any OS input. Everything goes in by WRITING THE GAME'S OWN MEMORY
through the same Win32 debugger used by live_trace.py:

  * Controller input: the game polls Windows.Gaming.Input once per program tick in
    FUN_1401d1250 (-> FUN_14028ea10, per pad FUN_14028e710) and stores each pad as a 0x30-byte
    object at ghidra 0x1405a5ce0 + 0x30*pad: +0x1c button mask, +0x20/+0x24 left stick x/y,
    +0x28/+0x2c right stick x/y (signed, +-32767). We break on the instruction right after
    the poll returns (0x1401d12b0) and overwrite that object, so the rest of the tick (button
    edge detection FUN_14028eac0, the fighters' input block FUN_140256580, menus) sees our
    values exactly as if a pad had produced them.
  * Skills in the slots, the opponent and the camera are plain memory writes at the same hook.
  * The log (trace.csv) is written from the same hook, so every row is one program tick and
    lines up with the video by tick number (30 per second).

See engine/docs/MOVE-RECORDER.md for the addresses read, what is confirmed and how to run it.
Never run this against the game while Laurence is playing it.
"""
import argparse
import csv
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

TICK_HZ = 30.0

# ----------------------------------------------------------------------- addresses (ghidra)
HOOK_AFTER_POLL = 0x1401D12B0       # instruction after "call FUN_14028ea10" in FUN_1401d1250
PAD_OBJ = 0x1405A5CE0               # 0x30-byte pad objects (FUN_14028e710 fills them)
PAD_STRIDE = 0x30
PAD_MASK_OFF = 0x1C
PAD_STICK_OFF = 0x20                # LX, LY, RX, RY as int32
PAD_TABLE = 0x14059B800             # per-pad held / pressed / repeat tables (0x34 stride)
PAD_TABLE_STRIDE = 0x34
BATTLE_KIND = 0x1404BC9D4           # DAT_1404bc9d4 (0 = no battle)
FIGHTER_PTRS = 0x1404BC9D8          # DAT_1404bc9d8[player]
FIGHTER_POS = 0x12E40               # x, y, z floats (y up)
FIGHTER_STATE = 0x12F04
FIGHTER_CLIP = 0x12F00
FIGHTER_FACING = 0x12E68
FIGHTER_PAD_IDX = 0x14244

# ----------------------------------------------------------------------- pad bits
# Game-side button mask bits (the value stored at pad object +0x1c). Established from the
# conversion table at 0x1403dc710 (xinput-style bit -> game bit) and the Windows.Gaming.Input
# mapping at 0x1405bb4a0; the meaning of each bit in battle is confirmed live (see the docs).
BUTTONS = {
    "DPAD_UP": 0x1, "DPAD_DOWN": 0x2, "DPAD_LEFT": 0x4, "DPAD_RIGHT": 0x8,
    "A": 0x10, "B": 0x20, "X": 0x80, "Y": 0x100,
    "RB": 0x200, "LT": 0x400, "RT": 0x800,
    "START": 0x1000, "BACK": 0x2000,
    "LB": 0x1000000, "LTHUMB": 0x1000000, "RTHUMB": 0x2000000,
}
STICK_MAX = 32767


def button_mask(names):
    """OR of the named buttons (case-insensitive). Unknown names raise ValueError."""
    m = 0
    for n in names:
        key = n.strip().upper()
        if key not in BUTTONS:
            raise ValueError("unknown button %r (known: %s)" % (n, ", ".join(sorted(BUTTONS))))
        m |= BUTTONS[key]
    return m


def stick_int(v):
    """Float -1..1 -> the int the game stores (clamped)."""
    v = max(-1.0, min(1.0, float(v)))
    return int(round(v * STICK_MAX))


def pad_object_addr(pad):
    return PAD_OBJ + PAD_STRIDE * pad


def pack_pad(mask, lx=0, ly=0, rx=0, ry=0):
    """The 20 bytes written at pad object +0x1c: mask, LX, LY, RX, RY (int32 each)."""
    return struct.pack("<Iiiii", mask & 0xFFFFFFFF, lx, ly, rx, ry)


# ----------------------------------------------------------------------- pure: scripts
class ScriptError(ValueError):
    pass


EVENT_KINDS = ("hold", "press", "stick", "rstick", "release", "mark")


class Event:
    """One timeline entry: at frame `start`, for `length` frames, apply `kind`.

    hold/press: buttons (list of names). stick/rstick: x, y in -1..1. mark: a label only.
    """

    def __init__(self, start, length, kind, buttons=(), x=0.0, y=0.0, label=""):
        self.start, self.length, self.kind = start, length, kind
        self.buttons, self.x, self.y, self.label = tuple(buttons), x, y, label

    @property
    def end(self):
        return self.start + self.length

    def __repr__(self):
        return "Event(%d+%d %s %s %.2f,%.2f)" % (self.start, self.length, self.kind, self.buttons, self.x, self.y)


def secs_to_frames(s):
    """Seconds -> program ticks (30 Hz), rounded to the nearest tick, never negative."""
    f = int(round(float(s) * TICK_HZ))
    if f < 0:
        raise ScriptError("negative time %r" % s)
    return f


def parse_event(d):
    if not isinstance(d, dict):
        raise ScriptError("timeline entry must be an object, got %r" % (d,))
    if "t" not in d:
        raise ScriptError("timeline entry needs a start time 't' (seconds): %r" % (d,))
    kind = d.get("do")
    if kind not in EVENT_KINDS:
        raise ScriptError("unknown 'do' %r (use %s)" % (kind, ", ".join(EVENT_KINDS)))
    start = secs_to_frames(d["t"])
    if kind == "press":
        length = max(1, secs_to_frames(d.get("for", 0.1)))
    elif kind == "mark":
        length = 0
    else:
        if "for" not in d:
            raise ScriptError("%r needs 'for' (seconds)" % kind)
        length = max(1, secs_to_frames(d["for"]))
    buttons = d.get("buttons", d.get("button", []))
    if isinstance(buttons, str):
        buttons = [b for b in buttons.replace("+", " ").split() if b]
    if kind in ("hold", "press"):
        if not buttons:
            raise ScriptError("%r needs 'buttons'" % kind)
        try:
            button_mask(buttons)  # validates names
        except ValueError as e:
            raise ScriptError(str(e))
    x = float(d.get("x", 0.0))
    y = float(d.get("y", 0.0))
    for v in (x, y):
        if not -1.0 <= v <= 1.0:
            raise ScriptError("stick value %r outside -1..1" % v)
    return Event(start, length, kind, buttons, x, y, str(d.get("label", "")))


class Script:
    """One skill recording: what to set up, what to press, how long to film."""

    DEFAULTS = {
        "stage": None,          # stage id for the match; None = whatever the menu path picks
        "distance": 60.0,       # world units between the fighters when the action starts
        "lock": True,           # hold lock-on during the action
        "opponent": "still",    # only "still" is built: the second fighter stands on an idle pad
        "slot": 0,              # which of the 4 skill slots holds the skill (0-3)
        "lead_in": 1.0,         # seconds of idle footage before the first event
        "tail": 2.0,            # seconds kept after the last event
        "attempts": 1,          # take again (stick turned 90 degrees each time) if the fighter ran over a capsule
        "level": 12,            # Level (and Aura) both fighters are set to for the take
        "health": 20,           # the caster's health for the take
        "opponent_skill": None,  # catalogue skill id put in the opponent's slot 0 (a scripted attacker for defences)
        "opponent_timeline": None,  # list of events (as 'timeline') for the opponent's pad
    }

    def __init__(self, name, skill, events, record_frames, **opts):
        self.name, self.skill, self.events, self.record_frames = name, skill, list(events), record_frames
        for k, v in self.DEFAULTS.items():
            setattr(self, k, opts.get(k, v))
        unknown = set(opts) - set(self.DEFAULTS)
        if unknown:
            raise ScriptError("unknown option(s): %s" % ", ".join(sorted(unknown)))
        if not 0 <= int(self.slot) <= 3:
            raise ScriptError("slot must be 0-3, got %r" % self.slot)
        if self.opponent != "still":
            raise ScriptError("opponent must be 'still' (the only mode built; use opponent_timeline to script it)")
        if float(self.distance) <= 0:
            raise ScriptError("distance must be positive")
        if not 1 <= int(self.health) <= 99:
            raise ScriptError("health must be 1-99")
        if not 1 <= int(self.attempts) <= 8:
            raise ScriptError("attempts must be 1-8")
        if not 0 <= int(self.level) <= 99:
            raise ScriptError("level must be 0-99")
        self.opponent_events = []

    @property
    def last_event_frame(self):
        return max([e.end for e in self.events + self.opponent_events] + [0])


def parse_script(obj):
    """Build a Script from a dict (the JSON file's content). Raises ScriptError on bad input."""
    if not isinstance(obj, dict):
        raise ScriptError("a script is a JSON object")
    if "skill" not in obj:
        raise ScriptError("script needs 'skill' (the skill id)")
    try:
        skill = int(obj["skill"])
    except (TypeError, ValueError):
        raise ScriptError("skill id must be an integer, got %r" % (obj["skill"],))
    if skill < 0:
        raise ScriptError("skill id must be >= 0")
    unknown = set(obj) - set(Script.DEFAULTS) - {"name", "skill", "timeline", "record"}
    if unknown:
        raise ScriptError("unknown option(s): %s" % ", ".join(sorted(unknown)))
    name = str(obj.get("name", "skill%d" % skill))
    tl = obj.get("timeline", [])
    if not isinstance(tl, list):
        raise ScriptError("'timeline' must be a list")
    events = [parse_event(e) for e in tl]
    opts = {k: obj[k] for k in Script.DEFAULTS if k in obj}
    sc = Script(name, skill, events, 0, **opts)
    lead = secs_to_frames(sc.lead_in)
    # events are relative to the end of the lead-in
    for e in sc.events:
        e.start += lead
    if sc.opponent_timeline:
        if not isinstance(sc.opponent_timeline, list):
            raise ScriptError("'opponent_timeline' must be a list")
        sc.opponent_events = [parse_event(e) for e in sc.opponent_timeline]
        for e in sc.opponent_events:
            e.start += lead
    if "record" in obj:
        sc.record_frames = secs_to_frames(obj["record"])
    else:
        sc.record_frames = sc.last_event_frame + secs_to_frames(sc.tail)
    if sc.record_frames <= 0:
        raise ScriptError("nothing to record (no events and no 'record' length)")
    return sc


def load_script(path):
    with open(path, "r", encoding="utf-8") as f:
        return parse_script(json.load(f))


class PadState:
    """What to write into one pad object for one tick."""

    def __init__(self, mask=0, lx=0, ly=0, rx=0, ry=0):
        self.mask, self.lx, self.ly, self.rx, self.ry = mask, lx, ly, rx, ry

    def __eq__(self, o):
        return isinstance(o, PadState) and (self.mask, self.lx, self.ly, self.rx, self.ry) == (o.mask, o.lx, o.ly, o.rx, o.ry)

    def __repr__(self):
        return "PadState(mask=%#x lx=%d ly=%d rx=%d ry=%d)" % (self.mask, self.lx, self.ly, self.rx, self.ry)


def pad_state_at(events, frame, base_mask=0):
    """The pad state at program tick `frame` given the timeline. Overlapping holds OR together;
    for sticks the later-starting event wins."""
    st = PadState(base_mask)
    best_l = best_r = -1
    for e in events:
        if e.kind == "mark" or not (e.start <= frame < e.end):
            continue
        if e.kind in ("hold", "press"):
            st.mask |= button_mask(e.buttons)
        elif e.kind == "stick" and e.start >= best_l:
            st.lx, st.ly, best_l = stick_int(e.x), stick_int(e.y), e.start
        elif e.kind == "rstick" and e.start >= best_r:
            st.rx, st.ry, best_r = stick_int(e.x), stick_int(e.y), e.start
        # "release" has no pad effect by itself: it only ends overlapping holds below
    for e in events:
        if e.kind == "release" and e.start <= frame < e.end:
            st.mask &= ~button_mask(e.buttons)
    return st


def rotate_events(events, quarter_turns):
    """Copies of the events with every stick vector turned by quarter_turns * 90 degrees
    (x, y) -> (-y, x) per quarter turn. Button events are unchanged."""
    out = []
    for e in events:
        x, y = e.x, e.y
        for _ in range(quarter_turns % 4):
            x, y = -y, x
        out.append(Event(e.start, e.length, e.kind, e.buttons, x, y, e.label))
    return out


def marks(events):
    """(frame, label) for every mark event, in time order."""
    return sorted((e.start, e.label) for e in events if e.kind == "mark")


# ----------------------------------------------------------------------- pure: batch lists
def safe_name(s):
    out = "".join(c if c.isalnum() or c in "-_" else "_" for c in str(s).strip())
    return out.strip("_") or "skill"


def output_stem(skill_id, name):
    """File stem for one skill: '<id>_<name>' (id zero-padded to 3)."""
    return "%03d_%s" % (int(skill_id), safe_name(name))


def read_skill_list(path):
    """A skill list is a CSV with at least an 'id' column (and optional 'name', 'kind', 'script').
    Lines starting with # and blank lines are ignored. Returns a list of dicts."""
    rows = []
    with open(path, newline="", encoding="utf-8") as f:
        lines = [ln for ln in f if ln.strip() and not ln.lstrip().startswith("#")]
    rdr = csv.DictReader(lines)
    if not rdr.fieldnames or "id" not in [c.strip() for c in rdr.fieldnames]:
        raise ScriptError("skill list needs an 'id' column")
    for r in rdr:
        r = {k.strip(): (v or "").strip() for k, v in r.items() if k}
        try:
            r["id"] = int(r["id"])
        except ValueError:
            raise ScriptError("bad skill id %r" % r.get("id"))
        r.setdefault("name", "skill%d" % r["id"])
        rows.append(r)
    return rows


# --------------------------------------------------------------------------------- (live part below)
import ctypes  # noqa: E402
import time  # noqa: E402

import live_trace as lt  # noqa: E402
import save_guard as sg  # noqa: E402

EXE_DEFAULT = r"..\..\..\game_files\PDUWP.exe"
OUT_ROOT = os.path.expandvars(r"%USERPROFILE%\.pdtools\moverec")


class Rig(lt.Tracer):
    """The debugger with a once-per-tick hook (right after the game polls its pads).

    on_tick(rig, tick) is called on the game's thread while it is stopped at the hook; inside it
    use rig.set_pad / rig.rd32 / rig.rdf / rig.wr32 / rig.wrf. Extra --bp style hooks (sounds
    etc.) can be added with extra_specs; their hits go to rig.on_hit(name, values, tick).
    """

    def __init__(self, pid, on_tick, extra_specs=(), out_path=None, exe_path=EXE_DEFAULT, on_hit=None):
        specs = [lt.BpSpec("0x%x:tick:rax" % HOOK_AFTER_POLL)] + [lt.BpSpec(s) for s in extra_specs]
        super().__init__(pid, specs, out_path or os.path.join(OUT_ROOT, "rig_raw.csv"),
                         exe_path, echo=False, summary=False)
        self.on_tick_cb, self.on_hit_cb = on_tick, on_hit
        self.tick = 0
        self.pad_cache = {}
        self.fatal = None
        self.exceptions = []
        self.exception_text = ""
        self.redirect = None
        self.call_queue = []
        self.call_results = {}
        self.active_call = None
        self._saved = None
        self.last_tick_t = time.monotonic()
        self.watch_s = None             # set: stop and flag .stalled when no tick arrives for this long
        self.stalled = False
        self.stall_dump = ""

    # ---- typed memory helpers (ghidra addresses; negative-safe)
    def _abs(self, g):
        return lt.ghidra_to_runtime(g, self.base) if lt.GHIDRA_BASE <= g < lt.GHIDRA_BASE + 0x10000000 else g

    def rd(self, g, n):
        return self.read(self._abs(g), n)

    def rd32(self, g, signed=False):
        r = self.rd(g, 4)
        if r is None or len(r) < 4:
            return None
        return struct.unpack("<i" if signed else "<I", r)[0]

    def rd64(self, g):
        r = self.rd(g, 8)
        return struct.unpack("<Q", r)[0] if r and len(r) == 8 else None

    def rdf(self, g):
        r = self.rd(g, 4)
        return struct.unpack("<f", r)[0] if r and len(r) == 4 else None

    def wr(self, g, data):
        self.write(self._abs(g), data)

    def wr32(self, g, v):
        self.wr(g, struct.pack("<I", v & 0xFFFFFFFF))

    def wrf(self, g, v):
        self.wr(g, struct.pack("<f", v))

    # ---- input
    def set_pad(self, pad, state):
        """Overwrite pad object `pad` (mask + sticks) for this tick."""
        self.wr(pad_object_addr(pad) + PAD_MASK_OFF,
                pack_pad(state.mask, state.lx, state.ly, state.rx, state.ry))

    def fighter(self, player):
        """Runtime-independent pointer (absolute) of fighter `player`, or 0."""
        p = self.rd64(FIGHTER_PTRS + 8 * player)
        return p or 0

    def teleport(self, player, x, y, z):
        """Put a fighter at (x, y, z): position, the 'previous position' copy and the ground
        reference the program keeps (+0x12e4c, +0x12e78/7c), vertical speed zeroed."""
        p = self.fighter(player)
        if not p:
            return False
        v = struct.pack("<3f", x, y, z)
        self.write(p + FIGHTER_POS, v)
        self.write(p + 0x12E4C, v)
        self.write(p + 0x12E78, struct.pack("<2f", y, y))
        self.write(p + 0x12E84, struct.pack("<f", 0.0))
        return True

    def fighter_pos(self, player):
        p = self.fighter(player)
        if not p:
            return None
        r = self.read(p + FIGHTER_POS, 12)
        return struct.unpack("<3f", r) if r and len(r) == 12 else None

    # ---- remote calls: run one of the game's own functions on its thread at the hook
    def call(self, ghidra_addr, *args, tag=None):
        """Queue a call of the game function at `ghidra_addr` (x64 ABI, up to 4 integer args plus
        more on the stack). It runs on the game thread at the next tick hook, between its own
        per-tick steps, and returns to the hook; rax goes to self.call_results[tag]."""
        self.call_queue.append((ghidra_addr, args, tag if tag is not None else len(self.call_results)))

    def _start_call(self, saved):
        addr, args, tag = self.call_queue.pop(0)
        rsp = saved["rsp"]
        base = (rsp - 0x400) & ~0xF
        extra = list(args[4:])
        slots = 4 + len(extra)
        new_rsp = base - 8 - 8 * (slots + (slots & 1))   # keeps rsp % 16 == 8 at the callee entry
        hook = lt.ghidra_to_runtime(HOOK_AFTER_POLL, self.base)
        self.write(new_rsp, struct.pack("<Q", hook))
        for i, a in enumerate(extra):
            self.write(new_rsp + 8 + 0x20 + 8 * i, struct.pack("<Q", a & 0xFFFFFFFFFFFFFFFF))
        regs = [lt.REG64["rcx"], lt.REG64["rdx"], lt.REG64["r8"], lt.REG64["r9"]]
        for i, off in enumerate(regs):
            self.ctx.set_u64(off, (args[i] if i < len(args) else 0) & 0xFFFFFFFFFFFFFFFF)
        self.ctx.set_u64(lt.CTX_RSP_OFF, new_rsp)
        self.ctx.set_u64(lt.CTX_RIP_OFF, lt.ghidra_to_runtime(addr, self.base))
        self.redirect = True
        self.active_call = (addr, tag)

    def _save_regs(self):
        raw = self.ctx.bytes()
        return {n: struct.unpack_from("<Q", raw, lt.REG64[n])[0]
                for n in ("rax", "rcx", "rdx", "r8", "r9", "r10", "r11", "rsp")}

    def _restore_regs(self, saved):
        for n, v in saved.items():
            self.ctx.set_u64(lt.REG64[n], v)

    # ---- hook dispatch (called by the base class on every breakpoint hit)
    def log_hit(self, bp, tid, ctx_bytes):
        name = bp.spec.name
        try:
            if name == "tick" and self.active_call is not None:
                # the remote call returned to the hook
                rax = struct.unpack_from("<Q", ctx_bytes, lt.REG64["rax"])[0]
                addr, tag = self.active_call
                self.active_call = None
                self.call_results[tag] = rax
                if self.call_queue:
                    self._start_call(self._saved)
                else:
                    self._restore_regs(self._saved)
                    self._saved = None
            elif name == "tick":
                self.last_tick_t = time.monotonic()
                self.tick += 1
                bp.hits += 1
                self.on_tick_cb(self, self.tick)
                if self.call_queue:
                    self._saved = self._save_regs()
                    self._start_call(self._saved)
            elif self.on_hit_cb:
                vals = [a.read(ctx_bytes, self.read) for a in bp.spec.args]
                bp.hits += 1
                self.on_hit_cb(self, name, vals, self.tick)
        except Exception as e:  # never let a script bug stop the game thread: record and stop cleanly
            self.fatal = e
            self.stop.set()
            import traceback
            traceback.print_exc()

    def pump(self, timeout_ms, draining=False):
        r = super().pump(timeout_ms, draining)
        if self.watch_s and not draining and r is not None and not self.stalled \
                and time.monotonic() - self.last_tick_t > self.watch_s:
            self.stalled = True         # the game thread stopped calling its per-tick poll
            try:
                self.stall_dump = self.dump_threads()
            except Exception as e:      # diagnostics must never get in the way
                self.stall_dump = "thread dump failed: %r" % (e,)
            self.stop.set()
        return r

    def dump_threads(self, max_threads=40):
        """Where every game thread is right now (ghidra addresses of its RIP and of code addresses on its
        stack), for the run log when the game stalls."""
        lines = []
        for tid, h in list(self.threads.items())[:max_threads]:
            if not h or self.k.SuspendThread(h) == 0xFFFFFFFF:
                continue
            try:
                self.ctx.clear(lt.CONTEXT_CONTROL | lt.CONTEXT_INTEGER)
                if not self.k.GetThreadContext(h, self.ctx.addr):
                    continue
                raw = self.ctx.bytes()
            finally:
                self.k.ResumeThread(h)
            rip = struct.unpack_from("<Q", raw, lt.CTX_RIP_OFF)[0]
            rsp = struct.unpack_from("<Q", raw, lt.CTX_RSP_OFF)[0]
            g = lt.runtime_to_ghidra(rip, self.base, self.size)
            stack = self.read(rsp, 8 * 64) or b""
            chain = []
            for i in range(0, len(stack) - 7, 8):
                gg = lt.runtime_to_ghidra(struct.unpack_from("<Q", stack, i)[0], self.base, self.size)
                if gg and 0x140001000 <= gg < 0x1402d2600:
                    chain.append("0x%x" % gg)
            lines.append("t%d rip=%s stack=%s" % (tid, ("0x%x" % g) if g else "ext", " ".join(chain[:8])))
        return chr(10).join(lines)

    def handle_exception(self, tid, draining):
        r = super().handle_exception(tid, draining)
        b = self.evbuf.raw
        code = struct.unpack_from("<I", b, 16)[0]
        if code not in (lt.EXC_BREAKPOINT, lt.EXC_SINGLE_STEP) and not draining:
            addr = struct.unpack_from("<Q", b, 32)[0]
            info = struct.unpack_from("<QQ", b, 48)
            self.exceptions.append((code, addr))
            g = lt.runtime_to_ghidra(addr, self.base, self.size)
            lines = ["EXCEPTION %#x at %s (thread %d) info=%s" % (
                code, ("ghidra 0x%x" % g) if g else "runtime 0x%x (outside the exe)" % addr, tid,
                [hex(x) for x in info])]
            h = self.get_ctx(tid)
            if h:
                raw = self.ctx.bytes()
                rsp = struct.unpack_from("<Q", raw, lt.CTX_RSP_OFF)[0]
                stack = self.read(rsp, 8 * 96) or b""
                chain = []
                for i in range(0, len(stack) - 7, 8):
                    q = struct.unpack_from("<Q", stack, i)[0]
                    gg = lt.runtime_to_ghidra(q, self.base, self.size)
                    if gg and 0x140001000 <= gg < 0x1402d2600:
                        chain.append("0x%x" % gg)
                lines.append("  stack code addresses: " + " ".join(chain[:24]))
            msg = chr(10).join(lines)
            self.exception_text = msg
            print(msg)
        return r

    def run_until_stopped(self, seconds=0):
        self.run(seconds)


def find_game_pid():
    k = lt._k32()
    pids = lt.find_pdu_process(k)
    return pids[0] if pids else None


# ======================================================================= the live flow
# Game functions the flow calls on the game's own thread (ghidra addresses).
FN_SET_PARAM = 0x1401D0630      # FUN_1401d0630(kind, player, value): versus launch parameters
FN_START_VERSUS = 0x1401D0750   # FUN_1401d0750(): begins battle kind 6 from those parameters
FN_MENU_GOTO = 0x140246130      # FUN_140246130(menu manager, menu id, force)
FN_CAPTURE = 0x140261A00        # FUN_140261a00(slot record, skill id, 1, play sound): fill a slot
MENU_MGR = 0x140533C58          # DAT_140533c58: the menu manager object
MENU_STATE = 0x140533744        # DAT_140533744: menu manager state (2 = a menu script initialising, 3 = ready)
BATTLE_STATE = 0x1404BC950      # DAT_1404bc950: 1 loading ... 5 countdown, 6 fighting
PROFILE_MGR = 0x1404BEC10       # DAT_1404bec10: profile manager; +0x3a38 = stock (signed bytes by skill id)
STOCK_PTR_OFF = 0x3A38
SLOT_BASE = 0x388               # fighter + 0x388 + 0x50 * slot = skill slot record
SLOT_STRIDE = 0x50
STAT_BASE = 0x40                # fighter stats block: health +0x0c, max +0x10, Level +0x18, Aura +0x1c
SLOT_BUTTON = (BUTTONS["Y"], BUTTONS["X"], BUTTONS["B"], BUTTONS["A"])   # read live from the input block
LOCK_BAND_OFF, LOCK_TARGET_OFF, LOCK_ACTIVE_OFF = 0x13F1C, 0x13F20, 0x13F24
VERSUS_RULE_QUICK = 2           # rule 2 = Quick: arsenals come from the program's own table, no save read
DEFAULT_STAGE = 6               # stage number for the versus launch (mapping to st-names UNCONFIRMED)
# hooks logged into trace.csv: "ghidra addr:name:args" (a trailing * marks a summary key)
EVENT_HOOKS = (
    "0x140276330:sound:ecx*,edx*",              # sound(bank, id, ...)
    "0x140258180:press:rdx,d[rdx+0x34]*",       # a skill button accepted (slot record, skill id)
    "0x1402626d0:fire:rcx,edx*",                # the skill fires (record, id)
    "0x14024d930:level:rcx,edx*",               # Level add
    "0x14018d2f0:effect:ecx,edx*",              # effect request (player, effect id)
    "0x140261a00:capture:rcx,edx*",             # a slot filled (slot record, skill id)
)


def stock_counts(rig, n=800):
    """Owned count per skill id from the loaded profile (None when no profile is loaded yet)."""
    mgr = rig.rd64(PROFILE_MGR)
    if not mgr:
        return None
    p = rig.read(mgr + STOCK_PTR_OFF, 8)
    if not p or len(p) < 8:
        return None
    d = rig.read(struct.unpack("<Q", p)[0], n)
    if not d or len(d) < n:
        return None
    return [b - 256 if b > 127 else b for b in d]


def game_index(skill):
    """The index the program uses for a skill: its row in the skill table at DAT_1404bdc50 (0x90
    bytes each; stock counts, deck entries and slot records use it) = the skills.json record + 1,
    index 0 = the Aura Particle (FUN_140262670 reads DAT_1404bdc50 + index * 0x90; established by
    the skillaudit / decks lanes). It is NOT text_id: the two agree only up to id 300, and ids
    301-340 differ (text_id 304 vs table index 301)."""
    return skill["record"] + 1


def unique_skills(skills):
    """One entry per catalogue id: ids 257 and 294 appear twice in skills.json (records 749/750 are
    the dodge moves, nameless duplicates of the real skills at 76 / 242); keep the lowest record."""
    best = {}
    for s in skills:
        if s["id"] not in best or s["record"] < best[s["id"]]["record"]:
            best[s["id"]] = s
    return [best[i] for i in sorted(best)]


def owned_rows(counts, skills):
    """(id, game_index, name, count_owned, capsule, school) rows, sorted by catalogue id."""
    out = []
    for s in unique_skills(skills):
        gi = game_index(s)
        out.append((s["id"], gi, s["name"], max(0, counts[gi]) if gi < len(counts) else 0,
                    s.get("capsule", ""), s.get("school", "")))
    return out


def write_owned_csv(path, counts, skills):
    """owned_skills.csv: id (catalogue), game_index, name, count owned. Returns (owned, unowned)."""
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    rows = owned_rows(counts, skills)
    with open(path, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["id", "game_index", "name", "count_owned", "capsule", "school"])
        w.writerows(rows)
    owned = sum(1 for r in rows if r[3] > 0)
    return owned, len(rows) - owned


def load_skill_table(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)["skills"]


class Driver:
    """Runs a generator as a per-tick coroutine. `yield n` means: wake me n ticks from now.

    Between wake-ups the pad state in .pads[0] / .pads[1] is written every tick, so a held button
    stays held. The generator finishing stops the rig.
    """

    def __init__(self, rig, gen):
        self.rig, self.gen = rig, gen
        self.wait = 1
        self.pads = [PadState(), PadState()]
        self.done = False
        self.error = None
        self.take_deadline = None       # wall-clock limit for the take being recorded (watchdog)
        self.stalled_reason = None

    def on_tick(self, rig, tick):
        if self.take_deadline and time.monotonic() > self.take_deadline and not self.done:
            self.stalled_reason = "a take ran past its wall-clock limit"
            self.done = True
            rig.stop.set()
        self.wait -= 1
        if self.wait <= 0 and not self.done:
            try:
                n = next(self.gen)
                self.wait = max(1, int(n))
            except StopIteration:
                self.done = True
                rig.stop.set()
            except Exception as e:  # keep the game alive; report after detach
                self.error = e
                self.done = True
                rig.stop.set()
                import traceback
                traceback.print_exc()
        for pad, st in enumerate(self.pads):
            rig.set_pad(pad, st)


def boot_to_profile(rig, drv, timeout_ticks=2400):
    """Generator: press START on pad 0 now and then until the profile (stock) has loaded."""
    t = 0
    while t < timeout_ticks:
        counts = stock_counts(rig, 400)
        if counts is not None and any(c > 0 for c in counts):
            drv.pads[0] = PadState()
            yield 60
            return True
        drv.pads[0] = PadState(BUTTONS["START"] if t % 60 < 4 else 0)
        yield 1
        t += 1
    raise RuntimeError("profile did not load within %d ticks" % timeout_ticks)


def launch_versus(rig, drv, stage=DEFAULT_STAGE, chars=(0, 1), rule=VERSUS_RULE_QUICK, timeout_ticks=900):
    """Generator: set the versus parameters, start the battle and wait until it is fighting (state 6)."""
    for kind, idx, val in [(1, 0, chars[0]), (1, 1, chars[1]), (2, 0, 0), (2, 1, 0), (3, 0, stage), (4, 0, 0x34), (6, 0, rule)]:
        rig.call(FN_SET_PARAM, kind, idx, val)
    rig.call(FN_START_VERSUS, tag="launch")
    yield 3
    # From the title the menu manager is still running the title's script (state 2) and the battle
    # waits for state 3, so send it to the blank menu. In a running battle it is already at 3.
    mgr = rig.rd64(MENU_MGR)
    if mgr and rig.rd32(MENU_STATE) != 3:
        rig.call(FN_MENU_GOTO, mgr, 1, 0, tag="blank_menu")
    t = 0
    while t < timeout_ticks:
        if rig.rd32(BATTLE_STATE) == 6 and rig.fighter(0) and rig.fighter(1):
            yield 30
            return True
        yield 1
        t += 1
    raise RuntimeError("battle did not reach the fighting state")


def _ri(rig, addr):
    r = rig.read(addr, 4)
    return struct.unpack("<i", r)[0] if r and len(r) == 4 else ""


def _rf(rig, addr):
    r = rig.read(addr, 4)
    return struct.unpack("<f", r)[0] if r and len(r) == 4 else None


def fighter_row(rig, p):
    """One fighter's trace columns: x y z facing clip state health level aura lock_active lock_band."""
    f = rig.fighter(p)
    if not f:
        return [""] * 11
    raw = rig.read(f + FIGHTER_POS, 12)
    x, y, z = struct.unpack("<3f", raw) if raw and len(raw) == 12 else (0.0, 0.0, 0.0)
    aura = _rf(rig, f + STAT_BASE + 0x1C)
    return ["%.2f" % x, "%.2f" % y, "%.2f" % z, _ri(rig, f + FIGHTER_FACING), _ri(rig, f + FIGHTER_CLIP),
            _ri(rig, f + FIGHTER_STATE), _ri(rig, f + STAT_BASE + 0x0C), _ri(rig, f + STAT_BASE + 0x18),
            "%.2f" % (aura or 0.0), _ri(rig, f + LOCK_ACTIVE_OFF), _ri(rig, f + LOCK_BAND_OFF)]


_COLS = ("x", "y", "z", "facing", "clip", "state", "health", "level", "aura", "lock", "band")
TRACE_COLUMNS = (["rec_tick", "tick", "ms"] + ["p0_" + c for c in _COLS] + ["p1_" + c for c in _COLS]
                 + ["pad_mask", "events"])


def set_slot(rig, player, slot, skill_id):
    """Put `skill_id` in the player's skill slot through the program's own capture routine."""
    f = rig.fighter(player)
    rig.call(FN_CAPTURE, f + SLOT_BASE + SLOT_STRIDE * slot, skill_id, 1, 0, tag="slot_%d_%d" % (player, slot))


def set_stats(rig, player, level, health=None):
    f = rig.fighter(player)
    rig.wr32(f + STAT_BASE + 0x18, level)
    rig.wrf(f + STAT_BASE + 0x1C, float(level))
    if health is not None:
        rig.wr32(f + STAT_BASE + 0x0C, health)
        rig.wr32(f + STAT_BASE + 0x10, max(health, rig.rd32(f + STAT_BASE + 0x10) or 0))


def candidate_directions(anchor0, anchor1):
    """Unit (x, z) directions to try for the opponent: toward his own start point first, then the
    eight compass points."""
    import math
    dx, dz = anchor1[0] - anchor0[0], anchor1[2] - anchor0[2]
    n = (dx * dx + dz * dz) ** 0.5 or 1.0
    out = [(dx / n, dz / n)]
    for k in range(8):
        a = k * math.pi / 4
        out.append((math.cos(a), math.sin(a)))
    return out


def place_pair(rig, anchor0, direction, distance, lift=1.0):
    """Teleport P0 onto its anchor and P1 `distance` units away along `direction`, `lift` above."""
    ux, uz = direction
    rig.teleport(0, *anchor0)
    rig.teleport(1, anchor0[0] + ux * distance, anchor0[1] + lift, anchor0[2] + uz * distance)


def settle_placement(rig, drv, distance, anchors, cache):
    """Generator: find a direction in which P1 lands on the same floor as P0 at `distance`; returns
    (direction, verified). Remembers the answer per distance in `cache`."""
    key = round(float(distance), 1)
    tries = [cache[key]] if key in cache else candidate_directions(anchors[0], anchors[1])
    first = None
    for d in tries:
        first = first or d
        place_pair(rig, anchors[0], d, distance)
        yield 40
        p0, p1 = rig.fighter_pos(0), rig.fighter_pos(1)
        s1 = _ri(rig, rig.fighter(1) + FIGHTER_STATE)
        if p0 and p1 and abs(p1[1] - p0[1]) <= 1.0 and abs(p0[1] - anchors[0][1]) <= 1.0 and s1 in (0, 1):
            cache[key] = d
            return d, True
    place_pair(rig, anchors[0], first, distance)
    yield 40
    return first, False


def find_game_hwnd(title="Phantom Dust"):
    """The game's top-level window. A UWP game is hosted: the visible top-level window is an
    ApplicationFrameWindow owned by ApplicationFrameHost.exe, titled "Phantom Dust" (the game's own
    Windows.UI.Core.CoreWindow is a child owned by PDUWP.exe). EnumWindows does not list it on
    this machine, FindWindowW does. Returns the hwnd or 0."""
    u = ctypes.windll.user32
    u.FindWindowW.restype = ctypes.c_void_p
    h = u.FindWindowW("ApplicationFrameWindow", title)
    return int(h) if h else 0


def build_capture_command(ffmpeg, hwnd, out_path, fps=30, width=1440, seconds=None):
    """The ffmpeg command line for a one-window capture (Windows Graphics Capture by hwnd; the
    window_exe / window_title options do not find a hosted UWP window). Kept pure for tests."""
    src = ("gfxcapture=hwnd=%d:max_framerate=%d:capture_cursor=false:width=%d:height=%d:resize_mode=scale_aspect"
           ",hwdownload,format=bgra" % (int(hwnd), fps, width, width * 5 // 8))
    cmd = [ffmpeg, "-hide_banner", "-loglevel", "warning", "-y", "-f", "lavfi", "-i", src]
    if seconds:
        cmd += ["-t", "%.2f" % seconds]          # ffmpeg ends (and finalises the file) by itself
    # fragmented mp4: the file grows from the first frame (so we can tell it is filming) and stays
    # playable even if ffmpeg has to be killed
    return cmd + ["-r", str(fps), "-c:v", "libx264", "-preset", "veryfast", "-crf", "18",
                  "-g", "10", "-pix_fmt", "yuv420p", "-movflags", "+frag_keyframe+empty_moov+default_base_moof", out_path]


class Capture:
    """ffmpeg capture of the game window only (Windows Graphics Capture: films that window even
    while it is covered or unfocused; it needs the window to exist on an active desktop)."""

    FFMPEG = os.path.expandvars(r"%USERPROFILE%\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe"
              r"\ffmpeg-8.1.2-full_build\bin\ffmpeg.exe")

    def __init__(self, out_path, fps=30, hwnd=None, ffmpeg=None, seconds=None):
        self.out_path, self.fps, self.seconds = out_path, fps, seconds
        self.hwnd = hwnd
        self.ffmpeg = ffmpeg or self.FFMPEG
        self.proc = None
        self.log = None
        self.ready_at = None

    def command(self):
        return build_capture_command(self.ffmpeg, self.hwnd, self.out_path, self.fps, seconds=self.seconds)

    def start(self):
        import subprocess
        if not os.path.exists(self.ffmpeg):
            return False
        self.hwnd = self.hwnd or find_game_hwnd()
        if not self.hwnd:
            return False
        os.makedirs(os.path.dirname(os.path.abspath(self.out_path)), exist_ok=True)
        self.log = open(self.out_path + ".log", "wb")
        self.proc = subprocess.Popen(self.command(), stdin=subprocess.PIPE, stdout=self.log, stderr=self.log,
                                     creationflags=0x08000000)  # CREATE_NO_WINDOW
        return True

    def epoch(self):
        """Monotonic time that corresponds to video time 0. Calibrated live on three takes: the
        HUD Aura digit changes in the video 0.75 s earlier than the take's wall clock says when
        epoch = ready - 0.4, so video time 0 is about ready_at + 0.35 (ffmpeg only starts writing
        once it has buffered its first frames)."""
        return (self.ready_at or time.monotonic()) + 0.35

    def ready(self):
        """True once ffmpeg has written its first frames (it needs a few seconds to start).
        With -g 10 the file grows every third of a second, so this is within ~0.4 s of the
        video's first frame."""
        ok = os.path.exists(self.out_path) and os.path.getsize(self.out_path) > 20000
        if ok and self.ready_at is None:
            self.ready_at = time.monotonic()
        return ok

    def stop(self):
        if not self.proc:
            return False
        if self.proc.poll() is None:
            # Ask ffmpeg to finish ('q'); if it does not (it may be blocked waiting for a frame), end it:
            # the fragmented mp4 stays playable.
            try:
                self.proc.stdin.write(b"q")
                self.proc.stdin.flush()
                self.proc.wait(timeout=12)
            except Exception:
                pass
        if self.proc.poll() is None:
            self.proc.kill()
            try:
                self.proc.wait(timeout=5)
            except Exception:
                pass
        self.proc = None
        if self.log:
            self.log.close()
        ok = os.path.exists(self.out_path) and os.path.getsize(self.out_path) > 1000
        if ok:
            try:
                os.remove(self.out_path + ".log")
            except OSError:
                pass
        return ok


def start_master_capture(out_path, tries=3, wait_ticks=450):
    """Generator: start one ffmpeg capture of the game window for the whole batch (starting and
    stopping a capture per take wedged the game once). Returns the Capture or None."""
    for n in range(tries):
        cap = Capture(out_path)
        if not cap.start():
            return None
        waited = 0
        while waited < wait_ticks and not cap.ready() and cap.proc.poll() is None:
            yield 1
            waited += 1
        if cap.ready() and cap.proc.poll() is None:
            return cap
        cap.stop()
    return None


def cut_takes(master_path, out_dir, metas, lead=0.8, tail=1.2, ffmpeg=None):
    """Cut every take's clip out of the master recording and set meta["video"]. Slow part, done
    after the game is released. Returns the number of clips written."""
    import subprocess
    ff = ffmpeg or Capture.FFMPEG
    n = 0
    for m in metas:
        if m.get("video_t0_s") is None:
            continue
        stem = output_stem(m["skill"], m["name"])
        start = max(0.0, m["video_t0_s"] - lead)
        dur = m["video_dur_s"] + lead + tail
        out = os.path.join(out_dir, stem + ".mp4")
        r = subprocess.run([ff, "-hide_banner", "-loglevel", "error", "-y", "-ss", "%.2f" % start, "-i", master_path,
                            "-t", "%.2f" % dur, "-c:v", "libx264", "-preset", "veryfast", "-crf", "20",
                            "-pix_fmt", "yuv420p", "-movflags", "+faststart", out], capture_output=True)
        ok = r.returncode == 0 and os.path.exists(out) and os.path.getsize(out) > 1000
        m["video"] = os.path.basename(out) if ok else None
        m["video_clip_start_s"] = round(start, 2)
        jp = os.path.join(out_dir, stem + ".json")
        with open(jp, "w", encoding="utf-8") as f:
            json.dump(m, f, indent=1)
        n += ok
    return n


def record_skill(rig, drv, sc, out_dir, anchors, video=True, notes=None, index=None, cache=None,
                 relaunch_cb=None, opp_index=None):
    """Generator: set up one take of script sc, play it, write <stem>.mp4 (if capturable),
    <stem>_trace.csv and <stem>.json into out_dir. index is the game's index for sc.skill.
    With attempts > 1 the take is repeated (fresh match through relaunch_cb, stick turned 90
    degrees) when the fighter ran over a capsule, which would have replaced the skill under test."""
    stem = output_stem(sc.skill, sc.name)
    os.makedirs(out_dir, exist_ok=True)
    notes = list(notes or [])
    cache = {} if cache is None else cache
    index = sc.skill if index is None else index
    meta = None
    touched = False
    for attempt in range(int(sc.attempts)):
        if attempt:
            notes.append("attempt %d: the fighter touched a capsule, repeated with the stick turned" % attempt)
            if relaunch_cb:
                anchors = yield from relaunch_cb()
                cache = {}
        meta, touched = yield from _one_take(rig, drv, sc, out_dir, stem, anchors, video, notes, index, cache, attempt,
                                             opp_index)
        if not touched:
            break
    if meta is not None and touched:
        meta["notes"].append("a capsule was touched during the take in every attempt")
        with open(os.path.join(out_dir, stem + ".json"), "w", encoding="utf-8") as f:
            json.dump(meta, f, indent=1)
    return meta


def split_events(rows, index, opponent=False):
    """Press / fire events of a take split by owner. The event arguments are (slot record pointer, skill
    index); the two fighters' slot records are 0x60000 apart (fighter objects are allocated that far
    apart, P0 below P1), so with both seen the lower pointer is P0. With one pointer only, when an opponent
    attacker is scripted the owner is told by the skill index (P0 fires the skill under test).
    Returns {"p0": {"press": (tick, idx)|None, "fire": ...}, "p1": {...}}."""
    evs = []
    for r in rows:
        for e in [e for e in r["events"].split("|") if e]:
            if e.startswith(("press:", "fire:")):
                kind, _, arg = e.partition(":")
                try:
                    ptr, idx = (int(x) for x in arg.split(".", 1))
                except ValueError:
                    continue
                evs.append((int(r["rec_tick"]), kind, ptr, idx))
    ptrs = sorted({e[2] for e in evs})
    owner = {}
    if len(ptrs) >= 2:
        owner = {ptrs[0]: "p0"}
        owner.update({p: "p1" for p in ptrs[1:]})
    out = {"p0": {"press": None, "fire": None}, "p1": {"press": None, "fire": None}}
    for tick, kind, ptr, idx in evs:
        who = owner.get(ptr)
        if who is None:                      # a single pointer seen
            who = "p0" if (not opponent or idx == index) else "p1"
        if out[who][kind] is None:
            out[who][kind] = (tick, idx)
    return out


def take_summary(rows, index, opponent=False):
    """What happened in a take, from its trace rows (a list of dicts): the caster's press / fire ticks,
    the skill index its fire named, whether it is the one under test, the first hit on the opponent, and
    the opponent's own press / fire (a scripted attacker). A defence skill is reactive: the program fires
    it when the attack comes, with no accepted press of its own (seen live), so fired without pressed is
    a normal take for it."""
    ev = split_events(rows, index, opponent)
    p0, p1 = ev["p0"], ev["p1"]
    press = p0["press"][0] if p0["press"] else None
    fire = p0["fire"][0] if p0["fire"] else None
    fire_index = p0["fire"][1] if p0["fire"] else None
    hit = None
    h0 = None
    for r in rows:
        try:
            h = int(r["p1_health"])
        except (ValueError, KeyError):
            continue
        if h0 is None:
            h0 = h
        if h < h0 and hit is None:
            hit = int(r["rec_tick"])
    return {"pressed": press is not None, "press_tick": press, "fired": fire is not None, "fire_tick": fire,
            "fire_index": fire_index, "index_ok": (fire_index == index) if fire is not None else None,
            "hit_tick": hit, "opp_press_tick": p1["press"][0] if p1["press"] else None,
            "opp_fire_tick": p1["fire"][0] if p1["fire"] else None}


def _one_take(rig, drv, sc, out_dir, stem, anchors, video, notes, index, cache, attempt, opp_index=None):
    trace_path = os.path.join(out_dir, stem + "_trace.csv")
    events = []
    touched = []
    def on_hit(r, name, vals, tick):
        events.append("%s:%s" % (name, ".".join(str(v) for v in vals)))
        if name == "capture":
            touched.append(tick)
    rig.on_hit_cb = on_hit
    notes = list(notes)
    tl = rotate_events(sc.events, attempt)
    op = rotate_events(sc.opponent_events, attempt)
    # --- setup (not recorded): place, let them land, then fix health / Level / slot
    drv.pads = [PadState(), PadState()]
    direction, verified = yield from settle_placement(rig, drv, float(sc.distance), anchors, cache)
    if not verified:
        notes.append("placement unverified: the opponent did not land on the caster's floor")
    set_stats(rig, 0, int(sc.level), health=int(sc.health))
    set_stats(rig, 1, int(sc.level), health=40)
    set_slot(rig, 0, int(sc.slot), index)
    if sc.opponent_skill is not None:
        set_slot(rig, 1, 0, int(opp_index if opp_index is not None else sc.opponent_skill))
    yield 20
    # --- take (the batch's master capture, if any, is already filming)
    events.clear()
    touched.clear()
    t0 = time.monotonic()
    first_tick = rig.tick + 1
    fh = open(trace_path, "w", newline="")
    w = csv.writer(fh)
    w.writerow(TRACE_COLUMNS)
    lock_mask = BUTTONS["RT"] if sc.lock else 0
    trace_rows = []
    drv.take_deadline = time.monotonic() + sc.record_frames / TICK_HZ + 90.0
    try:
        for rt in range(sc.record_frames):
            p0 = pad_state_at(tl, rt, base_mask=lock_mask)
            p1 = pad_state_at(op, rt) if op else PadState()
            drv.pads = [p0, p1]
            yield 1
            row = [rt, rig.tick, "%.1f" % ((time.monotonic() - t0) * 1000.0)] + fighter_row(rig, 0) \
                + fighter_row(rig, 1) + ["0x%x" % p0.mask, "|".join(events)]
            w.writerow(row)
            trace_rows.append(dict(zip(TRACE_COLUMNS, [str(v) for v in row])))
            events.clear()
    finally:
        fh.close()
        drv.take_deadline = None
        drv.pads = [PadState(), PadState()]
    t_end = time.monotonic()
    meta = {
        "skill": sc.skill, "game_index": index, "name": sc.name, "slot": sc.slot,
        "slot_button": "YXBA"[int(sc.slot)], "distance": sc.distance, "lock": bool(sc.lock),
        "level": sc.level, "record_ticks": sc.record_frames, "first_tick": first_tick,
        "tick_hz": TICK_HZ, "marks": marks(sc.events), "placement_verified": verified,
        "attempt": attempt, "capsule_touched": bool(touched),
        "trace": os.path.basename(trace_path), "video": None,
        # where the take sits in the master recording (seconds); filled in only when one is filming
        "video_t0_s": round(t0 - video.epoch(), 2) if video else None,
        "finished": True,
        "video_dur_s": round(t_end - t0, 2) if video else None, "notes": notes,
    }
    meta.update(take_summary(trace_rows, index, opponent=sc.opponent_skill is not None))
    if not meta["pressed"] and not meta["fired"]:
        meta["notes"].append("the press was refused (no cast): requirement / cost / slot not met")
    elif meta["index_ok"] is False:
        meta["notes"].append("WRONG SKILL: fire named index %s, expected %s" % (meta["fire_index"], index))
    with open(os.path.join(out_dir, stem + ".json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=1)
    return meta, bool(touched)


def default_script_for(skill, slot=0):
    """A first script for a row of skills.json: attack = lock on and cast; anything else = press
    the button once with nothing happening to the opponent."""
    cap = skill.get("capsule", "attack")
    distance, tail = {"attack": (60, 4.0), "defence": (30, 4.0)}.get(cap, (60, 3.0))
    return {"name": skill["name"], "skill": skill["id"], "slot": slot, "distance": distance,
            "lock": cap == "attack", "tail": tail,
            "timeline": [{"t": 0.3, "do": "press", "buttons": "YXBA"[slot], "for": 0.1}]}


# ======================================================================= batch + CLI
SKILLS_JSON = r"..\..\cache\skills.json"
OWNED_CSV = os.path.join(OUT_ROOT, "owned_skills.csv")
PILOT_IDS = (65, 3, 80, 49)      # Laser, Psycho Knife, Blaster, Speed Up (+ the Aura Particle script below)


def pilot_script_dicts(skills):
    """The five pilot takes: four documented skills plus the Aura Particle used while the stick is held."""
    by_id = {s["id"]: s for s in skills}
    out = []
    for sid in PILOT_IDS:
        if sid in by_id:
            out.append(default_script_for(by_id[sid]))
    out.append({
        "name": "Aura Particle held stick", "skill": 0, "slot": 0, "lock": False, "distance": 220, "level": 3,
        "attempts": 4,
        "lead_in": 0.5, "tail": 1.5,
        "timeline": [{"t": 0.0, "do": "stick", "x": 0.0, "y": 1.0, "for": 3.5},
                     {"t": 1.2, "do": "press", "buttons": "Y", "for": 0.1}]})
    return out


def scripts_from_list(path, skills, slot=0):
    """Scripts for every row of a skill-list CSV: its 'script' column (a JSON file) when given,
    otherwise the default script for that skill id."""
    by_id = {s["id"]: s for s in skills}
    out = []
    for row in read_skill_list(path):
        if row.get("script"):
            sc = load_script(row["script"])
        elif row["id"] in by_id:
            sc = parse_script(default_script_for(by_id[row["id"]], slot))
        else:
            sc = parse_script({"name": row["name"], "skill": row["id"],
                               "timeline": [{"t": 0.3, "do": "press", "buttons": "YXBA"[slot], "for": 0.1}]})
        out.append(sc)
    return out


def keep_awake(on):
    """Ask Windows not to start the screen saver / switch the display off while a run is going
    (the same call video players make). Not input; undone when the run ends."""
    try:
        flags = 0x80000000 | (0x1 | 0x2 if on else 0)   # ES_CONTINUOUS | SYSTEM_REQUIRED | DISPLAY_REQUIRED
        ctypes.windll.kernel32.SetThreadExecutionState(flags)
    except Exception:
        pass


def launch_game(wait_s=60):
    """Start the Store app the way tools/bats/PLAY_ORIGINAL_TRACED.bat does. Returns (pid, started_by_us)."""
    import subprocess
    pid = find_game_pid()
    if pid:
        return pid, False
    subprocess.Popen(["explorer.exe", r"shell:AppsFolder\Microsoft.MSEsper_8wekyb3d8bbwe!App"])
    for _ in range(wait_s):
        time.sleep(1)
        pid = find_game_pid()
        if pid:
            time.sleep(15)      # let it reach the title
            return pid, True
    return None, True


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--script", help="one script (JSON)")
    g.add_argument("--list", help="skill list CSV (id,name[,script])")
    g.add_argument("--pilot", action="store_true", help="the five pilot takes")
    g.add_argument("--dump-owned", action="store_true", help="only write owned_skills.csv and stop")
    g.add_argument("--dry-run-pilot", action="store_true", help="print the pilot plan, touch nothing")
    ap.add_argument("--out", default=os.path.join(OUT_ROOT, "takes"))
    ap.add_argument("--stage", type=int, default=DEFAULT_STAGE)
    ap.add_argument("--no-video", action="store_true")
    ap.add_argument("--unowned", choices=("include", "skip"), default="include")
    ap.add_argument("--reuse-battle", action="store_true", help="do not start a fresh match for every take")
    ap.add_argument("--leave-game", action="store_true",
                    help="do not close the game this run started, and do not restore the save from the snapshot")
    ap.add_argument("--no-resume", action="store_true", help="redo takes that already have a finished json")
    ap.add_argument("--skills", default=SKILLS_JSON)
    a = ap.parse_args(argv)
    skills = load_skill_table(a.skills)
    if a.dry_run_pilot:
        for d in pilot_script_dicts(skills):
            sc = parse_script(d)
            print("%-28s skill %3d slot %d distance %-5s lock %-5s ticks %d  events %s" % (
                sc.name, sc.skill, sc.slot, sc.distance, sc.lock, sc.record_frames, sc.events))
        return 0
    import mr_batch
    if a.dump_owned:
        scripts = []
    elif a.script:
        scripts = [(load_script(a.script), {})]
    elif a.list:
        scripts = mr_batch.scripts_from_csv(a.list, skills)
    else:
        scripts = [(parse_script(d), {}) for d in pilot_script_dicts(skills)]
    return mr_batch.run_full(scripts, a.out, skills, stage=a.stage, video=not a.no_video, unowned=a.unowned,
                             relaunch=not a.reuse_battle, tidy=not a.leave_game, resume=not a.no_resume)


if __name__ == "__main__":
    sys.exit(main())
