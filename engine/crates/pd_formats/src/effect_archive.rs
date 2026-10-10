//! The packed effect archive `Data/Effect/e.i` + `a<k>.e`: what the game
//! actually loads skill effects from (loose `eftNNNN.alr`/`.eft` files are
//! not on its normal path). Layout and evidence: docs/PROGRAM-RE.md "Packed
//! effects" (`FUN_14018cd10` -> `FUN_14018d0b0`); ported from our own Python
//! reader, `engine/re/effect_archive.py`.
//!
//! `e.i`: u16 archive count, u16 entry count, then entry-count x
//! `{u16 effect number, u16 archive index, u32 byte offset, u32 byte size}`,
//! sorted by number. An entry's bytes (in `a<archive>.e`) are the effect's
//! ALR padded to 0x800, followed by one 0x800 block whose first 412 bytes are
//! its `.eft` event table (the padding after that is not zeroed).

use crate::alr::Alr;
use crate::bytes::*;
use anyhow::{bail, Context, Result};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Size of the `.eft` event table block at the end of each archive entry
/// (same layout as the loose `eftNNNN.eft` files; see `effect::parse_eft`).
pub const EFT_SIZE: usize = 412;

/// One entry of `e.i`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub number: u16,
    pub archive: u16,
    pub offset: u32,
    pub size: u32,
}

/// Parses `e.i`'s bytes into its entries (sorted by effect number, as stored).
pub fn parse_index(d: &[u8]) -> Result<Vec<ArchiveEntry>> {
    let _archive_count = u16_at(d, 0)?;
    let count = u16_at(d, 2)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let base = 4 + 12 * i;
        out.push(ArchiveEntry { number: u16_at(d, base)?, archive: u16_at(d, base + 2)?, offset: u32_at(d, base + 4)?, size: u32_at(d, base + 8)? });
    }
    Ok(out)
}

/// The `Effect/` directory's packed archive: `e.i` plus the `a<k>.e` files it indexes.
pub struct EffectArchive {
    dir: PathBuf,
    entries: Vec<ArchiveEntry>,
}

impl EffectArchive {
    pub fn open(dir: &Path) -> Result<Self> {
        let path = dir.join("e.i");
        let d = std::fs::read(&path).with_context(|| path.display().to_string())?;
        Ok(EffectArchive { dir: dir.to_path_buf(), entries: parse_index(&d)? })
    }

    pub fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    pub fn contains(&self, number: u16) -> bool {
        self.entries.iter().any(|e| e.number == number)
    }

    /// Reads one entry's raw bytes: the ALR (padded to 0x800) and the 412
    /// bytes of its `.eft` event table. `None` if `number` isn't in `e.i`.
    pub fn read_raw(&self, number: u16) -> Result<Option<(Vec<u8>, [u8; EFT_SIZE])>> {
        let Some(e) = self.entries.iter().find(|e| e.number == number) else { return Ok(None) };
        if (e.size as usize) < EFT_SIZE {
            bail!("effect {number}: entry size 0x{:x} smaller than the .eft block", e.size);
        }
        let path = self.dir.join(format!("a{}.e", e.archive));
        let mut f = std::fs::File::open(&path).with_context(|| path.display().to_string())?;
        f.seek(SeekFrom::Start(e.offset as u64)).with_context(|| format!("seeking {} in {}", e.offset, path.display()))?;
        let mut blob = vec![0u8; e.size as usize];
        f.read_exact(&mut blob).with_context(|| format!("reading {} bytes from {}", e.size, path.display()))?;
        let alr_len = e.size as usize - 0x800;
        let alr = blob[..alr_len].to_vec();
        let mut eft = [0u8; EFT_SIZE];
        eft.copy_from_slice(&blob[alr_len..alr_len + EFT_SIZE]);
        Ok(Some((alr, eft)))
    }

    /// Reads and parses one effect's ALR + `.eft` straight from the archive.
    pub fn read(&self, number: u16) -> Result<Option<(Alr, [u8; EFT_SIZE])>> {
        let Some((alr, eft)) = self.read_raw(number)? else { return Ok(None) };
        Ok(Some((Alr::parse(alr).with_context(|| format!("effect {number}: parsing packed ALR"))?, eft)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect_dir() -> Option<PathBuf> {
        // Real game data (never committed, and not in this lane's worktree):
        // the main checkout's game_files, or PD_GAME_FILES for another layout.
        let p = std::env::var_os("PD_GAME_FILES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"..\..\..\game_files\Assets\Data\Effect"));
        p.join("e.i").exists().then_some(p)
    }

    #[test]
    fn index_parses_a_synthetic_e_i() {
        let mut d = vec![0u8; 4 + 12 * 2];
        d[0..2].copy_from_slice(&8u16.to_le_bytes());
        d[2..4].copy_from_slice(&2u16.to_le_bytes());
        let put = |d: &mut Vec<u8>, base: usize, number: u16, archive: u16, offset: u32, size: u32| {
            d[base..base + 2].copy_from_slice(&number.to_le_bytes());
            d[base + 2..base + 4].copy_from_slice(&archive.to_le_bytes());
            d[base + 4..base + 8].copy_from_slice(&offset.to_le_bytes());
            d[base + 8..base + 12].copy_from_slice(&size.to_le_bytes());
        };
        put(&mut d, 4, 1, 0, 0, 0x1000);
        put(&mut d, 16, 302, 3, 0x1000, 0x1800);
        let entries = parse_index(&d).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], ArchiveEntry { number: 1, archive: 0, offset: 0, size: 0x1000 });
        assert_eq!(entries[1], ArchiveEntry { number: 302, archive: 3, offset: 0x1000, size: 0x1800 });
    }

    /// Real archive: 496 entries (`e.i`'s own header count), known entries
    /// (effect 1 exists loose too; 302-321 are packed-only), and the packed
    /// ALR of a packed-only effect parses with our reader.
    #[test]
    fn real_archive_has_496_entries() {
        let Some(dir) = effect_dir() else {
            eprintln!("no game_files: archive test skipped");
            return;
        };
        let a = EffectArchive::open(&dir).unwrap();
        assert_eq!(a.entries().len(), 496);
        assert!(a.contains(1));
        assert!(a.contains(302), "effect 302 (packed-only, no loose eft0302) missing from e.i");
        assert!(a.contains(321), "effect 321 (packed-only) missing from e.i");

        let (alr, eft) = a.read(302).unwrap().expect("effect 302 in archive");
        assert!(!alr.models.is_empty(), "packed eft0302's ALR should parse with model data");
        assert_eq!(eft.len(), EFT_SIZE);
        // eft is a real event table: word at +244 (slot 0's flag) is 0 or 1/5.
        let flag = i16_at(&eft, 244).unwrap();
        assert!(flag == 0 || flag == 1 || flag == 5, "slot 0 flag {flag}");
    }

    /// A loose file exists for effect 1 (Psycho Wave): the packed copy should
    /// still parse to a valid ALR with the same rough shape (model count).
    #[test]
    fn real_archive_matches_loose_alr_model_count() {
        let Some(dir) = effect_dir() else {
            eprintln!("no game_files: archive test skipped");
            return;
        };
        let loose = dir.join("eft0001.alr");
        if !loose.exists() {
            eprintln!("no loose eft0001.alr: comparison skipped");
            return;
        }
        let a = EffectArchive::open(&dir).unwrap();
        let (packed, _eft) = a.read(1).unwrap().expect("effect 1 in archive");
        let loose_alr = Alr::load(&loose).unwrap();
        assert_eq!(packed.models.len(), loose_alr.models.len());
    }
}
