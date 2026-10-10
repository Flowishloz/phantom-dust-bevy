# FORMATS-CPU: the computer opponent's script VM, `.cad` nav graph, `.epa` difficulty rows

Lane `cpureads` (wave 6). Companion to `audits/P0-AI.md` (what the CPU does); this file is the byte-level
spec Lane B (`pd_formats` parser / VM) and Lane C (CPU host) build from.
Tags: **READ** = a program address (image base `0x140000000`, `decomp_all.c` or capstone assembly),
**UNCONFIRMED** = not settled (what a live trace would need is named). Nothing here is MEASURED or LIVE.
Tools (each with a `test_*.py`): `engine/re/ssb_dis.py` (disassembler, name hash, reachability, stack verifier), `ssb_dec.py` (goto-style
pseudo-C decompiler: `python ssb_dec.py eneprog006.ssb [offset ...]`), `ssb_run.py` (Python reference interpreter / test oracle with a fake-world
host), `cad_dump.py`, `threat_predict.py`. `python ssb_dis.py <file.ssb>` prints the disassembly.

Sections: 1 `.ssb` file format, 2 the VM (instruction set), 3 how a tick drives it, 4 native-call table (4.3 world natives,
4.4 threat predictor, 4.5 statuses), 5 `.cad` nav graph and the steering code, 6 `.epa` and the seat / AI info structs,
7 what stays UNCONFIRMED. Tools in `engine/re/`: `ssb_dis.py`, `ssb_dec.py`, `ssb_run.py`, `cad_dump.py`, `threat_predict.py`.

## 1. `.ssb` file format (READ: loader `FUN_1402aa620`, entry lookup `FUN_1402ab5d0`)

All integers little-endian 32-bit words. `eneprog006.ssb` is 28,004 bytes.

| word | file offset | meaning (on disk) |
|---|---|---|
| 0 | 0x00 | `0x20` = start of code. The loader overwrites this word at run time with a module slot id |
| 1 | 0x04 | **byte offset of the function table** = end of the code area (here `0x6d58`) |
| 2 | 0x08 | byte offset of the end of the function table = start of the import table (`0x6d64`) |
| 3 | 0x0c | byte offset of the string pool = end of the import table (`0x6d64`, empty pool) |
| 4..7 | 0x10.. | `0, 0xffffffff, 0x9b2f9158, 0` (not read by the VM) |

- **Code area** = `[0x20, word1)`. Instructions are 4-byte words (op `0x0f` is 8 bytes). **Read-only data tables
  live inside the code area** (between functions); they are reached only by op `0x1f` and are never executed,
  so a linear decode of the area misreads them as opcodes. Decode by control flow
  (`ssb_dis.reachable`). Example: the table at `0x1bcc` is 21 words `{13,20,0,0,0,20,0,0,0,13,25,20,15,15,0,0,0,13,25,0,0}`.
- **Function table** = `[word1, word2)`, entries of 3 words `(hash1, hash2, word_offset)`. `word_offset * 4` is the
  byte offset of the function **from the start of the file** (on disk). `eneprog006` has exactly one entry:
  `main` = `(0x8e225400, 0, 0x1ae3)` -> byte `0x6b8c`. All other functions are reached by relative `call`s
  (87 functions in total, found by `verify_stack`).
- **Name hash** (`FUN_1402847c0`, char table `0x1402f76a0`): per 6-character half, `h = h*40 + v(c)` with
  `v('0'..'9') = 1..10`, `v('a'..'z' / 'A'..'Z') = 11..36`, `v('_') = v('=') = 37`; a name shorter than six chars is
  padded with zeros (so `main` = `((((23*40+11)*40+19)*40+24)*40)*40` = `0x8e225400`). `hash1` = chars 1-6,
  `hash2` = chars 7-12. Stops at NUL or `.`. An unmapped char gives `-1`.
- **Import table** (`FUN_1402aa960`): 16-byte entries from word2 to word3, for op `0x04` (call another module).
  `eneprog006` has none; op `0x04` is never reached in it. UNREAD beyond that.
- **String pool** (op `0x11`): the pushed value is the address `file_base + word3 + s`. `eneprog006` pushes none
  (the debug print native `g4.f224` is only used by the older scripts).
- **Loader relocation** (`FUN_1402aa620`): in memory words 1-3 are rewritten self-relative (`word1 - 4`,
  `word2 - 8`, `word3 - 12`), each function-table offset is rewritten to `word_offset*4 - entry_field_address + base`
  (self-relative bytes), and a second array of `word1/4` words is allocated holding, for each code word, `0` or
  `0xffffffff` according to the sign of the word (used by `g0.f16`, section 4). A reimplementation can keep the file
  as is: base = file start, `main` at `table[0].word_offset * 4`.

All 16 files in `Assets/Data/com/ai` parse with these rules; each exports only `main`
(`eneprog000-005` -> `0x1ffc/0x1fec/0x201c/0x2004/0x23c0/0x23b4/0x2414`, `freya` `0x324c`, `chkprog000` `0x4a0`,
`motchk/rootchk` `0x20`). Only `eneprog006` is used by shipped quest data (P0-AI.md 0.2).

## 2. The VM (READ: `FUN_1402aaa30`, with the dispatch loop checked in assembly)

### 2.1 Machine state (the VM struct is embedded in the AI struct at `ai+0x20`)

| field | offset | meaning |
|---|---|---|
| flags | +0x00 | bit0 fault, bit1 halted (no code pointer / restart), bit2 yield-this-tick. Execution stops when `flags & 7 != 0` |
| pc | +0x08 | pointer to the next instruction word |
| sp | +0x10 | data stack pointer, **8-byte slots, grows down** |
| csp | +0x18 | call stack pointer (separate stack, grows up): each call pushes 3 qwords `(return pc, frame, module)` |
| frame | +0x20 | frame base = value of `sp` at the moment of the call |
| result | +0x28 | result register: written by every native call, read by op `0x10` |
| module | +0x30 | pointer to the script image in use |
| stack init | +0x38 / +0x40 | initial stack depth (qwords) and stack base |
| wait | +0x48 | wait counter: if non-zero on entry it is decremented and the VM returns at once |

Values are **64-bit signed integers**. A native call receives `sp` as its argument block; it reads each
argument as the low 32 bits of an 8-byte slot (`a_i = *(i32*)(sp + 8*i)`), **`a0` = the top of stack = the last
value pushed**. So arguments are pushed right to left: `push a1; push a0; native` is `f(a0, a1)`.
Return value of a native = `rax` (64-bit), stored to the result register; after it returns the VM pops `b1` slots
(`sp += 8*b1`), where `b1` in the instruction is the argument count.

**Function frames**: at a call the callee's frame base = `sp`. Arguments are therefore at `frame[0], frame[1], ...`
(`a0` at `frame[0]`), and `alloc k` reserves locals at `frame[-1] .. frame[-k]`. Op `0x09`/`0x0a` index
`frame[s]` with the signed 16-bit `s` (so `s >= 0` = argument, `s < 0` = local). A function returns with
`ret pop=N`: it pops `N = args + locals` slots; `ret ... +value` first pops the top slot into the result register.
The caller reads the return value with op `0x10` (`push result`). `ssb_dis.verify_stack` confirms that for all 87
functions of `eneprog006` (and for every function of all 16 shipped scripts) every path reaches every instruction with one
stack depth and every `ret` finds exactly the locals (and optional return value) on the stack: this is the check that the
semantics below are consistent. `ssb_run.py` (reference interpreter) runs `eneprog006` for 300 ticks against a fake world
without a fault and the CPU picks its target and casts.

### 2.2 Instruction word

`word = op | b1 << 8 | s << 16`: `op` = bits 0-7, `b1` = bits 8-15 (unsigned), `s` = bits 16-31 (signed 16).
Exception: op `0x01` (native call) uses `group` = bits 28-31, `fn` = bits 16-27, `b1` = bits 8-15 (argument count).
Relative targets are **relative to the word after the instruction, in words**: `target = (addr + 4) + 4*s`.
Notation: `S0` = top of stack, `S1` = the slot below it. Binary ops are written with `S1` as the LEFT operand
(compilers push the left operand first), so `sub` computes `S1 - S0`.

| op | name | semantics |
|---|---|---|
| 00 | nop | |
| 01 | native | `result = table[group][fn](sp)`; then if `flags&7` stop; else `sp += 8*b1`. Table: `*(0x1402ddd00 + 8*group)` -> array of code pointers |
| 02 | wait/ctl | `b1=0`: WAIT: set flags bit2 and `wait = s` (this tick ends; the next `s` entries return at once). `b1=1`: fault. `b1=2`: restart (pc=0, flags=2, csp/sp reset). `b1=3`: set the step budget to -0x4d2fa1ff (effectively unlimited). Not used by `eneprog006` |
| 03 | call | push `(pc_after, frame, module)` on the call stack; `frame = sp`; `pc = pc_after + 4*s` |
| 04 | call-import | call into another module by import index `s` (`FUN_1402aa960`). Not used by `eneprog006` |
| 05 | ret | pop `(module, frame, pc)` from the call stack. If `b1 != 0`: `result = pop()`. Then `sp += 8*s` |
| 06 | alloc | `sp -= 8*s` (reserve `s` local slots, uninitialised) |
| 07 | branch | `b1=0`: `pc = pc_after + 4*s`. `b1=1`: pop v; jump if `v != 0`. `b1=2`: pop v; jump if `v == 0`. `b1=3`: pop v; if `v == 0` jump **and** set yield (flags bit2); if non-zero continue. (`eneprog006` uses b1 = 0 and 2 only) |
| 08 | case | `if (u64)b1 == S0`: pop it and fall through; else `pc = pc_after + 4*s` (value stays). A switch is a chain of `case`s; the value is popped by the matching arm |
| 09 | push local | push `frame[s]` |
| 0a | store local | `b1=0`: `frame[s] = pop()`; `b1=1`: `frame[s] += pop()`; `b1=2`: `frame[s] -= pop()` |
| 0b, 0c | (invalid) | set the fault flag |
| 0d | pop | `sp += 8*s` |
| 0e | push imm16 | push sign-extended `s` |
| 0f | push imm32 | push the next word (signed 32); `pc += 4` extra. The only use in `eneprog006` is `-1083808154`, the bit pattern of the float -0.9 (floats are passed to natives as raw bits; see `vadd`, f21) |
| 10 | push result | push the result register |
| 11 | push string | push `module + word3 + s` (string pool address). Not used by `eneprog006` |
| 12 | add | `S1 + S0`, pop one |
| 13 | sub | `b1=0`: `S1 - S0`; `b1!=0`: `S0 - S1`. (006 uses b1=0) |
| 14 | mul | `S1 * S0` |
| 15 | div | `b1=0`: `S1 / S0`; else `S0 / S1`. Signed 64-bit, truncating; division by zero is not guarded |
| 16 | mod | `b1=0`: `S1 % S0`; else `S0 % S1`. Signed (C `%`) |
| 17 | compare | result 0/1, pop one, `b1=0` as below (`b1!=0` swaps the operands; 006 uses it once, with s=5): `s=1` `S1 != S0`; `s=2` `S1 > S0`; `s=3` `S1 < S0`; `s=4` `S1 >= S0`; `s=5` `S1 <= S0`; `s=6` `(S0 & S1) == S0`; any other `s` (0, 7..): `S1 == S0` |
| 18 / 19 / 1a | and / or / xor | `S1 op S0`, pop one |
| 1b / 1c / 1d | not / neg / lnot | in place: `~S0`, `-S0`, `S0 == 0` |
| 1e | shift | `b1=0`: `S1` shifted by `S0 & 63`; `b1!=0`: `S0` shifted by `S1 & 63`. `s=1` arithmetic right, `s=2` logical right, any other `s` left. Pop one. Not used by `eneprog006` |
| 1f | push data address | push `pc_after + 4*signed24(word >> 8)` (a **code-area address**: table pointer) |
| 20..ff | (invalid) | fault |

`eneprog006` uses ops 01 03 05 06 07 08 09 0a 0d 0e 0f 10 12 13 14 15 16 17 18 19 1f only. Operand-order notes
are cross-checked against script behaviour (e.g. `func_0048` "can I afford skill": `cost >= hp` is `cmp s=4` with
the cost pushed first and the result 0 means affordable).

**Data tables** are 32-bit signed words. A pointer from op `1f` is only dereferenced by `g0.f16`.

**Step budget**: `FUN_14017b220` passes 50,000. Lane `ssbvm` read the dispatch loop in assembly (module doc of
`pd_formats/src/ssb_vm.rs`, restore handlers such as `0x1402aae63`): the counter is decremented once per instruction at the loop
head and **restored** by the handlers of branches, `case`, push/pop/alloc/local access, comparisons, `push &data` and
value-returning `ret`; only `nop`, native calls, op `02`, calls, `ret` without value, arithmetic (`add sub mul div mod`), bit
ops and shifts consume it. (The Ghidra decompile above is lossy on this; the assembly reading wins.) `eneprog006` never comes
close to the limit; a tick that exhausts it stops where it is and the next tick's `main` entry nests on top.

## 3. How a tick drives the VM (READ: `FUN_14017b220`, `FUN_1402ab5d0`, `FUN_1402ab510`)

Per CPU player per 30 Hz tick (P0-AI 2.2): set `DAT_1404bc5f0 = ai + 0x2070` (the register file base) and
`DAT_1404bbea0 = ai`; look up export `main`; **push a call** to it (the VM's old `pc`, frame and module are saved on
the call stack, `frame = sp`, `pc = main`, `wait = 0`, halted bit cleared); run `FUN_1402aaa30(vm, 50000)`.
`main` returns to the saved `pc` (0 on a fresh VM, which halts it: "no code pointer"). So **every tick re-enters
`main` from the top; the only memory between ticks is the register file** (50 int32 registers, 20 vectors). The WAIT
op is never used, so the usual state is: stack and call stack empty between ticks.

- **Registers**: `R[i]` is an `int32` at `ai + 0x2074 + 4*i` (i = 0..49; `DAT_1404bc5f0 + 4 + 4*i`).
  Zeroed at creation by `FUN_14017afb0` (also all vectors).
- **Vectors**: `V[i]` is 3 floats at `ai + 0x213c + 12*i` (i = 0..19; `DAT_1404bc5f0 + 12*(i+0x11)`).
  Positions are in **world units** (our world units = the engine's); script distances are `world * 0.1`.
- `ai + 0x0c` = target player index (`set_target`), `ai + 0x10` = pointer to the nav controller `COM` (`P+0x142d0`),
  `COM + 8` = the player struct `P`.
- After `main` returns (`FUN_14017b220` tail): if the target index (< 4) names a live player, `P+0x13f20` := that
  player's stats `+0x60` and `P+0x13f1c` := 1 if the pair distance <= 80, 3 if > 300, else 2 (P0-AI 2.2);
  otherwise `P+0x13f20 := -1`.
- Then (still per tick, `FUN_1401775b0`): nav steering (section 5) and the pad output (P0-AI 2.2).

## 4. Native-call table

Convention: `g<group>.f<fn>`. The script never calls anything but group 0 (one function) and group 4. `a0` is the top of
stack (last pushed). "P" = the player struct (`DAT_1404bc9d8[p]`, 4 slots), "S" = the stats struct of player `p`
(`DAT_1404bc608[p]`), `COM` = nav controller, `R[i]` register, `V[i]` vector, `x` = int result. `p` outside 0..3 or an
empty slot generally gives 0 (noted where it gives -1). Names are ours; addresses are the native's entry.
Script-unit distance = world distance * 0.1 (`0.1f` at `0x1402f7cdc`), truncated toward zero.

**Arguments are listed in `a0, a1, ...` order, which is the REVERSE of the order the script pushes them.**

### 4.1 Group 0 (VM library)

| fn | name | semantics | status |
|---|---|---|---|
| g0.f5 | `rand(n)` | the same function as `g4.f32` (`FUN_14020dcf0`); not used by `eneprog006` (lane `ssbvm`) | READ `0x14020dcf0` |
| g0.f17 | `tbl_store(ptr, idx, v)` | writes both halves of a table word (`0x1402abe30`); not used by `eneprog006` (lane `ssbvm`) | READ |
| g0.f16 | `tbl(ptr, idx)` | returns the signed 32-bit table word at address `ptr + 4*idx` **sign-extended to 64 bits** (low half = the word, high half = the sign mask from the loader's side array). `ptr` comes from op `1f`. Used 13 times (the skill tables `T@1bcc/1e54/1f1c/23a4` and the per-stage tables from `0x31b8`) | READ `0x1402abdf0` |

### 4.2 Group 4 (battle API): the 75 natives of `eneprog006`

Pure helpers (state in `R`/`V` only):

| fn | name | semantics | address |
|---|---|---|---|
| f0 | `setvar(i, v)` | `R[i] = (i32)v`; returns 0 | `0x14020d460` |
| f1 | `getvar(i)` | returns `R[i]` sign-extended | `0x14020d480` |
| f2 | `vcopy(dst, src)` | `V[dst] = V[src]`; returns 0 | `0x14020d490` |
| f16 | `dist(a, b)` | `(int)(|V[a] - V[b]| * 0.1)` (3-D length, truncated) | `0x14020d4d0` |
| f21 | `vadd(v, dx, dy, dz)` | `V[v] += (f(dx), f(dy), f(dz)) * 10.0f` where `f(a)` is the argument slot's low 32 bits **reinterpreted as a float** (`movss`, no int conversion): the compiler pushes float constants as `push32` bit patterns; the script's only one is `-1083808154` = `0xbf666666` = **-0.9f**, so `vadd(v, 0, -0.9f, 0)` lowers V[v] by 9.0 world units (own capsule position -> its standing point); every other argument is integer 0 = 0.0f. Returns 0 | `0x14020d6f0` |
| f23 | `heading(a, b)` | degrees from point `a` to point `b`: `(int)((float)(int)(atan2f(Vb.x - Va.x, Vb.z - Va.z) * 5215.189) * 360.0f * (1/32768))`. Range about -180..180; 0 along +z | `0x14020d7b0` |
| f28 / f29 / f30 | `vx(v, k)` `vy` `vz` | `(int)(V[v].{x,y,z} * 0.1f * (float)k)` | `0x14020d900/940/980` |
| f32 | `rand(n)` | `((rand() & 0x7fff) * n) >> 15`, uniform `0..n-1` (MSVC `rand`) | `0x14020dcf0` |
| f33 | `angdiff(a, b)` | `d = a - b; if (d < -180) d += 360; return d` (only the low side is wrapped, so the range is -180..360) | `0x14020dd10` |
| f49 | `path49(deg)` | dodge request: `COM+0x40 = 1`, `COM+0x44 = (int)((float)deg * 91.02222)` (yaw units of 1/32768 turn); returns 0 | `0x14020df20` |
| f56 | `stuck()` | `COM+0x120` (ticks the body has not moved; see section 5) | `0x14020e300` |
| f82 / f85 | `style0() / style2()` | `COM+0x48 = 0 / 2`; returns 0 | `0x14020e5f0 / 0x14020e5d0` |
| f96 | `set_target(p)` | `ai+0x0c = p` | `0x14020e650` |
| f97 | `get_target()` | `ai+0x0c` | `0x14020e660` |
| f112 | `self()` | `P+0x14` (own player index) | `0x14020e670` |
| f208 | `TICK()` | global tick counter `DAT_1404bc63c` (30 Hz) | `0x140210ce0` |
| f210 | `nodecount()` | `cad+0x2ee4` (nav node count) | `0x140210d00` |
| f234 | `f234()` | `COM+0x110` (flag; writers: section 5) | `0x14020e340` |
| f425 / f426 | `stage()` / `stage10()` | stage code `DAT_1404c01d8` (`0x7777` -> -1); stage code / 10 (signed, `0x7777` -> -1 via the same test) | `0x140211ed0 / 0x140211ef0` |

World queries (need the match state; all `p` arguments are player indices 0..3):

| fn | name | semantics | address |
|---|---|---|---|
| f113 | `vpos_of(p, v)` | if player `p` exists, `P[p]+0x20 != 0` and `P[p]+0x10 == 0`: `V[v] = P[p]+0x12e40` (x,y,z); returns 0 | `0x14020e690` |
| f114 | `vstart_of(p, v)` | `V[v] = ` first 12 bytes of the start-point record of player `p`: `FUN_14012a5e0` returns `DAT_1405300a8 + 0x50 * ((idx & 3) % DAT_14052dd62)` with `idx = P[p]+0x18` (0 if the table is not set up); skipped when the slot is empty. Returns 0 | `0x14020e700` |
| f115 | `yaw_of(p)` | `(int)((float)(i32)P[p]+0x12e68 * 360 * (1/32768))` (degrees), 0 if absent | `0x14020e760` |
| f117 | `pairflag(a, b)` | int32 at `0x1404bbe20 + 8*(b + 4*a)` (pair table; the float at +4 is the pair distance) | `0x14020e7f0` |
| f118 | `hp(p)` | `S[p]+0x0c` (Health), 0 if none | `0x14020e810` |
| f119 | `maxaura(p)` | `S[p]+0x18` | `0x14020e830` |
| f120 | `aura(p)` | `(int)(float)S[p]+0x1c` | `0x14020e850` |
| f121 | `has_status(p, id)` | `FUN_14024d4d0(S[p], id)` (0 if none) | `0x14020e880` |
| f122 | `team_of(p)` | if `DAT_1404bc9d4` is 4 or 5: `team_tbl[p] == 0` as 0/1 (table `0x140527980`); else `P[p]+0x1c`, or -1 for an empty slot | `0x14020e8c0` |
| f123 | `f123(p)` | see 4.3 | `0x14020e920` |
| f124 | `cond(p)` | 0 absent / 1 idle / 2 casting (state 10..12) / 3 state 0x11 / 4 acting / 5 out / 6 not entered; see 4.3 | `0x14020e980` |
| f125 | `cast_skill(p)` | `P[p]+0x12f5c` while `P[p]+0x12f04` is 10..12, else -1 (also -1 if `P[p]+0x10 != 0`) | `0x14020ea50` |
| f134 | `dodge_skill(p)` | `charset[P[p]+0x12c70 (u16)].dodge` = int at `0x1404c024c + 0xa0*idx`; 0 if empty | `0x14020ed30` |
| f135 | `f135(p)` | `FUN_14012e160(P[p], P[p]+0x12e40, P[p]+0x12e5c)` if `P[p]+0x10 == 0`, else -1 (nearest pickable stage object, section 4.3) | `0x14020ed70` |
| f144 | `slot_skill(p, k)` | hand slot `k`: skill id at `S+0x378+0x50k` if `S+0x350+0x50k != 0`, else -1 | `0x14020ee00` |
| f146 | `slot_ctype(p, k)` | capsule type `s9` of that skill (skill record word `+0x0e`, stride 0x90), -1 if empty | `0x14020ee70` |
| f147 | `slot_cost(p, k)` | `S+0x360+0x50k` (Aura cost) | `0x14020eec0` |
| f148 | `slot_ready(p, k)` | 1 iff occupied and `S+0x36c+0x50k == 0` and `S+0x388+0x50k == -1`, else 0 | `0x14020ef00` |
| f150 | `slot_state(p, k, x)` | 1 iff occupied and `S+0x36c+0x50k == x` | `0x14020ef90` |
| f161 | `sk_ctype(s)` | skill record `+0x0e` (s9) | `0x14020f2a0` |
| f162 | `sk_sel(s)` | skill record `+0x52` (s43: 0 attack, 2 defence) | `0x14020f2c0` |
| f165 | `sk_class(s)` | `FUN_140167970(s)`, 3 mapped to 4 | `0x14020f330` |
| f166 | `sk_motion(s)` | `+0x7a` (s63) if `+0x52 == 0`, else -1 | `0x14020f350` |
| f168 | `sk_s44(s)` | `+0x54` if `+0x52 == 2`, else -1 | `0x14020f3b0` |
| f169 | `sk_hpcost(s)` | `+0x2e` (s25 Health cost) | `0x14020f3e0` |
| f175 | `sk_aitype(s)` | `+0x50` (s42, the AI type) | `0x14020f4d0` |
| f176 / f177 | `scan_reset() / scan_next()` | cursor `R[-1]` (= `*DAT_1404bc5f0`) := 0 / advance to the next world object (4 players x 48 objects, `P+0x4e0 + 0x1f0*slot`) that is active (`+0x10 != 0`), whose skill has s43 == 0 and s63 not in {3,4,12,13,14}; returns its object number (`player*48 + slot`) or -1 when past 191. It does **not** test the owner's team | `0x14020f550 / 0x14020f560` |
| f178 | `obj_skill(o)` | `obj+0x14` (skill id) via `FUN_14020f4f0`, or -1 | `0x14020f6a0` |
| f179 | `obj_pos(o, v)` | `V[v] = obj+0x100` (x,y,z) | `0x14020f6d0` |
| f186 | `obj186(o)` | predicted ticks until object `o` reaches me (0 = harmless): wraps `FUN_14020f9a0` (section 4.4) | `0x1402105a0` |
| f192 | `cap_id(p, k)` | capsule record `(3*p + k)` of the table at `0x140585c90` (stride 0x48): `+0x18` (skill id) if `+0x0c == 2`, else -1 | `0x140210a50` |
| f193 | `cap_pos(p, k, v)` | `V[v] = record(3*p+k)+0` (x,y,z) | `0x140210a90` |
| f194 / f195 | `f194() / f195()` | `P+0x17160 / P+0x17164` (own capsule-contact state / index; script: 1 own capsule, 3 foreign) | `0x140210b10 / 0x140210b30` |
| f197 | `cap_skill()` | skill id of the next capsule in the global list `0x14058bd30` (stride 0x1c; `FUN_14017c4c0` finds it) or -1; special case `DAT_1404bc950 == 6` with `DAT_1404bbed8 > 0`: that value | `0x140210b70` |
| f198 | `cap_due()` | in the global capsule list (4 entries, stride 0x1c, see 4.3) take entry `i = FUN_14017c4c0(list)`; returns 1 iff `i != -1`, its id `!= -1`, (its expiry tick `== -1` or `> now`) and `now > birth_key + D` where `D` = skill record word `+0x56` if that skill's `+0x52 == 5` else -1 | `0x140210be0` |
| f199 | `cap_pos(v)` | `V[v] =` that capsule's position (`0x14058bd40`) | `0x140210c60` |
| f200 | `cap_index()` | `FUN_14017c4c0(&DAT_14058bd30)` | `0x140210cc0` |
| f229 | `f229(p, x)` | `FUN_14024d7a0(S[p], x)`; returns 0 (section 4.3) | `0x140210e80` |
| f235 | `respawn(p)` | if `P[p]` exists, `+0x20 != 0`, `+0x10 == 0`, `p < 4`: `COM+0x120 = 0` then `FUN_1402501d0(P[p], start_point_record(P[p]+0x18), P[p]+0x12e58)` (puts the player back on its start point, facing `P+0x12e58`). Returns 0 | `0x140210f70` |

Actions on the pad / nav controller:

| fn | name | semantics | address |
|---|---|---|---|
| f19 | `MOVE_TO(v)` | if `COM+0x10 == 0`: `COM+0x50 := V[v]` (steer target) and `COM+0x5c := V[v]` (goal). Returns 0 | `0x14020d660` |
| f20 | `nav_step(n)` | if `(node[n].flags & 0xc) == 0` (flags byte = node `+0x00`, see 5): `FUN_14020d100(COM, n)` (sets `COM+0x50 = COM+0x5c = centroid(n)` when `COM+0x10 == 0`) and return 1; else 0 | `0x14020d6b0` |
| f48 | `PRESS(slot)` | `if has_status(S_of_COM, 0x24)`: `P+0x14264 \|= P+0x14288[ P+0x14278[slot] ]`, else `P+0x14264 \|= P+0x14288[slot]`; returns 0 | `0x14020deb0` |

Nav-graph natives (semantics in 5.4): f18 `navnode_at(v)` = loose `node_at(V[v])` (`FUN_140176f90`, `0x14020d600`), f31
`navnode2(v)` = strict `node_at` (`FUN_140176e90`, `0x14020d630`), f24 `ground_ok(a, b)` = `FUN_140179f80` (`0x14020d830`), f37
`nodedist(a, b)` (`0x14020daf0`: distance between the centroids of the four corner vertices of nodes `a` and `b`, times 0.1),
f51 `path51(avoid, goal)` = `FUN_14017a1a0(COM, V[avoid], V[goal])` (`0x14020df90`; 1 = a route was written, 0 = none),
f53 `probe(deg)` (`0x14020e020`), f60 `path_step()` (`0x14020e380`; 0 / 1 / 2).

### 4.3 Details of the world natives (READ unless marked)

- **Players and stats.** `P = DAT_1404bc9d8[p]` (the fighter object, class `pdNormalPlayerClass`, vtable `0x14048e3b0`, update =
  slot `+0x20` = `FUN_14024ec80`), `S = DAT_1404bc608[p]` (its stats block; `S+0x61a0` points back to `P`). Slots 0..3.
  `P+0x14` player index, `P+0x1c` team, `P+0x30` **human flag** (0 = CPU seat), `P+0x34` "controls enabled", `P+0x10`
  (non-zero = the slot is absent / removed), `P+0x20` / `P+0x24` active and in-the-match flags, `P+0x28` entered flag.
- **`cond(p)` (f124)**, exact order: slot empty or `P+0x20 == 0` -> 0 (absent); `P+0x28 == 0` -> 6 (not entered); `P+0x20 == 0 ||
  P+0x24 == 0` -> 5 (out); if `P+0x10 != 0` -> 0; state `P+0x12f04` in 10..12 -> 2 (casting); state 0x11 -> 3;
  otherwise `4` if the class predicate (vtable `+0x58` = `FUN_140250180`) is non-zero else `1`. The predicate returns 0
  ("not acting") only when **all** of: (state not in 15..18 or state == 0x10), (state != 12 or `P+0x12f98 > 0` or
  `P+0x12f9c < 1` or `P+0x12fa0 == 0`) and `P+0x6434 == 0`; otherwise 1 (so states 15, 17, 18 and a few others count as
  "acting" = busy / reacting).
- **`team_of(p)` (f122)**: in the 4/5-team match modes (`DAT_1404bc9d4` 4 or 5) it returns `1` iff `team_tbl[p] == 0`
  (table `0x140527980`), else 0 (inverted on purpose); in every other mode it returns `P[p]+0x1c`. -1 for an empty slot.
  The versus setup (`FUN_1401d0750`) writes `P+0x1c = seat index` (free-for-all teams).
- **`f123(p)`** (script: "<= 1 means too low to hunt capsules"): `X = S[p]+0x61a0` (the player); if `X+0x34 != 0` then
  `arr_b[p] - arr_a[p] + 30` (int arrays `0x140585c20` and `0x140585c00`, one entry per player) else if `X+0x10 == 0` then
  `X+0x17158` (int) else 0. In versus `P+0x34 != 0`, so the first branch applies. What `arr_b - arr_a` counts (a respawn /
  cooldown pair) is UNCONFIRMED; the script only tests `<= 1`, so a host that returns a large value never blocks capsule hunting.
- **`f125 cast_skill(p)`** returns the skill id being cast (`P+0x12f5c`) while the state is 10..12, else -1.
- **`f134 dodge_skill(p)`**: `charset[P+0x12c70 (u16)]` record stride 0xa0 at `0x1404c024c`, field = the dodge skill id
  (750 or 751 in the shipped character sets); the script dodges only when it is not 750 (P0-AI 2.7).
- **`f135` = nearest pickable stage object.** `FUN_14012e160(P, P+0x12e40, P+0x12e5c (yaw))`: walks the stage-object list
  (`DAT_1405322c8`, `DAT_1405322bc` entries, class `pdStageObjStructBase`), keeps objects whose vtable `+0x98(1)` is true (the base class implementation `FUN_1401d9ff0` returns **bit n of the object's flag word at `+0x54`**; derived `pdStageObj*` classes may override; 178 class names in the exe) and
  that lie within 100 u (distance^2 < 10000, `0x1402f825c`), sorts them by distance, and returns the id (u16) of the nearest one
  that is inside a +-0x1000 yaw cone (45 degrees) of the player's facing **or** closer than 25 u (distance^2 <= 625, `0x1402f81f0`);
  -1 if none. This is what the Psycho Kinesis rule (s42 = 19) tests: "is there a throwable stage object in front of me".
  Which stage-object classes / kinds have flag bit 1 set is UNCONFIRMED (the flag word is built from the placement's class / kind bytes, `FORMATS-STAGES`
  placements; live trace: log the returned id with the object's class and kind byte).
- **`f229(p, x)`** = `FUN_14024d7a0(S[p], x)`: **Health += x** (clamped to the maximum `S+0x10`; at <= 0 the fighter dies via
  `FUN_14024e000`). The script calls it once, with -100, when slot capture finds all four hand slots unusable (every slot holds
  Killer Satellite 85 / Judgment 282 / Invulnerability 298 or is button-frozen by status 13+k): **the CPU kills itself**. A port
  should keep it (it can never fire while a freeable slot exists).
- **`f176/f177` scan**: the cursor is the first int of the register file (`*DAT_1404bc5f0`, i.e. `R[-1]`); reset to 0; `scan_next`
  walks objects 0..191 (`player = n / 48`, `slot = n % 48`, object = `P[player]+0x4e0+0x1f0*slot`) and returns the first whose
  `+0x10 != 0` (active), whose skill record has `+0x52 == 0` (attack) and whose motion `+0x7a` is not 3/12/13/14 and not 4.
  It does **not** test the owner, so the script also sees its own projectiles; `f186` rejects them through the object's hit
  mask `+0x24` (bit `me`).
- **Capsule list (`f197-f200`, `FUN_14017c4c0`)**: 4 entries of 0x1c bytes at `0x14058bd40`: `+0` position (3 floats), `+0xc`
  skill id (-1 empty), `+0x10` ordering key (birth tick), `+0x14` expiry tick (-1 none), `+0x18` unread. `FUN_14017c4c0` sorts the
  four indices by the key (largest first) and returns the first entry that has a skill and is not past its expiry (the returned
  index is in `eax`; Ghidra shows `void`). `f197` returns that entry's skill id or -1 (also -1 when `DAT_1404bc5dc == 5`; when
  `DAT_1404bc950 == 6` and `DAT_1404bbed8 > 0` it returns `DAT_1404bbed8`), `f200` the index.
  Ownership (P0-AI 5 item 7): this is **one global list, shared by all fighters** (nothing in the list names an owner); the
  per-player list `f192/f193` (3 per player, table `0x140585c90`, stride 0x48: `+0` position, `+0xc` state (2 = placed), `+0x18`
  skill id) holds the capsules **deployed for that player** (the script calls them its "own capsules"; `FUN_14014ee70` fills the table per seat at match
  start). The script's capture modes use the global list for "other" capsules and the per-player list for its own (P0-AI 2.3, modes 8 / 9). Whether the engine lets any fighter step on any placed capsule is the
  human rule (our `capsule` code); the AI adds no restriction.

### 4.4 The threat predictor `FUN_14020f9a0` (g4.f186), READ from the assembly

`obj186(o)` = `FUN_14020f9a0(o, me = P+0x44, &pos, margin = P+0x12e9c, ground_ok)` via the wrapper `0x1402105a0`:
`pos = (P.x, P+0x12e44 + P+0x12e98, P.z)` (own position lifted by the aim height `P+0x12e98`), `margin` = the body radius
`P+0x12e9c` (a float), `ground_ok = FUN_140179f80(pos, O.pos)` (the nav LOS test between me and the object, section 5.5).
`me` is `P+0x44` = `S+4` (the stats block is embedded at `P+0x40`): the player's own index. Evidence: Decoy stores `attacker S+4 + 1` as status 9 and the
script reads that value back as a player index; `FUN_14017b220` indexes the pair table with it. No constructor writes it in the code read (it starts at -1 in
`FUN_14024d020`), so READ by inference; verify live with the value per seat. The hit-mask bit and the homing-target compare below use it. The function returns **ticks until impact** (int; 0 = harmless; may return values <= 0 for an
object already inside the margin).

Common prefix: `O = FUN_14020f4f0(o)` (`+0x10` state must be non-zero; `(O+0x24 >> me) & 1` must be set; the skill record
(`DAT_1404bdc50 + 0x90*O+0x14`) must have `s43 == 0`, motion `s63` not in {3, 12, 13, 14} (`FUN_140167910`) and not 4) else 0.
If `O+0x10 == 3`: `straight_shot(pos, O.pos, margin, O.radius, O+0x1c0)`. Otherwise by motion `s63`, with `m2 = margin + O+0x190`
(radius) and `T = O+0x48` (a pointer to the object it homes on; its `+0xb8 -> +4` is that player's index):

| s63 | rule (world units and ticks; `v` is per tick unless noted) |
|---|---|
| 0, 11 | `v = O+0x1c0` (s63 0) or `O+0x1d0` (11). **T set**: not me -> 0; unless skill `s55` (+0x6a) is 2 or 3 the ray `FUN_1401641b0(pos, O.pos)` must be clear; `n = unit(pos - O.pos)`; need `n . O.dir > 0.5` (`O+0xf0`); `t = trunc((|pos - O.pos| - m2) / v)`; `t > O+0x18` (age) -> 0, else `t`. **T null**: free flight: `E = O.pos + O.dir * v * (float)O+0x18`; `C` = closest point of segment `O.pos..E` to `pos`; `m2 <= |pos - C|` -> 0, else `trunc(|O.pos - C| / v)` |
| 1 (arc / lob) | T set: me -> `int O+0x1d4`, else 0. T null: `a = O+0x1cc` (float), `vy = O+0x1c4`, `y0 = O.y`, `py = pos.y`; `ta = vy / a`; `apex = abs(a*ta*ta*0.5) + y0`; `py > apex` -> 0; `fall = sqrt((apex - py)*2 / a)`; `t = ta + fall`; (`py > y0` and `vy < 0`) or `t <= 0` -> 0; landing `E = O.pos + (sin(yaw), 0, cos(yaw)) * (t * O+0x1c0)` with `yaw = O+0x1d0` (int, 32768 per turn); `(s16)s52 * 0.1 <= hypot(pos.x - E.x, pos.z - E.z)` -> 0, else `trunc(t)` |
| 2 (falling / dropped) | T null: free flight as s63 0 with `v = O+0x1c0`. T = me: ray as above (skipped when `s55` is 2/3); need `pos.y < O.y` (I am below it); `trunc((|pos - O.pos| - m2) / O+0x1c0)` (no age test). T other -> 0 |
| 5 (ground-bound) | needs `ground_ok != 0`. T null: free flight with `v = O+0x1c4`. T = me: `trunc((hypot(dx, dz) - m2) / O+0x1c4)` (horizontal only). T other -> 0 |
| 6 | T = me -> `int O+0x1cc`; else 0 |
| 7, 15, 16 (delayed) | caster (`O+0x40 -> +0x61a0 -> +0x14`) is me -> 0; else `O+0x18 - (s16)s65` (`s65` = skill `+0x7e`, `FUN_14020d050` / `FUN_14020d0c0`) |
| 9 | `straight_shot(pos, O.pos, margin, O.radius, (s16)s64 * 0.1)` (`s64` = skill `+0x7c`, `FUN_14020d080`) |
| 10 (ground line) | needs `ground_ok`; flatten `pos` and `O.pos` to y = 0; segment `O.pos .. O.pos + dir * ((float)age * O+0x1c4)` (the end's y is `dir.y * w`, without a start y); `m2 <= dist(pos, segment)` -> 0, else `trunc((hypot2d(O.pos, pos) - m2) / O+0x1c4)` |
| 3, 4, 8, 12, 13, 14, other | 0 |

`straight_shot(A, B, margin, radius, speed)` (`FUN_14020f8c0`): 0 if the ray `A..B` is blocked; `d = |A - B| - margin`; 0 if
`speed < d` (speed here is **per second**: an object that cannot cover the gap in one second is ignored); else
`max(1, trunc((d - radius) / (speed / 30)))`. `trunc` = `cvttss2si` (toward zero, NaN -> INT_MIN). `FUN_1401641b0(.., A, B, 0)` is the
stage collision ray (1 = clear; always 1 when the stage code is `0x7777`). Constants: 0.5 (`0x1402f7d40`), 30.0 (`0x1402f7fd8`).
Executable reference with unit tests: `engine/re/threat_predict.py` (`test_threat_predict.py`). LIVE needs: log `f186`'s argument
and return for every tick a projectile of each motion kind is in flight (return vs the real remaining flight time); also which
real attacks populate `O+0x1c0..0x1d4` for each motion (our sim's projectile state must provide speed, age, gravity, yaw).

### 4.5 Status ids the script reads (`f121 has_status(p, id)` = `FUN_14024d4d0`)

`FUN_14024d4d0(S, id)` = clamp(`S+0x61b8+12*id` (permanent) + `S+0x61bc+12*id` (timed amount) + `FUN_140271660(id)`, +-limit) with the
per-id limit table at `0x1402dcf30` (47 ids: 0: 9; 1-3: 90; 4-8: 1; 9: 4; 10: 4; 11-20: 1; 21, 22: 10; 23-33: 1; 34: 900; 35, 36,
37: 1; 38, 39: 10; ...). Ids read by `eneprog006`:

| id | meaning | writer (READ) |
|---|---|---|
| 0, 1, 2, 3 | attack STR / speed % / homing % / aura-recovery % modifiers (the type-72/76/77 rules compare with 0; the movement state reads `status(1)/100 + 1` as its speed scale) | add codes 12-15 and the self-skill selectors (`FUN_140272bc0`, `FUN_140270b00`) |
| 5 | skill buttons frozen (the script presses nothing) | add code 7 |
| 6 | poison (+1 Health in the affordability test) | add code 8 |
| 8 | invisible (Optical Camo family): the fire gate skips targets under 8 or 12 | self-skill selectors 12 / 13 (`FUN_140270b00` cases 0xc, 0xd) |
| 9 | **Decoy**: value = attacker's player index + 1 (limit 4); the script targets that player | add code 40 (`FUN_140272bc0` case 0x28) |
| 12 | second invisibility-type status (zeroed with 8 and 11 when a fighter dies) | UNCONFIRMED which skill sets it |
| 13 + k | button k (0..3) frozen by Frostbite / Freeze Shot | add code 16 |
| 17 | unused in shipped data: written only by add code 19, which no registered skill carries | `FUN_140272bc0` case 0x13 |

Other statuses that act on the CPU's pad output: **4** (bind: no walking; add code 6), **0x23 = 35** (inverted controls: the movement code
adds 0x4000 (90 degrees) to the stick angle; add code 37), **0x24 = 36** (Dazzle: `PRESS` remaps the slot through the shuffle table
`P+0x14278`; add code 41, `FUN_1402568a0` shuffles it).

### 4.6 Natives used only by other scripts (not needed for `eneprog006`)

`g4.f50, f52, f59, f83, f152, f153, f171-f173, f183, f224-f232, f237-f239` (old scripts, checkers, debug print `f224`).
Lane B only needs `eneprog006`; the other scripts parse and can be decoded with `ssb_dis.py` but are story / developer only.

### 4.7 What a host must supply (checklist for the CPU host lane)

Per call the script needs only the following engine state; everything else is script registers (`R`, `V`) that the host stores as plain arrays.

| area | natives | host data |
|---|---|---|
| time / random | f208, f32 | the 30 Hz tick counter; a seeded RNG (`rand(n)` uniform `0..n-1`) |
| fighters (4 slots) | f112, f113, f114, f115, f117, f118-f125, f134, f135 | player index, position (x, y, z) and facing, start point, Health, max Aura / Aura, team, `cond`, cast skill, statuses 0-3, 5, 6, 8, 9, 12, 13+k, 17, character dodge record id, pair LOS flag + distance |
| hand slots | f144-f150 | per slot: skill id (-1 empty), Aura cost, ready (0 = usable), slot state (2 / 4 are accepted by capture / defence checks) |
| skill table | f161, f162, f165, f166, f168, f169, f175 | fields s9, s43, s63, s44, s25, s42 and the delivery class (`FUN_140167970`) |
| projectiles | f176-f179, f186 | an object list (4 players x 48 slots) with the fields of FORMATS-CPU 4.4 |
| capsules | f192-f200 | the global list (4 entries: position, skill id, birth tick, expiry) and the per-player list (3 per player: position, state, skill id), contact state / index of the body |
| nav | f18-f24, f31, f37, f19, f20, f51, f53, f56, f60, f82, f85, f210 | the `.cad` of section 5, `COM` state, a stage ray (line of fire) for `ground_ok`, jump points |
| stage | f135, f425, f426 | pickable stage objects, the stage code |
| output | f48, f49, f96, f97 | `PRESS(slot)` into the seat's input, a dodge request, the target index (feeds the lock block: P0-AI 2.2) |

## 5. `.cad` (nav graph) and the steering code

Tools: `engine/re/cad_dump.py` (reader, `node_at` strict / loose, `ground_ok`, route lookup, invariant check; `test_cad_dump.py`). On the shipped
files `node_at(centroid(n))` returns `n` for 655 of the 662 nodes of stages 2-8 (the other seven: six nodes of st05 where another node answers, one st08 node
that no quad contains), which checks the point-in-quad and plane-height rules; `ground_ok` between the centroids of two nodes joined by a plain edge holds for
60-85 % of the edges (the straight line often misses a portal that is only part of a long boundary, which is geometry, not a defect). All 12 shipped
`comdata*.cad` of stages 2-8 pass `Cad.check()` (indices in range, every stored route stays on the graph).

### 5.1 Which file, and what happens without one (READ: `FUN_140176420`, `FUN_140176630`, `FUN_140187680`, `FUN_140192780`)

`com\st%02d` with `%02d` = **stage code % 10** (`DAT_1404c01d8`) and file `comdata%d.cad` with the **stage version**
`DAT_1404c51f0` (the stage-select screen's variant index 0..n; a random stage forces 0; `FUN_140192780`). Also loads
`camdata.cam` (776 bytes; camera data, not used by the CPU). Both files are read into a 0x180000-byte block; the program uses the
`.cad` bytes in place (`DAT_1404c0180` = its base), then `FUN_140176710` runs once (5.3). If a folder or file is missing, the
loader never completes: `DAT_1404c0180` stays 0 and `FUN_14024ec80` skips the CPU tick (the seat is inert).

| stage code (mod 10) | folder | files | nodes | notes |
|---|---|---|---|---|
| 2 | st02 | comdata0, comdata1 (identical) | 72 | |
| 3 | st03 | 0, 1 (identical) | 76 | |
| 4 | st04 | 0, 1 (identical), **2** | 85 (v0/1), **23 (v2)** | the 23-node v2 matches the script's class-3 table (`func_3004`: stage code 24 has 23 entries, and 24 % 10 = 4). How the stage-select list pairs a stage code with a version is not read here (the list is built at run time) |
| 5 | st05 | 0, 1 (identical) | 57 | |
| 6 | st06 | 0, 1 (identical) | 122 | |
| 7 | st07 | 0, 1 (identical) | 127 | |
| 8 | st08 | 0, 1, 2 (identical) | 123 | |
| 0, 1, 9 (codes 9, 10, ...) | none | | | no nav data: AI inert. `st01/comdata.cad` (6,252 bytes) and `st24/comdata2.cad` (88 vertices, 77 nodes) are never named by the loader |
| 12-17, 22-26 | folders hold only `camdata.cam` | | | a stage code 12 loads folder `st02`, not `st12` |

A stage version with no such file (e.g. v3) is a missing file: no CPU. All 19 versus arenas map to codes with a folder (P0-AI 2.1).
The mapping from our arena ids to the program's stage codes is not part of this lane (use the stage list in `DECISIONS`/`FORMATS-STAGES`).

### 5.2 Layout of the 674,312-byte file (READ: addresses of every accessor listed with the record)

| offset | size | contents |
|---|---|---|
| `0x0000` | u32 | vertex count V (<= 1000) |
| `0x0004` | V x 12 | vertices (3 x f32 x, y, z, world units); capacity 1000, up to `0x2ee4` |
| `0x2ee4` | i32 | node count N (<= 128) |
| `0x2ee8` | N x 0x54 | nodes; capacity 128, up to `0x58e8` |
| `0x58e8` | i32 | edge count E (<= 512) |
| `0x58ec` | E x 0x2c | edges; capacity 512, up to `0xb0ec` |
| `0xb0ec` | 0x2d4 | **not read by any nav function** (header `i32 21` + a point + 21 records of 6 words; looks like editor data; UNREAD) |
| `0xb3c0` | i32 | jump-point count J (<= 30) |
| `0xb3c4` | J x 16 | jump points: 3 x f32 position + 1 word (0 in the files seen); capacity 30, up to `0xb5a4` |
| `0xb5a4` | 0x10000 | route table **T0** |
| `0x1b5a4` | 0x10000 | route table **T1** |
| `0x2b5a4` | 0x10000 | route table **T2** |
| `0x3b5a4` | 0x10000 | route table **T3** |
| `0x4b5a4` | 0x10000 | route table **T4** |
| `0x5b5a4` | 128 bytes | per-node **hub class** byte (`0xff` = none) (`g4.f54`, `0x14020e200`, returns it; `g4.f55` tests whether a point's node is on the route ring; `eneprog006` uses neither) |
| `0x5b624` | u32 | route pool length in bytes (e.g. 0x5517 for st05) |
| `0x5b628` | to end | route pool: byte runs of node ids, each run ends with `0xff`; the rest of the file is zero |

**Node (0x54 bytes)**: `+0x00` u32 flags (bit 0 `NF_TEST`, bit 1 `NF_JUMPZONE`, bit 2 `NF_BLOCKED`; the code tests `& 0xc` for
"blocked", bit 3 is never set in the files; values seen: 0, 1, 2); `+0x04` 8 x i32 jump-link list (5.5); `+0x24` 4 x i32 corner
vertex indices (a convex quad, either winding); `+0x34` 8 x u32 (pairs of u16; **not read by the program**; UNREAD).
**Edge (0x2c)**: `+0x00` portal endpoint 0 (3 x f32), `+0x0c` portal endpoint 1 (3 x f32), `+0x18` i32 node A, `+0x1c` i32 node B,
`+0x20` u32 flags (bit 0 one-way: usable only A -> B; bit 1 dynamic door: re-tested every tick; bit 2 closed, set at run time),
`+0x24` f32 length (cost used when summing route lengths), `+0x28` i32 **gate node** (0 = none; the edge counts as connected only
while that node is not blocked, `FUN_140179e30`). Edges in the files: flags 0, 1, 2, 3; gates only in st03 / st04.
**Route tables**: entry `(from * 128 + to) * 4` is an i32 byte offset into the route pool (or -1). The run at that offset is the
node list **after `from`, up to and including `to`** (e.g. T0 st05 0 -> 10: `[2, 5, 6, 3, 18, 7, 8, 9, 10]`). Table use (READ):
**T0** the default walking route; **T2** the route for planner style 2 (`COM+0x48 == 2`, falling back to T0 when -1);
**T4** the "jump" route used when `COM+0x48 != 0` or by the planner's second pass (`FUN_140178d00` with mode 1); **T1** and
**T3** are fall-backs tried by the splice code (`FUN_140179420`, `FUN_1401796b0`, order T0, T1, T3, then T4 if allowed).
The exact definition of each table (which kinds of edges the offline generator allowed) is UNREAD; in st02-st04 every stored
step is an edge of the graph, in st05-st08 some steps are jump hops (no edge).
**Hub class** byte `c` of node `n`: the node index of the representative ("hub") node of `n`'s area (e.g. st05: 22 / 39 / 48);
`0xff` none. `path51` with no goal point routes to the hub of the point's node (`FUN_14017a1a0`, `FUN_14017a750`).

Edge counts per file are in the 5.1 table; for example st05 has 57 nodes, 71 edges, 18 jump points, 5 jump-zone nodes (flags 2)
and 19 edges with the one-way flag.

### 5.3 Runtime flags (READ: `FUN_140176710`, `FUN_1401e2850`, `FUN_140176980`)

- **After load** (`FUN_140176710`, once): for every node with flag bit 0 (`NF_TEST`): first `FUN_1401e2850` asks every stage object
  (list `DAT_1405322c8`, vtable `+0x68(corners)`) whether it overlaps the node's quad: a result of 1 sets node flag **8** (and caches it),
  a cached "blocked" bit sets flag **4**; if no object claims the node, a stage-collision ray from 10.0 u above the node centroid to
  20.0 u below it (`0x1402f7f7c`, `0x1402f7fc4`) must hit floor, else the node gets flag **4**. The nav code treats `flags & 0xc` as
  blocked. Which object kinds answer `+0x68` is UNCONFIRMED (live trace: dump the node flags after load for each arena).
- **Every match tick** (`FUN_140176980`, called from the master match tick `FUN_14012bbb0`): for each edge with flag bit 1 (dynamic door)
  cast a line-of-fire ray (`FUN_1401641b0`) across the portal: from `mid - n*5` to `mid + n*5`, where `mid` is the portal midpoint raised
  by 7.5 u (`0x1402f7f58`) and `n` the unit XZ normal of the portal (`FUN_140176b80`); a blocked ray sets the edge's closed bit 2, a clear
  ray clears it. Nothing else changes a node's flags after load.

### 5.4 The queries the script uses (READ)

All use `V[i]` points (3 floats) and the base `DAT_1404c0180`. `strict` = `FUN_140176e90` (f31), `loose` = `FUN_140176f90` (f18).

- **`inside(n, p)`** (`FUN_1401771b0`): the node must not be blocked (`flags & 0xc` = 0). With the quad corners `v0..v3` (indices at
  node `+0x24`), for k = 0..3 compute `cross_k = e.x * d.z - d.x * e.z` with `e = v[k+1] - v[k]`, `d = p - v[k]` (XZ plane). The point is
  inside unless some `cross_k > 0` and another `< 0` (zero counts as either). Height: `FUN_140177460`: the plane through `v0, v1, v2`
  (`n = (v1 - v0) x (v2 - v0)`) gives `y = v0.y - ((p.z - v0.z) * n.z + (p.x - v0.x) * n.x) / n.y`.
- **`strict` node_at**: over all nodes that contain `p`, take the one whose plane height is closest to `p.y`; accept it only when
  `|plane_y - p.y| < 20.0` (`0x1402f7fc4`); else -1.
- **`loose` node_at**: as strict; when there is no accepted node, the **nearest unblocked node by the distance from `p` to its four
  boundary segments** (`FUN_140193bc0`, 3-D point-to-segment), whatever the height.
- **LOS / `ground_ok(a, b)` (f24, `FUN_140179f80`)**: `na = strict(a)`, `nb = strict(b)`; either -1 or blocked -> 0; same node -> 1;
  otherwise up to 16 hops from `na`: scan the edges and take the first edge `i` that is **not** skipped, where an edge is skipped when any of:
  it is one of the last three edges used; `(flags & 2 || gate != 0) && ((gate != 0 && !(node[gate].flags & 4)) || (flags & 6) == 2)`;
  `flags != 0`; neither endpoint is the current node; or its portal `p0..p1` is not crossed by the XZ segment `a..b` (`FUN_140196420`). The
  next node is the edge's other endpoint; it must exist, be unblocked and not a jump zone (`flags & 2`), else the result is 0; reaching `nb`
  -> 1; no unskipped edge -> 0; 16 hops used up -> 1.
- **`nodedist(a, b)` (f37)**: distance between the centroids of the two nodes' four corners (mean of the four vertices), times 0.1.
- **`nav_step(n)` (f20)**: if node `n` is not blocked: `COM+0x50 = COM+0x5c = centroid(n)` (only when `COM+0x10 == 0`), returns 1; else 0.
- **`MOVE_TO(v)` (f19)**: `COM+0x50 = COM+0x5c = V[v]` when `COM+0x10 == 0`.
- **`path51(avoid, goal)` (f51, `FUN_14017a1a0`)** (READ at decompile level): `ni = loose(V[avoid])`; `nme = loose(own position)` (stored in
  `COM+0x20`); if either is -1 -> 0. Goal node `ng = loose(V[goal])`; when that gives -1 the goal is the **hub class** of `ni` and the goal point
  that node's centroid. If `ni == nme` (the avoided point is in my own node) it first picks, among the nodes joined to `ni` by an edge (one-way
  respected), the one whose portal lies most opposite the avoided point as seen from me (minimum of the dot product of the two unit vectors) and
  routes to it; otherwise it looks up the stored route `nme -> ng` (`FUN_140179420`) with **`ni` treated as blocked** (tables T0, T1, T3, then T4
  when `COM+0x48 != 0`); failure -> ring cleared, returns 0; success writes the ring (5.6) and `COM+0x50 = COM+0x5c = goal point` (when
  `COM+0x10 == 0`), returns 1.
- **`path_step()` (f60, `FUN_14020e380`)**: the goal node `g = loose(COM+0x5c)`; `g == -1` or blocked -> **0**. If my node `COM+0x20 == g`:
  `COM+0x50 = COM+0x5c`, return **2** when the XZ distance to the goal point is < 7.0 (`0x1402f7f40`) else **1**. Else if the LOS flag `COM+0x3c == 0`:
  ring empty (no last node) -> 0; the ring's last node is not my current node -> (when `COM+0x10 == 0` steer := goal point) **1**; (a further
  branch, "last node == `g` and squared distance < 30.0 -> 2", is unreachable because it needs `COM+0x20 == last` and `last == g`, which the first test
  already answered). LOS flag set: steer := goal point, **1**. So the script sees 2 ("arrived") only inside the goal node and within 7 u of the goal.
- **`probe(deg)` (f53, `FUN_14020e020`)**: the dodge record's `s43 == 3 && s44 == 8` (else returns 1); `D = (s16)s45 * 10.0`;
  `target = pos + (sin(yaw), 0, cos(yaw)) * D` with `yaw = deg * 32768 / 360` (**absolute heading**, not relative to my facing);
  returns `ground_ok(pos, target)`.

### 5.5 Jump links (READ: `FUN_1401776c0` tail)

A node's 8-word list at `+0x04` holds jump-link ids: a value `k` in `0..30` means "a jump to jump point k can start from here" and
a value `0x1f + k` means "jump point k lands here" (the jump point's position coincides with the landing node's centroid:
st05 points 2-7 lie within 1.1-3.3 u of their landing nodes). Entries are padded with -1. When the **next route node** (`COM+0x24`) has an
arrival entry `x + 0x1f` and the current node (`COM+0x20`) has the departure entry `x`, the CPU sets `COM+0x10 = 1` and steers to
jump point `x` (`cad + 0xb3c4 + 16 * x`). Jump-zone nodes (flag 2) are entered by walking to their centroid when the body is not in state 5.
`COM+0x10` is the **steer lock**: while 1, `MOVE_TO` / `nav_step` / `path51` do not change the steer target; the tick sets it to 1 when the
body is airborne (`P+0x12e74 == 0`) or in state 4, back to 0 when the state is 0, 1 or 6 (`FUN_1401775b0`).

### 5.6 The route ring and planner (READ, `COM+0x68`, `+0x6c`)

`COM+0x68` = ring head (i32, -1 = no route), `COM+0x6c` = 128 bytes of node ids, **circular** (index `(head + i) & 0x7f`), the route ends
with `0xff`. `FUN_1401791f0(COM, n)` = the node after `n` in the ring (the first node when `n` is not found), `FUN_1401792b0(COM, a, b)`
checks / splices the ring. `FUN_140178d00(COM, mode)` fills the ring from my node (`COM+0x20`) to the goal node (`COM+0x34`): mode 0
follows the stored route (T2 if `COM+0x48 == 2` and its entry is not -1, else T0) node by node, but at every node first looks for a **passable
direct edge** to a neighbour `w` that is closer to the goal by stored route length than the cur node (`0 < len(w -> goal) < len(cur -> goal)`;
`FUN_1401790e0` = sum of the stored edge lengths along the stored route, -1.0 when none; edge acceptance uses flags, the gate node and the dynamic
closed bit as in `FUN_140179e30`) and, when it finds one, continues with the stored route of `w`; at the end `FUN_1401796b0` splices around **blocked**
nodes (tries T0, T1, T3, then T4 between the last good node and the next good one). If mode 0 leaves the ring empty the planner runs again with mode 1,
which copies the T4 route (the jump-capable one). READ at decompile level; the intent above is my summary of ~200 lines. UNCONFIRMED: tie-breaks, and
whether the shortcut search ever fires in an arena with no closed doors.

### 5.7 Steering target selection per tick (READ at decompile level: `FUN_1401775b0`, `FUN_1401776c0`)

State in `COM` (the nav controller, embedded in `P` at `P+0x142d0`; script registers are in the AI struct at `COM+0x138`):

| field | meaning |
|---|---|
| `+0x08` | pointer to `P` |
| `+0x10` | steer lock (5.5) |
| `+0x18` | node of the steer target (loose) |
| `+0x1c` / `+0x20` | candidate / current node of my body (`+0x20` is "sticky": only replaced by a valid node) |
| `+0x24` | next node on the ring after `+0x20` |
| `+0x30` | edge index between `+0x20` and `+0x24` |
| `+0x34` | goal node |
| `+0x38` | stick magnitude to output (1.0 every tick) |
| `+0x3c` | result of `ground_ok(P.pos, COM+0x50)`, computed every tick **after** the script ran, from the steer target the script left; `FUN_1401776c0` (5.7 step 3) uses the fresh value, `path_step()` (called inside the script) sees the previous tick's |
| `+0x40` / `+0x44` | dodge request flag and yaw units (`path49`) |
| `+0x48` | planner style (0 or 2; `style0()/style2()`; also 5.2 T2 / T4 use) |
| `+0x50` | **steer target** (x, y, z); `+0x5c` goal point |
| `+0x68` / `+0x6c` | route ring (5.6) |
| `+0xec` | last position on the graph, `+0xf8` ticks off the graph, `+0xfc` recovery node scan index, `+0x100` recovery active flag, `+0x104` recovery target |
| `+0x110` | "hopeless" flag (`f234`), set when the recovery scan is exhausted |
| `+0x114` / `+0x120` | stuck reference point / stuck counter (`stuck()`) |
| `+0x124..+0x12c` | last three distinct nodes, `+0x130` ticks since the node changed, `+0x134` debug code of the last steering branch (0..6) |
| `+0x138` | the AI struct (`ai`): `+0xc` target, `+0x10` back pointer to `COM`, VM at `ai+0x20`, registers `ai+0x2074` |

Per tick (`FUN_1401775b0`, called from `FUN_14024ec80` only when the nav file is loaded and `P+0x30 == 0`): clear the pad words
`P+0x1424c..0x1427b`, set `P+0x1427c/80/84 = 1, 2, 3`, `COM+0x38 = 1.0`; run the script (`FUN_14017b220`); `COM+0x3c = ground_ok(P.pos,
COM+0x50)`; **if `COM+0x10 == 0`** run `FUN_1401776c0(COM, COM+0x50)` and `FUN_140177f60` (hazards, 5.8); update `COM+0x10` from the body
state; `FUN_140178ad0` (pad output, 5.9). `FUN_1401776c0` in order:

1. `n_me = strict(P.pos)`. If found: remember `COM+0xec = pos`, clear `+0xf8`, `+0x100`, `+0x110`, `+0x104 = pos`.
2. When `P+0x12f04 < 2` (standing or running): stuck test: `|pos - COM+0x114|^2 <= 50` (`0x1402f8014`) -> `stuck++` else `COM+0x114 = pos`, `stuck = 0`;
   `COM+0x130++`; push `n_me` into the last-three list when new (`COM+0x130 = 0`).
3. If `COM+0x3c != 0` (direct line to the steer target): only update the sticky node (`COM+0x20 = n_me` when valid) and keep the steer target.
4. Otherwise, if the 3-D distance to the steer target is >= 5.0 (`0x1402f7f18`): when off the graph (`n_me == -1`) and standing/running,
   `COM+0xf8++` and after 30 ticks run the **recovery** (5.8) and return. Goal node `COM+0x18 = loose(steer)`, `COM+0x34 = it` when valid.
   My node: prefer last tick's `COM+0x24` (still inside, height within 15.0 = `0x1402f7fb0` of me), else the cached `COM+0x1c`, else `loose(pos)`.
   If the node is a jump-zone: walk to its centroid unless the body is in state 5, else apply 5.5. If a goal exists and the ring is empty or
   `FUN_1401792b0` fails, plan (`FUN_140178d00(COM, 0)`). Same node as the goal -> steer = the script's target point. No ring -> steer = my
   own position (stand), code 1. Else take the next node and its edge: if the next node is the goal and an edge exists, aim at the script's
   target point; otherwise aim at the centroid of the next node. When an edge exists test whether the straight line to the aim passes
   through the portal: with `A = me - aim`, `B = p0 - aim`, `C = p1 - aim` (XZ cross products) it passes when `cross(C,B)*cross(A,B) >= 0` and
   `cross(A,C)*cross(A,B) <= 0` (I am inside the angle the portal subtends at the aim): yes -> steer = aim (code 4); no -> steer = the portal midpoint
   `(p0 + p1) * 0.5` **plus `unit_xz(midpoint - me) * 5.0`** (`0x1402f7f18`, asm `0x140177ef6`; the y is not changed): run through the portal (code 5); no
   edge -> steer = aim (code 6).
5. Distance < 5.0: keep the node bookkeeping only.

UNCONFIRMED: the exact tie-breaks of the ring splice and the greedy edge search (5.6). A live trace of `COM+0x50` (the steer target) and
`COM+0x134` per tick while a CPU walks a scripted route validates a port (the log is specified in P0-AI section 6).

### 5.8 Local hazards (`FUN_140177f60`) and off-graph recovery (`FUN_140178470`)

- **Hazards** (READ; every tick while `COM+0x10 == 0`, after the route code): from the stage-object list (`DAT_1405322c8`, up to 32 hits)
  take the objects whose flag word `+0x50` has bit `0x2000000` or whose vtable `+0x98(4)` is true (bit 4 of the object's flag word `+0x54` in the base class, as for f135) and whose squared distance to me is
  < 2500 (50 u, `0x1402f823c`). For each object, `vtable +0x1f0(i, out)` (i = 0, 1, ... until it returns 0) yields a **sphere** `out = (x, y, z, r)`.
  With `m = P+0x12ea4 * 0.5` and `yp = (P.y + steer.y) * 0.5`, a sphere matters when `y - r < yp < y + r`, the XZ distance from the sphere centre to
  the segment `P.pos .. steer` (`FUN_140193df0`) is `< r + m`, and `dot_xz(steer - me, centre - me) > 0` (it is ahead). Then the steer target
  becomes `(centre.x, P.y, centre.z) + perp * (r + m)` with `perp = unit(c.z, 0, -c.x) * sign`, `c = centre - me` (XZ) and
  `sign = cross_y(steer - me, c)` (so I pass on the side the original path was on). Which objects answer `+0x98(4)` / `+0x1f0` is
  UNCONFIRMED (live trace: log the object ids, kinds and spheres within 50 u in each arena). A first port may skip hazards and rely on the graph;
  this is a deviation to list.
- **Recovery** (READ; reached from step 4 of 5.7 after more than 30 ticks off the graph, `COM+0xf8` counts them): (a) `COM+0x110 != 0` (hopeless):
  steer = own position. While `COM+0xf8 < 90`: (b) if `COM+0x100 != 0` the recovery target `COM+0x104` is kept; else take the last on-graph
  position `COM+0xec` raised 7.5 u and my own position raised 7.5 u: if a stage ray between them is clear, target = `COM+0xec + unit_xz(COM+0xec - me) * 6.0`
  (`0x1402f7f30`), set `COM+0x100 = 1`, `COM+0xf8 = 30`; (c) else the centroid of `loose(pos)` raised 7.5 u with the same clear-ray test and `+ unit * 6.0`.
  Failing (b)/(c), or from 90 ticks on: (d) scan one node per tick (`COM+0xfc`): for each of its four boundary segments find the closest point to me
  (`FUN_140193fd0`); if it is within 100 u (`0x1402f8060`) and a ray from my raised position to it (raised 7.5 u) is clear, target = that point
  `+ unit * 6.0`, `COM+0x100 = 1`, `COM+0xf8 = 30`; after the scan passes the last node `COM+0x110 = 1`. The script also calls `f235` (teleport to the start
  point, `FUN_1402501d0`) once `stuck() > 900` or `f234` is set.

### 5.9 From steer target to the pad block (READ: `FUN_140178ad0`) and what the movement state does with it

`FUN_140178ad0(COM)`, `P = *(COM+8)`, `s = COM+0x50`:

- **Airborne** (`P+0x12e74 == 0`): `vy = P+0x12e84`, gravity `g = 0.12` per tick^2 (`0x1402f7ce4`), `t_up = vy / g`, `apex = vy * t_up * 0.5 + P.y`.
  If `s.y < apex`: `t_air = t_up + sqrt(2 * (apex - s.y) / g)` and `t_run = dist2d(P, s) / 1.85185` (`0x1402f7e8c`, the run speed in u/tick); when
  `t_air <= t_run` (the flight ends before I could cover the horizontal distance) the grounded branch below runs (keep pushing toward the target);
  otherwise, and whenever `s.y >= apex`, the stick is released (`P+0x14268 = 0`): no air control that would overshoot.
- **Grounded branch** (also taken when the airborne estimate says reach): if `COM+0x40 == 0` (no dodge): `dist2d(s, P) > 5.0` -> **`P+0x14268 = COM+0x38 (1.0)`
  and `P+0x14270 = (int)(atan2(s.x - P.x, s.z - P.z) * 5215.189)`** (yaw units, 32768 per turn, 0 = +z, absolute because the camera yaw is 0);
  else the magnitude stays 0 (cleared at the start of the tick). With a dodge request (`COM+0x40 != 0`) and the state in {0, 1, 8}
  (`(state & ~9) == 0 && state != 9`): set the dodge button bit `0x1000000` in `P+0x1425c` (held), clear it in `P+0x14260` (previous), set the
  facing `P+0x12e68` and the stick angle `P+0x14270` to `COM+0x44`, magnitude 1.0. Otherwise the request is dropped (`COM+0x40` is cleared
  by `FUN_14024ec80` at the start of every tick: P0-AI 5 item 8 closed).
- **The movement state machine** (`FUN_140256e80`, per tick, state in `state[1]`): `state[8] = P+0x14268` (when `DAT_1404bbd80[p*9] == 0`,
  i.e. input not locked) is the stick magnitude as a float; gait `state[6]`: walk -> run when magnitude `> 0.7` (`0x1402f7d60`), run -> walk
  when `<= 0.4` (`0x1402f7d30`), so **a CPU (magnitude exactly 1.0 or 0) always runs** and never walks. Facing target = `P+0x12e5c` (camera yaw, 0
  for CPUs) + `FUN_1402569b0(pad block)` = `P+0x14270` (+0x4000 when status 0x23); the body turns toward it at most `DAT_1404c0240[char * 0x28]` yaw
  units per tick (the character's turn rate, same as a human). Run speed `1.85185 u/tick` x status speed scale (walk `0.6944`, never used by a CPU).
- **Lock-on / strafe is never used by a CPU.** The only readers of the locked flag `P+0x13f24` are in the standing-state function `FUN_14025b5e0`
  (states 0, 0x15, 0x16, 0x1d): the strafing / turn-in-place / backpedal choices (`TURN_*` clips, `DAT_1403faed4 / ed0 / edc` tables) are taken only when
  `P+0x13f24 != 0 && P+0x30 != 0` (a locked **human**); with `P+0x30 == 0` it takes the free branch (run clip `DAT_1403faed4`, state 1) whenever the magnitude is
  non-zero. The run state `FUN_14025b270` (state 8) simply turns toward the stick angle for everyone. `P+0x13f24` is written only for humans (the CPU tick writes
  `P+0x13f1c / 0x13f20`). The D4 discrepancy (P0-AI section 4) is therefore **CONFIRMED** on the consumer side.
- **Aim of the casts (D5, CONFIRMED)**: `FUN_140258180` (cast start) reads only `P+0x13f1c` (band) and `P+0x13f20` (target index), never
  `P+0x13f24`: it copies the target object into `P+0x70` (homing target) when `band != 0` and (the skill is not motion 3/12/13/14, or `band == 1`).
  For an attack skill whose motion is not 8 / 0x10 / (4 with `s64 == 2`) **and a target exists**, the cast yaw/pitch are computed toward the target
  (`atan2(target - me)`, pitch from the aim height `P+0x12e98 + P+0x12e50` to the target's `+0x64`), for CPUs exactly as for a locked human;
  without a target (or for those motions, or `P+0x30 == 0` and a non-attack skill) the cast uses the body's facing `P+0x12e68` (a human with
  no target uses the camera yaw `P+0x12e5c` and pitch `P+0x12e64` instead; CPUs never do). So a CPU shot homes / aims by the lock block alone.

## 6. `.epa` (difficulty rows) and the AI info struct

`Data/com/param/enemyparam.epa` (READ: base `DAT_1404c0188`; users `FUN_14017b020`, `FUN_14017b100`, `FUN_14024e790`). 1,004 bytes:
`u32 count = 82`, then **100 rows of 10 bytes** (rows 82-99 are zero; the program never bounds-checks the row index, so a row >= 82 behaves
as all zeros: never attacks, think interval 0). Row address = `base + 4 + 10 * row`. All bytes are read as **signed chars**.

| byte | script register | meaning |
|---|---|---|
| 0 | none | starting Health (20 in every used row; applied only when the seat is built by `FUN_14024e790`: `Health = (s8)byte0`) |
| 1 | v30 | target rule: 0 lowest slot index, 1 highest slot index, 2 lowest Health, 3 highest Health, 4 nearest, 5 farthest, 6 random (P0-AI 2.5) |
| 2 | v31 | think interval in ticks (30 in every used row) |
| 3 | v32 | attack probability % per think |
| 4 | v33 | react-to-bullet % |
| 5 | v34 | react-to-melee % |
| 6 | v35 | dodge % |
| 7 | v36 | wander % |
| 8 | v37 | idle style: 0 avoid, 1 roam, 2 camp at the target's start point |
| 9 | v38 | capsule-hunt strategy 0..3 |

(register `v(30 + i)` at `ai + 0x20ec + 4*i`, copied by `FUN_14017b020/b100`; names of v31..v38 as in P0-AI 2.5.) Families as in P0-AI 2.5; the rows
with the lowest effort are 70 (never attacks), 74 (5 % everything, wanders 80 %) and 59 (5 % attacks).

Row families, exactly (`e0` rule, `e7` idle, `e8` capsule strategy; `e2..e5` the four percentages):

| rows | e0 | e7 | e8 | ladder (e2 / e3 / e4 / e5, e6) |
|---|---|---|---|---|
| 0-4, 5-9, 10-14, 15-19 | 0, 1, 2, 3 per block of 5 | 0 | 0 | 88 / 77 / 66 / 55 / 44 for all four, e6 = 3, 6, 9, 12, 15 |
| 20-24, 25-29, 30-34, 35-39 | 0, 1, 2, 6 | 0 | 2 | as rows 0-19 |
| 40-44 / 45-49 / 50-54 | 0 / 1 / 2 | 2 | 1 | e2 = 88, 77, 77, 77, 66; e3 = e4 = e5 = 55, 44, 33, 22, 0; e6 = 3, 6, 9, 12, 15 |
| 55-59 | 6 | 0 | 3 | e2 = 44, 33, 22, 11, 5; e3-e5 = 88, 77, 77, 66, 66; e6 = 3, 6, 9, 12, 15 |
| 60-64 | 4 | 2 | 0 | e2 = 88, 77, 77, 77, 66; e3 = e5 = 55, 44, 33, 22, 11; e4 = e2; e6 = 3..15 |
| 65-69 | 5 | 1 | 2 | e2 = 88, 88, 77, 77, 77; e3 = e2 (88, 88, 77, 77, 77); e4 = 55, 33, 33, 22, 11; e5 = e2; e6 = 3, 6, 9, 12, 15 |
| 70-74 | 0 | 1 | 2 | e2 = 0, 33, 22, 11, 5; e3-e5 = 88 / 22 / 11 / 11 / 5; e6 = 11, 18, 21, 80, 80 |
| 75-77 | 1 | 1 | 2 | e2 = 44, 33, 5; e3-e5 = 33, 22, 0; e6 = 15, 18, 27 |
| 78-80 | 3 | 1 | 2 | e2 = 33, 22, 11; e3-e5 = 22, 11, 5; e6 = 18, 21, 24 |
| 81 | 5 | 1 | 2 | 44 / 55 / 0 / 55, e6 = 27 |

**AI info struct** (the seat's `+0x20` pointer; versus seats have none, the debug launcher `FUN_140214770` builds one at `0x1405b18b0`): `+0x04` kind
(0 = a normal fighter running a script; 1-5 = story boss classes), `+0x18` epa row (i32), `+0x31` deck name (NUL-terminated; empty = no deck is
loaded), `+0x46` script name (`eneprog006.ssb`). The quest record variant (`FUN_14017b100`) reads the row from the i16 at `+0x34`, the script name from
`+0x1a` (`"%s.ssb"`) and the deck from `+4`.

**Seat setup struct** (0x48 bytes per seat, 4 seats, copied to `DAT_1404bc420` by `FUN_140128990`; the versus launcher `FUN_1401d0750` fills it; the
seat loop `FUN_140128cc0` builds `pdNormalPlayerClass` via vtable slot 0 = `FUN_14024e790`): `+0x00` seat active, `+0x04` team (versus writes the seat
index), `+0x08` character, `+0x0c` (-> `P+0x14244`), `+0x10` **human flag** (versus writes 1 for every seat), `+0x14` controls enabled (1), `+0x18` start
point index, `+0x20` AI info pointer (versus writes NULL). **A CPU seat is the same constructor with `+0x10 = 0` and `+0x20` = an AI info struct**:
`FUN_14024e790` then sets `P+0x30 = 0`, takes the Health from the epa row, calls `FUN_140176c80(P+0x142d0, P, aiinfo)` (initialises `COM`,
`FUN_14017b020` loads the script and deck) and the per-tick path in P0-AI 2.2 runs. The shipped versus screens never produce such a seat
(P0-AI 5 item 6 closed: **no shipped path builds a CPU seat for versus**; the engine supports it through the seat struct alone).

## 7. Open items after this lane

Everything the build lanes need to start is above. What stays UNCONFIRMED (each with what a live trace needs):

1. (closed by lane `ssbvm`: the budget accounting is read from the assembly and implemented in `ssb_vm.rs`.)
2. **Route table semantics T1/T3** (5.2) and the **greedy planner / ring splice tie-breaks** (5.6): need a trace of `COM+0x68..0xec` for a scripted walk.
3. **Hazard objects** (`FUN_140177f60`, `FUN_14012e160`, 5.8 / 4.3): need the object ids/kinds/spheres near a fighter in each arena; and `NF_*` flags 4 / 8 after load (5.3).
4. **`f123`, `f125` consumers, status 12 writer** (4.3, 4.5): need the values in a live versus match (`f123` for each player each tick).
5. **Predictor inputs** (4.4): which engine fields feed `O+0x1c0..0x1d4` per motion; `P+0x44` per seat (read by inference).
6. **Natives `f123`'s `arr_a / arr_b` pair, `f198`'s `D`** (4.2 / 4.3): value traces.
7. Whether the CPU needs the stage-object avoidance and `COM+0x48` planner styles at all in a first port (deviations to list in the build lane's report).


## 8. Host notes (lane `cpuhost`, `pd_game/src/cpu/`)

Built from the sections above; what the host adds or could not read:
- Tick: every 2nd sim tick (30 Hz) after the countdown; stick held 2 sim ticks, presses / dodge 1. `Input` carries the stick as a camera-frame vector through the program's dead zone inverse, so the sim reads the absolute heading at magnitude 1 (always Run). No lock input; `Player::cpu_aim` (new, additive) = lock block set / lock flag clear (D5) so casts aim and melee dashes at `Player::target` while the body never strafes.
- `rand`: private xorshift seeded from `Combat::rng_state()`; the sim's stream is untouched.
- Cross-check: `engine/re/cpu_xcheck.py` replays the Rust call log (83,689 calls, 810 ticks, row 20 on st03) through `ssb_run.py`: identical call sequence, pure natives recomputed (test `native_call_log_matches_the_python_reference_vm`).
- `.cad`: parsed with z -> -z (our space); 74 of 76 st03 node centroids land on stage floor mirrored, 55 unmirrored. The package has 16 `comdata*.cad` files in st02..st08 (not 12; variants 0/1 identical). `pd_convert cpu <Data dir> <cache>` copies them, `eneprog006.ssb` and `enemyparam.epa` into `cache/com/`; the game also reads `game_files/.../com` beside the cache.
- f123 = skills undealt + capsules out (the sum `Combat::requirement_met` case 5 reads). Ring = `[my node, route...]` (FUN_1401791f0 / 1401792b0 read). The planner's door-edge shortcut is implemented as read (only open door / gated edges are candidates).
- UNCONFIRMED, implemented as read-only or as an explicit constant: the shared capsule list f197-f200 (empty: nothing in versus fills it), slot states 2 / 4, status 12 / 17, f53 probe (records 750/751 are not in skills.json: returns 1), hazards `FUN_140177f60` and the node-vs-object block (not run), predictor inputs (shell, fall, crawl, lob via the free-flight branch; arcs, ricochets, areas, mines and blasts predict 0), f37 rounding, off-graph recovery constants, jump links (steer only: no jump press exists in the program's pad code), per-round script reset.
