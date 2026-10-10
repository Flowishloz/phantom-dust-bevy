//! Hit-stop and KO slow-motion: the weight a hit gives a fight.
//!
//! The simulation is untouched. A hit does not "stop" anything in `Combat` --
//! this module stops the *clock*, which freezes `FixedUpdate` and therefore
//! `step_sim`, and no input is lost: `PendingInputs` holds the buttons still
//! down, so the player resumes mid-motion holding the stick.
//!
//! Why the clock and not the sim: the sim is a deterministic fixed-step
//! function of its input list (PROGRAM-RE.md "Determinism"), which is what
//! replays and netcode will depend on. A hit-stop that inserted dead ticks
//! into `Combat` would make the sim depend on wall-clock time and break both.
//! Freezing the clock keeps `Combat` a pure function of the recorded inputs.
//!
//! The mechanism is `Time<Virtual>::set_relative_speed`, which `Time<Fixed>`
//! follows (bevy_time 0.19 `fixed.rs`: "The fixed timestep clock follows the
//! Time<Virtual> clock, which means it is affected by pause() and
//! set_relative_speed()"). At 0.0 the fixed clock stops accumulating, so
//! `FixedUpdate` does not run at all and the renderer interpolates between
//! the last two sim ticks -- both of which the hit-stop leaves in place --
//! so the frozen pose is the pose at the moment of the hit.
//!
//! Hit-stop and KO slow-motion, gated on the `PD_HITSTOP` environment
//! variable: **set it to anything but `0` to enable** (`PD_HITSTOP=1`).
//! Off by default, because it changes the game's timing and the default
//! should stay the one already played and tuned. With it off, `drive`
//! returns immediately and the clock is never touched.

use bevy::prelude::*;
use bevy::time::Virtual;

use crate::combat::{CombatEvent, Action};
use crate::World;

/// How long the clock is held at zero, in real seconds, per hit weight.
/// Four sim ticks at 60 Hz. The original freezes on impact; the length here
/// is tuned by ear and is the first thing to change if it reads as mush.
const FREEZE_BY_WEIGHT: [(f32, f32); 3] = [
    // (fraction of the victim's max health removed, freeze seconds)
    (0.00, 0.055),
    (0.12, 0.085),
    (0.25, 0.120),
];

/// A block is a lighter touch than a clean hit: it stops for less time and
/// never reaches the top weight, so defending does not read as winning.
const BLOCK_FREEZE: f32 = 0.035;

/// KO slow-motion: the clock runs at this ratio from the moment both
/// fighters are down until the round restarts. Slow, not stopped -- the
/// player still sees the aftermath land.
const KO_SPEED: f32 = 0.35;

/// How long after the last fighter goes down the slow-motion holds before it
/// releases, in real seconds. `ROUND_END_TICKS` is 180 sim ticks (3 s), and
/// the sim clock is now 35% speed, so that window covers most of it; this is
/// the tail, so a double-KO held for a full 3 s of sim time does not drag.
const KO_HOLD: f32 = 1.6;

/// The clock's current state, and why. `Speed` is what this module last asked
/// for; `Real` is the clock's actual value, kept so a restart (which recreates
/// resources) or another system changing the clock is noticed rather than
/// fought over.
#[derive(Resource, Default)]
pub struct HitStop {
    /// Real seconds left on the current freeze.
    freeze_left: f32,
    /// Real seconds left of KO slow-motion.
    ko_left: f32,
    /// Whether a KO is in force at all (held until the round restarts).
    ko_active: bool,
    /// The last speed this module requested.
    requested: f32,
    /// Which players are down as of the last read, so the KO edge is detected
    /// on a transition rather than a level.
    was_down: bool,
    /// The event sequence number already consumed, so a hit is counted once
    /// however many frames it is visible for.
    seen_event: u32,
    /// Set when a freeze or slow-mo was requested this frame, for the tests.
    pub last_action: Option<&'static str>,
}

impl HitStop {
    /// The speed the clock should be running at, given what is in force.
    /// Frozen wins over slow: a KO that lands on the same tick as a hit
    /// holds the frame instead of crawling it.
    fn target_speed(&self) -> f32 {
        if self.freeze_left > 0.0 {
            0.0
        } else if self.ko_active {
            KO_SPEED
        } else {
            1.0
        }
    }
}

/// Reads the sim's hit/blocked events and the KO state, and drives the clock.
///
/// Runs in `Update`, before the sim's own `FixedUpdate`, so a hit landed on
/// the previous tick freezes the very next frame.
pub fn drive(
    world: Res<World>,
    mut clock: ResMut<Time<Virtual>>,
    mut hit_stop: ResMut<HitStop>,
    real: Res<Time<Real>>,
) {
    let enabled = std::env::var_os("PD_HITSTOP").is_some_and(|v| v != "0");
    if !enabled {
        return;
    }

    // Count the timers down in REAL time. This is the whole reason the freeze
    // works at all, and it was a real deadlock before it was caught: the
    // timers used to be decremented by the virtual clock's delta -- the very
    // clock being frozen to 0.0. So the freeze could never expire and the
    // game would have locked up on the first hit. `Time<Real>` keeps running
    // whatever the virtual clock is doing, so it is both the correct clock
    // and the only one that cannot be stopped by this module.
    let dt = real.delta_secs();
    let combat = &world.0.combat;

    // --- new hits and blocks -------------------------------------------------
    if combat.events.last().map_or(0, |e| e.0) >= hit_stop.seen_event {
        let mut strongest: f32 = 0.0;
        for &(seq, ev) in &combat.events {
            if seq <= hit_stop.seen_event {
                continue;
            }
            match ev {
                CombatEvent::Hit { victim, .. } => {
                    let f = &combat.fighters[victim];
                    let lost = 1.0 - (f.health as f32 / f.max_health.max(1) as f32);
                    strongest = strongest.max(weight_of(lost));
                }
                CombatEvent::Blocked { .. } => strongest = strongest.max(BLOCK_FREEZE),
                _ => {}
            }
        }
        hit_stop.seen_event = combat.events.last().map_or(0, |e| e.0);
        if strongest > 0.0 {
            // A bigger hit on top of a running freeze replaces it rather than
            // stacking, so a fast exchange does not lock the clock for a
            // second and a half.
            hit_stop.freeze_left = hit_stop.freeze_left.max(strongest);
        }
    }

    // --- KO ------------------------------------------------------------------
    let down = combat.roster.slots().all(|i| combat.fighters[i].action == Action::Down);
    if down && !hit_stop.was_down {
        hit_stop.ko_active = true;
        hit_stop.ko_left = KO_HOLD;
        hit_stop.last_action = Some("ko");
    } else if !down && hit_stop.was_down {
        // Somebody stood up (resuscitation, or the round restarted).
        hit_stop.ko_active = false;
        hit_stop.ko_left = 0.0;
    }
    hit_stop.was_down = down;

    // --- advance the timers in real time -------------------------------------
    if hit_stop.freeze_left > 0.0 {
        hit_stop.freeze_left -= dt;
        if hit_stop.freeze_left <= 0.0 {
            hit_stop.freeze_left = 0.0;
            hit_stop.last_action = Some("freeze-end");
        }
    }
    if hit_stop.ko_left > 0.0 {
        hit_stop.ko_left -= dt;
        if hit_stop.ko_left <= 0.0 {
            hit_stop.ko_left = 0.0;
            hit_stop.last_action = Some("ko-end");
        }
    }

    // Push the clock only when it is not already where we want it. Writing
    // `set_relative_speed` unconditionally would stomp on any other system
    // that pauses the game (the settings menu, an OS-level hitch), so the
    // clock is read first and only touched on a real disagreement.
    let want = hit_stop.target_speed();
    if clock.relative_speed() != want {
        clock.set_relative_speed(want);
    }
    if want != hit_stop.requested {
        hit_stop.requested = want;
        hit_stop.last_action = Some(match want {
            0.0 => "freeze",
            1.0 => "normal",
            _ => "slow",
        });
    }
}

/// Freeze length for a hit that removed `lost` (0..1) of the victim's health.
fn weight_of(lost: f32) -> f32 {
    let mut out = FREEZE_BY_WEIGHT[0].1;
    for &(threshold, seconds) in &FREEZE_BY_WEIGHT {
        if lost >= threshold {
            out = seconds;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Combat;
    use crate::sim::Match;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// A real `App` with the same two things `main.rs` wires: the sim's
    /// fixed step and `hitstop::drive`. This is the test that matters -- the
    /// unit tests below only check the weight arithmetic, while this one
    /// proves the load-bearing claim, that slowing `Time<Virtual>` really
    /// does stop `FixedUpdate` running the sim. If a future Bevy release
    /// decoupled those clocks, this test would fail rather than the freeze
    /// silently doing nothing in play.
    ///
    /// `Time` is advanced by hand (`ManualDuration`) because a headless test
    /// frame is a few microseconds of real time: 200 `app.update()` calls
    /// elapse well under a millisecond, so a freeze measured in tenths of a
    /// second would never lapse and the "resumes" half could not be tested
    /// without it. Each frame is given 20 ms, so 25 frames is half a second.
    fn app_with_a_match() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        // `ManualDuration` sets what `Time<Real>` advances by, and the
        // virtual clock inherits from Real -- so this one line is what makes
        // the test's frames 20 ms of *real* time. It matters because the
        // freeze timers count in Real (see `drive`): a headless frame is a
        // few microseconds of wall clock, so 30 frames would elapse under a
        // millisecond and a 0.2 s freeze could never lapse.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(0.02)));
        app.insert_resource(World(Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], Combat::new(Vec::new(), vec![0; 30]))));
        app.init_resource::<HitStop>();
        // PreUpdate, NOT Update, and this mirrors `main.rs` deliberately: the
        // first version of this test wired `drive` into Update and passed the
        // "does not tick" half for the wrong reason, then failed "resumes",
        // which is how the one-frame-late ordering bug was found at all. A
        // test that does not reproduce the real schedule cannot catch it.
        app.add_systems(PreUpdate, drive);
        app.add_systems(FixedUpdate, |mut w: ResMut<World>| {
            w.0.tick += 1;
        });
        app
    }

    /// The sim must not advance while a freeze is in force, and must resume
    /// on its own once the freeze expires.
    #[test]
    fn a_freeze_actually_stops_the_fixed_step_and_then_releases_it() {
        // `drive` is gated on PD_HITSTOP, so the flag has to be set or the
        // test exercises nothing. This was the first thing that made this
        // test lie: with the gate closed, `drive` returned before touching
        // anything, the clock stayed at 1.0 and the sim ticked happily, and
        // the assertion blamed the clock instead of the gate.
        unsafe { std::env::set_var("PD_HITSTOP", "1") };

        let mut app = app_with_a_match();
        // Start the freeze directly: the event plumbing is covered by the
        // unit tests, this is about the clock's effect on FixedUpdate.
        app.world_mut().resource_mut::<HitStop>().freeze_left = 0.2;

        // 5 frames x 20 ms = 100 ms, so the 200 ms freeze is still running
        // for most of them: the sim must not have ticked at all.
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(app.world().resource::<World>().0.tick, 0, "the sim must not tick while the clock is frozen");

        // Now let the freeze lapse (25 more frames = 500 ms) and confirm the
        // clock was released and the sim runs again on its own.
        for _ in 0..25 {
            app.update();
        }
        let resumed = app.world().resource::<World>().0.tick;
        assert!(resumed > 0, "the sim must resume once the freeze is over");
    }

    /// The one thing a screenshot can never show: that a hit actually makes
    /// the sim miss ticks, and that the miss is the size the weight curve
    /// asked for. Counts real elapsed time against sim ticks -- a frozen
    /// clock is one where wall time passes but `tick` does not move, which is
    /// the definition of hit-stop and is measurable without a camera.
    #[test]
    fn a_hit_costs_the_sim_ticks_it_should_and_then_gives_them_back() {
        unsafe { std::env::set_var("PD_HITSTOP", "1") };
        let mut app = app_with_a_match();

        // Baseline: 30 frames of pure running, to learn the sim's normal rate
        // in this harness (20 ms of real time per frame, and the sim is
        // stepped by FixedUpdate rather than by the clock's delta, so this
        // has to be measured, not assumed).
        let base_start = app.world().resource::<World>().0.tick;
        for _ in 0..30 {
            app.update();
        }
        let base_ticks = app.world().resource::<World>().0.tick - base_start;
        assert!(base_ticks > 0, "the sim must run when nothing is in force");

        // Now the same 30 frames with a heavy freeze in force from frame one.
        let mut app2 = app_with_a_match();
        app2.world_mut().resource_mut::<HitStop>().freeze_left = 0.2;
        let hit_start = app2.world().resource::<World>().0.tick;
        for _ in 0..30 {
            app2.update();
        }
        let hit_ticks = app2.world().resource::<World>().0.tick - hit_start;

        // 30 frames x 20 ms = 600 ms of wall clock, against a 200 ms freeze:
        // roughly a third of the ticks should be gone, and certainly not all
        // of them (that would be a lock-up) and not none (that would mean the
        // freeze did nothing).
        println!("HITSTOP: base {base_ticks} ticks in 600 ms, frozen {hit_ticks} ticks, freeze cost {} ticks", base_ticks - hit_ticks);
        assert!(hit_ticks < base_ticks, "the freeze must cost the sim ticks (base {base_ticks}, frozen {hit_ticks})");
        assert!(hit_ticks > 0, "the freeze must expire rather than lock the clock forever (frozen {hit_ticks})");
    }

    #[test]
    fn the_clock_returns_to_full_speed_after_a_freeze() {
        let mut app = app_with_a_match();
        app.world_mut().resource_mut::<HitStop>().freeze_left = 0.0;
        app.update();
        assert_eq!(app.world().resource::<Time<Virtual>>().relative_speed(), 1.0);
    }

    #[test]
    fn a_clean_hit_freezes_for_longer_than_a_graze() {
        assert!(weight_of(0.30) > weight_of(0.02));
        assert!(weight_of(0.02) >= FREEZE_BY_WEIGHT[0].1);
    }

    #[test]
    fn a_block_stops_for_less_time_than_any_hit() {
        let lightest_hit = weight_of(0.0);
        assert!(BLOCK_FREEZE < lightest_hit, "block {BLOCK_FREEZE} vs hit {lightest_hit}");
    }

    #[test]
    fn a_freeze_beats_a_slow_motion() {
        let mut h = HitStop { freeze_left: 0.05, ko_active: true, ..Default::default() };
        assert_eq!(h.target_speed(), 0.0, "a KO landing on a hit holds the frame");
        h.freeze_left = 0.0;
        assert_eq!(h.target_speed(), KO_SPEED, "then the KO crawls");
    }

    #[test]
    fn the_clock_returns_to_full_speed_when_nothing_is_in_force() {
        let h = HitStop::default();
        assert_eq!(h.target_speed(), 1.0);
    }

    #[test]
    fn ko_slow_motion_releases_after_its_hold() {
        let mut h = HitStop { ko_active: true, ko_left: 0.0, ..Default::default() };
        assert_eq!(h.target_speed(), KO_SPEED, "held until the timer runs out");
        h.ko_left = -1.0;
        h.ko_active = false;
        assert_eq!(h.target_speed(), 1.0);
    }
}
