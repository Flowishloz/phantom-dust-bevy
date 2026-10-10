//! MSPARTICLE emitters (`90e7fd3e`), read from the program (lane fxopen,
//! 2026-10-01). Before this the puffs were guessed: words 30 / 31 were taken
//! for the sprite size and 8 / 9 for life and drift. They are the particle
//! COUNT and LIFE (as for PARTICLE01); the size is word 8 and word 9 its
//! growth per frame. At Timer Mine's x10 blast scale the guessed 15 -> 25 u
//! puffs became 150-250 u clouds that filled the screen.
//!
//! What was read (addresses in `decomp_all.c`; image base 0x140000000):
//! - Kind `...e9` = `MSParticleFunc00` (ctor `0x14010da00`, spawn
//!   `0x14010e590`, update `0x14010da90`); kind `...ea` = `MSParticleFunc01`
//!   (ctor `0x14010ed60`, spawn `0x14010f710`, update `0x14010edf0`). Both are
//!   drawn by `0x14010e260` (vtable slot 4). The kind-to-class pairing is the
//!   numbering plus the words (Func01 reads word 24 as a shape 0 / 1 / 2, the
//!   `e9` emitters hold small fractions there that Func00 reads as a speed
//!   spread); no table was found. UNCONFIRMED but consistent.
//! - Spawn, once per effect frame `start <= frame <= end`: Func01 `(int)(word
//!   30 x 0.8)` particles (at least 1), Func00 `(int)word 30`. Word 19 picks the
//!   matrix mode: 0 = the particle lives in the emitter matrix's local space
//!   (the slot keeps a copy of the matrix and the draw goes through it, so the
//!   matrix scale multiplies position AND size; Func01 also applies the
//!   matrix to the spawn offset, taken literally from the decompile: twice in
//!   all), 3 (Func01) = local space, offset untouched, 1 / 2 and the rest =
//!   world space at the matrix translation, the spawn offset through the
//!   matrix, sizes and speeds NOT scaled.
//! - Func01 spawn offset: shape `(int)word 24`: 0 = a random radius up to
//!   word 25 on a circle (x = sin, z = cos), 1 = exactly word 25, 2 = random
//!   radius with a random tilt of up to +-45 degrees about the half-angle
//!   quaternion (`dir` below, transcribed). Word 28 non-zero lerps the radius
//!   word 25 -> 27 over the window.
//! - Velocity: a random direction (word 20 = 0: a full turn of azimuth and a
//!   +-45 degree half-angle elevation; else a cone about yaw word 21 / pitch
//!   word 22 degrees, spread word 23 (Func00) or the radius word (Func01)) at
//!   speed word 7 (+ a random share of word 24 for Func00), size word 8 (+ a
//!   random share of word 25 for Func00).
//! - Per frame (the effect manager's step): position += velocity; velocity +=
//!   (words 4-6 + words 10 x the stage wind); size += word 9; colour by age.
//!   Dies when age > word 31. Colour: Func00 lerps word 12 -> 13 over the
//!   life; Func01 lerps 12 -> 13 up to age word 11 and 13 -> 14 after (word 11
//!   = 0: 12 -> 13 over the whole life).
//! - Draw (`0x14010e260`): word 16 = draw mode (0 point sprite, 1 billboard,
//!   2 line, 3 billboard line; the data has only 1, 2 and 3), word 26 = blend
//!   (0 ONE,ONE; 1 SRC_ALPHA,1-SRC_ALPHA; 2 ZERO,SRC_COLOR; 3 DST_COLOR,ONE;
//!   4 DST_COLOR,1-SRC_COLOR; the material's own blend state is overridden).
//!   Billboard (`FUN_14018d930` / `FUN_1401761a0`): a camera-facing quad of
//!   full width = the particle's size in the slot's local units. Pixel stage
//!   (`vk_billbord2_ps`, the BILL program ❓ by name only): `rgb = texture x
//!   clamp(colour rgb)`, alpha out = 1, discard where the texture's alpha is 0
//!   -- so colour alpha does nothing and blend 1 draws opaque.
//!
//! NOT built / UNCONFIRMED: the line ribbons of draw modes 2 / 3 (the head is
//! drawn as a billboard instead); the stage wind (words 10, taken as 0); whether the
//! effect manager steps these once per 30 Hz tick (assumed, like the clock).

use super::*;

/// The emitter words a MSPARTICLE needs, in the game's own (left-handed) frame.
#[derive(Clone)]
pub(super) struct MsSpec {
    /// Kind `...ea` = MSParticleFunc01 (else Func00).
    pub func01: bool,
    pub w: [f32; 32],
    /// Words 12, 13, 14 (ARGB).
    pub colors: [u32; 3],
    pub joint_m: Mat4,
}

impl MsSpec {
    pub(super) fn from_emitter(e: &serde_json::Value, joint_m: Mat4) -> Option<Self> {
        let kind = e["kind"].as_str()?;
        let func01 = kind.ends_with("ea");
        if !(func01 || kind.ends_with("e9")) {
            return None;
        }
        let mut w = [0.0f32; 32];
        for (i, v) in w.iter_mut().enumerate() {
            let x = e["raw"][i].as_u64().map_or(0.0, |x| f32::from_bits(x as u32));
            *v = if x.is_finite() { x } else { 0.0 };
        }
        let col = |i: usize| e["raw"][12 + i].as_u64().unwrap_or(0) as u32;
        Some(MsSpec { func01, w, colors: [col(0), col(1), col(2)], joint_m })
    }

    /// Particle life in effect frames (word 31).
    pub(super) fn life(&self) -> f32 {
        self.w[31].max(1.0)
    }

    fn mode(&self) -> i32 {
        self.w[19] as i32
    }

    /// Draw mode, word 16: 0 point sprite, 1 billboard, 2 line, 3 billboard line (`FUN_14010e260`).
    pub(super) fn draw_mode(&self) -> i32 {
        self.w[16] as i32
    }

    /// Nodes in a particle's chain (word 17). READ: `FUN_14010f710` (Func01) and `FUN_14010e590`
    /// (Func00) clamp it at spawn: below draw mode 2.0 (`DAT_1402f2a10`) a particle is one node;
    /// otherwise at most 5 (Func01) / 6 (Func00, `DAT_1402f7f30`). Every extra node is the
    /// particle's own position `i` effect frames ago, so modes 2 and 3 draw its recent path.
    pub(super) fn chain_len(&self) -> usize {
        let n = self.w[17] as i32;
        if (self.w[16] as f32) < 2.0 {
            return 1;
        }
        n.clamp(0, if self.func01 { 5 } else { 6 }) as usize
    }

    /// Ribbon (mode 3) half width, word 18.
    fn ribbon_width(&self) -> f32 {
        self.w[18]
    }

    /// Draw modes 2 and 3 with at least two nodes draw a ribbon / line instead of a billboard.
    pub(super) fn is_ribbon(&self) -> bool {
        matches!(self.draw_mode(), 2 | 3) && self.chain_len() >= 2
    }

    /// Blend 1 (SRC_ALPHA, 1 - SRC_ALPHA) with the colour's own alpha as the opacity (not the literal
    /// pixel shader, which writes alpha 1).
    pub(super) fn alpha_fades(&self) -> bool {
        self.w[26] as i32 == 1 && !literal_opaque()
    }

    /// Particles per spawn call.
    fn count(&self) -> usize {
        if self.func01 { ((self.w[30] * 0.8) as usize).max(1) } else { self.w[30] as usize }
    }

    /// The blend of word 26 (`FUN_14010e260`: 0x1101 / 0x5401 / 0x2001 / 0x1801 / 0x3801).
    /// `None` = no blending (alpha 1 output, so an opaque draw with an alpha kill).
    pub(super) fn blend(&self) -> Option<BlendState> {
        use BlendFactor::*;
        let (src, dst) = match self.w[26] as i32 {
            1 if literal_opaque() => return None,
            1 => (SrcAlpha, OneMinusSrcAlpha),
            2 => (Zero, SrcColor),
            3 => (DstColor, One),
            4 => (DstColor, OneMinusSrcColor),
            _ => (One, One),
        };
        Some(BlendState { src, dst, op: BlendOp::Add })
    }
}

/// `PD_MSP=0`: the old guessed puffs (A/B for sheets).
pub(super) fn enabled() -> bool {
    env_once("PD_MSP").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0) > 0.0
}

/// `PD_MSP_OPAQUE=1`: draw blend-1 puffs exactly as `vk_billbord2_ps` reads (alpha out 1 = an opaque
/// cut-out whose colour fades to black). Default off: the colour's authored alpha fades them (see
/// `MsSpec::alpha_fades` and FORMATS-EFFECTS.md "MSPARTICLE blend 1").
pub(super) fn literal_opaque() -> bool {
    env_once("PD_MSP_OPAQUE").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) > 0.0
}

const PI_F: f32 = 3.141_590_1; // DAT_1402f7ed8
const DEG: f32 = 0.017_453_292; // DAT_1402f7c84

/// The game-frame direction the spawn code builds from an azimuth (cos `a`, sin
/// `b`) and a half-angle elevation (`sin_e`, `cos_e`): transcribed from the
/// quaternion-style products in `FUN_14010f710` / `FUN_14010e590`.
fn dir(a: f32, b: f32, sin_e: f32, cos_e: f32) -> Vec3 {
    let f28 = a * sin_e;
    let f27 = b * sin_e;
    let f30 = -(b * f28) - a * f27;
    let f26 = b * f27 - a * f28;
    Vec3::new((b * cos_e * cos_e - f30 * f28) - f26 * f27, (f26 * cos_e + b * cos_e * f27) - a * cos_e * f28, a * cos_e * cos_e - (f30 * f27 - f26 * f28))
}

/// Game frame (left-handed, z forward) -> ours.
fn flip(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, -v.z)
}

/// One new particle's (position, velocity, size, attached) -- positions in the slot's
/// local space when `attached`, else in the world. `m` = the emitter's world matrix,
/// `u` = the random stream.
fn spawn_one(s: &MsSpec, m: &Mat4, frame: f32, win: [f32; 2], u: &dyn Fn(u32) -> f32) -> (Vec3, Vec3, f32, bool) {
    let w = &s.w;
    let mode = s.mode();
    let m3 = Mat3::from_mat4(*m);
    let t = m.w_axis.truncate();
    if s.func01 {
        // Offset.
        let ty = w[24] as i32;
        let rad = if w[28] == 0.0 { w[25] } else { (frame - win[0]) / (win[1] - win[0]).max(1e-6) * (w[27] - w[25]) + w[25] };
        let (r, theta, elev) = match ty {
            1 => (rad, 2.0 * u(1) * PI_F, 0.0),
            2 => (u(0) * rad, 2.0 * u(1) * PI_F, (u(2) - 0.5) * PI_F * 0.5),
            _ => (u(0) * rad, 2.0 * u(1) * PI_F, 0.0),
        };
        let o = flip(dir(theta.cos(), theta.sin(), elev.sin(), elev.cos()) * r);
        let (pos, attached) = match mode {
            0 => (m3 * o, true),
            3 => (o, true),
            _ => (m3 * o + t, false),
        };
        // Velocity.
        let (phi, e) = if w[20] as i32 == 0 {
            (2.0 * u(3) * PI_F, (u(4) - 0.5) * PI_F * 0.5)
        } else {
            ((u(3) - 0.5) * rad + w[21] * DEG, (u(4) - 0.5) * rad * 0.25 + w[22] * DEG)
        };
        let v = flip(dir(phi.cos(), phi.sin(), e.sin(), e.cos()) * w[7]);
        (pos, v, w[8], attached)
    } else {
        let (phi, e) = if w[20] as i32 == 0 {
            (2.0 * u(0) * PI_F, (u(1) - 0.5) * PI_F * 0.5)
        } else {
            ((u(0) - 0.5) * w[23] + w[21] * DEG, ((u(1) - 0.5) * w[23] * 0.5 + w[22] * DEG) * 0.5)
        };
        let speed = u(2) * w[24] + w[7];
        let v = flip(dir(phi.cos(), phi.sin(), e.sin(), e.cos()) * speed);
        let size = u(3) * w[25] + w[8];
        match mode {
            0 => (Vec3::ZERO, v, size, true),
            _ => (t, v, size, false),
        }
    }
}

/// A live particle (drawn as one camera-facing quad).
#[derive(Component)]
pub(super) struct MsPart {
    instance: Entity,
    emitter: Entity,
    attached: bool,
    pos: Vec3,
    vel: Vec3,
    acc: Vec3,
    size: f32,
    growth: f32,
    age: f32,
    life: f32,
    func01: bool,
    hold: f32,
    colors: [u32; 3],
    /// Blend 1 drawn translucent: the colour's alpha is the particle's opacity.
    alpha_fades: bool,
    material: Handle<FxMaterial>,
    /// Its row in the per-instance buffer (`effects/inst.rs`); None = it colours its own material.
    row: Option<u32>,
    /// Draw modes 2 / 3: this particle is a chain of nodes drawn as a line / ribbon.
    ribbon: Option<Ribbon>,
}

/// A particle's chain (`FUN_14010edf0` shifts it once per effect frame: each tail node takes the
/// position of the node before it and 3/4 of its colour; the head then moves).
struct Ribbon {
    /// 3 = billboard ribbon of width `width` (`FUN_14018d760`), 2 = lines (`FUN_14018d5f0`).
    mode: i32,
    width: f32,
    /// Head first: position in the particle's own space (see `MsPart::attached`) and rgb 0-1.
    nodes: Vec<(Vec3, Vec3)>,
    /// Whole effect frames this particle has lived (the shift runs once per frame).
    frames: u32,
    mesh: Handle<Mesh>,
    /// What the mesh was last written from (world nodes, width, view, root inverse), and this
    /// frame's world nodes (kept to reuse the vector): an unchanged ribbon is not rewritten
    /// (`mesh_reuse.rs`).
    drawn: Option<(Vec<(Vec3, Vec3)>, f32, Vec3, Mat4)>,
    world: Vec<(Vec3, Vec3)>,
}

/// The colour at `age` (0-1 channels, alpha ignored by the pixel shader).
fn ramp(p: &MsPart) -> Vec3 {
    let ch = |c: u32| Vec3::new(((c >> 16) & 0xFF) as f32, ((c >> 8) & 0xFF) as f32, (c & 0xFF) as f32);
    let (a, b, c) = (ch(p.colors[0]), ch(p.colors[1]), ch(p.colors[2]));
    let col = if p.func01 && p.hold != 0.0 {
        if p.age >= p.hold { b.lerp(c, (p.age - p.hold) / (p.life - p.hold).max(1e-6)) } else { a.lerp(b, p.age / p.hold) }
    } else {
        a.lerp(b, p.age / p.life.max(1e-6))
    };
    (col / 255.0).clamp(Vec3::ZERO, Vec3::ONE)
}

/// The colour's alpha at `age` (0-1), same curve as `ramp` over the alpha byte.
fn ramp_alpha(p: &MsPart) -> f32 {
    let a = |c: u32| ((c >> 24) & 0xFF) as f32;
    let (a0, a1, a2) = (a(p.colors[0]), a(p.colors[1]), a(p.colors[2]));
    let v = if p.func01 && p.hold != 0.0 {
        if p.age >= p.hold { a1 + (a2 - a1) * ((p.age - p.hold) / (p.life - p.hold).max(1e-6)).clamp(0.0, 1.0) } else { a0 + (a1 - a0) * (p.age / p.hold).clamp(0.0, 1.0) }
    } else {
        a0 + (a1 - a0) * (p.age / p.life.max(1e-6)).clamp(0.0, 1.0)
    };
    (v / 255.0).clamp(0.0, 1.0)
}

/// Where the particle's quad sits: world position and edge length.
fn place(p: &MsPart, slot: &Mat4) -> (Vec3, f32) {
    if p.attached {
        (slot.transform_point3(p.pos), p.size.max(0.0) * slot.x_axis.truncate().length())
    } else {
        (p.pos, p.size.max(0.0))
    }
}

fn local_transform(root: &GlobalTransform, world: Vec3, edge: f32) -> Transform {
    let m = Mat4::from(root.affine().inverse()) * Mat4::from_scale_rotation_translation(Vec3::splat(edge.max(1e-4)), Quat::IDENTITY, world);
    Transform::from_matrix(m)
}

/// Spawn this frame's particles (`FxEmitter` is ticked by `tick_sprites`).
#[allow(clippy::too_many_arguments)]
pub(super) fn emit(
    commands: &mut Commands,
    em: &mut FxEmitter,
    em_entity: Entity,
    inst: &FxInstance,
    root: &GlobalTransform,
    s: &MsSpec,
    m: Mat4,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<FxMaterial>,
    mesh: &mut Option<Handle<Mesh>>,
    mut cache: Option<&mut FxMatCache>,
    mut rows: Option<&mut super::inst::FxInstances>,
) {
    let sp = em.sprite.clone();
    em.next = em.next.max((inst.age - 2.0).floor());
    while em.next <= inst.age {
        let frame = if inst.looping { em.next % inst.len } else { em.next };
        em.next = em.next.floor() + 1.0;
        if frame < sp.start || frame > sp.end {
            continue;
        }
        // The slot's matrix: refreshed at each spawn call in the local-space modes.
        em.ms_slot = if matches!(s.mode(), 0 | 3) && (s.func01 || s.mode() == 0) { m } else { Mat4::IDENTITY };
        let blend = s.blend();
        for _ in 0..s.count() {
            em.count += 1;
            let (seed, k) = (em.seed, em.count.wrapping_mul(8));
            let (pos, vel, size, attached) = spawn_one(s, &m, inst.age, [sp.start, sp.end], &|i| hash01(seed, k.wrapping_add(i)));
            let mut part = MsPart {
                instance: em.instance,
                emitter: em_entity,
                attached,
                pos,
                vel,
                acc: flip(Vec3::new(s.w[4], s.w[5], s.w[6])),
                size,
                growth: s.w[9],
                age: 0.0,
                life: s.life(),
                func01: s.func01,
                hold: s.w[11],
                colors: s.colors,
                alpha_fades: s.alpha_fades(),
                material: Handle::default(),
                row: None,
                ribbon: None,
            };
            let c = ramp(&part);
            let a0 = if part.alpha_fades { ramp_alpha(&part) } else { 1.0 };
            if s.is_ribbon() {
                // Draw modes 2 / 3: a chain of `chain_len` nodes, all born at the spawn point.
                let n = s.chain_len();
                let mesh_h = meshes.add(ribbon_mesh(&[], 0, 0.0, Vec3::Z, Mat4::IDENTITY));
                part.ribbon = Some(Ribbon { mode: s.draw_mode(), width: s.ribbon_width(), nodes: vec![(pos, c); n], frames: 0, mesh: mesh_h.clone(), drawn: None, world: Vec::new() });
                let textured = s.draw_mode() == 3;
                let mat = ribbon_material(textured.then(|| em.texture.clone()), blend);
                // Never written after spawn (white; the colours are vertex colours): shared.
                part.material = mat_cache::shared(cache.as_deref_mut(), mats, mat);
                let e = commands.spawn((Mesh3d(mesh_h), MeshMaterial3d(part.material.clone()), Transform::IDENTITY, Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling, part)).id();
                commands.entity(em.instance).add_child(e);
                continue;
            }
            let mat = quad_material(Color::srgba(c.x, c.y, c.z, a0), em.texture.clone(), blend);
            // Coloured by `tick`: a row of the shared buffer (or a pooled material).
            let pe = commands.spawn_empty().id();
            let (material, row) = super::inst::lend(rows.as_deref_mut(), cache.as_deref_mut(), mats, pe, mat, "ms");
            super::inst::tag(commands, pe, pe, row);
            part.material = material;
            part.row = row;
            let q = mesh.get_or_insert_with(|| meshes.add(quads_mesh(&[XY], FULL_UV))).clone();
            let (world, edge) = place(&part, &em.ms_slot);
            let e = commands.entity(pe).insert((Mesh3d(q), MeshMaterial3d(part.material.clone()), local_transform(root, world, edge), Visibility::Inherited, part)).id();
            commands.entity(em.instance).add_child(e);
        }
    }
}

/// A ribbon particle's material (white: its colours are vertex colours; textured in draw mode 3).
pub(super) fn ribbon_material(texture: Option<Handle<Image>>, blend: Option<BlendState>) -> FxMaterial {
    let base = StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: texture,
        unlit: true,
        alpha_mode: if blend.is_some() { AlphaMode::Blend } else { AlphaMode::Mask(0.02) },
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    let mut mat = fx_material_gamma(base, fx_gamma());
    mat.extension.params.y = blend_mode(blend);
    mat.extension.piece = Vec4::new(0.0, 0.0, 0.0, 1.0);
    mat.extension.blend = blend;
    mat
}

/// A camera-facing particle quad's material (`color` = its first colour).
pub(super) fn quad_material(color: Color, texture: Handle<Image>, blend: Option<BlendState>) -> FxMaterial {
    let base = StandardMaterial {
        base_color: color,
        base_color_texture: Some(texture),
        unlit: true,
        alpha_mode: if blend.is_some() { AlphaMode::Blend } else { AlphaMode::Mask(0.02) },
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    let mut mat = fx_material_gamma(base, fx_gamma());
    mat.extension.params.y = blend_mode(blend);
    mat.extension.piece = Vec4::new(1.0, 0.0, 0.0, 1.0);
    mat.extension.blend = blend;
    mat
}

/// An empty ribbon's mesh (one degenerate triangle; the layout every ribbon has).
pub(super) fn empty_ribbon_mesh() -> Mesh {
    ribbon_mesh(&[], 0, 0.0, Vec3::Z, Mat4::IDENTITY)
}

/// The line / ribbon through a chain's nodes (head first, world position + rgb), positions put
/// through `to_local` (the instance root's inverse, the entity's parent).
///
/// Mode 3 (`FUN_14018d760`, node pairs from `FUN_14018dc10`): a triangle strip, two vertices per
/// node at `p +- side`, where `side = width x (view x d)` with `d` the unit direction to the next
/// node (the last node reuses the last direction: the program extends it one more segment) and
/// `view` the camera's own axis (`DAT_1405a5ca8 / 5cb8 / 5cc8`, taken as its forward). The cross
/// product is NOT renormalised, so a ribbon seen along its length thins out, as in the program.
/// u = 0 at the head, 0.5 along the middle (`0x3fff`), 1 at the last node (`0x7fff`); v = 0 / 1
/// across. Mode 2 (`FUN_14018d5f0`) is a line per node pair; drawn here as the same strip with a
/// fixed thin width (UNCONFIRMED: the original's lines are one pixel wide).
fn ribbon_mesh(nodes: &[(Vec3, Vec3)], mode: i32, width: f32, view: Vec3, to_local: Mat4) -> Mesh {
    let mut s = mesh_reuse::Strip::default();
    ribbon_fill(nodes, mode, width, view, to_local, &mut s);
    s.into_mesh()
}

/// `ribbon_mesh`'s vertices, written into an emptied strip.
fn ribbon_fill(nodes: &[(Vec3, Vec3)], mode: i32, width: f32, view: Vec3, to_local: Mat4, out: &mut mesh_reuse::Strip) {
    let n = nodes.len();
    let mesh_reuse::Strip { pos, uv, col, idx, .. } = &mut *out;
    let mut d_last = Vec3::ZERO;
    for k in 0..n {
        let (p, c) = nodes[k];
        let d = if k + 1 < n { (nodes[k + 1].0 - p).normalize_or_zero() } else { d_last };
        d_last = d;
        let side = view.cross(d) * width;
        let u = if k == 0 { 0.0 } else if k + 1 == n { 1.0 } else { 0.5 };
        for (sign, v) in [(1.0f32, 0.0f32), (-1.0, 1.0)] {
            pos.push(to_local.transform_point3(p + side * sign).to_array());
            uv.push([u, v]);
            col.push([c.x, c.y, c.z, 1.0]);
        }
        if k + 1 < n {
            let b = (k * 2) as u32;
            idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
        }
    }
    let _ = mode;
    // Never empty: a zero-length vertex buffer upsets the render allocator.
    out.never_empty();
}

/// Mode 2's stand-in line width, world units before the slot scale.
const LINE_WIDTH: f32 = 0.12;

/// Age, move and draw every particle.
#[allow(clippy::too_many_arguments)]
pub(super) fn tick(
    mut commands: Commands,
    time: Res<Time>,
    emitters: Query<&FxEmitter>,
    globals: Query<&GlobalTransform>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    mut parts: Query<(Entity, &mut MsPart, &mut Transform, &mut Visibility)>,
    mut mats: ResMut<Assets<FxMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rows: Option<ResMut<super::inst::FxInstances>>,
) {
    let fps = env_once("PD_FX_FPS").and_then(|v| v.parse().ok()).unwrap_or(FX_FPS);
    let step = time.delta_secs() * fps;
    for (e, mut p, mut t, mut vis) in &mut parts {
        let (Ok(root), Ok(em)) = (globals.get(p.instance), emitters.get(p.emitter)) else {
            commands.entity(e).despawn();
            continue;
        };
        p.age += step;
        if p.age > p.life {
            commands.entity(e).despawn();
            continue;
        }
        // Position uses the old velocity, then the velocity and size step (the update order).
        let v = p.vel;
        let frames_now = p.age.floor() as u32;
        if p.ribbon.is_some() {
            // The tail takes the older node's place once per effect frame, BEFORE the head moves.
            if let Some(r) = p.ribbon.as_mut() {
                while r.frames < frames_now {
                    r.frames += 1;
                    for i in (1..r.nodes.len()).rev() {
                        let prev = r.nodes[i - 1];
                        r.nodes[i] = (prev.0, prev.1 * 0.75);
                    }
                }
            }
        }
        p.pos += v * step;
        let a = p.acc;
        p.vel += a * step;
        let g = p.growth;
        p.size += g * step;
        if p.ribbon.is_some() {
            let (head, c) = (p.pos, ramp(&p));
            let attached = p.attached;
            let scale = if attached { em.ms_slot.x_axis.truncate().length() } else { 1.0 };
            let to_world = |q: Vec3| if attached { em.ms_slot.transform_point3(q) } else { q };
            if let Some(r) = p.ribbon.as_mut() {
                r.nodes[0] = (head, c);
                let mut world = std::mem::take(&mut r.world);
                world.clear();
                world.extend(r.nodes.iter().map(|&(q, col)| (to_world(q), col)));
                let width = if r.mode == 3 { r.width } else { LINE_WIDTH } * scale;
                let eye = world[0].0;
                let view = cameras.iter().min_by(|a, b| a.translation().distance_squared(eye).total_cmp(&b.translation().distance_squared(eye))).map_or(Vec3::Z, |cam| *cam.forward());
                let to_local = Mat4::from(root.affine().inverse());
                // Same inputs as the last write: the mesh already holds this ribbon.
                let same = mesh_reuse::enabled() && r.drawn.as_ref().is_some_and(|(w, wd, v, l)| *w == world && *wd == width && *v == view && *l == to_local);
                if !same {
                    let mode = r.mode;
                    mesh_reuse::rewrite(&mut meshes, &r.mesh, |s| ribbon_fill(&world, mode, width, view, to_local, s));
                    if mesh_reuse::enabled() {
                        let mut prev = r.drawn.take().map(|d| d.0).unwrap_or_default();
                        prev.clear();
                        prev.extend_from_slice(&world);
                        r.drawn = Some((prev, width, view, to_local));
                    }
                }
                r.world = world;
                *vis = Visibility::Inherited;
            }
            continue;
        }
        let (world, edge) = place(&p, &em.ms_slot);
        *t = local_transform(root, world, edge);
        let c = ramp(&p);
        let a = if p.alpha_fades { ramp_alpha(&p) } else { 1.0 };
        let col = Color::srgba(c.x, c.y, c.z, a);
        if let (Some(r), Some(rows)) = (p.row, rows.as_deref_mut()) {
            rows.set_color(r, col);
        } else if let Some(mut m) = mats.get_mut(&p.material) {
            if m.base.base_color != col {
                m.base.base_color = col;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_direction_products_give_unit_vectors() {
        for i in 0..200 {
            let (a, e) = (i as f32 * 0.37, (i as f32 * 0.113).sin() * 0.78);
            let d = dir(a.cos(), a.sin(), e.sin(), e.cos());
            assert!((d.length() - 1.0).abs() < 1e-4, "{d:?}");
        }
        // Level: azimuth only (x = sin, z = cos).
        let d = dir(0.0f32.cos(), 0.0f32.sin(), 0.0, 1.0);
        assert!((d - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-6);
    }

    impl MsPart {
        fn clone_for_test(&self) -> MsPart {
            MsPart { instance: self.instance, emitter: self.emitter, attached: self.attached, pos: self.pos, vel: self.vel, acc: self.acc, size: self.size, growth: self.growth, age: self.age, life: self.life, func01: self.func01, hold: self.hold, colors: self.colors, alpha_fades: self.alpha_fades, material: Handle::default(), row: None, ribbon: None }
        }
    }

    fn timer_mine_puff() -> MsSpec {
        // eft0097 emitter 3 (slot 2, the x10 blast): words 7 = 0.2, 8 = 0.8, 11 = 15, 24 = 2, 25 = 0.32, 30 = 15, 31 = 25.
        let mut w = [0.0f32; 32];
        w[7] = 0.2;
        w[8] = 0.8;
        w[11] = 15.0;
        w[16] = 1.0;
        w[24] = 2.0;
        w[25] = 0.32;
        w[30] = 15.0;
        w[31] = 25.0;
        MsSpec { func01: true, w, colors: [0, 0xFFFF_FFFF, 0], joint_m: Mat4::IDENTITY }
    }

    #[test]
    fn size_is_word_8_and_the_count_word_30_times_point_8() {
        let s = timer_mine_puff();
        assert_eq!(s.count(), 12);
        assert_eq!(s.life(), 25.0);
        let m = Mat4::from_scale(Vec3::splat(10.0));
        // Mode 0 (word 19 = 0): drawn through the matrix, so x10 on the size.
        let (_, _, size, attached) = spawn_one(&s, &m, 0.0, [0.0, 5.0], &|_| 0.5);
        assert!(attached);
        assert_eq!(size, 0.8, "the size is word 8 in the slot's local units, not word 30 / 31");
    }

    #[test]
    fn mode_one_is_world_space_and_unscaled() {
        let mut s = timer_mine_puff();
        s.w[19] = 1.0;
        let m = Mat4::from_scale_rotation_translation(Vec3::splat(10.0), Quat::IDENTITY, Vec3::new(5.0, 6.0, 7.0));
        let (pos, vel, size, attached) = spawn_one(&s, &m, 0.0, [0.0, 5.0], &|_| 0.5);
        assert!(!attached);
        assert_eq!(size, 0.8);
        assert!((vel.length() - 0.2).abs() < 1e-5, "speed word 7, not matrix-scaled");
        assert!(pos.distance(Vec3::new(5.0, 6.0, 7.0)) <= 0.32 * 10.0 + 1e-3);
    }

    #[test]
    fn colour_ramp_holds_at_word_11() {
        let s = timer_mine_puff();
        let mut p = MsPart { instance: Entity::PLACEHOLDER, emitter: Entity::PLACEHOLDER, attached: true, pos: Vec3::ZERO, vel: Vec3::ZERO, acc: Vec3::ZERO, size: 1.0, growth: 0.0, age: 0.0, life: 25.0, func01: true, hold: 15.0, colors: s.colors, alpha_fades: false, material: Handle::default(), row: None, ribbon: None };
        assert_eq!(ramp(&p), Vec3::ZERO);
        p.age = 15.0;
        assert!((ramp(&p) - Vec3::ONE).length() < 1e-5);
        p.age = 25.0;
        assert!(ramp(&p).length() < 1e-5);
    }

    /// Rapid Cannon's cast smoke (eft0048 emitter 0: colour 0x337f7f7f -> 0) and Fire of Gehenna's
    /// (0xff7f664c -> 0) are authored as a fade of the colour's ALPHA (20 % grey puffs / opaque to clear):
    /// a blend-1 puff takes its opacity from that byte, the same curve as the colour.
    #[test]
    fn blend_one_puffs_fade_by_the_colours_alpha() {
        let mut s = timer_mine_puff();
        s.w[26] = 1.0;
        s.colors = [0x337f_7f7f, 0, 0];
        s.w[11] = 0.0;
        let mut p = MsPart { instance: Entity::PLACEHOLDER, emitter: Entity::PLACEHOLDER, attached: false, pos: Vec3::ZERO, vel: Vec3::ZERO, acc: Vec3::ZERO, size: 5.0, growth: 0.5, age: 0.0, life: 30.0, func01: false, hold: 0.0, colors: s.colors, alpha_fades: s.alpha_fades(), material: Handle::default(), row: None, ribbon: None };
        assert!((ramp_alpha(&p) - 0.2).abs() < 1e-6, "0x33 = 20 % opaque at birth");
        p.age = 15.0;
        assert!((ramp_alpha(&p) - 0.1).abs() < 1e-6, "half way: 10 %");
        p.age = 30.0;
        assert!(ramp_alpha(&p).abs() < 1e-6, "clear at the end of its life");
        // A Func01 puff with a hold word runs 12 -> 13 up to the hold age, 13 -> 14 after (the alpha too).
        let mut q = p.clone_for_test();
        q.func01 = true;
        q.hold = 10.0;
        q.colors = [0x0000_0000, 0xff00_0000, 0x0000_0000];
        q.age = 10.0;
        assert!((ramp_alpha(&q) - 1.0).abs() < 1e-6);
        q.age = 20.0;
        assert!((ramp_alpha(&q) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn chain_length_follows_the_spawn_clamps() {
        // `FUN_14010f710` (Func01): below draw mode 2.0 one node; else min(word 17, 5).
        let mut s = timer_mine_puff();
        s.w[17] = 10.0;
        assert_eq!(s.chain_len(), 1, "billboards (mode 1) are never chained");
        assert!(!s.is_ribbon());
        s.w[16] = 3.0;
        assert_eq!(s.chain_len(), 5, "Func01 clamps at 5");
        assert!(s.is_ribbon());
        s.func01 = false;
        assert_eq!(s.chain_len(), 6, "Func00 clamps at 6 (DAT_1402f7f30)");
        s.w[17] = 3.0;
        s.w[16] = 2.0;
        assert_eq!(s.chain_len(), 3);
        s.w[17] = 1.0;
        assert!(!s.is_ribbon(), "one node is just a point");
    }

    #[test]
    fn ribbon_strip_has_two_vertices_per_node_and_the_programs_u_values() {
        let nodes = [(Vec3::ZERO, Vec3::ONE), (Vec3::new(0.0, 0.0, 1.0), Vec3::ONE * 0.75), (Vec3::new(0.0, 0.0, 2.0), Vec3::ONE * 0.5)];
        let m = ribbon_mesh(&nodes, 3, 0.5, Vec3::Y, Mat4::IDENTITY);
        let pos: Vec<[f32; 3]> = m.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap().to_vec();
        assert_eq!(pos.len(), 6);
        // view = +Y, d = +Z: side = Y x Z x 0.5 = +X x 0.5.
        assert_eq!(pos[0], [0.5, 0.0, 0.0]);
        assert_eq!(pos[1], [-0.5, 0.0, 0.0]);
        let uv: Vec<[f32; 2]> = match m.attribute(Mesh::ATTRIBUTE_UV_0).unwrap() {
            bevy::mesh::VertexAttributeValues::Float32x2(v) => v.clone(),
            _ => panic!(),
        };
        assert_eq!([uv[0][0], uv[2][0], uv[4][0]], [0.0, 0.5, 1.0], "head 0, middle 0x3fff, end 0x7fff");
        assert_eq!([uv[0][1], uv[1][1]], [0.0, 1.0]);
    }

    // The ribbon builder as it was before `mesh_reuse.rs` (verbatim), the reference for the test below.
    fn legacy_ribbon_mesh(nodes: &[(Vec3, Vec3)], mode: i32, width: f32, view: Vec3, to_local: Mat4) -> Mesh {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology};
        let n = nodes.len();
        let (mut pos, mut uv, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut d_last = Vec3::ZERO;
        for k in 0..n {
            let (p, c) = nodes[k];
            let d = if k + 1 < n { (nodes[k + 1].0 - p).normalize_or_zero() } else { d_last };
            d_last = d;
            let side = view.cross(d) * width;
            let u = if k == 0 { 0.0 } else if k + 1 == n { 1.0 } else { 0.5 };
            for (sign, v) in [(1.0f32, 0.0f32), (-1.0, 1.0)] {
                pos.push(to_local.transform_point3(p + side * sign).to_array());
                uv.push([u, v]);
                col.push([c.x, c.y, c.z, 1.0]);
            }
            if k + 1 < n {
                let b = (k * 2) as u32;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        }
        let _ = mode;
        if pos.is_empty() {
            // Never empty: a zero-length vertex buffer upsets the render allocator.
            pos = vec![[0.0; 3]; 3];
            uv = vec![[0.0; 2]; 3];
            col = vec![[0.0; 4]; 3];
            idx = vec![0, 1, 2];
        }
        let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_indices(Indices::U32(idx))
    }

    #[test]
    fn ribbons_rewritten_in_place_match_the_old_builder() {
        let mut meshes = Assets::<Mesh>::default();
        let h = meshes.add(legacy_ribbon_mesh(&[], 0, 0.0, Vec3::Z, Mat4::IDENTITY));
        let to_local = Mat4::from_scale_rotation_translation(Vec3::splat(0.5), Quat::from_rotation_y(0.7), Vec3::new(3.0, 1.0, -2.0));
        for n in [1usize, 4, 7, 0, 3] {
            let nodes: Vec<(Vec3, Vec3)> = (0..n).map(|k| (Vec3::new(k as f32 * 1.3, (k as f32).sin(), k as f32 * 0.4), Vec3::new(1.0, 0.75f32.powi(k as i32), 0.5))).collect();
            for (mode, width, view) in [(3, 0.5, Vec3::Y), (2, LINE_WIDTH, Vec3::new(0.3, -0.2, 0.9).normalize())] {
                mesh_reuse::rewrite(&mut meshes, &h, |s| ribbon_fill(&nodes, mode, width, view, to_local, s));
                assert!(mesh_reuse::same_mesh(meshes.get(&h).unwrap(), &legacy_ribbon_mesh(&nodes, mode, width, view, to_local)), "{n} nodes mode {mode}");
                assert!(mesh_reuse::same_mesh(&ribbon_mesh(&nodes, mode, width, view, to_local), &legacy_ribbon_mesh(&nodes, mode, width, view, to_local)));
            }
        }
    }

    #[test]
    fn blend_words() {
        let mut s = timer_mine_puff();
        s.w[26] = 3.0;
        assert_eq!(s.blend(), Some(BlendState { src: BlendFactor::DstColor, dst: BlendFactor::One, op: BlendOp::Add }));
        s.w[26] = 1.0;
        // Blend 1 is SRC_ALPHA, 1 - SRC_ALPHA (0x5401); `PD_MSP_OPAQUE=1` is the literal opaque pixel shader.
        assert_eq!(s.blend(), Some(BlendState { src: BlendFactor::SrcAlpha, dst: BlendFactor::OneMinusSrcAlpha, op: BlendOp::Add }));
        assert!(s.alpha_fades());
        s.w[26] = 0.0;
        assert_eq!(s.blend(), Some(BlendState { src: BlendFactor::One, dst: BlendFactor::One, op: BlendOp::Add }));
    }
}
