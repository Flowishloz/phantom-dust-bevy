//! The program's script VM (`FUN_1402aaa30`), reimplemented deterministically.
//!
//! Every statement below is READ from the decompile / disassembly of `PDUWP.exe`
//! (image base 0x140000000). Nothing is measured. Where we deviate it is listed under
//! "Deviations".
//!
//! # Machine
//! - 64-bit integer stack machine. The VM struct (`ai + 0x20` for CPU fighters, set up by
//!   `FUN_14017b020`): flags `+0x00` (1 fault, 2 idle / finished, 4 waiting), pc `+0x08`
//!   (null = idle), data-stack pointer `+0x10`, call-stack pointer `+0x18`, frame `+0x20`,
//!   result register `+0x28`, current block `+0x30`, stack size `+0x38` (0x400 qwords),
//!   stack base `+0x40`, wait counter `+0x48`. One region holds both stacks: the call stack
//!   grows up from the base, the data stack grows down from base + size.
//! - A call (`FUN_1402ab510`, op 3) pushes `[pc, frame, block]` on the call stack and sets
//!   `frame = sp`; `RET` pops them. Arguments are pushed right to left, so argument 0 is
//!   the top of the stack; the callee pops `args + locals` itself (`RET pop=N`). Locals live
//!   below the frame (`ALLOC n`, then `frame[-1]`, `frame[-2]`...), arguments at `frame[0..]`.
//! - The program re-enters the script every tick (`FUN_14017b220`): `FUN_1402ab5d0` finds
//!   `main` by name and calls it, then `FUN_1402aaa30(vm, 50000)` runs until the script
//!   returns (idle flag), waits, faults or the instruction budget runs out. There is no
//!   persistent program counter between ticks, only the script's registers (host side).
//! - Budget: it is decremented once per instruction at the loop head, but restored by the
//!   handlers of branches, `CASE`, push/pop/alloc/local access, comparisons, `PUSH_ADDR`
//!   and value-returning `RET` (`mov edi, esi` in the handlers, e.g. `0x1402aae63`). Only
//!   `NOP`, native calls, `CTRL`, calls, `RET` without value, arithmetic (`ADD SUB MUL DIV
//!   MOD`), bit ops and shifts consume it. Reproduced exactly ([`Vm::run`]).
//!
//! # Natives
//! `op 1`: group = word>>28, function = (word>>16)&0xFFF, `b1` = argument words dropped after
//! the call (`0x1402aac34`: `call [table[group] + fn*8]` with `rcx` = stack pointer; the result
//! `rax` goes to the result register read by `PUSHRES`, `0x1402aac3a`). Group 0 is the VM
//! library: `f5` is `rand(n)` (the same function as group 4 `f32`, `FUN_14020dcf0`), `f16`
//! `tbl(addr, index)` (`0x1402abdf0`) and `f17` `tbl_store(addr, index, value)` (`0x1402abe30`)
//! are implemented here; every other group 0 function and all of group 4 (the battle API) go to
//! the host through [`Natives`]. **Randomness is the host's**: the program uses MSVC `rand()`
//! seeded from `GetTickCount64` (`FUN_140291f00`); the game should answer `rand` from its
//! seeded sim RNG. [`MsvcRand`] and [`script_rand`] reproduce the CRT stream and the
//! `(rand & 0x7FFF) * n >> 15` mapping when a CRT-identical stream is wanted.
//!
//! `tbl` returns a sign-extended 32-bit table entry: `FUN_1402aa620` keeps a *shadow* buffer
//! parallel to the loaded code whose dwords hold the high half (`entry >> 31`), and `tbl` returns
//! `shadow << 32 | entry` (high half from the shadow, low half zero-extended); `tbl_store`
//! writes both halves. We keep the same two arrays.
//!
//! # Deviations (defensive, cannot occur in a correct script)
//! - Stack over/underflow, a bad pc / data address, division by zero (an x86 trap in the
//!   program) and unknown opcodes set the fault flag. Unknown opcodes fault in the program too
//!   (`0x1402ab377`).
//! - `CALL_IMPORT` (op 4, `FUN_1402aa960`) is not linked: no `com/ai` script has an import
//!   record. It behaves like the program's "function not found" path: nothing is called and the
//!   result register is 0.
//! - Addresses are byte offsets into the script file (the program uses real pointers).

use crate::ssb::{op, Ssb};

pub const FLAG_FAULT: u32 = 1;
pub const FLAG_IDLE: u32 = 2;
pub const FLAG_WAIT: u32 = 4;
/// Stack size in qwords (`FUN_14017b020`: `vm+0x38 = 0x400`).
pub const STACK_WORDS: usize = 0x400;
/// Per-tick instruction budget (`FUN_14017b220`: `FUN_1402aaa30(vm, 50000)`).
pub const TICK_BUDGET: i32 = 50_000;

/// Read access to the loaded script image for natives (strings, tables).
pub struct Memory<'a> {
    pub image: &'a [u8],
}

impl Memory<'_> {
    pub fn read_i32(&self, addr: i64) -> Option<i32> {
        let a = usize::try_from(addr).ok()?;
        Some(i32::from_le_bytes(self.image.get(a..a + 4)?.try_into().ok()?))
    }
    /// NUL-terminated string at an address (e.g. a `PUSH_STR` argument).
    pub fn read_cstr(&self, addr: i64) -> Option<&str> {
        let a = usize::try_from(addr).ok()?;
        let tail = self.image.get(a..)?;
        let end = tail.iter().position(|&c| c == 0)?;
        std::str::from_utf8(&tail[..end]).ok()
    }
}

/// The native-call interface. `args[0]` is argument 0 (the top of the stack at the call);
/// `args.len()` is the instruction's `b1`. The return value becomes the VM result register.
pub trait Natives {
    fn call(&mut self, group: u8, func: u16, args: &[i64], mem: &Memory) -> i64;
}

/// A host that answers 0 to everything.
pub struct NullNatives;
impl Natives for NullNatives {
    fn call(&mut self, _g: u8, _f: u16, _a: &[i64], _m: &Memory) -> i64 {
        0
    }
}

/// Why [`Vm::run`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    /// `main` returned (idle flag): the normal end of a tick.
    Finished,
    /// A `WAIT` or a `JMP_Z_YIELD`: resumes on a later [`Vm::run`].
    Waiting,
    /// The fault flag is set (permanent).
    Faulted,
    /// The instruction budget ran out mid-script.
    OutOfBudget,
}

#[derive(Debug, Clone, Copy)]
struct Fault;

#[derive(Debug, Clone)]
pub struct Vm {
    pub flags: u32,
    /// `None` = null (idle).
    pc: Option<u32>,
    sp: usize,
    cs: usize,
    frame: usize,
    /// Result register (`vm+0x28`), read by `PUSHRES`.
    pub result: i64,
    pub wait: u32,
    stack: Vec<i64>,
    image: Vec<u8>,
    shadow: Vec<u32>,
    pool_off: u32,
    code_end: u32,
    functions: Vec<crate::ssb::FuncEntry>,
    /// Instructions executed (including budget-free ones), for tests and profiling.
    pub steps: u64,
}

impl Vm {
    /// A VM in the state `FUN_14017b020` leaves it: flags = idle, empty stacks. The script
    /// image is copied (the program mutates its loaded block through `tbl_store`).
    pub fn new(ssb: &Ssb) -> Vm {
        // FUN_1402aa620: shadow dword i = image dword i >> 31 (sign extension), for the
        // first `file[1] / 4` dwords of the file.
        let n = (ssb.table_off / 4) as usize;
        let shadow = (0..n).map(|i| ((i32::from_le_bytes(ssb.data[i * 4..i * 4 + 4].try_into().unwrap())) >> 31) as u32).collect();
        Vm {
            flags: FLAG_IDLE,
            pc: None,
            sp: STACK_WORDS,
            cs: 0,
            frame: 0,
            result: 0,
            wait: 0,
            stack: vec![0; STACK_WORDS],
            image: ssb.data.clone(),
            shadow,
            pool_off: ssb.pool_off,
            code_end: ssb.table_off,
            functions: ssb.functions.clone(),
            steps: 0,
        }
    }

    pub fn image(&self) -> &[u8] {
        &self.image
    }
    pub fn is_idle(&self) -> bool {
        self.flags & FLAG_IDLE != 0
    }
    pub fn pc(&self) -> Option<u32> {
        self.pc
    }
    /// Data stack as a slice from the top (`sp`) to the base.
    pub fn data_stack(&self) -> &[i64] {
        &self.stack[self.sp..]
    }

    /// `FUN_1402ab510`: call a function at `entry` (byte offset). Clears the idle flag, pushes
    /// `[pc, frame, block]` on the call stack, `frame = sp`, `pc = entry`, wait counter 0.
    pub fn call(&mut self, entry: u32) {
        self.flags &= !FLAG_IDLE;
        let saved = [self.pc.unwrap_or(0) as i64, self.frame as i64, 0];
        for v in saved {
            if self.cs >= self.sp || self.cs >= self.stack.len() {
                self.flags |= FLAG_FAULT;
                return;
            }
            self.stack[self.cs] = v;
            self.cs += 1;
        }
        self.frame = self.sp;
        self.pc = Some(entry);
        self.wait = 0;
    }

    /// `FUN_1402ab5d0`: call the exported function with this name. False if it does not exist.
    pub fn call_by_name(&mut self, name: &str) -> bool {
        let h = crate::ssb::name_hash(name);
        match self.functions.iter().find(|f| f.hash == h) {
            Some(f) => {
                let off = f.offset;
                self.call(off);
                true
            }
            None => false,
        }
    }

    /// One program tick for a CPU fighter: call `main`, run with the program's budget.
    pub fn tick(&mut self, natives: &mut dyn Natives) -> RunStatus {
        self.call_by_name("main");
        self.run(natives, TICK_BUDGET)
    }

    fn fault<T>(&mut self) -> Result<T, Fault> {
        Err(Fault)
    }

    fn push(&mut self, v: i64) -> Result<(), Fault> {
        if self.sp == 0 || self.sp <= self.cs {
            return Err(Fault);
        }
        self.sp -= 1;
        self.stack[self.sp] = v;
        Ok(())
    }
    fn pop(&mut self) -> Result<i64, Fault> {
        let v = *self.stack.get(self.sp).ok_or(Fault)?;
        self.sp += 1;
        Ok(v)
    }
    fn top(&self) -> Result<i64, Fault> {
        self.stack.get(self.sp).copied().ok_or(Fault)
    }
    /// Stack word `i` below the top (0 = top, 1 = second).
    fn nth(&self, i: usize) -> Result<i64, Fault> {
        self.stack.get(self.sp + i).copied().ok_or(Fault)
    }
    fn set_sp(&mut self, sp: i64) -> Result<(), Fault> {
        if sp < 0 || sp as usize > self.stack.len() {
            return Err(Fault);
        }
        self.sp = sp as usize;
        Ok(())
    }
    fn local(&self, s: i16) -> Result<usize, Fault> {
        let i = self.frame as i64 + s as i64;
        if i < 0 || i as usize >= self.stack.len() {
            return Err(Fault);
        }
        Ok(i as usize)
    }
    /// Replace the top two words by one binary result (`SP += 8; SP[0] = r`): `a` = old top,
    /// `b` = second.
    fn binary(&mut self) -> Result<(i64, i64), Fault> {
        let a = self.nth(0)?;
        let b = self.nth(1)?;
        self.sp += 1;
        Ok((a, b))
    }
    fn set_top(&mut self, v: i64) {
        self.stack[self.sp] = v;
    }

    fn tbl_read(&self, addr: i64, idx: i64) -> Result<i64, Fault> {
        // 0x1402abdf0: lo = *(u32*)(addr + 4*idx), hi = shadow dword at the same offset
        let a = addr.wrapping_add(idx.wrapping_mul(4));
        if a < 0 || a % 4 != 0 {
            return Err(Fault);
        }
        let a = a as usize;
        let lo = u32::from_le_bytes(self.image.get(a..a + 4).ok_or(Fault)?.try_into().unwrap());
        let hi = *self.shadow.get(a / 4).ok_or(Fault)?;
        Ok(((hi as u64) << 32 | lo as u64) as i64)
    }
    fn tbl_write(&mut self, addr: i64, idx: i64, v: i64) -> Result<(), Fault> {
        // 0x1402abe30: *(u32*)(addr + 4*idx) = low half, shadow dword = high half
        let a = addr.wrapping_add(idx.wrapping_mul(4));
        if a < 0 || a % 4 != 0 {
            return Err(Fault);
        }
        let a = a as usize;
        if a / 4 >= self.shadow.len() || a + 4 > self.image.len() {
            return Err(Fault);
        }
        self.image[a..a + 4].copy_from_slice(&(v as u32).to_le_bytes());
        self.shadow[a / 4] = (v >> 32) as u32;
        Ok(())
    }

    /// `FUN_1402aaa30(vm, budget)`. Runs until the script returns, waits, faults or the budget
    /// reaches 0. Returns the status derived from the flags.
    pub fn run(&mut self, natives: &mut dyn Natives, mut budget: i32) -> RunStatus {
        if self.wait != 0 {
            // 0x1402aaa3f: a pending WAIT just counts down and returns
            self.wait -= 1;
            return self.status();
        }
        self.flags &= !FLAG_WAIT;
        if budget == 0 {
            return self.status();
        }
        loop {
            // loop head 0x1402aaa90: edi = budget - 1, then the flag / pc checks
            let dec = budget.wrapping_sub(1);
            if self.flags & 7 != 0 {
                break;
            }
            let Some(pc) = self.pc else {
                self.flags |= FLAG_IDLE;
                break;
            };
            match self.step(pc, natives, &mut budget, dec) {
                Ok(StepEnd::Next(b)) => {
                    budget = b;
                    if budget == 0 {
                        break;
                    }
                }
                Ok(StepEnd::Rehead) => {}
                Err(Fault) => {
                    self.flags |= FLAG_FAULT;
                    break;
                }
            }
        }
        self.status()
    }

    fn status(&self) -> RunStatus {
        if self.flags & FLAG_FAULT != 0 {
            RunStatus::Faulted
        } else if self.flags & FLAG_IDLE != 0 {
            RunStatus::Finished
        } else if self.flags & FLAG_WAIT != 0 {
            RunStatus::Waiting
        } else {
            RunStatus::OutOfBudget
        }
    }

    fn step(&mut self, pc: u32, natives: &mut dyn Natives, budget: &mut i32, dec: i32) -> Result<StepEnd, Fault> {
        let off = pc as usize;
        if pc % 4 != 0 || pc < 0x20 || pc >= self.code_end {
            return self.fault();
        }
        let word = u32::from_le_bytes(self.image[off..off + 4].try_into().unwrap());
        let pc_after = pc + 4;
        self.pc = Some(pc_after);
        self.steps += 1;
        let b1 = (word >> 8) as u8;
        let s = (word >> 16) as i16;
        let rel = |p: u32| -> u32 { (p as i64 + s as i64 * 4) as u32 };
        // `keep` = the handler restored the budget (no consumption); otherwise `dec`.
        let keep = *budget;
        let mut next = dec;
        match word as u8 {
            op::NOP => {}
            op::NATIVE => {
                let group = (word >> 28) as u8;
                let func = ((word >> 16) & 0xFFF) as u16;
                let n = b1 as usize;
                let r = match (group, func) {
                    (0, 16) => {
                        let (a0, a1) = (self.nth(0)?, self.nth(1)?);
                        self.tbl_read(a0, a1)?
                    }
                    (0, 17) => {
                        let (a0, a1, a2) = (self.nth(0)?, self.nth(1)?, self.nth(2)?);
                        self.tbl_write(a0, a1, a2)?;
                        0
                    }
                    _ => {
                        let end = (self.sp + n).min(self.stack.len());
                        let args: Vec<i64> = self.stack[self.sp.min(end)..end].to_vec();
                        let mem = Memory { image: &self.image };
                        natives.call(group, func, &args, &mem)
                    }
                };
                self.result = r;
                if self.flags & 7 != 0 {
                    // 0x1402aac37: the VM returns at once when the call left a flag set
                    return Ok(StepEnd::Next(dec));
                }
                let sp = self.sp as i64 + n as i64;
                self.set_sp(sp)?;
            }
            op::CTRL => match b1 {
                0 => {
                    // WAIT s: flags |= wait, counter = s; the loop exits at the next flag check
                    self.flags |= FLAG_WAIT;
                    self.wait = s as i32 as u32;
                }
                1 => self.flags |= FLAG_FAULT,
                2 => {
                    // RESTART: pc = null, flags = idle, stacks reset
                    self.pc = None;
                    self.flags = FLAG_IDLE;
                    self.cs = 0;
                    self.sp = STACK_WORDS;
                }
                3 => {
                    // 0x1402aab13: edi = 0xB2D05E01, then through the loop head (one more decrement)
                    *budget = -0x4d2f_a1ff;
                    return Ok(StepEnd::Rehead);
                }
                _ => {}
            },
            op::CALL => {
                self.call_push(pc_after)?;
                self.pc = Some(rel(pc_after));
            }
            op::CALL_IMPORT => {
                // not linked (see module doc): the "not found" path
                self.result = 0;
            }
            op::RET => {
                // pops block, frame, pc; b1 != 0 pops the return value into the result register
                if self.cs < 3 {
                    return self.fault();
                }
                self.cs -= 3;
                let (p, f) = (self.stack[self.cs], self.stack[self.cs + 1]);
                self.frame = f as usize;
                self.pc = if p == 0 { None } else { Some(p as u32) };
                if b1 != 0 {
                    self.result = self.pop()?;
                    next = keep;
                }
                let sp = self.sp as i64 + s as i64;
                self.set_sp(sp)?;
            }
            op::ALLOC => {
                let sp = self.sp as i64 - s as i64;
                if sp < self.cs as i64 {
                    return self.fault();
                }
                self.set_sp(sp)?;
                next = keep;
            }
            op::BRANCH => {
                next = keep;
                match b1 {
                    1 => {
                        if self.pop()? != 0 {
                            self.pc = Some(rel(pc_after));
                        }
                    }
                    2 => {
                        if self.pop()? == 0 {
                            self.pc = Some(rel(pc_after));
                        }
                    }
                    3 => {
                        if self.pop()? == 0 {
                            self.flags |= FLAG_WAIT;
                            self.pc = Some(rel(pc_after));
                        }
                    }
                    _ => self.pc = Some(rel(pc_after)),
                }
            }
            op::CASE => {
                next = keep;
                if b1 as u64 == self.top()? as u64 {
                    self.sp += 1;
                } else {
                    self.pc = Some(rel(pc_after));
                }
            }
            op::PUSH_LOCAL => {
                let i = self.local(s)?;
                let v = self.stack[i];
                self.push(v)?;
                next = keep;
            }
            op::STORE_LOCAL => {
                next = keep;
                match b1 {
                    0 => {
                        let v = self.pop()?;
                        let i = self.local(s)?;
                        self.stack[i] = v;
                    }
                    1 => {
                        let v = self.pop()?;
                        let i = self.local(s)?;
                        self.stack[i] = self.stack[i].wrapping_add(v);
                    }
                    2 => {
                        let v = self.pop()?;
                        let i = self.local(s)?;
                        self.stack[i] = self.stack[i].wrapping_sub(v);
                    }
                    _ => {}
                }
            }
            op::POP => {
                let sp = self.sp as i64 + s as i64;
                self.set_sp(sp)?;
                next = keep;
            }
            op::PUSH_I16 => {
                self.push(s as i64)?;
                next = keep;
            }
            op::PUSH_I32 => {
                let imm = u32::from_le_bytes(self.image.get(pc_after as usize..pc_after as usize + 4).ok_or(Fault)?.try_into().unwrap());
                self.pc = Some(pc_after + 4);
                self.push(imm as i32 as i64)?;
                next = keep;
            }
            op::PUSH_RESULT => {
                self.push(self.result)?;
                next = keep;
            }
            op::PUSH_STR => {
                // block + 0xC + s + block[3] with the runtime-relative header = pool offset + s
                self.push(self.pool_off as i64 + s as i64)?;
                next = keep;
            }
            op::ADD => {
                let (a, b) = self.binary()?;
                self.set_top(b.wrapping_add(a));
            }
            op::SUB => {
                let (a, b) = self.binary()?;
                self.set_top(if b1 == 0 { b.wrapping_sub(a) } else { a.wrapping_sub(b) });
            }
            op::MUL => {
                let (a, b) = self.binary()?;
                self.set_top(a.wrapping_mul(b));
            }
            op::DIV => {
                let (a, b) = self.binary()?;
                let (l, r) = if b1 == 0 { (b, a) } else { (a, b) };
                self.set_top(l.checked_div(r).ok_or(Fault)?);
            }
            op::MOD => {
                let (a, b) = self.binary()?;
                let (l, r) = if b1 == 0 { (b, a) } else { (a, b) };
                self.set_top(l.checked_rem(r).ok_or(Fault)?);
            }
            op::CMP => {
                // "left OP right" with left = second word when b1 == 0, the top word otherwise;
                // s: 1 NE, 2 GT, 3 LT, 4 GE, 5 LE, 6 left has every bit of right, else EQ
                let (a, b) = self.binary()?;
                let (l, r) = if b1 == 0 { (b, a) } else { (a, b) };
                let v = match s {
                    1 => l != r,
                    2 => l > r,
                    3 => l < r,
                    4 => l >= r,
                    5 => l <= r,
                    6 => l & r == r,
                    _ => l == r,
                };
                self.set_top(v as i64);
                next = keep;
            }
            op::AND => {
                let (a, b) = self.binary()?;
                self.set_top(a & b);
            }
            op::OR => {
                let (a, b) = self.binary()?;
                self.set_top(a | b);
            }
            op::XOR => {
                let (a, b) = self.binary()?;
                self.set_top(a ^ b);
            }
            op::NOT => {
                let v = self.top()?;
                self.set_top(!v);
            }
            op::NEG => {
                let v = self.top()?;
                self.set_top(v.wrapping_neg());
            }
            op::LNOT => {
                let v = self.top()?;
                self.set_top((v == 0) as i64);
            }
            op::SHIFT => {
                // b1 == 0: value = second word, count = top; else value = top, count = second.
                // s: 1 arithmetic right, 2 logical right, else left. Count masked to 6 bits.
                let (a, b) = self.binary()?;
                let (v, c) = if b1 == 0 { (b, a) } else { (a, b) };
                let c = (c & 0x3F) as u32;
                self.set_top(match s {
                    1 => v >> c,
                    2 => ((v as u64) >> c) as i64,
                    _ => v << c,
                });
            }
            op::PUSH_ADDR => {
                let a = pc_after as i64 + ((word as i32) >> 8) as i64 * 4;
                self.push(a)?;
                next = keep;
            }
            _ => return self.fault(),
        }
        Ok(StepEnd::Next(next))
    }

    /// Push `[pc, frame, block]` for a call and set `frame = sp` (`op 3`).
    fn call_push(&mut self, pc_after: u32) -> Result<(), Fault> {
        for v in [pc_after as i64, self.frame as i64, 0] {
            if self.cs >= self.sp {
                return Err(Fault);
            }
            self.stack[self.cs] = v;
            self.cs += 1;
        }
        self.frame = self.sp;
        Ok(())
    }
}

enum StepEnd {
    /// Continue with this remaining budget (0 ends the run).
    Next(i32),
    /// Jump to the loop head with `budget` already set (op 2 b1=3).
    Rehead,
}

/// MSVC CRT `rand()` / `srand()` (the stream the program's `FUN_14020dcf0` draws from):
/// `seed = seed * 214013 + 2531011; return (seed >> 16) & 0x7FFF`.
#[derive(Debug, Clone)]
pub struct MsvcRand(pub u32);

impl MsvcRand {
    pub fn new(seed: u32) -> MsvcRand {
        MsvcRand(seed)
    }
    pub fn next15(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.0 >> 16) & 0x7FFF
    }
    /// `rand(n)` as the scripts call it.
    pub fn below(&mut self, n: i64) -> i64 {
        script_rand(self.next15(), n)
    }
}

/// `FUN_14020dcf0`: `(rand() & 0x7FFF) * n >> 15` computed in 32 bits (`imul eax, ebx; shr rax, 15`).
pub fn script_rand(raw: u32, n: i64) -> i64 {
    ((raw & 0x7FFF).wrapping_mul(n as u32) >> 15) as i64
}

#[cfg(test)]
#[path = "ssb_vm_tests.rs"]
mod tests;
