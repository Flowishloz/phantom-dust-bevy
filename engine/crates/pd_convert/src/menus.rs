//! `pd_convert menus <Data dir> <out dir>` — the versus front end's art,
//! taken from the game's own menu files (docs/MENUS.md):
//!
//! - `st0..6.png`  `Menu/StMenu.alr`  (row panel, ghost logo, teal clouds, gradients)
//! - `vs0..3.png`  `Menu/VsMode.alr`  (stage previews, character portraits)
//! - `wus0..7.png` `Menu/WusMnEn.alr` (A/B/X/Y button glyphs, arrows)
//! - `font.png`    `System/dbfont.ddx` — the game's menu font. The file is a
//!   raw 1024x2048 DXT1 texture with no header; glyphs sit in 16x16 cells,
//!   64 per row, indexed by Windows-1252 code (cell = code). Only codes
//!   0..255 (the top 1024x64) are written, white with a baked 1px shadow so
//!   a tint colours the letter and leaves the shadow dark.
//! - `bios.json`   character id -> {name, bio}: names from
//!   `setup/charaset/charaset_en.dat`, bios from the versus menu script
//!   `Menu/vsmenu_en.ssb` ("■ Name\nText\n" entries).

use anyhow::{Context, Result};
use pd_formats::alr::Alr;
use pd_formats::texture::Rgba;
use std::path::Path;

pub fn run(args: &[String]) -> Result<()> {
    let usage = "usage: pd_convert menus <Data dir> <out dir>";
    let data = Path::new(args.first().context(usage)?);
    let out = Path::new(args.get(1).context(usage)?);
    std::fs::create_dir_all(out)?;
    for (file, prefix) in [("Menu/StMenu.alr", "st"), ("Menu/VsMode.alr", "vs"), ("Menu/WusMnEn.alr", "wus")] {
        let a = Alr::load(&data.join(file)).with_context(|| file.to_string())?;
        for (i, t) in a.textures.iter().enumerate() {
            let mut img = pd_formats::texture::decode(&a, t)?;
            std::fs::write(out.join(format!("{prefix}{i}.png")), img.to_png()?)?;
            println!("{prefix}{i}.png: {}x{} ({file})", img.width, img.height);
            if prefix == "st" && i == 2 {
                // The logo's shape as a white mask, so a tint can draw it as
                // the original's light, ghosted watermark.
                img.pixels.chunks_exact_mut(4).for_each(|p| p[..3].fill(255));
                std::fs::write(out.join("logo.png"), img.to_png()?)?;
            }
        }
    }
    let ddx = std::fs::read(data.join("System/dbfont.ddx")).context("System/dbfont.ddx")?;
    let font = menu_font(&ddx)?;
    std::fs::write(out.join("font.png"), font.to_png()?)?;
    std::fs::write(out.join("font.json"), serde_json::to_string(&serde_json::json!({ "cell": 16, "columns": 64, "glyphs": glyph_extents(&font) }))?)?;
    println!("font.png + font.json: 1024x64 (System/dbfont.ddx, codes 0-255, per-glyph [left, width])");
    let names = chara_names(&std::fs::read(data.join("setup/charaset/charaset_en.dat")).context("charaset_en.dat")?);
    let bios = bios(&std::fs::read(data.join("Menu/vsmenu_en.ssb")).context("Menu/vsmenu_en.ssb")?);
    let mut map = serde_json::Map::new();
    for (id, name) in names.iter().filter(|(id, _)| id.starts_with("pc")) {
        let bio = bios.iter().find(|(t, _)| t == name).map(|(_, b)| b.clone()).unwrap_or_default();
        map.insert(id.clone(), serde_json::json!({ "name": name, "bio": bio }));
    }
    println!("bios.json: {} characters, {} with a bio", map.len(), map.values().filter(|v| v["bio"] != "").count());
    std::fs::write(out.join("bios.json"), serde_json::to_string_pretty(&serde_json::Value::Object(map))?)?;
    Ok(())
}

/// Codes 0..255 of the font atlas (the top 16 block rows of the 1024-wide
/// DXT1 texture), white glyphs plus a dark shadow one pixel down-right.
fn menu_font(ddx: &[u8]) -> Result<Rgba> {
    const W: usize = 1024;
    const H: usize = 64;
    anyhow::ensure!(ddx.len() >= W / 4 * H / 4 * 8, "dbfont.ddx too short");
    let mut alpha = vec![0u8; W * H];
    for by in 0..H / 4 {
        for bx in 0..W / 4 {
            let b = &ddx[(by * (W / 4) + bx) * 8..][..8];
            let (c0, c1) = (u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]));
            let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            for p in 0..16 {
                let idx = (bits >> (p * 2)) & 3;
                // BC1: with c0 <= c1, index 3 is transparent; everything else is opaque.
                let a = if c0 <= c1 && idx == 3 { 0 } else { 255 };
                alpha[(by * 4 + p / 4) * W + bx * 4 + p % 4] = a;
            }
        }
    }
    let mut px = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let o = (y * W + x) * 4;
            if alpha[y * W + x] > 127 {
                px[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
            } else if x > 0 && y > 0 && x % 16 != 0 && y % 16 != 0 && alpha[(y - 1) * W + x - 1] > 127 {
                px[o..o + 4].copy_from_slice(&[0, 0, 0, 190]);
            }
        }
    }
    Ok(Rgba { width: W as u32, height: H as u32, pixels: px })
}

/// `[left, width]` of the letter (not its shadow) in each 16x16 cell of
/// the baked font, for proportional spacing; `[0, 0]` for empty cells.
fn glyph_extents(font: &Rgba) -> Vec<[u32; 2]> {
    let w = font.width as usize;
    (0..256)
        .map(|code| {
            let (cx, cy) = ((code % 64) * 16, (code / 64) * 16);
            let cols: Vec<usize> = (0..16).filter(|&x| (0..16).any(|y| font.pixels[((cy + y) * w + cx + x) * 4] == 255 && font.pixels[((cy + y) * w + cx + x) * 4 + 3] == 255)).collect();
            match (cols.first(), cols.last()) {
                (Some(&a), Some(&b)) => [a as u32, (b - a + 1) as u32],
                _ => [0, 0],
            }
        })
        .collect()
}

/// `(id, display name)` for every character in `charaset_en.dat`: header
/// (size, count, string-table offset), then NUL-terminated strings in groups
/// of id / name / name / voice bank (the name may be empty or "player").
fn chara_names(dat: &[u8]) -> Vec<(String, String)> {
    let table = dat.get(8..12).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).unwrap_or(0);
    let strings: Vec<String> = dat.get(table..).unwrap_or(&[]).split(|&c| c == 0).map(|s| String::from_utf8_lossy(s).into_owned()).collect();
    let mut out = Vec::new();
    for (i, s) in strings.iter().enumerate() {
        let is_id = |s: &str| s.len() >= 5 && s.len() <= 7 && s.ends_with('a') && s.chars().take(2).all(|c| c.is_ascii_lowercase()) && s[s.len() - 3..s.len() - 1].chars().all(|c| c.is_ascii_digit());
        if is_id(s) {
            let name = strings.get(i + 1).cloned().unwrap_or_default();
            out.push((s.clone(), name));
        }
    }
    out
}

/// "■ Title\nBody\n" entries of a menu script, as (title, body joined into one line).
fn bios(ssb: &[u8]) -> Vec<(String, String)> {
    let mark = "\u{25A0} ".as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = ssb[i..].windows(mark.len()).position(|w| w == mark) {
        let start = i + p + mark.len();
        let end = ssb[start..].iter().position(|&c| c == 0).map_or(ssb.len(), |e| start + e);
        let text = String::from_utf8_lossy(&ssb[start..end]).into_owned();
        let mut lines = text.lines();
        let title = lines.next().unwrap_or("").trim().to_string();
        let body = lines.map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
        out.push((title, body));
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bios_are_split_into_title_and_body() {
        let mut ssb = b"junk\x00".to_vec();
        ssb.extend_from_slice("\u{25A0} Edgar\nLine one. \nLine two.\n\0\0\u{25A0} JD\nOnly line.\n\0".as_bytes());
        assert_eq!(bios(&ssb), vec![("Edgar".into(), "Line one. Line two.".into()), ("JD".into(), "Only line.".into())]);
    }

    #[test]
    fn chara_names_pair_ids_with_names() {
        let mut dat = vec![0u8; 16];
        dat[8..12].copy_from_slice(&16u32.to_le_bytes());
        dat.extend_from_slice(b"pc00a\0player\0player\0BattleVo_Hero.bin\0pc06a\0pH\0pH\0BattleVo_PH.bin\0");
        assert_eq!(chara_names(&dat), vec![("pc00a".into(), "player".into()), ("pc06a".into(), "pH".into())]);
    }
}
