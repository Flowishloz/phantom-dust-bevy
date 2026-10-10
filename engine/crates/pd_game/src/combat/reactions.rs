//! Hit-reaction kinds 7 / 8 / 9 (pinned, bind, lift), the paralysed struggle, the s60
//! reaction of status skills, and clip root motion (reactions2 lane). Every rule cites the
//! program address it was read from (`PDUWP.exe`, image base `0x140000000`; the clip-start
//! arguments Ghidra drops were re-read with capstone).
//!
//! A reaction row is `0x72` bytes; `w[k]` below is the word at row `+0x52 + 2k` (the
//! docs' w41 + k). `FUN_140258690(controller, row, queued row, bearing)` is the start; its
//! `switch (row[+0x4c])` is the kind.

use super::*;
use crate::moves::SIM_TICKS_PER_TABLE_TICK as TICK;

/// Reaction row 42: the sound / effect the paralysed struggle plays (`FUN_140258fb0(0x2a)`
/// at `0x14025b5e0`'s status-4 branch and `0x14025ad50`). Its clip (`w41`) is WAIT0: unused.
pub const STRUGGLE_ROW: i16 = 42;

/// The clip ids whose body counts as lying face-up for a pinned hit: `0x1c1ce00000000000 >> id`
/// at `0x140258bf2` -- BLOW0-2_FRONT 45-47, BLOW_LAND_LF / MF / SF 50-52, WALLHIT_BACK / _LOOP
/// 58 / 59 and DOWN_FRONT 60: the same mask as the knock-down's front set (`program_front_set`,
/// `0x140259990`).
pub fn pinned_face_up(clip_id: usize) -> bool {
    clip_id < 0x3d && (0x1c1c_e000_0000_0000u64 >> clip_id) & 1 != 0
}

/// The clip frames the program's per-tick clock leaves a hold-loop clip at after `frames`
/// ticks: it plays on to the loop start `h0`, and from the loop end `h1` on wraps back by
/// the loop's length (`0x140257510`, `t = h0 + t - h1`).
pub fn hold_clock(frames: i32, h0: i32, h1: i32) -> i32 {
    if frames < h1 {
        frames
    } else if h1 <= h0 {
        h0
    } else {
        h0 + (frames - h1) % (h1 - h0)
    }
}

impl Combat {
    /// Each player's root-motion curves by slot: clip name -> cumulative `[side, forward]` per
    /// 30 Hz frame (`cache/chars/<id>.rootmotion.json`, `pd_convert character`). Replaces what
    /// was set before; a slot without a table has no root motion.
    pub fn set_root_curves(&mut self, tables: impl IntoIterator<Item = HashMap<String, Vec<[f32; 2]>>>) {
        for (slot, t) in self.root_curves.iter_mut().zip(tables) {
            *slot = t;
        }
    }

    /// The loaded row of `victim`'s current reaction.
    fn current_row(&self, victim: usize) -> Option<&ReactionRow> {
        self.reaction_rows.get(&self.fighters[victim].reaction_row)
    }

    /// Kind 8, bind (`0x140258b87`): state 16 for `w43 + 1 + w42` program ticks (`FUN_140259d10`
    /// is passed `[rsi+0x56] + 1 + [rsi+0x54]`); the clip `w41` plays to frame `w43` and then
    /// loops frames `w43..w44` (the clip start's hold-loop arguments, `[rsp+0x28]` / `[rsp+0x30]`).
    /// When the timer runs out state 16 hands over to state 14 for the rest of the clip
    /// (`0x140256e80` case 0x10: `(int)(clip length - clip time)`), and then the fighter is idle.
    /// Sim ticks. The clip time at the hand-over is `w43 + w42 + 2` frames wrapped by the loop;
    /// +-1 frame of it is UNCONFIRMED (where in the tick the clip clock runs).
    pub(super) fn bind_ticks(&self, victim: usize, r: &ReactionRow) -> u16 {
        let (hold, h0, h1) = (r.w[1].max(0) as i32, r.w[2].max(0) as i32, r.w[3].max(0) as i32);
        let timer = h0 + 1 + hold;
        let clip = Self::anim_name(r.w[0].max(0) as usize);
        let len = (self.clip_hold_ticks(victim, clip) / TICK) as i32;
        let at = hold_clock(timer + 1, h0, h1);
        let rest = (len - at).max(0);
        ((timer + 1 + rest) as u16).saturating_mul(TICK)
    }

    /// Kind 9, lift (`0x140258c95`): state 14 for `w42 + w43` program ticks, with the clip's
    /// hold loop `w43..w44` and the float status (status 11) for as long. Sim ticks.
    pub(super) fn lift_ticks(r: &ReactionRow) -> u16 {
        ((r.w[1].max(0) + r.w[2].max(0) + 1) as u16).saturating_mul(TICK)
    }

    /// The hold loop `(start, end)` in clip frames of the bind / lift clip now playing, for the
    /// renderer (`0x140257510`'s hold branch); `None` for any other reaction.
    pub fn reaction_hold(&self, i: usize) -> Option<(f32, f32)> {
        let f = &self.fighters[i];
        if f.action != Action::Hit || f.struggle {
            return None;
        }
        let r = self.reaction_rows.get(&f.reaction_row)?;
        matches!(r.kind, reaction_kind::BIND_A | reaction_kind::BIND_B).then_some((r.w[2] as f32, r.w[3] as f32))
    }

    /// A reaction that started as a stagger (`Action::Hit`) starts its extras: the lift's launch
    /// and float status, and the clip's root motion.
    pub(super) fn row_started(&mut self, victim: usize) {
        {
            let f = &mut self.fighters[victim];
            f.stamp_left = 0;
            f.struggle = false;
            f.lift_hold = false;
            f.lift_launch = None;
        }
        let Some(r) = self.current_row(victim).cloned() else { return };
        if r.kind == reaction_kind::BIND_B {
            // `0x140258c95`: gravity := 0.12, vy := sqrt(0.12 * 20 * w45) -- apex = 10 x w45
            // (the same rule as the Jump, `0x140257b90`); the float status for w42 + w43
            // ticks (`+0x6280 = 1`, `+0x6284`); vertical state cleared (`+0x12e74`).
            let apex = 10.0 * r.w[4].max(0) as f32;
            let f = &mut self.fighters[victim];
            f.lift_launch = Some((2.0 * crate::sim::GRAVITY * apex).sqrt());
            f.lift_hold = true;
            f.status.hover_ticks = ((r.w[1].max(0) + r.w[2].max(0)) as u16).saturating_mul(TICK);
            f.status_hover = true;
        }
        self.root_begin(victim);
    }

    /// Starts reaction row `fighters[victim].reaction_row` the way the program's switch does
    /// for the kinds that end in `Action::Hit` / a collapse / a launch (the caller has set the
    /// row, direction and `hit_seq`). `away`: horizontal unit vector from the attacker.
    pub(super) fn play_row(&mut self, victim: usize, away: Vec3) {
        let row = self.fighters[victim].reaction_row;
        if self.reaction_is_knockdown(row) {
            self.start_knockdown(victim, away);
        } else if self.reaction_kind_of(row) == Some(reaction_kind::CLIP_THEN_LIE) {
            self.start_collapse(victim);
            self.root_begin(victim);
        } else {
            let ticks = self.stagger_ticks(victim);
            let f = &mut self.fighters[victim];
            f.action = Action::Hit;
            f.action_ticks = ticks;
            f.queued_reaction = 0;
            self.row_started(victim);
        }
    }

    /// Kind 7, pinned (`0x140258be1`): a hit on a body lying on the ground (Pursuit, Sky Pursuit,
    /// Backdraft Bullet; only these can reach a downed body, `only_hits_downed`). The clip is
    /// `w41` (DMG_STAMP_FACEUP) when the body's current clip is in `pinned_face_up`, else `w43`
    /// (FACEDOWN); the queued clip behind it is DOWN_FRONT (60) / DOWN_BACK (62) (`+0xdc`),
    /// loop 1; the state is 18 with timer 30. State 18 gets up (`0x14025c3d0`) only once the
    /// current clip is DOWN_FRONT / DOWN_BACK, i.e. after the stamp clip: the lying time is the
    /// longer of the clip and the 30-tick timer, then the ordinary get-up. Returns whether it
    /// applied (a body still in the air is not pinned: its hit picks row 27).
    pub(super) fn start_pinned(&mut self, victim: usize) -> bool {
        let f = &self.fighters[victim];
        if f.action != Action::Knocked || f.flying {
            return false;
        }
        let Some(r) = self.current_row(victim).cloned() else { return false };
        let now = pd_formats::anim::ANIM_NAMES.iter().position(|n| *n == self.lying_clip(victim)).unwrap_or(0);
        let up = pinned_face_up(now);
        let (stamp, pose) = if up { (r.w[0], 60u8) } else { (r.w[2], 62u8) };
        let clip = Self::anim_name(stamp.max(0) as usize);
        let len = self.clip_hold_ticks(victim, clip) + 2;
        let lie = len.max(COLLAPSE_LIE_TICKS);
        let f = &mut self.fighters[victim];
        f.hit_seq = f.hit_seq.wrapping_add(1);
        f.hit_front = up;
        f.lie_from_collapse = true;
        f.lie_pose = pose;
        f.lie_ticks = lie;
        f.lying_elapsed = 0;
        f.land_class = 0;
        f.land_flip = false;
        f.wall_hit = None;
        f.knock_vel = Vec3::ZERO;
        f.queued_reaction = 0;
        f.stamp_left = len;
        f.action_ticks = lie + KNOCK_GETUP_TICKS;
        self.root_begin(victim);
        true
    }

    /// The reaction the program plays when a skill with `s43 == 0` that is not an attack lands
    /// its effect (`FUN_14026f870` -> vtable `+0x48` = `FUN_14024fd80(.., landed)` ->
    /// `FUN_140258690`): on the ground the row is s57 / s58 / s59 by the damage taken in the
    /// last 3 s (`+0x63ec`: < 3 / < 5 / more), 0 for the status skills, and the second row s60
    /// follows only when the effect landed; when the first is 0 the second is played at once
    /// (`0x140258690` start: `param_2 == 0` takes `param_3`). Pressure / Entangle / Pin Down
    /// (kind 8), Lift Up / Antigravity Trap (kind 9) and every other s60 row play this way.
    /// A skill with `s43 != 0` gets row -1, i.e. nothing (`0x14024fe8a`). In the air the same
    /// function picks row 27 / s61 whether or not the effect landed: NOT built here (it would
    /// knock down every airborne victim of a status shell; the attack side does it already).
    pub(super) fn status_skill_reaction(&mut self, caster: usize, victim: usize, def: &SkillDef, players: &[Player]) {
        use pd_formats::skills::field::*;
        if def.capsule == Capsule::Attack || def.raw[43] != 0 || self.reaction_rows.is_empty() || self.airborne[victim] {
            return;
        }
        let f = &self.fighters[victim];
        if matches!(f.action, Action::Down | Action::Knocked) || self.round_over.is_some() {
            return;
        }
        let total = self.recent_damage(victim);
        let first = def.raw[if total < 3 {
            REACTION_LIGHT
        } else if total < 5 {
            REACTION_MEDIUM
        } else {
            REACTION_HEAVY
        }];
        let after = def.raw[REACTION_AFTER_ADD];
        let (row, queue) = if first != 0 { (first, after) } else { (after, 0) };
        if row == 0 || !self.reaction_rows.contains_key(&row) {
            return;
        }
        let (vp, vy) = (players[victim].pos, players[victim].yaw);
        // An instant effect (s63 = 4) gives the reaction the victim's OWN facing as the bearing
        // (`FUN_14024fd80`: `+0x12e68` instead of the atan2 from the attack object), so the hit
        // comes from the front and pushes straight back; anything else is struck from the caster.
        let (dir, front, away) = if def.motion() == 4 {
            (HitDir::Front, true, -Vec3::new(vy.sin(), 0.0, vy.cos()))
        } else {
            let from = players[caster].pos;
            (hit_direction(vp, vy, from), hit_front(vp, vy, from), toward(from, vp))
        };
        let f = &mut self.fighters[victim];
        f.reaction_row = row;
        f.hit_dir = dir;
        f.hit_front = front;
        f.hit_seq = f.hit_seq.wrapping_add(1);
        f.pending_shot = None;
        f.bill = None;
        f.charge = None;
        self.play_row(victim, away);
        if queue != 0 && self.fighters[victim].action == Action::Hit {
            self.fighters[victim].queued_reaction = queue;
        }
    }

    // ---- the paralysed struggle ------------------------------------------------------------

    /// Whether a press of the stick by player `i` starts the struggle: status 4 (paralysed =
    /// `bound_ticks`) on a fighter standing, walking or pivoting (`0x14025b5e0` for states 0,
    /// `0x14025ad50` for state 1; the pivot, state 8, ends in these).
    pub fn can_struggle(&self, i: usize) -> bool {
        let f = &self.fighters[i];
        f.action == Action::Ready && f.status.bound_ticks > 0 && self.round_over.is_none() && !self.counting_down()
    }

    /// The paralysed struggle (state 20): the body takes the stick's direction in the camera's
    /// frame at once (`+0x12e68 := camera yaw + stick angle`, `0x14025b5e0` / `0x14025adcb`),
    /// plays DMG_PARARISE (clip 0x74, loop 0, blend -1, root flag 1) for its length (state 20's
    /// timer = the clip length, `param_1[0x215]`) and then is idle again (`0x140256e80` case 0x14);
    /// each press of the stick repeats it. Reaction row 42's sound / effect play with it
    /// (`FUN_140258fb0(0x2a)`; not wired: no event for it yet).
    pub fn begin_struggle(&mut self, i: usize, yaw: f32) {
        let ticks = self.clip_hold_ticks(i, "DMG_PARARISE").max(2);
        let f = &mut self.fighters[i];
        f.action = Action::Hit;
        f.action_ticks = ticks;
        f.struggle = true;
        f.reaction_row = STRUGGLE_ROW;
        f.hit_seq = f.hit_seq.wrapping_add(1);
        f.pending_turn = Some(yaw);
        f.queued_reaction = 0;
        f.stamp_left = 0;
        f.lift_hold = false;
        f.lift_launch = None;
        f.pending_shot = None;
        f.bill = None;
        f.charge = None;
        f.root = Some(("DMG_PARARISE", 0.0));
    }

    // ---- root motion -----------------------------------------------------------------------

    /// Starts the root motion of `victim`'s current reaction clip when its row says so (word 1 of
    /// the row is the clip start's 5th argument, `[rsi+2]`; the flyers of kinds 4-6 pass 0,
    /// `0x140258b64`).
    pub(super) fn root_begin(&mut self, victim: usize) {
        let moves_body = match self.current_row(victim) {
            Some(r) => r.root && !matches!(r.kind, reaction_kind::KNOCKDOWN | 4 | 6),
            None => false,
        };
        let clip = self.reaction_clip(victim);
        self.fighters[victim].root = moves_body.then_some((clip, 0.0));
    }

    /// Starts the root motion of a clip that is not a reaction row's: the landing clip
    /// (BLOW_LAND_*, root 1 at `0x140259990`) and the get-up (DOWN_*_STAND, `0x14025c4c0`).
    pub(super) fn root_clip(&mut self, i: usize, clip: &'static str) {
        self.fighters[i].root = Some((clip, 0.0));
    }

    /// One sim tick of root motion (`0x140257510`): the clip's root track moves the body by its
    /// x / z change over the tick, rotated by the facing (`+0x12e00` matrix: forward = (sin yaw,
    /// cos yaw), side = (cos yaw, -sin yaw) at yaw 0 = +X, the exporter's convention). The
    /// program's clock is 30 Hz (one frame a tick); the sim's is 60 Hz, so half a frame a sim
    /// tick through the same curve. A clip that is not looping clamps at its end: no drift
    /// after it. Vertical drift is not applied (the program's matrix rows for y are 0).
    pub(super) fn root_step(&mut self, i: usize, players: &mut [Player]) {
        let Some((clip, t)) = self.fighters[i].root else { return };
        let Some(curve) = self.root_curves[i].get(clip) else {
            self.fighters[i].root = None;
            return;
        };
        let last = curve.len().saturating_sub(1) as f32;
        let next = (t + 0.5).min(last);
        let at = |t: f32| -> [f32; 2] {
            let (a, b) = (t.floor() as usize, (t.ceil() as usize).min(curve.len() - 1));
            let k = t - t.floor();
            [curve[a][0] + (curve[b][0] - curve[a][0]) * k, curve[a][1] + (curve[b][1] - curve[a][1]) * k]
        };
        if curve.is_empty() {
            self.fighters[i].root = None;
            return;
        }
        let (p0, p1) = (at(t), at(next));
        let (side, fwd) = (p1[0] - p0[0], p1[1] - p0[1]);
        let yaw = players[i].yaw;
        let (s, c) = yaw.sin_cos();
        players[i].pos += Vec3::new(fwd * s + side * c, 0.0, fwd * c - side * s);
        self.fighters[i].root = if next >= last { None } else { Some((clip, next)) };
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    fn id(name: &str) -> i16 {
        pd_formats::anim::ANIM_NAMES.iter().position(|n| *n == name).unwrap() as i16
    }

    fn clips(c: &mut Combat, table: &[(&str, u16)]) {
        let t: HashMap<String, u16> = table.iter().map(|(n, v)| (n.to_string(), *v)).collect();
        c.set_clip_ticks([t.clone(), t]);
    }

    /// `FUN_140258690` rows 20 / 21 / 24 as the game data has them (cache/skills.json).
    fn rows(c: &mut Combat) {
        c.set_reaction_row(20, ReactionRow { kind: reaction_kind::BIND_A, root: true, w: [id("DMG_BIND_PRESS"), 60, 10, 40, 0, 0, 0, 0], ..Default::default() });
        c.set_reaction_row(21, ReactionRow { kind: reaction_kind::BIND_B, root: true, w: [id("DMG_BIND_AIR"), 50, 25, 50, 5, 0, 0, 0], ..Default::default() });
        c.set_reaction_row(24, ReactionRow { kind: reaction_kind::PINNED, root: true, w: [id("DMG_STAMP_FACEUP"), 60, id("DMG_STAMP_FACEDOWN"), 62, 0, 0, 0, 0], ..Default::default() });
    }

    #[test]
    fn the_pinned_face_up_set_is_the_programs_mask() {
        // `0x140258bf7`: movabs rcx, 0x1c1ce00000000000; bt rcx, current clip id.
        let set: Vec<usize> = (0..70).filter(|&n| pinned_face_up(n)).collect();
        assert_eq!(set, vec![45, 46, 47, 50, 51, 52, 58, 59, 60]);
        assert!(!pinned_face_up(62), "DOWN_BACK is face down");
        assert!((0..70).all(|n| pinned_face_up(n) == program_front_set(n)), "the same mask as the front set");
    }

    #[test]
    fn the_hold_clock_wraps_inside_the_loop() {
        // Plays on to the loop start, then from the loop end back by the loop's length.
        assert_eq!(hold_clock(5, 10, 40), 5);
        assert_eq!(hold_clock(39, 10, 40), 39);
        assert_eq!(hold_clock(40, 10, 40), 10);
        assert_eq!(hold_clock(72, 10, 40), 10 + 32 % 30);
        // A degenerate loop (end <= start) holds at its start once it is reached.
        assert_eq!(hold_clock(20, 10, 10), 10);
    }

    #[test]
    fn a_bind_lasts_its_timer_then_the_rest_of_the_clip() {
        // Row 20 (Pressure): w42 = 60, w43 = 10, w44 = 40: state 16 for 10 + 1 + 60 = 71
        // program ticks (`0x140258bd5`: lea r8d, [rax + 1]; add r8d, ecx), then state 14 for
        // the clip's length minus its time (`0x140256e80` case 0x10).
        let (mut c, _p, _col) = setup(vec![], 100.0);
        rows(&mut c);
        clips(&mut c, &[("DMG_BIND_PRESS", 2 * 100)]);
        let r = c.reaction_rows[&20].clone();
        // at hand-over: frames 72 -> loop 10..40 -> 10 + 32 % 30 = 12; the clip is 100 frames.
        assert_eq!(c.bind_ticks(1, &r), 2 * (71 + 1 + (100 - 12)));
        c.fighters[1].reaction_row = 20;
        assert_eq!(c.stagger_ticks(1), c.bind_ticks(1, &r), "the stagger start uses it");
        assert_eq!(c.reaction_clip(1), "DMG_BIND_PRESS");
        // Hold loop for the renderer: frames w43..w44 while the bind plays.
        c.fighters[1].action = Action::Hit;
        assert_eq!(c.reaction_hold(1), Some((10.0, 40.0)));
        c.fighters[1].action = Action::Ready;
        assert_eq!(c.reaction_hold(1), None);
    }

    #[test]
    fn a_lift_floats_the_body_for_w42_plus_w43_and_is_a_reaction_until_it_lands() {
        // Row 21 (Lift Up): kind 9, w42 = 50, w43 = 25, w44 = 50, w45 = 5: state 14 for 75
        // program ticks (`0x140258c99`), float status 11 for as long (`+0x6280` / `+0x6284`),
        // vy := sqrt(0.12 x 20 x 5) per tick = apex 10 x w45 = 50 u (`0x140258cbc-0x140258cdb`,
        // constant 20.0 at `0x1402f7fc4`).
        let (mut c, mut p, col) = setup(vec![], 100.0);
        rows(&mut c);
        c.fighters[1].reaction_row = 21;
        c.fighters[1].hit_seq += 1;
        c.play_row(1, Vec3::Z);
        let f = &c.fighters[1];
        assert_eq!((f.action, f.action_ticks), (Action::Hit, 2 * 76));
        assert_eq!(f.status.hover_ticks, 2 * 75);
        assert!(f.status_hover);
        let v = f.lift_launch.expect("launch pending");
        assert!((v * v / (2.0 * crate::sim::GRAVITY) - 50.0).abs() < 1e-3, "apex 50 u: {v}");
        assert_eq!(c.reaction_hold(1), Some((25.0, 50.0)));
        // The next tick the body takes the launch.
        run(&mut c, &mut p, &col, 1);
        assert!(!p[1].grounded && p[1].vel.y > 0.0);
    }

    #[test]
    fn a_lifted_body_stays_in_its_reaction_until_it_is_back_on_the_ground() {
        // `0x14025c1a0`: at timer 0 the state only goes idle when grounded (`+0x12e74 != 0`).
        let (mut c, mut p, col) = setup(vec![], 100.0);
        rows(&mut c);
        c.fighters[1].reaction_row = 21;
        c.play_row(1, Vec3::Z);
        // The timer runs out while the body is in the air (not grounded, as the sim leaves it).
        p[1].grounded = false;
        p[1].pos.y = 30.0;
        c.fighters[1].action_ticks = 2;
        c.fighters[1].status.hover_ticks = 0;
        c.fighters[1].lift_launch = None;
        run(&mut c, &mut p, &col, 5);
        assert_eq!(c.fighters[1].action, Action::Hit, "held until it lands");
        p[1].grounded = true;
        run(&mut c, &mut p, &col, 2);
        assert_eq!(c.fighters[1].action, Action::Ready);
        assert!(!c.fighters[1].lift_hold);
    }

    #[test]
    fn pinned_picks_the_stamp_by_the_lying_clip_and_gets_up_after_it() {
        // Kind 7 (row 24, Pursuit): the current clip DOWN_FRONT (60) is in the mask: FACEUP (w41)
        // with DOWN_FRONT queued (`+0xdc = 0x3c`); a body in DOWN_BACK (62) gets FACEDOWN (w43)
        // with DOWN_BACK queued (`0x3e`). State 18, timer 30; the get-up waits for the DOWN clip.
        let (mut c, mut p, col) = setup(vec![], 100.0);
        rows(&mut c);
        clips(&mut c, &[("DMG_STAMP_FACEUP", 2 * 40), ("DMG_STAMP_FACEDOWN", 2 * 20), ("DOWN_FRONT_STAND", 60), ("DOWN_BACK_STAND", 60)]);
        for (pose, stamp, getup) in [(60u8, "DMG_STAMP_FACEUP", "DOWN_FRONT_STAND"), (62, "DMG_STAMP_FACEDOWN", "DOWN_BACK_STAND")] {
            let f = &mut c.fighters[1];
            f.action = Action::Knocked;
            f.flying = false;
            f.lie_from_collapse = true;
            f.lie_pose = pose;
            f.lie_ticks = COLLAPSE_LIE_TICKS;
            f.action_ticks = COLLAPSE_LIE_TICKS + KNOCK_GETUP_TICKS;
            f.reaction_row = 24;
            assert!(c.start_pinned(1));
            let f = &c.fighters[1];
            assert_eq!(c.reaction_clip(1), stamp);
            assert_eq!(f.lie_pose, pose);
            assert!(f.collapsing(), "the stamp plays first");
            // Lying time = the longer of the clip (+ 2) and the 30-tick timer (62), then the get-up.
            let clip = c.clip_hold_ticks(1, stamp) + 2;
            assert_eq!(f.action_ticks, clip.max(COLLAPSE_LIE_TICKS) + KNOCK_GETUP_TICKS);
            run(&mut c, &mut p, &col, clip as usize + 1);
            assert!(!c.fighters[1].collapsing(), "then the DOWN loop");
            assert_eq!(c.getup_clip(1), getup);
        }
    }

    #[test]
    fn pinned_does_not_apply_to_a_standing_or_flying_body() {
        let (mut c, _p, _col) = setup(vec![], 100.0);
        rows(&mut c);
        c.fighters[1].reaction_row = 24;
        assert!(!c.start_pinned(1), "standing");
        c.fighters[1].action = Action::Knocked;
        c.fighters[1].flying = true;
        assert!(!c.start_pinned(1), "in flight");
    }

    #[test]
    fn pressure_plays_its_s60_bind_clip_when_the_effect_lands() {
        // Pressure (42): s43 = 0, s57-s59 = 0, s60 = 20: `FUN_140258690(0, 20, ..)` takes the
        // second row as the only one (`0x1402586a5`), on the ground.
        let mut pressure = def(42, Capsule::Status, 1, &[(43, 0), (51, 6), (52, 90), (60, 20)]);
        pressure.raw[43] = 0;
        let (mut c, mut p, _col) = setup(vec![pressure.clone()], 100.0);
        rows(&mut c);
        clips(&mut c, &[("DMG_BIND_PRESS", 200)]);
        c.status_skill_reaction(0, 1, &pressure, &p);
        let f = &c.fighters[1];
        assert_eq!((f.action, f.reaction_row), (Action::Hit, 20));
        assert_eq!(c.reaction_clip(1), "DMG_BIND_PRESS");
        assert_eq!(f.queued_reaction, 0, "nothing queued behind it");
        // A skill with s43 != 0 gets row -1: nothing (`0x14024fe8a`).
        let mut other = pressure.clone();
        other.raw[43] = 4;
        c.fighters[1].action = Action::Ready;
        c.fighters[1].reaction_row = 0;
        c.status_skill_reaction(0, 1, &other, &p);
        assert_eq!(c.fighters[1].action, Action::Ready);
        // In the air: the program's row 27 rule is not built (documented), nothing here.
        p[1].grounded = false;
        c.airborne[1] = true;
        c.status_skill_reaction(0, 1, &pressure, &p);
        assert_eq!(c.fighters[1].action, Action::Ready);
    }

    #[test]
    fn an_everyone_instant_gives_every_player_its_reaction_the_caster_included() {
        // Meltdown / Quantum Decay (s43 = 0, s44 = 2, motion 4, s64 = 2, s60 = 15): `FUN_140265f50`
        // hits every player with `FUN_14026f870`, whose vtable `+0x48` plays the s60 row at each (LIVE:
        // both fighters in state 14 for ~51 ticks, the caster too). The bearing is the victim's own
        // facing (`FUN_14024fd80`: motion 4 uses `+0x12e68`), so the hit is a front one.
        let mut melt = def(102, Capsule::Erase, 3, &[(43, 0), (51, 49), (52, 2), (63, 4), (60, 20)]);
        melt.raw[43] = 0;
        let (mut c, p, _col) = setup(vec![melt.clone()], 100.0);
        rows(&mut c);
        clips(&mut c, &[("DMG_BIND_PRESS", 200)]);
        for v in [0, 1] {
            c.status_skill_reaction(0, v, &melt, &p);
            let f = &c.fighters[v];
            assert_eq!((f.action, f.reaction_row, f.hit_front), (Action::Hit, 20, true), "player {v}");
        }
    }

    #[test]
    fn only_the_add_codes_that_can_change_nothing_withhold_the_reaction() {
        // LIVE: Meltdown 49, Quantum Decay 4, Fairy Ring 1, Turbulence 0 react; Quantum Amp 44 and
        // Blessed Rain 43 react even at full Level / Health, Chill Breeze 7 always; Purify 17, Armaros 51,
        // Return to Nature 39 and Judge of Ares 0 stay idle when they change nothing.
        for code in [0, 1, 4, 17, 39, 49, 51] {
            assert!(add_can_return_zero(code), "{code}");
        }
        for code in [6, 7, 8, 37, 43, 44, 46] {
            assert!(!add_can_return_zero(code), "{code}");
        }
    }

    #[test]
    fn the_struggle_plays_dmg_pararise_for_its_length_and_faces_the_stick() {
        // `0x14025b5e0`: status 4 + the stick: facing := camera yaw + stick angle, clip 0x74 (116),
        // state 20 for the clip's length (`param_1[0x215]`), then idle (`0x140256e80` case 0x14).
        assert_eq!(id("DMG_PARARISE"), 116);
        let (mut c, mut p, col) = setup(vec![], 100.0);
        clips(&mut c, &[("DMG_PARARISE", 50)]);
        c.fighters[0].status.bound_ticks = 400;
        assert!(c.can_struggle(0));
        c.begin_struggle(0, 1.0);
        assert!(!c.can_struggle(0), "already struggling");
        assert_eq!((c.fighters[0].action, c.fighters[0].action_ticks), (Action::Hit, 50));
        assert_eq!(c.reaction_clip(0), "DMG_PARARISE");
        run(&mut c, &mut p, &col, 1);
        assert!((p[0].yaw - 1.0).abs() < 1e-4, "turned to the stick: {}", p[0].yaw);
        run(&mut c, &mut p, &col, 60);
        assert_eq!(c.fighters[0].action, Action::Ready);
        assert!(!c.fighters[0].struggle, "idle again");
        assert!(c.can_struggle(0), "a new push repeats it");
        // Only while paralysed.
        c.fighters[0].status.bound_ticks = 0;
        assert!(!c.can_struggle(0));
    }

    #[test]
    fn root_motion_moves_the_body_along_its_clip_and_stops_at_the_clips_end() {
        // `0x140257510`: the per-tick change of the root track, rotated by the facing, added to
        // the position; 30 Hz clock -> half a frame a sim tick.
        let (mut c, mut p, col) = setup(vec![], 100.0);
        // 4 frames: +2 forward and +1 side per frame, cumulative.
        let curve: Vec<[f32; 2]> = (0..5).map(|k| [k as f32, 2.0 * k as f32]).collect();
        let t: HashMap<String, Vec<[f32; 2]>> = [("DMG_SHOT_FRONT".to_string(), curve)].into_iter().collect();
        c.set_root_curves([t.clone(), t]);
        c.fighters[0].root = Some(("DMG_SHOT_FRONT", 0.0));
        run(&mut c, &mut p, &col, 100);
        // yaw 0: forward = +Z, side = +X: (4, 0, 8) in all, then nothing more.
        assert!((p[0].pos.x - 4.0).abs() < 1e-3 && (p[0].pos.z - 8.0).abs() < 1e-3, "{:?}", p[0].pos);
        assert!(c.fighters[0].root.is_none());
        // Turned half a turn the same clip drifts the other way.
        p[1].yaw = std::f32::consts::PI;
        let before = p[1].pos;
        c.fighters[1].root = Some(("DMG_SHOT_FRONT", 0.0));
        run(&mut c, &mut p, &col, 100);
        let d = p[1].pos - before;
        assert!((d.x + 4.0).abs() < 1e-3 && (d.z + 8.0).abs() < 1e-3, "{d:?}");
    }

    #[test]
    fn a_reaction_row_with_the_root_flag_starts_root_motion_but_a_flyer_does_not() {
        let (mut c, _p, _col) = setup(vec![], 100.0);
        rows(&mut c);
        c.set_reaction_row(2, ReactionRow { kind: reaction_kind::FOUR_WAY, root: true, w: [id("DMG_SHOT_FRONT"), id("DMG_SHOT_RIGHT"), id("DMG_SHOT_BACK"), id("DMG_SHOT_LEFT"), 0, 0, 0, 0], ..Default::default() });
        c.set_reaction_row(46, ReactionRow { kind: reaction_kind::FIXED_TICKS, root: false, w: [id("DMG_SHORT"), 5, 0, 0, 0, 0, 0, 0], ..Default::default() });
        c.set_reaction_row(3, ReactionRow { kind: reaction_kind::KNOCKDOWN, root: true, launch_speed: 20, launch_angle: 50, w: [0, 45, 0, 0, 0, 48, 0, 0], ..Default::default() });
        c.fighters[1].reaction_row = 2;
        c.root_begin(1);
        assert_eq!(c.fighters[1].root, Some(("DMG_SHOT_FRONT", 0.0)), "row w1 = 1 (`0x1402588bc`: [rsp+0x20] = [rsi+2])");
        c.fighters[1].reaction_row = 46;
        c.root_begin(1);
        assert_eq!(c.fighters[1].root, None, "the flinch row has w1 = 0");
        c.fighters[1].reaction_row = 3;
        c.root_begin(1);
        assert_eq!(c.fighters[1].root, None, "a flyer's clip start passes root 0 (`0x140258b64`: r14d)");
    }

    /// With the converted cache (`PD_CACHE`, else `engine/cache`; skipped without it): pc00a's real
    /// curves move a body by their clip's totals -- DMG_SHOT_FRONT shoves it back 10.4 u, a hit
    /// from the side (DMG_SHOT_RIGHT) sideways 11.3 u, the get-up (DOWN_FRONT_STAND) 3.2 u forward.
    #[test]
    fn the_converted_root_curves_move_the_body_by_the_clips_totals() {
        let dir = std::env::var_os("PD_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
        let Ok(text) = std::fs::read_to_string(dir.join("chars/pc00a.rootmotion.json")) else { return };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let curves: HashMap<String, Vec<[f32; 2]>> = v["clips"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, c)| (k.clone(), c["curve"].as_array().unwrap().iter().map(|p| [p["side"].as_f64().unwrap() as f32, p["forward"].as_f64().unwrap() as f32]).collect()))
            .collect();
        for (clip, side, fwd) in [("DMG_SHOT_FRONT", 0.3, -10.4), ("DMG_SHOT_RIGHT", 11.3, 1.9), ("DOWN_FRONT_STAND", 0.4, 3.2)] {
            let (mut c, mut p, col) = setup(vec![], 100.0);
            c.set_root_curves([curves.clone(), curves.clone()]);
            let name: &'static str = pd_formats::anim::ANIM_NAMES.iter().copied().find(|n| *n == clip).unwrap();
            c.fighters[0].root = Some((name, 0.0));
            run(&mut c, &mut p, &col, 80);
            assert!((p[0].pos.x - side).abs() < 0.2 && (p[0].pos.z - fwd).abs() < 0.2, "{clip}: {:?}", p[0].pos);
        }
    }
}
