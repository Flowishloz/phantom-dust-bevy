//! Decode ALR textures (top mip level only) to RGBA8.

use crate::alr::{Alr, PixelFormat, TextureEntry};
use anyhow::{bail, Result};

pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn has_alpha(&self) -> bool {
        self.pixels.chunks_exact(4).any(|p| p[3] < 250)
    }

    pub fn to_png(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.width, self.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header()?;
            w.write_image_data(&self.pixels)?;
        }
        Ok(out)
    }
}

fn top_mip_size(t: &TextureEntry) -> usize {
    let (w, h) = (t.width as usize, t.height as usize);
    let blocks = w.div_ceil(4).max(1) * h.div_ceil(4).max(1);
    match t.format {
        PixelFormat::Dxt1 => blocks * 8,
        PixelFormat::Dxt3 | PixelFormat::Dxt5 => blocks * 16,
        PixelFormat::R8 | PixelFormat::A8 => w * h,
        PixelFormat::Bgra5551 | PixelFormat::Bgra4444 | PixelFormat::Bgr565 => w * h * 2,
        PixelFormat::Rgba8 => w * h * 4,
        PixelFormat::Unknown(_) => 0,
    }
}

fn expand(v: u16, bits: u32) -> u8 {
    let max = (1u32 << bits) - 1;
    ((v as u32 * 255 + max / 2) / max) as u8
}

pub fn decode(alr: &Alr, t: &TextureEntry) -> Result<Rgba> {
    let (w, h) = (t.width, t.height);
    let size = top_mip_size(t);
    if size == 0 {
        bail!("unsupported pixel format {:?}", t.format);
    }
    let src = alr.resource(t.data_ptr, size)?;
    let mut px = vec![0u8; (w * h * 4) as usize];

    let bc = match t.format {
        PixelFormat::Dxt1 => Some(texpresso::Format::Bc1),
        PixelFormat::Dxt3 => Some(texpresso::Format::Bc2),
        PixelFormat::Dxt5 => Some(texpresso::Format::Bc3),
        _ => None,
    };
    if let Some(f) = bc {
        f.decompress(src, w as usize, h as usize, &mut px);
        return Ok(Rgba { width: w, height: h, pixels: px });
    }

    for i in 0..(w * h) as usize {
        let o = &mut px[i * 4..i * 4 + 4];
        match t.format {
            PixelFormat::Rgba8 => {
                // Stored B, G, R, A (D3D A8R8G8B8 little-endian).
                o.copy_from_slice(&[src[i * 4 + 2], src[i * 4 + 1], src[i * 4], src[i * 4 + 3]]);
            }
            PixelFormat::R8 => o.copy_from_slice(&[src[i], src[i], src[i], 255]),
            PixelFormat::A8 => o.copy_from_slice(&[255, 255, 255, src[i]]),
            PixelFormat::Bgr565 => {
                let v = u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]);
                o.copy_from_slice(&[expand(v >> 11, 5), expand((v >> 5) & 0x3F, 6), expand(v & 0x1F, 5), 255]);
            }
            PixelFormat::Bgra5551 => {
                let v = u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]);
                o.copy_from_slice(&[
                    expand((v >> 10) & 0x1F, 5),
                    expand((v >> 5) & 0x1F, 5),
                    expand(v & 0x1F, 5),
                    if v >> 15 != 0 { 255 } else { 0 },
                ]);
            }
            PixelFormat::Bgra4444 => {
                let v = u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]);
                o.copy_from_slice(&[
                    expand((v >> 8) & 0xF, 4),
                    expand((v >> 4) & 0xF, 4),
                    expand(v & 0xF, 4),
                    expand(v >> 12, 4),
                ]);
            }
            _ => unreachable!(),
        }
    }
    Ok(Rgba { width: w, height: h, pixels: px })
}

/// The screen-distortion pieces' bump map (`PDEFTOPT03/05`'s texture slot 0, pixel format
/// code 8): two bytes per pixel, top mip first. Read as luminance + alpha (A8L8): the data
/// has both bytes equal in every pixel, with 127 as the flat value, and the pixel shader
/// (`PDEFTOPT05_ps`) takes `2 (sample - 0.5)` from two channels. The byte order and the
/// reading as A8L8 are not proven: an unsigned bump map where the two bytes agree reads the
/// same either way. Written as RGB = luminance, A = alpha.
pub fn decode_bump(alr: &Alr, t: &TextureEntry) -> Result<Rgba> {
    if !matches!(t.format, PixelFormat::Unknown(8)) {
        return decode(alr, t);
    }
    let (w, h) = (t.width, t.height);
    let src = alr.resource(t.data_ptr, (w * h * 2) as usize)?;
    let mut px = vec![0u8; (w * h * 4) as usize];
    for i in 0..(w * h) as usize {
        px[i * 4..i * 4 + 4].copy_from_slice(&[src[i * 2], src[i * 2], src[i * 2], src[i * 2 + 1]]);
    }
    Ok(Rgba { width: w, height: h, pixels: px })
}
