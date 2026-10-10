//! Per-skill audit probe (test-only): what OUR runtime does for every skill id.
//!
//! The measuring stick of `docs/audits/SKILL-BY-SKILL.md`. This module is the
//! "ours" half: for each skill of the versus roster (ids 1-340) plus the
//! built-in Aura Particle it drives the REAL game code -- `Combat::cast` /
//! `step` through the fire tick, the real `moves::*` flights stepped in a flat
//! test arena, the real guard / area / melee constructors, the real
//! `pick_reaction`, `sfx::skill_sounds`, `fx::SkillFx::name` -- and writes
//! what it observed as JSON. The "program" half (every value derived from the
//! game's own tables and the read rules in `docs/PROGRAM-RE.md`) and the
//! comparison live in `engine/re/skill_audit.py`, which runs this test.
//!
//! It changes no behaviour and writes nothing unless `PD_SKILL_AUDIT_OUT`
//! names a file:
//!
//! ```text
//! PD_SKILL_AUDIT_OUT=<file>.json cargo test -p pd_game skill_audit_dump -- --nocapture
//! python engine/re/skill_audit.py --ours <file>.json
//! ```
//!
//! Where a value has no single place in the code to read it, the probe
//! observes it (positions per program tick, health / Aura / status before and
//! after, events with their tick) instead of asking a field; where the probe
//! cannot set a situation up it says so (`"probe": "..."` strings) and the
//! Python side reports the cell as not probed rather than guessing.
//!
//! Arena: a flat floor, caster (slot 0) at the origin facing +Z, opponent
//! (slot 1) wherever the scenario needs it; no capsules; Level and Aura high
//! enough to pay; the countdown over. Sim ticks are 1/60 s; the program's
//! logic ticks are 1/30 s (two sim ticks).

use super::testkit::{floor, TEST_RULES};
use super::*;
use crate::sim::Match;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

fn r3(x: f32) -> f64 {
    ((x as f64) * 1000.0).round() / 1000.0
}

fn v3(v: Vec3) -> Value {
    json!([r3(v.x), r3(v.y), r3(v.z)])
}

fn cache_dir() -> PathBuf {
    std::env::var_os("PD_CACHE").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"))
}

/// Everything loaded once, the way `main.rs` loads it at startup.
struct Ctx {
    skills: Vec<SkillDef>,
    rows: HashMap<i16, (Option<ActionRow>, Option<ActionRow>)>,
    reactions: HashMap<i16, ReactionRow>,
    markers: HashMap<i16, (Vec<Vec3>, Vec<Vec3>)>,
    clips: HashMap<String, u16>,
    hands: HashMap<String, HandClip>,
    common_sounds: std::collections::HashSet<u32>,
    voice: HashMap<i16, i16>,
    cache: PathBuf,
}

impl Ctx {
    fn load() -> Option<Ctx> {
        let cache = cache_dir();
        let skills_json = cache.join("skills.json");
        if !skills_json.exists() {
            return None;
        }
        let skills = crate::load_skills(&skills_json, false);
        let clips = crate::load_clip_ticks(&cache, &["pc00a".to_string()]).into_iter().next().unwrap_or_default();
        let hands = crate::load_hand_paths(&cache, &["pc00a".to_string()]).into_iter().next().unwrap_or_default();
        let index: Value = std::fs::read_to_string(cache.join("sfx/index.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let common_sounds = index["banks"]["common"].as_object().into_iter().flatten().filter_map(|(k, _)| k.parse().ok()).collect();
        let sj: Value = serde_json::from_str(&std::fs::read_to_string(&skills_json).ok()?).ok()?;
        let group = |s: &Value| ["ground", "air"].iter().find_map(|k| s["action"][*k]["voice_group"].as_i64().filter(|&g| g > 0));
        let voice = sj["skills"].as_array().into_iter().flatten().filter_map(|s| Some((s["id"].as_i64()? as i16, group(s)? as i16))).collect();
        Some(Ctx {
            skills,
            rows: crate::load_action_rows(&skills_json),
            reactions: crate::load_reaction_rows(&skills_json),
            markers: crate::load_shot_markers(&cache),
            clips,
            hands,
            common_sounds,
            voice,
            cache,
        })
    }
}

/// One scenario's world.
struct World {
    c: Combat,
    p: [Player; 2],
    col: Collision,
    /// Steps taken (the press is step 0 of a cast).
    t: u32,
    /// The step the first press happened on (step 0 of a cast).
    press_t: u32,
    /// Leave the capsules on the field (the capsule-field probe lays its own).
    keep_capsules: bool,
    seen: u32,
    /// Events with the step they were logged on.
    log: Vec<(u32, CombatEvent)>,
}

impl World {
    fn new(ctx: &Ctx, dist: f32) -> World {
        let mut c = Combat::new(ctx.skills.clone(), vec![AURA_PARTICLE; 3]);
        c.set_rules(TEST_RULES);
        c.round_ticks = COUNTDOWN_TICKS;
        c.set_clip_ticks(vec![ctx.clips.clone(), ctx.clips.clone()]);
        c.set_action_rows(ctx.rows.clone());
        c.set_reaction_rows(ctx.reactions.clone());
        c.set_shot_markers(ctx.markers.clone());
        c.set_hand_paths(vec![ctx.hands.clone(), ctx.hands.clone()]);
        let p = [Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, dist), std::f32::consts::PI)];
        let mut w = World { c, p, col: floor(), t: 0, press_t: 0, keep_capsules: false, seen: 0, log: Vec::new() };
        for i in 0..2 {
            // `Player::new` starts locked on; the scenarios ask for it where they need it.
            w.p[i].locked = false;
            w.c.fighters[i].level = 20;
            w.c.fighters[i].aura_ticks = 20 * AURA_TICKS_PER_POINT;
            w.c.fighters[i].max_health = 20;
            w.c.fighters[i].health = 20;
        }
        w.seen = w.c.events.last().map_or(0, |e| e.0);
        w
    }

    fn step(&mut self, buttons: [u8; 2]) {
        if !self.keep_capsules {
            self.c.capsules.clear();
        }
        self.c.step(&mut self.p, buttons, &self.col);
        if !self.keep_capsules {
            self.c.capsules.clear();
        }
        self.t += 1;
        let seen = self.seen;
        for (seq, ev) in self.c.events.iter().filter(|(s, _)| *s > seen) {
            self.log.push((self.t, *ev));
            self.seen = self.seen.max(*seq);
        }
    }

    fn run(&mut self, n: u32) {
        for _ in 0..n {
            self.step([0; 2]);
        }
    }

    /// Player `who` presses button `slot` for one step (and lets go).
    fn press(&mut self, who: usize, slot: usize) {
        let mut b = [0u8; 2];
        b[who] = 1 << slot;
        self.step(b);
        if self.press_t == 0 {
            self.press_t = self.t;
        }
    }

    /// A step number relative to the press (the press itself is step 0).
    fn rel(&self, t: u32) -> u32 {
        t.saturating_sub(self.press_t)
    }

    /// Absolute step the caster's cast fired on.
    fn fired_at(&self) -> Option<u32> {
        self.log.iter().find(|(_, e)| matches!(e, CombatEvent::SkillFired { who: 0, .. })).map(|(t, _)| *t)
    }

    fn fired_rel(&self) -> Option<u32> {
        self.fired_at().map(|t| self.rel(t))
    }

    fn snap(&self, i: usize) -> Value {
        let f = &self.c.fighters[i];
        let st = &f.status;
        json!({
            "health": f.health, "level": f.level, "aura": f.aura(), "aura_ticks": f.aura_ticks,
            "slots": f.slots, "spent": f.spent, "used": f.used,
            "action": format!("{:?}", f.action), "action_ticks": f.action_ticks,
            "frozen": f.frozen_ticks, "poison": f.poison_ticks,
            "mods": st.mods.iter().map(|m| json!({"stat": format!("{:?}", m.stat), "amount": m.amount, "ticks": m.ticks})).collect::<Vec<_>>(),
            "bound": st.bound_ticks, "invisible": st.invisible_ticks, "inverted": st.inverted_ticks,
            "scrambled": st.scrambled_ticks, "immune": st.immune_ticks, "locked_slot": [st.locked_slot, st.locked_ticks],
            "hover": st.hover_ticks, "regen_every": st.regen_every, "still_immune": st.still_immune,
            "invuln": f.invuln_ticks, "reaction_row": f.reaction_row, "guard": f.guard.is_some(),
            "forced_lock": f.forced_lock.map(|(who, t)| json!([who, t])), "arsenal": f.arsenal.len(),
            "pos": v3(self.p[i].pos),
        })
    }
}

fn event_name(e: &CombatEvent) -> String {
    let s = format!("{e:?}");
    s.split([' ', '{', '(']).next().unwrap_or("").to_string()
}

fn events_json(w: &World) -> Value {
    Value::Array(w.log.iter().filter(|(_, e)| !matches!(e, CombatEvent::AuraTick { .. })).map(|(t, e)| json!([w.rel(*t), event_name(e)])).collect())
}

/// The caster's standard state: Level / Aura high enough to pay, full Health, the skill on
/// button 0, a few skills left to deal, one skill already used (for Relearn / Recall).
fn base(w: &mut World, def: &SkillDef) {
    let level = (def.cost.max(def.level_penalty).max(20)).min(99);
    let f = &mut w.c.fighters[0];
    f.level = level;
    f.aura_ticks = level * AURA_TICKS_PER_POINT;
    f.health = 20;
    f.slots = [Some(def.id), None, None, None];
    f.spent = [0; SLOTS];
    f.arsenal = vec![AURA_PARTICLE; 3];
    f.used = vec![65];
    // Recollection / Copy repeat the last attack used / taken.
    f.last_attack = Some(65);
    f.last_hurt_by = Some(65);
    // [Object] (Power, Psycho Kinesis) throws a small loose stage object within reach.
    if def.motion() == moves::motion::OBJECT && def.raw[43] == 0 {
        w.c.props = vec![crate::props::Prop::new(0, 2, 1, Vec3::new(15.0, 0.0, 25.0), Vec3::new(35.0, 20.0, 45.0), true, true)];
    }
}

/// Makes the skill's requirement (s38 / s39) true (`satisfy`) or false, on top of whatever the
/// caster's state is now.
fn requirement(w: &mut World, def: &SkillDef, satisfy: bool) {
    let n = def.requirement_amount;
    let level = w.c.fighters[0].level;
    let f = &mut w.c.fighters[0];
    match (def.requirement, satisfy) {
        (1, true) => f.health = n.clamp(1, 20),
        (1, false) => f.health = (n + 1).clamp(1, 20),
        (2, true) => f.health = 20,
        (2, false) => f.health = (n - 1).clamp(1, 20),
        (5, true) => f.arsenal.truncate(n.max(0) as usize),
        (5, false) => f.arsenal = vec![AURA_PARTICLE; (n + 1).max(1) as usize],
        (7, true) => f.aura_ticks = level * AURA_TICKS_PER_POINT,
        (7, false) => f.aura_ticks = (level - 1) * AURA_TICKS_PER_POINT + AURA_TICKS_PER_POINT / 2,
        (8, true) => {
            f.level = n.max(def.cost).max(def.level_penalty);
            f.aura_ticks = f.level * AURA_TICKS_PER_POINT;
        }
        (8, false) => {
            f.level = (n + 1).max(def.cost);
            f.aura_ticks = f.level * AURA_TICKS_PER_POINT;
        }
        (9, true) => {
            f.level = f.level.max(n);
            f.aura_ticks = f.aura_ticks.max(f.level.min(def.cost.max(1)) * AURA_TICKS_PER_POINT).min(f.level * AURA_TICKS_PER_POINT);
        }
        (9, false) => {
            f.level = (n - 1).max(0);
            f.aura_ticks = f.level * AURA_TICKS_PER_POINT;
        }
        _ => {}
    }
}

/// What the caster needs to be able to cast `def` (and, with `satisfy` false, the requirement made false).
fn prepare(w: &mut World, def: &SkillDef, satisfy: bool) {
    base(w, def);
    requirement(w, def, satisfy);
}

fn motion(def: &SkillDef) -> i16 {
    if def.raw[43] == 0 { def.motion() } else { -1 }
}

fn melee_like(def: &SkillDef) -> bool {
    matches!(moves::behaviour_of(def), Behaviour::Melee(_))
}

/// The distance the "hit" scenarios put the opponent at: where the program aims
/// (the band's own default distance) so a free-aimed shot meets them.
fn aim_distance(def: &SkillDef) -> f32 {
    if def.raw[43] == 0 && def.raw[field_band()] == 1 { 300.0 } else { 190.0 }
}

fn field_band() -> usize {
    pd_formats::skills::field::RANGE_BAND
}

/// Row used for this cast and the same row's data, for the JSON.
fn row_json(c: &Combat, id: i16, air: bool) -> Value {
    match c.action_row(id, air) {
        None => Value::Null,
        Some(r) => json!({
            "anim": r.anim_name, "cast_sound": r.cast_sound, "charge_cycle": r.charge_cycle,
            "dash": [r.dash_start, r.dash_end], "fire_tick": r.fire_tick, "hit_end": r.hit_window_end,
        }),
    }
}

// ---------------------------------------------------------------------------
// Static views of the real constructors.

fn static_view(ctx: &Ctx, def: &SkillDef) -> Value {
    let b = moves::behaviour_of(def);
    let (kind, detail): (String, Value) = match b {
        Behaviour::AuraParticle => ("AuraParticle".into(), Value::Null),
        Behaviour::Melee(m) => (
            "Melee".into(),
            json!({"shape": format!("{:?}", m.shape), "reach": r3(m.reach), "dash": m.dash, "turn": m.turn, "dash_turn_deg": r3(m.dash_turn().to_degrees())}),
        ),
        Behaviour::Shoot(k) => {
            let name = match k {
                ShotKindRef::Shell(_) => "Shell",
                ShotKindRef::Parabola(_) => "Parabola",
                ShotKindRef::Arc(_) => "Arc",
                ShotKindRef::Crawl(_) => "Crawl",
                ShotKindRef::Fall(_) => "Fall",
                ShotKindRef::Ricochet(_) => "Ricochet",
                ShotKindRef::Tornado(_) => "Tornado",
            };
            let dbg = match k {
                ShotKindRef::Shell(s) => format!("{s:?}"),
                ShotKindRef::Parabola(s) => format!("{s:?}"),
                ShotKindRef::Arc(s) => format!("{s:?}"),
                ShotKindRef::Crawl(s) => format!("{s:?}"),
                ShotKindRef::Fall(s) => format!("{s:?}"),
                ShotKindRef::Ricochet(s) => format!("{s:?}"),
                ShotKindRef::Tornado(s) => format!("{s:?}"),
            };
            let placeholder = matches!(k, ShotKindRef::Shell(s) if s == moves::shell::Shell::PLACEHOLDER);
            (format!("Shoot:{name}"), json!({"debug": dbg, "placeholder": placeholder}))
        }
        Behaviour::Place(a) => (
            "Place".into(),
            json!({
                "kind": format!("{:?}", a.kind), "place": format!("{:?}", a.place), "trigger": format!("{:?}", a.trigger),
                "warn": a.warn, "active": a.active, "radius": r3(a.radius), "height": r3(a.height),
                "pulses": a.pulses, "every": a.every, "cloud_height": r3(a.cloud_height), "follow": r3(a.follow),
                "drop_speed": r3(a.drop_speed), "from_nowhere": a.from_nowhere, "needs_open_sky": a.needs_open_sky,
                "ground_only": a.ground_only,
            }),
        ),
        Behaviour::Guard(g) => ("Guard".into(), json!({"kind": g.kind, "def": g.def, "ticks": g.ticks, "anti_pierce": g.anti_pierce, "field": r3(g.field), "add": g.add, "add_amount": g.add_amount, "short_range_only": g.short_range_only, "all_round": g.all_round()})),
        Behaviour::Effect(d) => (format!("Effect:{d:?}"), Value::Null),
        Behaviour::NotBuilt => ("NotBuilt".into(), Value::Null),
    };
    let (add_code, add_ticks) = def.add_effect();
    json!({
        "behaviour": kind, "detail": detail,
        "uses": def.uses(), "cost_per_use": def.cost_per_use(), "retained": def.retained(), "protected": def.protected(),
        "charged": def.charged(), "passive": def.passive(), "pierces": def.pierces(), "ignores_defence": def.ignores_defence(),
        "ignores_walls": def.ignores_walls(), "blast_radius": r3(def.blast_radius()), "add": [add_code, add_ticks],
        "cost": def.cost, "level_penalty": def.level_penalty, "health_cost": def.health_cost, "strength": def.strength,
        "defence": def.defence, "capsule": format!("{:?}", def.capsule),
        "row_ground": row_json(&scratch_combat(ctx), def.id, false), "row_air": row_json(&scratch_combat(ctx), def.id, true),
    })
}

use crate::moves::ShotKind as ShotKindRef;

fn scratch_combat(ctx: &Ctx) -> Combat {
    let mut c = Combat::new(ctx.skills.clone(), vec![]);
    c.set_action_rows(ctx.rows.clone());
    c
}

// ---------------------------------------------------------------------------
// Casting: timing, payment, uses, requirements.

/// Cast once from a clean world; returns it after the cast has run its course
/// plus what was seen. `locked`: the caster has the opponent locked (needed for
/// a melee dash); the opponent stands `dist` away in front.
fn cast_once(ctx: &Ctx, def: &SkillDef, dist: f32, locked: bool, air: bool, after: u32) -> (World, Value) {
    let mut w = World::new(ctx, dist);
    prepare(&mut w, def, true);
    w.p[0].locked = locked;
    w.p[0].target = locked.then_some(1);
    if air {
        w.p[0].grounded = false;
        w.c.airborne[0] = true;
    }
    if melee_like(def) {
        // The blow lands on nobody, so what it does (Add effects, Level / Aura drains) cannot hide the payment.
        w.c.fighters[1].invuln_ticks = 2000;
    }
    let before = w.snap(0);
    let before_v = w.snap(1);
    let seq0 = w.c.fighters[0].cast_seq;
    // A hold-to-charge skill keeps its button down through the row's wind-up and a
    // little more, then lets go (the shot leaves a fixed time after the release).
    let charge = def.charged();
    let charge_hold: u32 = if charge { w.c.action_row(def.id, air).map_or(0, |r| 2 * r.charge_cycle.max(0) as u32) + 10 } else { 0 };
    w.press(0, 0);
    let press_pos = w.p[0].pos;
    // A cast that starts no state (a row naming clip 0) leaves the caster free in the press itself.
    // (The Aura Particle starts no cast at all, so it bumps no `cast_seq`: it is free in the press as well.)
    let no_cast_state = matches!(moves::behaviour_of(def), Behaviour::AuraParticle);
    let mut ready_at = ((w.c.fighters[0].cast_seq != seq0 || no_cast_state) && w.c.fighters[0].action == Action::Ready).then_some(0);
    let mut fired_snap_before = None;
    let mut fired_snap_after = None;
    // An instant cast fires in the press step itself.
    if w.fired_at() == Some(w.t) {
        fired_snap_before = Some(before.clone());
        fired_snap_after = Some(w.snap(0));
    }
    let mut prev = w.snap(0);
    for k in 1..=(after + 400) {
        w.step(if charge && k < charge_hold { [1, 0] } else { [0; 2] });
        let cur = w.snap(0);
        if w.fired_at() == Some(w.t) && fired_snap_after.is_none() {
            fired_snap_before = Some(prev.clone());
            fired_snap_after = Some(cur.clone());
        }
        if w.c.fighters[0].action == Action::Ready && ready_at.is_none() {
            ready_at = Some(k);
        }
        prev = cur;
        if ready_at.is_some() && w.t > after {
            break;
        }
    }
    let started = w.c.fighters[0].cast_seq != seq0;
    let m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, dist)], w.c.clone());
    let clip = crate::cast_clip_now(&m, 0, |n| ctx.clips.contains_key(n));
    let info = json!({
        "accepted": started, "fired": w.fired_at().is_some(),
        "fire_step": w.fired_rel(),
        "hold_steps": ready_at, "release_step": charge.then_some(charge_hold),
        "before": before, "before_victim": before_v,
        "at_fire_before": fired_snap_before, "at_fire_after": fired_snap_after,
        "end": w.snap(0), "end_victim": w.snap(1),
        "clip": clip,
        "moved": v3(w.p[0].pos - press_pos),
        "events": events_json(&w),
    });
    (w, info)
}

fn cast_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let id = def.id;
    let dist = if melee_like(def) { 60.0 } else { aim_distance(def) };
    let (_, ground) = cast_once(ctx, def, dist, melee_like(def), false, 8);
    // Mid-air: only meaningful for s40 != 0 (the press is refused otherwise).
    let (_, air) = cast_once(ctx, def, dist, melee_like(def), true, 8);
    // Which row the code picks mid-air, whether or not the press is allowed.
    let probe = scratch_combat(ctx);
    let air_row = probe.action_row(id, true).map(|r| r.anim_name.clone());
    let ground_row = probe.action_row(id, false).map(|r| r.anim_name.clone());
    json!({
        "ground": ground, "air": air,
        "air_row_anim": air_row, "ground_row_anim": ground_row,
        "air_allowed": probe.cast_allowed(def.raw[40], crate::sim::BodyState::Air),
        "ground_allowed": probe.cast_allowed(def.raw[40], crate::sim::BodyState::Ground),
    })
}

/// Level gate and requirement gate: does the press go through when it should
/// and stay refused when it should.
fn gate_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let attempt = |setup: &dyn Fn(&mut World)| -> bool {
        let mut w = World::new(ctx, aim_distance(def));
        prepare(&mut w, def, true);
        w.p[0].locked = melee_like(def);
        w.p[0].target = melee_like(def).then_some(1);
        if melee_like(def) {
            w.p[1].pos = Vec3::new(0.0, 0.0, 60.0);
        }
        setup(&mut w);
        let seq0 = w.c.fighters[0].cast_seq;
        w.press(0, 0);
        w.c.fighters[0].cast_seq != seq0
    };
    let ok = attempt(&|_| {});
    let low_level = (def.level_penalty > 0).then(|| {
        attempt(&|w| {
            let f = &mut w.c.fighters[0];
            f.level = def.level_penalty - 1;
            f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT).max(def.cost * AURA_TICKS_PER_POINT);
        })
    });
    let low_aura = (def.cost > 0).then(|| {
        attempt(&|w| {
            w.c.fighters[0].aura_ticks = (def.cost - 1) * AURA_TICKS_PER_POINT;
        })
    });
    let req_violated = (def.requirement != 0).then(|| {
        let mut w = World::new(ctx, aim_distance(def));
        prepare(&mut w, def, false);
        w.p[0].locked = melee_like(def);
        w.p[0].target = melee_like(def).then_some(1);
        let seq0 = w.c.fighters[0].cast_seq;
        w.press(0, 0);
        w.c.fighters[0].cast_seq != seq0
    });
    json!({"ok": ok, "low_level": low_level, "low_aura": low_aura, "requirement_violated": req_violated})
}

/// How many casts it takes to use the skill up (cap 8 = unlimited).
fn uses_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let dist = if melee_like(def) { 60.0 } else { aim_distance(def) };
    let mut w = World::new(ctx, dist);
    prepare(&mut w, def, true);
    w.p[0].locked = melee_like(def);
    w.p[0].target = melee_like(def).then_some(1);
    let mut cost_series = Vec::new();
    let aura_before = w.c.fighters[0].aura_ticks as f64 / AURA_TICKS_PER_POINT as f64;
    for n in 1..=8u32 {
        {
            // Back to the arena's starting state (Level, Aura, Health, requirement met) but
            // keep how many times this button has been used.
            let spent = w.c.fighters[0].spent;
            let slot = w.c.fighters[0].slots[0];
            prepare(&mut w, def, true);
            let f = &mut w.c.fighters[0];
            f.spent = spent;
            f.slots[0] = slot;
            f.action = Action::Ready;
            f.action_ticks = 0;
        }
        w.p[1].pos = Vec3::new(0.0, 0.0, dist);
        w.c.fighters[1].health = 20;
        w.c.fighters[1].level = 3;
        w.c.fighters[1].aura_ticks = 2 * AURA_TICKS_PER_POINT;
        w.c.fighters[1].action = Action::Ready;
        w.c.fighters[1].invuln_ticks = 2000;
        w.c.round_over = None;
        w.c.fighters[0].guard = None;
        w.c.projectiles.clear();
        w.c.areas.clear();
        w.c.blasts.clear();
        let aura0 = w.c.fighters[0].aura_ticks;
        let seq0 = w.c.fighters[0].cast_seq;
        w.press(0, 0);
        let started = w.c.fighters[0].cast_seq != seq0;
        for _ in 0..300 {
            w.step([0; 2]);
            if w.c.fighters[0].action == Action::Ready {
                break;
            }
        }
        // Refill is paused for the whole cast, so what is missing when it ends is what it cost.
        cost_series.push(json!((aura0 - w.c.fighters[0].aura_ticks) as f64 / AURA_TICKS_PER_POINT as f64));
        // A mine takes its use when it is gone: take it away and let the sim notice.
        w.c.areas.clear();
        w.c.projectiles.clear();
        w.c.blasts.clear();
        w.step([0; 2]);
        if !started {
            return json!({"casts_until_used_up": null, "unlimited": null, "cost_series": cost_series, "refused": true, "aura_before": aura_before});
        }
        let still = w.c.fighters[0].slots[0] == Some(def.id);
        if !still {
            return json!({"casts_until_used_up": n, "unlimited": false, "cost_series": cost_series, "aura_before": aura_before});
        }
        w.c.fighters[0].guard = None;
    }
    json!({"casts_until_used_up": null, "unlimited": true, "cost_series": cost_series, "aura_before": aura_before})
}

// ---------------------------------------------------------------------------
// Shots in flight.

struct Track {
    /// Program position after each program tick, from the first one the probe saw.
    pts: Vec<Vec3>,
    /// The projectile's program age when first seen (1 = one step already taken since
    /// it was made, so `pts[0]` is the first step's end).
    age0: u16,
    ends: &'static str,
}

fn track_projectiles(w: &mut World, max_ticks: u32) -> Vec<Track> {
    let mut tracks: Vec<(u32, Track)> = Vec::new();
    let mut last_age: HashMap<u32, u16> = HashMap::new();
    for _ in 0..max_ticks {
        for pr in &w.c.projectiles {
            let id = pr.id;
            if !tracks.iter().any(|(i, _)| *i == id) {
                tracks.push((id, Track { pts: vec![pr.shot.pos], age0: pr.shot.age, ends: "life" }));
                last_age.insert(id, pr.shot.age);
            } else if last_age.get(&id) != Some(&pr.shot.age) {
                last_age.insert(id, pr.shot.age);
                tracks.iter_mut().find(|(i, _)| *i == id).unwrap().1.pts.push(pr.shot.pos);
            }
        }
        if !tracks.is_empty() && w.c.projectiles.is_empty() {
            break;
        }
        w.step([0; 2]);
    }
    tracks.into_iter().map(|(_, t)| t).collect()
}

fn tracks_json(tracks: &[Track], origin: Vec3) -> Value {
    Value::Array(
        tracks
            .iter()
            .map(|t| {
                let pts: Vec<Value> = t.pts.iter().take(220).map(|p| v3(*p - origin)).collect();
                json!({"steps": t.pts.len().saturating_sub(1), "age0": t.age0, "pts": pts, "end": t.ends})
            })
            .collect(),
    )
}

/// Flight probes of an attack that leaves as projectiles: free flight (nothing
/// near, nothing locked), a steering run (target pulled off to the side after
/// the launch), and a hit run (the opponent where the program aims).
fn flight_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let band_dist = aim_distance(def);
    // 1. free flight: opponent behind, unlocked.
    let free = {
        let mut w = World::new(ctx, -400.0);
        prepare(&mut w, def, true);
        w.press(0, 0);
        let mut fired = false;
        for _ in 0..200 {
            w.step([0; 2]);
            if w.fired_at().is_some() {
                fired = true;
                break;
            }
        }
        let spawn_at_fire = w.p[0].pos;
        let n = w.c.projectiles.len();
        // What the launch itself makes (before any program step): the marks the code
        // spawns at and each object's first position and heading.
        let launch0 = match moves::behaviour_of(def) {
            Behaviour::Shoot(kind) => {
                let marks = w.c.shot_marks(0, def.id, &w.p);
                let ctx2 = w.c.launch_ctx(0, 1, &w.p, &w.col, &marks);
                let shots = moves::launch_from(kind, def, &ctx2);
                json!({"marks": marks.iter().map(|m| v3(*m - spawn_at_fire)).collect::<Vec<_>>(), "spawn": v3(ctx2.spawn - spawn_at_fire), "target": ctx2.target.map(v3),
                    "shots": shots.iter().map(|s| json!({"pos": v3(s.pos - spawn_at_fire), "heading": v3(s.heading()), "life": s.life, "homing": r3(s.homing), "blast": r3(s.blast), "radius": r3(s.radius), "delay": s.delay, "through_walls": s.through_walls})).collect::<Vec<_>>()})
            }
            _ => Value::Null,
        };
        let tr = track_projectiles(&mut w, 900);
        json!({"fired": fired, "count_at_fire": n, "launch0": launch0, "tracks": tracks_json(&tr, spawn_at_fire), "areas": areas_json(&w)})
    };
    // 2. steering: locked, opponent in the band; after the launch, move them 40 degrees off the heading.
    let steer = {
        let mut w = World::new(ctx, band_dist);
        prepare(&mut w, def, true);
        w.p[0].locked = true;
        w.p[0].target = Some(1);
        w.press(0, 0);
        let mut fired = false;
        for _ in 0..200 {
            w.step([0; 2]);
            if w.fired_at().is_some() {
                fired = true;
                break;
            }
        }
        let origin = w.p[0].pos;
        let mut pts: Vec<Vec3> = Vec::new();
        let mut moved = false;
        let mut ages = 0u16;
        let mut ages0 = 0u16;
        let mut heading0 = Vec3::Z;
        for _ in 0..600 {
            if let Some(pr) = w.c.projectiles.first() {
                if !moved {
                    let h = pr.shot.heading();
                    heading0 = Vec3::new(h.x, 0.0, h.z).normalize_or(Vec3::Z);
                    let ang = 40f32.to_radians();
                    let dir = Vec3::new(heading0.x * ang.cos() + heading0.z * ang.sin(), 0.0, -heading0.x * ang.sin() + heading0.z * ang.cos());
                    w.p[1].pos = pr.shot.pos + dir * 150.0;
                    w.p[1].pos.y = 0.0;
                    moved = true;
                    pts.push(pr.shot.pos);
                    ages = pr.shot.age;
                    ages0 = ages;
                } else if pr.shot.age != ages {
                    ages = pr.shot.age;
                    pts.push(pr.shot.pos);
                }
            } else if moved {
                break;
            }
            w.step([0; 2]);
        }
        let trk = Track { pts, age0: ages0, ends: "end" };
        json!({"fired": fired, "heading0": v3(heading0), "tracks": tracks_json(std::slice::from_ref(&trk), origin)})
    };
    json!({"free": free, "steer": steer})
}

fn areas_json(w: &World) -> Value {
    Value::Array(
        w.c.areas
            .iter()
            .map(|a| {
                json!({
                    "kind": format!("{:?}", a.spec.kind), "pos": v3(a.pos - w.p[0].pos), "timer": a.timer, "pulses_left": a.pulses_left, "armed": a.armed,
                    "life": a.life, "warn": a.spec.warn, "active": a.spec.active, "radius": r3(a.spec.radius), "height": r3(a.spec.height),
                    "pulses": a.spec.pulses, "every": a.spec.every, "cloud": v3(a.cloud - w.p[0].pos),
                })
            })
            .collect(),
    )
}

/// Areas (holes, rain, mines, waves): set unlocked with the opponent standing at
/// the aim point; record where it was set, its timeline and when it hits.
fn area_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let band = aim_distance(def);
    let spot = if matches!(motion(def), 8 | 9) { 20.0 } else { band };
    let mut w = World::new(ctx, spot);
    prepare(&mut w, def, true);
    let victim0 = w.p[1].pos;
    w.press(0, 0);
    let mut fire_step = None;
    for _ in 0..200 {
        w.step([0; 2]);
        if w.fired_at().is_some() {
            fire_step = Some(w.rel(w.t));
            break;
        }
    }
    let set = areas_json(&w);
    let blasts_at_fire: Vec<Value> = w.c.blasts.iter().map(|b| json!({"radius": r3(b.radius), "area": b.area, "pos": v3(b.pos - w.p[0].pos)})).collect();
    let t0 = w.t;
    let h0 = (w.c.fighters[0].health, w.c.fighters[1].health);
    let mut first_hit = None;
    let mut gone_at = None;
    for _ in 0..900 {
        w.step([0; 2]);
        if first_hit.is_none() && w.log.iter().any(|(t, e)| *t == w.t && matches!(e, CombatEvent::Hit { .. } | CombatEvent::BlastStarted { .. })) {
            first_hit = Some(w.t - t0);
        }
        if gone_at.is_none() && w.c.areas.is_empty() && w.c.blasts.is_empty() {
            gone_at = Some(w.t - t0);
        }
        // a sensor mine waits for anyone: nothing to do; the loop just runs it out
        if gone_at.is_some() {
            break;
        }
    }
    let blast_events: Vec<Value> = w.log.iter().filter_map(|(t, e)| if let CombatEvent::BlastStarted { radius, area, .. } = e { Some(json!([t - t0, r3(*radius), area])) } else { None }).collect();
    json!({
        "fire_step": fire_step, "victim": v3(victim0), "set": set, "blasts_at_fire": blasts_at_fire,
        "first_hit_after_fire": first_hit, "gone_after_fire": gone_at, "blast_events": blast_events,
        "health_delta": [w.c.fighters[0].health - h0.0, w.c.fighters[1].health - h0.1],
        "events": events_json(&w),
    })
}

/// Melee: the opponent 60 u ahead, caster locked on; per-step caster travel and the first hit.
/// Two runs: the opponent hittable (the hit window, damage) and the opponent untouchable (the dash
/// as it is when nothing knocks the target away).
fn melee_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let run = |invulnerable: bool| -> Value {
        let mut w = World::new(ctx, 60.0);
        prepare(&mut w, def, true);
        w.c.fighters[1].level = 3;
        w.p[0].locked = true;
        w.p[0].target = Some(1);
        if w.c.only_hits_downed(def) {
            // downed-only (Pursuit): the opponent lies on the floor
            w.c.fighters[1].action = Action::Knocked;
            w.c.fighters[1].action_ticks = 400;
            w.c.fighters[1].lie_ticks = 300;
        }
        if invulnerable {
            w.c.fighters[1].invuln_ticks = 2000;
        }
        let h0 = w.c.fighters[1].health;
        w.press(0, 0);
        let mut z = vec![w.p[0].pos.z];
        let mut hit = None;
        for _ in 0..200 {
            w.step([0; 2]);
            z.push(w.p[0].pos.z);
            if hit.is_none() && w.log.iter().any(|(t, e)| *t == w.t && matches!(e, CombatEvent::Hit { attacker: 0, .. })) {
                hit = Some(w.rel(w.t));
            }
            if w.c.fighters[0].action == Action::Ready && w.t > 5 {
                break;
            }
        }
        let moving: Vec<usize> = (1..z.len()).filter(|&k| (z[k] - z[k - 1]).abs() > 1e-4).collect();
        json!({
            "z": z.iter().take(120).map(|v| r3(*v)).collect::<Vec<_>>(),
            "dash_first": moving.first(), "dash_last": moving.last(), "travel": r3(z.last().copied().unwrap_or(0.0)),
            "hit_step": hit, "fire_step": w.fired_rel(), "victim_damage": h0 - w.c.fighters[1].health,
            "events": events_json(&w),
        })
    };
    let mut hit = run(false);
    let free = run(true);
    hit["free"] = free;
    hit
}

// ---------------------------------------------------------------------------
// Hits, damage, reactions, add effects.

/// The opponent's hands the erase skills are tried against: attacks of cost 1 and 4, a
/// defence, an Aura Particle; and a special, an erase, a movement special, an attack.
const HAND_A: [Option<i16>; 4] = [Some(65), Some(80), Some(26), Some(AURA_PARTICLE)];
const HAND_B: [Option<i16>; 4] = [Some(179), Some(36), Some(239), Some(65)];

fn hit_probe(ctx: &Ctx, def: &SkillDef, hand: [Option<i16>; 4]) -> Value {
    // Locked on like a player in a match: shots home on the opponent. The opponent stands where the
    // program's band lets the shot reach: 190 u (Optical lasers and mortars, band 1: 400 u).
    let (dist, locked) = match (melee_like(def), motion(def)) {
        (true, _) => (60.0, true),
        (_, 8 | 9) => (20.0, false),
        (_, 10) => (60.0, true),
        (_, 5) => (100.0, true),
        _ if def.raw[43] == 0 && def.raw[field_band()] == 1 => (400.0, true),
        _ => (190.0, true),
    };
    let mut w = World::new(ctx, dist);
    prepare(&mut w, def, true);
    w.p[0].locked = locked;
    w.p[0].target = locked.then_some(1);
    if w.c.only_hits_downed(def) {
        w.c.fighters[1].action = Action::Knocked;
        w.c.fighters[1].action_ticks = 400;
        w.c.fighters[1].lie_ticks = 300;
    }
    // Give the opponent a hand (four skills of different colours and costs) so erase and
    // slot-reading skills have something to read.
    w.c.fighters[1].slots = hand;
    w.c.fighters[1].level = 7;
    // Full Aura (= Level): the refill cannot move it, so any change is the skill's.
    w.c.fighters[1].aura_ticks = 7 * AURA_TICKS_PER_POINT;
    // "Health increase = damage dealt" needs room to heal.
    if def.raw[pd_formats::skills::field::ADD_EFFECT] == 9 && def.capsule == Capsule::Attack {
        w.c.fighters[0].health = 12;
    }
    // What the skill's Add effect (s51) needs to be seen: room to heal, different Healths to average,
    // a hand to hand over, statuses to reset / reverse, a crystal to erase.
    let code = def.raw[pd_formats::skills::field::ADD_EFFECT];
    if def.capsule != Capsule::Attack {
        match code {
            43 | 46 => w.c.fighters[1].health = 12,
            50 => w.c.fighters[0].slots = [Some(def.id), Some(65), Some(26), Some(80)],
            17 | 39 | 51 => {
                let st = &mut w.c.fighters[1].status;
                st.add(Stat::Attack, 2, None);
                st.add(Stat::Speed, -10, None);
                st.add(Stat::Homing, 20, Some(600));
                if code == 51 {
                    let st0 = &mut w.c.fighters[0].status;
                    st0.add(Stat::Attack, 3, None);
                }
            }
            _ => {}
        }
        if code == 38 || code == 39 {
            w.c.crystal = Some(Crystal { id: 1, owner: 1, skill: 105, pos: Vec3::new(0.0, 0.0, 50.0), age: 0 });
        }
    }
    let before0 = w.snap(0);
    let before1 = w.snap(1);
    // Anything about the opponent that an effect can change (not the Aura refill: whole points only).
    let sig = |w: &World| {
        let f = &w.c.fighters[1];
        format!("{} {} {} {} {} {} {:?} {} {} {:?} {} {}", f.health, f.level, f.aura(), f.frozen_ticks > 0, f.poison_ticks > 0, f.status.bound_ticks > 0, f.status.mods.iter().map(|m| (m.amount, m.ticks.is_some())).collect::<Vec<_>>(), f.status.scrambled_ticks > 0, f.status.inverted_ticks > 0, f.slots, f.status.locked_ticks > 0, f.forced_lock.is_some())
    };
    let sig0 = sig(&w);
    let mut effect_step = None;
    let mut at_effect = Value::Null;
    w.press(0, 0);
    let mut first_hit = None;
    let t0 = w.t;
    // The instant effects (motion 4) land s65 program ticks after their fire tick: wait for them.
    let wait = if def.raw[43] == 0 && def.motion() == 4 { 220 } else { 0 };
    for _ in 0..700 {
        w.step([0; 2]);
        if effect_step.is_none() && sig(&w) != sig0 {
            effect_step = Some(w.rel(w.t));
            at_effect = json!([w.snap(0), w.snap(1)]);
        }
        if first_hit.is_none() && w.log.iter().any(|(t, e)| *t == w.t && matches!(e, CombatEvent::Hit { attacker: 0, victim: 1, .. })) {
            first_hit = Some(w.rel(w.t));
            w.run(30);
            break;
        }
        if w.c.fighters[0].action == Action::Ready && w.c.projectiles.is_empty() && w.c.areas.is_empty() && w.c.blasts.is_empty() && w.t > t0 + 6 + wait {
            break;
        }
    }
    let blasts: Vec<Value> = w.log.iter().filter_map(|(t, e)| if let CombatEvent::BlastStarted { radius, area, .. } = e { Some(json!([t - t0, r3(*radius), area])) } else { None }).collect();
    json!({
        "dist": dist, "first_hit_step": first_hit, "effect_step": effect_step, "at_effect": at_effect, "after_step": w.rel(w.t), "before": [before0, before1], "after": [w.snap(0), w.snap(1)],
        "crystal_after": w.c.crystal.is_some(), "victim_spawn": v3(w.p[1].spawn), "caster_spawn": v3(w.p[0].spawn),
        "blasts": blasts, "victim_hit_dir": format!("{:?}", w.c.fighters[1].hit_dir),
        "events": events_json(&w),
    })
}

/// STR through the real `damage_to` in two fixed situations (S_a: attacker 17 Health, target 12;
/// S_b the other way round). The rest is fixed and told to the Python side in `state`: attacker
/// Level 7 / Aura 7, one button full, two attack skills and one defence skill already used; target
/// Level 5, Aura 4.5, two buttons full; two Aura Particle capsules lying on the field.
/// Rule 26 (STR of the equipped skills of a kind) is also probed per skill: what each attack skill
/// adds when it sits on a second button. Rule 28 (random) is drawn many times.
/// Then the real `pick_reaction` for a fresh victim at 1 / 3 / 5 damage and in the air.
fn str_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let scenario = |ha: i32, hb: i32, extra: Option<i16>| -> (World, i32) {
        let mut w = World::new(ctx, 100.0);
        {
            let a = &mut w.c.fighters[0];
            a.health = ha;
            a.level = 7;
            a.aura_ticks = 7 * AURA_TICKS_PER_POINT;
            a.slots = [Some(def.id), extra, None, None];
            a.used = vec![1, 26, 1];
        }
        {
            let t = &mut w.c.fighters[1];
            t.health = hb;
            t.level = 5;
            t.aura_ticks = 4 * AURA_TICKS_PER_POINT + AURA_TICKS_PER_POINT / 2;
            t.slots = [Some(1), Some(26), None, None];
        }
        for k in 0..2 {
            w.c.capsules.push(CapsuleItem { id: 900 + k, owner: 0, skill: AURA_PARTICLE, pos: Vec3::new(30.0 * k as f32, 0.0, 30.0), appear: 0 });
        }
        let d = w.c.damage_to(0, 1, def.id);
        (w, d)
    };
    let mut out = Vec::new();
    for (ha, hb) in [(17, 12), (12, 17)] {
        let (_, d) = scenario(ha, hb, None);
        out.push(json!({"attacker_health": ha, "target_health": hb, "str": d}));
    }
    // Random STR: many draws on one world.
    let mut draws = Vec::new();
    if def.capsule == Capsule::Attack && def.strength_rule == str_rule::RANDOM {
        let (mut w, _) = scenario(17, 12, None);
        for _ in 0..60 {
            draws.push(w.c.damage_to(0, 1, def.id));
        }
    }
    // What every other attack skill adds to this one on a second button (rule 26 only).
    let mut bundle = serde_json::Map::new();
    if def.capsule == Capsule::Attack && def.strength_rule == str_rule::BUNDLED {
        let (_, base) = scenario(17, 12, None);
        for x in ctx.skills.iter().filter(|d| (1..=340).contains(&d.id) && d.id != def.id) {
            let (_, with) = scenario(17, 12, Some(x.id));
            if with != base {
                bundle.insert(x.id.to_string(), json!(with - base));
            }
        }
    }
    let pick = |dmg: i32, air: bool| -> Value {
        let mut w2 = World::new(ctx, 100.0);
        w2.c.airborne[1] = air;
        let (row, total) = w2.c.pick_reaction(1, def, dmg);
        json!({"row": row, "total": total, "kind": w2.c.reaction_kind_of(row), "knockdown": w2.c.reaction_is_knockdown(row), "pinned": w2.c.reaction_is_pinned(row)})
    };
    json!({
        "damage": out, "random_draws": draws, "bundle": bundle,
        "state": {"attacker": {"level": 7, "aura": 7, "empty": 3, "used_attack": 2, "used_defence": 1}, "target": {"level": 5, "aura": 4.5, "empty": 2}, "aura_capsules": 2},
        "react_light": pick(1, false), "react_medium": pick(3, false), "react_heavy": pick(5, false), "react_air": pick(1, true),
    })
}

/// The range band: the real `Launch::aim` for an opponent at several distances,
/// and with nobody locked.
fn band_probe(def: &SkillDef) -> Value {
    let spawn = Vec3::new(0.0, CHEST, 0.0);
    let mk = |target: Option<f32>| -> (Vec3, bool) {
        let col = floor();
        let l = Launch { spawn, feet: Vec3::ZERO, pitch: 0.0, facing: Vec3::Z, target: target.map(|d| body_centre(Vec3::new(0.0, 0.0, d))), index: 0, col: Some(&col), marks: &[] };
        l.aim(def, false)
    };
    let mut rows = Vec::new();
    for d in [40.0f32, 150.0, 450.0, 800.0] {
        let (p, kept) = mk(Some(d));
        rows.push(json!({"target_at": d, "aim_dist": r3((p - spawn).length()), "kept": kept}));
    }
    let (p, kept) = mk(None);
    rows.push(json!({"target_at": null, "aim_dist": r3((p - spawn).length()), "kept": kept}));
    // `band()` itself, with the target behind (negative) to see the clamp from the near side.
    Value::Array(rows)
}

// ---------------------------------------------------------------------------
// Defence guards, status / special self effects, environment crystals.

fn guard_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let mut w = World::new(ctx, 120.0);
    prepare(&mut w, def, true);
    w.press(0, 0);
    let mut fire = None;
    for _ in 0..200 {
        w.step([0; 2]);
        if w.fired_at().is_some() {
            fire = Some(w.rel(w.t));
            break;
        }
    }
    let g = w.c.fighters[0].guard.map(|g| {
        json!({
            "skill": g.skill, "kind": g.guard.kind, "def": g.guard.def, "ticks": g.ticks, "anti_pierce": g.guard.anti_pierce,
            "field": r3(g.guard.field), "add": g.guard.add, "add_amount": g.guard.add_amount, "short_range_only": g.guard.short_range_only,
            "all_round": g.guard.all_round(), "button": g.button,
        })
    });
    let t0 = w.t;
    let mut up_for = None;
    for _ in 0..600 {
        if w.c.fighters[0].guard.is_none() {
            up_for = Some(w.t - t0);
            break;
        }
        w.step([0; 2]);
    }
    // Used before twice: DEF = s47 + s48 x uses.
    let mut w2 = World::new(ctx, 120.0);
    prepare(&mut w2, def, true);
    w2.c.fighters[0].spent[0] = 2;
    w2.press(0, 0);
    for _ in 0..200 {
        w2.step([0; 2]);
        if w2.fired_at().is_some() {
            break;
        }
    }
    let def_after_two = w2.c.fighters[0].guard.map(|g| g.guard.def);
    json!({"fire_step": fire, "guard_at_fire": g, "up_for_after_fire": up_for, "def_after_two_uses": def_after_two, "events": events_json(&w)})
}

/// Self / targeted effects of status and special skills (and the erase ones that act on the
/// caster): the whole before / after of both fighters.
fn effect_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let mut w = World::new(ctx, 100.0);
    base(&mut w, def);
    {
        let f = &mut w.c.fighters[0];
        f.level = 10.max(def.cost.min(99));
        f.aura_ticks = 4.max(def.cost.min(99)) * AURA_TICKS_PER_POINT;
        f.health = 10;
        f.slots = [Some(def.id), Some(1), Some(26), None];
        f.used = vec![1, 65];
        f.arsenal = vec![1, 26, 65, 179, 3, 4];
    }
    // Statuses to reset (kind 8) or to copy from the target (kind 18).
    let kinds = if def.raw[43] == 4 { [def.raw[45], def.raw[46], def.raw[47], def.raw[48]] } else { [0; 4] };
    // Bravery Decision (kind 22) removes the Aura Particles from the undealt deck: give it some.
    if kinds.contains(&22) {
        w.c.fighters[0].arsenal = vec![1, AURA_PARTICLE, 26, AURA_PARTICLE, AURA_PARTICLE, 65, 179, AURA_PARTICLE, 3, 4];
    }
    let arsenal_before = w.c.fighters[0].arsenal.clone();
    if kinds.contains(&8) {
        let st = &mut w.c.fighters[0].status;
        st.add(Stat::Attack, 2, None);
        st.add(Stat::Speed, 30, Some(600));
        st.bound_ticks = 100;
    }
    if kinds.contains(&18) {
        let st = &mut w.c.fighters[1].status;
        st.add(Stat::Attack, 4, None);
        st.add(Stat::Speed, 30, Some(600));
        st.add(Stat::Homing, -10, None);
    }
    // The skill's own requirement wins over the arena's numbers (Level Boost wants a low Level ...).
    requirement(&mut w, def, true);
    {
        let t = &mut w.c.fighters[1];
        t.level = 8;
        t.aura_ticks = 5 * AURA_TICKS_PER_POINT;
        t.health = 14;
        t.slots = [Some(1), Some(26), Some(65), Some(179)];
    }
    let locked = melee_like(def) || def.raw[44] == 1;
    w.p[0].locked = locked;
    w.p[0].target = locked.then_some(1);
    let b = [w.snap(0), w.snap(1)];
    let seq0 = w.c.fighters[0].cast_seq;
    w.press(0, 0);
    let started = w.c.fighters[0].cast_seq != seq0;
    let mut fire = None;
    for _ in 0..200 {
        // The effect has landed once the cast fired, or (the Aura Particle) logged its status effect.
        if w.fired_at().is_some() || w.log.iter().any(|(_, e)| matches!(e, CombatEvent::StatusEffect { .. })) {
            fire = Some(w.rel(w.t));
            break;
        }
        w.step([0; 2]);
    }
    let at_fire = [w.snap(0), w.snap(1)];
    let arsenal_at_fire = w.c.fighters[0].arsenal.clone();
    // Let it settle (instants land a few ticks after the fire tick).
    w.run(120);
    let after = [w.snap(0), w.snap(1)];
    // Timed effects: how long they last.
    let mut last = None;
    for k in 0..4000u32 {
        let any = {
            let s = &w.c.fighters[0].status;
            !s.mods.is_empty() || s.invisible_ticks > 0 || s.immune_ticks > 0 || s.hover_ticks > 0 || s.bound_ticks > 0
        };
        if !any {
            last = Some(k);
            break;
        }
        w.step([0; 2]);
    }
    // The sounds the events ask for (the Aura Particle's use: what `sfx::cues_for_event` plays).
    let sfx_ids: Vec<u32> = w
        .log
        .iter()
        // (The Aura regeneration tick over the settle run is not part of the use.)
        .filter(|(_, e)| !matches!(e, CombatEvent::AuraTick { .. }))
        .flat_map(|(_, e)| crate::sfx::cues_for_event(e, |_| Default::default(), |_| (0, 0)))
        .filter_map(|c| if let crate::sfx::Cue::Sfx(id, _) = c { Some(id) } else { None })
        .collect();
    json!({
        "started": started, "fire_step": fire, "before": b, "at_fire": at_fire, "after": after, "caster_effect_ends_after": last,
        "arsenal_before": arsenal_before, "arsenal_at_fire": arsenal_at_fire,
        "events": events_json(&w), "sfx_ids": if matches!(moves::behaviour_of(def), Behaviour::AuraParticle) { json!(sfx_ids) } else { Value::Null },
    })
}

fn env_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let mut w = World::new(ctx, 120.0);
    prepare(&mut w, def, true);
    w.press(0, 0);
    let mut fire = None;
    for _ in 0..200 {
        w.step([0; 2]);
        if w.fired_at().is_some() {
            fire = Some(w.rel(w.t));
            break;
        }
    }
    let statuses: BTreeMap<i16, i32> = (0..60).filter_map(|id| {
        let v = w.c.env_status(id);
        (v != 0).then_some((id, v))
    }).collect();
    let crystal = w.c.crystal.as_ref().map(|c| json!({"skill": c.skill, "owner": c.owner}));
    let mut ended = None;
    let t0 = w.t;
    for _ in 0..4200 {
        w.step([0; 2]);
        w.c.fighters[0].health = 20;
        if w.c.crystal.is_none() {
            ended = Some(w.t - t0);
            break;
        }
    }
    json!({
        "fire_step": fire, "statuses": statuses.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
        "crystal": crystal, "ended_after": ended,
        "capture_lock": crate::moves::environment::capture_lock_ticks(def), "lifetime": crate::moves::environment::lifetime_ticks(def),
    })
}

/// Erase-the-capsules skills (s43 = 1): lay capsules on the field and see which ones the cast takes.
/// Scenario `touched_skill`: the capsule within reach is a skill capsule (else an Aura Particle).
fn capsule_probe(ctx: &Ctx, def: &SkillDef, touched_skill: bool) -> Value {
    let mut w = World::new(ctx, 100.0);
    base(&mut w, def);
    w.keep_capsules = true;
    for f in w.c.fighters.iter_mut() {
        f.arsenal = vec![1, 26, 65, 80, 179, 36];
    }
    let cap = |id: u32, owner: usize, skill: i16, x: f32, z: f32| CapsuleItem { id, owner, skill, pos: Vec3::new(x, 0.0, z), appear: 0 };
    let (near, far) = if touched_skill { (65, AURA_PARTICLE) } else { (AURA_PARTICLE, 65) };
    w.c.capsules = vec![
        cap(1, 0, near, 5.0, 0.0),
        cap(2, 0, far, 40.0, 0.0),
        cap(3, 0, 26, 0.0, 40.0),
        cap(4, 1, AURA_PARTICLE, 0.0, 90.0),
        cap(5, 1, 80, 20.0, 100.0),
        cap(6, 1, AURA_PARTICLE, -20.0, 100.0),
    ];
    let before: Vec<Value> = w.c.capsules.iter().map(|c| json!([c.id, c.owner, c.skill])).collect();
    let arsenal_before: Vec<Value> = w.c.fighters.iter().take(2).map(|f| json!(f.arsenal)).collect();
    let seq0 = w.c.fighters[0].cast_seq;
    // The press would capture the capsule it stands at: use the skill directly.
    w.c.cast(0, 0, &mut w.p);
    let started = w.c.fighters[0].cast_seq != seq0;
    w.press_t = w.t;
    for _ in 0..200 {
        w.step([0; 2]);
        if w.fired_at().is_some() {
            break;
        }
    }
    w.run(10);
    json!({
        "started": started, "touched_skill": touched_skill, "before": before,
        "arsenal_before": arsenal_before, "arsenal_after": w.c.fighters.iter().take(2).map(|f| json!(f.arsenal)).collect::<Vec<_>>(),
        "after": w.c.capsules.iter().map(|c| json!([c.id, c.owner, c.skill])).collect::<Vec<_>>(),
        "slots": w.c.fighters[0].slots, "events": events_json(&w),
    })
}

/// A movement special (s43 = 3) through the real match step (`Match::step`: the sim's physics): the
/// caster's position, height, floating and action every tick for six seconds.
fn movement_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let mut c = Combat::new(ctx.skills.clone(), vec![AURA_PARTICLE; 3]);
    c.set_rules(TEST_RULES);
    c.set_clip_ticks(vec![ctx.clips.clone(), ctx.clips.clone()]);
    c.set_action_rows(ctx.rows.clone());
    c.set_reaction_rows(ctx.reactions.clone());
    c.set_shot_markers(ctx.markers.clone());
    c.set_hand_paths(vec![ctx.hands.clone(), ctx.hands.clone()]);
    let mut m = Match::new([Vec3::new(0.0, 0.0, -150.0), Vec3::new(0.0, 0.0, 150.0)], c);
    m.combat.round_ticks = COUNTDOWN_TICKS;
    {
        let f = &mut m.combat.fighters[0];
        f.level = 20;
        f.aura_ticks = 20 * AURA_TICKS_PER_POINT;
        f.slots = [Some(def.id), None, None, None];
        f.arsenal.clear();
    }
    m.combat.fighters[1].arsenal.clear();
    // A spot of the caster's own to retreat to.
    m.combat.capsules = vec![CapsuleItem { id: 1, owner: 0, skill: AURA_PARTICLE, pos: Vec3::new(60.0, 0.0, -120.0), appear: 0 }];
    // Stand a little away from the start point so Return has somewhere to come back from.
    m.players[0].pos = Vec3::new(30.0, 0.0, -100.0);
    m.players[0].prev_pos = m.players[0].pos;
    let mut col = floor();
    let start = m.players[0].pos;
    let facing_yaw = m.players[0].yaw;
    let spawn = m.players[0].spawn;
    let mut track = Vec::new();
    let mut fire = None;
    for t in 0..360u32 {
        let mut i0 = crate::sim::Input::default();
        if t == 0 {
            i0.skills = 1;
        }
        m.step([i0, crate::sim::Input::default()], &mut col);
        m.combat.capsules.retain(|cp| cp.owner == 0 && cp.id == 1);
        if fire.is_none() && m.combat.events.iter().any(|(_, e)| matches!(e, CombatEvent::SkillFired { who: 0, .. })) {
            fire = Some(t);
        }
        let p = &m.players[0];
        // Every tick (the moveskills lane's per-tick height / position logs read it).
        track.push(json!([t, v3(p.pos - start), m.combat.fighters[0].status.hover_ticks, format!("{:?}", m.combat.fighters[0].action)]));
    }
    let p = &m.players[0];
    json!({
        "start": v3(start), "spawn": v3(spawn), "facing_yaw": r3(facing_yaw), "fire_step": fire, "track": track,
        "end": v3(p.pos), "end_rel": v3(p.pos - start), "target_spawn": v3(m.players[1].spawn),
        "hover_height": r3(m.combat.fighters[0].status.hover_height),
        "capsule_anchor": m.combat.fighters[0].capsule_anchor.map(v3),
        "max_y": track.iter().map(|t| t[1][1].as_f64().unwrap_or(0.0)).fold(0.0f64, f64::max),
        "max_hover": track.iter().map(|t| t[2].as_u64().unwrap_or(0)).max(),
    })
}

// ---------------------------------------------------------------------------
// Sounds and effects.

fn sound_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let probe = scratch_combat(ctx);
    let row = probe.action_row(def.id, false);
    let cast = row.map_or(0, |r| r.cast_sound);
    let voice = ctx.voice.get(&def.id).copied().unwrap_or(0);
    let s = crate::sfx::skill_sounds(&def.raw, cast, voice, |id| ctx.common_sounds.contains(&id));
    json!({"cast": s.cast, "fire": s.fire, "travel": s.travel, "impact": s.impact, "voice": s.voice, "no_reaction": s.no_reaction})
}

fn effect_ids_probe(ctx: &Ctx, fxlib: &mut crate::effects::FxLibrary, map: &crate::fx::SkillFx, def: &SkillDef) -> Value {
    let name = |air: bool| map.name(def.id, air).map(str::to_string);
    let g = name(false);
    let a = name(true);
    let slots = |n: &Option<String>, fx: &mut crate::effects::FxLibrary| -> Value {
        match n.as_deref().and_then(|n| fx.get(n)) {
            Some(d) => json!((0..6).map(|s| d.has_slot(s)).collect::<Vec<_>>()),
            None => Value::Null,
        }
    };
    let _ = ctx;
    json!({"ground": g, "air": a, "ground_slots": slots(&g, fxlib), "air_slots": slots(&a, fxlib)})
}

// ---------------------------------------------------------------------------
// The guard battery (FUN_140271760, wave 6 lane probes): every defence struck through
// `Combat::strike` by an attacker of each class the roster has, from the front, the back and (Rain,
// Hole, Tornado) from above, at STR below / equal to / above its DEF. `engine/re/skill_audit_rules.py`
// derives each outcome from the program's own rules and compares.

/// One attacker class: the first roster skill of each (colour, motion, s54, blast) kind, the way it
/// arrives at a body in `Combat` (`Arrival`), and whether its object is in the program's blast state.
struct Atk {
    label: String,
    id: i16,
    arrival: Arrival,
    /// Arrives from above or below (Rain, Hole, Tornado): there is no side.
    above: bool,
    /// The attack object is a growing blast (the program's object state 3, `FUN_14026dbd0`).
    state3: bool,
    /// It is the blast of a shot (Bomb, Bomb Shot...), not a mine.
    of_shot: bool,
}

fn atk_classes(ctx: &Ctx) -> Vec<Atk> {
    let mut seen: std::collections::HashSet<(u8, i16, i16, bool)> = Default::default();
    let mut out = Vec::new();
    let mut defs: Vec<&SkillDef> = ctx.skills.iter().filter(|d| (1..=340).contains(&d.id)).collect();
    defs.sort_by_key(|d| d.id);
    defs.dedup_by_key(|d| d.id);
    for d in defs {
        let m = d.motion();
        // An object (s43 = 0) that strikes a body: instants (motion 4) land through the effect queue,
        // 18 / 19 have no handler.
        if d.raw[43] != 0 || matches!(m, 3 | 4 | 18 | 19) {
            continue;
        }
        let attack = d.capsule == Capsule::Attack;
        if !attack && !matches!(m, 0 | 12 | 13) {
            continue;
        }
        let blast = d.blast_radius() > 0.0;
        if !seen.insert((d.capsule as u8, m, d.raw[pd_formats::skills::field::PIERCE], blast)) {
            continue;
        }
        let (arrival, above) = match m {
            12..=14 => (Arrival::Melee, false),
            7 | 10 | 15 | 16 | 20 => (Arrival::Area, true),
            // A mine goes off as a blast (`Combat::tick_blasts`).
            8 | 9 => (Arrival::Blast, false),
            _ => (Arrival::Shot, false),
        };
        out.push(Atk { label: format!("{} (m{} p{})", d.name, m, d.raw[pd_formats::skills::field::PIERCE]), id: d.id, arrival, above, state3: matches!(m, 8 | 9), of_shot: false });
        // A blast of a shot (Bomb, Bomb Shot...) meets a guard in its blast state when it reaches someone who
        // was not the shot's target: `Combat::tick_blasts` strikes it with `Arrival::Blast` (lane guardfix;
        // it used to be `Arrival::Shot`).
        if blast && arrival == Arrival::Shot {
            out.push(Atk { label: format!("{} blast (m{} p{})", d.name, m, d.raw[pd_formats::skills::field::PIERCE]), id: d.id, arrival: Arrival::Blast, above, state3: true, of_shot: true });
        }
    }
    out
}

/// A bare two-player combat for a one-strike scenario: the skills, the rules, the reaction rows and
/// nothing else (hand paths and shot markers are not needed to resolve a strike).
fn lean_world(ctx: &Ctx) -> (Combat, [Player; 2]) {
    let mut c = Combat::new(ctx.skills.clone(), vec![AURA_PARTICLE; 3]);
    c.set_rules(TEST_RULES);
    c.round_ticks = COUNTDOWN_TICKS;
    c.set_reaction_rows(ctx.reactions.clone());
    let mut p = [Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, 60.0), std::f32::consts::PI)];
    for q in p.iter_mut() {
        q.locked = false;
    }
    for i in 0..2 {
        c.fighters[i].level = 20;
        c.fighters[i].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        c.fighters[i].max_health = 20;
        c.fighters[i].health = 15;
    }
    (c, p)
}

/// What one strike did, read straight off the state (guard, Health, Aura, slots, statuses, events,
/// new projectiles) before and after.
fn strike_obs(c: &mut Combat, p: &[Player; 2], id: i16, from: Option<Vec3>, arrival: Arrival, strength: i32) -> Value {
    let seen0 = c.events.last().map_or(0, |e| e.0);
    let (h0, a0, s0) = (c.fighters[1].health, c.fighters[1].aura_ticks, c.fighters[1].slots);
    let proj0 = c.projectiles.len();
    let bodies = bodies_of(p);
    let ret = c.strike(0, 1, id, from, bodies, arrival, Via { button: 0, strength: Some(strength) });
    let evs: Vec<CombatEvent> = c.events.iter().filter(|(s, _)| *s > seen0).map(|(_, e)| *e).collect();
    let blocked = evs.iter().any(|e| matches!(e, CombatEvent::Blocked { .. }));
    let broken = evs.iter().any(|e| matches!(e, CombatEvent::Blocked { broken: true, .. }));
    let hit = evs.iter().any(|e| matches!(e, CombatEvent::Hit { attacker: 0, victim: 1, .. }));
    let new = &c.projectiles[proj0.min(c.projectiles.len())..];
    let (v, a) = (&c.fighters[1], &c.fighters[0]);
    json!({
        "ret": ret, "blocked": blocked, "broken": broken, "hit": hit,
        "vhealth": v.health - h0, "vaura": (v.aura_ticks - a0) as f64 / AURA_TICKS_PER_POINT as f64,
        "guard": v.guard.map(|g| json!({"def": g.guard.def, "lingering": g.lingering})),
        "slot0": v.slots[0], "slots": v.slots, "slot0_before": s0[0],
        "reflected": new.iter().filter(|q| q.owner == 1 && !q.harmless).count(),
        "brushed": new.iter().filter(|q| q.harmless).count(),
        "stunned": a.action == Action::Hit,
        "a_bound": a.status.bound_ticks, "a_frozen": a.frozen_ticks, "a_poison": a.poison_ticks, "a_slot0": a.slots[0],
    })
}

fn gb_run(ctx: &Ctx, g: &SkillDef, a: &Atk, strength: i32, side: u8) -> Value {
    let (mut c, p) = lean_world(ctx);
    let gd = barrier::Guard::from_def(g).expect("a guard").with_uses(g, 0);
    c.fighters[1].slots = [Some(g.id), None, None, None];
    c.fighters[1].guard = Some(ActiveGuard::new(g.id, gd, 0));
    c.fighters[0].slots = [Some(a.id), None, None, None];
    // Pursuit hits only a body lying on the floor.
    if let Some(def) = c.skill(a.id).cloned() {
        if c.only_hits_downed(&def) {
            c.fighters[1].action = Action::Knocked;
            c.fighters[1].action_ticks = 400;
            c.fighters[1].lie_ticks = 300;
        }
    }
    // The victim stands at z = 60 facing -Z: z < 60 is its front.
    let from = match side {
        0 => Some(Vec3::new(0.0, 0.0, 20.0)),
        1 => Some(Vec3::new(0.0, 0.0, 100.0)),
        _ => None,
    };
    strike_obs(&mut c, &p, a.id, from, a.arrival, strength)
}

fn guard_battery(ctx: &Ctx, g: &SkillDef) -> Value {
    let Some(base) = barrier::Guard::from_def(g) else { return Value::Null };
    if base.automatic() {
        return json!({"automatic": true});
    }
    let atks = atk_classes(ctx);
    let d = g.defence;
    let mut strs: Vec<i32> = vec![1, d.max(1), d + 1, d + 3];
    strs.sort_unstable();
    strs.dedup();
    let mut cases = Vec::new();
    for (ai, a) in atks.iter().enumerate() {
        let sides: &[u8] = if a.above { &[2] } else { &[0, 1] };
        for &s in &strs {
            for &side in sides {
                let r = match catch_unwind(AssertUnwindSafe(|| gb_run(ctx, g, a, s, side))) {
                    Ok(v) => v,
                    Err(e) => json!({"err": e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()}),
                };
                cases.push(json!([ai, s, side, r]));
            }
        }
    }
    json!({
        "def": d,
        "attackers": atks.iter().map(|a| json!({"label": a.label, "id": a.id, "arrival": format!("{:?}", a.arrival), "above": a.above, "state3": a.state3, "of_shot": a.of_shot})).collect::<Vec<_>>(),
        "cases": cases,
    })
}

// ---------------------------------------------------------------------------
// Automatic Shelters, Orb and Amulet: probed through an incoming attack (FUN_14026f530).

/// Two weak strikes in a row on a holder of an automatic Shelter, `gap` sim ticks apart.
fn auto_run(ctx: &Ctx, def: &SkillDef, gap: u32, strength: i32) -> Value {
    let (mut c, mut p) = lean_world(ctx);
    c.fighters[1].slots = [Some(def.id), None, None, None];
    c.fighters[1].health = 20;
    c.fighters[0].slots = [Some(80), None, None, None];
    let before = json!({"health": c.fighters[1].health, "aura": c.fighters[1].aura_ticks as f64 / AURA_TICKS_PER_POINT as f64, "level": c.fighters[1].level});
    let front = Some(Vec3::new(0.0, 0.0, 20.0));
    let first = strike_obs(&mut c, &p, 80, front, Arrival::Shot, strength);
    // How long the guard object stands afterwards (the program ends a kind 7 object at its first update).
    let mut up_for = None;
    {
        let mut c2 = c.clone();
        let mut p2 = p.clone();
        for k in 0..120u32 {
            if c2.fighters[1].guard.is_none() {
                up_for = Some(k);
                break;
            }
            c2.step(&mut p2, [0; 2], &floor());
        }
    }
    let after_first = json!({"health": c.fighters[1].health, "aura": c.fighters[1].aura_ticks as f64 / AURA_TICKS_PER_POINT as f64, "slot0": c.fighters[1].slots[0], "guard": c.fighters[1].guard.is_some(), "action": format!("{:?}", c.fighters[1].action)});
    let col = floor();
    for _ in 0..gap {
        c.step(&mut p, [0; 2], &col);
    }
    let second = strike_obs(&mut c, &p, 80, front, Arrival::Shot, strength);
    json!({"gap": gap, "strength": strength, "before": before, "first": first, "after_first": after_first, "up_for": up_for, "second": second,
        "after": {"health": c.fighters[1].health, "aura": c.fighters[1].aura_ticks as f64 / AURA_TICKS_PER_POINT as f64, "slot0": c.fighters[1].slots[0]}})
}

fn auto_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    json!({
        "runs": [auto_run(ctx, def, 0, 1), auto_run(ctx, def, 2, 1), auto_run(ctx, def, 12, 1), auto_run(ctx, def, 40, 1)],
        // A strong hit (above its DEF) breaks it.
        "strong": auto_run(ctx, def, 0, def.defence + 2),
    })
}

/// Orb / Amulet: an erase attack (Interference: Level - 2) strikes the holder; read what it cost the holder
/// and what the erase did. `aura`: the holder's Aura in whole points at the strike.
fn orb_run(ctx: &Ctx, def: &SkillDef, aura: i32) -> Value {
    let (mut c, mut p) = lean_world(ctx);
    c.fighters[1].slots = [None, None, None, Some(def.id)];
    c.fighters[1].level = 8;
    c.fighters[1].aura_ticks = aura * AURA_TICKS_PER_POINT;
    c.fighters[0].slots = [Some(96), None, None, None];
    let col = floor();
    let before = json!({"level": c.fighters[1].level, "aura": c.fighters[1].aura_ticks as f64 / AURA_TICKS_PER_POINT as f64, "caster_level": c.fighters[0].level});
    let front = Some(Vec3::new(0.0, 0.0, 20.0));
    let obs = strike_obs(&mut c, &p, 96, front, Arrival::Shot, 0);
    // The erase takes effect once the sim has finished moving things this tick.
    for _ in 0..6 {
        c.step(&mut p, [0; 2], &col);
    }
    json!({"aura": aura, "before": before, "strike": obs,
        "after": {"level": c.fighters[1].level, "aura": c.fighters[1].aura_ticks as f64 / AURA_TICKS_PER_POINT as f64, "caster_level": c.fighters[0].level, "slot3": c.fighters[1].slots[3]}})
}

fn orb_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    json!({"runs": [orb_run(ctx, def, 8), orb_run(ctx, def, 0)]})
}

// ---------------------------------------------------------------------------
// STR rule 21 (the cast's stored value x s46): hold-to-charge and the Level-paid Finisher.

/// Hold the button `hold` sim ticks after the press, let go, and read the Health the hit takes off.
fn str21_run(ctx: &Ctx, def: &SkillDef, hold: u32, level: i32) -> Value {
    let dist = if melee_like(def) { 60.0 } else { aim_distance(def) };
    let mut w = World::new(ctx, dist);
    prepare(&mut w, def, true);
    w.c.fighters[0].level = level;
    w.c.fighters[0].aura_ticks = level * AURA_TICKS_PER_POINT;
    w.c.fighters[1].max_health = 99;
    w.c.fighters[1].health = 99;
    w.p[0].locked = true;
    w.p[0].target = Some(1);
    let h0 = w.c.fighters[1].health;
    let aura0 = w.c.fighters[0].aura_ticks as f64 / AURA_TICKS_PER_POINT as f64;
    w.press(0, 0);
    let mut first_hit = None;
    for k in 1..=(hold + 400) {
        let b = if k <= hold { [1, 0] } else { [0; 2] };
        w.step(b);
        if first_hit.is_none() && w.log.iter().any(|(t, e)| *t == w.t && matches!(e, CombatEvent::Hit { attacker: 0, victim: 1, .. })) {
            first_hit = Some(k);
            break;
        }
    }
    json!({
        "hold": hold, "level": level, "hit_step": first_hit, "damage": h0 - w.c.fighters[1].health, "aura_before": aura0,
        "aura_after": w.c.fighters[0].aura_ticks as f64 / AURA_TICKS_PER_POINT as f64, "level_after": w.c.fighters[0].level,
        "charge_cycle": w.c.action_row(def.id, false).map(|r| r.charge_cycle),
    })
}

fn str21_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    // [Object] skills (Power, Psycho Kinesis) take their STR from the thrown object, not from a charge.
    if def.raw[field_str_rule()] != 21 || def.motion() == moves::motion::OBJECT {
        return Value::Null;
    }
    // Finisher pays by Level (s23 < 0): STR = 2 x Level at the press.
    if def.raw[23] < 0 {
        return json!({"kind": "level", "runs": [str21_run(ctx, def, 0, 3), str21_run(ctx, def, 0, 8)]});
    }
    // Hold-to-charge: wind-up w39 program ticks, then one charge point per 10 program ticks held.
    json!({"kind": "charge", "runs": [str21_run(ctx, def, 76, 20), str21_run(ctx, def, 116, 20), str21_run(ctx, def, 156, 20)]})
}

fn field_str_rule() -> usize {
    pd_formats::skills::field::STRENGTH_RULE
}

// ---------------------------------------------------------------------------
// Requirement 5 (skills left): the deck AND the player's own capsules lying on the field (FUN_140262470).

fn req5_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    let n = def.requirement_amount;
    let attempt = |arsenal: usize, capsules: usize| -> bool {
        let mut w = World::new(ctx, aim_distance(def));
        prepare(&mut w, def, true);
        w.keep_capsules = true;
        w.c.fighters[0].arsenal = vec![AURA_PARTICLE; arsenal];
        w.c.capsules.clear();
        for k in 0..capsules {
            // Far from the caster: not picked up.
            w.c.capsules.push(CapsuleItem { id: 900 + k as u32, owner: 0, skill: AURA_PARTICLE, pos: Vec3::new(300.0 + 20.0 * k as f32, 0.0, 300.0), appear: 0 });
        }
        w.p[0].locked = melee_like(def);
        w.p[0].target = melee_like(def).then_some(1);
        let seq0 = w.c.fighters[0].cast_seq;
        w.press(0, 0);
        w.c.fighters[0].cast_seq != seq0
    };
    json!({
        "n": n,
        // deck n, no capsule: skills left n: allowed. deck n + 1 capsule 0: n + 1: refused.
        "deck_n": attempt(n as usize, 0), "deck_n_plus_1": attempt(n as usize + 1, 0),
        // deck n - 1 + 1 capsule: n: allowed. deck n + 1 capsule: n + 1: refused. deck n + 2 capsules (deck n - 1): n + 1.
        "deck_n_minus_1_capsule": attempt(n as usize - 1, 1), "deck_n_capsule": attempt(n as usize, 1),
        "deck_n_minus_2_two_capsules": attempt(n as usize - 2, 2), "deck_n_minus_1_two_capsules": attempt(n as usize - 1, 2),
    })
}

// ---------------------------------------------------------------------------
// Recollection / Copy: resolved at the press as the copied skill (FUN_140261e90 / FUN_140258180).

fn copy_probe(ctx: &Ctx, def: &SkillDef) -> Value {
    if def.raw[43] != 6 {
        return Value::Null;
    }
    let dist = aim_distance(def);
    // Nothing remembered: the press is refused (FUN_140258180: skill index -1 returns after the 'no' sound).
    let mut w = World::new(ctx, dist);
    prepare(&mut w, def, true);
    w.c.fighters[0].last_attack = None;
    w.c.fighters[0].last_hurt_by = None;
    let seq0 = w.c.fighters[0].cast_seq;
    w.press(0, 0);
    let refused = w.c.fighters[0].cast_seq == seq0;
    let denied = w.log.iter().any(|(_, e)| matches!(e, CombatEvent::Denied { .. }));
    json!({"refused_when_nothing_remembered": refused, "denied_sound": denied, "copies": if def.raw[44] == 1 { "last_hurt_by" } else { "last_attack" }})
}

// ---------------------------------------------------------------------------

fn probe_skill(ctx: &Ctx, fxlib: &mut crate::effects::FxLibrary, fxmap: &crate::fx::SkillFx, def: &SkillDef) -> Value {
    let guard = |label: &str, f: &mut dyn FnMut() -> Value| -> Value {
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(v) => v,
            Err(e) => {
                let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                json!({"probe_error": format!("{label}: {msg}")})
            }
        }
    };
    let b = moves::behaviour_of(def);
    let mut out = serde_json::Map::new();
    out.insert("id".into(), json!(def.id));
    out.insert("name".into(), json!(def.name));
    out.insert("static".into(), guard("static", &mut || static_view(ctx, def)));
    out.insert("cast".into(), guard("cast", &mut || cast_probe(ctx, def)));
    out.insert("gate".into(), guard("gate", &mut || gate_probe(ctx, def)));
    out.insert("uses".into(), guard("uses", &mut || uses_probe(ctx, def)));
    out.insert("str".into(), guard("str", &mut || str_probe(ctx, def)));
    out.insert("sound".into(), guard("sound", &mut || sound_probe(ctx, def)));
    out.insert("fx".into(), guard("fx", &mut || effect_ids_probe(ctx, fxlib, fxmap, def)));
    if def.capsule == Capsule::Attack || def.raw[43] == 0 || def.raw[43] == 1 {
        out.insert("band".into(), guard("band", &mut || band_probe(def)));
    }
    match b {
        Behaviour::Shoot(_) => {
            out.insert("flight".into(), guard("flight", &mut || flight_probe(ctx, def)));
        }
        Behaviour::Place(_) => {
            out.insert("area".into(), guard("area", &mut || area_probe(ctx, def)));
        }
        Behaviour::Melee(_) => {
            out.insert("melee".into(), guard("melee", &mut || melee_probe(ctx, def)));
        }
        Behaviour::Guard(g) => {
            out.insert("guard".into(), guard("guard", &mut || guard_probe(ctx, def)));
            out.insert("guardbat".into(), guard("guardbat", &mut || guard_battery(ctx, def)));
            if g.automatic() {
                out.insert("auto".into(), guard("auto", &mut || auto_probe(ctx, def)));
            }
        }
        Behaviour::Effect(moves::effects::Delivery::Environment) => {
            out.insert("env".into(), guard("env", &mut || env_probe(ctx, def)));
        }
        _ => {}
    }
    let non_attack = matches!(def.capsule, Capsule::Status | Capsule::Special | Capsule::Erase);
    if def.capsule == Capsule::Special && matches!(def.id, 180 | 299) {
        out.insert("orb".into(), guard("orb", &mut || orb_probe(ctx, def)));
    }
    if def.capsule == Capsule::Attack && def.raw[field_str_rule()] == 21 {
        out.insert("str21".into(), guard("str21", &mut || str21_probe(ctx, def)));
    }
    if def.requirement == 5 {
        out.insert("req5".into(), guard("req5", &mut || req5_probe(ctx, def)));
    }
    if def.raw[43] == 6 {
        out.insert("copy".into(), guard("copy", &mut || copy_probe(ctx, def)));
    }
    if def.raw[43] == 1 {
        out.insert("capsules".into(), guard("capsules", &mut || json!([capsule_probe(ctx, def, false), capsule_probe(ctx, def, true)])));
    }
    if def.raw[43] == 3 {
        out.insert("movement".into(), guard("movement", &mut || movement_probe(ctx, def)));
    }
    if (non_attack && def.raw[43] != 0) || matches!(b, Behaviour::AuraParticle) {
        out.insert("effect".into(), guard("effect", &mut || effect_probe(ctx, def)));
    }
    // Attacks, and the erase / status / special skills that travel like one (s43 = 0), meet the opponent.
    if (def.capsule == Capsule::Attack || (non_attack && def.raw[43] == 0)) && !matches!(b, Behaviour::AuraParticle) {
        out.insert("hit".into(), guard("hit", &mut || hit_probe(ctx, def, HAND_A)));
        if non_attack {
            out.insert("hit_b".into(), guard("hit_b", &mut || hit_probe(ctx, def, HAND_B)));
        }
    }
    Value::Object(out)
}

#[test]
fn skill_audit_dump() {
    let Some(out) = std::env::var_os("PD_SKILL_AUDIT_OUT") else {
        // Not asked for: only prove the probe still runs on two skills when the cache is here.
        let Some(ctx) = Ctx::load() else { return };
        let mut fxlib = crate::effects::FxLibrary::new(ctx.cache.clone());
        let fxmap = crate::fx::SkillFx::load(&ctx.cache.join("skills.json"));
        for id in [65, 80] {
            let def = ctx.skills.iter().find(|d| d.id == id).unwrap();
            let v = probe_skill(&ctx, &mut fxlib, &fxmap, def);
            assert_eq!(v["cast"]["ground"]["fire_step"].as_u64(), Some(if id == 65 { 26 } else { 40 }), "{id} fires at its action row's tick");
        }
        return;
    };
    let ctx = Ctx::load().expect("cache/skills.json is needed for the skill audit");
    let mut fxlib = crate::effects::FxLibrary::new(ctx.cache.clone());
    let fxmap = crate::fx::SkillFx::load(&ctx.cache.join("skills.json"));
    let mut seen = std::collections::HashSet::new();
    let mut ids: Vec<&SkillDef> = ctx.skills.iter().filter(|d| (1..=340).contains(&d.id) && seen.insert(d.id)).collect();
    ids.sort_by_key(|d| d.id);
    let only: Option<Vec<i16>> = std::env::var("PD_SKILL_AUDIT_IDS").ok().map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    let mut skills = Vec::new();
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let aura = SkillDef::aura_particle();
    let mut all: Vec<&SkillDef> = vec![&aura];
    all.extend(ids);
    for def in all {
        if only.as_ref().is_some_and(|o| !o.contains(&def.id)) {
            continue;
        }
        skills.push(probe_skill(&ctx, &mut fxlib, &fxmap, def));
    }
    std::panic::set_hook(prev);
    let builtin_decks = {
        let sj: Value = serde_json::from_str(&std::fs::read_to_string(ctx.cache.join("skills.json")).unwrap()).unwrap();
        let registered = crate::load_skills(&ctx.cache.join("skills.json"), true);
        crate::frontend::discovery::school_decks(&sj, &registered).iter().map(|d| json!({"name": d.name, "kind": d.kind, "skills": d.skills})).collect::<Vec<_>>()
    };
    let doc = json!({
        "builtin_decks": builtin_decks,
        "version": 1,
        "clips": ctx.clips,
        "tick": {"sim_hz": 60, "program_hz": 30},
        "skills": skills,
    });
    std::fs::write(&out, serde_json::to_string(&doc).unwrap()).expect("write the audit json");
}
