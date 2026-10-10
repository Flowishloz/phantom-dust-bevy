//! `--fx eft0042 [--fx-slot 0] [--fx-frame 12]`: play one skill effect slot
//! at the character (looping, or frozen at a frame for screenshots).
//! Playback itself is the game's `effects.rs`.

use crate::effects::{self, FxLibrary};
use bevy::prelude::*;

#[derive(Resource)]
pub struct FxSetup {
    pub name: String,
    pub pos: Vec3,
    pub slot: usize,
    pub frame: Option<f32>,
}

pub fn spawn(mut commands: Commands, assets: Res<AssetServer>, s: Option<Res<FxSetup>>, lib: Option<ResMut<FxLibrary>>) {
    let (Some(s), Some(mut lib)) = (s, lib) else { return };
    if effects::play(&mut commands, &assets, &mut lib, &s.name, s.slot, Transform::from_translation(s.pos), true, s.frame).is_none() {
        error!("effect {} has nothing in slot {} (or cache/fx is missing it)", s.name, s.slot);
    }
}
