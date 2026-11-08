//! The program's collider point through the current clip (round 8 of the gameplay-accuracy work):
//! every hit test, the melee dash, the aim and the homing read the fighter's collider point
//! `+0x60..+0x68`, which `FUN_1402513a0` copies from the BODY1 bone's world matrix after the tick's
//! skeleton update (`FUN_140298020`), i.e. BODY1 in the pose of the clip and frame the fighter is
//! at. `Combat::update_pose` (end of `Match::step`) keeps a clip clock per fighter from sim state
//! only and writes `Player::pose_point`, which `Player::body_point` / `target_point` read.
//! docs/PROGRAM-RE.md "BODY1 per clip".

use super::*;
use crate::sim::Motion;

/// One clip's BODY1 path (`cache/chars/<id>.body1.json`, `pd_convert shotpoints`), one sample per
/// 30 fps clip frame, in the fighter's own frame (x right, y up, z forward at yaw 0).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Body1Clip {
    /// The root joint held at its bind translation (the clip's root drift left out).
    pub bind: Vec<Vec3>,
    /// The root joint's animated translation kept.
    pub anim: Vec<Vec3>,
}

impl Body1Clip {
    /// BODY1 at clip frame `frame` (fractional: interpolated; held at the last frame past the end).
    pub fn at(&self, animated_root: bool, frame: f32) -> Option<Vec3> {
        let v = if animated_root { &self.anim } else { &self.bind };
        let last = v.len().checked_sub(1)?;
        let f = frame.clamp(0.0, last as f32);
        let (a, t) = (f.floor() as usize, f - f.floor());
        Some(v[a].lerp(v[(a + 1).min(last)], t))
    }
}

/// Which root column a clip's BODY1 is read from.
///
/// READ (round 10): the skeleton's root is pinned to the fighter's position in x / z. `FUN_14025d380` gives
/// the first joint with flag bit 1 (every playable model: the BODY1 chain's root, bind (0, 0, 0)) flag 0x100
/// and callback case 2 (joint `+0x3c` = 2), and installs `LAB_14025dc20` as the skeleton's joint callback;
/// `FUN_140298020` calls it for a 0x100 joint after building its world matrix, and case 2 (`14025df50`)
/// overwrites that matrix's x / z translation (`+0x30` / `+0x38`) with slot 0's, the fighter matrix's
/// (position). The root tracks key only x / z (code 2, every clip of pc00a / pc01a), so the clip's root drift
/// never reaches BODY1: the `bind` column is the program's point. MEASURED on the original: the lock-on
/// facing of 273 takes points at P1's WAIT1 `bind` point (pc01a (-2.8 fwd, -2.05 right) vs the fit
/// (-2.75, -2.0), 0.2 u RMS) and, in the 48 program ticks of WAIT0 after a reaction, at its WAIT0 `bind`
/// point (about the position); the `anim` column misses both by 2 u.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Body1Root {
    /// The animated root, except while the clip's root track is also moving the body (`Combat::root_step`,
    /// the program's root-motion flag `FUN_140257510`): then the bind root, so the drift is not counted twice.
    /// Round 8's rule (`PD_BODY1_ROOT=auto`: standing / walking / running back on the measured `Body::lock`).
    Auto,
    /// The program's rule everywhere (`14025df50`, above), the default since round 11: every clip reads `bind`, and
    /// standing / walking / running run the idle rows and gait loops. Round 10 kept it off because Swift Kick 182 /
    /// Leg Throw 183 lost their match with it; round 11 found why (WAVES "r11aim lane"): the knocked victim plays
    /// BLOW1_BACK (rows 61, back set), whose pinned BODY1 is at the feet (+0.04 forward), so in the take the dash
    /// measures 11.19 + 0.04 > 11 and steps; ours measured 10.95 because our push-apart came after the blow and the
    /// victim's flight instead of in the caster's own physics before its blow (`Combat::lunge`), and the victim flew
    /// only half its first program tick. With both fixed ours measures 11.23.
    #[default]
    Bind,
    /// Round 10's default (`PD_BODY1_ROOT=standing`): standing, walking and running as `Bind`; cast / reaction /
    /// landing clips on round 8's `Auto` column.
    Standing,
    /// Always the animated root (the literal read of `FUN_140298020` without the callback).
    Anim,
}

impl Body1Root {
    /// `bind` / `anim` / `auto` / `standing` (anything else: the default, `Bind`).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Body1Root::Auto,
            "standing" => Body1Root::Standing,
            "anim" => Body1Root::Anim,
            _ => Body1Root::Bind,
        }
    }
}

/// A fighter's clip clock for the collider point: the clip, the sequence number that restarts it, the
/// frame (30 fps, half a frame a sim tick) and whether its root is read animated.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct PoseClock {
    clip: &'static str,
    seq: u16,
    frame: f32,
    animated_root: bool,
}

/// A standing fighter's idle clip (the program's state 0, READ round 10): the tail of `FUN_14025b5e0`
/// picks and queues the idle rows of the clip table `0x1403faed0` (row 0 WAIT0, 1 WAIT1, 2 WAIT1IN, 3 WAIT1OUT,
/// 4 WAIT_ACT0, 5 / 6 the mood idles WAIT_CON / WAIT_DAM), and the clip advance `FUN_140257510` switches to a
/// queued clip when the playing one ends with its loop flag off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct IdleClock {
    clip: &'static str,
    /// 30 fps clip frame (half a frame a sim tick).
    frame: f32,
    looping: bool,
    /// The queued next clip (`+0x37`) and its loop flag (`+0x39`).
    queued: Option<(&'static str, bool)>,
    /// The state timer `+8` in program ticks (half a tick a sim tick).
    timer: f32,
}

/// The idle rows that take part in the WAIT1 sequence (`FUN_14025b5e0`: `row - 1 < 2`, `row == 3`, rows <= 6).
const IDLE_ROWS: [&str; 7] = ["WAIT0", "WAIT1", "WAIT1IN", "WAIT1OUT", "WAIT_ACT0", "WAIT_CON", "WAIT_DAM"];

impl IdleClock {
    fn start(idle: &'static str) -> Self {
        IdleClock { clip: idle, frame: 0.0, looping: true, queued: None, timer: 0.0 }
    }

    /// Half a program tick of state 0 for a fighter whose mood idle is `idle` (WAIT0 / WAIT_CON / WAIT_DAM),
    /// `locked` = the lock-on flag `+0x13f24`; `len(clip)` = the clip's length in frames.
    fn step(&mut self, idle: &'static str, locked: bool, len: impl Fn(&str) -> f32) {
        let row = IDLE_ROWS.iter().position(|r| *r == self.clip);
        // The mood check before the lock branch: a mood idle that no longer fits restarts the mood's own.
        if let Some(r) = row {
            let normal = idle == "WAIT0";
            if (normal && (r == 5 || r == 6)) || (!normal && self.clip != idle) {
                *self = IdleClock { timer: self.timer, ..IdleClock::start(idle) };
            }
        }
        self.timer += 0.5;
        let row = IDLE_ROWS.iter().position(|r| *r == self.clip);
        if locked {
            // Locked (or no pad this tick): timer 0; WAIT1 / WAIT1IN -> WAIT1OUT at once (no blend), then the idle.
            self.timer = 0.0;
            match row {
                Some(1 | 2) => *self = IdleClock { clip: "WAIT1OUT", frame: 0.0, looping: false, queued: Some((idle, true)), timer: 0.0 },
                Some(3) => self.queued = Some((idle, true)),
                Some(_) if self.clip == idle => {}
                Some(_) => *self = IdleClock::start(idle),
                None => {}
            }
        } else {
            match row {
                Some(2 | 4) => self.queued = Some(("WAIT1", true)),
                Some(3) => self.queued = Some((idle, true)),
                _ => {}
            }
            if self.timer >= 31.0 {
                match row {
                    Some(0) => {
                        self.queued = Some(("WAIT1IN", false));
                        self.looping = false;
                    }
                    Some(1) => {
                        self.queued = Some(("WAIT_ACT0", false));
                        self.looping = false;
                    }
                    _ => {}
                }
                self.timer = -240.0;
            }
        }
        // The clip advance: at the end a looping clip wraps, else the queued clip (or the idle) starts at 0.
        self.frame += 0.5;
        let l = len(self.clip).max(1.0);
        if self.frame >= l {
            if self.looping {
                self.frame -= l;
            } else {
                let (clip, looping) = self.queued.take().unwrap_or((idle, true));
                self.clip = intern_idle(clip);
                self.frame = 0.0;
                self.looping = looping;
            }
        }
    }
}

fn intern_idle(clip: &str) -> &'static str {
    IDLE_ROWS.iter().copied().find(|r| *r == clip).unwrap_or("WAIT0")
}

impl Combat {
    /// Fighter `i` stands in the settled idle (WAIT1 looping, just entered: timer -240), as a fighter that has
    /// stood unlocked for a while does (film takes, `PD_FILM_SETTLED`).
    pub fn settle_idle(&mut self, i: usize) {
        if let Some(c) = self.idle.get_mut(i) {
            *c = Some(IdleClock { clip: "WAIT1", frame: 0.0, looping: true, queued: None, timer: -240.0 });
        }
    }

    /// Each player's BODY1 paths by slot (`cache/chars/<id>.body1.json`) and the root column rule.
    /// A slot with an empty table keeps the measured idle point (`Player::target_point`).
    pub fn set_body1_paths(&mut self, tables: impl IntoIterator<Item = HashMap<String, Body1Clip>>, root: Body1Root) {
        for (slot, t) in self.body1.iter_mut().zip(tables) {
            *slot = t;
        }
        self.body1_root = root;
    }

    /// The clip fighter `i`'s pose is in for the collider point, and the number that restarts it, or
    /// `None` for the states that keep the measured idle point: a guard's cast (`DEF_*` clips), standing,
    /// walking, running (these three under `Body1Root::Standing` / `Bind` go through `update_pose`'s idle machine and gait
    /// loops instead), pivoting (the idle offset is MEASURED from the original's lock-on, `body.rs`), jumping (the renderer
    /// stretches JUMP_UP / JUMP_DOWN to the airtime; no sim clock for that) and dodging (clip pairing
    /// unconfirmed). Mirrors `main.rs::clip_for` for casts, reactions and landings.
    fn pose_clip(&self, i: usize, p: &Player) -> Option<(&'static str, u16)> {
        let f = &self.fighters[i];
        match p.motion {
            Motion::Cast | Motion::Lunge => {
                if p.jump_windup() > 0 {
                    return None;
                }
                let row = self.cast_row(i, f.cast_skill)?;
                // A guard's DEF_* clip keeps the measured idle point: with BODY1 of the clip (either root column)
                // the guards of the film set stopped blocking the opponent's Blaster on the original's tick
                // (round 8 film: Power Shell 29, Laser Shell 94, Fortress of Iron 155, Iron Skin 207, Protect
                // Circle 271, all 0 -> 2-4). Why the program's guard pose differs from our FK is UNCONFIRMED.
                if row.anim_name.is_empty() || row.fires_at_once() || row.anim_name.starts_with("DEF_") {
                    return None;
                }
                Some((intern_clip(&row.anim_name), f.cast_seq))
            }
            Motion::Hit | Motion::Blown => Some((self.reaction_clip(i), f.hit_seq)),
            Motion::Floored => Some((self.lying_clip(i), f.hit_seq)),
            Motion::GetUp => Some((self.getup_clip(i), f.hit_seq)),
            Motion::Down => Some(("BLOW0_FRONT", f.hit_seq)),
            Motion::Land => Some(("JUMP_LAND1", 0)),
            Motion::HeavyLand => Some(("JUMP_LAND0", 0)),
            _ => None,
        }
    }

    /// Fighter `i` is in the melee state (12) on the ground: a short-range cast's wind-up, dash or blow
    /// (`Match::step` hands it to the body physics: the program's ledge rule for that state, `sim.rs`).
    pub(crate) fn melee_on_ground(&self, i: usize) -> bool {
        let f = &self.fighters[i];
        f.melee && !f.air_cast && matches!(f.action, Action::Lunge | Action::Cast)
    }

    /// One sim tick of fighter `i`'s clip clock (end of `Match::step`, after the tick's states, root
    /// motion and physics: the program writes the collider point after the clip advance and the
    /// skeleton, `FUN_14024ec80`), and the collider point the next tests read (`Player::pose_point`).
    /// A new clip (or the same clip restarted: a new cast / hit) starts at frame 0; then half a frame a
    /// sim tick (30 fps clips, 60 Hz sim), held at the last frame; a bind / lift reaction wraps its hold
    /// loop like the renderer (`reaction_hold`, `0x140257510`). READ (round 11): a reaction clip starts with no
    /// blend (`FUN_140258690` passes blend -1 to `FUN_140257310`, so `FUN_140297d80` drops the other layers and
    /// gives the new one weight 1), and the start flag `+0xf0` that `FUN_140257310` sets stops `FUN_140257510`'s
    /// time step until the pre-tick (fighter vtable +0x18 = `FUN_14024f6a0`, called for every fighter before any
    /// tick in `FUN_14012bbb0`) clears it: a victim knocked in the hit tick shows frame 0 at the end of that tick.
    pub fn update_pose(&mut self, i: usize, p: &mut Player) {
        // Standing: the program's idle machine (`IdleClock`); walking / running: the mood's gait loop (table
        // `0x1403faed0` columns 1 / 2, as `main.rs::clip_for`). Both read the `bind` column (root pinned).
        if matches!(self.body1_root, Body1Root::Bind | Body1Root::Standing) && !self.body1[i].is_empty() && p.jump_windup() == 0 {
            let mood = crate::anim_args::mood(self.stat(i, crate::moves::effects::Stat::Speed), self.inverted(i), self.fighters[i].health);
            let gait = match p.motion {
                Motion::Walk => Some(crate::sim::Gait::Walk),
                Motion::Run => Some(crate::sim::Gait::Run),
                _ => None,
            };
            if p.motion == Motion::Idle {
                let idle = crate::anim_args::mood_clip(mood, None);
                let table = &self.body1[i];
                let len = |c: &str| table.get(c).map_or(1.0, |t| t.bind.len().saturating_sub(1) as f32);
                let mut clock = self.idle[i].unwrap_or_else(|| IdleClock::start(idle));
                if self.idle[i].is_some() {
                    clock.step(idle, p.locked, len);
                }
                self.idle[i] = Some(clock);
                self.pose[i] = None;
                if let Some(path) = table.get(clock.clip) {
                    p.pose_point = path.at(false, clock.frame);
                    return;
                }
            } else if let Some(g) = gait {
                self.idle[i] = None;
                let clip = crate::anim_args::mood_clip(mood, Some(g));
                if let Some(path) = self.body1[i].get(clip) {
                    let l = path.bind.len().saturating_sub(1).max(1) as f32;
                    let frame = match self.pose[i] {
                        Some(c) if c.clip == clip => (c.frame + 0.5) % l,
                        _ => 0.0,
                    };
                    self.pose[i] = Some(PoseClock { clip, seq: 0, frame, animated_root: false });
                    p.pose_point = path.at(false, frame);
                    return;
                }
            }
        }
        if p.motion != Motion::Idle {
            self.idle[i] = None;
        }
        let Some((clip, seq)) = self.pose_clip(i, p) else {
            self.pose[i] = None;
            p.pose_point = None;
            return;
        };
        let Some(path) = self.body1[i].get(clip) else {
            self.pose[i] = None;
            p.pose_point = None;
            return;
        };
        let clock = match self.pose[i] {
            Some(c) if c.clip == clip && c.seq == seq => {
                let mut frame = c.frame + 0.5;
                if let Some((h0, h1)) = self.reaction_hold(i) {
                    if h1 > h0 && frame >= h1 {
                        frame = h0 + (frame - h1).rem_euclid(h1 - h0);
                    }
                }
                PoseClock { frame, ..c }
            }
            _ => {
                let root_moves = self.fighters[i].root.is_some_and(|(c, _)| c == clip);
                let animated_root = match self.body1_root {
                    Body1Root::Anim => true,
                    Body1Root::Bind => false,
                    Body1Root::Auto | Body1Root::Standing => !root_moves,
                };
                PoseClock { clip, seq, frame: 0.0, animated_root }
            }
        };
        self.pose[i] = Some(clock);
        p.pose_point = path.at(clock.animated_root, clock.frame);
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;

    /// A path whose frame k sits at bind (0, 10 + k, 0) and anim (k, 10 + k, -2k).
    fn path(frames: usize) -> Body1Clip {
        Body1Clip {
            bind: (0..=frames).map(|k| Vec3::new(0.0, 10.0 + k as f32, 0.0)).collect(),
            anim: (0..=frames).map(|k| Vec3::new(k as f32, 10.0 + k as f32, -2.0 * k as f32)).collect(),
        }
    }

    fn table(clips: &[&str]) -> HashMap<String, Body1Clip> {
        clips.iter().map(|c| (c.to_string(), path(10))).collect()
    }

    /// READ `FUN_1402513a0`: the collider point is BODY1 of the pose the skeleton is at, reactions included.
    /// A hit body plays its reaction clip from frame 0, half a frame a sim tick (30 fps clips, 60 Hz sim),
    /// held at the last frame; a clip with no root motion reads the animated root (`Body1Root::Auto`).
    #[test]
    fn a_reacting_body_point_follows_its_clip_and_frame() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.set_body1_paths([table(&["DMG_SHOT_FRONT"]), table(&["DMG_SHOT_FRONT"])], Body1Root::Auto);
        p[1].motion = Motion::Hit;
        c.update_pose(1, &mut p[1]);
        // Player 1 faces -Z (yaw pi): local (x, y, z) -> world (-x, y, -z) from its feet.
        assert!((p[1].body_point() - (p[1].pos + Vec3::new(0.0, 10.0, 0.0))).length() < 1e-4, "{:?}", p[1].body_point());
        for _ in 0..4 {
            c.update_pose(1, &mut p[1]);
        }
        let want = p[1].pos + Vec3::new(-2.0, 12.0, 4.0);
        assert!((p[1].body_point() - want).length() < 1e-4, "frame 2: {:?}", p[1].body_point());
        assert!((p[1].target_point() - Vec3::new(want.x, p[1].pos.y, want.z)).length() < 1e-4);
        for _ in 0..40 {
            c.update_pose(1, &mut p[1]);
        }
        assert!((p[1].body_point() - (p[1].pos + Vec3::new(-10.0, 20.0, 20.0))).length() < 1e-4, "held at the last frame");
        // A new hit restarts the clip; a clip whose root track moves the body reads the bind root.
        c.fighters[1].hit_seq = c.fighters[1].hit_seq.wrapping_add(1);
        c.fighters[1].root = Some(("DMG_SHOT_FRONT", 0.0));
        c.update_pose(1, &mut p[1]);
        c.update_pose(1, &mut p[1]);
        assert!((p[1].body_point() - (p[1].pos + Vec3::new(0.0, 10.5, 0.0))).length() < 1e-4, "{:?}", p[1].body_point());
    }

    /// READ round 10 (`FUN_14025b5e0` state 0 + `FUN_140257510`): an unlocked standing fighter plays WAIT0; at 31
    /// program ticks WAIT1IN is queued and starts when WAIT0's cycle ends, then WAIT1 loops. Locking on sends WAIT1
    /// straight to WAIT1OUT and then WAIT0. The point is the `bind` column (the root pinned, `14025df50`).
    #[test]
    fn a_standing_fighter_walks_the_programs_idle_rows() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        let clip = |x: f32, frames: usize| Body1Clip { bind: vec![Vec3::new(x, 10.0, 0.0); frames + 1], anim: vec![Vec3::new(x + 5.0, 10.0, 7.0); frames + 1] };
        let t: HashMap<String, Body1Clip> =
            [("WAIT0", clip(0.0, 40)), ("WAIT1IN", clip(1.0, 30)), ("WAIT1", clip(2.0, 50)), ("WAIT1OUT", clip(3.0, 30))].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        c.set_body1_paths([t.clone(), t], Body1Root::Bind);
        p[0].locked = false;
        p[0].motion = Motion::Idle;
        // Yaw 0: local x = world x.
        let x = |p: &Player| p.target_point().x - p.pos.x;
        c.update_pose(0, &mut p[0]);
        assert!(x(&p[0]).abs() < 1e-4, "WAIT0, bind column (not anim's +5)");
        let mut seen = vec![];
        for tick in 1..400 {
            c.update_pose(0, &mut p[0]);
            let v = x(&p[0]).round() as i32;
            if seen.last().map(|l: &(i32, i32)| l.0) != Some(v) {
                seen.push((v, tick));
            }
        }
        // WAIT0 to its end (40 frames = 80 sim ticks), WAIT1IN 30 frames, then WAIT1 looping.
        assert_eq!(seen, vec![(0, 1), (1, 80), (2, 140)], "{seen:?}");
        p[0].locked = true;
        c.update_pose(0, &mut p[0]);
        assert!((x(&p[0]) - 3.0).abs() < 1e-4, "locked: WAIT1OUT at once");
        for _ in 0..60 {
            c.update_pose(0, &mut p[0]);
        }
        assert!(x(&p[0]).abs() < 1e-4, "then WAIT0");
    }

    /// Standing (and walking, running, pivoting, jumping, dodging) keeps the MEASURED idle point; so does
    /// a reaction whose clip is not in the table, and a fighter with no table at all (the old values).
    #[test]
    fn without_a_pose_the_measured_points_stay() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        p[1].body = crate::body::Body::of("pc01a").unwrap();
        let idle = p[1].body_point();
        c.set_body1_paths([table(&["WAIT0"]), table(&["WAIT0"])], Body1Root::Auto);
        c.update_pose(1, &mut p[1]);
        assert_eq!(p[1].pose_point, None);
        assert_eq!(p[1].body_point(), idle);
        p[1].motion = Motion::Hit;
        c.update_pose(1, &mut p[1]);
        assert_eq!(p[1].pose_point, None, "DMG_SHOT_FRONT is not in this table");
        c.set_body1_paths([HashMap::new(), HashMap::new()], Body1Root::Auto);
        c.update_pose(1, &mut p[1]);
        assert_eq!(p[1].pose_point, None);
    }

    /// A cast's pose is its action row's clip from the cast start (`FUN_14025a3e0`), restarted by the next
    /// cast; the rule switch picks the root column.
    #[test]
    fn a_cast_poses_its_rows_clip() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.set_body1_paths([table(&["ATK_GROUND"]), table(&["ATK_GROUND"])], Body1Root::Anim);
        let row = ActionRow { anim_name: "ATK_GROUND".into(), fire_tick: 20, ..Default::default() };
        c.set_action_row(500, Some(row), None);
        c.fighters[0].cast_skill = 500;
        p[0].motion = Motion::Cast;
        c.fighters[0].root = Some(("ATK_GROUND", 0.0));
        for _ in 0..3 {
            c.update_pose(0, &mut p[0]);
        }
        // Yaw 0: local = world offset. Frame 1, animated root (the switch overrides the root motion).
        assert!((p[0].body_point() - Vec3::new(1.0, 11.0, -2.0)).length() < 1e-4, "{:?}", p[0].body_point());
        c.fighters[0].cast_seq = c.fighters[0].cast_seq.wrapping_add(1);
        c.update_pose(0, &mut p[0]);
        assert!((p[0].body_point() - Vec3::new(0.0, 10.0, 0.0)).length() < 1e-4, "a new cast starts at frame 0");
    }

    /// A guard's cast (`DEF_*` clip) keeps the measured idle point (round 8 film: guards stopped blocking
    /// on the original's tick with BODY1 of the clip).
    #[test]
    fn a_guard_cast_keeps_the_idle_point() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.set_body1_paths([table(&["DEF_SHELTER"]), table(&["DEF_SHELTER"])], Body1Root::Auto);
        c.set_action_row(500, Some(ActionRow { anim_name: "DEF_SHELTER".into(), fire_tick: 2, ..Default::default() }), None);
        c.fighters[0].cast_skill = 500;
        p[0].motion = Motion::Cast;
        c.update_pose(0, &mut p[0]);
        assert_eq!(p[0].pose_point, None);
    }
}
