//! Short menu cues, from the game's own `common` sound bank -- real decoded
//! game audio, not synthesised.
//!
//! The ids are the program's own (audits/P0-SOUND.md section 5 and D9; every
//! menu screen of the original plays them through `FUN_140276330(0, id, id,
//! ...)`): **3** = cursor move (any direction input, `0x14016e333`,
//! `0x1401700a6`, `0x1401714f0` ...), **0** = select / confirm (`0x14016a3b5`,
//! the pick in `FUN_14016e180`), **1** = back / cancel (`0x14016e524`,
//! `0x14016e6d1`), **4** = switch list (`0x140162002`, `FUN_140161fa0`),
//! **34** = a spinner / value tick (`0x14017587e`, `0x14018eba3`). They
//! replace the earlier waveform-survey guesses 60 / 116 / 154, which had no
//! program basis and were never ear-checked. Unpause (the pause menu closing)
//! stays common/1, Laurence's ear result; it is the same clip as Back.
use crate::Config;
use bevy::audio::Volume;
use bevy::prelude::*;

const MOVE_ID: u32 = 3;
const CONFIRM_ID: u32 = 0;
const BACK_ID: u32 = 1;
const LIST_ID: u32 = 4;
const SPINNER_ID: u32 = 34;
/// Text-entry delete (`FUN_14018eb40`, `0x14018ebXX` `0xb`) and the disabled-item buzz (generic list `FUN_14024b000`, id 2).
const DELETE_ID: u32 = 11;
const DENIED_ID: u32 = 2;
/// O5 (docs/FRONTIER.md, Laurence 2026-09-29): exiting the pause menu in the
/// original plays common-bank id 1 (`sfx/common/1.wav`) -- identified by ear.
/// Heard only when player 2 unpauses, so it is unconfirmed whether P1's close
/// sounds the same; our overlay (`menu::MenuState`) is a single shared
/// resource toggled by Esc or any pad's Start -- not per-player -- so either
/// player's close plays this. PROTECTED: do not change.
const UNPAUSE_ID: u32 = 1;

const VOL: f32 = 0.45;

#[derive(Resource)]
struct MenuSfx {
    move_: Handle<AudioSource>,
    confirm: Handle<AudioSource>,
    back: Handle<AudioSource>,
    unpause: Handle<AudioSource>,
    list: Handle<AudioSource>,
    spinner: Handle<AudioSource>,
    delete: Handle<AudioSource>,
    denied: Handle<AudioSource>,
    /// False (no cache/sfx/common) plays nothing, silently.
    has: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cue {
    Move,
    Confirm,
    Back,
    /// The in-match pause/settings overlay closing (unpause). Pushed by the
    /// overlay's owner, not by the screens; kept here so the id lives with
    /// the other menu ids.
    Unpause,
    /// Switching between lists / tabs (`FUN_140161fa0`): the Options overlay's LB / RB
    /// tab switch (`menu.rs`) plays it.
    List,
    /// Text-entry cursor / character step (`FUN_14018eb40`: common/34). NOT left/right on a list row: the generic list (`FUN_14024b000`) is silent there.
    Spinner,
    /// Text-entry delete (common/11).
    Delete,
    /// A disabled list item pressed (common/2, `FUN_14024b000`).
    Denied,
}

/// The common-bank id a cue plays, as a pure function so tests pin it.
/// Unpause is 1 (Laurence's O5 ear-identification); the rest are the
/// program's menu ids (module doc).
pub fn cue_bank_id(cue: Cue) -> u32 {
    match cue {
        Cue::Move => MOVE_ID,
        Cue::Confirm => CONFIRM_ID,
        Cue::Back => BACK_ID,
        Cue::Unpause => UNPAUSE_ID,
        Cue::List => LIST_ID,
        Cue::Spinner => SPINNER_ID,
        Cue::Delete => DELETE_ID,
        Cue::Denied => DENIED_ID,
    }
}

/// Screens push cues here instead of spawning audio entities themselves;
/// `play` drains it once a frame.
#[derive(Resource, Default)]
pub struct MenuSfxQueue(pub Vec<Cue>);

/// Cues for a frame's [`super::input::MenuAxis`]: confirm and cancel win
/// over a direction held the same frame (both can't usefully fire at once).
pub fn cues_for(a: &super::input::MenuAxis) -> impl Iterator<Item = Cue> {
    let cue = if a.confirm || a.start {
        Some(Cue::Confirm)
    } else if a.cancel {
        Some(Cue::Back)
    } else if a.up || a.down {
        // READ `FUN_14024b000`: only up/down play common/3 (and only when the index changes); left/right on a list row is silent.
        Some(Cue::Move)
    } else {
        None
    };
    cue.into_iter()
}

pub fn plugin(app: &mut App) {
    app.init_resource::<MenuSfxQueue>().add_systems(Startup, load).add_systems(Update, play);
}

fn load(mut commands: Commands, cfg: Res<Config>, assets: Res<AssetServer>) {
    let has = |id: u32| cfg.cache.join(format!("sfx/common/{id}.wav")).exists();
    let handle = |id: u32| assets.load(format!("sfx/common/{id}.wav"));
    commands.insert_resource(MenuSfx {
        move_: handle(MOVE_ID),
        confirm: handle(CONFIRM_ID),
        back: handle(BACK_ID),
        unpause: handle(UNPAUSE_ID),
        list: handle(LIST_ID),
        spinner: handle(SPINNER_ID),
        delete: handle(DELETE_ID),
        denied: handle(DENIED_ID),
        has: [MOVE_ID, CONFIRM_ID, BACK_ID, UNPAUSE_ID, LIST_ID, SPINNER_ID, DELETE_ID, DENIED_ID].into_iter().all(has),
    });
}

fn play(mut commands: Commands, mut queue: ResMut<MenuSfxQueue>, sfx: Option<Res<MenuSfx>>) {
    let Some(sfx) = sfx else {
        queue.0.clear();
        return;
    };
    if !sfx.has {
        queue.0.clear();
        return;
    }
    // Same convention as `sfx.rs`: PD_SFX_LOG=1 logs every sound started, so a
    // headless --screenshot run can prove a cue fired without ears.
    let log = std::env::var_os("PD_SFX_LOG").is_some();
    for cue in queue.0.drain(..) {
        let h = match cue {
            Cue::Move => sfx.move_.clone(),
            Cue::Confirm => sfx.confirm.clone(),
            Cue::Back => sfx.back.clone(),
            Cue::Unpause => sfx.unpause.clone(),
            Cue::List => sfx.list.clone(),
            Cue::Spinner => sfx.spinner.clone(),
            Cue::Delete => sfx.delete.clone(),
            Cue::Denied => sfx.denied.clone(),
        };
        if log {
            info!("menu-sfx: play sfx/common/{}.wav ({cue:?})", cue_bank_id(cue));
        }
        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(VOL)), crate::mixer::Mixed::effects(VOL)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpause_plays_common_bank_id_1() {
        assert_eq!(cue_bank_id(Cue::Unpause), 1);
    }

    /// The program's menu ids (P0-SOUND.md D9): 3 move (`0x14016e333`), 0
    /// select (`0x14016a3b5`), 1 back (`0x14016e524`), 4 list (`0x140162002`),
    /// 34 spinner (`0x14017587e`).
    #[test]
    fn menu_ids_are_the_programs_own() {
        assert_eq!(cue_bank_id(Cue::Move), 3);
        assert_eq!(cue_bank_id(Cue::Confirm), 0);
        assert_eq!(cue_bank_id(Cue::Back), 1);
        assert_eq!(cue_bank_id(Cue::List), 4);
        assert_eq!(cue_bank_id(Cue::Spinner), 34);
    }

    /// Back and Unpause are the same program id (1): the pause menu closes
    /// with the cancel sound, which is exactly what the ear check found. Every
    /// other cue has a clip of its own.
    #[test]
    fn only_back_and_unpause_share_a_clip() {
        let ids = [Cue::Move, Cue::Confirm, Cue::Back, Cue::List, Cue::Spinner].map(cue_bank_id);
        let mut uniq = ids.to_vec();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), ids.len());
        assert_eq!(cue_bank_id(Cue::Unpause), cue_bank_id(Cue::Back));
    }
}
