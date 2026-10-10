//! Background music the way the original plays it (docs/PIPELINE.md "Music").
//!
//! READ from `PDUWP.exe` and the game's own scripts:
//!  * `FUN_14019cdf0` plays a track by its number in the BGM table (`0x1403e8c00`:
//!    name pointer + flags; the track number is the file's number, `04_Highway` = 4).
//!    Asking for the track already playing does nothing.
//!  * Menus call it from their scripts (`g1.f16`, `Menu/*_en.ssb`): the title / main
//!    menu 31, the versus menus (set-up, character, stage) 34, the arsenal editor 29,
//!    options 33, the shop 60. The battle result script plays 37 (win) or 38 (lose).
//!  * A battle picks its track once, when it starts (`gamestart` in the quest script
//!    `Event/en/questNNN.ssb`, every one carries the same table): the stage number looks
//!    up a track number (table below); for Lane and Sein (16, 19) a random 0-2 is added
//!    (`g4.f32(3)` = `rand() * 3 >> 15`), i.e. one of the three variants. Nothing in the
//!    battle changes the track afterwards (no round, low-health or time-out switch: the
//!    only music calls in the script are in `gamestart`).
//!  * A track plays its intro once, then repeats the loop region its STX header gives
//!    (`pd_formats::stx::Music::loop_frames`); the win / lose / ending jingles have no
//!    loop and play once. Tracks never advance to another track by themselves.
//!
//! Volume: each track is tagged `mixer::Mixed::music(1.0)`; the Options overlay's master and
//! music sliders scale it there (`mixer.rs`), live.
//!
//! The tracks come from `cache/music/<name>.pdm` (`pd_convert musictracks`). Without
//! them the match falls back to the old `stages.json` track, the menus stay silent.

use crate::frontend::state::AppState;
use crate::{Config, World};
use bevy::asset::{io::Reader, Asset, AssetLoader, LoadContext};
use bevy::audio::{AddAudioSource, ChannelCount, Decodable, SampleRate, Source};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use std::sync::Arc;
use std::time::Duration;

/// Track numbers by stage number (`quest000.ssb` at 0x208, the same table in all 138 quests
/// that carry it): 0 = none. 16 and 19 get a random 0-2 added.
pub const STAGE_TRACKS: [u8; 29] = [0, 24, 5, 1, 8, 14, 11, 16, 19, 0, 0, 24, 6, 2, 7, 15, 10, 16, 19, 0, 0, 24, 4, 3, 9, 13, 12, 16, 19];

pub const TITLE: u32 = 31;
pub const VERSUS_MENU: u32 = 34;
pub const ARSENAL: u32 = 29;
pub const WIN: u32 = 37;
pub const LOSE: u32 = 38;

/// The track a battle on stage `stage_no` plays; `roll` is the random 0-2 (any number: reduced).
pub fn stage_track(stage_no: usize, roll: u32) -> Option<u32> {
    let t = *STAGE_TRACKS.get(stage_no)? as u32;
    match t {
        0 => None,
        16 | 19 => Some(t + roll % 3),
        _ => Some(t),
    }
}

/// "st06" -> 6.
pub fn stage_number(id: &str) -> Option<usize> {
    id.strip_prefix("st")?.get(..2)?.parse().ok()
}

/// The file stem of track `n` (the BGM file names, `BGM/<n>_<Name>.stx`).
pub fn track_stem(n: u32) -> Option<&'static str> {
    Some(match n {
        1 => "01_ShoppingMall",
        2 => "02_ShoppingMall1",
        3 => "03_ShoppingMall2",
        4 => "04_Highway",
        5 => "05_Highway1",
        6 => "06_Highway2",
        7 => "07_Panorama",
        8 => "08_Panorama1",
        9 => "09_Panorama2",
        10 => "10_YojoPlant",
        11 => "11_YojoPlant1",
        12 => "12_YojoPlant2",
        13 => "13_JuryokuHanten",
        14 => "14_JuryokuHanten1",
        15 => "15_JuryokuHanten2",
        16 => "16_KiokuNoMachi",
        17 => "17_KiokuNoMachi1",
        18 => "18_KiokuNoMachi2",
        19 => "19_Iseki",
        20 => "20_Iseki1",
        21 => "21_Iseki2",
        24 => "24_LastBoss",
        29 => "29_DeckEdit",
        31 => "31_Title",
        34 => "34_TaisenSelect",
        37 => "37_BattleWin",
        38 => "38_BattleLose",
        _ => return None,
    })
}

/// Decoded music with its loop region.
#[derive(Asset, TypePath, Clone)]
pub struct LoopMusic {
    samples: Arc<[i16]>,
    rate: u32,
    /// Frames `(start, end)`; `None` = plays once.
    looped: Option<(usize, usize)>,
}

/// Plays the intro, then the loop region over and over.
pub struct LoopDecoder {
    samples: Arc<[i16]>,
    rate: u32,
    looped: Option<(usize, usize)>,
    /// Index into `samples` (interleaved).
    pos: usize,
}

impl Iterator for LoopDecoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some((start, end)) = self.looped {
            if self.pos >= end * 2 {
                self.pos = start * 2;
            }
        }
        let v = *self.samples.get(self.pos)?;
        self.pos += 1;
        Some(v as f32 / 32768.0)
    }
}

impl Source for LoopDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).expect("2 channels")
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.rate.max(1)).expect("rate")
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for LoopMusic {
    type Decoder = LoopDecoder;
    fn decoder(&self) -> LoopDecoder {
        LoopDecoder { samples: self.samples.clone(), rate: self.rate, looped: self.looped, pos: 0 }
    }
}

#[derive(Default, TypePath)]
pub struct LoopMusicLoader;

impl AssetLoader for LoopMusicLoader {
    type Asset = LoopMusic;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(&self, reader: &mut dyn Reader, _settings: &(), _ctx: &mut LoadContext<'_>) -> Result<LoopMusic, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let (rate, looped, samples) = pd_formats::stx::parse_pdm(&bytes).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "not a PDM1 music file"))?;
        Ok(LoopMusic { samples: samples.into(), rate, looped })
    }

    fn extensions(&self) -> &[&str] {
        &["pdm"]
    }
}

/// What is playing and what the current match wants.
#[derive(Resource, Default)]
pub struct Music {
    /// The track number playing (or asked for and missing from the cache).
    current: Option<u32>,
    entity: Option<Entity>,
    /// The match's track, chosen when the match starts.
    match_track: Option<u32>,
    /// The old per-stage WAV (`stages.json`), for a cache without `.pdm` files.
    match_fallback: Option<String>,
    /// A match just started: restart even if the same track is up.
    fresh: bool,
}

impl Music {
    /// A battle starts on `stage`: pick its track (once, here) and start it afresh.
    pub fn start_match(&mut self, stage: &str, fallback: String) {
        let roll = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() / 1000);
        self.match_track = stage_number(stage).and_then(|n| stage_track(n, roll));
        self.match_fallback = Some(fallback);
        self.current = None;
        self.fresh = true;
    }
}

pub fn build(app: &mut App) {
    app.init_resource::<Music>().add_audio_source::<LoopMusic>().init_asset_loader::<LoopMusicLoader>().add_systems(Update, drive_music);
}

/// The track the program plays on `state`.
fn wanted(state: AppState, music: &Music, world: &World) -> Option<u32> {
    match state {
        AppState::Title | AppState::Online => Some(TITLE),
        AppState::Setup => Some(VERSUS_MENU),
        AppState::Arsenal => Some(ARSENAL),
        AppState::Match => music.match_track,
        // `battleend_en.ssb` plays 37 on a win and 38 on a loss; a draw is a loss here (UNCONFIRMED for versus).
        AppState::Result => Some(if matches!(crate::frontend::result::results(&world.0.combat).headline, crate::frontend::result::Headline::Draw) { LOSE } else { WIN }),
    }
}

fn drive_music(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<Config>, state: Res<State<AppState>>, world: Res<World>, mut music: ResMut<Music>) {
    let want = wanted(*state.get(), &music, &world);
    if want == music.current && !music.fresh {
        return;
    }
    music.fresh = false;
    if let Some(e) = music.entity.take() {
        commands.entity(e).despawn();
    }
    music.current = want;
    let Some(n) = want else { return };
    let file = track_stem(n).map(|s| format!("music/{s}.pdm"));
    if let Some(f) = file.filter(|f| cfg.cache.join(f).exists()) {
        let id = commands.spawn((AudioPlayer::<LoopMusic>(assets.load(f)), PlaybackSettings::ONCE, crate::mixer::Mixed::music(1.0))).id();
        music.entity = Some(id);
    } else if *state.get() == AppState::Match {
        // No converted tracks: the old behaviour.
        let path = music.match_fallback.clone().unwrap_or_else(|| "music.wav".into());
        music.entity = Some(commands.spawn((AudioPlayer::new(assets.load(path)), PlaybackSettings::LOOP, crate::mixer::Mixed::music(1.0))).id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_play_the_tracks_the_quest_script_table_gives() {
        // Highway: Spiral 5, Lightning 6, Sunlight 4 (not ascending).
        assert_eq!([2, 12, 22].map(|s| stage_track(s, 0)), [Some(5), Some(6), Some(4)]);
        // Palace 1 2 3; Panorama 8 7 9; City 14 15 13; Refinery 11 10 12.
        assert_eq!([3, 13, 23].map(|s| stage_track(s, 0)), [Some(1), Some(2), Some(3)]);
        assert_eq!([4, 14, 24].map(|s| stage_track(s, 0)), [Some(8), Some(7), Some(9)]);
        assert_eq!([5, 15, 25].map(|s| stage_track(s, 0)), [Some(14), Some(15), Some(13)]);
        assert_eq!([6, 16, 26].map(|s| stage_track(s, 0)), [Some(11), Some(10), Some(12)]);
        // Dummy slots play nothing.
        assert_eq!(stage_track(9, 0), None);
        assert_eq!(stage_track(40, 0), None);
    }

    #[test]
    fn lane_and_sein_pick_one_of_three_variants() {
        for stage in [7, 17, 27] {
            let all: Vec<u32> = (0..3).filter_map(|r| stage_track(stage, r)).collect();
            assert_eq!(all, [16, 17, 18], "stage {stage}");
        }
        for stage in [8, 18, 28] {
            let all: Vec<u32> = (0..3).filter_map(|r| stage_track(stage, r)).collect();
            assert_eq!(all, [19, 20, 21], "stage {stage}");
        }
        assert_eq!(stage_track(7, 4), Some(17), "any roll is reduced");
    }

    #[test]
    fn every_track_a_stage_can_play_has_a_file_name() {
        for s in 0..STAGE_TRACKS.len() {
            for r in 0..3 {
                if let Some(t) = stage_track(s, r) {
                    assert!(track_stem(t).is_some(), "stage {s} track {t}");
                }
            }
        }
        assert_eq!(stage_number("st06"), Some(6));
        assert_eq!(stage_number("st09_01"), Some(9));
        assert_eq!(stage_number("pc00a"), None);
    }

    #[test]
    fn the_intro_plays_once_then_the_loop_repeats() {
        // Frames 0..6, loop frames 2..4: 0 1 | 2 3 | 2 3 | 2 3 ...
        let samples: Vec<i16> = (0..6).flat_map(|f| [f * 1000, f * 1000]).collect();
        let m = LoopMusic { samples: samples.into(), rate: 44100, looped: Some((2, 4)) };
        let frames: Vec<i32> = m.decoder().step_by(2).take(8).map(|v| (v * 32768.0).round() as i32 / 1000).collect();
        assert_eq!(frames, [0, 1, 2, 3, 2, 3, 2, 3]);
    }

    #[test]
    fn a_jingle_plays_once_and_ends() {
        let m = LoopMusic { samples: vec![1i16, 2, 3, 4].into(), rate: 44100, looped: None };
        assert_eq!(m.decoder().count(), 4);
    }
}
