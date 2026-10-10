//! Per-instance effect data (overnight perf, 2026-10-08; Laurence: "frame drops when casting
//! moves", "continue overnight until all performance issues are fixed").
//!
//! A profiler run (Tracy 0.14, `PD_FRAME_STATS` spike logs) showed what a cast costs: every effect
//! piece had its OWN material, and `animate_pieces` rewrote it whenever its colour, flipbook cell,
//! scroll, roll, alpha or quadrant alpha changed (most pieces, most frames). Each rewritten
//! material is re-prepared by the render thread (a new uniform buffer and a new bind group), and a
//! new cast made up to ~250 at once: 20-60 ms `prepare-assets` / `prep-bind-groups` frames while
//! moves were being cast, and nothing at all while nobody cast (the clean runs).
//!
//! Now a piece's animated values live in ONE storage buffer shared by every effect material (binding
//! 110 of `FxGamma`), one row per live piece. The piece's mesh carries a `MeshTag` with its row;
//! `fx_gamma.wgsl` reads the row when the tag is not 0 and the material's own values otherwise
//! (row 0 is never handed out, so every draw that is not a tagged piece -- trails, blades, flares,
//! points, puffs, MSPARTICLE, light decals, aura wisps, the warm-up -- draws exactly as before).
//! A tagged piece's material holds only what never changes after spawn (textures, blend, alpha
//! mode, cull, shader family, billboard, premultiply, scene blend): it is shared by every piece
//! with the same static look (`FxMatCache::shared`) and never rewritten, so a cast prepares no
//! material it has drawn before and an animating piece costs one row of a buffer written in place.
//!
//! The row holds exactly the fields `PieceWrite` used to write into the material, in the units the
//! material held them (the colour as the linear RGBA `StandardMaterial` uploads, the plain UV
//! transform as scale + offset), so the shader computes the same numbers.
//!
//! `PD_FX_INST=0` restores the per-piece materials. When every row is taken, a new piece falls back
//! to its own pooled material (the old path) instead of failing.

use super::*;
use bevy::render::storage::ShaderBuffer;

/// Rows in the buffer (row 0 = "not instanced"). 128 bytes each: 256 KB, written in place.
pub const CAP: usize = 2048;

/// `PD_FX_INST=0`: every piece gets its own animated material again (the old path). Read once.
pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("PD_FX_INST").map_or(true, |v| v != "0"))
}

static BUFFER: std::sync::OnceLock<Handle<ShaderBuffer>> = std::sync::OnceLock::new();

/// The shared row buffer every effect material binds (a default handle before `plugin` ran: tests).
pub fn buffer() -> Handle<ShaderBuffer> {
    BUFFER.get().cloned().unwrap_or_default()
}

/// One piece's animated values: `FxGamma`'s vec4s plus the `StandardMaterial` colour and UV
/// transform. Must match `FxInst` in `fx_gamma.wgsl` (eight vec4s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxInst {
    /// `StandardMaterial::base_color` as the material uploads it (linear RGBA).
    pub color: Vec4,
    /// `StandardMaterial::uv_transform` of a plain piece: xy = scale, zw = offset.
    pub uv: Vec4,
    pub piece: Vec4,
    pub uv0: Vec4,
    pub uv1: Vec4,
    pub extra: Vec4,
    pub quad: Vec4,
    pub params: Vec4,
}

pub const FLOATS: usize = 32;

impl FxInst {
    /// The values a freshly made piece material holds.
    pub fn of(m: &FxMaterial) -> FxInst {
        let x = &m.extension;
        let c = LinearRgba::from(m.base.base_color);
        let t = m.base.uv_transform;
        FxInst {
            color: Vec4::new(c.red, c.green, c.blue, c.alpha),
            uv: Vec4::new(t.matrix2.x_axis.x, t.matrix2.y_axis.y, t.translation.x, t.translation.y),
            piece: x.piece,
            uv0: x.uv0,
            uv1: x.uv1,
            extra: x.extra,
            quad: x.quad,
            params: x.params,
        }
    }

    /// This frame's `PieceWrite`, as `PieceWrite::apply` puts it into a material. Returns whether
    /// anything changed.
    pub(super) fn apply(&mut self, w: &PieceWrite) -> bool {
        let before = *self;
        if let Some(q) = w.quad {
            self.quad = q;
        }
        if let Some((ox, oy)) = w.origin {
            self.params.z = ox;
            self.params.w = oy;
        }
        self.piece.y = w.roll;
        self.piece.w = w.alpha;
        if let Some((bump, normal)) = w.dist {
            self.extra.z = bump;
            self.extra.w = normal;
        }
        match w.uv {
            PieceUv::None => {}
            PieceUv::Plain(t) => self.uv = Vec4::new(t.matrix2.x_axis.x, t.matrix2.y_axis.y, t.translation.x, t.translation.y),
            PieceUv::Family { uv0, uv1, cross } => {
                self.uv0 = uv0;
                self.uv1 = uv1;
                self.extra.x = cross;
            }
        }
        let c = LinearRgba::from(w.color);
        self.color = Vec4::new(c.red, c.green, c.blue, c.alpha);
        *self != before
    }

    fn write(&self, out: &mut [f32]) {
        for (k, v) in [self.color, self.uv, self.piece, self.uv0, self.uv1, self.extra, self.quad, self.params].iter().enumerate() {
            out[k * 4..k * 4 + 4].copy_from_slice(&v.to_array());
        }
    }
}

/// The material a tagged piece shares: `m` with every animated value set to a fixed neutral one,
/// so pieces that differ only in what they animate share one asset (and one bind group).
pub fn neutral(mut m: FxMaterial) -> FxMaterial {
    m.base.base_color = Color::WHITE;
    m.base.uv_transform = bevy::math::Affine2::IDENTITY;
    let x = &mut m.extension;
    x.piece.y = 0.0;
    x.piece.w = 1.0;
    x.uv0 = Vec4::new(1.0, 1.0, 0.0, 0.0);
    x.uv1 = Vec4::new(1.0, 1.0, 0.0, 0.0);
    x.extra.x = 0.0;
    x.extra.z = 0.0;
    x.extra.w = 0.0;
    x.quad = Vec4::ONE;
    x.params.z = 1.0;
    x.params.w = 1.0;
    m
}

/// On the entity whose despawn ends a row's use (a piece node, a trail, a spark ...): the row goes back
/// on the free list when this component goes (`release_rows`).
#[derive(Component, Clone, Copy, Debug)]
pub struct FxRow(#[allow(dead_code)] pub u32);

/// A material its user animates every frame. With rows on and one free: the shared neutral material and
/// a row holding `fresh`'s values (the caller puts `FxRow` on `owner` and a `MeshTag` on the mesh, see
/// `tag`); otherwise `fresh` as a pooled material of its own (the old path).
/// `kind` names the site for `PD_FX_INST_SKIP=piece,trail,blade,light,point,puff,ms` (bisecting: those
/// kinds keep their own materials).
pub fn lend(rows: Option<&mut FxInstances>, cache: Option<&mut FxMatCache>, mats: &mut Assets<FxMaterial>, owner: Entity, fresh: FxMaterial, kind: &str) -> (Handle<FxMaterial>, Option<u32>) {
    let skipped = env_once("PD_FX_INST_SKIP").is_some_and(|v| v.split(',').any(|k| k.trim() == kind));
    match rows.filter(|_| enabled() && !skipped).and_then(|r| r.take(owner, FxInst::of(&fresh))) {
        Some(r) => (super::mat_cache::shared(cache, mats, neutral(fresh)), Some(r)),
        None => (super::mat_cache::pooled(cache, mats, fresh), None),
    }
}

/// Marks `owner` as holding row `row` and points the mesh entity `mesh` (often the same entity) at it.
pub fn tag(commands: &mut Commands, owner: Entity, mesh: Entity, row: Option<u32>) {
    if let Some(r) = row {
        commands.entity(owner).insert(FxRow(r));
        commands.entity(mesh).insert(bevy::mesh::MeshTag(r));
    }
}

fn lin(c: Color) -> Vec4 {
    let l = LinearRgba::from(c);
    Vec4::new(l.red, l.green, l.blue, l.alpha)
}

/// The rows: CPU copy, free list and owners.
#[derive(Resource)]
pub struct FxInstances {
    rows: Vec<FxInst>,
    free: Vec<u32>,
    owner: HashMap<Entity, u32>,
    dirty: bool,
    /// Most rows in use at once, and pieces that found no free row (`[fx-churn]`).
    pub high: usize,
    pub overflow: u64,
}

impl Default for FxInstances {
    fn default() -> Self {
        let zero = FxInst { color: Vec4::ONE, uv: Vec4::new(1.0, 1.0, 0.0, 0.0), piece: Vec4::ZERO, uv0: Vec4::ZERO, uv1: Vec4::ZERO, extra: Vec4::ZERO, quad: Vec4::ONE, params: Vec4::ZERO };
        FxInstances { rows: vec![zero; CAP], free: (1..CAP as u32).rev().collect(), owner: HashMap::new(), dirty: true, high: 0, overflow: 0 }
    }
}

impl FxInstances {
    /// A row for piece entity `e` holding `v`; None when every row is taken.
    pub fn take(&mut self, e: Entity, v: FxInst) -> Option<u32> {
        if let Some(&r) = self.owner.get(&e) {
            self.rows[r as usize] = v;
            self.dirty = true;
            return Some(r);
        }
        let Some(r) = self.free.pop() else {
            self.overflow += 1;
            return None;
        };
        self.rows[r as usize] = v;
        self.owner.insert(e, r);
        self.dirty = true;
        self.high = self.high.max(self.owner.len());
        Some(r)
    }

    /// Row `r`'s values (to animate in place).
    pub fn row_mut(&mut self, r: u32) -> &mut FxInst {
        &mut self.rows[r as usize]
    }

    pub fn mark(&mut self) {
        self.dirty = true;
    }

    /// Row `r`'s colour (what a material's `base_color` was set to).
    pub fn set_color(&mut self, r: u32, c: Color) {
        let v = lin(c);
        let row = &mut self.rows[r as usize];
        if row.color != v {
            row.color = v;
            self.dirty = true;
        }
    }

    /// Row `r`'s plain UV transform (what a material's `uv_transform` was set to; scale + offset only).
    pub fn set_uv(&mut self, r: u32, t: bevy::math::Affine2) {
        let v = Vec4::new(t.matrix2.x_axis.x, t.matrix2.y_axis.y, t.translation.x, t.translation.y);
        let row = &mut self.rows[r as usize];
        if row.uv != v {
            row.uv = v;
            self.dirty = true;
        }
    }

    /// Piece entity `e` is gone: its row is free again.
    pub fn release(&mut self, e: Entity) {
        if let Some(r) = self.owner.remove(&e) {
            self.free.push(r);
        }
    }

    pub fn live(&self) -> usize {
        self.owner.len()
    }

    /// The whole buffer as the GPU reads it (std430: eight vec4s per row, no padding).
    pub fn bytes(&self) -> Vec<u8> {
        let mut f = vec![0f32; CAP * FLOATS];
        for (k, r) in self.rows.iter().enumerate() {
            r.write(&mut f[k * FLOATS..(k + 1) * FLOATS]);
        }
        f.iter().flat_map(|x| x.to_le_bytes()).collect()
    }
}

/// Makes the shared buffer (before any effect material exists) and the systems that keep it.
pub fn plugin(app: &mut App) {
    app.init_resource::<FxInstances>();
    if let Some(mut buffers) = app.world_mut().get_resource_mut::<Assets<ShaderBuffer>>() {
        let rows = FxInstances::default();
        let h = buffers.add(ShaderBuffer::new(&rows.bytes(), bevy::asset::RenderAssetUsages::default()));
        let _ = BUFFER.set(h);
    }
    app.add_systems(Update, release_rows.before(attach_pieces));
    // After every writer (pieces and trails in `Update`; blades, light decals and aura wisp bodies in
    // `PostUpdate` after the transforms) and before the frame's asset events go out, so the rows reach the GPU this frame.
    app.add_systems(PostUpdate, upload.after(update_blades).after(update_lights).after(super::aura_wisp::update).before(bevy::asset::AssetEventSystems));
}

/// Rows of despawned owners go back on the free list.
fn release_rows(mut gone: RemovedComponents<FxRow>, mut rows: ResMut<FxInstances>) {
    for e in gone.read() {
        rows.release(e);
    }
}

/// When a row changed this frame: ONE in-place write of the whole buffer (same size, `COPY_DST`).
fn upload(mut rows: ResMut<FxInstances>, buffers: Option<ResMut<Assets<ShaderBuffer>>>) {
    if !rows.dirty {
        return;
    }
    let (Some(h), Some(mut buffers)) = (BUFFER.get(), buffers) else { return };
    let bytes = rows.bytes();
    if let Some(mut b) = buffers.get_mut(h) {
        b.data = Some(bytes);
        rows.dirty = false;
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FxMaterial {
        let mut m = fx_material(StandardMaterial { base_color: Color::srgba(0.3, 0.6, 0.9, 0.5), uv_transform: bevy::math::Affine2::from_scale_angle_translation(Vec2::new(0.25, 0.5), 0.0, Vec2::new(0.75, 0.5)), ..default() });
        m.extension.piece = Vec4::new(1.0, 0.3, 1.0, 0.7);
        m.extension.uv0 = Vec4::new(0.5, 0.5, 0.25, 0.0);
        m.extension.extra = Vec4::new(0.4, 1.0, 0.1, 0.2);
        m.extension.quad = Vec4::new(1.0, 0.5, 1.0, 0.25);
        m
    }

    /// A row holds what the material held, and a `PieceWrite` changes the row exactly as it changed
    /// the material (the shader then reads the same numbers from either).
    #[test]
    fn a_row_follows_the_material_it_replaces() {
        let mut m = sample();
        let mut row = FxInst::of(&m);
        let w = PieceWrite {
            quad: Some(Vec4::new(0.2, 1.0, 1.0, 1.0)),
            origin: Some((10.0, 20.0)),
            roll: 1.25,
            alpha: 0.4,
            dist: Some((0.3, 0.6)),
            uv: PieceUv::Plain(bevy::math::Affine2::from_scale_angle_translation(Vec2::new(0.5, 0.25), 0.0, Vec2::new(0.5, 0.75))),
            color: Color::srgba(1.0, 0.2, 0.1, 0.8),
        };
        assert!(row.apply(&w));
        w.apply(&mut m);
        assert_eq!(row, FxInst::of(&m));
        assert!(!row.apply(&w), "the same write again changes nothing");
        let w2 = PieceWrite { uv: PieceUv::Family { uv0: Vec4::new(0.1, 0.2, 0.3, 0.4), uv1: Vec4::new(0.5, 0.6, 0.7, 0.8), cross: 0.9 }, ..w };
        row.apply(&w2);
        w2.apply(&mut m);
        assert_eq!(row, FxInst::of(&m));
    }

    /// The shared material keeps every static value and only the animated ones are made neutral, so
    /// two pieces that differ only in animated values share it.
    #[test]
    fn the_neutral_material_keeps_the_static_look() {
        let m = sample();
        let n = neutral(m.clone());
        assert_eq!(n.extension.piece.x, m.extension.piece.x, "billboard");
        assert_eq!(n.extension.piece.z, m.extension.piece.z, "premultiply");
        assert_eq!(n.extension.extra.y, m.extension.extra.y, "family");
        assert_eq!(n.extension.params.x, m.extension.params.x);
        assert_eq!(n.extension.params.y, m.extension.params.y);
        assert_eq!(format!("{:?}", neutral(sample())), format!("{:?}", n));
        let mut other = sample();
        other.base.base_color = Color::srgb(0.0, 1.0, 0.0);
        other.extension.uv1 = Vec4::splat(3.0);
        other.extension.quad = Vec4::splat(0.5);
        assert_eq!(format!("{:?}", neutral(other)), format!("{:?}", n), "animated values never split the shared material");
        let mut fam = sample();
        fam.extension.extra.y = 2.0;
        assert_ne!(format!("{:?}", neutral(fam)), format!("{:?}", n), "a static value does");
    }

    /// Rows are handed out once, never row 0, come back when the piece goes, and overflow cleanly.
    #[test]
    fn rows_are_lent_and_returned() {
        let mut rows = FxInstances::default();
        let v = FxInst::of(&sample());
        let a = rows.take(Entity::from_bits(1 << 32 | 1), v).unwrap();
        let b = rows.take(Entity::from_bits(1 << 32 | 2), v).unwrap();
        assert!(a != 0 && b != 0 && a != b);
        assert_eq!(rows.take(Entity::from_bits(1 << 32 | 1), v), Some(a), "the same piece keeps its row");
        rows.release(Entity::from_bits(1 << 32 | 1));
        assert_eq!(rows.live(), 1);
        for k in 0..CAP as u64 {
            let _ = rows.take(Entity::from_bits(2 << 32 | (k + 10)), v);
        }
        assert_eq!(rows.live(), CAP - 1, "row 0 is never lent");
        assert!(rows.overflow > 0);
        assert_eq!(rows.bytes().len(), CAP * FLOATS * 4);
    }

    /// The byte layout the shader reads: eight vec4s in field order.
    #[test]
    fn the_row_layout_matches_the_shader() {
        let mut rows = FxInstances::default();
        let v = FxInst { color: Vec4::splat(1.0), uv: Vec4::splat(2.0), piece: Vec4::splat(3.0), uv0: Vec4::splat(4.0), uv1: Vec4::splat(5.0), extra: Vec4::splat(6.0), quad: Vec4::splat(7.0), params: Vec4::splat(8.0) };
        let r = rows.take(Entity::from_bits(1 << 32 | 5), v).unwrap() as usize;
        let b = rows.bytes();
        let f = |i: usize| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
        for k in 0..8 {
            assert_eq!(f(r * FLOATS + k * 4), (k + 1) as f32);
        }
        let wgsl = include_str!("../fx_gamma.wgsl").replace("\r", "");
        assert!(wgsl.contains("struct FxInst {\n    color: vec4<f32>,\n    uv: vec4<f32>,\n    piece: vec4<f32>,\n    uv0: vec4<f32>,\n    uv1: vec4<f32>,\n    extra: vec4<f32>,\n    quad: vec4<f32>,\n    params: vec4<f32>,\n}"));
    }
}
