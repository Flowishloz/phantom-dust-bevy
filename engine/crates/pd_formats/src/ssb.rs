//! `.ssb` script files (the program's bytecode: AI scripts `Data/com/ai/*.ssb`, menus, events).
//!
//! Evidence (all READ from `PDUWP.exe`, image base 0x140000000; nothing here is measured):
//! - VM and instruction format: `FUN_1402aaa30` (see `ssb_vm.rs`).
//! - File layout. The **file** form has absolute byte offsets in the header:
//!   `[0]` code start (always 0x20), `[1]` function-table offset, `[2]` function-table end
//!   (= start of the 16-byte import records), `[3]` string-pool offset; words 4..7 are a
//!   link slot / name block the VM never reads. The loader `FUN_1402aa620` turns this into the
//!   runtime form (`[1]-=4, [2]-=8, [3]-=0xC`, function entries become self-relative) and
//!   `FUN_1402aa7e0` turns it back (`+= 4/8/0xC`, `entry[2] = (field offset + rel) >> 2`), which is
//!   how the file form was pinned down; the shipped files are in the file form
//!   (checked on all 16 `com/ai` scripts: `main` entry of `eneprog006` = word 0x1AE3 = byte 0x6B8C).
//! - Function table: 12-byte entries `(name hash 0, name hash 1, entry word offset)`, searched
//!   linearly by `FUN_1402ab5d0`. The name is packed base 40, 6 characters per hash word, most
//!   significant first, 0 = end of name, table `0x1402f76a0` (digits 1..10, letters 11..36 with
//!   case folded, `_` and `=` 37); hashing is `FUN_1402847c0`. `main` = `(0x8E225400, 0)`.
//!   Only `main` is exported; every other function is a local call target (op 3).
//! - Import records (op 4, `FUN_1402aa960`): 16 bytes each at `[2]..[3]`: UNCONFIRMED layout,
//!   `(hash0, hash1, block slot, function slot)` from the resolver. No `com/ai` script has any.
//! - Instruction words: low byte = opcode, `b1` = bits 8..15, `s` = signed 16 bits 16..31;
//!   `0x0F` carries a second word. See [`op`].

use crate::bytes::{slice, u32_at};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Opcode numbers (low byte of an instruction word), from the dispatch table at `0x1402AB3C0`.
pub mod op {
    pub const NOP: u8 = 0x00;
    pub const NATIVE: u8 = 0x01;
    pub const CTRL: u8 = 0x02; // b1: 0 WAIT s, 1 FAULT, 2 RESTART, 3 unlimited budget
    pub const CALL: u8 = 0x03;
    pub const CALL_IMPORT: u8 = 0x04;
    pub const RET: u8 = 0x05;
    pub const ALLOC: u8 = 0x06;
    pub const BRANCH: u8 = 0x07; // b1: 0 always, 1 if nonzero, 2 if zero, 3 if zero then yield
    pub const CASE: u8 = 0x08;
    pub const PUSH_LOCAL: u8 = 0x09;
    pub const STORE_LOCAL: u8 = 0x0A; // b1: 0 =, 1 +=, 2 -=
    pub const POP: u8 = 0x0D;
    pub const PUSH_I16: u8 = 0x0E;
    pub const PUSH_I32: u8 = 0x0F;
    pub const PUSH_RESULT: u8 = 0x10;
    pub const PUSH_STR: u8 = 0x11;
    pub const ADD: u8 = 0x12;
    pub const SUB: u8 = 0x13;
    pub const MUL: u8 = 0x14;
    pub const DIV: u8 = 0x15;
    pub const MOD: u8 = 0x16;
    pub const CMP: u8 = 0x17;
    pub const AND: u8 = 0x18;
    pub const OR: u8 = 0x19;
    pub const XOR: u8 = 0x1A;
    pub const NOT: u8 = 0x1B;
    pub const NEG: u8 = 0x1C;
    pub const LNOT: u8 = 0x1D;
    pub const SHIFT: u8 = 0x1E;
    pub const PUSH_ADDR: u8 = 0x1F;
}

/// Character code of the base-40 name packing (`DAT_1402f76a0`), 0 = not allowed.
pub fn name_char_code(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0' + 1,
        b'A'..=b'Z' => c - b'A' + 11,
        b'a'..=b'z' => c - b'a' + 11,
        b'_' | b'=' => 37,
        _ => 0,
    }
}

/// `FUN_1402847c0`: a name packed into two base-40 words (6 characters each). An invalid
/// character makes the word `0xFFFFFFFF` (the program stores -1). The name stops at NUL or `.`.
pub fn name_hash(name: &str) -> (u32, u32) {
    let b = name.as_bytes();
    let pack = |chunk: &[u8]| -> u32 {
        let mut v: i32 = 0;
        for i in 0..6 {
            let c = match chunk.get(i) {
                None | Some(0) | Some(b'.') => 0,
                Some(&c) => {
                    let k = name_char_code(c);
                    if k == 0 {
                        return u32::MAX;
                    }
                    k
                }
            };
            v = v.wrapping_mul(40).wrapping_add(c as i32);
        }
        v as u32
    };
    let end = b.iter().position(|&c| c == 0 || c == b'.').unwrap_or(b.len());
    let b = &b[..end];
    (pack(b), if b.len() > 6 { pack(&b[6..]) } else { 0 })
}

/// The inverse of [`name_hash`] for display (`None` if a word is not a valid packing).
pub fn name_unhash(h: (u32, u32)) -> Option<String> {
    let unpack = |mut v: u32| -> Option<String> {
        let mut out = [0u8; 6];
        for i in (0..6).rev() {
            let k = (v % 40) as u8;
            v /= 40;
            out[i] = match k {
                0 => 0,
                1..=10 => b'0' + k - 1,
                11..=36 => b'a' + k - 11,
                37 => b'_',
                _ => return None,
            };
        }
        if v != 0 {
            return None;
        }
        Some(out.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect())
    };
    Some(format!("{}{}", unpack(h.0)?, unpack(h.1)?))
}

/// One exported function (function table entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FuncEntry {
    pub hash: (u32, u32),
    /// Entry point as a byte offset into the file (the table stores words).
    pub offset: u32,
}

impl FuncEntry {
    pub fn name(&self) -> String {
        name_unhash(self.hash).unwrap_or_else(|| format!("{:08x}:{:08x}", self.hash.0, self.hash.1))
    }
}

/// An import record (op 4). UNCONFIRMED field meanings, kept raw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportEntry {
    pub hash: (u32, u32),
    pub slots: (u32, u32),
}

/// A parsed `.ssb` file. `data` is the file itself.
#[derive(Debug, Clone)]
pub struct Ssb {
    pub data: Vec<u8>,
    pub code_start: u32,
    pub table_off: u32,
    pub table_end: u32,
    pub pool_off: u32,
    pub functions: Vec<FuncEntry>,
    pub imports: Vec<ImportEntry>,
}

impl Ssb {
    pub fn parse(data: &[u8]) -> Result<Ssb> {
        if data.len() < 0x20 {
            bail!("ssb: {} bytes is shorter than the 0x20-byte header", data.len());
        }
        let w = |i: usize| u32_at(data, i * 4);
        let (code_start, table_off, table_end, pool_off) = (w(0)?, w(1)?, w(2)?, w(3)?);
        let len = data.len() as u32;
        if code_start < 0x20 || code_start % 4 != 0 || code_start > table_off {
            bail!("ssb: code start 0x{code_start:X} is not a word offset inside the file");
        }
        if table_off > table_end || table_end > len || table_off % 4 != 0 {
            bail!("ssb: function table 0x{table_off:X}..0x{table_end:X} is outside the file (0x{len:X} bytes)");
        }
        if (table_end - table_off) % 12 != 0 {
            bail!("ssb: function table length 0x{:X} is not a multiple of 12", table_end - table_off);
        }
        if pool_off < table_end || pool_off > len {
            bail!("ssb: string pool 0x{pool_off:X} is not after the table 0x{table_end:X}");
        }
        let mut functions = Vec::new();
        for i in 0..((table_end - table_off) / 12) as usize {
            let o = table_off as usize + i * 12;
            let words = u32_at(data, o + 8)?;
            let offset = words.checked_mul(4).filter(|&b| b >= code_start && b < table_off).with_context(|| format!("ssb: function entry {i} word offset 0x{words:X} is not in the code"))?;
            functions.push(FuncEntry { hash: (u32_at(data, o)?, u32_at(data, o + 4)?), offset });
        }
        let mut imports = Vec::new();
        let n_imp = (pool_off - table_end) / 16;
        for i in 0..n_imp as usize {
            let o = table_end as usize + i * 16;
            imports.push(ImportEntry { hash: (u32_at(data, o)?, u32_at(data, o + 4)?), slots: (u32_at(data, o + 8)?, u32_at(data, o + 12)?) });
        }
        Ok(Ssb { data: data.to_vec(), code_start, table_off, table_end, pool_off, functions, imports })
    }

    /// `FUN_1402ab5d0` lookup: the exported function with this name.
    pub fn function(&self, name: &str) -> Option<FuncEntry> {
        let h = name_hash(name);
        self.functions.iter().copied().find(|f| f.hash == h)
    }

    /// The string at `pool_off + s` (what `PUSH_STR` pushes the address of); NUL-terminated.
    pub fn string_at(&self, addr: usize) -> Option<&str> {
        let tail = self.data.get(addr..)?;
        let end = tail.iter().position(|&c| c == 0)?;
        std::str::from_utf8(&tail[..end]).ok()
    }

    /// Instruction at a byte offset, `None` if outside the code.
    pub fn insn_at(&self, off: u32) -> Option<Insn> {
        if off % 4 != 0 || off < self.code_start || off >= self.table_off {
            return None;
        }
        let word = u32_at(&self.data, off as usize).ok()?;
        let imm32 = if (word & 0xFF) as u8 == op::PUSH_I32 { Some(u32_at(&self.data, off as usize + 4).ok()?) } else { None };
        Some(Insn { off, word, imm32 })
    }

    /// Recursive-descent disassembly from `main` and every function reached by op 3 calls.
    /// Inline data (the script's tables) is never reached, so it stays out of `insns`.
    pub fn disassemble(&self) -> Disassembly {
        let mut insns: BTreeMap<u32, Insn> = BTreeMap::new();
        let mut functions: BTreeSet<u32> = BTreeSet::new();
        let mut work: Vec<u32> = Vec::new();
        for f in &self.functions {
            functions.insert(f.offset);
            work.push(f.offset);
        }
        while let Some(mut off) = work.pop() {
            loop {
                if insns.contains_key(&off) {
                    break;
                }
                let Some(i) = self.insn_at(off) else { break };
                insns.insert(off, i);
                let next = i.next();
                match i.op() {
                    op::RET => break,
                    op::CTRL if i.b1() == 1 || i.b1() == 2 => break,
                    op::CALL => {
                        let t = i.rel_target();
                        if functions.insert(t) {
                            work.push(t);
                        }
                    }
                    op::BRANCH => {
                        work.push(i.rel_target());
                        if i.b1() == 0 {
                            break;
                        }
                    }
                    op::CASE => work.push(i.rel_target()),
                    _ => {}
                }
                off = next;
            }
        }
        Disassembly { insns, functions }
    }

    /// Text listing: function labels, instructions with native names, and the unreached
    /// ranges inside the code region (the script's inline tables) as dwords.
    pub fn disassembly_text(&self) -> String {
        let d = self.disassemble();
        let mut out = String::new();
        let main = self.function("main").map(|f| f.offset);
        let _ = writeln!(out, "; ssb: {} bytes, code 0x{:X}..0x{:X}, {} exported, {} functions reached, {} instructions", self.data.len(), self.code_start, self.table_off, self.functions.len(), d.functions.len(), d.insns.len());
        let mut off = self.code_start;
        while off < self.table_off {
            if d.functions.contains(&off) {
                let label = if Some(off) == main { "main".to_string() } else { format!("func_{off:04x}") };
                let _ = writeln!(out, "\n{label}:");
            }
            if let Some(i) = d.insns.get(&off) {
                let _ = writeln!(out, "  {off:04x}  {:08x}  {}", i.word, i.text(self));
                off = i.next();
            } else {
                // data: print until the next reached instruction
                let end = d.insns.range(off..).next().map(|(&o, _)| o).unwrap_or(self.table_off).min(self.table_off);
                if end - off == 4 {
                    // a lone unreached word: the compiler's trailing RET after a return
                    if let Some(i) = self.insn_at(off) {
                        let _ = writeln!(out, "  {off:04x}  {:08x}  {}   ; unreached", i.word, i.text(self));
                        off = end;
                        continue;
                    }
                }
                let _ = writeln!(out, "  {off:04x}  .data 0x{:X} bytes (inline table / unreached)", end - off);
                let mut p = off;
                while p < end {
                    let line_end = (p + 32).min(end);
                    let vals: Vec<String> = (p..line_end).step_by(4).map(|q| format!("{}", u32_at(&self.data, q as usize).map(|v| v as i32).unwrap_or(0))).collect();
                    let _ = writeln!(out, "        {p:04x}: {}", vals.join(" "));
                    p = line_end;
                }
                off = end;
            }
        }
        out
    }
}

/// The reachable instructions and function entry points of a script.
#[derive(Debug, Clone)]
pub struct Disassembly {
    pub insns: BTreeMap<u32, Insn>,
    /// Entry offsets: exported functions plus every op 3 call target.
    pub functions: BTreeSet<u32>,
}

/// One decoded instruction word (plus the immediate word of `PUSH_I32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insn {
    pub off: u32,
    pub word: u32,
    pub imm32: Option<u32>,
}

impl Insn {
    pub fn op(&self) -> u8 {
        self.word as u8
    }
    pub fn b1(&self) -> u8 {
        (self.word >> 8) as u8
    }
    /// The signed 16-bit field (bits 16..31).
    pub fn s(&self) -> i16 {
        (self.word >> 16) as i16
    }
    /// Offset of the next instruction (PUSH_I32 takes two words).
    pub fn next(&self) -> u32 {
        self.off + if self.imm32.is_some() { 8 } else { 4 }
    }
    /// Target of a branch / case / call: relative to the word after the instruction word,
    /// in words (`pc_after + s * 4`).
    pub fn rel_target(&self) -> u32 {
        (self.off as i64 + 4 + self.s() as i64 * 4) as u32
    }
    /// Native call fields: group (bits 28..31), function (bits 16..27), argument words dropped (b1).
    pub fn native(&self) -> (u8, u16, u8) {
        ((self.word >> 28) as u8, ((self.word >> 16) & 0xFFF) as u16, self.b1())
    }
    /// Signed 24-bit word offset of `PUSH_ADDR` (relative to the word after it).
    pub fn addr_words(&self) -> i32 {
        (self.word as i32) >> 8
    }

    pub fn text(&self, ssb: &Ssb) -> String {
        let (b1, s) = (self.b1(), self.s());
        match self.op() {
            op::NOP => "nop".into(),
            op::NATIVE => {
                let (g, f, n) = self.native();
                match native_name(g, f) {
                    Some(name) => format!("CALL g{g}.f{f} {name} ({n} args)"),
                    None => format!("CALL g{g}.f{f} ({n} args)"),
                }
            }
            op::CTRL => match b1 {
                0 => format!("WAIT {s}"),
                1 => "FAULT".into(),
                2 => "RESTART".into(),
                3 => "UNLIMITED".into(),
                _ => format!("ctrl.{b1}"),
            },
            op::CALL => format!("CALL -> {:04x}", self.rel_target()),
            op::CALL_IMPORT => format!("CALL_IMPORT {s}"),
            op::RET => format!("RET pop={s}{}", if b1 != 0 { " val" } else { "" }),
            op::ALLOC => format!("ALLOC {}", -(s as i32)),
            op::BRANCH => {
                let k = match b1 {
                    0 => "",
                    1 => "_NZ",
                    2 => "_Z",
                    3 => "_Z_YIELD",
                    _ => "_?",
                };
                format!("JMP{k} -> {:04x}", self.rel_target())
            }
            op::CASE => format!("CASE {b1} else -> {:04x}", self.rel_target()),
            op::PUSH_LOCAL => format!("PUSHLOC {s}"),
            op::STORE_LOCAL => format!("STORELOC{} {s}", ["=", "+=", "-="].get(b1 as usize).unwrap_or(&"?")),
            op::POP => format!("POP {s}"),
            op::PUSH_I16 => format!("PUSH {s}"),
            op::PUSH_I32 => format!("PUSH32 {}", self.imm32.unwrap_or(0) as i32),
            op::PUSH_RESULT => "PUSHRES".into(),
            op::PUSH_STR => {
                let a = ssb.pool_off as i64 + s as i64;
                format!("PUSHSTR {s} ; {:?}", ssb.string_at(a as usize).unwrap_or("?"))
            }
            op::ADD => "ADD".into(),
            op::SUB => format!("SUB{}", if b1 != 0 { "r" } else { "" }),
            op::MUL => "MUL".into(),
            op::DIV => format!("DIV{}", if b1 != 0 { "r" } else { "" }),
            op::MOD => format!("MOD{}", if b1 != 0 { "r" } else { "" }),
            op::CMP => {
                let k = match s {
                    1 => "NE",
                    2 => "GT",
                    3 => "LT",
                    4 => "GE",
                    5 => "LE",
                    6 => "HAS",
                    _ => "EQ",
                };
                format!("CMP {k}{}", if b1 != 0 { "r" } else { "" })
            }
            op::AND => "AND".into(),
            op::OR => "OR".into(),
            op::XOR => "XOR".into(),
            op::NOT => "NOT".into(),
            op::NEG => "NEG".into(),
            op::LNOT => "LNOT".into(),
            op::SHIFT => format!("SHIFT{} {}", if b1 != 0 { "r" } else { "" }, match s { 1 => "SAR", 2 => "SHR", _ => "SHL" }),
            op::PUSH_ADDR => format!("PUSHADDR -> {:04x}", (self.off as i64 + 4 + self.addr_words() as i64 * 4) as u32),
            o => format!("??op{o:02x}"),
        }
    }
}

/// Names (ours; the exe has none) of the group-4 battle natives used by `eneprog006.ssb`,
/// from P0-AI.md Appendix A. Addresses are in that table.
pub fn native_name(group: u8, func: u16) -> Option<&'static str> {
    Some(match (group, func) {
        (0, 5) => "rand",
        (0, 16) => "tbl",
        (0, 17) => "tbl_store",
        (4, 0) => "setvar",
        (4, 1) => "getvar",
        (4, 2) => "vcopy",
        (4, 16) => "dist",
        (4, 18) => "navnode_at",
        (4, 19) => "MOVE_TO",
        (4, 20) => "nav_step",
        (4, 21) => "vadd",
        (4, 23) => "heading",
        (4, 24) => "ground_ok",
        (4, 28) => "vx",
        (4, 29) => "vy",
        (4, 30) => "vz",
        (4, 31) => "navnode2",
        (4, 32) => "rand",
        (4, 33) => "angdiff",
        (4, 37) => "nodedist",
        (4, 48) => "PRESS",
        (4, 49) => "path49",
        (4, 51) => "path51",
        (4, 53) => "f53",
        (4, 56) => "stuck",
        (4, 60) => "path_step",
        (4, 82) => "style0",
        (4, 85) => "style2",
        (4, 96) => "set_target",
        (4, 97) => "get_target",
        (4, 112) => "self",
        (4, 113) => "vpos_of",
        (4, 114) => "vstart_of",
        (4, 115) => "yaw_of",
        (4, 117) => "pairflag",
        (4, 118) => "hp",
        (4, 119) => "maxaura",
        (4, 120) => "aura",
        (4, 121) => "has_status",
        (4, 122) => "team_of",
        (4, 123) => "f123",
        (4, 124) => "cond",
        (4, 125) => "f125",
        (4, 134) => "dodge_skill",
        (4, 135) => "f135",
        (4, 144) => "slot_skill",
        (4, 146) => "slot_ctype",
        (4, 147) => "slot_aura_cost",
        (4, 148) => "slot_ready",
        (4, 150) => "slot_state",
        (4, 161) => "sk_ctype",
        (4, 162) => "sk_sel",
        (4, 165) => "sk_class",
        (4, 166) => "sk_motion",
        (4, 168) => "sk_s44",
        (4, 169) => "sk_hpcost",
        (4, 171) => "sk_ok",
        (4, 175) => "sk_aitype",
        (4, 176) => "scan_reset",
        (4, 177) => "scan_next",
        (4, 178) => "obj_skill",
        (4, 179) => "obj_pos",
        (4, 186) => "obj186",
        (4, 192) => "tbl192",
        (4, 193) => "tbl193",
        (4, 194) => "f194",
        (4, 195) => "f195",
        (4, 197) => "cap_skill",
        (4, 198) => "cap_due",
        (4, 199) => "cap_pos",
        (4, 200) => "cap_index",
        (4, 208) => "TICK",
        (4, 210) => "nodecount",
        (4, 229) => "f229",
        (4, 234) => "f234",
        (4, 235) => "f235",
        (4, 425) => "stage_code",
        (4, 426) => "stage_div10",
        _ => return None,
    })
}

/// Read a `.ssb` file's bytes into [`Ssb`] (convenience for tools).
pub fn load(path: &std::path::Path) -> Result<Ssb> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let _ = slice(&data, 0, 0x20)?;
    Ssb::parse(&data).with_context(|| path.display().to_string())
}

/// Test helper: a file-form script with `code` words at 0x20, one `main` entry at word 8,
/// and `pool` as the string pool.
#[cfg(test)]
pub(crate) fn build_test_script_with_pool(code: &[u32], pool: &[u8]) -> Ssb {
    let table_off = 0x20 + code.len() as u32 * 4;
    let mut w: Vec<u32> = vec![0x20, table_off, table_off + 12, table_off + 12, 0, 0xFFFF_FFFF, 0, 0];
    w.extend_from_slice(code);
    let (h0, h1) = name_hash("main");
    w.extend_from_slice(&[h0, h1, 8]);
    let mut data: Vec<u8> = w.iter().flat_map(|v| v.to_le_bytes()).collect();
    data.extend_from_slice(pool);
    Ssb::parse(&data).unwrap()
}

#[cfg(test)]
pub(crate) fn build_test_script(code: &[u32]) -> Ssb {
    build_test_script_with_pool(code, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::build_test_script as build;

    #[test]
    fn main_hashes_to_the_programs_value() {
        // READ: the function-table entry of every com/ai script, `0x8E225400, 0`.
        assert_eq!(name_hash("main"), (0x8E22_5400, 0));
        assert_eq!(name_hash("MAIN"), (0x8E22_5400, 0));
        assert_eq!(name_unhash((0x8E22_5400, 0)).as_deref(), Some("main"));
        // second word packs characters 7..12; '.' ends the name (FUN_1402847c0)
        let (a, b) = name_hash("InitMenu");
        assert_eq!(name_unhash((a, b)).as_deref(), Some("initmenu"));
        assert_eq!(name_hash("main.x"), name_hash("main"));
        assert_eq!(name_hash("a!").0, u32::MAX);
    }

    #[test]
    fn parses_a_minimal_script() {
        let s = build(&[0x0000_0005]);
        assert_eq!(s.code_start, 0x20);
        assert_eq!(s.functions, vec![FuncEntry { hash: (0x8E22_5400, 0), offset: 0x20 }]);
        assert_eq!(s.function("main").unwrap().offset, 0x20);
        assert!(s.function("nope").is_none());
        assert!(s.imports.is_empty());
    }

    #[test]
    fn rejects_bad_headers() {
        assert!(Ssb::parse(&[0u8; 8]).is_err());
        let mut s = build(&[5]).data;
        s[4] = 0xF0; // table offset beyond the file
        assert!(Ssb::parse(&s).is_err());
    }

    #[test]
    fn instruction_fields_and_targets() {
        // 0x07 branch at 0x20: target = pc_after + s*4 (FUN_1402aaa30 op 7: puVar18 + sVar9)
        let i = Insn { off: 0x20, word: 0x0002_0207, imm32: None };
        assert_eq!((i.op(), i.b1(), i.s()), (op::BRANCH, 2, 2));
        assert_eq!(i.rel_target(), 0x20 + 4 + 8);
        let back = Insn { off: 0x40, word: 0xFFFF_0007, imm32: None };
        assert_eq!(back.rel_target(), 0x40);
        // native: 0x40a90101 = group 4, f169, drops 1 argument word
        let n = Insn { off: 0, word: 0x40A9_0101, imm32: None };
        assert_eq!(n.native(), (4, 169, 1));
        // PUSH_ADDR: signed 24-bit words from the next word
        let a = Insn { off: 0x1c38, word: 0xFFFF_E41F, imm32: None };
        assert_eq!((a.off as i64 + 4 + a.addr_words() as i64 * 4) as u32, 0x1BCC);
    }

    #[test]
    fn disassembly_skips_inline_data_and_names_natives() {
        // main: native call, return, then inline data (not reachable)
        let s = build(&[0x4030_0101, 0x0000_0005, 0xDEAD_BEEF, 0x0000_0001]);
        let d = s.disassemble();
        assert_eq!(d.insns.len(), 2);
        let t = s.disassembly_text();
        assert!(t.contains("PRESS"), "{t}");
        assert!(t.contains(".data"), "{t}");
    }
}
