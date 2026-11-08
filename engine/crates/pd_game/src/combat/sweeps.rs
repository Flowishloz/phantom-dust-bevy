//! The stage sweep a melee blow's attack object makes on each update: read by the sound side only
//! (`ground_sfx`), which voices the stage face it crosses. Nothing here changes the sim.
//!
//! READ (r7sound, 2026-10-27; P0-SOUND.md section 14):
//! * sword, motion 12 (`FUN_140269db0`): every update copies the owner's stored RHAND matrix into the
//!   object (`+0xd0..+0x10c`), and while flag `0x400` is set sweeps the probe `DAT_1404bec68` (owner
//!   `pdStageColPtr_PlayerBullet`, mode 4: its stage contacts are the ones `FUN_1402021f0` turns into
//!   impact marks and surface-hit sounds) along the segment from the caster's centre (`+0x1a4` = feet +
//!   `+0x12e98`) to the blade tip `+0x100 - +0xf0 * len` (`len` = `+0x190`, the blade): the hand point
//!   minus the bone's Z row times the blade length (`FUN_1401247c0` scales, `FUN_140124780` = a - b).
//!   A contact clears `0x400`, so one blow sounds once at most.
//! * punch / kick / wave, motion 13 (`FUN_14026a0d0`): the same sweep from the caster's centre to a point
//!   20 u (`DAT_1402f7fc4`) ahead along the caster's facing (`+0x12e68`) at centre height.
//!
//! MEASURED (the PC original's takes, `moverec/takes_full`): with the blade tip on the minus-Z side of the
//! converted hand path (`cache/chars/pc00a.hand.json`), the tip goes under the floor one or two program
//! ticks after the fire for exactly Blazing Sword, Frostbite and Excaliber, the three sword takes whose
//! original plays a surface-hit sound then (0.9 / 0.8 / 0.8 at fire + 1 / + 2 / + 2); for every other
//! sword it stays 4.6 u or more above the feet in those ticks. Film (`.pdtools/compare_r7sound_after`):
//! Ice Sword's 0.9 is its blade crossing a class-6 wall beside the caster; Vacuum Slash and Muramasa Blade's
//! tip dips under the floor late in the swing and plays the original's 0.8 on its tick (t74.5 vs t74).

use super::*;

/// How far ahead of the caster's centre a punch's sweep reaches (`DAT_1402f7fc4` = 20.0).
pub const PUNCH_SWEEP_REACH: f32 = 20.0;

impl Combat {
    /// The segment player `i`'s melee blow sweeps against the stage on this sim tick: (from, to), or
    /// `None` when no blow object is alive. The object lives from one program tick after the fire (its
    /// first update only arms it) to the end of its window, the same ticks the blow tests bodies in
    /// (`lunge`); the hand is the clip frame the blade test reads.
    pub fn melee_sweep(&self, i: usize, players: &[Player]) -> Option<(Vec3, Vec3)> {
        let f = self.fighters.get(i)?;
        if !f.melee || f.charge.is_some() || !matches!(f.action, Action::Cast | Action::Lunge) {
            return None;
        }
        let def = self.skill(f.cast_skill)?;
        if def.raw[43] != 0 {
            return None;
        }
        let Behaviour::Melee(melee) = moves::behaviour_of(def) else { return None };
        let row = self.cast_row(i, f.cast_skill)?;
        let fire = moves::table_ticks(row.fire_tick);
        let end = moves::table_ticks(row.hit_window_end).max(fire.saturating_add(1));
        let arm = moves::SIM_TICKS_PER_TABLE_TICK;
        let e = f.cast_elapsed;
        if e < fire + arm || e > end + arm {
            return None;
        }
        let me = players.get(i)?;
        let from = body_centre(me.pos);
        match melee.shape {
            moves::melee::Shape::Sword => {
                let (p, d) = self.hands[i].get(row.anim_name.as_str())?.at(e as f32 / 2.0)?;
                let hand = local_to_world(me.pos, me.yaw, p);
                Some((from, hand - local_dir_to_world(me.yaw, d) * melee.reach))
            }
            moves::melee::Shape::Sphere => Some((from, from + Vec3::new(me.yaw.sin(), 0.0, me.yaw.cos()) * PUNCH_SWEEP_REACH)),
            moves::melee::Shape::Stamp => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use glam::Vec3;
    use std::collections::HashMap;

    /// A punch sweeps 20 u straight ahead of the caster's centre while its blow lives; no blow, no sweep.
    #[test]
    fn a_punch_sweeps_twenty_units_ahead_at_centre_height() {
        let punch = def(9001, Capsule::Attack, 1, &[(f::MOTION, 13), (P, 50)]);
        let mut c = Combat::new(vec![punch], vec![]);
        c.set_action_row(9001, Some(ActionRow { anim_name: "ATK_PUNCH".into(), fire_tick: 10, hit_window_end: 12, ..Default::default() }), None);
        let players = [Player::new(Vec3::new(10.0, 5.0, 20.0), std::f32::consts::FRAC_PI_2), Player::new(Vec3::ZERO, 0.0)];
        assert_eq!(c.melee_sweep(0, &players), None, "not casting");
        let f = &mut c.fighters[0];
        f.melee = true;
        f.action = Action::Cast;
        f.cast_skill = 9001;
        f.cast_elapsed = 21; // fire at 20 sim ticks: the blow is not armed yet
        assert_eq!(c.melee_sweep(0, &players), None);
        c.fighters[0].cast_elapsed = 22;
        let (from, to) = c.melee_sweep(0, &players).expect("armed blow");
        assert!((from - Vec3::new(10.0, 14.0, 20.0)).length() < 1e-4, "{from:?}");
        assert!((to - Vec3::new(30.0, 14.0, 20.0)).length() < 1e-4, "{to:?}");
        c.fighters[0].cast_elapsed = 27; // past the window (24 + 2)
        assert_eq!(c.melee_sweep(0, &players), None);
    }

    /// A sword sweeps from the centre to the blade tip on the hand's minus-Z side (`FUN_140269db0`).
    #[test]
    fn a_sword_sweeps_to_the_tip_behind_the_hand_z_axis() {
        let sword = def(9002, Capsule::Attack, 1, &[(f::MOTION, 12), (P, 250)]);
        let mut c = Combat::new(vec![sword], vec![]);
        c.set_action_row(9002, Some(ActionRow { anim_name: "ATK_SORD".into(), fire_tick: 10, hit_window_end: 12, ..Default::default() }), None);
        c.set_hand_paths([HashMap::from([("ATK_SORD".to_string(), HandClip { pos: vec![Vec3::new(0.0, 12.0, 5.0); 40], fwd: vec![Vec3::Y; 40] })])]);
        let players = [Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, 50.0), 0.0)];
        let f = &mut c.fighters[0];
        f.melee = true;
        f.action = Action::Cast;
        f.cast_skill = 9002;
        f.cast_elapsed = 22;
        let (from, to) = c.melee_sweep(0, &players).expect("armed blow");
        assert!((from - Vec3::new(0.0, 9.0, 0.0)).length() < 1e-4);
        assert!((to - Vec3::new(0.0, -13.0, 5.0)).length() < 1e-3, "tip 25 u below the hand: {to:?}");
    }
}
