//! The sky's cloud layer drifts.
//!
//! The original's backdrop is two shells, not one: `m2_d0` is an untextured
//! vertex-colour gradient and `m2_d1` is a *tiled* cloud layer — a single 512²
//! PNG, a REPEAT sampler, and UVs authored to a 3.475 box on both axes
//! (FORMATS-STAGES.md "The sky is two shells"). A texture wrapped 3.5× across a
//! 22,000-unit shell is there to move; the original advanced it, we never did,
//! so the sky sat perfectly still.
//!
//! The advance is *not* in the original's shaders (neither `SD_ps` nor
//! `PDEFTBB01_ps` contains a single `sin`/`cos`/time term), so it was done by
//! the fixed-function texture-transform stage and the exact speed, axis and
//! direction are **not recovered** — see the constant below. This is a slow UV
//! drift on the textured shell only; the gradient never moves.
//!
//! Render-side only: it reads `Time` and writes a material. The simulation
//! (`sim.rs` / `combat.rs`) never sees any of it.
//!
//! **Why this runs in `PostUpdate`, not `Update`** (learned 2026-09-29): writing
//! an `Assets<T>` inside a normal system schedules the change for the *next*
//! frame's apply (Bevy `b0004`), so an `Added<MeshMaterial3d>` filter — which
//! only matches for one frame — would write into the void and be dropped. A/B
//! screenshots with the UV set and then discarded measured as *zero* pixel
//! difference. `PostUpdate` runs after the apply, so the write lands this frame.

use bevy::camera::primitives::MeshAabb;
use bevy::prelude::*;

/// UV scroll in **texture repeats per second**.
///
/// ⚠️ **Eyeballed, not recovered.** The original's per-frame UV step lives in a
/// fixed-function state block that never reaches the shader, so the rate is not
/// in the binary. Judged against the look: the cloud sheet tiles 3.5× over a
/// 22,000-unit shell, so one full repeat is a very long travel. 0.004 repeats/s
/// takes ~4 minutes to cross one tile — a drift you notice if you watch the
/// sky and never notice if you are playing. Faster reads as a spinning texture
/// and immediately looks fake; this is deliberately near the threshold of
/// visibility, which is the 2004 look. Revisit only against footage.
pub const SKY_SCROLL_RATE: Vec2 = Vec2::new(0.004, 0.0);

/// The original's per-material scroll, per stage (`<cache>/stNN.sky.json`, written
/// by `pd_convert skyscroll`). **READ** (`FUN_1402a07b0` @ `0x1402a07b0`): the
/// six int16 fields of the material record at +0x3c..+0x46, each times the
/// program's frame counter (30 Hz) times 1e-5, are the vertex-shader constants
/// c90/c91 = three (u, v) texture offsets, slot 0 = the colour layer
/// (`SBTSCLOUD2_vs`: `uv0 = uv * c12.z + c90.xy`). **LIVE**: the same formula
/// measured on the menu clouds (FRONTEND.md, 2026-10-02): (10, -200) -> (+0.003,
/// -0.060) UV/s. So a draw's drift is `(f0, f1) * 1e-5 * 30` repeats per second.
/// `SKY_SCROLL_RATE` above is now only the fallback for a cache with no
/// `.sky.json`; a stage that has one scrolls only the draws it lists.
#[derive(Resource, Default)]
pub struct SkyFields {
    /// Node name (`m2_d1`) -> UV drift of the colour layer, repeats per second.
    pub draws: Option<std::collections::HashMap<String, Vec2>>,
}

impl SkyFields {
    pub fn load(cache: &std::path::Path, stage: &str) -> Self {
        let Some(v) = std::fs::read_to_string(cache.join(format!("{stage}.sky.json"))).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
            return Self::default();
        };
        let k = v["scale"].as_f64().unwrap_or(1e-5) as f32 * v["counter_hz"].as_f64().unwrap_or(30.0) as f32;
        let draws = v["draws"].as_object().map(|o| {
            o.iter()
                .filter_map(|(n, f)| {
                    let f = f.as_array()?;
                    Some((n.clone(), Vec2::new(f.first()?.as_f64()? as f32 * k, f.get(1)?.as_f64()? as f32 * k)))
                })
                .collect()
        });
        Self { draws }
    }
}

/// Frozen clock for proof shots: `PD_SKY_FRAME_DT=<secs>` makes the drift clock
/// `frame * dt` instead of wall time, so a burst of screenshots is evenly spaced.
fn clock(time: &Time, frame: u32) -> f32 {
    match std::env::var("PD_SKY_FRAME_DT").ok().and_then(|v| v.parse::<f32>().ok()) {
        Some(dt) => frame as f32 * dt,
        None => time.elapsed_secs(),
    }
}

/// The rate actually used, overridable **for verification only** by
/// `PD_SKY_SCROLL=<repeats/sec>`. The shipped value is `SKY_SCROLL_RATE`; the
/// override exists so the drift can be proved in one screenshot pair instead of
/// waiting four minutes for a 0.004 repeats/sec translation, and so Laurence can
/// A/B the speed (`VERIFY.md` "labelled variants"). Unset in normal play.
fn rate() -> Vec2 {
    match std::env::var("PD_SKY_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
        Some(v) => Vec2::new(v, 0.0),
        None => SKY_SCROLL_RATE,
    }
}

/// True for a **backdrop** draw: a model node named `m<model>_d<draw>`
/// (e.g. `m2_d1`, `m82_d0`) **whose mesh is the huge enclosing shell.**
///
/// ⚠️ Two corrections learned the hard way on 2026-09-29, both of which made
/// the first attempt measure *zero* pixel change while looking correct:
///
/// 1. **Not just `m2_`.** The backdrop is not always model 2 — st08 (Sein)
///    draws its 24,256-unit shell as `m82_d0`/`m82_d1`, while st06's is
///    `m2_*`. A `starts_with("m2_")` filter matches nothing on st08.
/// 2. **The name alone is not enough either**, because *every* model instance
///    in a stage is named this way (`m0_d0`…`m122_d1`, ~600 nodes). Matching
///    the pattern alone would scroll the whole level's textures.
///
/// So the test is the name pattern **and** a world-space extent above
/// `MIN_SHELL_EXTENT`, which no real prop or fighter mesh comes near (the
/// largest non-backdrop mesh in st08 is 11,914 units — see BACKDROP_EXTENT in
/// FORMATS-STAGES.md; that one is the ground, which is why the threshold is
/// 15,000 and not 20,000).
fn is_backdrop_draw(name: &str, extent: f32) -> bool {
    let Some(rest) = name.strip_prefix('m') else { return false };
    let Some((model, draw)) = rest.split_once("_d") else { return false };
    let numeric = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    numeric(model) && numeric(draw) && extent >= MIN_SHELL_EXTENT
}

/// World-space size a mesh must reach to be the enclosing backdrop shell.
pub const MIN_SHELL_EXTENT: f32 = 15_000.0;

/// Drifts the sky's tiled cloud layer.
///
/// Targets only materials that are both (a) on a backdrop node (`mN_dM`) and
/// (b) *textured* — the untextured gradient shell must stay put. The shells
/// carry distinct materials (st08: `m82_d0` → 22, `m82_d1` → 23), which is what
/// makes this safe: a shared material would drag the gradient along with it.
pub fn scroll(
    time: Res<Time>,
    frames: Res<bevy::diagnostic::FrameCount>,
    fields: Option<Res<SkyFields>>,
    added: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Added<MeshMaterial3d<StandardMaterial>>>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    meshes: Query<&Mesh3d>,
    mesh_assets: Res<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut live: Local<Vec<(AssetId<StandardMaterial>, Vec2)>>,
) {
    let t = clock(&time, frames.0);
    let manual = std::env::var_os("PD_SKY_SCROLL").is_some();
    let rate = rate();
    for (e, handle) in &added {
        // Bevy synthesises the entity names when spawning a glTF: the mesh
        // entity is called "Mesh" and the node's own name (`m82_d1`) sits on its
        // parent. So walk up until a model-node name turns up.
        let mut cursor = e;
        let mut name: Option<&str> = None;
        for _ in 0..3 {
            let Ok(child_of) = parents.get(cursor) else { break };
            cursor = child_of.parent();
            if let Ok(n) = names.get(cursor) {
                if n.as_str().starts_with('m') {
                    name = Some(n.as_str());
                    break;
                }
            }
        }
        let Some(name) = name else { continue };
        // `Mesh3d` lives on the material's own entity (the one named "Mesh").
        let Ok(mesh3d) = meshes.get(e) else { continue };
        let Some(mesh) = mesh_assets.get(&mesh3d.0) else { continue };
        let Some(aabb) = mesh.compute_aabb() else { continue };
        let extent = aabb.half_extents.length() * 2.0;
        let dbg = std::env::var_os("PD_SKY_DEBUG").is_some();
        if dbg {
            eprintln!("[sky] {e} name={name} extent={extent:.0}");
        }
        if !is_backdrop_draw(name, extent) {
            continue;
        }
        // Gradient shell: untextured, so leave it alone.
        if mats.get(&handle.0).is_none_or(|m| m.base_color_texture.is_none()) {
            if dbg {
                eprintln!("[sky] {e} name={name} extent={extent:.0} -> UNTEXTURED, skipped");
            }
            continue;
        }
        // The original's own rate for this draw's material (READ), else the
        // old fallback (cache without `.sky.json`), else PD_SKY_SCROLL.
        let r = if manual {
            rate
        } else {
            match fields.as_ref().and_then(|f| f.draws.as_ref()) {
                Some(d) => d.get(name).copied().unwrap_or(Vec2::ZERO),
                None => rate,
            }
        };
        if dbg {
            eprintln!("[sky] {e} name={name} extent={extent:.0} -> SCROLLING {r:?}");
        }
        if r != Vec2::ZERO && !live.iter().any(|(id, _)| *id == handle.0.id()) {
            live.push((handle.0.id(), r));
        }
    }
    // `uv_transform` is applied before sampling, so a translation is a scroll in
    // texture repeats (the REPEAT sampler wraps it). Written every frame: the
    // material is shared by every draw that uses the same layer. The offset is
    // kept inside one repeat so f32 precision never degrades the drift.
    for (id, r) in live.iter() {
        if let Some(mut m) = mats.get_mut(*id) {
            let o = *r * t;
            m.uv_transform.translation = Vec2::new(o.x.rem_euclid(1.0), o.y.rem_euclid(1.0));
        }
    }
}
