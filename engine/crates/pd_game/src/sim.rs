//! Deterministic match simulation, stepped at a fixed 60 Hz.
//!
//! Everything the outcome depends on lives here and only here: inputs in,
//! state out. Rendering, cameras and animation read this state but never
//! write it, so the same inputs replay identically (the basis for online
//! play later).

use crate::combat::{Action, Combat, KNOCK_GETUP_TICKS, MAX_PLAYERS};
use glam::{Vec2, Vec3};
use pd_formats::collision::Collision;

pub const HZ: f64 = 60.0;
pub const DT: f32 = 1.0 / HZ as f32;

// Units are game units: a character is ~21 tall (~1.75 m, so ~12 per metre).
//
// Ground speeds, turning, gravity and landing below are the program's own
// numbers (`engine/docs/PROGRAM-RE.md` "Ground movement" / "Jumps"), read
// from PDUWP.exe and checked against Laurence's footage: one program tick is
// 1/30 s, ours is 1/60 s, so a per-tick speed halves and a per-tick²
// acceleration quarters when carried over -- easier to keep everything in
// u/s and u/s² (Hz-independent) and let `DT` do the conversion, as this file
// already did.
/// Run: 20 pts / 10.8 per program tick = 1.852 u/tick @ 30 Hz = 55.6 u/s.
pub const RUN_SPEED: f32 = 55.6;
/// Walk: 7.5 pts / 10.8 per program tick = 0.694 u/tick @ 30 Hz = 20.8 u/s.
pub const WALK_SPEED: f32 = 20.8;
/// The program's stick dead zone: 13,000 of 32,767 per axis (`0x140256600`),
/// then the rest of the axis is rescaled to 0..1 (divide by 19,767). It is a
/// square dead zone (each axis on its own, no radial test first) and the
/// magnitude of the two processed axes is not capped at 1 (a full diagonal
/// reaches 1.414). MOVEMENT-AUDIT.md "Program read results" findings 1-3.
pub const STICK_AXIS_DEAD_ZONE: f32 = 13_000.0 / 32_767.0;
/// Processed magnitude below which a running player drops back to walk, and
/// the floor of the no-change band (`0x140256ef8`: the gait keeps its state
/// throughout 0.4..=0.7).
pub const STICK_DEAD_ZONE: f32 = 0.4;
/// Processed magnitude above which the gait becomes run (program: 0.7).
pub const RUN_GAIT_ON: f32 = 0.7;
/// Facing turn rate: 20 degrees per program tick = 600 degrees/s.
pub const TURN_RATE_DEG_PER_S: f32 = 600.0;
/// Locked-on strafe entry window: the body must already face the camera
/// direction to within ~10 degrees (`0x14025b703`: 0x38d, MOVEMENT-AUDIT.md
/// "step 3b" finding 1) -- otherwise the push is ordinary movement.
pub const STRAFE_ENTRY_ALIGN_DEG: f32 = 9.99;
/// Pivot length in 30 Hz animation ticks for the default character, by gait
/// and side (MOVEMENT-AUDIT.md "step 3c"; the names are inverted relative
/// to gait: walking uses the TURN_RUN_* clips, running TURN_WALK_*). The
/// clip switches on the clock update whose proposed time is strictly
/// greater than the length (`0x1402575ca`): `length + 1` animation-clock
/// calls counting the entry tick's, i.e. `2 * length` sim ticks after the
/// entry tick at normal speed (`Pivot::time`).
pub const PIVOT_TICKS_30HZ_RUN: [u16; 4] = [24, 33, 26, 35]; // R, L, BR, BL (TURN_WALK_*)
pub const PIVOT_TICKS_30HZ_WALK: [u16; 4] = [35, 49, 28, 42]; // R, L, BR, BL (TURN_RUN_*)
pub const AIR_CONTROL: f32 = 0.6;
/// Ordinary fall: 0.12 u/tick² @ 30 Hz = 108 u/s².
pub const GRAVITY: f32 = 108.0;
/// Falling speed cap: 7.5 u/tick @ 30 Hz = 225 u/s.
pub const FALL_SPEED_CAP: f32 = 225.0;
/// The Jump dodge's (hidden skill 750) own launch speed: 2.84 u/tick @ 30 Hz.
/// Not the ordinary jump -- no such skill is built yet (M11); this is the
/// click-the-stick dodge move, kept working under the new base gravity
/// above by giving it its own numbers (below) rather than sharing GRAVITY,
/// which would have made it ~3x floatier (PROGRAM-RE.md "Dodge: Jump (750)").
pub const JUMP_VELOCITY: f32 = 85.0;
/// The Jump dodge's own, heavier gravity: (s47 + 100) x 0.12 x 0.01 = 0.288
/// u/tick² @ 30 Hz = 259.2 u/s² -- apex 14 u, ~0.66 s airborne, matching
/// PROGRAM-RE.md and the footage's ~1.0 s press-to-free total.
pub const DODGE_JUMP_GRAVITY: f32 = 259.2;
/// How far up a player can step onto a ledge/stair without jumping.
pub const STEP_HEIGHT: f32 = 6.0;
/// Landing by fall height (highest point reached − landing point):
/// below this, no landing clip (straight back to idle/run).
pub const LAND_FALL_MIN: f32 = 40.0;
/// Above this, the heavy landing (JUMP_LAND0) plus a dust effect (fx lane).
pub const LAND_FALL_HEAVY: f32 = 80.0;
/// Light landing (JUMP_LAND1, 13 frames @ 30 fps): locked for length − 1 ticks.
pub const LAND_TIME: f32 = 12.0 / 30.0;
/// Heavy landing (JUMP_LAND0, 30 frames @ 30 fps): locked for length − 1 ticks.
pub const HEAVY_LAND_TIME: f32 = 29.0 / 30.0;
pub const FALL_RESPAWN_BELOW: f32 = 150.0;
/// Ticks a grounded player may cross a gap in the floor before falling.
pub const COYOTE_TICKS: u8 = 6;
/// Right-stick camera turn speed when not locked on (radians/s, full tilt).
pub const CAM_TURN_SPEED: f32 = 2.6;
/// Locked-on camera yaw slew (lockon lane O4, 2026-09-29): the program turns
/// the camera toward the target at its own per-tick rate instead of snapping
/// it (PROGRAM-RE.md "Lock-on": the first two `setup/configdata.dat` floats
/// read as 6 deg/tick yaw and 3 deg/tick pitch -- the column mapping is
/// inferred from the values' symmetry, not program-verified, so this rate
/// is unconfirmed). 6 deg/program-tick at 30 Hz = 180 deg/s; a 90 deg
/// misalignment takes 30 ticks (0.5 s) to close. `PD_LOCK_CAM_RATE`
/// overrides it for tuning (test switches only; the shipped default is the
/// const -- the sim must stay identical on every machine, so this is read
/// once per process, not per tick).
pub const LOCK_CAM_YAW_RATE_DEG_PER_S: f32 = 180.0;
/// The ground dodges (click the left stick; guides: "Tumbling rolls your
/// character in the direction they are facing. It has no recovery time";
/// "Cartwheeling ... is more sudden ... but you must recover after using
/// it"). Times are the characters' own clips (tumble: DASHEND_A 0.5 s;
/// cartwheel: DASH_A 0.62 s, then DASHEND_A 1.04 s recovery); speeds are
/// guesses, unconfirmed (a cartwheel ~ the Dash skill's 30 ft ~ 110 u).
pub const TUMBLE_TICKS: u16 = 30;
pub const TUMBLE_SPEED: f32 = 130.0;
pub const CARTWHEEL_TICKS: u16 = 37;
pub const CARTWHEEL_SPEED: f32 = 180.0;
pub const CARTWHEEL_RECOVER_TICKS: u16 = 62;
/// The top of the cylinder `Collision::push_out_of_walls` pushes out of walls, and the height a prop is
/// judged within reach of a fighter by (`props::nearest_throwable`). A model parameter of ours
/// (UNCONFIRMED), kept as it was: the program's body against the stage is three spheres of radius
/// `contact / 2` along the fighter's own height (`body.rs`), which that cylinder only approximates, and
/// its sample heights decide where a sloped wall edge pushes, so the per-character height is not fed
/// into it (the movement feel stays as approved; the radius is the READ value).
pub const BODY_HEIGHT: f32 = 20.0;
pub use crate::body::Body;

/// One player's input for one tick, quantised so it is identical on every
/// machine and cheap to send over a network.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    /// Stick: x = right, y = forward (toward the opponent), each -127..=127.
    pub stick: [i8; 2],
    pub jump: bool,
    /// Lock-on toggle button held (toggles on press).
    pub lock: bool,
    /// Skill buttons held: bit k = slot k (fires on press).
    pub skills: u8,
    /// Camera turn (right stick x, -127..=127). Free camera only.
    pub cam: i8,
    /// `cam` is on the double scale: 127 = twice the original's top turning speed (CAM SENSITIVITY
    /// above what the original's full stick gives; Laurence 2026-10-08). Never set at or below the
    /// original's top speed, so every recorded input means what it did. See `Input::cam_value`.
    pub cam_fast: bool,
    /// Recentre the free camera behind the player (right stick click, held).
    pub recenter: bool,
    /// Switch-target button held: on its rising edge, while locked on, the
    /// lock moves to the next valid player in slot order (the program's input
    /// mask `0x400`, `0x1402531d0`; which physical button feeds it is unread,
    /// audit R1). With only one valid target it does nothing.
    pub switch_target: bool,
}

/// One stick axis (-1..1 raw) through the program's dead zone and rescale
/// (`0x140256600`): inside the dead zone 0, outside it the remainder mapped
/// to 0..1 with the sign kept.
pub fn process_axis(v: f32) -> f32 {
    let a = v.abs();
    if a <= STICK_AXIS_DEAD_ZONE { 0.0 } else { v.signum() * (a - STICK_AXIS_DEAD_ZONE) / (1.0 - STICK_AXIS_DEAD_ZONE) }
}

impl Input {
    pub fn from_axes(x: f32, y: f32, jump: bool, lock: bool) -> Self {
        let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0).round() as i8;
        Input { stick: [q(x), q(y)], jump, lock, ..Default::default() }
    }

    /// The stick as the program's movement code sees it: each axis through
    /// the square dead zone and rescaled (`process_axis`). Not capped at 1.
    fn stick(&self) -> Vec2 {
        Vec2::new(process_axis(self.stick[0] as f32 / 127.0), process_axis(self.stick[1] as f32 / 127.0))
    }

    /// Wire / replay form: exactly `Input::BYTES` bytes (online play,
    /// docs/ONLINE-PLAN.md). Byte 3 packs the five buttons.
    pub const BYTES: usize = 5;

    /// A free-camera turn (-2..2, 1 = the original's top speed) as `(cam, cam_fast)`: up to the top
    /// speed exactly as before (`cam_fast` off), beyond it on the double scale.
    pub fn cam_value(v: f32) -> (i8, bool) {
        let v = v.clamp(-2.0, 2.0);
        if v.abs() <= 1.0 { ((v * 127.0).round() as i8, false) } else { ((v * 0.5 * 127.0).round() as i8, true) }
    }

    pub fn to_bytes(&self) -> [u8; Self::BYTES] {
        let buttons = self.jump as u8 | (self.lock as u8) << 1 | (self.recenter as u8) << 2 | (self.switch_target as u8) << 3 | (self.cam_fast as u8) << 4;
        [self.stick[0] as u8, self.stick[1] as u8, self.skills, buttons, self.cam as u8]
    }

    pub fn from_bytes(b: [u8; Self::BYTES]) -> Self {
        Input {
            stick: [b[0] as i8, b[1] as i8],
            skills: b[2],
            jump: b[3] & 1 != 0,
            lock: b[3] & 2 != 0,
            recenter: b[3] & 4 != 0,
            switch_target: b[3] & 8 != 0,
            cam_fast: b[3] & 16 != 0,
            cam: b[4] as i8,
        }
    }
}

/// FNV-1a over formatted text: `Match::checksum` feeds it the `Debug` form
/// of the sim state. `Debug` prints floats in their shortest exact
/// round-trip form, so two states hash equal only if every float is
/// bit-identical (the one exception, -0.0 vs 0.0, prints differently too).
pub struct StateHasher(pub u64);

impl Default for StateHasher {
    fn default() -> Self {
        StateHasher(0xcbf2_9ce4_8422_2325)
    }
}

impl std::fmt::Write for StateHasher {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for b in s.bytes() {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        Ok(())
    }
}

/// Each character's fixed dodge move (click the left stick): the character
/// table's +0x36 (750 Jump, 751 ground move), tumble vs cartwheel by clip
/// length (`pd_convert chars`, cache/chars.json).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dodge {
    #[default]
    Jump,
    Tumble,
    Cartwheel,
}

impl Dodge {
    pub fn from_name(s: &str) -> Self {
        match s {
            "tumble" => Dodge::Tumble,
            "cartwheel" => Dodge::Cartwheel,
            _ => Dodge::Jump,
        }
    }
}

/// Walk or run, nothing in between: the stick magnitude only picks the
/// gait, with hysteresis (PROGRAM-RE.md "Ground movement"). Persists on the
/// player so the hysteresis has a "current side" to compare against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gait {
    #[default]
    Walk,
    Run,
}

/// Locked-on strafe direction (PROGRAM-RE.md "Movement while locked"): the
/// body stays turned toward the target while the feet move sideways or
/// back, playing the TURN_WALK_*/TURN_RUN_* clips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrafeDir {
    Right,
    Left,
    BackRight,
    BackLeft,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    Idle,
    Walk,
    Run,
    /// Locked on, moving sideways/back of the target: body turned toward
    /// them, TURN_WALK_*/TURN_RUN_* clips (`main.rs::clip_for`).
    Strafe(StrafeDir),
    Rise,
    Fall,
    Land,
    HeavyLand,
    /// Casting a skill (which one: `Fighter::cast_skill`).
    Cast,
    /// Dashing in with a short-range attack.
    Lunge,
    Hit,
    /// Knocked down: blown back, landed, getting up.
    Blown,
    Floored,
    GetUp,
    Down,
    /// Tumbling or cartwheeling (`Player::dodge`).
    Dodge,
    /// Getting up after a cartwheel.
    Recover,
}

/// Which of the program's body states a skill press is judged against
/// (`0x14024fce0`): 0 idle, 1 walk/run and 8 pivot are all `Ground`; 5
/// airborne (and 7 floating, told apart by the Levitate/Skywalk status the
/// combat side holds) is `Air`; everything else -- 3 ground dodge, 4 jump
/// wind-up, 6 landing lock, the cartwheel's recovery -- is `Busy` and takes no
/// cast and no capsule capture (`0x1402517c0`: capture only in states 0/1/8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyState {
    Ground,
    Air,
    Busy,
}

/// A locked-on strafe pivot in progress (the program's movement state 8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pivot {
    pub dir: StrafeDir,
    /// Gait whose pivot clip is playing (walking plays TURN_RUN_*, running
    /// TURN_WALK_*); a gait change swaps to the other clip of the pair at the
    /// same relative progress (`0x14025b50b`, `FUN_1402573e0` start time =
    /// time / old length * new length, no blend).
    pub gait: Gait,
    /// The pivot clip's own clock in 30 Hz animation frames: advanced after
    /// each tick's movement by the playback scale (the speed status), like
    /// the program's `FUN_140257510`. The renderer plays the clip at exactly
    /// this time, so the clip and the sim's facing hand off together.
    pub time: f32,
}

/// The body's fixed offset from the movement heading while a pivot plays
/// (`0x14025b5e0` stores it at `+0x12e70`, `0x14025b270` applies
/// `facing = heading + offset` every pivot tick). In our yaw convention
/// (turning right is negative): right +90 deg, left -90 deg, back 180 deg.
/// It is not a correction layered on the clip: each TURN_* clip bakes the
/// matching turn into the body, and its last pose plus this offset is the
/// WALK/RUN pose facing the heading, so the unblended switch at the end
/// (`0x140257694`, blend -1 = none) is seamless (MOVEMENT-AUDIT.md
/// "Locked-on lane").
pub fn pivot_offset(dir: StrafeDir) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    match dir {
        StrafeDir::Right => FRAC_PI_2,
        StrafeDir::Left => -FRAC_PI_2,
        StrafeDir::BackRight | StrafeDir::BackLeft => PI,
    }
}

/// Animation frames (30 Hz) the program's clock advances per sim tick at
/// normal playback speed.
const ANIM_FRAMES_PER_TICK: f32 = (30.0 / HZ) as f32;

/// The pivot clip's authored length in 30 Hz ticks for this side and gait.
pub fn pivot_len_30hz(dir: StrafeDir, gait: Gait) -> u16 {
    let table = if gait == Gait::Run { PIVOT_TICKS_30HZ_RUN } else { PIVOT_TICKS_30HZ_WALK };
    table[match dir {
        StrafeDir::Right => 0,
        StrafeDir::Left => 1,
        StrafeDir::BackRight => 2,
        StrafeDir::BackLeft => 3,
    }]
}


#[derive(Clone, Debug)]
pub struct Player {
    pub pos: Vec3,
    pub vel: Vec3,
    /// Facing angle around Y (radians); 0 faces +Z.
    pub yaw: f32,
    pub grounded: bool,
    pub motion: Motion,
    pub land_timer: f32,
    pub heavy_land: bool,
    pub spawn: Vec3,
    /// Position and facing at the start of the last tick, for smooth
    /// rendering between ticks.
    pub prev_pos: Vec3,
    pub prev_yaw: f32,
    /// Locked on to the opponent (movement and camera aim at them).
    pub locked: bool,
    /// A CPU seat's aim (cpuhost lane, D5 of docs/audits/P0-AI.md): casts aim at `target` and short-range
    /// skills dash at it exactly as for a locked human (`FUN_140258180` reads only the lock block's band and
    /// target, never the lock-active flag), while `locked` stays false so the body never strafes or slews a
    /// camera. Set by `cpu::CpuHost::drive` every tick; always false for humans. Not hashed.
    pub cpu_aim: bool,
    /// Direction the camera looks (yaw). Locked: slews toward the opponent
    /// at `LOCK_CAM_YAW_RATE_DEG_PER_S`. Free: only the right stick turns
    /// it (moving doesn't). Sim state because free movement is relative
    /// to it.
    pub cam_yaw: f32,
    /// The player this one's lock-on (and aim) is on: the first valid target,
    /// kept while it stays valid and cycled with `Input::switch_target`
    /// (`combat::lock.rs`; `None` = nobody valid). Sim state like `locked`:
    /// kept across respawns and round restarts.
    pub target: Option<usize>,
    /// Lock-on toggle count, incremented on every press-edge toggle, on or
    /// off (lockon lane O4, 2026-09-29). The renderer diffs this to play the
    /// lock-on cue once per toggle: a `CombatEvent` variant would force new
    /// match arms in files this lane doesn't own (`combat.rs` is shared),
    /// and `locked` alone can't edge-detect (it reads the same every tick).
    /// Deterministic: a plain tick-driven counter, part of sim state, kept
    /// across respawns and round restarts like `locked` itself.
    pub lock_on_events: u32,
    /// This character's dodge move; ticks left of a ground dodge and of
    /// the recovery after it, and its direction.
    pub dodge: Dodge,
    /// This character's size (charaset record, `body.rs`): wall and body-vs-body radius, height.
    /// Set once per match like `dodge`, and kept across respawns and round restarts.
    pub body: Body,
    pub dodge_ticks: u16,
    pub recover_ticks: u16,
    /// Walk or run (hysteresis state, PROGRAM-RE.md "Ground movement").
    pub gait: Gait,
    /// Locked-on strafe pivot in progress (the program's state 8): the side
    /// is chosen once on entry and held until the pivot clip ends or the
    /// stick is released (MOVEMENT-AUDIT.md step 3b).
    strafe: Option<Pivot>,
    /// Movement heading (`+0x12e6c`): the direction a pivot moves along; it
    /// chases the live stick at the turn rate and becomes the body facing
    /// when the pivot ends.
    heading: f32,
    /// Last tick ended in ordinary or pivot movement (the program's states
    /// 1 and 8). The strafe entry test only runs from the other states --
    /// idle, or any action/landing/dodge that hands back to idle -- on the
    /// first tick the stick reads non-zero (`0x140256e80` dispatches
    /// `0x14025b5e0` for states 0/21/22/29 only; state 1's handler
    /// `0x14025ad50` never calls it). MOVEMENT-AUDIT.md "Locked-on lane".
    was_moving: bool,
    dodge_dir: Vec3,
    /// Set for the duration of a Jump-dodge's flight, so gravity uses that
    /// move's own, heavier constant instead of the ordinary fall's.
    dodge_jumping: bool,
    /// Time to apex (launch speed / gravity) for the jump in progress, 0
    /// when the current airborne phase isn't a tracked launch (e.g. walked
    /// off a ledge). Read by `main.rs::drive_animation` to time-stretch
    /// JUMP_UP/JUMP_DOWN to match (PROGRAM-RE.md "Jumps").
    pub rise_seconds: f32,
    prev_jump: bool,
    prev_lock: bool,
    prev_switch: bool,
    coyote: u8,
    /// Highest y reached since leaving the ground, for the fall-height
    /// landing rule (PROGRAM-RE.md "Landing by fall height").
    air_peak_y: f32,
    /// A cast that began in mid-air hangs the body where it is (state 11:
    /// `0x14025a3e0` zeroes the vertical speed and `0x140254590` skips the
    /// vertical step while the state lasts). Set by `Match::step` from the
    /// combat side each tick, before the body moves.
    hold_air: bool,
    /// In a cast (states 10-12): the idle "turn back toward the camera" of the
    /// movement handler does not run, the cast's own turn toward its aim does
    /// (`0x14025bd40`, combat side). Set by `Match::step` like `hold_air`.
    casting: bool,
    /// Jump wind-up (state 4, `0x140257b90`): sim ticks left before take-off
    /// and the launch speed the take-off gives. Only the Jump skill (239) uses
    /// it; the dodge jump's wind-up is 0-1 ticks (its start clip is 2-3
    /// frames) and stays folded into the press.
    windup: u16,
    launch_v: f32,
    /// A skill jump (Jump / Levitate / Skywalk / Demon's Wing) left the ground this tick
    /// (`FUN_14025a0b0` -> `FUN_1402549e0`, the take-off sound). Raised by the step, consumed by
    /// `Combat::tick_fighter` (`CombatEvent::JumpTakeoff`); never read by the movement itself.
    pub(crate) took_off: bool,
    /// The body touched down from the airborne state this tick, by this fall height (highest
    /// point reached minus the landing point): `FUN_140254c80`'s state-5 branch, which ends by
    /// calling `FUN_140254b30` (the landing sound of the floor material). Raised by the step,
    /// consumed by `Combat::tick_fighter` (`CombatEvent::Landed`); never read by the movement.
    pub(crate) landed: Option<f32>,
    /// Facing at the start point: a fall-out respawn faces it again
    /// (`0x140251d80` copies `+0x12e58` to the facing, the heading and the
    /// camera).
    pub spawn_yaw: f32,
    /// Mid-flight from a knock-down launch: the combat side owns the body
    /// (`Combat::step_knockdown_flight`: launch velocity, the program's own
    /// gravity, wall and ground contact), so the ordinary jump/fall physics
    /// and the stick-to-the-floor step below must not also move it. Set by
    /// `Match::step` from the fighter each tick.
    launched: bool,
    /// In a hit reaction, knock-down or KO (states 14-19, 27): the idle "turn
    /// back toward the camera" does not run (`0x14025b5e0` is only the
    /// idle/pivot states' handler), so the body keeps the facing the
    /// reaction gave it. Set by `Match::step` like `casting`.
    reacting: bool,
    /// The jump in progress is a float launch (Levitate / Skywalk,
    /// `begin_float`): it rises like the Jump skill but, once it stops rising
    /// while status 11 (the float status) holds, the vertical speed is
    /// zeroed every tick and the body hangs at the top (`0x140254590`, the
    /// `vy <= 0` test; state 7, `0x14025b170`). Not in the replay hash: it
    /// only ever matters after a float skill has been cast, and its effect
    /// shows in the hashed position.
    float_rise: bool,
    /// `float_rise` and the float status still running this tick; set by
    /// `Match::step` from the combat side before the body moves, like
    /// `casting`. Not hashed (derived each tick).
    float_hold: bool,
    /// A scripted dash (the Dash skill, `FUN_140270200` case 8 -> state 3,
    /// `FUN_14025afe0`): sim ticks left to move and the speed in u/s. Each
    /// of the program's `dash length` ticks the body moves `speed` units along
    /// its facing; here that is 2 sim ticks per program tick at half the
    /// step. Not hashed (the position carries the effect).
    scripted_dash: u16,
    scripted_dash_speed: f32,
}

impl Player {
    /// The point the program's object tests, lock-on bearing, melee dash and knock-back direction read
    /// as this fighter's place (entity `+0x60` / `+0x68`, the world x / z of its BODY1 bone, not its
    /// position `+0x12e40`): the position moved by `Body::target_offset` of the facing. Valid for the
    /// standing / casting pose (the measured idle offset); a fighter in a reaction (hit, knocked, down)
    /// has its bone somewhere the data has not measured, so it keeps its own position.
    pub fn target_point(&self) -> Vec3 {
        if self.reacting {
            self.pos
        } else {
            self.pos + self.body.target_offset(self.yaw)
        }
    }

    pub fn new(spawn: Vec3, yaw: f32) -> Self {
        Player {
            pos: spawn,
            vel: Vec3::ZERO,
            yaw,
            grounded: true,
            motion: Motion::Idle,
            land_timer: 0.0,
            heavy_land: false,
            spawn,
            prev_pos: spawn,
            prev_yaw: yaw,
            locked: true,
            cpu_aim: false,
            cam_yaw: yaw,
            target: None,
            lock_on_events: 0,
            dodge: Dodge::Jump,
            body: Body::DEFAULT,
            dodge_ticks: 0,
            recover_ticks: 0,
            gait: Gait::Walk,
            strafe: None,
            heading: yaw,
            was_moving: false,
            dodge_dir: Vec3::Z,
            dodge_jumping: false,
            rise_seconds: 0.0,
            prev_jump: false,
            prev_lock: false,
            prev_switch: false,
            coyote: 0,
            air_peak_y: spawn.y,
            hold_air: false,
            casting: false,
            windup: 0,
            launch_v: 0.0,
            took_off: false,
            landed: None,
            spawn_yaw: yaw,
            launched: false,
            reacting: false,
            float_rise: false,
            float_hold: false,
            scripted_dash: 0,
            scripted_dash_speed: 0.0,
        }
    }

    /// The body state a skill press is judged against (`BodyState`).
    pub fn body_state(&self) -> BodyState {
        if self.dodge_ticks > 0 || self.recover_ticks > 0 || self.windup > 0 {
            BodyState::Busy
        } else if !self.grounded {
            BodyState::Air
        } else if self.land_timer > 0.0 {
            BodyState::Busy
        } else {
            BodyState::Ground
        }
    }

    /// In the ground dodge (state 3) or the jump wind-up (state 4): the two
    /// body states that pause Aura recovery besides casting (`0x1402523a0`).
    pub fn dodging_or_winding_up(&self) -> bool {
        self.dodge_ticks > 0 || self.recover_ticks > 0 || self.windup > 0
    }

    /// Starts the Jump skill's wind-up: `ticks` sim ticks of the start clip
    /// (JUMP_START, length - 2 program ticks, `0x140257b90`), then take-off at
    /// `launch` u/s under the ordinary gravity. The renderer plays JUMP_START
    /// while `jump_windup()` is non-zero.
    pub fn begin_jump(&mut self, ticks: u16, launch: f32) {
        self.windup = ticks.max(1);
        self.launch_v = launch;
    }

    /// Sim ticks of jump wind-up left (0 = none).
    #[allow(dead_code)] // the render lane plays JUMP_START while it is non-zero (request in the lane report)
    pub fn jump_windup(&self) -> u16 {
        self.windup
    }

    /// Levitate / Skywalk (`FUN_140270200` case 5): the Jump skill's wind-up
    /// and launch (`begin_jump`) with the float rule on top: at the top of the
    /// rise the body stops and hangs while the float status (the combat side's
    /// `status.hover_ticks`) runs, then falls under the ordinary gravity.
    pub fn begin_float(&mut self, ticks: u16, launch: f32) {
        self.begin_jump(ticks, launch);
        self.float_rise = true;
    }

    /// Kind 9 of the hit reactions (lift, `FUN_140258690` `0x140258d7f`): the body is thrown
    /// up at `launch` u/s under the ordinary gravity and, the float status being on, hangs
    /// where `vy` first reaches 0 (`float_rise`, as a Levitate rise does) until the status
    /// ends (reactions2 lane; `combat/reactions.rs`).
    pub(crate) fn lift(&mut self, launch: f32) {
        self.vel.y = launch;
        self.grounded = false;
        self.float_rise = true;
        self.dodge_jumping = false;
        self.rise_seconds = 0.0;
        self.air_peak_y = self.pos.y;
        self.coyote = 0;
    }

    /// The direction the stick points in the world (camera yaw + stick angle, the program's
    /// `+0x12e5c` + `+0x14270`): where the paralysed struggle turns the body
    /// (`0x14025b5e0`). `None` with the stick at rest.
    pub(crate) fn stick_world_yaw(&self, input: &Input) -> Option<f32> {
        let stick = input.stick();
        let mag = stick.length();
        if mag <= 1e-6 {
            return None;
        }
        let d = stick / mag;
        let fwd = Vec3::new(self.cam_yaw.sin(), 0.0, self.cam_yaw.cos());
        let right = Vec3::new(-fwd.z, 0.0, fwd.x);
        Some(yaw_of(right * d.x + fwd * d.y))
    }

    /// The Dash skill (`FUN_140270200` case 8): move `speed` u/s along the
    /// facing for `ticks` sim ticks, no steering (state 3, `FUN_14025afe0`).
    pub fn begin_dash(&mut self, ticks: u16, speed: f32) {
        self.scripted_dash = ticks;
        self.scripted_dash_speed = speed;
    }

    /// Sim ticks of scripted dash left (0 = none), for the renderer and tests.
    #[allow(dead_code)]
    pub fn dash_ticks_left(&self) -> u16 {
        self.scripted_dash
    }

    /// Hanging at the top of a Levitate / Skywalk rise this tick.
    #[allow(dead_code)]
    pub fn floating(&self) -> bool {
        self.float_hold && !self.grounded && self.vel.y == 0.0
    }

    /// A cast from a pivot drops it: the body takes the heading when the cast
    /// leaves the facing alone (`keep`, `0x140258585`), otherwise it keeps
    /// turning from where the pivot's offset left it.
    pub(crate) fn end_pivot(&mut self, keep: bool) {
        if self.strafe.take().is_some() && keep {
            let h = self.heading;
            snap_yaw(self, h);
        }
    }

    /// The pivot in progress, for the renderer (which clip, at what time).
    pub fn pivot(&self) -> Option<Pivot> {
        self.strafe
    }

    /// The movement heading (radians), for the trace.
    pub fn heading(&self) -> f32 {
        self.heading
    }
}

#[derive(Clone, Debug)]
pub struct Match {
    pub tick: u64,
    /// One body per slot; only the slots in `combat.roster` are ever stepped
    /// (the rest hold an inert placeholder at the origin).
    pub players: [Player; MAX_PLAYERS],
    pub combat: Combat,
    /// The stage's jump zones ("Jump Zone ... will take you high into the
    /// sky"): floor rectangles (min, max corners) and launch speed (u/s).
    pub jump_zones: Vec<(Vec3, Vec3, f32)>,
    /// Prop Hunt's rules and state (`prophunt.rs`); `None` in every other mode, which then steps exactly as before.
    pub prophunt: Option<crate::prophunt::PropHunt>,
}

/// Horizontal direction from `from` toward `to`, or +Z if they overlap.
pub fn toward(from: Vec3, to: Vec3) -> Vec3 {
    let d = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    if d.length_squared() < 1e-6 { Vec3::Z } else { d.normalize() }
}

impl Match {
    /// Desync check for online play and replays (docs/ONLINE-PLAN.md D5):
    /// a hash of everything that changes tick to tick -- the tick, every
    /// body, and `Combat::hash_state`. Static lookup tables (skills, clip
    /// lengths, action rows, jump zones) are left out: they are loaded from
    /// the cache, which the online handshake compares separately. Two
    /// machines that agree on this every tick are in sync.
    pub fn checksum(&self) -> u64 {
        use std::fmt::Write;
        let mut h = StateHasher::default();
        let _ = write!(h, "{}|{:?}|", self.tick, self.players);
        self.combat.hash_state(&mut h);
        // Only a Prop Hunt match has more to hash: every other match's checksum is what it always was.
        if let Some(ph) = &self.prophunt {
            let _ = write!(h, "|{ph:?}");
        }
        h.0
    }

    /// A two-player match (slots 0 and 1). `combat` must have the duel roster
    /// (`Combat::new` / `with_arsenals`).
    #[allow(dead_code)] // tests; the game starts every match with `with_spawns` now (fourview lane)
    pub fn new(spawns: [Vec3; 2], combat: Combat) -> Self {
        Self::with_spawns(&spawns, combat)
    }

    /// A match of any shape: the k-th active slot of `combat.roster` starts at
    /// `spawns[k]` (so `spawns.len()` must be the roster's player count).
    pub fn with_spawns(spawns: &[Vec3], combat: Combat) -> Self {
        assert_eq!(spawns.len(), combat.roster.count(), "one start point per active player");
        let mut players: [Player; MAX_PLAYERS] = std::array::from_fn(|_| Player::new(Vec3::ZERO, 0.0));
        for (k, i) in combat.roster.slots().enumerate() {
            players[i] = Player::new(spawns[k], 0.0);
        }
        let mut m = Match { tick: 0, players, combat, jump_zones: Vec::new(), prophunt: None };
        m.reset_positions();
        m
    }

    /// Everyone back on their spawn, facing their first enemy (lock-on choice kept).
    fn reset_positions(&mut self) {
        let spawns: [Vec3; MAX_PLAYERS] = std::array::from_fn(|i| self.players[i].spawn);
        for i in self.combat.roster.slots() {
            let (locked, dodge, body, lock_on_events, target) = (self.players[i].locked, self.players[i].dodge, self.players[i].body, self.players[i].lock_on_events, self.players[i].target);
            let facing = spawns[self.combat.aim_at(i, target)];
            self.players[i] = Player::new(spawns[i], yaw_of(toward(spawns[i], facing)));
            (self.players[i].locked, self.players[i].dodge, self.players[i].body, self.players[i].lock_on_events, self.players[i].target) = (locked, dodge, body, lock_on_events, target);
        }
    }

    /// Turns this match into Prop Hunt (`prophunt.rs`): round 1 is set up and everyone is put on their spawn.
    pub fn start_prop_hunt(&mut self, config: crate::prophunt::Config) {
        self.prophunt = Some(crate::prophunt::PropHunt::start(config, &mut self.combat));
        self.reset_positions();
    }

    /// One tick. `col` is the stage collision; the objects' part of it is
    /// switched to follow the sim's objects (broken ones stop blocking).
    pub fn step(&mut self, inputs: impl AsRef<[Input]>, col: &mut Collision) {
        // Prop Hunt: what each player may do this tick (the hiders' X / B, the seeker's countdown and knives).
        let filtered;
        let mut inputs = inputs.as_ref();
        if let Some(ph) = self.prophunt.as_mut() {
            filtered = ph.filter_inputs(inputs, &mut self.combat, &mut self.players);
            inputs = &filtered;
        }
        let roster = self.combat.roster;
        self.combat.sync_collision(col);
        for i in roster.slots() {
            let p = &mut self.players[i];
            p.prev_pos = p.pos;
            p.prev_yaw = p.yaw;
        }
        let positions: [Vec3; MAX_PLAYERS] = std::array::from_fn(|i| self.players[i].pos);
        let mut fell = [false; MAX_PLAYERS];
        for i in roster.slots() {
            let p = &mut self.players[i];
            let mut input = inputs.get(i).copied().unwrap_or_default();
            // Casting, dashing, stunned, down or bound: no running or jumping.
            if self.combat.fighters[i].rooted() || self.combat.counting_down() {
                // Paralysed (status 4) and pushing the stick while standing or walking: the
                // struggle (state 20, DMG_PARARISE), facing the stick (`0x14025b5e0`).
                if self.combat.can_struggle(i) && p.body_state() == BodyState::Ground {
                    if let Some(yaw) = p.stick_world_yaw(&input) {
                        self.combat.begin_struggle(i, yaw);
                    }
                }
                input.stick = [0, 0];
                input.jump = false;
                p.dodge_ticks = 0;
            }
            // Brain Concussion / Confused Gravity: "Inverts ... directional controls".
            if self.combat.inverted(i) {
                input.stick = input.stick.map(|v| v.saturating_neg());
            }
            // Optical Camo: "no one can lock onto you". The lock drops when nobody
            // is left to lock onto (with several candidates only the camouflaged
            // one stops being a valid target: `combat::lock.rs` hands over).
            if !self.combat.lockable_exists(i) {
                input.lock = false;
                p.locked = false;
            }
            // The lock target (`0x1402531d0`): re-picked while not locked, cycled on
            // the target button's rising edge while locked. `locked_now` is this
            // tick's lock state (`step_player_at` toggles it from `input.lock`).
            let cycle = input.switch_target && !p.prev_switch;
            p.prev_switch = input.switch_target;
            let locked_now = p.locked != (input.lock && !p.prev_lock);
            p.target = self.combat.next_target(i, p.target, locked_now, cycle);
            let opponent = positions[self.combat.aim_at(i, p.target)];
            let f = &self.combat.fighters[i];
            p.casting = matches!(f.action, Action::Cast | Action::Lunge);
            p.hold_air = p.casting && f.air_cast;
            p.launched = f.flying && matches!(f.action, Action::Knocked | Action::Down);
            p.reacting = matches!(f.action, Action::Hit | Action::Knocked | Action::Down);
            // A hit reaction ends the Dash skill's state 3 (the reaction is its own state).
            if p.reacting {
                p.scripted_dash = 0;
            }
            // The float status ran out or a hit cleared it: the float launch is over
            // and the body falls under the ordinary gravity (`FUN_14025b170` -> state 0).
            if f.status.hover_ticks == 0 {
                p.float_rise = false;
            }
            p.float_hold = p.float_rise;
            let before = p.pos;
            step_player_at(p, input, opponent, col, self.combat.move_factor(i));
            // Standing in a jump zone launches you (param = launch strength, unconfirmed as u/s).
            if p.grounded {
                if let Some(&(_, _, v)) = self.jump_zones.iter().find(|(lo, hi, _)| p.pos.x >= lo.x && p.pos.x <= hi.x && p.pos.z >= lo.z && p.pos.z <= hi.z && (p.pos.y - lo.y).abs() <= STEP_HEIGHT) {
                    p.vel.y = v;
                    p.grounded = false;
                }
            }
            // Lift Up / Antigravity Trap / Demon's Wing: held above the floor. (Levitate and
            // Skywalk are not pinned: they are a real jump that hangs at its top, `float_rise`.)
            let st = &self.combat.fighters[i].status;
            if st.hover_ticks > 0 && !p.float_rise {
                if let Some(g) = col.ground_below(p.pos.x, p.pos.z, p.pos.y + 1.0) {
                    p.pos.y = p.pos.y.max(g + st.hover_height).min(g + st.hover_height);
                    p.vel.y = 0.0;
                    p.grounded = false;
                }
            }
            // Fell out of the arena and was put back on the spawn.
            // (Judged against where the body was before this step: the respawn
            // rebuilds the player, `prev_pos` included, so the old test on
            // `prev_pos` never saw a fall and the Health loss never happened.)
            if p.pos == p.spawn && before.y < p.spawn.y - FALL_RESPAWN_BELOW + 30.0 {
                fell[i] = true;
            }
        }
        for i in roster.slots() {
            if fell[i] {
                self.combat.fall_out(i);
            }
        }
        let buttons: [u8; MAX_PLAYERS] = std::array::from_fn(|i| inputs.get(i).map_or(0, |inp| inp.skills));
        if self.combat.step(&mut self.players, buttons, col) {
            self.reset_positions();
        }
        self.combat.sync_collision(col);
        for i in roster.slots() {
            let (p, f) = (&mut self.players[i], &self.combat.fighters[i]);
            p.motion = match f.action {
                Action::Ready => p.motion,
                Action::Cast => Motion::Cast,
                Action::Lunge => Motion::Lunge,
                Action::Hit => Motion::Hit,
                // Still airborne from the launch (real physics, so no fixed
                // tick count) -> the collapse clip of a kind-1 reaction ->
                // lying -> getting up (PROGRAM-RE.md "Knock-down physics";
                // `Combat::start_knockdown` / `start_collapse`). A fighter who
                // is out plays the same reaction and then lies for good.
                Action::Knocked | Action::Down if f.ko_play && f.ko_staggering() => Motion::Hit,
                Action::Knocked | Action::Down if f.flying && (f.action == Action::Knocked || f.ko_play) => Motion::Blown,
                Action::Knocked | Action::Down if f.collapsing() && (f.action == Action::Knocked || f.ko_play) => Motion::Hit,
                Action::Knocked if f.action_ticks > KNOCK_GETUP_TICKS => Motion::Floored,
                Action::Knocked => Motion::GetUp,
                Action::Down if f.ko_play => Motion::Floored,
                Action::Down => Motion::Down,
            };
        }
        // Bodies don't overlap (`FUN_140250780`, `body.rs`): a pair whose horizontal gap is under
        // `contact_a / 2 + contact_b / 2` (10 u for two playable fighters) while the two `height / 2`
        // centres are closer than `(h_a + h_b) / 2` is pushed apart along the line between them, half
        // of the overlap each. Every pair, in slot order. The program pushes once per program tick,
        // after the state handlers have moved the bodies, so a fighter half way through a dash's
        // program step (`Combat::mid_dash_step`) is pushed after the second half.
        let slots: Vec<usize> = roster.slots().collect();
        for (k, &ia) in slots.iter().enumerate() {
            for &ib in &slots[k + 1..] {
                if self.combat.mid_dash_step(ia) || self.combat.mid_dash_step(ib) {
                    continue;
                }
                let (head, tail) = self.players.split_at_mut(ib);
                let (a, b) = (&mut head[ia], &mut tail[0]);
                let d = Vec3::new(b.pos.x - a.pos.x, 0.0, b.pos.z - a.pos.z);
                let min = a.body.radius() + b.body.radius();
                let dy = (b.pos.y + b.body.height * 0.5) - (a.pos.y + a.body.height * 0.5);
                if dy.abs() < (a.body.height + b.body.height) * 0.5 && d.length_squared() < min * min {
                    let len = d.length();
                    let n = if len > 1e-4 { d / len } else { Vec3::X };
                    let push = (min - len) * 0.5;
                    a.pos = col.push_out_of_walls(a.pos - n * push, a.body.wall_radius(), STEP_HEIGHT + 1.0, BODY_HEIGHT);
                    b.pos = col.push_out_of_walls(b.pos + n * push, b.body.wall_radius(), STEP_HEIGHT + 1.0, BODY_HEIGHT);
                }
            }
        }
        // Prop Hunt: knives paid for, catches, frozen hiders held, the round clock; a new round puts everyone back.
        if let Some(mut ph) = self.prophunt.take() {
            let new_round = ph.after_step(&mut self.combat, &mut self.players);
            self.prophunt = Some(ph);
            if new_round {
                self.reset_positions();
            }
        }
        self.tick += 1;
    }
}

pub fn yaw_of(dir: Vec3) -> f32 {
    dir.x.atan2(dir.z)
}

#[cfg(test)]
fn step_player(p: &mut Player, input: Input, opponent: Vec3, col: &Collision) {
    step_player_at(p, input, opponent, col, 1.0);
}

/// Turn `yaw` toward `target` by at most `max_step` (radians, shortest way);
/// returns the new yaw and the angle that was left to turn (radians, before
/// this tick's step; with `max_step` 0 it is just the angle between them --
/// the slowdown uses the angle left AFTER the step). Plain angle-wrap
/// arithmetic, not trigonometry: no sin/cos/atan is called here, so the
/// fixed 20-degrees-per-program-tick turn (PROGRAM-RE.md "Ground movement")
/// adds no new trig beyond the atan2 already used elsewhere in this file to
/// find `target` in the first place.
pub(crate) fn turn_toward(yaw: f32, target: f32, max_step: f32) -> (f32, f32) {
    use std::f32::consts::{PI, TAU};
    let mut diff = (target - yaw) % TAU;
    if diff > PI {
        diff -= TAU;
    } else if diff < -PI {
        diff += TAU;
    }
    let angle_left = diff.abs();
    (yaw + diff.clamp(-max_step, max_step), angle_left)
}

/// Sets the facing in one step, as the program's direct writes to `+0x12e68`
/// do (pivot entry, release, clip end): the renderer blends the facing
/// between the last two ticks, so the jump is folded into the tick's start
/// value too -- drawn as the instant switch it is, not smeared over a frame
/// while the clip has already changed.
fn snap_yaw(p: &mut Player, yaw: f32) {
    p.prev_yaw += wrap_angle(yaw - p.yaw);
    p.yaw = yaw;
}

/// `a` wrapped into (-pi, pi].
fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI { w + TAU } else { w }
}

/// The strafe side for a stick `rel` radians from the body's forward (our
/// convention: positive = left), or `None` inside the forward 45 deg (the
/// program's plain sectors, `0x14025b79d`: |a| <= 45 forward, 45 < |a| < 135
/// right/left, from 135 back-right/back-left). Exactly behind, the program
/// flips a coin (`rand`); the sim must replay identically, so it picks
/// back-right.
fn strafe_sector(rel: f32) -> Option<StrafeDir> {
    use std::f32::consts::{FRAC_PI_4, PI};
    let a = rel.abs();
    if a <= FRAC_PI_4 {
        None
    } else if a < 3.0 * FRAC_PI_4 {
        Some(if rel < 0.0 { StrafeDir::Right } else { StrafeDir::Left })
    } else if rel < 0.0 || a >= PI {
        Some(StrafeDir::BackRight)
    } else {
        Some(StrafeDir::BackLeft)
    }
}

/// Locked-on camera slew rate (radians/sim-tick), from
/// `LOCK_CAM_YAW_RATE_DEG_PER_S` unless `PD_LOCK_CAM_RATE` overrides it.
/// Read once per process: the sim must stay identical on every machine, so
/// a per-tick env read (a side channel into `Match::step`) is not allowed;
/// test switches are the only sanctioned use.
fn lock_cam_yaw_rate() -> f32 {
    static RATE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *RATE.get_or_init(|| {
        std::env::var("PD_LOCK_CAM_RATE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|&r| r > 0.0)
            .map(|r| r.to_radians())
            .unwrap_or(LOCK_CAM_YAW_RATE_DEG_PER_S.to_radians() * DT)
    })
}

/// `speed`: movement speed factor (status and environment skills).
fn step_player_at(p: &mut Player, input: Input, opponent: Vec3, col: &Collision, speed: f32) {
    // The Jump skill's wind-up ignores the stick and the dodge button (state 4).
    let input = if p.windup > 0 { Input { stick: [0, 0], jump: false, ..input } } else { input };
    if input.lock && !p.prev_lock {
        p.locked = !p.locked;
        p.lock_on_events = p.lock_on_events.wrapping_add(1);
    }
    p.prev_lock = input.lock;
    // The stick is read in the camera's frame, locked or not: the program's
    // movement, strafe entry test and idle turn all use the camera yaw
    // (`+0x12e5c` in `0x14025b5e0`, `0x14024f8f0`), never the opponent's
    // bearing directly. Locked, the camera swings toward the opponent at the
    // program's own rate (`LOCK_CAM_YAW_RATE_DEG_PER_S`, Laurence O4), so
    // "forward" is where the player's view points. Free: only the right
    // stick turns it (below).
    if p.locked {
        let (yaw, _) = turn_toward(p.cam_yaw, yaw_of(toward(p.pos, opponent)), lock_cam_yaw_rate());
        p.cam_yaw = yaw;
    }
    let cam = p.cam_yaw;
    let fwd = Vec3::new(cam.sin(), 0.0, cam.cos());
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);

    // Walk or run, nothing in between (PROGRAM-RE.md "Ground movement";
    // MOVEMENT-AUDIT.md step 2 and 3 reads): the gait compares the
    // dead-zone-processed magnitude -- run above 0.7, walk below 0.4, the
    // previous gait kept in between. Any processed movement at all moves at
    // the gait's full speed (speed is never scaled by the stick).
    let stick = input.stick();
    let mag = stick.length();
    if mag > RUN_GAIT_ON {
        p.gait = Gait::Run;
    } else if mag < STICK_DEAD_ZONE {
        p.gait = Gait::Walk;
    }
    let has_stick = mag > 1e-6;
    let stick_dir = if has_stick { stick / mag } else { Vec2::ZERO };
    // The stick's direction in world space (camera relative).
    let stick_world = right * stick_dir.x + fwd * stick_dir.y;
    let stick_yaw = yaw_of(stick_world);
    let gait_speed = (if p.gait == Gait::Run { RUN_SPEED } else { WALK_SPEED }) * speed;

    // Ground movement as the program's state machine (MOVEMENT-AUDIT.md
    // "Locked-on lane": `0x140256e80` dispatch, `0x14025b5e0` idle/entry,
    // `0x14025ad50` ordinary movement, `0x14025b270` pivot, `0x140257510`
    // clip clock -- read, not guessed):
    //  * idle (state 0): locked, the body turns back toward the camera at
    //    the turn rate;
    //  * ENTRY is the first tick the stick reads non-zero after idle or any
    //    action, landing or dodge (`was_moving` false). Nothing moves or
    //    turns that tick. Locked with the body within ~10 deg of the camera,
    //    a stick more than 45 deg off the body's forward starts a pivot (side
    //    from plain 45/135 deg sectors, chosen once); anything else starts
    //    ordinary movement. So a strafe can start mid-run only through a
    //    tick of zero stick (the square dead zone is ~40% per axis, so a
    //    flick through the centre does it) or the end of an action;
    //  * ordinary movement (state 1) never re-runs the entry test: the body
    //    turns toward the stick at the turn rate and moves at the gait speed
    //    times the slowdown for the angle still left AFTER this tick's turn
    //    (`0x14024f95f`); zero stick returns to idle without moving;
    //  * a pivot (state 8) first moves along its stored heading at full gait
    //    speed (even on the tick the stick is released), then: zero stick
    //    ends it with the body snapped to the heading; otherwise the heading
    //    chases the live stick at the turn rate and the body is held at
    //    heading + the side's fixed offset (`pivot_offset`). A gait change
    //    swaps to the paired clip at the same relative progress. When the
    //    clip's clock passes its length (below) the body takes the heading and
    //    ordinary movement follows -- the clip's own baked turn has already
    //    carried the body round, so nothing jumps.
    // Losing the lock does not end a pivot (step 3 finding 7).
    let landing_locked = p.land_timer > 0.0 || p.windup > 0;
    let airborne_or_busy = !p.grounded || landing_locked || p.dodge_ticks > 0 || p.recover_ticks > 0;
    if airborne_or_busy && p.strafe.take().is_some() {
        // Cut off by a jump, fall or dodge: face the way it was going (as a
        // cast from a pivot does, `0x140258585`; unconfirmed for jumps).
        let h = p.heading;
        snap_yaw(p, h);
    }
    let entry_ready = !p.was_moving;
    let heading_before = p.heading;
    let max_turn_step = TURN_RATE_DEG_PER_S.to_radians() * DT;
    let mut pivot_move = false;
    let mut entered = false;
    let mut angle_left = 0.0;
    if let Some(mut pv) = p.strafe {
        pivot_move = true;
        if !has_stick {
            let h = p.heading;
            snap_yaw(p, h);
            p.strafe = None;
        } else {
            p.heading = turn_toward(p.heading, stick_yaw, max_turn_step).0;
            p.yaw = p.heading + pivot_offset(pv.dir);
            if p.gait != pv.gait {
                pv.time *= pivot_len_30hz(pv.dir, p.gait) as f32 / pivot_len_30hz(pv.dir, pv.gait) as f32;
                pv.gait = p.gait;
            }
            p.strafe = Some(pv);
        }
    } else if has_stick && entry_ready && !airborne_or_busy {
        entered = true;
        p.heading = stick_yaw;
        if p.locked && turn_toward(p.yaw, cam, 0.0).1 <= STRAFE_ENTRY_ALIGN_DEG.to_radians() {
            if let Some(dir) = strafe_sector(wrap_angle(stick_yaw - p.yaw)) {
                snap_yaw(p, stick_yaw + pivot_offset(dir));
                p.strafe = Some(Pivot { dir, gait: p.gait, time: 0.0 });
            }
        }
    } else if has_stick {
        p.yaw = turn_toward(p.yaw, stick_yaw, max_turn_step).0;
        angle_left = turn_toward(p.yaw, stick_yaw, 0.0).1;
        p.heading = p.yaw;
    } else {
        if p.locked && !p.casting && !p.reacting && p.windup == 0 {
            p.yaw = turn_toward(p.yaw, cam, max_turn_step).0;
        }
        p.heading = p.yaw;
    }

    let turn_factor = (1.0 - angle_left / std::f32::consts::FRAC_PI_2).max(0.0);
    let mut wish = if pivot_move {
        Vec3::new(heading_before.sin(), 0.0, heading_before.cos()) * gait_speed
    } else if landing_locked || !has_stick || entered {
        Vec3::ZERO
    } else {
        Vec3::new(p.yaw.sin(), 0.0, p.yaw.cos()) * gait_speed * turn_factor
    };

    // The pivot clip's clock, after movement (`FUN_140257510`): it advances by
    // the playback scale (the speed status) and, once the proposed time is
    // strictly past the clip's length, the queued WALK/RUN takes over with no
    // blend and the body takes the heading (`0x140257694`).
    if let Some(mut pv) = p.strafe {
        let proposed = pv.time + ANIM_FRAMES_PER_TICK * speed;
        if proposed > pivot_len_30hz(pv.dir, pv.gait) as f32 {
            let h = p.heading;
            snap_yaw(p, h);
            p.strafe = None;
        } else {
            pv.time = proposed;
            p.strafe = Some(pv);
        }
    }
    let strafe = p.strafe.map(|pv| pv.dir);

    let jump_pressed = input.jump && !p.prev_jump;
    p.prev_jump = input.jump;
    // Tumble / cartwheel: a burst along the stick (or forward), then for a
    // cartwheel a recovery that holds you still. The landing lock (below)
    // roots the player the same way casting/stunned does in `Match::step`.
    let busy = p.dodge_ticks > 0 || p.recover_ticks > 0 || landing_locked;
    if p.grounded && jump_pressed && !busy && p.dodge != Dodge::Jump {
        // Along the movement, else the stick (an entry tick has no movement
        // yet; the program aims the dodge at camera + stick, `0x14025b5e0`),
        // else straight ahead.
        p.dodge_dir = if wish.length_squared() > 1e-6 {
            Vec3::new(wish.x, 0.0, wish.z).normalize()
        } else if has_stick {
            stick_world
        } else {
            fwd
        };
        p.dodge_ticks = if p.dodge == Dodge::Cartwheel { CARTWHEEL_TICKS } else { TUMBLE_TICKS };
    }
    if p.dodge_ticks > 0 {
        let v = if p.dodge == Dodge::Cartwheel { CARTWHEEL_SPEED } else { TUMBLE_SPEED };
        wish = p.dodge_dir * v * speed;
        p.dodge_ticks -= 1;
        if p.dodge_ticks == 0 && p.dodge == Dodge::Cartwheel {
            p.recover_ticks = CARTWHEEL_RECOVER_TICKS;
        }
    } else if p.recover_ticks > 0 {
        wish = Vec3::ZERO;
        p.recover_ticks -= 1;
    }

    // The Dash skill (`FUN_140270200` case 8, state 3 `FUN_14025afe0`): a fixed step along the
    // facing each tick of its window, no steering, no smoothing (`FUN_14024f9c0` adds
    // `facing x speed` to the position). The step is the program's per 30 Hz tick; at 60 Hz it is
    // half of it twice (`scripted_dash_speed` is already in u/s).
    let dashing = p.scripted_dash > 0;
    if dashing {
        wish = Vec3::new(p.yaw.sin(), 0.0, p.yaw.cos()) * p.scripted_dash_speed;
        p.scripted_dash -= 1;
    }
    let control = if p.grounded || dashing { 1.0 } else { AIR_CONTROL };
    p.vel.x += (wish.x - p.vel.x) * control;
    p.vel.z += (wish.z - p.vel.z) * control;

    // The Jump skill's wind-up (state 4): rooted for its start clip, then the
    // launch under the ordinary gravity (`0x140257b90`, `0x14025a0b0`).
    if p.windup > 0 {
        p.windup -= 1;
        if p.windup == 0 && p.grounded {
            p.vel.y = p.launch_v;
            p.grounded = false;
            p.took_off = true;
            p.dodge_jumping = false;
            p.rise_seconds = if p.launch_v > 0.0 { p.launch_v / GRAVITY } else { 0.0 };
        }
    }
    if p.grounded && jump_pressed && !busy && p.dodge == Dodge::Jump {
        p.vel.y = JUMP_VELOCITY;
        p.grounded = false;
        p.dodge_jumping = true;
        p.rise_seconds = JUMP_VELOCITY / DODGE_JUMP_GRAVITY;
    }
    let was_grounded = p.grounded;
    if p.launched {
        // The flight is the combat side's (`launched`): no gravity, no floor
        // stick, no wall push here. Nothing counts as a fall to land from.
        p.vel = Vec3::ZERO;
        p.grounded = false;
        p.air_peak_y = p.pos.y;
        p.coyote = 0;
    } else {
        if p.hold_air && !p.grounded {
            // A mid-air cast hangs the body: no vertical step, no drift (state 11).
            p.vel = Vec3::ZERO;
        } else if !p.grounded {
            // The Jump dodge (750) falls under its own, heavier gravity
            // (PROGRAM-RE.md); an ordinary fall (walked off a ledge, or any
            // other airborne state) uses the base one. Both share the same cap.
            let g = if p.dodge_jumping { DODGE_JUMP_GRAVITY } else { GRAVITY };
            p.vel.y = (p.vel.y - g * DT).max(-FALL_SPEED_CAP);
            // Floating (Levitate / Skywalk): the moment the body stops rising while the float
            // status holds, the vertical speed is zeroed, every tick (`0x140254590`: after
            // `vy -= g`, `vy <= 0 && vy != 0` with status 11 -> `vy = 0`). The apex is wherever
            // the rise ran out, i.e. `10 x s45` u.
            if p.float_hold && p.vel.y <= 0.0 {
                p.vel.y = 0.0;
            }
        }

        let mut next = p.pos + p.vel * DT;
        // Walls: push the body cylinder (above step height, up to the head) out sideways.
        next = col.push_out_of_walls(next, p.body.wall_radius(), STEP_HEIGHT + 1.0, BODY_HEIGHT);
        // Ground: the highest standable surface not above our feet + a step.
        let ground = col.ground_below(next.x, next.z, p.pos.y.max(next.y) + STEP_HEIGHT);
        p.pos = next;
        match ground {
            Some(g) if p.pos.y <= g + 0.01 && p.vel.y <= 0.0 => {
                p.pos.y = g;
                p.vel.y = 0.0;
                p.grounded = true;
                p.coyote = 0;
            }
            Some(g) if was_grounded && p.pos.y - g <= STEP_HEIGHT && p.vel.y <= 0.0 => {
                // Walking down a small step: stick to the floor.
                p.pos.y = g;
                p.vel.y = 0.0;
                p.coyote = 0;
            }
            // Tiny gaps between floor triangles (or the first moment off a
            // ledge): stay grounded for a few ticks instead of flickering into
            // a fall.
            _ if was_grounded && p.vel.y <= 0.0 && p.coyote < COYOTE_TICKS => p.coyote += 1,
            _ => p.grounded = false,
        }
    }
    if p.pos.y < p.spawn.y - FALL_RESPAWN_BELOW {
        // The program puts the body back on the start point facing the start
        // yaw, with the camera and heading too (`0x140251d80`).
        let (locked, dodge, body, lock_on_events, target) = (p.locked, p.dodge, p.body, p.lock_on_events, p.target);
        *p = Player::new(p.spawn, p.spawn_yaw);
        (p.locked, p.dodge, p.body, p.lock_on_events, p.target) = (locked, dodge, body, lock_on_events, target);
        return;
    }

    // Free camera: only the right stick turns it (the original: "You cannot
    // move the camera while you are locked onto an enemy"); a click recentres
    // it behind you. Running with the left stick never turns it (Laurence:
    // like modern third-person games).
    if !p.locked && input.cam != 0 {
        // Stick right turns the view right; yaw grows to the left.
        p.cam_yaw -= input.cam as f32 / 127.0 * if input.cam_fast { 2.0 } else { 1.0 } * CAM_TURN_SPEED * DT;
    } else if !p.locked && input.recenter {
        p.cam_yaw = p.yaw;
    }

    if p.grounded {
        if !was_grounded {
            // Just touched down: pick the landing by fall height (highest
            // point reached minus the landing point), not airborne ticks
            // (PROGRAM-RE.md "Landing by fall height"). Below 40 u: no
            // landing clip, straight back to idle/run.
            let fall_height = p.air_peak_y - p.pos.y;
            // Only the airborne state (5) has a landing lock; a lifted body lands in its reaction
            // (state 14, `0x140254c80` handles states 5 and 0x11 only).
            if !p.reacting {
                p.landed = Some(fall_height);
                if fall_height > LAND_FALL_HEAVY {
                    p.land_timer = HEAVY_LAND_TIME;
                    p.heavy_land = true;
                } else if fall_height > LAND_FALL_MIN {
                    p.land_timer = LAND_TIME;
                    p.heavy_land = false;
                }
            }
            p.dodge_jumping = false;
            p.rise_seconds = 0.0;
        }
        // Ready for the next flight (also covers "was already grounded").
        p.air_peak_y = p.pos.y;
    } else {
        p.air_peak_y = p.air_peak_y.max(p.pos.y);
    }
    p.land_timer = (p.land_timer - DT).max(0.0);
    p.motion = if p.dodge_ticks > 0 {
        Motion::Dodge
    } else if p.recover_ticks > 0 {
        Motion::Recover
    } else if !p.grounded {
        if p.vel.y > 0.0 { Motion::Rise } else { Motion::Fall }
    } else if p.windup > 0 {
        // Standing in the Jump skill's start clip: the renderer reads `jump_windup()`.
        Motion::Idle
    } else if p.land_timer > 0.0 {
        // Locked for the whole landing clip now (replaces the old "moving
        // cancels a light landing" rule) -- PROGRAM-RE.md "Landing".
        if p.heavy_land { Motion::HeavyLand } else { Motion::Land }
    } else if let Some(dir) = strafe {
        Motion::Strafe(dir)
    } else if has_stick {
        if p.gait == Gait::Run { Motion::Run } else { Motion::Walk }
    } else {
        Motion::Idle
    };
    p.was_moving = matches!(p.motion, Motion::Walk | Motion::Run | Motion::Strafe(_));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> Collision {
        let s = 1000.0;
        Collision::new(vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
        ])
    }

    /// Taps the dodge button (no stick: straight ahead, toward the opponent
    /// at +Z), then lets go for `ticks`; returns the player.
    fn dodge_run(dodge: Dodge, ticks: usize) -> Player {
        let col = floor();
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.dodge = dodge;
        let far = Vec3::new(0.0, 0.0, 5000.0);
        step_player(&mut p, Input { jump: true, ..Default::default() }, far, &col);
        for _ in 0..ticks {
            step_player(&mut p, Input::default(), far, &col);
        }
        p
    }

    #[test]
    fn tumble_rolls_forward_on_the_ground_with_no_recovery() {
        let mid = dodge_run(Dodge::Tumble, 10);
        assert_eq!(mid.motion, Motion::Dodge);
        assert!(mid.grounded, "stays on the ground");
        let end = dodge_run(Dodge::Tumble, TUMBLE_TICKS as usize);
        assert!((end.pos.z - TUMBLE_SPEED * TUMBLE_TICKS as f32 * DT).abs() < 3.0, "went {}", end.pos.z);
        assert_eq!((end.motion, end.recover_ticks), (Motion::Idle, 0));
    }

    #[test]
    fn cartwheel_is_faster_then_holds_you_to_recover() {
        let end = dodge_run(Dodge::Cartwheel, CARTWHEEL_TICKS as usize);
        assert!(end.pos.z > dodge_run(Dodge::Tumble, CARTWHEEL_TICKS as usize).pos.z);
        assert_eq!(end.motion, Motion::Recover);
        // Recovering: the stick does nothing.
        let (col, mut p) = (floor(), end);
        let z = p.pos.z;
        step_player(&mut p, Input::from_axes(0.0, 1.0, false, false), Vec3::new(0.0, 0.0, 5000.0), &col);
        assert!((p.pos.z - z).abs() < 0.5);
    }

    #[test]
    fn jumpers_still_jump() {
        let p = dodge_run(Dodge::Jump, 5);
        assert!(!p.grounded && p.pos.y > 0.0);
    }

    #[test]
    fn the_double_scale_turn_is_twice_the_top_speed_and_never_used_below_it() {
        for v in [-1.0f32, -0.5, -0.01, 0.0, 0.3, 1.0] {
            assert_eq!(Input::cam_value(v), ((v * 127.0).round() as i8, false), "up to the top speed: as before");
        }
        assert_eq!(Input::cam_value(2.0), (127, true));
        assert_eq!(Input::cam_value(-1.5), (-95, true));
        let i = Input { cam: 64, cam_fast: true, ..Default::default() };
        assert_eq!(Input::from_bytes(i.to_bytes()), i, "the flag survives the wire");
        let col = floor();
        let turn = |fast: bool| {
            let mut p = Player::new(Vec3::ZERO, 0.0);
            p.locked = false;
            step_player(&mut p, Input { cam: 127, cam_fast: fast, ..Default::default() }, Vec3::new(0.0, 0.0, 100.0), &col);
            p.cam_yaw
        };
        let (slow, fast) = (turn(false), turn(true));
        assert!((fast - 2.0 * slow).abs() < 1e-5, "{slow} vs {fast}");
    }

    #[test]
    fn right_stick_turns_free_camera_only() {
        let col = floor();
        let mut p = Player::new(Vec3::ZERO, 0.0);
        let turn = Input { cam: 127, ..Default::default() };
        step_player(&mut p, turn, Vec3::new(0.0, 0.0, 100.0), &col);
        assert_eq!(p.cam_yaw, 0.0, "locked on: camera stays on the opponent");
        p.locked = false;
        for _ in 0..30 {
            step_player(&mut p, turn, Vec3::new(0.0, 0.0, 100.0), &col);
        }
        assert!((p.cam_yaw + CAM_TURN_SPEED * 0.5).abs() < 1e-3, "half a second at full tilt, turning right");
        step_player(&mut p, Input { recenter: true, ..Default::default() }, Vec3::new(0.0, 0.0, 100.0), &col);
        assert_eq!(p.cam_yaw, p.yaw, "click recentres behind the player");
    }

    /// O4, lockon lane: locking on slews the camera toward the opponent at
    /// the program's own rate instead of snapping it there.
    #[test]
    fn locking_on_slews_the_camera_at_the_program_rate() {
        let col = floor();
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.locked = false;
        p.cam_yaw = 0.0;
        // Opponent due +X: target yaw exactly +90 deg.
        let foe = Vec3::new(100.0, 0.0, 0.0);
        let target = std::f32::consts::FRAC_PI_2;
        step_player(&mut p, Input { lock: true, ..Default::default() }, foe, &col);
        step_player(&mut p, Input::default(), foe, &col);
        assert!(p.locked && p.lock_on_events == 1);
        let step = LOCK_CAM_YAW_RATE_DEG_PER_S.to_radians() * DT;
        assert!((step - 3.0f32.to_radians()).abs() < 1e-6, "3 deg/sim-tick at the shipped 180 deg/s");
        // Two ticks in (toggle tick + one): exactly two rate steps, no snap.
        assert!((p.cam_yaw - 2.0 * step).abs() < 1e-6, "eases, does not snap: cam_yaw {}", p.cam_yaw);
        let mut prev = p.cam_yaw;
        let mut arrived = 0;
        for t in 0..60 {
            step_player(&mut p, Input::default(), foe, &col);
            assert!(p.cam_yaw >= prev - 1e-6, "monotonic toward the target, tick {t}");
            prev = p.cam_yaw;
            let home = (p.cam_yaw - target).abs() < 1e-4;
            if home {
                arrived = t;
                break;
            }
        }
        assert!(arrived <= 30, "90 deg at 3 deg/tick arrives within 30 ticks, took {arrived}");
        step_player(&mut p, Input::default(), foe, &col);
        assert!(p.cam_yaw - target < 1e-4 && target - p.cam_yaw < 1e-4, "stays on the target, no overshoot");
    }

    /// The renderer plays the lock-on cue off this counter (sfx.rs): every
    /// press edge counts, holds don't, direction reads off `locked`.
    #[test]
    fn lock_toggle_counts_for_the_renderer() {
        let col = floor();
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.locked = false;
        let foe = Vec3::new(0.0, 0.0, 100.0);
        let press = Input { lock: true, ..Default::default() };
        step_player(&mut p, press, foe, &col);
        assert!((p.locked, p.lock_on_events) == (true, 1));
        for _ in 0..5 {
            step_player(&mut p, press, foe, &col);
        }
        assert!((p.locked, p.lock_on_events) == (true, 1), "holding the button never re-toggles");
        step_player(&mut p, Input::default(), foe, &col);
        step_player(&mut p, press, foe, &col);
        assert!((p.locked, p.lock_on_events) == (false, 2), "off counts too (the renderer stays silent on it)");
    }

    fn duel(a: Vec3, b: Vec3, bodies: [Body; 2]) -> Match {
        let combat = crate::combat::Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
        let mut m = Match::new([a, b], combat);
        (m.players[0].body, m.players[1].body) = (bodies[0], bodies[1]);
        m
    }

    fn gap(m: &Match) -> f32 {
        Vec3::new(m.players[1].pos.x - m.players[0].pos.x, 0.0, m.players[1].pos.z - m.players[0].pos.z).length()
    }

    /// `FUN_140250780`: two bodies closer than `size_a / 2 + size_b / 2` are pushed apart, half of the
    /// overlap each, along the line between them: two playable fighters (contact size 10) end exactly
    /// 10.0 u apart (the originals stop 10.0 apart, Swift Kick / Psycho Knife takes), pushed
    /// symmetrically about their midpoint; a pair already further apart does not move.
    #[test]
    fn two_fighters_are_pushed_to_ten_units_apart_half_each() {
        let mut col = floor();
        let mut m = duel(Vec3::new(-1.5, 0.0, 0.0), Vec3::new(1.5, 0.0, 0.0), [Body::of("pc00a").unwrap(), Body::of("pc01a").unwrap()]);
        m.step([Input::default(); 2], &mut col);
        assert!((gap(&m) - 10.0).abs() < 1e-3, "gap {}", gap(&m));
        assert!((m.players[0].pos.x + 5.0).abs() < 1e-3 && (m.players[1].pos.x - 5.0).abs() < 1e-3, "half each: {:?} {:?}", m.players[0].pos, m.players[1].pos);
        let mut far = duel(Vec3::new(-5.01, 0.0, 0.0), Vec3::new(5.01, 0.0, 0.0), [Body::DEFAULT; 2]);
        far.step([Input::default(); 2], &mut col);
        assert_eq!((far.players[0].pos.x, far.players[1].pos.x), (-5.01, 5.01), "10.02 apart: not touched");
        // The same body in the same place, whichever fighter: size 10 for every playable record.
        for k in 0..=10 {
            assert_eq!(Body::of(&format!("pc{k:02}a")).unwrap().radius() * 2.0, 10.0, "pc{k:02}a");
        }
    }

    /// The contact distance is the two records' sizes halved and added, so a character with another size
    /// stands further out (Germano, record 25, size 12: 11.0 from a playable fighter); coincident bodies
    /// leave along +x (`local_138 = 1.0` when the distance is 0).
    #[test]
    fn the_contact_distance_is_each_fighters_own_size_halved() {
        let mut col = floor();
        let mut m = duel(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), [Body::of("pc00a").unwrap(), Body::of("enm14a").unwrap()]);
        m.step([Input::default(); 2], &mut col);
        assert!((gap(&m) - 11.0).abs() < 1e-3, "gap {}", gap(&m));
        let mut same = duel(Vec3::ZERO, Vec3::ZERO, [Body::DEFAULT; 2]);
        same.step([Input::default(); 2], &mut col);
        assert!((same.players[1].pos.x - same.players[0].pos.x - 10.0).abs() < 1e-3 && same.players[0].pos.z == same.players[1].pos.z);
    }

    /// `FUN_140255020` against the stage: the body keeps `contact / 2` = 5.0 u from a wall (it was 4.5).
    #[test]
    fn a_fighter_stops_five_units_from_a_wall() {
        let w = 40.0;
        let col = Collision::new(vec![
            [Vec3::new(20.0, 0.0, -w), Vec3::new(20.0, 60.0, -w), Vec3::new(20.0, 60.0, w)],
            [Vec3::new(20.0, 0.0, -w), Vec3::new(20.0, 60.0, w), Vec3::new(20.0, 0.0, w)],
            [Vec3::new(-1000.0, 0.0, -1000.0), Vec3::new(1000.0, 0.0, -1000.0), Vec3::new(1000.0, 0.0, 1000.0)],
            [Vec3::new(-1000.0, 0.0, -1000.0), Vec3::new(1000.0, 0.0, 1000.0), Vec3::new(-1000.0, 0.0, 1000.0)],
        ]);
        let mut p = Player::new(Vec3::ZERO, std::f32::consts::FRAC_PI_2);
        let toward_wall = Vec3::new(1000.0, 0.0, 0.0);
        for _ in 0..180 {
            step_player(&mut p, Input { stick: [0, 127], ..Default::default() }, toward_wall, &col);
        }
        assert!((p.pos.x - 15.0).abs() < 0.05, "stopped at x {} (wall at 20, radius 5)", p.pos.x);
    }

    /// The vertical part of the same test: the two `height / 2` centres must be closer than
    /// `(h_a + h_b) / 2`, so two 18 u fighters 17.9 u apart in height collide and 18.1 u apart do not;
    /// an 18 u fighter above a 20 u one (Meister) is reached up to 20 (centres 10 and `y + 9`).
    #[test]
    fn bodies_on_different_levels_collide_only_inside_their_combined_half_heights() {
        let hero = Body::of("pc00a").unwrap();
        let meister = Body::of("pc03a").unwrap();
        let mut col = floor();
        for (above, bodies, pushed) in [(17.8, [hero, hero], true), (18.2, [hero, hero], false), (19.8, [meister, hero], true), (20.2, [meister, hero], false)] {
            let mut m = duel(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, above, 0.0), bodies);
            m.step([Input::default(); 2], &mut col);
            assert_eq!(gap(&m) > 5.0, pushed, "{above} u up with {bodies:?}: gap {}", gap(&m));
        }
    }

    /// Falling out mid-match rebuilds the player: the cue counter must ride
    /// along, or the respawn reads as a fresh toggle to the renderer.
    #[test]
    fn lock_counter_survives_a_fall_respawn() {
        let mut col = floor();
        let combat = crate::combat::Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat);
        m.players[0].locked = true;
        m.players[0].lock_on_events = 7;
        m.players[0].pos.y = m.players[0].spawn.y - FALL_RESPAWN_BELOW - 1.0;
        m.step([Input::default(); 2], &mut col);
        assert_eq!(m.players[0].pos, m.players[0].spawn, "respawned");
        assert!((m.players[0].locked, m.players[0].lock_on_events) == (true, 7), "counter kept");
    }

    /// Online desync check (docs/ONLINE-PLAN.md D5): two matches fed the
    /// same inputs agree on `checksum()` every tick; one different input
    /// makes them disagree from then on.
    #[test]
    fn checksum_tracks_identical_and_diverging_runs() {
        let new = || {
            let combat = crate::combat::Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
            Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat)
        };
        let (mut a, mut b, mut c) = (new(), new(), new());
        let (mut ca, mut cb, mut cc) = (floor(), floor(), floor());
        assert_eq!(a.checksum(), b.checksum());
        let mut diverged = false;
        for t in 0..400u32 {
            let i = Input::from_axes(((t / 37) % 3) as f32 - 1.0, 1.0, t % 90 == 0, t == 200);
            a.step([i, Input::default()], &mut ca);
            b.step([i, Input::default()], &mut cb);
            // `c` jumps once more after the countdown.
            let ic = if t == 250 { Input { jump: true, ..i } } else { i };
            c.step([ic, Input::default()], &mut cc);
            assert_eq!(a.checksum(), b.checksum(), "tick {t}");
            diverged |= a.checksum() != c.checksum();
        }
        assert!(diverged, "a different input changes the checksum");
    }

    /// Locking on mid-run is deterministic: two identical input scripts
    /// (walk, then lock while moving) produce identical state every tick --
    /// the slew and the counter add no wall clock or randomness.
    #[test]
    fn lock_slew_replays_identically_while_moving() {
        let run = || {
            let mut col = floor();
            let combat = crate::combat::Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
            let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat);
            m.players[0].locked = false;
            m.players[0].cam_yaw = 1.0;
            let mut trace = Vec::new();
            // Past the 180-tick countdown so the stick actually walks: the
            // lock lands mid-run, not while standing.
            for t in 0..260 {
                let mut input = Input::from_axes(0.0, 1.0, false, false);
                if t == 200 {
                    input.lock = true;
                }
                m.step([input, Input::default()], &mut col);
                let p = &m.players[0];
                trace.push((p.pos.to_array(), [p.yaw, p.cam_yaw], p.locked, p.lock_on_events));
            }
            trace
        };
        let (a, b) = (run(), run());
        assert_eq!(a.len(), b.len());
        for (t, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(x, y, "tick {t} differs");
        }
        let last = a.last().unwrap();
        assert!(last.2 && last.3 == 1, "locked mid-run with one counted toggle");
        assert!(last.0[0].abs() + last.0[2].abs() > 1.0, "actually walked: {:?}", last.0);
    }

    #[test]
    fn a_jump_zone_launches_a_player_standing_in_it() {
        let s = 1000.0;
        let mut col = Collision::new(vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
        ]);
        let combat = crate::combat::Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat);
        m.jump_zones = vec![(Vec3::new(-10.0, 0.0, -10.0), Vec3::new(10.0, 0.0, 10.0), 200.0)];
        for _ in 0..(crate::combat::COUNTDOWN_TICKS + 5) {
            m.step([Input::default(); 2], &mut col);
        }
        assert!(m.players[0].pos.y > 5.0 || m.players[0].vel.y > 0.0, "launched: y {} vy {}", m.players[0].pos.y, m.players[0].vel.y);
        assert!(m.players[1].pos.y.abs() < 1e-3, "not in the zone");
    }


    #[test]
    fn a_broken_floor_drops_whoever_stood_on_it() {
        use pd_formats::collision::NO_OBJECT;
        let (s, b) = (1000.0, 40.0);
        // Ground at 0; a bridge (object 0) at 30 under player 0's spawn.
        let col = || {
            Collision::new_tagged(
                vec![
                    [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
                    [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
                    [Vec3::new(-b, 30.0, -b), Vec3::new(b, 30.0, -b), Vec3::new(b, 30.0, b)],
                    [Vec3::new(-b, 30.0, -b), Vec3::new(b, 30.0, b), Vec3::new(-b, 30.0, b)],
                ],
                &[NO_OBJECT, NO_OBJECT, 0, 0],
            )
        };
        let mut col = col();
        let mut combat = crate::combat::Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
        combat.props = vec![crate::props::Prop::new(0, 2, 23, Vec3::new(-b, 28.0, -b), Vec3::new(b, 30.0, b), false, true)];
        let mut m = Match::new([Vec3::new(0.0, 30.0, 0.0), Vec3::new(0.0, 0.0, 200.0)], combat);
        for _ in 0..30 {
            m.step([Input::default(); 2], &mut col);
        }
        assert!((m.players[0].pos.y - 30.0).abs() < 1e-3, "standing on the bridge");
        m.combat.props[0].wear_down(99);
        for _ in 0..60 {
            m.step([Input::default(); 2], &mut col);
        }
        assert!(m.players[0].pos.y.abs() < 1e-3, "fell to the ground: y {}", m.players[0].pos.y);
    }

    #[test]
    fn running_never_turns_the_free_camera() {
        let col = floor();
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.locked = false;
        let yaw = p.cam_yaw;
        for _ in 0..120 {
            step_player(&mut p, Input::from_axes(1.0, 0.3, false, false), Vec3::new(0.0, 0.0, 100.0), &col);
        }
        assert!(p.pos.length() > 50.0, "ran");
        assert_eq!(p.cam_yaw, yaw, "only the right stick turns it");
    }

    /// A tick with the given raw stick magnitude, pointing straight ahead.
    fn forward_mag(mag: f32) -> Input {
        Input { stick: [0, (mag * 127.0).round() as i8], ..Default::default() }
    }

    #[test]
    fn gait_switches_with_hysteresis() {
        // The program's gait compares the dead-zone-processed stick
        // (MOVEMENT-AUDIT.md step 2/3): raw push r straight ahead processes
        // to (r - 0.397) / 0.603, so run needs r > ~0.82 and a running
        // player drops to walk below r ~0.64; under r 0.397 nothing moves.
        let col = floor();
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.locked = false;
        let far = Vec3::new(0.0, 0.0, 100.0);
        step_player(&mut p, forward_mag(0.9), far, &col);
        assert_eq!(p.gait, Gait::Run, "processed 0.83 > 0.7 runs");
        assert_eq!(p.motion, Motion::Run);
        step_player(&mut p, forward_mag(0.75), far, &col);
        assert_eq!(p.gait, Gait::Run, "processed 0.59 is inside 0.4..0.7: keeps the current gait");
        step_player(&mut p, forward_mag(0.6), far, &col);
        assert_eq!(p.gait, Gait::Walk, "processed 0.34 < 0.4 drops back to walk");
        assert_eq!(p.motion, Motion::Walk, "and any processed movement still walks at full walk speed");
        step_player(&mut p, forward_mag(0.3), far, &col);
        assert_eq!(p.motion, Motion::Idle, "inside the dead zone reads as no stick");
        // From walk, the in-between band stays walk.
        let mut q = Player::new(Vec3::ZERO, 0.0);
        q.locked = false;
        step_player(&mut q, forward_mag(0.75), far, &col);
        assert_eq!(q.gait, Gait::Walk, "a 75% push from walk stays walk (processed 0.59)");
    }

    #[test]
    fn stick_dead_zone_is_square_and_rescaled() {
        assert_eq!(process_axis(0.39), 0.0);
        assert!((process_axis(1.0) - 1.0).abs() < 1e-6);
        assert!((process_axis(-1.0) + 1.0).abs() < 1e-6);
        // Half way between the dead zone and full scale.
        let mid = (STICK_AXIS_DEAD_ZONE + 1.0) / 2.0;
        assert!((process_axis(mid) - 0.5).abs() < 1e-5);
        // A full diagonal on a square gate reaches ~1.414 (not capped).
        let full = Input { stick: [127, 127], ..Default::default() }.stick();
        assert!((full.length() - 2f32.sqrt()).abs() < 1e-4, "{:?}", full);
        // A circular-gate diagonal at 100% (each axis 0.707) processes to
        // ~0.73: barely over the run threshold; at 90% it is ~0.56, inside the
        // keep-the-current-gait band (a player who was walking stays walking).
        let d = |r: f32| Input::from_axes(r * 0.7071, r * 0.7071, false, false).stick().length();
        assert!(d(1.0) > RUN_GAIT_ON && d(1.0) < 0.76, "{}", d(1.0));
        assert!(d(0.9) > STICK_DEAD_ZONE && d(0.9) < RUN_GAIT_ON, "{}", d(0.9));
    }

    #[test]
    fn turn_rate_and_slowdown_while_turning() {
        let col = floor();
        // Facing -170 deg, camera/stick-forward = +Z (yaw 0): a 170 deg turn,
        // the entry tick (no turn) then 17 ticks at 10 deg/tick (kept short
        // of the exact 180 deg ambiguity, where +/-180 are the same angle).
        let mut p = Player::new(Vec3::ZERO, (-170.0f32).to_radians());
        p.locked = false;
        p.cam_yaw = 0.0;
        let input = Input::from_axes(0.0, 1.0, false, false);
        let far = Vec3::new(0.0, 0.0, 1000.0);
        for _ in 0..8 {
            step_player(&mut p, input, far, &col);
        }
        assert!(p.pos.length() < 1e-3, "angle left > 90 deg: speed is cut to 0, pos {:?}", p.pos);
        for _ in 0..10 {
            step_player(&mut p, input, far, &col);
        }
        assert!(p.yaw.abs() < 1e-3, "fully turned 17 ticks after the entry tick (170 / 10 deg), yaw {}", p.yaw);
        let before = p.pos.z;
        step_player(&mut p, input, far, &col);
        assert!((p.pos.z - before - RUN_SPEED * DT).abs() < 1e-2, "cruising at full run speed once aligned");
    }

    #[test]
    fn gravity_matches_the_program_and_caps_the_fall() {
        // Spawn far below so the "fell out of the arena" respawn (150 u
        // below spawn) never fires during this artificial free-fall.
        let col = floor();
        let mut p = Player::new(Vec3::new(0.0, -1_000_000.0, 0.0), 0.0);
        p.pos.y = 1000.0;
        p.grounded = false;
        p.air_peak_y = 1000.0;
        let far = Vec3::new(0.0, 1000.0, 100.0);
        step_player(&mut p, Input::default(), far, &col);
        assert!((p.vel.y - (-GRAVITY * DT)).abs() < 1e-3, "one tick of the base 108 u/s^2, vel.y {}", p.vel.y);
        // Terminal velocity reaches the cap well under 125 ticks (225/108 s);
        // stop well short of hitting the floor (~330 ticks away).
        for _ in 0..200 {
            step_player(&mut p, Input::default(), far, &col);
        }
        assert!(!p.grounded, "still airborne at tick 201");
        assert!((p.vel.y + FALL_SPEED_CAP).abs() < 1e-3, "capped at 225 u/s, vel.y {}", p.vel.y);
    }

    #[test]
    fn landing_choice_by_fall_height() {
        // Falls straight down onto a flat floor at y = 0 from `height`, and
        // returns the player at the tick it lands.
        fn drop_from(height: f32) -> Player {
            let col = floor();
            let mut p = Player::new(Vec3::new(0.0, -1_000_000.0, 0.0), 0.0);
            p.pos.y = height;
            p.grounded = false;
            p.air_peak_y = height;
            p.locked = false;
            let far = Vec3::new(0.0, height, 100.0);
            for _ in 0..600 {
                step_player(&mut p, Input::default(), far, &col);
                if p.grounded {
                    break;
                }
            }
            p
        }
        let none = drop_from(30.0);
        assert!(none.grounded && none.land_timer == 0.0, "under 40 u: no landing clip");
        let light = drop_from(60.0);
        assert!(light.grounded && light.land_timer > 0.0 && !light.heavy_land, "40-80 u: light landing (JUMP_LAND1)");
        let heavy = drop_from(100.0);
        assert!(heavy.grounded && heavy.land_timer > 0.0 && heavy.heavy_land, "over 80 u: heavy landing (JUMP_LAND0)");
    }

    fn run_ticks(p: &mut Player, input: Input, n: usize) {
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        for _ in 0..n {
            step_player(p, input, opp, &col);
        }
    }

    /// A stick pushed at `mag` (0..1) and `angle_deg` from forward (0 = pure
    /// forward/toward the target, 90 = right, 180 = back -- sim.rs's own
    /// x=right/y=forward frame).
    fn stick_at(mag: f32, angle_deg: f32) -> Input {
        let a = angle_deg.to_radians();
        Input::from_axes(mag * a.sin(), mag * a.cos(), false, false)
    }

    /// Sim ticks from a pivot's entry tick (counted) until its hand-off tick
    /// (counted), for a steady push.
    fn pivot_ticks(input: Input) -> usize {
        let mut p = Player::new(Vec3::ZERO, 0.0);
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        for n in 1..1000 {
            step_player(&mut p, input, opp, &col);
            if n > 1 && !matches!(p.motion, Motion::Strafe(_)) {
                return n;
            }
        }
        panic!("pivot never ended");
    }

    #[test]
    fn locked_sideways_push_from_rest_pivots_then_runs_facing_where_it_goes() {
        // From rest, aligned with the camera, a full sideways push starts a
        // right pivot on the run pair (TURN_WALK_R, 24 animation frames):
        // nothing moves on the entry tick, the body is held at heading + 90
        // (still facing the camera), full run speed from the next tick along
        // the heading, and the clip ends -- body takes the heading -- on the
        // clock update proposing more than 24 frames: 2 * 24 sim ticks after
        // the entry tick.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        let input = Input::from_axes(1.0, 0.0, false, false);
        step_player(&mut p, input, opp, &col);
        assert!(matches!(p.motion, Motion::Strafe(StrafeDir::Right)), "entry tick is a right pivot, {:?}", p.motion);
        assert_eq!(p.pos, Vec3::ZERO, "nothing moves on the entry tick");
        assert!(p.yaw.abs() < 1e-4, "body held at heading + 90 = still facing the camera, yaw {}", p.yaw);
        assert!((p.heading + std::f32::consts::FRAC_PI_2).abs() < 1e-4, "heading = the stick (right = -90 deg), {}", p.heading);
        step_player(&mut p, input, opp, &col);
        assert!((p.pos.x + RUN_SPEED * DT).abs() < 0.01, "full run speed along the heading from the next tick, x {}", p.pos.x);
        run_ticks(&mut p, input, 46);
        assert!(matches!(p.motion, Motion::Strafe(_)), "still pivoting 47 ticks after entry, {:?}", p.motion);
        step_player(&mut p, input, opp, &col);
        assert!(matches!(p.motion, Motion::Run), "hand-off 48 ticks after entry, {:?}", p.motion);
        assert!(wrap_angle(p.yaw - p.heading).abs() < 1e-4, "the body takes the heading at the hand-off");
        assert!(wrap_angle(p.yaw + std::f32::consts::FRAC_PI_2).abs() < 0.05, "facing where it goes (right), yaw {}", p.yaw);
    }

    #[test]
    fn instant_facing_changes_are_not_blended_between_ticks() {
        // The hand-off turns the facing by 90 deg in one tick; the start-of-
        // tick value used for drawing carries the same jump, so the drawn
        // facing never passes through the angles in between.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        for _ in 0..49 {
            p.prev_yaw = p.yaw;
            step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
            assert!(wrap_angle(p.yaw - p.prev_yaw).abs() < 11f32.to_radians(), "drawn facing moved {} deg in one tick", wrap_angle(p.yaw - p.prev_yaw).to_degrees());
        }
        assert!(matches!(p.motion, Motion::Run), "the hand-off happened, {:?}", p.motion);
    }

    #[test]
    fn pivot_lengths_follow_the_clip_table() {
        // Entry tick + 2 * length sim ticks, per side and gait (step 3c).
        let full = |a: f32| pivot_ticks(stick_at(1.0, a));
        assert_eq!(full(90.0), 1 + 2 * 24, "TURN_WALK_R");
        assert_eq!(full(-90.0), 1 + 2 * 33, "TURN_WALK_L");
        assert_eq!(full(150.0), 1 + 2 * 26, "TURN_WALK_BR");
        assert_eq!(full(-150.0), 1 + 2 * 35, "TURN_WALK_BL");
        // A light push (raw 0.7 sideways processes to 0.5: walk from rest).
        // Back pushes use raw 0.8 at 140 deg: at 150 deg the sideways axis
        // (0.4) is still inside the square dead zone and the push reads as
        // dead back.
        let light = |m: f32, a: f32| pivot_ticks(stick_at(m, a));
        assert_eq!(light(0.7, 90.0), 1 + 2 * 35, "TURN_RUN_R");
        assert_eq!(light(0.7, -90.0), 1 + 2 * 49, "TURN_RUN_L");
        assert_eq!(light(0.8, 140.0), 1 + 2 * 28, "TURN_RUN_BR");
        assert_eq!(light(0.8, -140.0), 1 + 2 * 42, "TURN_RUN_BL");
    }

    #[test]
    fn pivot_holds_the_body_at_the_side_offset_from_the_heading() {
        // `0x14025b270`: facing = heading + offset every pivot tick, even
        // while the heading chases a moving stick.
        use std::f32::consts::{FRAC_PI_2, PI};
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        for (angle, dir, offset) in [(70.0, StrafeDir::Right, FRAC_PI_2), (-70.0, StrafeDir::Left, -FRAC_PI_2), (150.0, StrafeDir::BackRight, PI), (-150.0, StrafeDir::BackLeft, PI)] {
            let mut p = Player::new(Vec3::ZERO, 0.0);
            step_player(&mut p, stick_at(1.0, angle), opp, &col);
            assert_eq!(p.motion, Motion::Strafe(dir), "{angle}");
            for t in 0..20 {
                let a = angle + if t < 10 { 0.0 } else { 20.0 };
                step_player(&mut p, stick_at(1.0, a), opp, &col);
                assert!(wrap_angle(p.yaw - p.heading - offset).abs() < 1e-4, "{angle}: body not at heading + offset on tick {t}");
            }
        }
    }

    #[test]
    fn locked_strafe_entry_sectors_are_plain_45_and_135_degrees() {
        let opp = Vec3::new(0.0, 0.0, 100.0);
        let col = floor();
        let sector = |angle: f32| {
            let mut p = Player::new(Vec3::ZERO, 0.0);
            step_player(&mut p, stick_at(1.0, angle), opp, &col);
            p.motion
        };
        assert!(matches!(sector(0.0), Motion::Run));
        assert!(matches!(sector(44.0), Motion::Run), "inside 45 is ordinary movement");
        assert!(matches!(sector(46.0), Motion::Strafe(StrafeDir::Right)));
        assert!(matches!(sector(134.0), Motion::Strafe(StrafeDir::Right)));
        assert!(matches!(sector(136.0), Motion::Strafe(StrafeDir::BackRight)));
        assert!(matches!(sector(-46.0), Motion::Strafe(StrafeDir::Left)));
        assert!(matches!(sector(-136.0), Motion::Strafe(StrafeDir::BackLeft)));
        assert!(matches!(sector(180.0), Motion::Strafe(StrafeDir::BackRight)), "dead back: fixed stand-in for the program's coin flip");
    }

    #[test]
    fn entry_tick_neither_moves_nor_turns() {
        // `0x14025b5e0` only starts the clip and sets the state; ordinary
        // movement (or the pivot) starts on the next tick. Free camera too.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 100.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.locked = false;
        step_player(&mut p, stick_at(1.0, 60.0), opp, &col);
        assert_eq!(p.motion, Motion::Run);
        assert_eq!(p.pos, Vec3::ZERO);
        assert_eq!(p.yaw, 0.0);
        step_player(&mut p, stick_at(1.0, 60.0), opp, &col);
        assert!(p.yaw < 0.0 && p.pos.length() > 0.0, "turning and moving from the next tick");
    }

    #[test]
    fn locked_strafe_cannot_start_mid_run_without_a_zero_stick_tick() {
        // State 1 never runs the entry test (`0x14025ad50`): running at the
        // foe, body aligned, then rolling the stick round the gate to the
        // side is an ordinary run turn, not a pivot. (This overturns the
        // 2026-09-30 "entry allowed while moving" change; see
        // MOVEMENT-AUDIT.md "Locked-on lane".)
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 1000.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        for _ in 0..10 {
            step_player(&mut p, stick_at(1.0, 0.0), opp, &col);
        }
        assert!(matches!(p.motion, Motion::Run));
        for a in [30.0, 60.0, 90.0, 90.0, 90.0] {
            step_player(&mut p, stick_at(1.0, a), opp, &col);
            assert!(matches!(p.motion, Motion::Run), "{a}: {:?}", p.motion);
        }
    }

    #[test]
    fn a_flick_through_the_dead_zone_mid_run_starts_a_pivot() {
        // One tick with both axes inside the square dead zone is idle (state
        // 0); the next non-zero tick is an entry tick. Running at the foe the
        // body is aligned with the camera, so a sideways push pivots.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 1000.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        for _ in 0..10 {
            step_player(&mut p, stick_at(1.0, 0.0), opp, &col);
        }
        // Passing the centre: (0.3, 0.3) raw is inside the dead zone on both axes.
        step_player(&mut p, Input::from_axes(0.3, 0.3, false, false), opp, &col);
        assert_eq!(p.motion, Motion::Idle);
        step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
        assert!(matches!(p.motion, Motion::Strafe(StrafeDir::Right)), "{:?}", p.motion);
    }

    #[test]
    fn after_a_pivot_the_body_must_turn_back_before_the_next_one() {
        // Running sideways after a pivot, the body faces 90 deg off the
        // camera. Released, the idle turn swings it back at 600 deg/s; a
        // push before it is within ~10 deg is ordinary movement, after it
        // is a fresh pivot.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        let after = |release: usize| {
            let mut p = Player::new(Vec3::ZERO, 0.0);
            for _ in 0..60 {
                step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
            }
            assert!(matches!(p.motion, Motion::Run));
            for _ in 0..release {
                step_player(&mut p, Input::default(), opp, &col);
            }
            step_player(&mut p, stick_at(1.0, -90.0), opp, &col);
            p.motion
        };
        assert!(matches!(after(2), Motion::Run), "body still ~70 deg off: ordinary movement");
        assert!(matches!(after(9), Motion::Strafe(StrafeDir::Left)), "body back on the camera: pivot");
    }

    #[test]
    fn locked_pivot_keeps_its_side_while_the_stick_wobbles_across_dead_back() {
        // The spasm case: stick held straight back wobbling left/right of
        // dead centre. The side is chosen once on entry and held.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 100.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        step_player(&mut p, stick_at(1.0, 175.0), opp, &col);
        let Motion::Strafe(first) = p.motion else { panic!("pull-back from rest pivots, {:?}", p.motion) };
        for i in 0..40 {
            let a = if i % 6 < 3 { 175.0 } else { -175.0 };
            step_player(&mut p, stick_at(1.0, a), opp, &col);
            if let Motion::Strafe(d) = p.motion {
                assert_eq!(d, first, "side flipped mid-pivot on tick {i}");
            }
        }
    }

    #[test]
    fn locked_strafe_does_not_restart_after_the_pivot_while_the_stick_is_held() {
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 100.0);
        for angle in [90.0, -90.0, 150.0, 180.0] {
            let mut p = Player::new(Vec3::ZERO, 0.0);
            let mut pivots = 0;
            let mut was = false;
            for _ in 0..300 {
                step_player(&mut p, stick_at(1.0, angle), opp, &col);
                let now = matches!(p.motion, Motion::Strafe(_));
                if now && !was {
                    pivots += 1;
                }
                was = now;
            }
            assert_eq!(pivots, 1, "one pivot for one held push at {angle}");
        }
    }

    #[test]
    fn locked_strafe_needs_the_body_aligned_with_the_camera() {
        // The test compares body facing with the CAMERA yaw (`+0x12e5c`),
        // not the opponent's bearing.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 100.0);
        let mut p = Player::new(Vec3::ZERO, 30.0f32.to_radians());
        p.cam_yaw = 0.0;
        step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
        assert!(!matches!(p.motion, Motion::Strafe(_)), "30 deg off the camera: ordinary movement");
        let mut q = Player::new(Vec3::ZERO, 8.0f32.to_radians());
        q.cam_yaw = 0.0;
        step_player(&mut q, stick_at(1.0, 90.0), opp, &col);
        assert!(matches!(q.motion, Motion::Strafe(_)), "8 deg off still enters");
    }

    #[test]
    fn locked_pivot_heading_chases_the_stick_at_the_turn_rate() {
        // Reverse the stick mid-pivot: the movement direction swings at most
        // 10 deg per sim tick (600 deg/s), it never snaps.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 100.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
        for _ in 0..4 {
            step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
        }
        let mut last = p.vel;
        for _ in 0..12 {
            step_player(&mut p, stick_at(1.0, -90.0), opp, &col);
            let d = p.vel.normalize_or_zero().dot(last.normalize_or_zero()).clamp(-1.0, 1.0).acos().to_degrees();
            assert!(d < 12.0, "velocity direction changed {d} deg in one tick");
            last = p.vel;
        }
    }

    #[test]
    fn releasing_mid_pivot_moves_once_more_then_snaps_the_body_to_the_heading() {
        // `0x14025b270` moves along the heading before its zero-stick test,
        // then copies the heading into the facing and goes idle.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        for _ in 0..6 {
            step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
        }
        assert!(matches!(p.motion, Motion::Strafe(_)));
        let x = p.pos.x;
        step_player(&mut p, Input::default(), opp, &col);
        assert_eq!(p.motion, Motion::Idle, "release ends the pivot");
        assert!(p.pos.x < x - 0.1, "still moved along the heading on the release tick");
        assert!(wrap_angle(p.yaw - p.heading).abs() < 1e-4, "body snapped to the heading");
    }

    #[test]
    fn a_gait_change_mid_pivot_swaps_clips_at_the_same_relative_progress() {
        // Light first push: walk pair (TURN_RUN_R, 35 frames). Then a full
        // push: run pair (TURN_WALK_R, 24 frames), time scaled by 24/35 and
        // the pivot moves at run speed -- the fix for "a light-then-full push
        // plays the walk pivot at run pace".
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        step_player(&mut p, stick_at(0.7, 90.0), opp, &col);
        assert_eq!(p.gait, Gait::Walk);
        for _ in 0..17 {
            step_player(&mut p, stick_at(0.7, 90.0), opp, &col);
        }
        let before = p.pivot().unwrap();
        assert_eq!((before.gait, before.time), (Gait::Walk, 9.0));
        step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
        let pv = p.pivot().unwrap();
        assert_eq!(pv.gait, Gait::Run);
        assert!((pv.time - (9.0 * 24.0 / 35.0 + 0.5)).abs() < 1e-4, "time {}", pv.time);
        let x = p.pos.x;
        step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
        assert!(((x - p.pos.x) - RUN_SPEED * DT).abs() < 0.05, "moving at run speed");
    }

    #[test]
    fn pivot_clock_follows_the_speed_status() {
        // The clip's playback scale is the speed-status multiplier
        // (`param_1[0x32] = param_1[9]`): twice the speed, half the ticks.
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 10_000.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        let input = stick_at(1.0, 90.0);
        let mut n = 0;
        loop {
            step_player_at(&mut p, input, opp, &col, 2.0);
            n += 1;
            if n > 1 && !matches!(p.motion, Motion::Strafe(_)) {
                break;
            }
        }
        assert_eq!(n, 1 + 24, "entry + 24 frames at double speed");
    }

    #[test]
    fn ordinary_turn_slowdown_uses_the_angle_left_after_the_turn() {
        // `0x14024f95f`: factor = max(0, 1 - |error after this tick's turn| / 90).
        let col = floor();
        let opp = Vec3::new(0.0, 0.0, 100.0);
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.locked = false;
        step_player(&mut p, stick_at(1.0, 0.0), opp, &col);
        step_player(&mut p, stick_at(1.0, 0.0), opp, &col);
        let z = p.pos;
        step_player(&mut p, stick_at(1.0, 90.0), opp, &col);
        let moved = (p.pos - z).length();
        assert!((moved - RUN_SPEED * DT / 9.0).abs() < 1e-3, "80 deg left after a 10 deg step: 1/9 speed, moved {moved}");
    }

    #[test]
    fn locked_jittery_light_push_near_45_degrees_never_flip_flops() {
        // A light push wobbling 43/47 deg (the reported "loops between walk
        // and a quick-step" bug) cannot flip: entry classifies once, and
        // ordinary movement never re-runs the entry test. No band needed.
        let col = floor();
        let mut p = Player::new(Vec3::ZERO, 0.0);
        let opp = Vec3::new(0.0, 0.0, 100.0);
        let a = stick_at(0.5, 43.0);
        let b = stick_at(0.5, 47.0);
        let mut flips = 0;
        let mut last_was_strafe = None;
        for t in 0..40 {
            step_player(&mut p, if t % 2 == 0 { a } else { b }, opp, &col);
            let is_strafe = matches!(p.motion, Motion::Strafe(_));
            if last_was_strafe.is_some_and(|prev| prev != is_strafe) {
                flips += 1;
            }
            last_was_strafe = Some(is_strafe);
        }
        assert!(flips <= 1, "classification flip-flopped {flips} times");
    }

    #[test]
    fn locked_steady_push_never_flips_back_to_a_pivot() {
        // Holding the stick rock-steady, the motion sequence is monotone:
        // at most one pivot, then ordinary movement, never back again.
        let col = floor();
        for angle in [0.0, 20.0, 44.0, 46.0, 70.0, 120.0, 150.0] {
            let mut p = Player::new(Vec3::ZERO, 0.0);
            let opp = Vec3::new(0.0, 0.0, 100.0);
            let input = stick_at(0.6, angle);
            let mut left_pivot = false;
            for _ in 0..200 {
                step_player(&mut p, input, opp, &col);
                match p.motion {
                    Motion::Strafe(_) => assert!(!left_pivot, "pivot restarted at a steady {angle} deg push"),
                    _ => left_pivot |= true,
                }
            }
        }
    }
}
