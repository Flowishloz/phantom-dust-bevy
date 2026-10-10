//! A fighter's body, from the character table (`setup/charaset/charaset_en.dat`).
//!
//! The program loads every record's four size fields in `FUN_14017d300` and copies
//! them onto the fighter in `FUN_14024e190` (`0x14024e190` lines `+0x12e98..+0x12eac`):
//!
//! | record | fighter | value (playable) | used by |
//! |---|---|---|---|
//! | `+0x0C / 10` | `+0x12ea0` height | Hero 18 u, Edgar / pH / Sammah 19, Meister / Cuff / JD 20 | wall sweep, body-vs-body |
//! | `+0x0E / 10` | `+0x12ea4` **contact size** | 10 | body-vs-body push-apart, wall sweep |
//! | `+0x10 / 10` | `+0x12e9c` hit radius (copied to `+0x130`) | 5 | every hit test (`moves::HIT_BODY_RADIUS`) |
//! | `+0x12 / 10` | `+0x12e98` centre height | 9 | aim point (`moves::HIT_BODY_CENTRE`) |
//!
//! Both movement users halve the contact size (`DAT_1402f7d40` = 0.5):
//! * fighter against fighter, `FUN_140250780` (vtable slot `0x70`, called first in
//!   `FUN_140254590` every tick): the horizontal gap must reach
//!   `size_a * 0.5 + size_b * 0.5` (= 10.0 for any two playable fighters); a pair closer than that
//!   (and with the two `height * 0.5` centres closer than `(h_a + h_b) * 0.5` in height) is pushed
//!   apart along the line between them, half the overlap each (the other fighter's `+0x34` is 1 in
//!   a match; with 0 only the first would move, by all of it);
//! * fighter against the stage, `FUN_140255020` -> `FUN_1401959c0`: the body is a vertical capsule
//!   of three spheres, radius `param_5 / param_9` with `param_5 = height * 0.5` and
//!   `param_9 = height / size` (`0x140255102 divss xmm14, [rax + 0x12ea4]`), i.e. `size * 0.5`.
//!
//! So the radius our walls and spacing use is `contact / 2`, per character. It used to be a
//! first guess of 4.5 (each body 9.0 from the other, the take 10.0: MEASURED in Swift Kick and
//! Psycho Knife by the shotclass lane).
//!
//! The height is used by the push-apart's vertical test only. Our wall test is a cylinder whose top
//! stays `sim::BODY_HEIGHT` (a model parameter, not the program's three spheres: three sample heights,
//! so feeding the per-character height into it would move where a sloped edge pushes, with no read
//! behind that part).

/// One character's size, in game units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Record `+0x0C / 10`.
    pub height: f32,
    /// Record `+0x0E / 10`; half of it is the radius in both movement tests.
    pub contact: f32,
    /// Where the program's target point sits against the fighter's position, in the fighter's own
    /// frame as the program has it: `(forward, right)` units (see `target_offset`). `(0, 0)` = not
    /// measured for this character.
    pub lock: (f32, f32),
    /// Height of the target point above the feet: the y of the same BODY1 bone matrix (`FUN_1402513a0` copies
    /// its x / y / z into the collider point `+0x60..+0x68`). MEASURED, see `point_height`.
    pub point_height: f32,
}

impl Body {
    /// The first playable record (`pc00a`), for a player whose character is not set (tests).
    pub const DEFAULT: Body = Body { height: 18.0, contact: 10.0, lock: (0.0, 0.0), point_height: crate::moves::HIT_BODY_CENTRE };

    /// The radius a fighter occupies against walls and other fighters (`+0x12ea4 * 0.5`).
    pub fn radius(self) -> f32 {
        self.contact * 0.5
    }

    /// The radius the stage's walls push the body out by: the same half contact size (`FUN_140255020`).
    pub fn wall_radius(self) -> f32 {
        self.radius()
    }

    /// The target point's offset from the fighter's position, in sim space (z mirrored, so the program's
    /// right axis turns into minus ours): `forward * (sin y, cos y) - right * (cos y, -sin y)` for a
    /// fighter facing `yaw`, on the ground plane.
    pub fn target_offset(self, yaw: f32) -> glam::Vec3 {
        let (s, c) = yaw.sin_cos();
        let (f, r) = self.lock;
        glam::Vec3::new(f * s - r * c, 0.0, f * c + r * s)
    }

    /// The record for a model folder (`pc01a`, `enm03a`, ...); `None` if the table has no such model.
    pub fn of(model: &str) -> Option<Body> {
        CHARASET.iter().find(|r| r.0 == model).map(|r| Body { height: f32::from(r.1) / 10.0, contact: f32::from(r.2) / 10.0, lock: lock_offset(r.0), point_height: point_height(r.0) })
    }
}

/// The target point offset (`Body::lock`), MEASURED, in the program's frame (forward = (sin f, cos f),
/// right = (cos f, -sin f), f = the facing): every object test, the lock-on bearing, the melee dash and
/// the knock-back direction read the entity point `+0x60` / `+0x68`, which `FUN_1402513a0` fills with
/// the world x / z of the fighter's BODY1 bone matrix (the table of five bone pointers at fighter
/// `+0x13db0` is built by `FUN_14025d380` from the hashes at `0x1402dcdd8`: LHAND, RHAND, BODY1 =
/// `+0x13dc0`, ALPHA, HEAD), not the fighter position `+0x12e40`. The distance is the idle pose's;
/// fitted over the recorded takes (the lock-on facing of a fighter standing still points at this point:
/// 265 takes for pc01a, 0.58 deg RMS, median 0.11; 43 takes with pc00a as the target, 0.10 deg RMS).
/// Other characters: not measured (need their takes or the BODY1 bone through the idle clip), so 0.
fn lock_offset(model: &str) -> (f32, f32) {
    match model {
        "pc00a" => (-1.96, -0.50),
        "pc01a" => (-2.76, -1.74),
        _ => (0.0, 0.0),
    }
}

/// The target point's height above the feet (`Body::point_height`): the BODY1 bone's world y in the idle clip
/// (WAIT0 frames 0-20, root joint at its bind translation, the program's 30 fps pose; the same forward
/// kinematics as `pd_formats::character::hand_paths`), MEASURED 2026-10-09 from each model's `<id>0.alr`. It
/// varies by under 0.3 u through the clip (pc00a 9.45..9.68, pc01a 11.88..11.89), so one value per model.
/// Checked against the film: `309_Piece_of_Wind` (pc01a the target) clears the class-27 box top and lands
/// 23 program ticks after the fire on class 6 like the original only with a target point at least ~9.7 u up
/// (the fire matrix pitch, `moves::Launch::pitch`); 9 (our old body centre) clips the box at tick 13.
/// UNCONFIRMED: the same FK does not reproduce the MEASURED x / z offset (`lock_offset`), so the bone's world
/// matrix has a factor we do not read; a vertical part of it would move these heights. Other models (the
/// enemy rows): the charaset centre height 9.
fn point_height(model: &str) -> f32 {
    match model {
        "pc00a" | "pc04a" | "pc07a" => 9.45,
        "pc01a" | "pc06a" | "pc10a" => 11.89,
        "pc02a" => 10.89,
        "pc03a" => 9.55,
        "pc05a" | "pc09a" => 8.87,
        "pc08a" => 11.31,
        _ => crate::moves::HIT_BODY_CENTRE,
    }
}

/// Every distinct record of `charaset_en.dat`: model, `+0x0C`, `+0x0E` (raw u16, tenths of a unit).
/// Read from the file 2026-10-03; `the_table_equals_the_character_file` checks it when the game
/// files are on the machine.
const CHARASET: [(&str, u16, u16); 44] = [
    ("pc00a", 180, 100),
    ("pc01a", 190, 100),
    ("pc02a", 180, 100),
    ("pc03a", 200, 100),
    ("pc04a", 180, 100),
    ("pc05a", 200, 100),
    ("pc06a", 190, 100),
    ("pc07a", 180, 100),
    ("pc08a", 180, 100),
    ("pc09a", 200, 100),
    ("pc10a", 190, 100),
    ("enm08a", 200, 100),
    ("enm09a", 200, 100),
    ("enm10a", 200, 100),
    ("enm11a", 200, 100),
    ("enm00a", 180, 100),
    ("enm01a", 200, 100),
    ("enm02a", 200, 100),
    ("enm03a", 200, 100),
    ("enm04a", 285, 100),
    ("enm05a", 200, 100),
    ("enm06a", 240, 100),
    ("enm07a", 260, 100),
    ("enm12a", 200, 100),
    ("enm13a", 210, 100),
    ("enm14a", 220, 120),
    ("enm15a", 270, 100),
    ("evpc00a", 180, 100),
    ("npc00a", 120, 100),
    ("npc01a", 180, 100),
    ("npc02a", 190, 100),
    ("npc03a", 120, 100),
    ("npc04a", 180, 100),
    ("npc05a", 180, 100),
    ("npc06a", 180, 100),
    ("npc07a", 180, 100),
    ("npc08a", 200, 100),
    ("npc09a", 200, 100),
    ("npc10a", 160, 100),
    ("npc11a", 160, 100),
    ("npc12a", 170, 100),
    ("npc13a", 140, 100),
    ("boss03", 200, 100),
    ("boss03b", 200, 100),
];

#[cfg(test)]
mod tests {
    use super::*;
    use pd_formats::charaset;

    fn charaset_file() -> Option<Vec<u8>> {
        let rel = "game_files/Assets/Data/setup/charaset/charaset_en.dat";
        let mut roots: Vec<std::path::PathBuf> = std::env::var_os("PD_GAME_FILES").map(|p| vec![std::path::PathBuf::from(p)]).unwrap_or_default();
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for _ in 0..6 {
            roots.push(dir.join(rel));
            if !dir.pop() {
                break;
            }
        }
        roots.iter().find_map(|p| std::fs::read(p).ok())
    }

    #[test]
    fn the_playable_fighters_are_ten_across_and_eighteen_to_twenty_tall() {
        let heights = [180, 190, 180, 200, 180, 200, 190, 180, 180, 200, 190];
        for (k, h) in heights.iter().enumerate() {
            let b = Body::of(&format!("pc{k:02}a")).unwrap();
            assert_eq!((b.height, b.contact, b.radius()), (*h as f32 / 10.0, 10.0, 5.0), "pc{k:02}a");
        }
        assert_eq!(Body::of("nobody"), None);
    }

    /// With the game files on this machine (`PD_GAME_FILES` overrides the search): every record's
    /// height and contact size are the table's, and the playable fighters' hit radius and centre
    /// height are the single values `moves` uses for everyone (5 and 9).
    #[test]
    fn the_table_equals_the_character_file() {
        let Some(data) = charaset_file() else {
            eprintln!("skipped: charaset_en.dat not found (set PD_GAME_FILES)");
            return;
        };
        let count = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let u16_at = |o: usize| u16::from_le_bytes(data[o..o + 2].try_into().unwrap());
        let entries = charaset::entries(&data).unwrap();
        assert_eq!(entries.len(), count);
        for (i, e) in entries.iter().enumerate() {
            let rec = 0x10 + i * charaset::RECORD;
            let row = CHARASET.iter().find(|r| r.0 == e.model).unwrap_or_else(|| panic!("{} missing from the table", e.model));
            assert_eq!((row.1, row.2), (u16_at(rec + 0x0C), u16_at(rec + 0x0E)), "{}", e.model);
            if e.model.starts_with("pc") && e.model.len() == 5 {
                assert_eq!((u16_at(rec + 0x10), u16_at(rec + 0x12)), (50, 90), "{}: hit radius 5, centre 9", e.model);
            }
        }
        assert_eq!(crate::moves::HIT_BODY_RADIUS, 5.0);
        assert_eq!(crate::moves::HIT_BODY_CENTRE, 9.0);
    }
}
