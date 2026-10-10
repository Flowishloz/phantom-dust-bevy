//! Bounds-checked little-endian reads.

use anyhow::{bail, Result};

pub fn slice(d: &[u8], off: usize, len: usize) -> Result<&[u8]> {
    match off.checked_add(len) {
        Some(end) if end <= d.len() => Ok(&d[off..end]),
        _ => bail!("read of {len} bytes at 0x{off:X} is past the end (size 0x{:X})", d.len()),
    }
}

pub fn u8_at(d: &[u8], off: usize) -> Result<u8> {
    Ok(slice(d, off, 1)?[0])
}

pub fn u16_at(d: &[u8], off: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(slice(d, off, 2)?.try_into().unwrap()))
}

pub fn i16_at(d: &[u8], off: usize) -> Result<i16> {
    Ok(i16::from_le_bytes(slice(d, off, 2)?.try_into().unwrap()))
}

pub fn u32_at(d: &[u8], off: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(d, off, 4)?.try_into().unwrap()))
}

pub fn i32_at(d: &[u8], off: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(slice(d, off, 4)?.try_into().unwrap()))
}

pub fn f32_at(d: &[u8], off: usize) -> Result<f32> {
    Ok(f32::from_le_bytes(slice(d, off, 4)?.try_into().unwrap()))
}

pub fn vec3_at(d: &[u8], off: usize) -> Result<[f32; 3]> {
    Ok([f32_at(d, off)?, f32_at(d, off + 4)?, f32_at(d, off + 8)?])
}
