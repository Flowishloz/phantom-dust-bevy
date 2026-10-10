//! ALR: Phantom Dust's container for models, textures, skeletons and animations.
//!
//! An ALR is a run of chunks (`u32 id`, `i32 size`, payload) followed by a
//! "resource buffer" holding raw vertex and texture data. The header chunk
//! (0x11) records where that buffer starts. Models appear as groups:
//! material (0x1), skeleton/transforms (0x3), vertex buffer table (0x16),
//! optional 0x13, then one or more index buffers (0x2).

use crate::bytes::*;
use anyhow::{bail, Context, Result};

pub const ID_MATERIAL: u32 = 0x1;
pub const ID_INDICES: u32 = 0x2;
pub const ID_SKELETON: u32 = 0x3;
pub const ID_HEADER: u32 = 0x11;
pub const ID_TEXTURE: u32 = 0x15;
pub const ID_VERTBUF: u32 = 0x16;

/// Skinned/palette vertex format used by animated character body parts
/// ("SWBOS": position + UV + two blend weights + two shader-register slots;
/// see `character.rs`'s module doc). 24-byte layout: pos f32x3 @0, uv i16x2
/// @12 (/32767), weight bytes @17/18 (BGRA-style, second-then-first), regs
/// u16x2 @20/22 (shader constant 120 + 3k = palette slot k).
pub const VERTFMT_CHAR_SKIN: u8 = 0x11;

/// Skinned/palette vertex format found on a handful of effect pieces
/// (`eft0600`'s "aura" glow, docs/CAPSULE-VISUALS.md "Probe results
/// 2026-09-28" lead 1): byte-identical layout to [`VERTFMT_CHAR_SKIN`]
/// (confirmed with `pd_convert drawinfo`/`joints` on eft0600.alr: same
/// weight/register offsets, a 7-entry `palette`), but flagged with a
/// different format id — probably the effect-side variant of the same
/// skinning vertex shader input. Before this was recognised, `read_vertices`
/// fell through to the untextured default (stride 12, no UV), and the glTF
/// exporter dropped every draw using it (no UV -> treated as
/// collision/helper geometry and skipped unless untextured export is asked
/// for) even though the geometry, indices and texture all existed.
pub const VERTFMT_EFFECT_SKIN: u8 = 0x20;

/// One vertex of a skinned/palette draw (`VERTFMT_CHAR_SKIN` or
/// `VERTFMT_EFFECT_SKIN`): position, UV, and up to two weighted blend
/// joints (via `palette_joint`). Shared by `character.rs` (per-vertex
/// dominant-joint sampling) and `gltf.rs` (full weighted-blend export of
/// effect pieces that use this format).
pub struct SkinVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    /// Blend weights for `regs[0]`/`regs[1]` (0..1, from the raw bytes /255).
    pub weights: [f32; 2],
    /// Shader constant registers naming this vertex's up-to-two palette
    /// slots; resolve with `palette_joint`.
    pub regs: [u16; 2],
    /// Byte 16 (BLENDWEIGHT.z in `SWBOS_vs`): 255 selects the mirrored
    /// light-axis table row (`cbase[95]` = (-1,1,-1), else `cbase[94]` =
    /// (1,1,-1)); the left-side parts of a character use it.
    pub mirror: bool,
    /// Byte 19 (BLENDWEIGHT.w): the per-vertex specular weight, 0..1
    /// (`SWBOS_ps`: `COLOR0.w` x normal-map alpha x LUT alpha).
    pub spec_weight: f32,
}

/// Decode a skinned/palette vertex buffer (`VERTFMT_CHAR_SKIN` /
/// `VERTFMT_EFFECT_SKIN`) into its raw fields (position, UV, blend weights
/// and register slots), without resolving the palette yet.
pub fn read_skin_vertices(a: &Alr, vb: &VertBufEntry) -> Result<Vec<SkinVertex>> {
    let stride = vb.vertex_size as usize;
    if stride < 24 {
        bail!("skinned vertex stride {stride} too small");
    }
    let raw = a.resource(vb.data_ptr, stride * vb.vertex_count as usize)?;
    Ok(raw
        .chunks_exact(stride)
        .map(|v| {
            let f = |o: usize| f32::from_le_bytes(v[o..o + 4].try_into().unwrap());
            let s = |o: usize| i16::from_le_bytes([v[o], v[o + 1]]) as f32 / i16::MAX as f32;
            SkinVertex {
                pos: [f(0), f(4), f(8)],
                uv: [s(12), s(14)],
                weights: [v[18] as f32 / 255.0, v[17] as f32 / 255.0],
                regs: [u16::from_le_bytes([v[20], v[21]]), u16::from_le_bytes([v[22], v[23]])],
                mirror: v[16] == 255,
                spec_weight: v[19] as f32 / 255.0,
            }
        })
        .collect())
}

/// Shader constant register -> bone palette slot -> joint index, for a
/// skinned/palette draw's `IndexBuffer::palette` (slot k lives at register
/// `PALETTE_BASE_REGISTER + 3k`).
pub fn palette_joint(palette: &[u8], reg: u16) -> Option<u16> {
    let off = reg.checked_sub(PALETTE_BASE_REGISTER)?;
    if off % 3 != 0 {
        return None;
    }
    palette.get((off / 3) as usize).map(|&j| j as u16)
}

#[derive(Debug, Clone, Copy)]
pub struct Chunk {
    pub id: u32,
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    R8,
    A8,
    Bgra5551,
    Bgra4444,
    Bgr565,
    Rgba8,
    Dxt1,
    Dxt3,
    Dxt5,
    Unknown(u8),
}

impl PixelFormat {
    fn from_raw(v: u8) -> Self {
        match v {
            0x00 | 0x13 => Self::R8,
            0x1F => Self::A8,
            0x02 => Self::Bgra5551,
            0x04 => Self::Bgra4444,
            0x05 => Self::Bgr565,
            0x06 | 0x12 => Self::Rgba8,
            0x0C => Self::Dxt1,
            0x0E => Self::Dxt3,
            0x0F => Self::Dxt5,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextureEntry {
    /// Offset into the resource buffer.
    pub data_ptr: u32,
    pub is_cubemap: bool,
    pub format: PixelFormat,
    pub mip_count: u8,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct VertBufEntry {
    pub format: u8,
    pub vertex_size: u8,
    pub vertex_count: u32,
    pub data_ptr: u32,
}

#[derive(Debug, Clone)]
pub struct MaterialEntry {
    pub vertbuf_format: u8,
    pub texture_idx: u16,
    /// Second texture: object-space normal map on characters (SWBOS);
    /// lightmap on LBTS stage formats.
    pub normal_idx: u16,
    /// Third and fourth texture slots (+0x1C/+0x1E, right after
    /// `texture_idx`/`normal_idx` at +0x18/+0x1A -- a 4-entry texture list,
    /// not just two independent fields). Confirmed 2026-09-28 on the idle
    /// orb's own 4-texture unlit materials (docs/CAPSULE-VISUALS.md "Other
    /// kinds" table): Status's material reads exactly `18, 19, 19, 20` and
    /// Environment's exactly `25, 26, 26, 27` across these four fields, byte
    /// for byte. 0 (or unused) when a material only needs fewer than four.
    pub texture_idx2: u16,
    pub texture_idx3: u16,
    /// Render flags (u16 at +0x0C) ❓ mostly: 0x01 always, 0x08 textured,
    /// 0x04 specular (colour at +0x34), 0x20 bump-mapped, 0x10 extra
    /// blended layer (the sky model's inner glow shell). Read from which
    /// materials carry which bits (FORMATS-ALR.md "Stage lighting").
    pub flags: u16,
    /// Bytes at +0x34..+0x37: specular colour as B, G, R plus a fourth
    /// byte. The program reads them for flag 0x04 (`FUN_1402a75e0`: R=+0x36,
    /// G=+0x35, B=+0x34, each /255 x the directional light colour = the
    /// character pixel shader's `cSPECULAR`).
    pub spec_bgra: [u8; 4],
    /// The signed 16-bit value at +0x40. The stage vertex shaders' per-draw
    /// "specular fade" constant is `value * 0.01 + 1` (`cEYEVEC.w`,
    /// `FUN_1402a69c0` / `FUN_1402a6b60`: `(short)(+0x40) * DAT_1402f7c74
    /// + DAT_1402f7da8`, 0.01 and 1.0); `SBTS_vs` outputs
    /// `vertex_alpha * (that - max(N.V, 0))` as the specular weight.
    pub fresnel: i16,
    /// The float at +0x3c. For a shadow-volume draw (flag 0x2000) it is the
    /// extrusion length the program hands the volume vertex shader (`cb0[4].w`,
    /// `FUN_1402a3bd0`: `*(float*)(material + 0x3c)`); st03: 15 / 100 / 120 /
    /// 150, st04: 12 / 25 / 30 / 60, st05: 60, st06: 100 / 150 units.
    pub extrude: f32,
    /// The six int16 fields at +0x3c, +0x3e, +0x40, +0x42, +0x44, +0x46: the
    /// program multiplies each by the frame counter and 1e-5 and uploads them
    /// as vertex-shader constants c90 (x,y,z,w) and c91 (x,y) (`FUN_1402a07b0`):
    /// a per-material UV scroll, three (u,v) pairs. Meaningful only for the
    /// scrolling-layer shaders (menu clouds, sky); other materials reuse the
    /// bytes for other things (+0x40 = fresnel, +0x3c = extrude).
    pub scroll: [i16; 6],
}

#[derive(Debug, Clone)]
pub struct Joint {
    pub position: [f32; 3],
    /// Euler X/Y/Z in radians.
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub parent: i16,
    /// Name as a base-40 packed string (`joint_name`), 0xFFFFFFFF = unnamed.
    pub name: u32,
    /// u16 at +0x24. The program's runtime flags word (PROGRAM-RE.md
    /// "Character parts / cloth"): 0x100 = takes part in the cloth / attach
    /// system, 0x400 = hidden at spawn (the program sets 0x800 = "don't draw
    /// draws on this joint"), 0x200 = billboard. Other bits ❓.
    pub flags: u16,
    /// f32 × 3 at +0x30. Cloth nodes (`CLrc` joints): [velocity kept per
    /// tick, angle limit in degrees, unused]; all zero = defaults 0.9 / 60°.
    pub params: [f32; 3],
}

/// Decode a joint name: the u32 is a base-40 number whose digits index
/// " 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_" (charset at 0x1402f7670 in the
/// program), most significant first; trailing blanks are dropped. E.g.
/// 0x70320580 = "HEAD", 0x52CD4A68 = "CLP00". None for 0xFFFFFFFF.
pub fn joint_name(v: u32) -> Option<String> {
    const CS: &[u8; 40] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_\0\0";
    if v == u32::MAX {
        return None;
    }
    let mut s = Vec::new();
    let mut h = v;
    while h != 0 {
        s.push(CS[(h % 40) as usize]);
        h /= 40;
    }
    s.reverse();
    Some(String::from_utf8_lossy(&s).trim().to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    List,
    Strip,
}

#[derive(Debug, Clone)]
pub struct IndexBuffer {
    pub material_idx: u16,
    pub transform_idx: u16,
    pub vertex_buf: u16,
    pub primitive: Primitive,
    pub indices: Vec<u16>,
    /// Bone palette for skinned (SWBOS) draws: slot k -> joint index. Slot k
    /// is shader constant register 120 + 3k, which is what vertex blend
    /// indices store. Stored after the indices as: pad to 4, u32 0, u32
    /// count, count x u8 joint, pad, count x identity 4x4 placeholders.
    pub palette: Vec<u8>,
}

/// First bone-palette register in the skinning shader (SWBOS_vs).
pub const PALETTE_BASE_REGISTER: u16 = 120;

/// One model group: its own materials, transforms, vertex buffers and draws.
#[derive(Debug, Clone, Default)]
pub struct Model {
    pub materials: Vec<MaterialEntry>,
    pub joints: Vec<Joint>,
    pub has_skeleton: bool,
    pub vertbufs: Vec<VertBufEntry>,
    pub draws: Vec<IndexBuffer>,
}

pub struct Alr {
    pub data: Vec<u8>,
    pub version: u16,
    pub resbuf_offset: usize,
    pub chunks: Vec<Chunk>,
    pub textures: Vec<TextureEntry>,
    pub models: Vec<Model>,
}

/// A vertex reduced to what we currently understand.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdVertex {
    pub pos: [f32; 3],
    pub uv: Option<[f32; 2]>,
}

/// Vertex layouts by format id: (stride, uv offset, uv divisor).
/// Position is always 3 floats at offset 0. `None` uv means position only.
fn vertex_layout(format: u8) -> (usize, Option<(usize, f32)>) {
    match format {
        0x01 => (0x18, Some((16, 4096.0))),
        0x03 => (0x20, Some((16, 4096.0))),
        0x05 => (0x1C, Some((16, i16::MAX as f32))),
        0x07 => (0x10, None),
        0x08 => (0x14, None),
        0x09 => (0x14, Some((12, 4096.0))),
        0x0B => (0x10, None),
        0x10 => (0x0C, None),
        0x11 => (0x18, Some((12, i16::MAX as f32))),
        // Effect-side skinned/palette format (VERTFMT_EFFECT_SKIN): same
        // physical layout as 0x11 (confirmed on eft0600.alr).
        0x20 => (0x18, Some((12, i16::MAX as f32))),
        0x15 => (0x18, Some((12, i16::MAX as f32))),
        0x16 => (0x1C, Some((12, 4096.0))),
        0x17 => (0x20, Some((16, 4096.0))),
        0x18 => (0x10, None),
        0x1A => (0x28, Some((16, 4096.0))),
        0x1D => (0x24, Some((16, 4096.0))),
        0x1E => (0x1C, Some((16, 4096.0))),
        0x1F => (0x20, Some((16, 4096.0))),
        0x21 => (0x20, Some((16, 4096.0))),
        0x25 => (0x1C, Some((16, 4096.0))),
        _ => (12, None),
    }
}

/// A vertex's lighting inputs: normal (game left-handed space, model local)
/// and colour (RGBA, from the file's BGRA bytes).
#[derive(Debug, Clone, Copy, Default)]
pub struct VertexLight {
    pub normal: Option<[f32; 3]>,
    pub color: Option<[u8; 4]>,
}

/// Byte offsets of the packed normal and the vertex colour, by format id.
/// Established 2026-09-27 from the game's stage shaders (input signatures of
/// `SD_vs` = position/normal/colour/uv, `SBTS_vs` = + tangent/binormal,
/// `SVC_vs` = position/colour, `SC_vs` = position/colour/uv) matched to the
/// strides, and checked on data: see FORMATS-ALR.md "Stage lighting".
pub fn light_layout(format: u8) -> (Option<usize>, Option<usize>) {
    match format {
        0x01 | 0x03 => (Some(12), Some(20)),
        0x07 => (Some(12), None),
        0x09 => (None, Some(16)),
        0x0B => (None, Some(12)),
        // Point-sprite format (st00.alr capsule sparkle/burst pieces;
        // docs/CAPSULE-VISUALS.md "Sparkles, pickup and erase bursts"):
        // position (12 bytes) then a D3DCOLOR at +12, no UV. Reusing this
        // table lets `read_vertex_light` hand back each point's colour.
        0x18 => (None, Some(12)),
        _ => (None, None),
    }
}

/// Packed normal: x 11 bits, y 11 bits, z 10 bits, each signed and scaled to
/// ±1 (x, y / 1023, z / 511). 94 % of st03's triangles have their face
/// normal within 25° of this (the rest are smooth-shaded).
pub fn unpack_normal(u: u32) -> [f32; 3] {
    let s = |v: u32, bits: u32| -> f32 {
        let m = 1u32 << (bits - 1);
        if v & m != 0 {
            v as f32 - (1u32 << bits) as f32
        } else {
            v as f32
        }
    };
    [s(u & 0x7FF, 11) / 1023.0, s((u >> 11) & 0x7FF, 11) / 1023.0, s((u >> 22) & 0x3FF, 10) / 511.0]
}

impl Alr {
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(data)
    }

    pub fn parse(data: Vec<u8>) -> Result<Self> {
        if u32_at(&data, 0)? != ID_HEADER {
            bail!("not an ALR (first chunk is not 0x11)");
        }
        let version = u16_at(&data, 8)?;
        let resbuf_offset = u32_at(&data, 12)? as usize;
        let chunk_end = resbuf_offset.min(data.len());

        // Walk the chunk list up to the resource buffer.
        let mut chunks = Vec::new();
        let mut pos = 0usize;
        let mut prev_id = u32::MAX;
        while pos + 8 <= chunk_end {
            let id = u32_at(&data, pos)?;
            let size = i32_at(&data, pos + 4)?;
            if id == 0 && prev_id == 0 {
                break;
            }
            if size < 8 {
                break;
            }
            chunks.push(Chunk { id, offset: pos, size: size as usize });
            pos += size as usize;
            prev_id = id;
        }

        let mut alr = Alr { data, version, resbuf_offset, chunks, textures: Vec::new(), models: Vec::new() };
        alr.read_textures()?;
        alr.read_models()?;
        Ok(alr)
    }

    fn read_textures(&mut self) -> Result<()> {
        let Some(c) = self.chunks.iter().find(|c| c.id == ID_TEXTURE).copied() else {
            return Ok(());
        };
        let d = &self.data;
        let n = u32_at(d, c.offset + 8)? as usize;
        for i in 0..n {
            let e = c.offset + 12 + i * 0x1C;
            let data_ptr = u32_at(d, e + 4)?;
            let b12 = u8_at(d, e + 12)?;
            let b13 = u8_at(d, e + 13)?;
            let b14 = u8_at(d, e + 14)?;
            let b15 = u8_at(d, e + 15)?;
            let direct = u32_at(d, e + 16)?;
            let width_direct = direct & 0xFFF;
            let height_direct = (direct >> 12) & 0xFFF;
            // The game only falls back to power-of-two sizes when both are 0.
            let (width, height) = if width_direct == 0 && height_direct == 0 {
                (1u32 << (b14 >> 4), 1u32 << (b15 & 0xF))
            } else {
                (width_direct + 1, height_direct + 1)
            };
            self.textures.push(TextureEntry {
                data_ptr,
                is_cubemap: b12 & 0b100 != 0,
                format: PixelFormat::from_raw(b13 & 0x1F),
                mip_count: b14 & 0xF,
                width,
                height,
            });
        }
        Ok(())
    }

    fn read_models(&mut self) -> Result<()> {
        let d = &self.data;
        let mut models = Vec::new();
        let mut cur: Option<Model> = None;

        for c in &self.chunks {
            match c.id {
                ID_MATERIAL => {
                    if let Some(m) = cur.take() {
                        models.push(m);
                    }
                    let mut m = Model::default();
                    let n = u16_at(d, c.offset + 8)? as usize;
                    for i in 0..n {
                        let e = c.offset + 12 + i * 0x4C;
                        m.materials.push(MaterialEntry {
                            vertbuf_format: u8_at(d, e + 16)?,
                            texture_idx: u16_at(d, e + 24)?,
                            normal_idx: u16_at(d, e + 26)?,
                            texture_idx2: u16_at(d, e + 28)?,
                            texture_idx3: u16_at(d, e + 30)?,
                            flags: u16_at(d, e + 12)?,
                            spec_bgra: [u8_at(d, e + 0x34)?, u8_at(d, e + 0x35)?, u8_at(d, e + 0x36)?, u8_at(d, e + 0x37)?],
                            fresnel: i16_at(d, e + 0x40)?,
                            extrude: f32_at(d, e + 0x3c)?,
                            scroll: [i16_at(d, e + 0x3c)?, i16_at(d, e + 0x3e)?, i16_at(d, e + 0x40)?, i16_at(d, e + 0x42)?, i16_at(d, e + 0x44)?, i16_at(d, e + 0x46)?],
                        });
                    }
                    cur = Some(m);
                }
                ID_SKELETON => {
                    let Some(m) = cur.as_mut() else { continue };
                    let n = u16_at(d, c.offset + 8)? as usize;
                    m.has_skeleton = u16_at(d, c.offset + 10)? != 0;
                    for i in 0..n {
                        let e = c.offset + 16 + i * 0x40;
                        m.joints.push(Joint {
                            position: vec3_at(d, e)?,
                            rotation: vec3_at(d, e + 12)?,
                            scale: vec3_at(d, e + 24)?,
                            parent: i16_at(d, e + 42)?,
                            name: u32_at(d, e + 44)?,
                            flags: u16_at(d, e + 0x24)?,
                            params: vec3_at(d, e + 0x30)?,
                        });
                    }
                }
                ID_VERTBUF => {
                    let Some(m) = cur.as_mut() else { continue };
                    let n = u32_at(d, c.offset + 8)? as usize;
                    for i in 0..n {
                        let e = c.offset + 12 + i * 0x1C;
                        m.vertbufs.push(VertBufEntry {
                            format: u8_at(d, e)?,
                            vertex_size: u8_at(d, e + 1)?,
                            vertex_count: u32_at(d, e + 8)?,
                            data_ptr: u32_at(d, e + 20)?,
                        });
                    }
                }
                ID_INDICES => {
                    let Some(m) = cur.as_mut() else { continue };
                    let h = c.offset;
                    let num_indices = u32_at(d, h + 80)? as usize;
                    let avail = c.size.saturating_sub(0x68) / 2;
                    let count = num_indices.min(avail);
                    let raw = slice(d, h + 0x68, count * 2)?;
                    let indices = raw.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
                    let mut palette = Vec::new();
                    let tail = (h + 0x68 + count * 2 + 3) & !3;
                    if tail + 8 <= h + c.size {
                        let n = u32_at(d, tail + 4)? as usize;
                        if n > 0 && n <= 64 && tail + 8 + n <= h + c.size {
                            palette = slice(d, tail + 8, n)?.to_vec();
                        }
                    }
                    m.draws.push(IndexBuffer {
                        material_idx: u16_at(d, h + 64)?,
                        transform_idx: u16_at(d, h + 66)?,
                        vertex_buf: u16_at(d, h + 68)?,
                        primitive: if u16_at(d, h + 70)? == 6 { Primitive::Strip } else { Primitive::List },
                        indices,
                        palette,
                    });
                }
                _ => {}
            }
        }
        if let Some(m) = cur.take() {
            models.push(m);
        }
        self.models = models;
        Ok(())
    }

    /// Per-vertex normal and colour, where the vertex format has them (see
    /// [`light_layout`]); used to light stages the way the game's stage
    /// shaders do (FORMATS-ALR.md "Stage lighting").
    pub fn read_vertex_light(&self, vb: &VertBufEntry) -> Result<Vec<VertexLight>> {
        let (table_stride, _) = vertex_layout(vb.format);
        let stride = if vb.vertex_size != 0 { vb.vertex_size as usize } else { table_stride };
        let (n_off, c_off) = light_layout(vb.format);
        let bytes = self.resource(vb.data_ptr, stride * vb.vertex_count as usize)?;
        let mut out = Vec::with_capacity(vb.vertex_count as usize);
        for v in bytes.chunks_exact(stride) {
            let normal = match n_off {
                Some(o) if o + 4 <= stride => Some(unpack_normal(u32_at(v, o)?)),
                _ => None,
            };
            // D3DCOLOR: bytes B, G, R, A.
            let color = match c_off {
                Some(o) if o + 4 <= stride => Some([v[o + 2], v[o + 1], v[o], v[o + 3]]),
                _ => None,
            };
            out.push(VertexLight { normal, color });
        }
        Ok(out)
    }

    pub fn resource(&self, data_ptr: u32, len: usize) -> Result<&[u8]> {
        slice(&self.data, self.resbuf_offset + data_ptr as usize, len)
    }

    /// Decode every vertex of a vertex buffer into position (+uv).
    pub fn read_vertices(&self, vb: &VertBufEntry) -> Result<Vec<StdVertex>> {
        let (table_stride, uv) = vertex_layout(vb.format);
        let stride = if vb.vertex_size != 0 { vb.vertex_size as usize } else { table_stride };
        let bytes = self.resource(vb.data_ptr, stride * vb.vertex_count as usize)?;
        let mut out = Vec::with_capacity(vb.vertex_count as usize);
        for v in bytes.chunks_exact(stride) {
            let pos = vec3_at(v, 0)?;
            let uv = match uv {
                Some((off, div)) if off + 4 <= stride => {
                    Some([i16_at(v, off)? as f32 / div, i16_at(v, off + 2)? as f32 / div])
                }
                _ => None,
            };
            out.push(StdVertex { pos, uv });
        }
        Ok(out)
    }

    /// Full per-vertex data for the two idle-orb formats
    /// (docs/CAPSULE-VISUALS.md "Draw recipe (decoded 2026-09-28)"): the
    /// sphere core (format 0x1D, stride 0x24 -- pos @0, packed normal @12,
    /// uv0 @16 i16x2/4096, colour @20 BGRA, packed tangent @24, packed
    /// binormal @28, uv1 @32 i16x2/4096) and the cup/disc rim (format 0x09,
    /// stride 0x14 -- pos @0, uv @12 i16x2/4096, colour @16 BGRA). Neither
    /// `read_vertices` (pos+uv0 only) nor `read_vertex_light` (normal+colour
    /// only, no uv/tangent) carries everything the idle-orb shaders need, so
    /// this is a dedicated reader rather than extending those two shared
    /// paths (used by every stage/character/effect export and their tests).
    pub fn read_capsule_vertices(&self, vb: &VertBufEntry) -> Result<Vec<CapsuleVertex>> {
        let (table_stride, uv) = vertex_layout(vb.format);
        let stride = if vb.vertex_size != 0 { vb.vertex_size as usize } else { table_stride };
        let bytes = self.resource(vb.data_ptr, stride * vb.vertex_count as usize)?;
        let mut out = Vec::with_capacity(vb.vertex_count as usize);
        for v in bytes.chunks_exact(stride) {
            let pos = vec3_at(v, 0)?;
            let mut cv = CapsuleVertex { pos, ..Default::default() };
            if let Some((off, div)) = uv {
                if off + 4 <= stride {
                    cv.uv0 = Some([i16_at(v, off)? as f32 / div, i16_at(v, off + 2)? as f32 / div]);
                }
            }
            if vb.format == 0x1D && stride >= 36 {
                cv.normal = Some(unpack_normal(u32_at(v, 12)?));
                cv.color = Some([v[22], v[21], v[20], v[23]]);
                cv.tangent = Some(unpack_normal(u32_at(v, 24)?));
                cv.binormal = Some(unpack_normal(u32_at(v, 28)?));
                cv.uv1 = Some([i16_at(v, 32)? as f32 / 4096.0, i16_at(v, 34)? as f32 / 4096.0]);
            } else if vb.format == 0x03 && stride >= 32 {
                // The base (non-"M2") sphere-core variant (docs/CAPSULE-
                // VISUALS.md per-kind table: `vkCAP02` on Aura/Defence/
                // Special/opponent views, vs `vkCAP02M2` = "the same with
                // the bump maps on uv0" on Attack/Erase): one UV set instead
                // of two (both flame and bump samples reuse uv0, at their
                // own scroll speeds), same normal/colour/tangent/binormal
                // layout as 0x1D minus the +32 uv1 field it doesn't have.
                cv.normal = Some(unpack_normal(u32_at(v, 12)?));
                cv.color = Some([v[22], v[21], v[20], v[23]]);
                cv.tangent = Some(unpack_normal(u32_at(v, 24)?));
                cv.binormal = Some(unpack_normal(u32_at(v, 28)?));
            } else if vb.format == 0x09 && stride >= 20 {
                cv.color = Some([v[18], v[17], v[16], v[19]]);
            } else if vb.format == 0x15 && stride >= 20 {
                // The flat halo/disc layer (docs/CAPSULE-VISUALS.md "What an
                // idle orb is made of"): same 12-byte pos + 4-byte i16 uv
                // (divisor i16::MAX, not 4096 -- `vertex_layout`) as the
                // skinned/palette formats sharing this stride, but static;
                // colour offset ⚠️ assumed at +16 (BGRA), by analogy with the
                // cup format 0x09's own uv-then-colour layout -- not checked
                // against a byte dump this pass.
                cv.color = Some([v[18], v[17], v[16], v[19]]);
            } else if vb.format == 0x17 && stride >= 32 {
                // Status/Environment's unlit 4-texture core. The shared
                // `vertex_layout` table's own uv offset (+0x10, inherited
                // from the skinned/palette formats that reuse this stride)
                // is wrong *for this mesh*: a raw byte dump (`pd_convert
                // drawinfo` + `PD_RAWDUMP=1`, 2026-09-28) found +0x10..+0x13
                // is a BGRA vertex colour (three equal bytes + a 0xFF alpha,
                // forming a smooth per-ring grey gradient -- baked static
                // shading, consistent with an unlit sphere), and the real
                // UV0 sits at +0x0C instead, one field earlier. Reading the
                // colour bytes as UV was the round-3d "hard horizontal
                // stripes instead of a swirl" bug: a vertex colour that
                // steps in clean grey bands per latitude ring, misread as a
                // UV coordinate, samples the texture in equally clean
                // horizontal bands. Left un-touched: `vertex_layout`'s
                // shared table entry (also used by other, unverified 0x17
                // stage geometry) and this format's other three per-layer
                // UV fields at +0x14/+0x18/+0x1C (spot-checked on one vertex:
                // +0x14 and +0x1C matched +0x0C exactly, +0x18 differed only
                // in U -- likely per-layer UVs that mostly, but don't
                // always, coincide; not fully wired up this pass, so layers
                // 1-3 still reuse layer 0's UV0).
                cv.uv0 = Some([i16_at(v, 12)? as f32 / 4096.0, i16_at(v, 14)? as f32 / 4096.0]);
                cv.color = Some([v[18], v[17], v[16], v[19]]);
            }
            out.push(cv);
        }
        Ok(out)
    }
}

/// See [`Alr::read_capsule_vertices`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CapsuleVertex {
    pub pos: [f32; 3],
    pub normal: Option<[f32; 3]>,
    pub uv0: Option<[f32; 2]>,
    pub uv1: Option<[f32; 2]>,
    pub tangent: Option<[f32; 3]>,
    pub binormal: Option<[f32; 3]>,
    /// RGBA, converted from the file's BGRA bytes.
    pub color: Option<[u8; 4]>,
}

/// Turn an index buffer into a plain triangle list, dropping the degenerate
/// triangles strips use as separators and fixing strip winding.
pub fn triangulate(ib: &IndexBuffer) -> Vec<[u16; 3]> {
    let idx = &ib.indices;
    let mut tris = Vec::new();
    match ib.primitive {
        Primitive::List => {
            for t in idx.chunks_exact(3) {
                tris.push([t[0], t[1], t[2]]);
            }
        }
        Primitive::Strip => {
            for i in 2..idx.len() {
                let (mut a, mut b, c) = (idx[i - 2], idx[i - 1], idx[i]);
                if a == b || a == c || b == c {
                    continue;
                }
                if i % 2 != 0 {
                    std::mem::swap(&mut a, &mut b);
                }
                tris.push([a, b, c]);
            }
        }
    }
    tris
}

#[cfg(test)]
mod tests {
    use super::joint_name;

    #[test]
    fn joint_names_decode() {
        assert_eq!(joint_name(0x7032_0580).as_deref(), Some("HEAD"));
        assert_eq!(joint_name(0x52CD_4A68).as_deref(), Some("CLP00"));
        assert_eq!(joint_name(0x52C0_9868).as_deref(), Some("CLC00"));
        assert_eq!(joint_name(0x52B4_ED10).as_deref(), Some("CL021"));
        assert_eq!(joint_name(0xADB0_7630).as_deref(), Some("RHAND"));
        assert_eq!(joint_name(u32::MAX), None);
    }
}
