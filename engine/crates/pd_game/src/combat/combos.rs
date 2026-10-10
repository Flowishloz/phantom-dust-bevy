//! Combos (chained follow-ups): High-speed Punch, Rapid Cannon, Trench Mortar.
//!
//! READ in `decomp_all.c`: the gate `FUN_14024fb90` (called through `FUN_140261e90` from the
//! button loop `FUN_1402517c0`, which is the LAST step of the player update `FUN_14024ec80`), the
//! cast start `FUN_14025a3e0` (row choice, `+0xb0` = the chain counter) and the cast-start state
//! `FUN_140258180`. The rules are in `Combat::chain_step`.

use super::testkit as k;
use super::*;
use std::collections::HashMap;

const PUNCH: i16 = 185;
const CANNON: i16 = 75;
const OTHER: i16 = 9;

/// High-speed Punch as `skills.json` has it: program index (raw[2]) 14, chain list s27 = [14],
/// a short-range punch (s63 = 13), cost 2, unlimited uses.
fn punch() -> SkillDef {
    k::def(PUNCH, Capsule::Attack, 2, &[(2, 14), (27, 14), (k::f::MOTION, 13), (k::P, 130), (k::P + 1, 8), (k::P + 2, 30), (k::f::STRENGTH, 1), (k::f::FAMILY, 1)])
}

/// Rapid Cannon: program index 48, chain list [48], a shell.
fn cannon() -> SkillDef {
    k::def(CANNON, Capsule::Attack, 2, &[(2, 48), (27, 48), (k::P, -80), (k::P + 1, 120), (k::P + 4, 5), (k::f::STRENGTH, 1)])
}

/// An unrelated punch that is not in anybody's chain list.
fn other() -> SkillDef {
    k::def(OTHER, Capsule::Attack, 2, &[(2, 10), (k::f::MOTION, 13), (k::P, 130), (k::P + 1, 8), (k::P + 2, 30), (k::f::STRENGTH, 1)])
}

fn row(clip: &str, dash: (i16, i16), fire: i16, hit_end: i16) -> ActionRow {
    ActionRow { anim_name: clip.into(), cast_sound: 214, charge_cycle: 0, dash_start: dash.0, dash_end: dash.1, fire_tick: fire, hit_window_end: hit_end }
}

fn win(from: i16, to: i16, at: i16) -> ChainWindow {
    ChainWindow { from, to, at }
}

/// The action-table rows of the data: row 14 (ATK_PUNCH, fire 20, window 17..24, start 24), rows
/// 301 / 302 (ATK_PUNCH_2: dash 5..7, fire 7, window 4..11; ATK_PUNCH_3: dash 4..6, fire 6, window
/// 3..10), row 48 (ATK_LASER: fire 13, window 8..17), rows 303 / 304 (ATK_LASER_2 / 3: fire 7,
/// window 4..11).
fn rows(c: &mut Combat) {
    c.set_action_row(PUNCH, Some(row("ATK_PUNCH", (10, 20), 20, 22)), None);
    c.set_action_row(CANNON, Some(row("ATK_LASER", (0, 0), 13, 0)), None);
    c.set_chain_rows(
        HashMap::from([(PUNCH, win(17, 24, 24)), (CANNON, win(8, 17, 17))]),
        HashMap::from([
            (PUNCH, vec![(row("ATK_PUNCH_2", (5, 7), 7, 9), win(4, 11, 11)), (row("ATK_PUNCH_3", (4, 6), 6, 8), win(3, 10, 10))]),
            (CANNON, vec![(row("ATK_LASER_2", (0, 0), 7, 0), win(4, 11, 11)), (row("ATK_LASER_3", (0, 0), 7, 0), win(4, 11, 11))]),
        ]),
    );
    // Each cast clip holds its caster for its own length (30 fps frames; the real ones are longer
    // than the window ends).
    c.set_clip_ticks([HashMap::from([
        ("ATK_PUNCH".to_string(), 60),
        ("ATK_PUNCH_2".to_string(), 50),
        ("ATK_PUNCH_3".to_string(), 50),
        ("ATK_LASER".to_string(), 68),
        ("ATK_LASER_2".to_string(), 75),
        ("ATK_LASER_3".to_string(), 75),
    ])]);
}

/// A match with the combo skills on the buttons: player 0 has Level 8 / Aura 8.
fn world(extra: Vec<SkillDef>) -> (Combat, [Player; 2], Collision) {
    let mut sk = vec![punch(), cannon(), other()];
    sk.extend(extra);
    let (mut c, p, col) = k::setup(sk, 100.0);
    rows(&mut c);
    c.fighters[0].level = 8;
    c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
    c.fighters[0].slots = [Some(PUNCH), Some(CANNON), Some(OTHER), Some(PUNCH)];
    (c, p, col)
}

/// Runs `ticks` sim ticks; tick 0 presses `button` (the first cast), and every tick in `presses`
/// presses `chain_button` (the button is released the tick after, so each is an edge). Returns the
/// tick numbers (counted from the first press) on which the cast restarted (`cast_seq` changed).
fn drive(c: &mut Combat, p: &mut [Player; 2], col: &Collision, button: usize, presses: &[(usize, usize)], ticks: usize) -> Vec<usize> {
    let mut starts = Vec::new();
    for n in 0..ticks {
        let seq = c.fighters[0].cast_seq;
        let mut b = [0u8; 2];
        if n == 0 {
            b[0] |= 1 << button;
        }
        for &(at, who) in presses {
            if n == at {
                b[0] |= 1 << who;
            }
        }
        c.step(p, b, col);
        if n > 0 && c.fighters[0].cast_seq != seq {
            starts.push(n);
        }
    }
    starts
}

fn clip(c: &Combat) -> String {
    c.cast_row(0, c.fighters[0].cast_skill).map(|r| r.anim_name.clone()).unwrap_or_default()
}

#[test]
fn the_chain_lists_and_rows_are_the_programs_data() {
    // `FUN_14024fb90` reads the pressed skill's program index in s27..s36 of the casting skill; the
    // rows come from s14..s17 (`FUN_14025a3e0`). Skipped when the converted cache is absent.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/skills.json");
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let raw = |s: &serde_json::Value| s["raw"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect::<Vec<_>>();
    let mut chaining = Vec::new();
    for s in crate::first_skill_records(&json) {
        let r = raw(s);
        if r[27..37].iter().any(|&x| x != 0) {
            chaining.push((s["id"].as_i64().unwrap(), r[2], r[27..37].to_vec(), r[14..18].to_vec()));
        }
    }
    // High-speed Punch, Rapid Cannon and Trench Mortar list themselves; the three Splash records
    // (enemy-only, ids 711..713) list Rapid Cannon (program index 48).
    let own = |id: i64| chaining.iter().find(|c| c.0 == id).unwrap_or_else(|| panic!("{id} chains: {chaining:?}")).clone();
    for (id, rows) in [(185, [301, 302]), (75, [303, 304]), (72, [303, 304])] {
        let (_, pidx, list, got) = own(id);
        assert_eq!(list[0], pidx, "skill {id} lists its own program index");
        assert!(list[1..].iter().all(|&x| x == 0));
        assert_eq!(&got[..2], &rows, "skill {id}: s14, s15");
        assert_eq!(&got[2..], &[0, 0], "skill {id}: two rows, so the cycle has period 2");
    }
    assert_eq!(chaining.iter().map(|c| c.0).collect::<Vec<_>>().len(), 6, "{chaining:?}");
    for id in [711, 712, 713] {
        assert_eq!(own(id).2[0], 48, "Splash {id} lists Rapid Cannon");
    }
    // The combo rows as exported (a cache made before the `chain` export has none: skipped).
    let chained: Vec<_> = crate::first_skill_records(&json).filter(|s| s["action"]["chain"].as_array().is_some_and(|a| !a.is_empty())).collect();
    if chained.is_empty() {
        return;
    }
    assert_eq!(chained.len(), 3);
    for s in chained {
        let g = raw(&s["action"]["ground"]);
        for r in s["action"]["chain"].as_array().unwrap() {
            let w = raw(r);
            // w48 (the combo offset) is 0 on every combo row, so it never shifts a fire tick.
            assert_eq!(w[48], 0, "{}: row {} has w48 = 0", s["name"], r["row"]);
            assert_eq!(w[46], w[47], "the window closes on the very frame the chain starts");
            assert_eq!(w[1], 1, "root motion flag");
        }
        assert_eq!(g[46], g[47]);
        let first = &s["action"]["chain"][0];
        assert!(["ATK_PUNCH_2", "ATK_LASER_2"].contains(&first["anim_name"].as_str().unwrap()));
        assert!(["ATK_PUNCH_3", "ATK_LASER_3"].contains(&s["action"]["chain"][1]["anim_name"].as_str().unwrap()));
    }
}

#[test]
fn a_press_in_the_window_chains_exactly_at_frame_w47() {
    // High-speed Punch hit 1: window frames 17..24, starts at 24. A press at frame 17 (tick 34) is
    // buffered; the chained cast starts at frame 24 = tick 48, not before and not after.
    let (mut c, mut p, col) = world(vec![]);
    let starts = drive(&mut c, &mut p, &col, 0, &[(34, 0)], 60);
    assert_eq!(starts, vec![48], "frame 24 x 2 sim ticks");
    assert_eq!(c.fighters[0].chain_count, 1);
    assert_eq!(clip(&c), "ATK_PUNCH_2", "s14 for hit 1");
    assert_eq!(c.fighters[0].action, Action::Lunge, "a fresh short-range cast");
}

#[test]
fn the_chain_runs_s14_s15_s14_cyclically_with_each_rows_own_window() {
    // Hit 2 fires at frame 7, window 4..11, starts at 11; hit 3 fires at 6, window 3..10, starts at
    // 10 (rows 301 / 302), then row 301 again.
    let (mut c, mut p, col) = world(vec![]);
    // Cost 1 each so five payments fit in the 8 Aura.
    c.skills.iter_mut().find(|d| d.id == PUNCH).unwrap().cost = 1;
    // 48: hit 1 (ATK_PUNCH_2) starts. Its window is frames 4..11 (ticks 56..70 from there).
    // Press at frame 4 of each cast (the earliest buffered frame).
    let presses = [(34, 0), (48 + 8, 0), (48 + 22 + 6, 0), (48 + 22 + 20 + 8, 0)];
    let starts = drive(&mut c, &mut p, &col, 0, &presses, 190);
    assert_eq!(starts, vec![48, 48 + 22, 48 + 22 + 20, 48 + 22 + 20 + 22], "frames 24, 11, 10, 11");
    assert_eq!(c.fighters[0].chain_count, 4);
    assert_eq!(clip(&c), "ATK_PUNCH_3", "hit 4 = (4 - 1) % 2 = 1 -> s15");
    // The clips on the way.
    let (mut c, mut p, col) = world(vec![]);
    let mut seen = Vec::new();
    for n in 0..120usize {
        let mut b = [0u8; 2];
        b[0] = u8::from(n == 0) | u8::from(n == 34 || n == 56 || n == 78);
        c.step(&mut p, b, &col);
        let name = clip(&c);
        if seen.last() != Some(&name) {
            seen.push(name);
        }
    }
    assert_eq!(seen, vec!["ATK_PUNCH", "ATK_PUNCH_2", "ATK_PUNCH_3", "ATK_PUNCH_2"]);
}

#[test]
fn a_press_outside_the_window_chains_nothing() {
    // Frame 16 (tick 32) is one frame before the window: nothing is buffered, so frame 24 starts
    // nothing. A press at frame 25 (tick 50) is after the start frame: too late.
    for at in [32, 50, 20] {
        let (mut c, mut p, col) = world(vec![]);
        let starts = drive(&mut c, &mut p, &col, 0, &[(at, 0)], 100);
        assert!(starts.is_empty(), "press at tick {at}: {starts:?}");
        assert_eq!(c.fighters[0].chain_count, 0);
    }
    // The press on the first tick of the window is the earliest accepted; the last accepted frame
    // is w46 = 24 itself (tick 48): buffered and started in the same program tick.
    for at in [34, 48] {
        let (mut c, mut p, col) = world(vec![]);
        assert_eq!(drive(&mut c, &mut p, &col, 0, &[(at, 0)], 60), vec![48], "press at tick {at}");
    }
}

#[test]
fn the_gate_runs_once_per_program_tick() {
    // A press on the odd sim tick between two program ticks is seen by the next program tick: tick
    // 33 -> frame 17 (in), tick 31 -> frame 16 (out, seen at tick 32).
    let (mut c, mut p, col) = world(vec![]);
    assert_eq!(drive(&mut c, &mut p, &col, 0, &[(33, 0)], 60), vec![48]);
    let (mut c, mut p, col) = world(vec![]);
    assert!(drive(&mut c, &mut p, &col, 0, &[(31, 0)], 60).is_empty());
}

#[test]
fn only_a_skill_in_the_casting_skills_chain_list_chains() {
    // Another button's skill (not in s27..s36) in the window does nothing: a cast never accepts a
    // press (`FUN_14024fb90` returns 0 inside states 10..12).
    let (mut c, mut p, col) = world(vec![]);
    assert!(drive(&mut c, &mut p, &col, 0, &[(34, 2)], 100).is_empty());
    // Rapid Cannon's button does not chain into Punch either: its list is [48].
    let (mut c, mut p, col) = world(vec![]);
    assert!(drive(&mut c, &mut p, &col, 0, &[(34, 1)], 100).is_empty());
    // The same skill on a second button chains (the list holds the skill, not the button).
    let (mut c, mut p, col) = world(vec![]);
    assert_eq!(drive(&mut c, &mut p, &col, 0, &[(34, 3)], 60), vec![48]);
}

#[test]
fn one_button_is_buffered_a_later_press_of_another_is_ignored() {
    // Button 0 buffered at frame 17; button 3 (the same skill) pressed at frame 20 changes nothing:
    // the chain still starts at 24 and it starts on button 0 (`+0x61f4` holds one button).
    let (mut c, mut p, col) = world(vec![]);
    let starts = drive(&mut c, &mut p, &col, 0, &[(34, 0), (40, 3)], 60);
    assert_eq!(starts, vec![48]);
    assert_eq!(c.fighters[0].last_slot, 0);
}

#[test]
fn every_chained_hit_pays_its_aura_cost_again() {
    // `FUN_140258180` has no combo branch: each chained cast is an ordinary cast paid at its fire
    // (`FUN_1402626d0` -> `FUN_1402621f0`), cost 2 each.
    let (mut c, mut p, col) = world(vec![]);
    let presses = [(34, 0), (48 + 8, 0)];
    let starts = drive(&mut c, &mut p, &col, 0, &presses, 48 + 22 + 20);
    assert_eq!(starts.len(), 2);
    // Hit 1 fired at frame 20, hit 2 at 7 frames after its start, hit 3 at 6 frames after its start:
    // three payments by now.
    assert_eq!(c.fighters[0].aura_ticks, (8 - 3 * 2) * AURA_TICKS_PER_POINT, "3 hits x 2 Aura");
    assert_eq!(c.fighters[0].spent[0], 3, "three uses of the button's skill");
}

#[test]
fn a_chain_the_aura_cannot_pay_for_does_not_start() {
    // Aura 3: hit 1 pays 2 at frame 20 and leaves 1; at frame 24 the chained press finds the slot
    // refused (the usual Denied), starts nothing and the counter stays 0.
    let (mut c, mut p, col) = world(vec![]);
    c.fighters[0].aura_ticks = 3 * AURA_TICKS_PER_POINT;
    let starts = drive(&mut c, &mut p, &col, 0, &[(34, 0)], 60);
    assert!(starts.is_empty(), "{starts:?}");
    assert_eq!(c.fighters[0].chain_count, 0);
    assert!(c.events.iter().any(|e| e.1 == CombatEvent::Denied { who: 0, slot: 0 }));
}

#[test]
fn an_ordinary_cast_ends_the_chain() {
    // After a chain the counter is >0; pressing the skill again from idle is the non-combo branch of
    // the gate (counter 0), so it plays the ground row (s12) again.
    let (mut c, mut p, col) = world(vec![]);
    drive(&mut c, &mut p, &col, 0, &[(34, 0)], 60);
    assert_eq!(c.fighters[0].chain_count, 1);
    k::run(&mut c, &mut p, &col, 200);
    assert_eq!(c.fighters[0].action, Action::Ready);
    c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
    k::press(&mut c, &mut p, &col, 0, 0);
    assert_eq!(c.fighters[0].chain_count, 0);
    assert_eq!(clip(&c), "ATK_PUNCH");
}

#[test]
fn a_hit_during_a_chain_drops_the_buffered_press() {
    // The buffer dies with the cast (`FUN_140259d80` writes -1): hit between the press and frame 24.
    let (mut c, mut p, col) = world(vec![]);
    let mut starts = 0;
    for n in 0..80usize {
        let mut b = [0u8; 2];
        b[0] = u8::from(n == 0) | u8::from(n == 34);
        let seq = c.fighters[0].cast_seq;
        c.step(&mut p, b, &col);
        if n == 40 {
            c.fighters[0].action = Action::Hit;
            c.fighters[0].action_ticks = 20;
        }
        starts += usize::from(n > 0 && c.fighters[0].cast_seq != seq);
    }
    assert_eq!(starts, 0, "knocked out of the cast: no chain");
    assert_eq!(c.fighters[0].chain_count, 0);
}

#[test]
fn rapid_cannon_chains_with_shots_at_frame_17_then_11() {
    // Rapid Cannon fires at 13, window 8..17, chains at 17 (tick 34); later shots fire 7 after
    // their start with window 4..11 (chain at 11). ATK_LASER -> ATK_LASER_2 -> ATK_LASER_3 -> 2.
    let (mut c, mut p, col) = world(vec![]);
    let presses = [(16, 1), (34 + 8, 1), (34 + 22 + 8, 1)];
    let starts = drive(&mut c, &mut p, &col, 1, &presses, 130);
    assert_eq!(starts, vec![34, 34 + 22, 34 + 44], "frames 17, 11, 11");
    assert_eq!(clip(&c), "ATK_LASER_2", "hit 3 = (3 - 1) % 2 = 0 -> s14 again");
    assert_eq!(c.fighters[0].chain_count, 3);
}
