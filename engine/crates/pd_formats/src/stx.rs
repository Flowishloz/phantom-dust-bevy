//! STX ("STHD") streamed music: 0x800-byte blocks. Block 0 is a header;
//! every later block is a 0x20-byte header followed by 504 left samples then
//! 504 right samples (16-bit PCM).

use crate::bytes::*;
use anyhow::{bail, Result};

const BLOCK: usize = 0x800;
const BLOCK_SAMPLES: usize = 504;

pub struct Music {
    pub sample_rate: u32,
    /// Interleaved stereo 16-bit samples.
    pub samples: Vec<i16>,
    pub loop_start_block: u16,
    pub loop_end_block: u16,
}

pub fn decode(d: &[u8]) -> Result<Music> {
    if slice(d, 0, 4)? != b"STHD" {
        bail!("not an STX file");
    }
    let block_count = u16_at(d, 0x10)? as usize;
    let loop_start_block = u16_at(d, 0x1A)?;
    let loop_end_block = u16_at(d, 0x1C)?;
    // Channel table follows the first 0x20-byte header.
    let sample_rate = u32_at(d, 0x20)?;

    let mut samples = Vec::with_capacity(block_count * BLOCK_SAMPLES * 2);
    for b in 1..block_count {
        let base = b * BLOCK + 0x20;
        if base + BLOCK_SAMPLES * 4 > d.len() {
            break;
        }
        for i in 0..BLOCK_SAMPLES {
            samples.push(i16_at(d, base + i * 2)?);
            samples.push(i16_at(d, base + (BLOCK_SAMPLES + i) * 2)?);
        }
    }
    Ok(Music { sample_rate, samples, loop_start_block, loop_end_block })
}

pub fn to_wav(m: &Music) -> Vec<u8> {
    let data_len = (m.samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&m.sample_rate.to_le_bytes());
    out.extend_from_slice(&(m.sample_rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in &m.samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Audio frames (one left + one right sample) per block.
pub const BLOCK_FRAMES: usize = BLOCK_SAMPLES;

impl Music {
    /// The loop region in frames from the start of the audio: the player plays the
    /// intro once, then repeats `start..end` (header `u16 @0x1A` / `@0x1C` count
    /// blocks, block 0 being the header; 65535 = no loop point, the track plays once:
    /// the win / lose / ending jingles). `None` = plays once.
    pub fn loop_frames(&self) -> Option<(usize, usize)> {
        let total = self.samples.len() / 2;
        if self.loop_start_block == u16::MAX || self.loop_end_block == u16::MAX || self.loop_start_block == 0 {
            return None;
        }
        let start = (self.loop_start_block as usize - 1) * BLOCK_FRAMES;
        let end = (self.loop_end_block as usize).saturating_sub(1) * BLOCK_FRAMES;
        let end = end.min(total);
        (start < end).then_some((start, end))
    }
}

/// The game's own music container (`cache/music/<track>.pdm`, written by
/// `pd_convert musictracks`): `"PDM1"`, `u32` sample rate, `u32` loop start
/// frame, `u32` loop end frame (both 0 = plays once), then interleaved stereo
/// `i16`. Same samples as the WAV, plus the loop region the WAV cannot carry.
pub fn to_pdm(m: &Music) -> Vec<u8> {
    let (ls, le) = m.loop_frames().unwrap_or((0, 0));
    let mut out = Vec::with_capacity(16 + m.samples.len() * 2);
    out.extend_from_slice(b"PDM1");
    out.extend_from_slice(&m.sample_rate.to_le_bytes());
    out.extend_from_slice(&(ls as u32).to_le_bytes());
    out.extend_from_slice(&(le as u32).to_le_bytes());
    for s in &m.samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// A parsed `.pdm`: (sample rate, loop frames, interleaved samples).
pub fn parse_pdm(d: &[u8]) -> Option<(u32, Option<(usize, usize)>, Vec<i16>)> {
    if d.len() < 16 || &d[0..4] != b"PDM1" {
        return None;
    }
    let u = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
    let samples: Vec<i16> = d[16..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    let (ls, le) = (u(8) as usize, u(12) as usize);
    let frames = samples.len() / 2;
    let lp = (ls < le && le <= frames).then_some((ls, le));
    Some((u(4), lp, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn music(start: u16, end: u16, frames: usize) -> Music {
        Music { sample_rate: 44099, samples: (0..frames * 2).map(|i| i as i16).collect(), loop_start_block: start, loop_end_block: end }
    }

    #[test]
    fn loop_region_is_blocks_start_to_end_minus_the_header() {
        // 04_Highway: start 181, end = block count 11310 -> frames (180 * 504) .. (11309 * 504).
        let m = music(181, 11310, 11309 * BLOCK_FRAMES);
        assert_eq!(m.loop_frames(), Some((180 * BLOCK_FRAMES, 11309 * BLOCK_FRAMES)));
    }

    #[test]
    fn jingles_have_no_loop() {
        assert_eq!(music(65535, 65535, 1000).loop_frames(), None);
    }

    #[test]
    fn pdm_round_trips() {
        let m = music(3, 9, 8 * BLOCK_FRAMES);
        let (rate, lp, s) = parse_pdm(&to_pdm(&m)).unwrap();
        assert_eq!((rate, lp, s.len()), (44099, Some((2 * BLOCK_FRAMES, 8 * BLOCK_FRAMES)), m.samples.len()));
        let (_, lp, _) = parse_pdm(&to_pdm(&music(65535, 65535, 10))).unwrap();
        assert_eq!(lp, None);
    }
}
