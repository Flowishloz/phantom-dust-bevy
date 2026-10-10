//! `setup/stageset/stageset_en.dat` — the developer's own per-slot stage table
//! (versus stage select list). Confirmed by inspection: this is the authoritative
//! source for stage display names, far more reliable than guessing from renders.
//!
//! ✅ Confirmed layout (established 2026-09-26 by dumping printable ASCII runs
//! and checking the header ints against the string-table offset):
//! - `u32` file size (self-referential; matches the file's own length)
//! - `u32` entry count (30 in the shipped file: slots 0..29)
//! - `u32` string-table offset (absolute, from file start)
//! - then a `u32[]`/flags region we have not decoded (not needed for names)
//! - at the string-table offset: `count * 3` consecutive NUL-terminated ASCII
//!   strings, three per stage slot in table order: **long name**, **short
//!   name / family**, **internal code** (e.g. `"Spiral Highway"`, `"Highway"`,
//!   `"st02Highway-evening"`).
//! - **the slot's position in the table (0-based) is the stage id**: slot 2 ->
//!   `st02`, slot 12 -> `st12`, etc. Verified: every internal code that has an
//!   `stNN` prefix has `NN` equal to its slot position; slots without a real
//!   stage (`"09 Dummy"`, `"10 Dummy"`, ...) have no corresponding `.alr` file
//!   in `Map/`, and the reverse holds (every `stNN.alr` that isn't a dummy
//!   slot has a real name here). `st01`'s row is `"Edgar's Dream"` /
//!   `"Edgar's Dream"` / `"LD_InsideofRuin"` — a one-off story stage, not part
//!   of a recolour family.
//!
//! `st09`, its `st09_01..08` pieces, and `st32` are **not** in this table at
//! all (st09 is dummy slot 9; st32 has no slot — index 32 doesn't exist in a
//! 30-entry table). See `PIPELINE.md` / `ROADMAP.md` for what we think they are.

pub struct StageEntry {
    /// `stNN`, derived from the slot's position in the table.
    pub id: String,
    /// Long display name, e.g. "Spiral Highway".
    pub name: String,
    /// Short name / recolour family, e.g. "Highway".
    pub family: String,
    /// Internal code, e.g. "st02Highway-evening".
    pub code: String,
    /// True for an empty "NN Dummy" slot (no real stage).
    pub dummy: bool,
}

/// Parse `stageset_en.dat` (or the Japanese `stageset.dat`, though that one
/// has no long/short names, just codes — see FORMATS-ALR.md). Returns one
/// entry per table slot, in slot order.
pub fn parse(bytes: &[u8]) -> Vec<StageEntry> {
    if bytes.len() < 12 {
        return Vec::new();
    }
    let count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let table_off = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let mut strings = Vec::new();
    let mut cur = table_off;
    while strings.len() < count * 3 && cur < bytes.len() {
        let end = bytes[cur..].iter().position(|&b| b == 0).map(|p| cur + p).unwrap_or(bytes.len());
        strings.push(String::from_utf8_lossy(&bytes[cur..end]).trim().to_string());
        cur = end + 1;
    }
    strings
        .chunks(3)
        .enumerate()
        .map(|(i, c)| {
            let name = c.first().cloned().unwrap_or_default();
            let family = c.get(1).cloned().unwrap_or_default();
            let code = c.get(2).cloned().unwrap_or_default();
            let dummy = name.to_lowercase().contains("dummy");
            StageEntry { id: format!("st{i:02}"), name, family, code, dummy }
        })
        .collect()
}
