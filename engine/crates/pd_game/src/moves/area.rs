//! Attacks that go off at a place rather than fly: [Hole], [Rain] (column and
//! cloud), mines and "explodes around self" (Photon Wave). The sim keeps them
//! in `Combat::areas` until they are over; their blasts are `Combat::blasts`.
//!
//! Guide:
//! - Hole: "show the area they will affect and then appear suddenly at the
//!   target's feet ... strong but easy to avoid."
//! - Rain: "assault the enemy over a duration of time ... The area that will be
//!   affected by this attack is shown for an instant, and then the attack
//!   commences. If objects are above the target, these skills typically won't
//!   affect the target ... the only Defense skill that is successful against
//!   Rain types are Shelter types."
//! - Detonation: "these skills require being 'set,' they aren't immediately
//!   launched. The affected area, although very difficult to see, is indeed visible."
//! - Explosion: "explode in a sphere centered around the attacker".
//!
//! Read from the program (docs/MOVES.md "Verdicts"; all times are program
//! ticks of 1/30 s, stepped every second sim tick):
//! - **Hole** (motion 15 / 16, `FUN_14026a510` / `FUN_14026a7f0`): aimed at the
//!   floor under the target (range band; motion 16 has no limit and makes a
//!   hole under *every* player); lives s64 + s65 ticks: the first s64 are the
//!   warning (the marker grows from 0 to its full size and slides along the
//!   target's running direction at the speed it had when cast), the last s65
//!   (10) are active: a sphere of s66 x 0.1 u + body radius at the floor,
//!   swept every tick, only against players standing on the ground; it passes
//!   through bodies (each is hit once).
//! - **Rain column** (motion 7, Lightning Bolt, Thunder Storm, Killer
//!   Satellite; `FUN_140266fb0` / `FUN_140267410`): warning s64 ticks with the
//!   same sliding marker, then active s65 ticks: a vertical column from the
//!   floor up s66 x 10 u of radius s68 x 0.1, swept every tick; on its first
//!   active tick a roof in the column ends it (unless s55 = 2 / 3).
//! - **Rain cloud** (motion 20, nine skills; `FUN_14026b480` / `14026bbd0` /
//!   `14026b8d0`): a cloud s69 x 0.1 (100) u above the floor point under the
//!   target that then follows the target's position at s71 / 10.8 a tick; it
//!   lives s64 + s65 ticks; the first drop falls s64 + 1 ticks after the cast
//!   and then one every s67 + 1 ticks, s66 drops in all, each from a random
//!   point within s70 x 0.1 u of the cloud, pointing down with a random lean
//!   of up to 10 deg, falling at s72 / 10.8 a tick for about 16 ticks, hit
//!   sphere 5 u. The drops of one cast share one hit mask: it lands **once**
//!   per player per cast.
//! - **Mines** (motion 8, `FUN_140267b90` / `FUN_140267e90`): set on the floor
//!   at the caster; s64 = 0 Timer goes off after s66 ticks; 1 Sensor is armed
//!   after s66 ticks and then goes off when *anyone* (its owner too) has their
//!   centre within s65 x 0.1 u with a clear line; 2 Remote goes off on the
//!   owner's next button press. A mine never fizzles. Its blast is s67 x 0.1 u
//!   and hurts everyone including the owner.
//! - **Around self** (motion 9, `FUN_140268050`): a blast at the spawn point at
//!   once, radius s64 x 0.1, the caster excluded.
//! - **Blasts** (`FUN_14026dbd0` / `14026ae10`): a separate object living 30
//!   ticks whose sphere grows linearly from 0 to the full radius over them, a
//!   body test every tick (sphere + body radius, line of sight from the
//!   centre), each player once. A shot's blast on a wall or the floor hurts
//!   everyone, its thrower included; on a body or a guard it skips the target.

use super::{motion, SPEED_UNIT, TABLE_DIST};
use crate::combat::SkillDef;

/// A blast lives this many program ticks and its sphere grows over all of them.
pub const BLAST_TICKS: u16 = 30;
/// Used when a skill has no radius in the table.
pub const DEFAULT_RADIUS: f32 = 30.0;
/// A rain drop's own hit sphere (`FUN_140263a00`).
pub const DROP_RADIUS: f32 = 5.0;
/// Ticks a rain drop falls: (cloud height + 30) / its speed (`FUN_14026b480`).
pub const DROP_FALL_EXTRA: f32 = 30.0;
/// Rain-cloud records the table does not carry the hidden fields for (test
/// fixtures): the nine real ones share these.
pub const CLOUD_SPREAD: f32 = 30.0;
pub const CLOUD_FOLLOW: f32 = 15.0;
pub const CLOUD_DROP_SPEED: f32 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// At the target's feet (Hole, Rain).
    Target,
    /// At the caster's feet (mines, Photon Wave).
    Caster,
}

/// Which handler an area is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hole,
    RainColumn,
    RainCloud,
    Mine,
    /// "Explodes around self": a blast at once.
    Wave,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trigger {
    /// Goes off when the countdown ends (Hole, Rain, Timer Mine, Photon Wave).
    Timer,
    /// Armed when the countdown ends, then goes off when anyone's centre comes
    /// within `radius` (Sensor Mine: "Detonates when anyone enters blast area").
    Sensor { radius: f32 },
    /// Goes off when its owner presses a skill button ("Press once to set and
    /// again to detonate"); that press is used up.
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaSpec {
    pub kind: Kind,
    pub place: Place,
    pub trigger: Trigger,
    /// Program ticks of warning (Hole, Rain column), of waiting for the first
    /// drop (cloud), or a mine's fuse.
    pub warn: u16,
    /// Program ticks the Hole / Rain column is active; the cloud's life beyond `warn`.
    pub active: u16,
    /// Hit radius (Hole, column), blast radius (mines, wave), drop spread (cloud).
    pub radius: f32,
    /// A Rain column's height.
    pub height: f32,
    /// Times it strikes (Rain drops) and program ticks between them.
    pub pulses: u16,
    pub every: u16,
    /// Rain cloud: height over the floor point, follow speed, drop speed (a
    /// tick); the drop spread is `radius`.
    pub cloud_height: f32,
    pub follow: f32,
    pub drop_speed: f32,
    /// Only Shelter stops it (it comes from above or below, not from in front).
    pub from_nowhere: bool,
    /// Something overhead protects the target (Rain).
    pub needs_open_sky: bool,
    /// Only reaches a target standing on the floor (Hole).
    pub ground_only: bool,
}

pub fn from_def(def: &SkillDef) -> Option<AreaSpec> {
    let base = AreaSpec {
        kind: Kind::Hole,
        place: Place::Target,
        trigger: Trigger::Timer,
        warn: 0,
        active: 0,
        radius: DEFAULT_RADIUS,
        height: 0.0,
        pulses: 1,
        every: 0,
        cloud_height: 0.0,
        follow: 0.0,
        drop_speed: 0.0,
        from_nowhere: false,
        needs_open_sky: false,
        ground_only: false,
    };
    let dist = |k: usize| {
        let r = def.param(k) as f32 * TABLE_DIST;
        if r > 0.0 { r } else { DEFAULT_RADIUS }
    };
    let ticks = |k: usize| def.param(k).max(0) as u16;
    Some(match def.motion() {
        // [Hole]: s64 warning, s65 active, s66 radius (300 = 30 u).
        motion::HOLE_A | motion::HOLE_B => AreaSpec { kind: Kind::Hole, warn: ticks(0), active: ticks(1), radius: dist(2), from_nowhere: true, ground_only: true, ..base },
        // [Rain] column: s64 warning, s65 active, s66 height / 10, s68 radius.
        motion::RAIN_A => AreaSpec { kind: Kind::RainColumn, warn: ticks(0), active: ticks(1), height: def.param(2).max(0) as f32 * 10.0, radius: dist(4), from_nowhere: true, needs_open_sky: true, ..base },
        // [Rain] cloud: s64 first drop, s65 life beyond it, s66 drops, s67 gap,
        // s69 height, s70 spread, s71 follow speed, s72 drop speed.
        motion::RAIN_B => {
            let hidden = |k: usize, fallback: f32| if def.s70_73[k] > 0 { def.s70_73[k] as f32 } else { fallback };
            AreaSpec {
                kind: Kind::RainCloud,
                warn: ticks(0),
                active: ticks(1),
                radius: hidden(0, CLOUD_SPREAD * 10.0) * TABLE_DIST,
                pulses: def.param(2).max(1) as u16,
                every: ticks(3) + 1,
                cloud_height: def.param(5).max(0) as f32 * TABLE_DIST,
                follow: hidden(1, CLOUD_FOLLOW) * SPEED_UNIT,
                drop_speed: hidden(2, CLOUD_DROP_SPEED) * SPEED_UNIT,
                from_nowhere: true,
                needs_open_sky: true,
                ..base
            }
        }
        // Mines: s64 kind (0 timer, 1 sensor, 2 remote), s65 sensor radius, s66 fuse
        // (Timer Mine 150 = "5 sec"), s67 blast radius (500 = 50 u).
        motion::MINE => AreaSpec {
            kind: Kind::Mine,
            place: Place::Caster,
            trigger: match def.param(0) {
                1 => Trigger::Sensor { radius: dist(1) },
                2 => Trigger::Remote,
                _ => Trigger::Timer,
            },
            warn: ticks(2),
            radius: dist(3),
            ..base
        },
        // Explodes around self: s64 radius (Photon Wave 400 = 40 u, Photon Burst 500).
        motion::AROUND_SELF => AreaSpec { kind: Kind::Wave, place: Place::Caster, radius: dist(0), ..base },
        _ => return None,
    })
}

/// A blast's radius `life` program ticks before it ends: 0 on its first tick,
/// growing linearly to `max` on its last (`FUN_14026ae10`).
pub fn blast_radius(max: f32, life: u16) -> f32 {
    if life < BLAST_TICKS + 1 {
        (BLAST_TICKS - life.min(BLAST_TICKS)) as f32 * max / BLAST_TICKS as f32
    } else {
        max
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Capsule, SkillDef};
    use pd_formats::skills::field;

    fn def(motion: i16, p: &[i16], hidden: [i16; 4]) -> SkillDef {
        let mut d = SkillDef::aura_particle();
        (d.id, d.capsule) = (99, Capsule::Attack);
        d.raw[field::MOTION] = motion;
        for (k, v) in p.iter().enumerate() {
            d.raw[field::MOTION_PARAMS + k] = *v;
        }
        d.s70_73 = hidden;
        d
    }

    /// Blast radius: 0 on the first tick, `max` on the last, linear between
    /// (`FUN_14026ae10`: (30 - life) x max / 30).
    #[test]
    fn blasts_grow_over_thirty_ticks() {
        assert_eq!(blast_radius(30.0, 30), 0.0);
        assert!((blast_radius(30.0, 15) - 15.0).abs() < 1e-5);
        assert!((blast_radius(30.0, 0) - 30.0).abs() < 1e-5);
    }

    #[test]
    fn specs_read_the_documented_fields() {
        let hole = from_def(&def(16, &[30, 10, 300], [0; 4])).unwrap();
        assert_eq!((hole.kind, hole.warn, hole.active), (Kind::Hole, 30, 10));
        assert!((hole.radius - 30.0).abs() < 1e-5 && hole.ground_only);
        let col = from_def(&def(7, &[30, 10, 6, 0, 300], [0; 4])).unwrap();
        assert_eq!((col.kind, col.height, col.radius), (Kind::RainColumn, 60.0, 30.0));
        // The nine motion-20 rains: 30, 30, 6, 5, 0, 1000 + 300, 15, 90, 0.
        let cloud = from_def(&def(20, &[30, 30, 6, 5, 0, 1000], [300, 15, 90, 0])).unwrap();
        assert_eq!((cloud.warn, cloud.active, cloud.pulses, cloud.every), (30, 30, 6, 6));
        assert!((cloud.cloud_height - 100.0).abs() < 1e-4 && (cloud.radius - 30.0).abs() < 1e-4);
        assert!((cloud.follow - 15.0 / 10.8).abs() < 1e-4 && (cloud.drop_speed - 90.0 / 10.8).abs() < 1e-4);
        let sensor = from_def(&def(8, &[1, 300, 60, 500], [0; 4])).unwrap();
        assert_eq!((sensor.warn, sensor.trigger), (60, Trigger::Sensor { radius: 30.0 }));
        assert!((sensor.radius - 50.0).abs() < 1e-5);
        let wave = from_def(&def(9, &[400], [0; 4])).unwrap();
        assert_eq!((wave.kind, wave.radius), (Kind::Wave, 40.0));
    }
}
