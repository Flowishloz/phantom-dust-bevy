//! Draws the stage objects' state (props.rs): a damaged or broken object's
//! own model from the stage's model library (`stNN.props.glb`, nodes
//! `typeTT_damaged` / `typeTT_broken`) replaces its placed model (the stage
//! glTF's `propNNN_*` nodes); a thrown object's placed model flies with its
//! shot; a gone one disappears. Reads the sim, never writes it.

use crate::frontend::state::MatchScene;
use crate::props::PropState;
use crate::World;
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// The library scene (hidden; its meshes are copied for replacements).
#[derive(Component)]
pub struct PropLibrary;

/// A damaged/broken model standing in for a placed object.
#[derive(Component)]
pub struct PropReplacement;

struct Placed {
    ty: u32,
    matrix: Mat4,
}

#[derive(Resource, Default)]
pub struct PropView {
    placed: HashMap<u32, Placed>,
    /// Stage glTF node entities per object (found once the stage has loaded).
    nodes: HashMap<u32, Vec<Entity>>,
    /// What each object currently shows (only objects that left `Intact`).
    shown: HashMap<u32, (PropState, Option<Entity>)>,
}

impl PropView {
    /// The stage glTF node entities of object `index` (its `propNNN_*` nodes), once the stage has loaded and
    /// `sync_props` has found them; empty before. Prop Hunt copies their meshes for a hider's disguise
    /// (`prophunt_view.rs`).
    pub fn nodes_of(&self, index: u32) -> &[Entity] {
        self.nodes.get(&index).map_or(&[], Vec::as_slice)
    }

    /// TEST ONLY: a view whose object `index` is drawn by `nodes`.
    #[cfg(test)]
    pub fn with_nodes(nodes: impl IntoIterator<Item = (u32, Vec<Entity>)>) -> Self {
        PropView { nodes: nodes.into_iter().collect(), ..Default::default() }
    }

    /// Reads the stage's `props.json` and spawns its model library (hidden).
    pub fn spawn(commands: &mut Commands, assets: &AssetServer, cache: &Path, stage: &str) {
        let mut view = PropView::default();
        let json: serde_json::Value = std::fs::read_to_string(cache.join(format!("{stage}.props.json"))).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        for o in json["objects"].as_array().into_iter().flatten() {
            let (Some(i), Some(ty), Some(m)) = (o["index"].as_u64(), o["type"].as_u64(), o["matrix"].as_array()) else { continue };
            let cols: Vec<f32> = m.iter().filter_map(|v| v.as_f64().map(|x| x as f32)).collect();
            if let Ok(cols) = <[f32; 16]>::try_from(cols) {
                view.placed.insert(i as u32, Placed { ty: ty as u32, matrix: Mat4::from_cols_array(&cols) });
            }
        }
        if let Some(lib) = json["library"].as_str().filter(|l| cache.join(l).exists()) {
            commands.spawn((WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(lib.to_string()))), Transform::default(), Visibility::Hidden, PropLibrary, MatchScene));
        }
        commands.insert_resource(view);
    }
}

/// `prop012_deskA_d1` -> 12.
fn prop_index(name: &str) -> Option<u32> {
    name.strip_prefix("prop")?.get(..3)?.parse().ok()
}

#[allow(clippy::too_many_arguments)]
pub fn sync_props(
    mut commands: Commands,
    world: Res<World>,
    view: Option<ResMut<PropView>>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    names: Query<(Entity, &Name)>,
    children: Query<&Children>,
    meshes: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, &GlobalTransform)>,
    library: Query<Entity, With<PropLibrary>>,
    mut placed_nodes: Query<(&mut Transform, &mut Visibility), Without<PropReplacement>>,
) {
    let Some(mut view) = view else { return };
    let c = &world.0.combat;
    if c.props.is_empty() {
        return;
    }
    if view.nodes.is_empty() {
        for (e, n) in &names {
            if let Some(i) = prop_index(n.as_str()) {
                view.nodes.entry(i).or_default().push(e);
            }
        }
        if view.nodes.is_empty() {
            return; // the stage hasn't loaded yet
        }
    }
    let lib_root = library.iter().next();
    let a = fixed.overstep_fraction();
    for (k, prop) in c.props.iter().enumerate() {
        let i = prop.index;
        let (was, replacement) = view.shown.get(&i).copied().unwrap_or((PropState::Intact, None));
        let nodes = view.nodes.get(&i).cloned().unwrap_or_default();
        // Thrown: the placed model follows its shot, tumbling.
        if prop.state == PropState::Thrown {
            if let Some(p) = c.projectiles.iter().find(|p| p.prop == Some(k)) {
                let at = p.prev_pos.lerp(p.pos, a);
                let centre = prop.centre();
                let spin = Quat::from_rotation_x(time.elapsed_secs() * 6.0) * Quat::from_rotation_y(time.elapsed_secs() * 4.0);
                let m = Mat4::from_translation(at) * Mat4::from_quat(spin) * Mat4::from_translation(-centre);
                for &e in &nodes {
                    if let Ok((mut t, mut vis)) = placed_nodes.get_mut(e) {
                        *t = Transform::from_matrix(m);
                        *vis = Visibility::Inherited;
                    }
                }
                if std::env::var_os("PD_PROP_LOG").is_some() {
                    eprintln!("[props] object {i} flying: shot at {at:?}, its centre drawn at {:?}", m.transform_point3(centre));
                }
            }
        }
        if was == prop.state {
            continue;
        }
        // PD_PROP_LOG=1: each object's state changes (visual checks).
        if std::env::var_os("PD_PROP_LOG").is_some() {
            eprintln!("[props] t {:.2} object {i} {was:?} -> {:?} at {:?} ({} nodes)", time.elapsed_secs(), prop.state, prop.centre(), nodes.len());
        }
        // A replacement model for the new state, if it has one.
        let wanted = match prop.state {
            PropState::Damaged => Some("damaged"),
            PropState::Broken => Some("broken"),
            _ => None,
        };
        let mut new_replacement = None;
        if let (Some(state), Some(placed)) = (wanted, view.placed.get(&i)) {
            let group = format!("type{:02}_{state}", placed.ty);
            let Some(root) = lib_root else { continue };
            let Some(group_e) = names.iter().find(|(e, n)| n.as_str() == group && is_under(&children, root, *e)).map(|(e, _)| e) else {
                continue; // library not loaded yet: try again next frame
            };
            let parent = commands.spawn((Transform::from_matrix(placed.matrix), Visibility::Visible, PropReplacement, MatchScene)).id();
            for e in descendants(&children, group_e) {
                if let Ok((mesh, mat, g)) = meshes.get(e) {
                    commands.spawn((mesh.clone(), mat.clone(), g.compute_transform(), ChildOf(parent)));
                }
            }
            new_replacement = Some(parent);
        }
        if let Some(r) = replacement {
            commands.entity(r).despawn();
        }
        let show_placed = matches!(prop.state, PropState::Intact | PropState::Thrown) || (wanted.is_some() && new_replacement.is_none() && prop.state == PropState::Damaged);
        for &e in &nodes {
            if let Ok((mut t, mut vis)) = placed_nodes.get_mut(e) {
                *vis = if show_placed { Visibility::Inherited } else { Visibility::Hidden };
                if prop.state == PropState::Intact {
                    *t = Transform::IDENTITY;
                }
            }
        }
        view.shown.insert(i, (prop.state, new_replacement));
    }
}

fn is_under(children: &Query<&Children>, root: Entity, e: Entity) -> bool {
    descendants(children, root).contains(&e)
}

fn descendants(children: &Query<&Children>, root: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Ok(c) = children.get(e) {
            for &ch in c {
                out.push(ch);
                stack.push(ch);
            }
        }
    }
    out
}
