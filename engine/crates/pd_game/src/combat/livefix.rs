//! Live-evidence checks of the `livefix` lane (test-only): knock-down landing / get-up root motion
//! against the original's own trace (`%USERPROFILE%\.pdtools\moverec\takes_full_trace\`, P1 =
//! character index 1 = `pc01a`). Skipped when the converted cache is not present.

use super::testkit::{floor, TEST_RULES};
use super::*;
use std::path::{Path, PathBuf};

fn cache() -> PathBuf {
    std::env::var_os("PD_CACHE").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"))
}

/// A duel on a flat floor with the real tables and `chars` as both fighters' clip data.
fn world(chars: &str, dist: f32, victim_yaw: f32) -> Option<(Combat, [Player; 2])> {
    let cache = cache();
    let sj = cache.join("skills.json");
    if !sj.exists() || !cache.join("chars").join(format!("{chars}.rootmotion.json")).exists() {
        return None;
    }
    let ch = [chars.to_string()];
    let clips = crate::load_clip_ticks(&cache, &ch).into_iter().next().unwrap_or_default();
    let root = crate::load_root_curves(&cache, &ch).into_iter().next().unwrap_or_default();
    let mut c = Combat::new(crate::load_skills(&sj, false), vec![AURA_PARTICLE; 3]);
    c.set_rules(TEST_RULES);
    c.round_ticks = COUNTDOWN_TICKS;
    c.set_clip_ticks(vec![clips.clone(), clips]);
    c.set_root_curves([root.clone(), root]);
    c.set_action_rows(crate::load_action_rows(&sj));
    c.set_reaction_rows(crate::load_reaction_rows(&sj));
    let mut p = [Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, dist), victim_yaw)];
    for q in p.iter_mut() {
        q.locked = false;
    }
    for i in 0..2 {
        c.fighters[i].level = 12;
        c.fighters[i].aura_ticks = 12 * AURA_TICKS_PER_POINT;
        c.fighters[i].max_health = 40;
        c.fighters[i].health = 40;
    }
    Some((c, p))
}

/// Casts `skill` from button 0 and returns the victim's position every sim tick for `n` ticks, with
/// the victim's action.
fn knockdown_track(chars: &str, skill: i16, dist: f32, victim_yaw: f32, n: usize) -> Option<Vec<(Vec3, Action)>> {
    let (mut c, mut p) = world(chars, dist, victim_yaw)?;
    let col = floor();
    c.fighters[0].slots = [Some(skill), None, None, None];
    p[0].locked = true;
    p[0].target = Some(1);
    let mut out = Vec::new();
    for t in 0..n {
        c.capsules.clear();
        c.step(&mut p, [if t == 0 { 1 } else { 0 }, 0], &col);
        out.push((p[1].pos, c.fighters[1].action));
        if std::env::var_os("PD_LF_DEBUG").is_some() && c.fighters[1].action == Action::Knocked && t % 20 == 0 {
            let f = &c.fighters[1];
            eprintln!("t {t} front_set {} class {} flying {} vel {:?} yaw {:.2} root {:?} clip {}", f.front_set, f.land_class, f.flying, f.knock_vel, p[1].yaw, f.root, c.lying_clip(1));
        }
    }
    if std::env::var_os("PD_LF_DEBUG").is_some() {
        for (seq, e) in c.events.iter().filter(|(_, e)| !matches!(e, CombatEvent::AuraTick { .. })).take(40) {
            eprintln!("#{seq} {e:?}");
        }
        eprintln!("caster {:?} cast_seq {} hp {}", c.fighters[0].action, c.fighters[0].cast_seq, c.fighters[1].health);
    }
    if let Some(path) = std::env::var_os("PD_LF_OUT") {
        let mut s = String::from("tick,x,y,z,action\n");
        for (t, (v, a)) in out.iter().enumerate() {
            s += &format!("{t},{:.3},{:.3},{:.3},{a:?}\n", v.x, v.y, v.z);
        }
        let _ = std::fs::write(path, s);
    }
    Some(out)
}

/// The slide after a small landing, from the first tick on the ground to 25 program ticks later, along
/// the flight direction (z here) and the stick at which the body stopped.
fn slide_after_landing(t: &[(Vec3, Action)]) -> f32 {
    let k0 = t.iter().position(|(_, a)| *a == Action::Knocked).expect("knocked");
    let land = (k0 + 10..t.len()).find(|&i| t[i].0.y <= 0.001).expect("lands");
    t[land + 50].0.z - t[land].0.z
}

/// LIVE (`takes_full_trace`, P1 = `pc01a`; sheet in `engine/docs/MOVE-RECORDER.md` "livefix"): the
/// root tracks of the loaded character files move the original's body, with the file's own numbers.
/// Get-up: the program's net drift over the 30-frame DOWN_*_STAND clip was 6.48 u (DOWN_FRONT_STAND: Photon
/// Burst, Gravity Press, Fang of Tree, Massive Quake, Jack Pot) and 2.17 u (DOWN_BACK_STAND: Photon Wave,
/// Whirlwind, Twister, Fast Wave, Thor's Hammer, Arrow of Artemis, Rain of Rocks); `pc01a.rootmotion.json` has
/// 6.51 / 2.21. A large landing (Whirlwind, 87 u fall, BLOW_LAND_LF): -18.38 u along the facing in the take, -18.22
/// in the file. The P0 body (`pc00a`) got up with 3.38 u in four takes (Spore Schism, Fast Bomb, Massive Quake,
/// Jack Pot); the file has 3.24 / 3.20. Skipped when the converted cache is missing.
#[test]
fn root_tracks_match_the_live_takes_for_getups_and_the_large_landing() {
    let cache = cache();
    let mag = |chars: &str, clip: &str| -> Option<(f32, f32, f32)> {
        let ch = [chars.to_string()];
        let curves = crate::load_root_curves(&cache, &ch).into_iter().next()?;
        let e = *curves.get(clip)?.last()?;
        Some((e[0], e[1], (e[0] * e[0] + e[1] * e[1]).sqrt()))
    };
    let Some(front) = mag("pc01a", "DOWN_FRONT_STAND") else { return };
    let back = mag("pc01a", "DOWN_BACK_STAND").unwrap();
    let lf = mag("pc01a", "BLOW_LAND_LF").unwrap();
    assert!((front.2 - 6.48).abs() < 0.1, "DOWN_FRONT_STAND {front:?} vs live 6.48");
    assert!((back.2 - 2.17).abs() < 0.1, "DOWN_BACK_STAND {back:?} vs live 2.17");
    assert!((lf.1 + 18.38).abs() < 0.5, "BLOW_LAND_LF forward {lf:?} vs live -18.38 (Whirlwind)");
    let p0f = mag("pc00a", "DOWN_FRONT_STAND").unwrap();
    let p0b = mag("pc00a", "DOWN_BACK_STAND").unwrap();
    assert!((p0f.2 - 3.38).abs() < 0.25 && (p0b.2 - 3.38).abs() < 0.25, "pc00a get-ups {p0f:?} {p0b:?} vs live 3.38");
}

/// LIVE: an ordinary knock-down (Photon Burst, row 48, victim hit from the front: front set, BLOW_LAND_SF) slides 6.34 u
/// from the first tick on the ground (Photon Burst, Psycho Sniper, Mist Blade, Vibration Blaster, Judge of Anubis,
/// Illusion of Death all 6.31-6.35: the 0.85 per tick slide plus the SF root track). Ours must agree.
#[test]
fn the_front_set_slide_after_a_small_landing_matches_the_live_takes() {
    // The victim faces the attacker (yaw pi at +z facing the origin).
    let Some(t) = knockdown_track("pc01a", 64, 30.0, std::f32::consts::PI, 400) else { return };
    let net = slide_after_landing(&t);
    assert!((net - 6.34).abs() < 1.0, "front-set slide {net} vs live 6.34");
}

#[test]
fn lf_dump_photon_wave_knockdown() {
    let Some(t) = knockdown_track("pc01a", 63, 30.0, -std::f32::consts::FRAC_PI_4, 520) else { return };
    let landed = t.iter().position(|(_, a)| *a == Action::Knocked).unwrap_or(0);
    eprintln!("first Knocked tick {landed}; final {:?}; back-set slide {}", t.last().unwrap(), slide_after_landing(&t));
}

/// The step (0 = the press) on which `who`'s `skill` was fired, after pressing button 0 on step 0.
fn fire_step(c: &mut Combat, p: &mut [Player; 2], who: usize, skill: i16, n: usize, press: [u8; 2]) -> Option<usize> {
    let col = floor();
    let seen = c.events.last().map_or(0, |e| e.0);
    for t in 0..n {
        c.capsules.clear();
        c.step(p, if t == 0 { press } else { [0; 2] }, &col);
        if c.events.iter().any(|(s, e)| *s > seen && matches!(e, CombatEvent::SkillFired { who: w, skill: k } if *w == who && *k == skill)) {
            return Some(t);
        }
    }
    None
}

/// LIVE `240_Dash`: press tick 39, fire tick 40 (one program tick, 2 sim ticks); READ `FUN_14025a3e0` / `FUN_14025c720`.
#[test]
fn dash_fires_one_program_tick_after_the_press_live() {
    let Some((mut c, mut p)) = world("pc00a", 100.0, std::f32::consts::PI) else { return };
    c.fighters[0].slots = [Some(240), None, None, None];
    assert_eq!(fire_step(&mut c, &mut p, 0, 240, 20, [1, 0]), Some(2), "Dash: press step 0, fire step 2 (= 1 program tick)");
    // The clip-0 rows still fire in the press (Jump, id 239: LIVE press = fire).
    let Some((mut c, mut p)) = world("pc00a", 100.0, std::f32::consts::PI) else { return };
    c.fighters[0].slots = [Some(239), None, None, None];
    assert_eq!(fire_step(&mut c, &mut p, 0, 239, 20, [1, 0]), Some(0), "Jump fires in the press");
}

/// LIVE (`takes_full_trace` 208 Mind's Eye, 154 Guard of Water, 95 Auto Shell, 270 Guardian Angel): an automatic Shelter plays
/// no cast sound and no voice; a pressed guard (207 Iron Skin: sound 0.333 + voice 3.12 at the press) does. READ `FUN_14026f530`
/// -> `FUN_1402626d0` (clip started directly) against `FUN_14025a3e0` (`FUN_140276330` for row w3 / w4).
#[test]
fn an_automatic_shelter_casts_silently_and_a_pressed_guard_does_not() {
    let Some((mut c, mut p)) = world("pc01a", 35.0, std::f32::consts::PI) else { return };
    // P2 holds Mind's Eye (automatic), P1 shoots Blaster (id 80).
    c.fighters[0].slots = [Some(80), None, None, None];
    c.fighters[1].slots = [Some(208), None, None, None];
    p[0].locked = true;
    p[0].target = Some(1);
    let _ = fire_step(&mut c, &mut p, 0, 80, 200, [1, 0]);
    for _ in 0..40 {
        let col = floor();
        c.capsules.clear();
        c.step(&mut p, [0; 2], &col);
    }
    let f = &c.fighters[1];
    assert!(f.cast_seq > 0, "the automatic guard came up when Blaster arrived");
    assert_eq!(f.auto_cast_seq, f.cast_seq, "and its cast is marked silent (sfx.rs plays no cast sound / voice for it)");
    // Iron Skin on a button: pressed, so the cast sound plays (the marker stays behind).
    let Some((mut c, mut p)) = world("pc01a", 35.0, std::f32::consts::PI) else { return };
    c.fighters[0].slots = [Some(207), None, None, None];
    let _ = fire_step(&mut c, &mut p, 0, 207, 20, [1, 0]);
    let f = &c.fighters[0];
    assert!(f.cast_seq > 0 && f.auto_cast_seq != f.cast_seq, "a pressed guard is not silent");
}
