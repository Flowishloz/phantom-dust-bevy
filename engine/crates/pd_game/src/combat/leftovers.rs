//! Film-comparison leftovers (test-only): the real skill tables against the original's own takes
//! (`%USERPROFILE%\.pdtools\moverec\takes_full\`). Skipped when the converted cache is not present.

use super::testkit::{floor, TEST_RULES};
use super::*;
use std::path::{Path, PathBuf};

fn cache() -> PathBuf {
    std::env::var_os("PD_CACHE").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"))
}

/// A duel on a flat floor (the full sim: bodies, physics and combat), P0 at the origin facing +z, P1 `dist` ahead, with the real
/// tables. `seed` varies the sim's own random stream.
pub(super) fn world(dist: f32, seed: u32) -> Option<crate::sim::Match> {
    let cache = cache();
    let sj = cache.join("skills.json");
    if !sj.exists() {
        return None;
    }
    let ch = ["pc00a".to_string()];
    let clips = crate::load_clip_ticks(&cache, &ch).into_iter().next().unwrap_or_default();
    let root = crate::load_root_curves(&cache, &ch).into_iter().next().unwrap_or_default();
    let mut c = Combat::new(crate::load_skills(&sj, false), vec![AURA_PARTICLE; 3]);
    c.set_rules(TEST_RULES);
    c.round_ticks = COUNTDOWN_TICKS;
    c.set_clip_ticks(vec![clips.clone(), clips]);
    c.set_root_curves([root.clone(), root]);
    c.set_action_rows(crate::load_action_rows(&sj));
    c.set_reaction_rows(crate::load_reaction_rows(&sj));
    c.rng = 0x9e37_79b9 ^ seed.wrapping_mul(0x85eb_ca6b);
    for i in 0..2 {
        c.fighters[i].level = 12;
        c.fighters[i].aura_ticks = 12 * AURA_TICKS_PER_POINT;
        c.fighters[i].max_health = if i == 0 { 20 } else { 40 };
        c.fighters[i].health = c.fighters[i].max_health;
    }
    let mut m = crate::sim::Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, dist)], c);
    for q in m.players.iter_mut().take(2) {
        q.locked = false;
    }
    Some(m)
}

/// Presses button 0 with `skill` locked on P1 and runs `n` sim ticks. Returns, per PROGRAM tick after the press
/// (a program tick = 2 sim ticks), (P0 health, P1 health).
fn run(m: &mut crate::sim::Match, skill: i16, n: usize) -> Vec<(i32, i32)> {
    let mut col = floor();
    m.combat.fighters[0].slots = [Some(skill), None, None, None];
    m.players[0].locked = true;
    m.players[0].target = Some(1);
    let mut out = Vec::new();
    for t in 0..n {
        m.combat.capsules.clear();
        m.step([crate::sim::Input { skills: u8::from(t == 0), ..Default::default() }, crate::sim::Input::default()], &mut col);
        if t % 2 == 1 {
            out.push((m.combat.fighters[0].health, m.combat.fighters[1].health));
        }
        if std::env::var_os("PD_LO_DEBUG").is_some() && t % 2 == 1 {
            let c = &m.combat;
            let shots: Vec<String> = c.projectiles.iter().map(|s| format!("({:.1},{:.1},{:.1})", s.pos.x, s.pos.y, s.pos.z)).collect();
            eprintln!("pt {} p0 {:?} {:.1}/{:.1}/{:.1} hp {}/{} shots {:?} areas {}", t / 2, c.fighters[0].action, m.players[0].pos.x, m.players[0].pos.y, m.players[0].pos.z, c.fighters[0].health, c.fighters[1].health, shots, c.areas.len());
        }
    }
    out
}

/// Program ticks after the press at which P1's Health first dropped, if it did.
fn first_hit(h: &[(i32, i32)]) -> Option<usize> {
    h.windows(2).position(|w| w[1].1 < w[0].1).map(|i| i + 1)
}

/// Scratch: `PD_LO_SKILL=<id> PD_LO_DIST=<u> PD_LO_DEBUG=1 cargo test -p pd_game lo_dump -- --nocapture` follows one cast.
#[test]
fn lo_dump() {
    let Some(id) = std::env::var("PD_LO_SKILL").ok().and_then(|s| s.parse::<i16>().ok()) else { return };
    let dist: f32 = std::env::var("PD_LO_DIST").ok().and_then(|s| s.parse().ok()).unwrap_or(70.0);
    let Some(mut m) = world(dist, 0) else { return };
    let h = run(&mut m, id, 400);
    eprintln!("skill {id}: first P1 hit {:?} program ticks after the press; end hp {:?}", first_hit(&h), h.last());
}

// ---- item 3: one random STR roll per cast ---------------------------------------------------------------------------------

/// READ `FUN_140274a10` (table `0x1402dd170`: rules 26 -> `0x140274990`, 27 / 28 / 29 -> `0x140274a10`):
/// STR = s46 x ((rand() & 0x7fff) x (rule - 25) >> 15), so rule 28 is 0, s46 or 2 x s46 -- the notes' "randomly 0, 3, or 6".
/// READ `FUN_1402626d0` (`0x140272400` is called once, `local_128`): the number is rolled once per cast and handed to every
/// object the cast makes. LIVE `319_Jack_Pot` (P0 -3, P1 -3, same tick) and `321_Excaliber` (-3): a 3 each, drawn from {0, 3, 6};
/// the draw itself is the program's `rand()` (seeded from the clock on the audio thread, `FUN_14028aed0`), so a film cannot
/// reproduce it, only the set and the "one roll per cast".
#[test]
fn jack_pot_rolls_once_and_both_holes_take_the_same_number() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..24u32 {
        let Some(mut m) = world(50.0, seed) else { return };
        run(&mut m, 319, 400);
        let (lost0, lost1) = (20 - m.combat.fighters[0].health, 40 - m.combat.fighters[1].health);
        assert!([0, 3, 6].contains(&lost1), "seed {seed}: P1 lost {lost1}");
        assert_eq!(lost0, lost1, "seed {seed}: the caster's own hole and the target's share the cast's one roll (LIVE: -3 / -3)");
        seen.insert(lost1);
    }
    assert_eq!(seen.len(), 3, "all of 0, 3 and 6 come up over the seeds: {seen:?}");
}

#[test]
fn excaliber_is_zero_three_or_six() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..24u32 {
        let Some(mut m) = world(35.0, seed) else { return };
        run(&mut m, 321, 300);
        seen.insert(40 - m.combat.fighters[1].health);
    }
    assert!(seen.iter().all(|v| [0, 3, 6].contains(v)), "{seen:?}");
    assert!(seen.len() >= 2, "{seen:?}");
}

// ---- item 4: self-hit damage ----------------------------------------------------------------------------------------------

/// LIVE `323_Fast_Bomb`: the setter's own bomb takes 2 off P0 on its own (blast at tick 67), nothing more.
#[test]
fn fast_bomb_costs_its_setter_two() {
    let Some(mut m) = world(50.0, 0) else { return };
    run(&mut m, 323, 300);
    assert_eq!(m.combat.fighters[0].health, 18, "P0: -2 (LIVE `323_Fast_Bomb` P0 20 -> 18)");
}

/// LIVE `303_Antigravity_Trap`: the setter walks into its own trap, floats 48.3 u (270.48 -> 318.76) in state 14, comes down in
/// state 17 on tick 212 with Health -1 (the knock-down landing rule, `FUN_140254c80`: 40-80 u = 1). Ours used to land for free.
#[test]
fn antigravity_trap_lift_peaks_near_fifty_and_the_landing_costs_its_setter_one() {
    let Some(mut m) = world(50.0, 0) else { return };
    let mut col = floor();
    m.combat.fighters[0].slots = [Some(303), None, None, None];
    m.players[0].locked = true;
    m.players[0].target = Some(1);
    let mut peak = 0.0f32;
    for t in 0..700 {
        m.combat.capsules.clear();
        m.step([crate::sim::Input { skills: u8::from(t == 0), ..Default::default() }, crate::sim::Input::default()], &mut col);
        peak = peak.max(m.players[0].pos.y);
    }
    // LIVE: +48.3 u; READ (row 21, kind 9): apex 10 x w45 = 50 u. It used to stack a pinned 10 ft hover under the launch (85.7 u).
    assert!((peak - 50.0).abs() < 4.0, "the lift tops out at {peak}");
    assert_eq!(m.combat.fighters[0].health, 19, "P0: -1 on the landing (40-80 u of fall)");
    // (P1 stands 50 u from the mine -- inside its 50 u blast once it has grown, which the take ended 1 tick too soon to show.)
}


/// LIVE `086_Overheat`: P0 -3 and P1 -3 on the same tick (fire 64, hit 96). READ rule 30 (`STR = Aura Particles on the battlefield`,
/// FORMATS-SKILLS s45 = 30, here `AURA_PARTICLES_ON_FIELD`): the original's stage held 3. The film harness clears the field, so its
/// Overheat deals 0 -- a setup difference, not a rule one (film_compare.py sets that diff aside).
#[test]
fn overheat_deals_one_per_aura_particle_on_the_field_to_both_fighters() {
    let Some(mut m) = world(50.0, 0) else { return };
    let mut col = floor();
    m.combat.fighters[0].slots = [Some(86), None, None, None];
    m.players[0].locked = true;
    m.players[0].target = Some(1);
    for k in 0..3u32 {
        m.combat.capsules.push(CapsuleItem { id: 900 + k, owner: 1, skill: AURA_PARTICLE, pos: Vec3::new(600.0 + 20.0 * k as f32, 0.0, 600.0), appear: 0 });
    }
    for t in 0..400 {
        m.step([crate::sim::Input { skills: u8::from(t == 0), ..Default::default() }, crate::sim::Input::default()], &mut col);
    }
    assert_eq!((20 - m.combat.fighters[0].health, 40 - m.combat.fighters[1].health), (3, 3), "both holes, STR 3");
}

// ---- item 6: the Aura refill stops 60 program ticks after the round is decided -----------------------------------------------

/// LIVE `257_Judge_of_Anubis`: KO at rec tick 64; P0's Aura 4.00 -> 5.31, constant from tick 124 = KO + 60 program ticks
/// (READ `FUN_14012c510`: the 0x3c wait before the round closes).
#[test]
fn aura_refill_stops_sixty_program_ticks_after_the_knock_out() {
    let Some(mut m) = world(70.0, 0) else { return };
    m.combat.fighters[0].aura_ticks = 4 * AURA_TICKS_PER_POINT;
    m.combat.round_decided_tick = Some(m.combat.round_ticks);
    m.combat.round_over = Some((0, ROUND_END_TICKS));
    let mut col = floor();
    let mut at = Vec::new();
    for _ in 0..170 {
        m.combat.capsules.clear();
        m.step([crate::sim::Input::default(), crate::sim::Input::default()], &mut col);
        at.push(m.combat.fighters[0].aura_ticks);
    }
    let when = |t: usize| at[t - 1];
    assert!(when(60) > 4 * AURA_TICKS_PER_POINT, "still refilling inside the 60 program ticks");
    assert!(when(120) > when(60));
    assert_eq!(when(140), when(121), "frozen after KO + 120 sim ticks");
    assert_eq!(when(170), when(121));
}

// ---- film field (`PD_FILM_FIELD`) -----------------------------------------------------------------------------------------------

/// The field capsules the film puts on the stage's spots (`Combat::place_field_capsule`): on the owner's own spot, takeable, and a
/// spot a fighter stands on is moved away so a skill press there casts instead of capturing. Meltdown then reads each owner's
/// Aura Particles (LIVE `102_Meltdown`: P0 12 -> 8 with two, P1 12 -> 10 with one).
#[test]
fn film_field_capsules_lie_on_the_owners_spots_and_meltdown_counts_them() {
    let Some(mut m) = world(60.0, 0) else { return };
    let spots = [Vec3::new(500.0, 0.0, 0.0), Vec3::new(520.0, 0.0, 0.0), Vec3::new(540.0, 0.0, 0.0)];
    m.combat.set_capsule_spots([spots.to_vec(), spots.iter().map(|s| *s + Vec3::new(0.0, 0.0, 100.0)).collect::<Vec<_>>()]);
    let bodies = [m.players[0].pos, m.players[1].pos];
    for (owner, n) in [(0usize, 2usize), (1, 1)] {
        for k in 0..3 {
            m.combat.place_field_capsule(owner, k, if k < n { AURA_PARTICLE } else { 1 }, &bodies);
        }
    }
    assert_eq!(m.combat.capsules.len(), 6);
    assert!(m.combat.capsules.iter().filter(|c| c.owner == 0).all(|c| c.pos.x >= 500.0 && c.pos.z == 0.0 && c.appear == 0));
    assert_eq!(m.combat.capsules.iter().filter(|c| c.skill == AURA_PARTICLE && c.owner == 0).count(), 2);
    assert_eq!(m.combat.capsules.iter().filter(|c| c.skill == AURA_PARTICLE && c.owner == 1).count(), 1);
    // A spot under a fighter is moved away from it.
    let mut c2 = m.combat.clone();
    c2.capsules.clear();
    c2.set_capsule_spots([vec![bodies[0]], vec![]]);
    c2.place_field_capsule(0, 0, AURA_PARTICLE, &bodies);
    assert!(!in_pickup_reach(c2.capsules[0].pos, bodies[0]), "moved out of the fighter's reach");
}

/// LIVE `073_Gungnir`, `204_Ki_Cannon` (takes_full): the button is down 3 ticks (press 39, up 42), the shot leaves at press + 25 and the cast
/// state (10) lasts until tick 86 = press + clip (46) + 1. READ `FUN_14025c720`: a release inside the wind-up waits it out and the cast goes on
/// from the charge-cycle frame, so the whole cast is still the clip + 1 program tick from the press. Ours restarted the full clip at the
/// release (cast end press + 60, +13 over the original).
#[test]
fn a_short_press_on_a_charge_shot_keeps_the_cast_to_press_plus_clip_plus_one() {
    let Some(mut m) = world(60.0, 0) else { return };
    let mut col = floor();
    let f = &mut m.combat.fighters[0];
    f.slots = [Some(73), None, None, None];
    f.level = 12;
    f.aura_ticks = 12 * AURA_TICKS_PER_POINT;
    m.players[0].locked = true;
    m.players[0].target = Some(1);
    let (mut fired, mut ended) = (None, None);
    for t in 0..220u32 {
        m.step([crate::sim::Input { skills: u8::from(t < 6), ..Default::default() }, crate::sim::Input::default()], &mut col);
        if fired.is_none() && m.combat.events.iter().any(|(_, e)| matches!(e, CombatEvent::SkillFired { who: 0, skill: 73 })) {
            fired = Some(t);
        }
        if fired.is_some() && ended.is_none() && m.combat.fighters[0].action != Action::Cast {
            ended = Some(t);
        }
    }
    let (fired, ended) = (fired.expect("fired") as i32, ended.expect("cast ended") as i32);
    assert!((fired - 50).abs() <= 3, "the shot leaves at press + 25 program ticks, got sim tick {fired}");
    assert!((ended - 94).abs() <= 3, "the cast ends at press + 47 program ticks (clip 46 + 1), got sim tick {ended}");
}
