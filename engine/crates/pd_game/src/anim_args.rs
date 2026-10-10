//! Clip-start arguments, exactly as the program passes them.
//!
//! The original starts every animation clip through one of two helpers
//! (`FUN_140257310`, `FUN_1402573e0`; `engine/docs/audits/P0-ANIMSTATES.md`).
//! Their arguments are, in order: controller, clip, **loop**, **blend
//! frames** (`0x140257310`, 4th) or **seek frame** (`0x1402573e0`, 4th, with
//! blend last on the stack), **root-motion flag**, **hold-loop start**
//! (`+0xd4`), **hold-loop end** (`+0xd8`). A blend <= 0 means "full weight at
//! once" (an instant cut, no cross-fade); ordinary starts pass 5.0
//! (`0x1402f7f18`), the queued-clip / "no blend" constant is -1.0
//! (`0x1402f82e0`). Ghidra's export loses these float and stack arguments, so
//! every row below was re-read from the instructions (capstone over
//! `PDUWP.exe`; image base `0x140000000`).
//!
//! Until this table existed the renderer cross-faded every clip change over
//! 5 frames except the locked-on pivot hand-off and WALK <-> RUN
//! (MOVEMENT-AUDIT.md "Locked-on lane"). The rule now comes from the state
//! being entered, as in the program. Units: a blend or seek "frame" is one
//! 30 Hz animation frame (`FRAME_SECONDS`).
//!
//! Root motion (`ClipStart::root_motion`) is recorded for completeness; the
//! sim does not consume clip root curves yet (audit D8, Lane 4).

use crate::sim::Motion;
use std::collections::HashMap;
use std::time::Duration;

/// One 30 Hz animation frame in seconds.
pub const FRAME_SECONDS: f32 = 1.0 / 30.0;

/// The ordinary blend argument: 5 frames (`0x1402f7f18` = 5.0).
pub const BLEND_FRAMES: f32 = 5.0;

/// The "no blend" argument (`0x1402f82e0` = -1.0), also what a blend of 0
/// means to `FUN_140297d80`: full weight at once.
pub const NO_BLEND: f32 = -1.0;

/// Everything a clip start passes, for one state entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipStart {
    /// Cross-fade length in frames; `<= 0` = an instant cut.
    pub blend_frames: f32,
    /// The clip loops (`FUN_140257310` 3rd argument).
    pub looped: bool,
    /// Frame the clip starts at (`FUN_1402573e0`'s 4th argument; 0 otherwise).
    pub seek_frames: f32,
    /// Hold loop `(start, end)` in frames (`+0xd4`, `+0xd8`): once the clip
    /// reaches `start` it cycles `start..end` until the state ends.
    pub hold_loop: Option<(f32, f32)>,
    /// Root-motion flag (5th argument): the clip's own x/z drift moves the body.
    pub root_motion: bool,
    /// WALK <-> RUN style swap: the new clip starts at the old clip's relative
    /// phase (`0x14025ad50`: new time = old time / old length x new length).
    pub keep_phase: bool,
}

impl ClipStart {
    const fn new(blend_frames: f32, looped: bool, root_motion: bool) -> Self {
        ClipStart { blend_frames, looped, seek_frames: 0.0, hold_loop: None, root_motion, keep_phase: false }
    }

    /// Whether the start is an instant cut.
    pub fn is_cut(&self) -> bool {
        self.blend_frames <= 0.0
    }

    /// The cross-fade to hand the animation transitions (zero = cut).
    pub fn fade(&self) -> Duration {
        if self.is_cut() { Duration::ZERO } else { Duration::from_secs_f32(self.blend_frames * FRAME_SECONDS) }
    }

    /// The start frame in seconds.
    pub fn seek_seconds(&self) -> f32 {
        self.seek_frames * FRAME_SECONDS
    }
}

/// The kind of clip start a state entry makes. One arm per call site read in
/// the exe; the address is on each arm of `start_args`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StartKind {
    /// A player's cast (states 10/11). `blend_frames` is the per-character
    /// float (charaset record `+0x16`; 0 for all 11 playable fighters),
    /// `seek_frames` the combo offset (0 outside a combo chain), `hold_loop`
    /// the charge skill's `(w39, w40)` frames.
    Cast { blend_frames: f32, seek_frames: f32, hold_loop: Option<(f32, f32)> },
    /// A charge released: the clip restarts at the row's w40 frame.
    ChargeRelease { seek_frames: f32 },
    /// A hit reaction that plays one clip (reaction-row kinds 0 to 3), or the
    /// KO pose.
    Reaction,
    /// Reaction-row kind 11 (the 5-tick flinch): loops, blend = row w43.
    Flinch { blend_frames: f32 },
    /// Knocked flying (reaction-row kinds 4, 5, 6): the flight clip loops.
    KnockFlight,
    /// A landing clip: JUMP_LAND*, and BLOW_LAND_* when a knocked body lands.
    Landing,
    /// Getting up from a knock-down (DOWN_*_STAND).
    GetUp,
    /// Ground dodge / Dash skill (state 3).
    GroundDodge { seek_frames: f32 },
    /// Jump wind-up clip (`0x140257b90`): the Jump skill's rooted start
    /// (`Player::jump_windup`) before the take-off.
    JumpStart,
    /// Jump take-off, the up clip (`0x14025a0b0`).
    TakeOff,
    /// The falling clip (`0x14025a1c0`).
    Fall,
    /// Anything that returns to a neutral clip when an action ends.
    ReturnIdle,
    /// A WALK/RUN (mood) gait swap while moving.
    GaitSwap,
    /// The queued clip that follows a finished clip (`0x1402576bc`).
    Queued,
    /// FLOAT_IN, the first clip of a float (`0x14025a330`).
    FloatIn,
    /// FLOAT_WAIT / FLOAT_MOVE chosen by the float state (`0x14025b170`).
    FloatHold,
}

/// The start arguments for `kind`.
pub fn start_args(kind: StartKind) -> ClipStart {
    match kind {
        // `0x14025a3e0` -> `0x14025a737 call 0x1402573e0`: loop 0 (`xor r8d,
        // r8d` at `0x14025a721`), seek = combo offset (`0x14025a60c`, else 0),
        // root = row w1 (`[rsi+rbx+2]`), hold-loop = (w39, w40) only while the
        // charge flag `[rdi+0x68] == 1` (`0x14025a699`), else -1.0, blend =
        // the per-character float read at `0x14025a692`.
        StartKind::Cast { blend_frames, seek_frames, hold_loop } => ClipStart { seek_frames, hold_loop, ..ClipStart::new(blend_frames, false, true) },
        // `0x14025c8ae call 0x1402573e0`: loop 0, seek = w40 (`[rax+0x50]`),
        // blend xmm6 = 0 (`xorps` at `0x14025c739`), root = w1, hold -1.0.
        StartKind::ChargeRelease { seek_frames } => ClipStart { seek_frames, ..ClipStart::new(0.0, false, true) },
        // `0x140258897` (kind 0), `0x1402588d6` (1), `0x140258946` (2/3),
        // `0x140258c07` / `0x140258c3f`: xmm3 = -1.0, `xor r8d, r8d` (loop 0),
        // root = row w1.
        StartKind::Reaction => ClipStart::new(NO_BLEND, false, true),
        // `0x140258de6` (kind 11): loop `mov r8d, 1`, blend = `cvtdq2ps` of the
        // row's word 43 (`[rsi+0x56]`), root = w1, hold -1.0.
        StartKind::Flinch { blend_frames } => ClipStart::new(blend_frames, true, true),
        // `0x140258aa9` (four-way flyer) and `0x140258b43` (front/back flyer):
        // blend -1.0, `mov r8d, 1` (loop), root `r14d` = 0.
        StartKind::KnockFlight => ClipStart::new(NO_BLEND, true, false),
        // `0x140254da8 call 0x140257310` in the landing handler `0x140254c80`:
        // `xorps xmm3` (blend 0), `xor r8d, r8d` (loop 0), root 1.
        StartKind::Landing => ClipStart::new(0.0, false, true),
        // `0x14025c4c0 call 0x140257310`: blend 0 (`xorps xmm3`), loop 0
        // (`xor r8d, r8d`), root 1 (`[rsp+0x20] = 1`).
        StartKind::GetUp => ClipStart::new(0.0, false, true),
        // `0x1402707bf call 0x1402573e0` in the skill-effect interpreter
        // `0x140270200` (s44 = 8): loop 0, seek = row w43 (0; `[r14+rcx+0x56]`),
        // blend xmm6 = 0 (`xorps` at `0x1402706db`), root 1.
        StartKind::GroundDodge { seek_frames } => ClipStart { seek_frames, ..ClipStart::new(0.0, false, true) },
        // `0x140257c43 call 0x140257310`: blend 5.0 (`0x140257c27`), loop 0
        // (`xor r8d, r8d`), root 1.
        StartKind::JumpStart => ClipStart::new(BLEND_FRAMES, false, true),
        // `0x14025a125 call 0x140257310` in `0x14025a0b0`: `xorps xmm3` (blend
        // 0), `xor r8d, r8d` (loop 0), root `ebp` = 0.
        StartKind::TakeOff => ClipStart::new(0.0, false, false),
        // `0x14025a2b0 call 0x140257310`: blend 5.0 (`0x14025a282`; -1.0 when
        // the state is 2, `0x14025a298`), loop 0, root `edi` = 0.
        StartKind::Fall => ClipStart::new(BLEND_FRAMES, false, false),
        // `0x14025766a` / `0x140257658` (clip end, nothing queued) and
        // `0x140254df3` (landing without a clip): blend 5.0, loop 1, root 0.
        StartKind::ReturnIdle => ClipStart::new(BLEND_FRAMES, true, false),
        // `0x14025aede`-ish swap in `0x14025ad50` (`call 0x1402573e0` at
        // `0x14025aed9`): loop 1, blend xmm6 = 0, root 0, start time = old
        // time / old length x new length.
        StartKind::GaitSwap => ClipStart { keep_phase: true, ..ClipStart::new(0.0, true, false) },
        // `0x1402576f6 call 0x140257310`: the clip queued behind a finished
        // one (the pivot's WALK/RUN, the DOWN_* loop after BLOW_LAND_*) starts
        // with `0x1402576bc` = -1.0; loop and root come from the queue.
        StartKind::Queued => ClipStart::new(NO_BLEND, true, false),
        // `0x14025a330 call 0x140257310` (clip 0x2a = FLOAT_IN): `xmm3` =
        // `DAT_1402f7f18` (5.0), loop 0 (`xor r8d`), root 0, hold -1.0;
        // queues clip 0x2b (FLOAT_WAIT) with loop 1 (`+0xe4 = 1`).
        StartKind::FloatIn => ClipStart::new(BLEND_FRAMES, false, false),
        // `0x14025b170`: `FUN_140257310(.., 0x2c or 0x2b, 1, 5.0, 0, -1, -1)`.
        StartKind::FloatHold => ClipStart::new(BLEND_FRAMES, true, false),
    }
}

/// How a locomotion clip change is played (MOVEMENT-AUDIT.md "Locked-on
/// lane"; `0x140257694`, `0x14025ad50`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipSwitch {
    /// Cross-fade over `BLEND_FRAMES` from the old clip, new clip from 0.
    Blend,
    /// Instant, new clip from 0 (the pivot's queued WALK/RUN).
    Instant,
    /// Instant, new clip at the old clip's relative phase (WALK <-> RUN in
    /// ordinary movement; TURN pair swaps are seeked by the sim's clock).
    InstantKeepPhase,
}

/// A TURN_* clip (the locked-on pivot set, including the wounded set).
pub fn is_turn_clip(clip: &str) -> bool {
    clip.starts_with("TURN_")
}

/// DOWN_FRONT / DOWN_BACK: the lying pose a knock-down or collapse queues
/// behind its landing or collapse clip (not the _STAND clips that get up).
pub fn is_lying_loop(clip: &str) -> bool {
    matches!(clip, "DOWN_FRONT" | "DOWN_BACK")
}

/// A WALK or RUN clip of any mood (`0x1403faed0` columns 1 and 2).
pub fn is_gait_clip(clip: &str) -> bool {
    clip.starts_with("WALK") || clip.starts_with("RUN")
}

/// An idle clip of any mood (column 0).
fn is_wait_clip(clip: &str) -> bool {
    clip.starts_with("WAIT")
}

/// The rule for a locomotion clip change from `from` to `to`: a pivot's own
/// end or gait swap and a WALK/RUN swap (gait or mood) are instant,
/// everything else cross-fades.
pub fn clip_switch(from: Option<&str>, to: &str) -> ClipSwitch {
    match (from, to) {
        (Some(f), t) if is_turn_clip(f) && is_gait_clip(t) => ClipSwitch::Instant,
        (Some(f), t) if is_turn_clip(f) && is_turn_clip(t) => ClipSwitch::Instant,
        (Some(f), t) if is_gait_clip(f) && is_gait_clip(t) => ClipSwitch::InstantKeepPhase,
        _ => ClipSwitch::Blend,
    }
}

/// The start a locomotion clip change makes.
fn locomotion_start(from: Option<&str>, to: &str) -> ClipStart {
    let looped = is_wait_clip(to) || is_gait_clip(to);
    match (from, clip_switch(from, to)) {
        (None, _) => ClipStart { looped, ..start_args(StartKind::ReturnIdle) },
        (Some(_), ClipSwitch::InstantKeepPhase) => start_args(StartKind::GaitSwap),
        (Some(_), ClipSwitch::Instant) => ClipStart { looped, ..start_args(StartKind::Queued) },
        (Some(_), ClipSwitch::Blend) => ClipStart { looped, ..start_args(StartKind::ReturnIdle) },
    }
}

/// What the animation layer knows about the state being entered.
#[derive(Clone, Copy, Debug)]
pub struct StartCx<'a> {
    pub motion: Motion,
    /// The clip now playing, if any.
    pub from: Option<&'a str>,
    /// The clip being started.
    pub to: &'a str,
    /// Per-character cast blend (`cast_blend_frames`).
    pub cast_blend_frames: f32,
    /// A charge skill being held: its `(w39, w40)`.
    pub hold_loop: Option<(f32, f32)>,
    /// The charge was just released: the row's w40 frame.
    pub release_seek: Option<f32>,
    /// `Some(blend frames)` when the hit reaction is the 5-tick flinch.
    pub flinch_blend: Option<f32>,
}

/// The start arguments for entering `cx.motion` with clip `cx.to`.
pub fn start_for(cx: &StartCx) -> ClipStart {
    // The float clips (Levitate / Skywalk at the top of their rise): FLOAT_IN
    // blends in; FLOAT_WAIT queued behind it is a cut, the same clips picked
    // by the float state itself blend (`0x14025a330`, `0x14025b170`).
    match cx.to {
        "FLOAT_IN" => return start_args(StartKind::FloatIn),
        "FLOAT_WAIT" if cx.from == Some("FLOAT_IN") => return start_args(StartKind::Queued),
        "FLOAT_WAIT" | "FLOAT_MOVE" => return start_args(StartKind::FloatHold),
        _ => {}
    }
    // The Jump skill's wind-up is a cast to the combat side but its own state
    // (4) to the program: JUMP_START, blend 5 (`0x140257c27`).
    if cx.to == "JUMP_START" {
        return start_args(StartKind::JumpStart);
    }
    match cx.motion {
        Motion::Cast | Motion::Lunge => match cx.release_seek {
            Some(seek_frames) => start_args(StartKind::ChargeRelease { seek_frames }),
            None => start_args(StartKind::Cast { blend_frames: cx.cast_blend_frames, seek_frames: 0.0, hold_loop: cx.hold_loop }),
        },
        Motion::Hit => match cx.flinch_blend {
            Some(blend_frames) => start_args(StartKind::Flinch { blend_frames }),
            None => start_args(StartKind::Reaction),
        },
        // The KO pose is the killing blow's own reaction (audit D11).
        Motion::Down => start_args(StartKind::Reaction),
        Motion::Blown => start_args(StartKind::KnockFlight),
        // The DOWN_FRONT / DOWN_BACK loop queued behind a landing or collapse
        // clip (`0x1402576f6`: blend -1, loop 1), after the BLOW_LAND_* clip
        // itself (blend 0, `0x140254da8`'s sibling in `0x140259990`) -- or
        // JUMP_LAND* after a fall.
        Motion::Floored if is_lying_loop(cx.to) => start_args(StartKind::Queued),
        Motion::Floored | Motion::Land | Motion::HeavyLand => start_args(StartKind::Landing),
        Motion::GetUp => start_args(StartKind::GetUp),
        Motion::Dodge => start_args(StartKind::GroundDodge { seek_frames: 0.0 }),
        Motion::Rise => start_args(StartKind::TakeOff),
        Motion::Fall => start_args(StartKind::Fall),
        // The cartwheel's recovery is our own state (the program's dodge
        // clip covers it); it returns like any action's end.
        Motion::Recover | Motion::Idle | Motion::Walk | Motion::Run | Motion::Strafe(_) => locomotion_start(cx.from, cx.to),
    }
}

/// Per-character cast blend: the charaset float (`0x14025a692` reads
/// `[0x1404c020c + character * 0xa0]`; the loader `0x14017d5d3` fills it from
/// record `+0x16` as a float). Records 0-10 (the 11 playable fighters,
/// `pc00a`-`pc10a`) hold 0; the enemies 5.
pub fn cast_blend_frames(model: &str) -> f32 {
    let playable = model.len() == 5 && model.starts_with("pc") && model[2..4].chars().all(|c| c.is_ascii_digit()) && model.ends_with('a');
    if playable { 0.0 } else { BLEND_FRAMES }
}

/// Which mood clip set a fighter shows (`0x14025c980`, table `0x1403faed0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Normal,
    /// Speed status >= +50.
    Fast,
    /// Speed status <= -50.
    Slow,
    /// Confusion status (`0x23`).
    Confused,
    /// Health <= 5.
    Wounded,
}

/// The mood pick, in the program's priority order: status 1 >= 50 -> fast,
/// <= -50 -> slow, status 35 -> confused, health <= 5 -> wounded
/// (`0x14025c99e cmp eax, 0x32`, `0x14025c9c4 cmp eax, -0x32`,
/// `0x14025c9ea test eax, eax`, `0x14025ca08 cmp [rax+0x4c], 5`).
pub fn mood(speed_status: i32, confused: bool, health: i32) -> Mood {
    if speed_status >= 50 {
        Mood::Fast
    } else if speed_status <= -50 {
        Mood::Slow
    } else if confused {
        Mood::Confused
    } else if health <= 5 {
        Mood::Wounded
    } else {
        Mood::Normal
    }
}

/// The idle / walk / run clip of a mood: columns 0-2 of the program's table
/// at `0x1403faed0` (rows of 11 clip ids: normal `0,7,12`, fast `0,10,15`,
/// slow `0,11,16`, confused `5,8,13`, wounded `6,9,14`).
pub fn mood_clip(mood: Mood, gait: Option<crate::sim::Gait>) -> &'static str {
    use crate::sim::Gait;
    match (mood, gait) {
        (Mood::Normal | Mood::Fast | Mood::Slow, None) => "WAIT0",
        (Mood::Confused, None) => "WAIT_CON",
        (Mood::Wounded, None) => "WAIT_DAM",
        (Mood::Normal, Some(Gait::Walk)) => "WALK",
        (Mood::Normal, Some(Gait::Run)) => "RUN",
        (Mood::Fast, Some(Gait::Walk)) => "WALK_FAST",
        (Mood::Fast, Some(Gait::Run)) => "RUN_FAST",
        (Mood::Slow, Some(Gait::Walk)) => "WALK_SLOW",
        (Mood::Slow, Some(Gait::Run)) => "RUN_SLOW",
        (Mood::Confused, Some(Gait::Walk)) => "WALK_CON",
        (Mood::Confused, Some(Gait::Run)) => "RUN_CON",
        (Mood::Wounded, Some(Gait::Walk)) => "WALK_DAM",
        (Mood::Wounded, Some(Gait::Run)) => "RUN_DAM",
    }
}

/// The protect blink: the program draws the fighter only on **even** values
/// of its protect timer (`0x14024f4a4 test byte [rbx+0x6434], 1`; the timer
/// is decremented once per program tick by `FUN_14024d2f0`, called from the
/// fighter tick at `0x14024f15d`). One program tick is two sim ticks, so the
/// fighter flickers every 1/30 s. Our window counts sim ticks down; `ticks`
/// maps to program ticks by rounding up (150 sim ticks = 75 program ticks =
/// the get-up clip + 45). Unread: the extra draw conditions on `+0x14240`
/// and the global mask at `0x140530034` (`0x14024f4b7`, `0x14024f4c0`).
pub fn protect_hides_model(invuln_sim_ticks: u16) -> bool {
    invuln_sim_ticks > 0 && invuln_sim_ticks.div_ceil(2) % 2 == 1
}

/// The skill-row facts the animation layer needs, read once from
/// `skills.json`: each skill's action rows' `(charge_cycle, start_frame)`
/// (ground, air) -- w39 and w40 -- and each flinch reaction row's blend (w43).
#[derive(Default, Debug)]
pub struct Tables {
    cast_rows: HashMap<i16, [(i16, i16); 2]>,
    /// Reaction row -> (blend frames, flinch length in program ticks).
    flinch: HashMap<i16, (f32, f32)>,
}

impl Tables {
    pub fn load(path: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else { return Self::default() };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return Self::default() };
        Self::from_json(&v)
    }

    pub fn from_json(v: &serde_json::Value) -> Self {
        let pair = |r: &serde_json::Value| (r["charge_cycle"].as_i64().unwrap_or(0) as i16, r["start_frame"].as_i64().unwrap_or(0) as i16);
        let cast_rows = v["skills"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| Some((s["id"].as_i64()? as i16, [pair(&s["action"]["ground"]), pair(&s["action"]["air"])])))
            .filter(|(_, rows)| rows.iter().any(|&(c, s)| c > 0 || s > 0))
            .collect();
        let flinch = v["reactions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["kind"].as_i64() == Some(11))
            .filter_map(|r| {
                let w = |k: usize| r["w"].get(k).and_then(|x| x.as_i64()).unwrap_or(0) as f32;
                Some((r["row"].as_i64()? as i16, (w(2), w(1))))
            })
            .collect();
        Tables { cast_rows, flinch }
    }

    /// The hold loop and release frame of the row a cast plays: `s40` picks
    /// the ground or air row as the sim does (1 and 3 = air, 2 = by the
    /// caster's state).
    fn row(&self, skill: i16, s40: i16, airborne: bool) -> Option<(i16, i16)> {
        let rows = self.cast_rows.get(&skill)?;
        let air_row = s40 == 1 || s40 == 3 || (s40 == 2 && airborne);
        let (cycle, start) = if air_row && (rows[1].0 > 0 || rows[1].1 > 0) { rows[1] } else { rows[0] };
        Some((cycle, start))
    }

    /// A charge skill's hold loop in frames: `(w39, w40)` (`0x14025a6a4`-
    /// `0x14025a6ef`: only for a skill with the charge flag).
    pub fn hold_loop(&self, skill: i16, s40: i16, airborne: bool) -> Option<(f32, f32)> {
        self.row(skill, s40, airborne).filter(|&(c, s)| c > 0 && s > c).map(|(c, s)| (c as f32, s as f32))
    }

    /// The frame a released charge restarts at: the row's w40.
    pub fn release_frame(&self, skill: i16, s40: i16, airborne: bool) -> f32 {
        self.row(skill, s40, airborne).map_or(0.0, |(_, s)| s.max(0) as f32)
    }

    /// `Some(blend frames)` when reaction row `row` is the flinch (kind 11).
    /// The blend is capped at the flinch's own length: row 70 carries a blend
    /// of 10000 frames, which would never finish mixing.
    pub fn flinch_blend(&self, row: i16) -> Option<f32> {
        self.flinch.get(&row).map(|&(blend, ticks)| blend.min(ticks.max(1.0)))
    }
}

/// The time a hold-looped clip should be at: once it has reached the loop's
/// end, back by the loop's length (`0x14025755e`-`0x14025758d`:
/// `t = start + t - end`). `None` when it needs no change.
pub fn hold_wrap(t_seconds: f32, hold: (f32, f32)) -> Option<f32> {
    let (start, end) = (hold.0 * FRAME_SECONDS, hold.1 * FRAME_SECONDS);
    if t_seconds < end { return None }
    if end <= start { return Some(start) }
    Some(start + (t_seconds - end).rem_euclid(end - start))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Gait, StrafeDir};
    use pd_formats::anim::ANIM_NAMES;

    fn cx<'a>(motion: Motion, from: Option<&'a str>, to: &'a str) -> StartCx<'a> {
        StartCx { motion, from, to, cast_blend_frames: 0.0, hold_loop: None, release_seek: None, flinch_blend: None }
    }

    #[test]
    fn a_players_cast_cuts_in_with_no_hold_loop() {
        // `0x14025a692`: blend = charaset float, 0 for the playable fighters.
        let s = start_for(&cx(Motion::Cast, Some("WAIT0"), "ATK_SHOT"));
        assert!(s.is_cut(), "player cast blend 0: {s:?}");
        assert!(!s.looped && s.seek_frames == 0.0 && s.hold_loop.is_none() && s.root_motion);
        assert_eq!(s.fade(), Duration::ZERO);
        // The short-range dash is the same clip, not a new start.
        assert_eq!(start_for(&cx(Motion::Lunge, Some("WAIT0"), "ATK_SORD")), s);
        // An enemy's record holds 5.
        let e = start_for(&StartCx { cast_blend_frames: cast_blend_frames("enm01a"), ..cx(Motion::Cast, Some("WAIT0"), "ATK_SHOT") });
        assert_eq!(e.blend_frames, 5.0);
        assert_eq!(e.fade(), Duration::from_secs_f32(5.0 / 30.0));
    }

    #[test]
    fn every_playable_fighter_casts_with_blend_zero() {
        // charaset records 0-10 read from `game_files/.../charaset_en.dat`
        // (+0x16 = 0) on 2026-09-30; records 15-19, 21, 22, 24-26, 42, 43 = 5.
        for k in 0..=10 {
            assert_eq!(cast_blend_frames(&format!("pc{k:02}a")), 0.0, "pc{k:02}a");
        }
        assert_eq!(cast_blend_frames("enm00a"), 5.0);
        assert_eq!(cast_blend_frames("boss03"), 5.0);
    }

    #[test]
    fn a_charge_holds_frames_w39_to_w40_and_releases_with_a_cut_seek() {
        let s = start_for(&StartCx { hold_loop: Some((13.0, 15.0)), ..cx(Motion::Cast, Some("WAIT0"), "ATK_SPIKE") });
        assert_eq!(s.hold_loop, Some((13.0, 15.0)), "Gungnir: w39 13, w40 15 (`0x14025a6a4`)");
        let r = start_for(&StartCx { release_seek: Some(15.0), hold_loop: Some((13.0, 15.0)), ..cx(Motion::Cast, Some("ATK_SPIKE"), "ATK_SPIKE") });
        assert!(r.is_cut(), "release restarts with blend 0 (`0x14025c739 xorps xmm6`)");
        assert_eq!((r.seek_frames, r.hold_loop, r.looped), (15.0, None, false));
    }

    #[test]
    fn the_hold_loop_wraps_from_its_end_back_by_its_length() {
        let hold = (13.0, 15.0);
        assert_eq!(hold_wrap(14.0 * FRAME_SECONDS, hold), None, "before the end: plays on");
        let t = hold_wrap(15.5 * FRAME_SECONDS, hold).unwrap();
        assert!((t - 13.5 * FRAME_SECONDS).abs() < 1e-6, "15.5 -> 13.5: {t}");
        let t = hold_wrap(19.0 * FRAME_SECONDS, hold).unwrap();
        assert!(t >= 13.0 * FRAME_SECONDS && t < 15.0 * FRAME_SECONDS);
        assert_eq!(hold_wrap(20.0 * FRAME_SECONDS, (13.0, 13.0)), Some(13.0 * FRAME_SECONDS), "start == end holds one frame");
    }

    #[test]
    fn reactions_cut_and_the_flinch_blends() {
        // Kinds 0-3: blend -1, loop 0, root w1 (`0x140258897` and kin).
        let r = start_for(&cx(Motion::Hit, Some("WAIT0"), "DMG_SHOT_FRONT"));
        assert!(r.is_cut() && !r.looped && r.root_motion, "{r:?}");
        // Flyer kinds: -1, loop 1, root 0 (`0x140258aa9`, `0x140258b43`).
        let k = start_for(&cx(Motion::Blown, Some("DMG_SHOT_FRONT"), "BLOW0_FRONT"));
        assert!(k.is_cut() && k.looped && !k.root_motion, "{k:?}");
        // Kind 11: loops, blend = w43 (5) (`0x140258de6`).
        let f = start_for(&StartCx { flinch_blend: Some(5.0), ..cx(Motion::Hit, Some("WAIT0"), "DMG_SHORT") });
        assert_eq!((f.blend_frames, f.looped), (5.0, true));
        // The KO pose is the killing blow's reaction.
        assert!(start_for(&cx(Motion::Down, Some("WAIT0"), "BLOW0_FRONT")).is_cut());
    }

    #[test]
    fn landings_get_ups_dodges_and_the_take_off_cut() {
        for (m, c) in [(Motion::Land, "JUMP_LAND1"), (Motion::HeavyLand, "JUMP_LAND0"), (Motion::Floored, "BLOW_LAND_SF"), (Motion::GetUp, "DOWN_FRONT_STAND"), (Motion::Dodge, "DASH_A"), (Motion::Rise, "JUMP_UP")] {
            let s = start_for(&cx(m, Some("WAIT0"), c));
            assert!(s.is_cut() && !s.looped, "{m:?} {c}: {s:?}");
        }
        assert!(start_for(&cx(Motion::Land, Some("JUMP_DOWN"), "JUMP_LAND1")).root_motion, "landing root 1 (`0x140254da8`)");
        assert!(!start_for(&cx(Motion::Rise, Some("WAIT0"), "JUMP_UP")).root_motion, "take-off root 0 (`0x14025a106`)");
        assert_eq!(start_for(&cx(Motion::Dodge, Some("WAIT0"), "DASH_A")).seek_frames, 0.0, "dodge seek = row w43 = 0");
    }

    #[test]
    fn the_down_loop_queues_behind_the_landing_clip_and_the_jump_start_blends_five() {
        // The landing clip itself cuts in (blend 0, `0x140259990`); the DOWN_* loop behind it starts with
        // the queued-clip arguments (blend -1, loop 1, `0x1402576f6`).
        let land = start_for(&cx(Motion::Floored, Some("BLOW0_FRONT"), "BLOW_LAND_SF"));
        assert!(land.is_cut() && !land.looped);
        for pose in ["DOWN_FRONT", "DOWN_BACK"] {
            let q = start_for(&cx(Motion::Floored, Some("BLOW_LAND_SF"), pose));
            assert!(q.is_cut() && q.looped && !q.root_motion, "{pose}: {q:?}");
        }
        // A kind-1 collapse is a reaction start (cut, no loop) then the same queued loop.
        assert!(start_for(&cx(Motion::Hit, Some("WAIT0"), "DMG_DRAIN_HIGH")).is_cut());
        // The Jump skill's wind-up (`0x140257c27`): blend 5, plays once.
        let j = start_for(&cx(Motion::Idle, Some("WAIT0"), "JUMP_START"));
        assert_eq!((j.blend_frames, j.looped), (5.0, false));
    }

    #[test]
    fn the_fall_and_every_return_to_idle_blend_five_frames() {
        let f = start_for(&cx(Motion::Fall, Some("JUMP_UP"), "JUMP_DOWN"));
        assert_eq!((f.blend_frames, f.looped), (5.0, false), "`0x14025a282`");
        assert_eq!(start_args(StartKind::JumpStart).blend_frames, 5.0, "`0x140257c27`");
        for from in ["ATK_SHOT", "DMG_SHOT_FRONT", "JUMP_LAND1", "DOWN_FRONT_STAND", "BLOW_LAND_SF", "TURN_WALK_L", "RUN"] {
            let s = start_for(&cx(Motion::Idle, Some(from), "WAIT0"));
            assert_eq!((s.blend_frames, s.looped), (5.0, true), "{from} -> WAIT0: `0x14025766a`");
        }
    }

    #[test]
    fn locomotion_switches_follow_the_program() {
        assert_eq!(clip_switch(Some("TURN_WALK_R"), "RUN"), ClipSwitch::Instant, "pivot end: queued clip, blend -1");
        assert_eq!(clip_switch(Some("TURN_RUN_BL"), "WALK"), ClipSwitch::Instant);
        assert_eq!(clip_switch(Some("TURN_RUN_R"), "TURN_WALK_R"), ClipSwitch::Instant, "gait swap mid-pivot, blend 0");
        assert_eq!(clip_switch(Some("WALK"), "RUN"), ClipSwitch::InstantKeepPhase);
        assert_eq!(clip_switch(Some("RUN"), "WALK"), ClipSwitch::InstantKeepPhase);
        assert_eq!(clip_switch(Some("WAIT0"), "TURN_WALK_L"), ClipSwitch::Blend, "pivot entry blends 5 frames");
        assert_eq!(clip_switch(Some("WAIT0"), "RUN"), ClipSwitch::Blend);
        assert_eq!(clip_switch(Some("TURN_WALK_L"), "WAIT0"), ClipSwitch::Blend, "release blends to idle");
        assert_eq!(clip_switch(Some("RUN"), "WAIT0"), ClipSwitch::Blend);
        assert_eq!(clip_switch(None, "WAIT0"), ClipSwitch::Blend);
        // Mood swaps while moving go through the same `0x14025ad50` swap.
        assert_eq!(clip_switch(Some("WALK"), "WALK_DAM"), ClipSwitch::InstantKeepPhase);
        assert_eq!(clip_switch(Some("RUN_FAST"), "WALK_FAST"), ClipSwitch::InstantKeepPhase);
        assert_eq!(clip_switch(Some("TURN_WALK_R"), "RUN_DAM"), ClipSwitch::Instant);
        // A locomotion start carries those args.
        let hand_off = start_for(&cx(Motion::Run, Some("TURN_WALK_R"), "RUN"));
        assert!(hand_off.is_cut() && hand_off.looped && !hand_off.keep_phase);
        let swap = start_for(&cx(Motion::Run, Some("WALK"), "RUN"));
        assert!(swap.is_cut() && swap.looped && swap.keep_phase);
        let entry = start_for(&cx(Motion::Run, Some("WAIT0"), "RUN"));
        assert_eq!((entry.blend_frames, entry.looped), (5.0, true));
        let pivot = start_for(&cx(Motion::Strafe(StrafeDir::Left), Some("WAIT0"), "TURN_WALK_L"));
        assert_eq!((pivot.blend_frames, pivot.looped), (5.0, false), "a pivot plays once");
    }

    #[test]
    fn the_mood_pick_follows_0x14025c980() {
        assert_eq!(mood(50, false, 20), Mood::Fast);
        assert_eq!(mood(49, false, 20), Mood::Normal);
        assert_eq!(mood(-50, false, 20), Mood::Slow);
        assert_eq!(mood(-49, false, 20), Mood::Normal);
        assert_eq!(mood(0, true, 20), Mood::Confused);
        assert_eq!(mood(0, false, 5), Mood::Wounded);
        assert_eq!(mood(0, false, 6), Mood::Normal);
        // Priority: speed, then confusion, then health.
        assert_eq!(mood(60, true, 1), Mood::Fast);
        assert_eq!(mood(0, true, 1), Mood::Confused);
    }

    #[test]
    fn the_mood_table_is_the_program_s_columns_0_to_2() {
        // Rows of `0x1403faed0`, clip ids per mood: (idle, walk, run).
        let ids = [(Mood::Normal, [0, 7, 12]), (Mood::Fast, [0, 10, 15]), (Mood::Slow, [0, 11, 16]), (Mood::Confused, [5, 8, 13]), (Mood::Wounded, [6, 9, 14])];
        for (m, [idle, walk, run]) in ids {
            assert_eq!(mood_clip(m, None), ANIM_NAMES[idle], "{m:?} idle");
            assert_eq!(mood_clip(m, Some(Gait::Walk)), ANIM_NAMES[walk], "{m:?} walk");
            assert_eq!(mood_clip(m, Some(Gait::Run)), ANIM_NAMES[run], "{m:?} run");
        }
    }

    #[test]
    fn the_protect_blink_hides_the_model_on_odd_program_ticks() {
        assert!(!protect_hides_model(0), "no protection, no blink");
        // 150 sim ticks = program timer 75 (odd = hidden), then 74 (drawn)...
        assert!(protect_hides_model(150) && protect_hides_model(149));
        assert!(!protect_hides_model(148) && !protect_hides_model(147));
        assert!(protect_hides_model(146));
        // Fall-out's 90 program ticks start even = drawn (`0x140251f0c`).
        assert!(!protect_hides_model(180));
    }

    #[test]
    fn rows_load_from_skills_json_shapes() {
        let v: serde_json::Value = serde_json::json!({
            "skills": [
                {"id": 73, "action": {"ground": {"charge_cycle": 13, "start_frame": 15}, "air": null}},
                {"id": 195, "action": {"ground": {"charge_cycle": 7, "start_frame": 9}, "air": null}},
                {"id": 1, "action": {"ground": {"charge_cycle": 0, "start_frame": 0}, "air": null}}
            ],
            "reactions": [
                {"row": 46, "kind": 11, "w": [129, 5, 5, 0, 0, 0, 0, 0]},
                {"row": 70, "kind": 11, "w": [129, 90, 10000, 0, 0, 0, 0, 0]},
                {"row": 2, "kind": 2, "w": [117, 120, 118, 119, 0, 0, 0, 0]}
            ]
        });
        let t = Tables::from_json(&v);
        assert_eq!(t.hold_loop(73, 0, false), Some((13.0, 15.0)), "Gungnir");
        assert_eq!(t.release_frame(73, 0, false), 15.0);
        assert_eq!(t.hold_loop(195, 0, false), Some((7.0, 9.0)), "Ki Palm");
        assert_eq!(t.hold_loop(1, 0, false), None, "an ordinary cast has no hold loop");
        assert_eq!(t.release_frame(1, 0, false), 0.0);
        assert_eq!(t.flinch_blend(46), Some(5.0), "row 46 = the 5-tick flinch");
        assert_eq!(t.flinch_blend(70), Some(90.0), "capped at its own length");
        assert_eq!(t.flinch_blend(2), None, "a four-way reaction is not a flinch");
    }

    #[test]
    fn the_float_clips_blend_in_and_the_queued_wait_cuts() {
        // `0x14025a330`: FLOAT_IN = clip 0x2a, blend 5.0, loop 0, root 0; queued clip 0x2b
        // (FLOAT_WAIT) loop 1 behind it; `0x14025b170` starts 0x2c / 0x2b with loop 1, blend 5.0.
        assert_eq!((ANIM_NAMES[0x2a], ANIM_NAMES[0x2b], ANIM_NAMES[0x2c]), ("FLOAT_IN", "FLOAT_WAIT", "FLOAT_MOVE"));
        let s = start_for(&cx(Motion::Fall, Some("JUMP_UP"), "FLOAT_IN"));
        assert_eq!((s.blend_frames, s.looped, s.root_motion), (BLEND_FRAMES, false, false));
        let s = start_for(&cx(Motion::Fall, Some("FLOAT_IN"), "FLOAT_WAIT"));
        assert!(s.is_cut() && s.looped, "the queued clip: blend -1, loop 1: {s:?}");
        for (from, to) in [("FLOAT_WAIT", "FLOAT_MOVE"), ("FLOAT_MOVE", "FLOAT_WAIT")] {
            let s = start_for(&cx(Motion::Fall, Some(from), to));
            assert_eq!((s.blend_frames, s.looped, s.root_motion), (BLEND_FRAMES, true, false), "{from} -> {to}");
        }
    }
}
