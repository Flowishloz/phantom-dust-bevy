//! The Options overlay's SOUND tab (`menu.rs`): master, music and effects volume.
//!
//! Every sound the game starts carries a [`Mixed`] tag: its channel (music, or effects =
//! fight sounds, voices and menu cues) and the level it was always played at (`sfx.rs`'s
//! `VOL_SFX`, `frontend::sound`'s `VOL`, music 1.0). This module is the one place that
//! multiplies the player's volume in:
//!
//! * a sound that has just been spawned gets `base x gain` in its `PlaybackSettings`
//!   before Bevy's audio systems start it (they run in `PostUpdate` after transform
//!   propagation; [`mix`] runs before it);
//! * when a volume slider moves, every sound already playing is set to its new level at
//!   once (`AudioSink::set_volume`, times `GlobalVolume` the way Bevy's own start does,
//!   so `PD_MUTE` stays mute).
//!
//! At the defaults (100 / 100 / 100) the gain is exactly 1.0 and nothing is touched: the
//! game is as loud as it always was.

use bevy::audio::{AudioSink, AudioSinkPlayback, GlobalVolume, Volume};
use bevy::prelude::*;

/// Which volume slider (besides master) a sound follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Background music (`music.rs`).
    Music,
    /// Sound effects, battle voices and menu cues (`sfx.rs`, `frontend::sound`).
    Effects,
}

/// A sound's channel and the level it plays at with every volume at 100.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Mixed {
    pub channel: Channel,
    pub base: f32,
}

impl Mixed {
    pub fn music(base: f32) -> Self {
        Mixed { channel: Channel::Music, base }
    }

    pub fn effects(base: f32) -> Self {
        Mixed { channel: Channel::Effects, base }
    }
}

/// A volume slider's value (0-100) as a loudness factor. Squared (an "audio taper"): 50
/// sounds about half as loud rather than barely quieter; 100 is exactly 1.0, 0 is silent.
pub fn slider_gain(pct: f32) -> f32 {
    let f = (pct / 100.0).clamp(0.0, 1.0);
    f * f
}

/// The factor a channel's sounds are played at: master times the channel's own slider.
pub fn gain(settings: &crate::menu::Settings, channel: Channel) -> f32 {
    let own = match channel {
        Channel::Music => settings.music_vol,
        Channel::Effects => settings.effects_vol,
    };
    slider_gain(settings.master_vol) * slider_gain(own)
}

/// The level a tagged sound should play at now.
pub fn level(settings: Option<&crate::menu::Settings>, m: Mixed) -> f32 {
    settings.map_or(m.base, |s| m.base * gain(s, m.channel))
}

pub fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, mix.before(bevy::transform::TransformSystems::Propagate));
}

/// See the module doc: new sounds start at their mixed level; a slider change reaches
/// every sound already playing.
pub fn mix(settings: Option<Res<crate::menu::Settings>>, global: Option<Res<GlobalVolume>>, mut last: Local<Option<[f32; 3]>>, mut sounds: Query<(Ref<Mixed>, &mut PlaybackSettings, Option<&mut AudioSink>)>) {
    let s = settings.as_deref();
    let now = s.map_or([100.0; 3], |s| [s.master_vol, s.music_vol, s.effects_vol]);
    let moved = *last != Some(now);
    *last = Some(now);
    let global = global.map_or(1.0, |g| g.volume.to_linear());
    for (m, mut playback, sink) in &mut sounds {
        if !(moved || m.is_added()) {
            continue;
        }
        let want = Volume::Linear(level(s, *m));
        if playback.volume != want {
            playback.volume = want;
        }
        // A sound already playing: re-levelled when a slider moved.
        if let Some(mut sink) = sink {
            if moved {
                sink.set_volume(Volume::Linear(want.to_linear() * global));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> crate::menu::Settings {
        let mut s = crate::menu::Settings::load(std::env::temp_dir().join(format!("pd_mixer_unit_{}.json", std::process::id())));
        s.disable_saving();
        s
    }

    #[test]
    fn the_default_volumes_leave_every_sound_exactly_as_loud_as_before() {
        let s = settings();
        assert_eq!((s.master_vol, s.music_vol, s.effects_vol), (100.0, 100.0, 100.0));
        assert_eq!(gain(&s, Channel::Music), 1.0);
        assert_eq!(gain(&s, Channel::Effects), 1.0);
        // The game's own levels (sfx 0.55 / 0.25 / 0.7 / 0.38, menu 0.45, music 1.0) come out bit for bit.
        for base in [0.55f32, 0.25, 0.7, 0.38, 0.45, 0.35, 1.0] {
            assert_eq!(level(Some(&s), Mixed::effects(base)).to_bits(), base.to_bits());
            assert_eq!(level(Some(&s), Mixed::music(base)).to_bits(), base.to_bits());
        }
    }

    #[test]
    fn the_sliders_scale_their_own_channel_and_master_scales_both() {
        let mut s = settings();
        assert_eq!((slider_gain(0.0), slider_gain(50.0), slider_gain(100.0), slider_gain(150.0)), (0.0, 0.25, 1.0, 1.0));
        s.music_vol = 50.0;
        assert_eq!(level(Some(&s), Mixed::music(1.0)), 0.25);
        assert_eq!(level(Some(&s), Mixed::effects(0.55)), 0.55, "music does not touch effects");
        s.effects_vol = 0.0;
        assert_eq!(level(Some(&s), Mixed::effects(0.55)), 0.0, "0 is silent");
        s.music_vol = 100.0;
        s.effects_vol = 100.0;
        s.master_vol = 50.0;
        assert_eq!((level(Some(&s), Mixed::music(1.0)), level(Some(&s), Mixed::effects(0.4))), (0.25, 0.1));
    }

    /// Through the real `mix` system: a sound spawned at its base level starts at the mixed one,
    /// and moving a slider re-levels sounds that are waiting to start (live sinks need an audio
    /// device, which the test machine has none of: UNCONFIRMED by ear).
    #[test]
    fn the_mix_system_levels_new_sounds_and_follows_the_sliders() {
        let mut app = App::new();
        app.insert_resource(settings()).insert_resource(GlobalVolume::default()).add_systems(Update, mix);
        let fx = app.world_mut().spawn((PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.55)), Mixed::effects(0.55))).id();
        let bgm = app.world_mut().spawn((PlaybackSettings::LOOP, Mixed::music(1.0))).id();
        let untagged = app.world_mut().spawn(PlaybackSettings::ONCE.with_volume(Volume::Linear(0.3))).id();
        app.update();
        let vol = |app: &App, e: Entity| app.world().get::<PlaybackSettings>(e).unwrap().volume.to_linear();
        assert_eq!((vol(&app, fx), vol(&app, bgm), vol(&app, untagged)), (0.55, 1.0, 0.3), "defaults: unchanged");
        app.world_mut().resource_mut::<crate::menu::Settings>().music_vol = 50.0;
        app.update();
        assert_eq!((vol(&app, fx), vol(&app, bgm)), (0.55, 0.25));
        app.world_mut().resource_mut::<crate::menu::Settings>().master_vol = 0.0;
        app.update();
        assert_eq!((vol(&app, fx), vol(&app, bgm), vol(&app, untagged)), (0.0, 0.0, 0.3), "master silences both, not untagged sounds");
        // A sound started now gets the current mix at once.
        let late = app.world_mut().spawn((PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.7)), Mixed::effects(0.7))).id();
        app.world_mut().resource_mut::<crate::menu::Settings>().master_vol = 100.0;
        app.world_mut().resource_mut::<crate::menu::Settings>().effects_vol = 50.0;
        app.update();
        assert_eq!(vol(&app, late), 0.7 * 0.25);
    }
}
