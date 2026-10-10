//! Effect material reuse (lane fxperf, 2026-10-08; Laurence: "the effects are great but they
//! cause frame drops when casting moves").
//!
//! Before this, every cast made a brand-new `FxMaterial` asset for every effect piece, sprite,
//! point, MSPARTICLE particle, trail, blade, flare and light decal it spawned, and dropped each
//! one when it faded. Each new asset is extracted and prepared by the render thread (its own
//! uniform buffer and bind group), and each dropped one is torn down again, so a cast that
//! spawns a hundred particles paid for a hundred of each.
//!
//! What each site does with its material after spawn decides how it is reused (read from the
//! code, 2026-10-08):
//!
//! | site | written after spawn? | reuse |
//! |---|---|---|
//! | `attach_pieces` piece | yes, `animate_pieces` (colour, UV, roll, alpha, quad) | pool |
//! | `spawn_trails` trail | yes, `animate_trails` (colour) | pool |
//! | `spawn_trails` blade | yes, `update_blades` (colour) | pool |
//! | `spawn_trails` flare | no (colours are vertex colours) | shared |
//! | `spawn_trails` light decal | yes, `update_lights` (colour) | pool |
//! | `emit_points` point | yes, `tick_sprites` (colour, cell) | pool |
//! | `tick_sprites` puff | yes, `tick_sprites` (colour, cell) | pool |
//! | `ms_particle::emit` quad | yes, `ms_particle::tick` (colour) | pool |
//! | `ms_particle::emit` ribbon | no (white; colours are vertex colours) | shared |
//! | `aura_wisp` body | yes, every frame | not changed (see below) |
//!
//! * **Shared** (never written after spawn): one asset per distinct material value. The key is
//!   the material's whole `Debug` text, i.e. every field (texture handles by asset index, alpha
//!   mode, blend, cull, every colour and uniform), so two spawns share an asset only when the
//!   fresh materials they would have made are equal field by field.
//! * **Pool** (written per instance every frame): a material whose last user has gone (the pool
//!   holds the only handle left) goes on a free list under its static key (textures, alpha mode,
//!   cull, sidedness, transmission, the `FxKey` blend, the shader family). The next spawn with the
//!   same static key takes it and overwrites the WHOLE material with the fresh value it would
//!   have created, so its first frame is the fresh material exactly. Two live users never share.
//!   This saves the asset's creation and teardown, not its per-frame re-preparation (that is the
//!   per-instance-buffer change PERF.md lists as next; not done here: it rewrites the effect
//!   shader and cannot be checked without a GPU).
//! * `aura_wisp` bodies are left as they were: they are created in `PostUpdate`, after which a
//!   reused material's reset could reach the render world a frame late and show the previous
//!   wisp's colour for one frame. Wisp bodies also live for the whole effect, so there is little
//!   churn to save.
//!
//! Every pooled reuse happens in an `Update` system, before Bevy publishes the frame's asset
//! events (`AssetEventSystems`, `PostUpdate`), so the reset reaches the render world in the same
//! frame as the entity that uses it; and a modified material is dropped from the render world
//! before it is prepared again (`prepare_assets`), so a new user never sees the old values.
//!
//! `PD_FX_MATCACHE=0` turns all of it off: every site makes its own material, as before.

use super::*;
use std::sync::OnceLock;

/// `PD_FX_MATCACHE=0`: every spawn makes its own material (the old path). Read once.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PD_FX_MATCACHE").map_or(true, |v| v != "0"))
}

/// Most idle pooled materials kept (all keys together); more are freed.
const MAX_FREE: usize = 2048;
/// Shared materials kept before the unused ones are freed.
const MAX_SHARED: usize = 1024;

/// What a pooled material is filed under: every field that picks the pipeline or the bind group's
/// images. The animated values (colours, UVs, alpha, quad) are not in it; a reuse overwrites them.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct StaticKey {
    textures: [Option<AssetId<Image>>; 5],
    alpha: (u8, u32),
    cull: u8,
    double_sided: bool,
    unlit: bool,
    transmission: u32,
    fx: FxKey,
    family: u32,
}

impl StaticKey {
    pub fn of(m: &FxMaterial) -> StaticKey {
        let b = &m.base;
        let x = &m.extension;
        let alpha = match b.alpha_mode {
            AlphaMode::Opaque => (0, 0),
            AlphaMode::Mask(t) => (1, t.to_bits()),
            AlphaMode::Blend => (2, 0),
            AlphaMode::Premultiplied => (3, 0),
            AlphaMode::AlphaToCoverage => (4, 0),
            AlphaMode::Add => (5, 0),
            AlphaMode::Multiply => (6, 0),
        };
        let cull = match b.cull_mode {
            None => 0,
            Some(Face::Front) => 1,
            Some(Face::Back) => 2,
        };
        StaticKey {
            textures: [b.base_color_texture.as_ref().map(|h| h.id()), x.tex0.as_ref().map(|h| h.id()), x.tex1.as_ref().map(|h| h.id()), x.tex2.as_ref().map(|h| h.id()), x.tex3.as_ref().map(|h| h.id())],
            alpha,
            cull,
            double_sided: b.double_sided,
            unlit: b.unlit,
            transmission: b.specular_transmission.to_bits(),
            fx: FxKey::from(x),
            family: x.extra.y.to_bits(),
        }
    }
}

/// The shared and pooled effect materials, and what they saved (`PD_FRAME_STATS`' `[churn]`).
#[derive(Resource)]
pub struct FxMatCache {
    on: bool,
    shared: HashMap<String, Handle<FxMaterial>>,
    /// Pooled materials handed out (some may have been released since the last `sweep`).
    lent: Vec<(StaticKey, Handle<FxMaterial>)>,
    free: HashMap<StaticKey, Vec<Handle<FxMaterial>>>,
    free_count: usize,
    /// Materials created, pooled ones reused, shared ones handed out again.
    pub made: u64,
    pub reused: u64,
    pub shared_hits: u64,
}

impl Default for FxMatCache {
    fn default() -> Self {
        FxMatCache::new(enabled())
    }
}

/// True when `h` is the last handle to its asset (its users have all gone).
fn last_handle(h: &Handle<FxMaterial>) -> bool {
    match h {
        Handle::Strong(a) => Arc::strong_count(a) == 1,
        Handle::Uuid(..) => false,
    }
}

impl FxMatCache {
    pub fn new(on: bool) -> Self {
        FxMatCache { on, shared: HashMap::new(), lent: Vec::new(), free: HashMap::new(), free_count: 0, made: 0, reused: 0, shared_hits: 0 }
    }

    /// A material nobody writes after spawn: the one asset already made with this exact value, or a new one.
    pub fn shared(&mut self, mats: &mut Assets<FxMaterial>, m: FxMaterial) -> Handle<FxMaterial> {
        if !self.on {
            self.made += 1;
            return mats.add(m);
        }
        let key = format!("{m:?}");
        if let Some(h) = self.shared.get(&key) {
            if mats.contains(h) {
                self.shared_hits += 1;
                return h.clone();
            }
        }
        self.made += 1;
        let h = mats.add(m);
        self.shared.insert(key, h.clone());
        h
    }

    /// A material its user animates: a released one with the same static key, reset to `m`, or a new one.
    pub fn pooled(&mut self, mats: &mut Assets<FxMaterial>, m: FxMaterial) -> Handle<FxMaterial> {
        if !self.on {
            self.made += 1;
            return mats.add(m);
        }
        let key = StaticKey::of(&m);
        while let Some(h) = self.free.get_mut(&key).and_then(|v| v.pop()) {
            self.free_count -= 1;
            if let Some(mut slot) = mats.get_mut(&h) {
                *slot = m;
                self.reused += 1;
                self.lent.push((key, h.clone()));
                return h;
            }
        }
        self.made += 1;
        let h = mats.add(m);
        self.lent.push((key, h.clone()));
        h
    }

    /// File every lent material whose users have all gone under its key; free what is over the caps.
    pub fn sweep(&mut self) {
        let mut i = 0;
        while i < self.lent.len() {
            if last_handle(&self.lent[i].1) {
                let (k, h) = self.lent.swap_remove(i);
                if self.free_count < MAX_FREE {
                    self.free.entry(k).or_default().push(h);
                    self.free_count += 1;
                }
            } else {
                i += 1;
            }
        }
        if self.shared.len() > MAX_SHARED {
            self.shared.retain(|_, h| !last_handle(h));
        }
    }

    /// Materials currently idle in the pool.
    pub fn idle(&self) -> usize {
        self.free_count
    }
}

/// Once a frame, before the effect systems spawn: return released materials to the pool.
pub fn sweep(cache: Option<ResMut<FxMatCache>>) {
    if let Some(mut c) = cache {
        if c.on {
            c.sweep();
        }
    }
}

/// `FxMatCache::pooled`, or a plain new material without the resource (old callers, tests).
pub fn pooled(cache: Option<&mut FxMatCache>, mats: &mut Assets<FxMaterial>, m: FxMaterial) -> Handle<FxMaterial> {
    match cache {
        Some(c) => c.pooled(mats, m),
        None => mats.add(m),
    }
}

/// `FxMatCache::shared`, or a plain new material without the resource.
pub fn shared(cache: Option<&mut FxMatCache>, mats: &mut Assets<FxMaterial>, m: FxMaterial) -> Handle<FxMaterial> {
    match cache {
        Some(c) => c.shared(mats, m),
        None => mats.add(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> (Assets<FxMaterial>, Assets<Image>) {
        (Assets::<FxMaterial>::default(), Assets::<Image>::default())
    }

    fn additive(tex: Option<Handle<Image>>) -> FxMaterial {
        fx_material_gamma(StandardMaterial { base_color_texture: tex, unlit: true, alpha_mode: AlphaMode::Add, double_sided: true, cull_mode: None, ..default() }, 2.2)
    }

    #[test]
    fn shared_materials_are_one_asset_per_exact_value() {
        let (mut mats, mut images) = assets();
        let (a, b) = (images.add(Image::default()), images.add(Image::default()));
        let mut c = FxMatCache::new(true);
        let h1 = c.shared(&mut mats, additive(Some(a.clone())));
        let h2 = c.shared(&mut mats, additive(Some(a.clone())));
        assert_eq!(h1.id(), h2.id());
        // A different texture never shares.
        let h3 = c.shared(&mut mats, additive(Some(b.clone())));
        assert_ne!(h1.id(), h3.id());
        // A different blend never shares.
        let mut m = additive(Some(a.clone()));
        m.extension.blend = Some(BlendState { src: BlendFactor::SrcAlpha, dst: BlendFactor::OneMinusSrcAlpha, op: BlendOp::Add });
        let h4 = c.shared(&mut mats, m.clone());
        assert_ne!(h1.id(), h4.id());
        m.extension.blend = Some(BlendState { src: BlendFactor::SrcAlpha, dst: BlendFactor::One, op: BlendOp::Add });
        assert_ne!(h4.id(), c.shared(&mut mats, m).id());
        // Nor a different alpha mode, cull, colour or uniform.
        let mut m = additive(Some(a.clone()));
        m.base.alpha_mode = AlphaMode::Blend;
        assert_ne!(h1.id(), c.shared(&mut mats, m).id());
        let mut m = additive(Some(a.clone()));
        m.base.cull_mode = Some(Face::Back);
        assert_ne!(h1.id(), c.shared(&mut mats, m).id());
        let mut m = additive(Some(a.clone()));
        m.base.base_color = Color::srgba(0.5, 0.5, 0.5, 1.0);
        assert_ne!(h1.id(), c.shared(&mut mats, m).id());
        let mut m = additive(Some(a.clone()));
        m.extension.piece.x = 1.0;
        assert_ne!(h1.id(), c.shared(&mut mats, m).id());
        // The shared asset holds exactly the value asked for.
        assert_eq!(format!("{:?}", mats.get(&h1).unwrap()), format!("{:?}", additive(Some(a))));
    }

    #[test]
    fn static_keys_differ_across_textures_and_blends() {
        let mut images = Assets::<Image>::default();
        let (a, b) = (images.add(Image::default()), images.add(Image::default()));
        assert_eq!(StaticKey::of(&additive(Some(a.clone()))), StaticKey::of(&additive(Some(a.clone()))));
        assert_ne!(StaticKey::of(&additive(Some(a.clone()))), StaticKey::of(&additive(Some(b))));
        let mut m = additive(Some(a.clone()));
        m.extension.blend = Some(BlendState { src: BlendFactor::DstColor, dst: BlendFactor::Zero, op: BlendOp::Add });
        assert_ne!(StaticKey::of(&additive(Some(a.clone()))), StaticKey::of(&m));
        let mut m = additive(Some(a.clone()));
        m.extension.tex1 = Some(a.clone());
        assert_ne!(StaticKey::of(&additive(Some(a.clone()))), StaticKey::of(&m));
        // Animated values are not part of it.
        let mut m = additive(Some(a.clone()));
        m.base.base_color = Color::BLACK;
        m.extension.uv0 = Vec4::splat(3.0);
        assert_eq!(StaticKey::of(&additive(Some(a))), StaticKey::of(&m));
    }

    #[test]
    fn live_pooled_materials_never_share() {
        let (mut mats, _) = assets();
        let mut c = FxMatCache::new(true);
        let h1 = c.pooled(&mut mats, additive(None));
        c.sweep();
        let h2 = c.pooled(&mut mats, additive(None));
        c.sweep();
        assert_ne!(h1.id(), h2.id());
        assert_eq!(c.reused, 0);
        assert_eq!(c.idle(), 0);
    }

    #[test]
    fn a_released_material_is_reused_and_reset_to_the_fresh_value() {
        let (mut mats, _) = assets();
        let mut c = FxMatCache::new(true);
        let h1 = c.pooled(&mut mats, additive(None));
        let id = h1.id();
        // Its user animates it ...
        {
            let mut m = mats.get_mut(&h1).unwrap();
            m.base.base_color = Color::srgba(0.1, 0.2, 0.3, 0.4);
            m.extension.uv0 = Vec4::new(0.5, 0.5, 0.25, 0.0);
            m.extension.quad = Vec4::ZERO;
        }
        // ... and goes away.
        drop(h1);
        c.sweep();
        assert_eq!(c.idle(), 1);
        let mut fresh = additive(None);
        fresh.base.base_color = Color::srgba(1.0, 0.5, 0.0, 1.0);
        let h2 = c.pooled(&mut mats, fresh.clone());
        assert_eq!(h2.id(), id);
        assert_eq!(c.reused, 1);
        assert_eq!(format!("{:?}", mats.get(&h2).unwrap()), format!("{fresh:?}"));
        // A different static key does not take it.
        drop(h2);
        c.sweep();
        let mut other = additive(None);
        other.base.alpha_mode = AlphaMode::Blend;
        assert_ne!(c.pooled(&mut mats, other).id(), id);
    }

    #[test]
    fn switched_off_every_spawn_makes_its_own_material() {
        let (mut mats, _) = assets();
        let mut c = FxMatCache::new(false);
        let a = c.shared(&mut mats, additive(None));
        let b = c.shared(&mut mats, additive(None));
        assert_ne!(a.id(), b.id());
        let p = c.pooled(&mut mats, additive(None));
        let id = p.id();
        drop(p);
        c.sweep();
        assert_ne!(c.pooled(&mut mats, additive(None)).id(), id);
        assert_eq!(c.reused + c.shared_hits, 0);
        // No resource: the plain path.
        assert_ne!(shared(None, &mut mats, additive(None)).id(), shared(None, &mut mats, additive(None)).id());
    }
}
