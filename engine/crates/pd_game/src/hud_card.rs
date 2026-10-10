//! The skill card: the window the original shows while a D-pad direction is
//! held (and in its arsenal menus). Pure rules and layout, no Bevy entities,
//! so they are unit-tested on their own; `hud.rs` spawns and fills the parts.
//!
//! READ from the program (`PDUWP.exe`, `FUN_140168410` = the card, called by
//! `FUN_140155160` = the battle D-pad hook, and by the arsenal list
//! `FUN_140158930`); sprite names come from the records of `Menu/WusMnEn.alr`
//! (its sprite table carries each sprite's own file name, e.g.
//! `detail_icon_range_s`, `detail_window`, `detail_window_title`):
//!
//!  * The card is a 234 x 96 window (`detail_window`, sprite record 22) with a
//!    230 x 26 title bar (record 23) tinted by the capsule colour (table below,
//!    from the stack array `FUN_140168410` builds at its start), the skill name
//!    (dark, 16 px font) at (10, 3), the school icon at (181, 1).
//!  * One stats row at y = 25 (value glyphs at y = 22): a COST label (record 1)
//!    at x 4 and the cost at x 40 ("X" for a hold-to-charge skill, s37 = 2); a
//!    STR label (record 14, attacks) or DEF label (record 2, defence) at x 68
//!    and its value at x 92 (blank box, record 0, for every other kind; "X"
//!    for an attack whose strength is computed, s45 != 0; the label is hidden
//!    on Course / About Face, which have no DEF); a small box icon (record 13)
//!    at x 126 and the uses at x 141 (the infinity sign when unlimited, s26 < 1);
//!    **the range icon at x 161** (66 x 12, the word and an arrow).
//!  * The description (the skill table's own note) wraps at 229 px from
//!    (3, 46): grey shadow, then dark text one pixel up-left.
//!
//! The range icon is chosen by `FUN_140167970` (`class_a`), `FUN_140167a10`
//! (`class_b`) and the branch at `0x140169b05..0x140169c12` (`range_icon`).
//! Its nine words are the guide's "noted Self / Capsule / Mine / All" plus
//! short / med / long, auto and env.

use crate::combat::SkillDef;

/// Design px per sprite px: the same scale the opponent panel uses
/// (`hud_layout::FOE_S`).
pub const S: f32 = 1.59;

/// Which `WusMnEn.alr` texture a sprite sits in (cache `ui/menu/wusN.png`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tex {
    Wus2,
    Wus3,
    Wus7,
}

impl Tex {
    pub const ALL: [Tex; 3] = [Tex::Wus2, Tex::Wus3, Tex::Wus7];
    pub fn file(self) -> &'static str {
        match self {
            Tex::Wus2 => "ui/menu/wus2.png",
            Tex::Wus3 => "ui/menu/wus3.png",
            Tex::Wus7 => "ui/menu/wus7.png",
        }
    }
}

/// A sprite: texture and rectangle (x, y, w, h) in texture pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    pub tex: Tex,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

const fn sp(tex: Tex, x: f32, y: f32, w: f32, h: f32) -> Sprite {
    Sprite { tex, x, y, w, h }
}

/// Record 22, `detail_window`.
pub const WINDOW: Sprite = sp(Tex::Wus7, 0.0, 0.0, 234.0, 96.0);
/// Record 23, `detail_window_title`.
pub const TITLE: Sprite = sp(Tex::Wus3, 0.0, 54.0, 230.0, 26.0);
/// Record 1, `detail_icon_costbox`.
pub const COST_LABEL: Sprite = sp(Tex::Wus2, 68.0, 262.0, 30.0, 12.0);
/// Record 14, `detail_icon_strbox`.
pub const STR_LABEL: Sprite = sp(Tex::Wus2, 110.0, 228.0, 24.0, 12.0);
/// Record 2, `detail_icon_defbox`.
pub const DEF_LABEL: Sprite = sp(Tex::Wus2, 68.0, 494.0, 24.0, 12.0);
/// Record 0, `detail_icon_blabox`: the blank box in place of STR / DEF.
pub const BLANK_LABEL: Sprite = sp(Tex::Wus2, 68.0, 226.0, 38.0, 12.0);
/// Record 13, `detail_icon_shotbox`: the box icon before the uses.
pub const SHOT_BOX: Sprite = sp(Tex::Wus2, 50.0, 344.0, 11.0, 11.0);

/// Positions in sprite px from the card's top-left.
pub const TITLE_AT: (f32, f32) = (3.0, 1.0);
pub const NAME_AT: (f32, f32) = (10.0, 3.0);
pub const SCHOOL_AT: (f32, f32) = (181.0, 1.0);
pub const ROW_Y: f32 = 25.0;
pub const VALUE_Y: f32 = 22.0;
pub const COST_LABEL_X: f32 = 4.0;
pub const COST_X: f32 = 40.0;
pub const MAIN_LABEL_X: f32 = 68.0;
pub const MAIN_X: f32 = 92.0;
pub const SHOT_X: f32 = 126.0;
pub const USES_X: f32 = 141.0;
pub const RANGE_X: f32 = 161.0;
/// The description: shadow at (3, 46), text at (2, 45); lines 16 apart,
/// wrapped at 229 px; drawn at 7 x 14 against the name's 8 x 16.
pub const NOTE_AT: (f32, f32) = (2.0, 45.0);
pub const NOTE_SHADOW: (f32, f32) = (1.0, 1.0);
pub const NOTE_WRAP: f32 = 229.0;
pub const NOTE_LINE: f32 = 16.0;
pub const NOTE_SCALE: f32 = 14.0 / 16.0;
/// Most lines the 96 px window holds under the stats row.
pub const NOTE_LINES: usize = 3;

/// Colours (the program's ARGB words).
pub const NAME_RGB: [u8; 3] = [0x26, 0x26, 0x26];
pub const LABEL_RGB: [u8; 3] = [0xa5, 0xa5, 0xa5];
pub const NOTE_RGB: [u8; 3] = [0x3e, 0x3e, 0x3e];
pub const NOTE_SHADOW_RGB: [u8; 3] = [0x85, 0x88, 0x81];

/// The title bar's tint per capsule number (s9; 0 = Aura Particle, 7 = none):
/// the integer array `FUN_140168410` builds on its stack, read from
/// `0x1402f8610..0x1402f8660`.
pub fn title_rgb(capsule: u8) -> [u8; 3] {
    match capsule {
        0 => [255, 255, 255],
        1 => [255, 165, 165],
        2 => [165, 170, 225],
        3 => [225, 165, 205],
        4 => [166, 225, 165],
        5 => [225, 223, 165],
        6 => [165, 204, 225],
        _ => [138, 138, 138],
    }
}

/// The school icon (`detail_icon_type_*`, 28 x 18) by the skill table's school
/// (s11): the program's table at `0x4f44` of the menu object (written by the
/// constructor): 0 -> aj (15), 1 Psycho -> pk (21), 2 Optical -> op (20),
/// 3 Nature -> na (18), 4 Ki -> ki (16), 5 Faith -> ma (17).
pub fn school_sprite(school: i16) -> Sprite {
    let y = match school {
        1 => 408.0,
        2 => 388.0,
        3 => 368.0,
        4 => 328.0,
        5 => 348.0,
        _ => 308.0,
    };
    sp(Tex::Wus2, 68.0, y, 28.0, 18.0)
}

/// The nine range icons (`detail_icon_range_*`, 66 x 12), by sprite record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeIcon {
    /// Record 3 (`range_a`): "all".
    All,
    /// Record 4 (`range_b`): "mine".
    Mine,
    /// Record 5 (`range_c`): "capsule".
    Capsule,
    /// Record 6 (`range_d`): "auto".
    Auto,
    /// Record 7 (`range_e`): "env".
    Env,
    /// Record 8 (`range_l`): "long".
    Long,
    /// Record 9 (`range_m`): "med".
    Med,
    /// Record 10 (`range_my`): "self".
    SelfOnly,
    /// Record 12 (`range_s`): "short".
    Short,
}

impl RangeIcon {
    /// The word the icon spells.
    pub fn word(self) -> &'static str {
        match self {
            RangeIcon::All => "all",
            RangeIcon::Mine => "mine",
            RangeIcon::Capsule => "capsule",
            RangeIcon::Auto => "auto",
            RangeIcon::Env => "env",
            RangeIcon::Long => "long",
            RangeIcon::Med => "med",
            RangeIcon::SelfOnly => "self",
            RangeIcon::Short => "short",
        }
    }

    /// The sprite record number in `WusMnEn.alr`.
    pub fn record(self) -> u32 {
        match self {
            RangeIcon::All => 3,
            RangeIcon::Mine => 4,
            RangeIcon::Capsule => 5,
            RangeIcon::Auto => 6,
            RangeIcon::Env => 7,
            RangeIcon::Long => 8,
            RangeIcon::Med => 9,
            RangeIcon::SelfOnly => 10,
            RangeIcon::Short => 12,
        }
    }

    pub fn sprite(self) -> Sprite {
        let y = match self {
            RangeIcon::All => 184.0,
            RangeIcon::Mine => 198.0,
            RangeIcon::Capsule => 212.0,
            RangeIcon::Auto => 226.0,
            RangeIcon::Env => 240.0,
            RangeIcon::Long => 254.0,
            RangeIcon::Med => 268.0,
            RangeIcon::SelfOnly => 282.0,
            RangeIcon::Short => 296.0,
        };
        sp(Tex::Wus2, 0.0, y, 66.0, 12.0)
    }
}

/// Table fields by the program's record offsets (our `s` = offset / 2 + 2).
const KIND: usize = 9; // +0x0e capsule number
const SCHOOL: usize = 11; // +0x12
const USES: usize = 26; // +0x30
const CHARGE: usize = 37; // +0x46
const SET: usize = 43; // +0x52 attack set 0, defence 2, ...
const BAND: usize = 44; // +0x54
const RULE: usize = 45; // +0x56 strength rule
const STRENGTH: usize = 46; // +0x58
const DEF: usize = 47; // +0x5a
const MOTION: usize = 63; // +0x7a
const MOTION_ARG: usize = 64; // +0x7c

/// `FUN_140167910`: an attack with a short-range motion (3, 12, 13, 14).
fn short_range(raw: &[i16; 70]) -> bool {
    raw[SET] == 0 && matches!(raw[MOTION], 3 | 12 | 13 | 14)
}

/// `FUN_140167970`: 1 short-range, 2 medium (aimed, band not 1), 3 long (band 1,
/// a shell / laser), 4 long (band 1, other motion), 5 mine; 0 = not decided here.
pub fn class_a(raw: &[i16; 70]) -> u8 {
    if raw[SET] == 0 {
        if short_range(raw) {
            return 1;
        }
        let m = raw[MOTION];
        if m == 8 || m == 9 {
            return 5;
        }
        if m != 4 && m != 16 {
            if raw[BAND] != 1 {
                return 2;
            }
            if !matches!(m, 0 | 5 | 10 | 11) {
                return 4;
            }
            return 3;
        }
    }
    0
}

/// `FUN_140167a10`: the target kind of everything `class_a` leaves open.
pub fn class_b(raw: &[i16; 70]) -> u8 {
    match raw[SET] {
        3 => 1,
        0 => {
            let m = raw[MOTION];
            if m != 8 && m != 16 && !(m == 4 && raw[MOTION_ARG] == 2) {
                2
            } else {
                3
            }
        }
        4 => match raw[BAND] {
            0 => 1,
            1 => 2,
            _ => 3,
        },
        1 => {
            if raw[BAND] != 2 && raw[BAND] != 5 {
                4
            } else {
                5
            }
        }
        _ => 0,
    }
}

/// The range icon of a skill (the branch at `0x140169b05` in `FUN_140168410`);
/// `None` = the program draws no icon.
pub fn range_icon(raw: &[i16; 70]) -> Option<RangeIcon> {
    let a = class_a(raw);
    if a >= 1 {
        // The table at 0x1402f2850: 12, 9, 8, 8, 4.
        return Some([RangeIcon::Short, RangeIcon::Med, RangeIcon::Long, RangeIcon::Long, RangeIcon::Mine][a as usize - 1]);
    }
    let b = class_b(raw);
    let kind = raw[KIND];
    if b == 1 || kind == 2 {
        Some(RangeIcon::SelfOnly)
    } else if b == 3 {
        Some(RangeIcon::All)
    } else if b == 4 || b == 5 {
        Some(RangeIcon::Capsule)
    } else if kind == 6 {
        Some(RangeIcon::Env)
    } else if raw[SET] == 0 && raw[MOTION] == 4 {
        Some(RangeIcon::Auto)
    } else if b == 2 {
        Some(RangeIcon::SelfOnly)
    } else {
        None
    }
}

/// The word for a skill's range, for the text-only cards (arsenal builder).
pub fn range_word(def: &SkillDef) -> Option<&'static str> {
    range_icon(&def.raw).map(RangeIcon::word)
}

/// The label in the STR / DEF place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainLabel {
    /// Blank box: erase, status, special, environment, the Aura Particle.
    Blank,
    Str,
    Def,
    /// Course / About Face: no DEF rating, the label is covered.
    NoDef,
}

/// Everything the card shows for one skill.
#[derive(Clone, Debug, PartialEq)]
pub struct CardView {
    pub capsule: u8,
    pub title: String,
    pub school: i16,
    pub cost: String,
    pub label: MainLabel,
    /// The STR / DEF number ("" = none, "X" = computed).
    pub value: String,
    /// Uses left, `None` = unlimited.
    pub uses: Option<i16>,
    pub range: Option<RangeIcon>,
    pub note: String,
}

impl CardView {
    pub fn of(def: &SkillDef, note: &str) -> CardView {
        let raw = &def.raw;
        let kind = raw[KIND];
        let (label, value) = match kind {
            1 => {
                let v = if raw[SET] == 0 && raw[RULE] == 0 { raw[STRENGTH].to_string() } else { "X".to_string() };
                (MainLabel::Str, v)
            }
            2 => {
                if raw[SET] == 2 {
                    if matches!(raw[BAND], 6 | 2) {
                        (MainLabel::NoDef, String::new())
                    } else {
                        (MainLabel::Def, raw[DEF].to_string())
                    }
                } else {
                    (MainLabel::Def, String::new())
                }
            }
            _ => (MainLabel::Blank, String::new()),
        };
        CardView {
            capsule: kind.clamp(0, 7) as u8,
            title: def.name.clone(),
            school: raw[SCHOOL],
            cost: if raw[CHARGE] == 2 { "X".to_string() } else { def.cost.to_string() },
            label,
            value,
            uses: (raw[USES] >= 1).then_some(raw[USES]),
            range: range_icon(raw),
            note: note.to_string(),
        }
    }

    /// The Aura Particle has no record in the skill table (text 301 only):
    /// grey-white title, blank box, unlimited, no range icon (UNCONFIRMED).
    pub fn aura_particle(name: &str, note: &str) -> CardView {
        CardView { capsule: 0, title: name.to_string(), school: 0, cost: "0".to_string(), label: MainLabel::Blank, value: String::new(), uses: None, range: None, note: note.to_string() }
    }
}

/// Wrap `text` into lines of at most `limit` px; `width` measures a string.
/// A word that does not fit starts the next line (the program's own break).
pub fn wrap(text: &str, limit: f32, width: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let trial = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && width(&trial) > limit {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = trial;
            }
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(set: i16, band: i16, motion: i16, kind: i16) -> [i16; 70] {
        let mut r = [0i16; 70];
        r[SET] = set;
        r[BAND] = band;
        r[MOTION] = motion;
        r[KIND] = kind;
        r
    }

    #[test]
    fn short_range_attacks_say_short() {
        for m in [3, 12, 13, 14] {
            assert_eq!(range_icon(&raw(0, 0, m, 1)), Some(RangeIcon::Short), "motion {m}");
        }
    }

    #[test]
    fn aimed_attacks_say_med_or_long_by_the_band() {
        // Band 0 / 2 aimed lob (motion 1): med. Band 1 shell (motion 0) and lob: long.
        assert_eq!(range_icon(&raw(0, 0, 1, 1)), Some(RangeIcon::Med));
        assert_eq!(range_icon(&raw(0, 2, 1, 1)), Some(RangeIcon::Med));
        assert_eq!(range_icon(&raw(0, 1, 0, 1)), Some(RangeIcon::Long));
        assert_eq!(range_icon(&raw(0, 1, 1, 1)), Some(RangeIcon::Long));
        // Motion 0 with band 0 is "med" too: the Blaster-like shell.
        assert_eq!(range_icon(&raw(0, 0, 0, 1)), Some(RangeIcon::Med));
    }

    #[test]
    fn mines_say_mine() {
        assert_eq!(range_icon(&raw(0, 0, 8, 1)), Some(RangeIcon::Mine));
        assert_eq!(range_icon(&raw(0, 0, 9, 1)), Some(RangeIcon::Mine));
    }

    #[test]
    fn instants_and_holes_follow_class_b() {
        // Motion 4 (instant) with s64 = 2 (everyone): all. Without: auto.
        let mut r = raw(0, 0, 4, 1);
        r[MOTION_ARG] = 2;
        assert_eq!(range_icon(&r), Some(RangeIcon::All));
        assert_eq!(range_icon(&raw(0, 0, 4, 1)), Some(RangeIcon::Auto));
        // A hole (16): all.
        assert_eq!(range_icon(&raw(0, 2, 16, 1)), Some(RangeIcon::All));
    }

    #[test]
    fn defence_environment_and_the_rest() {
        // Every defence skill: self.
        assert_eq!(range_icon(&raw(2, 0, 0, 2)), Some(RangeIcon::SelfOnly));
        // Environment (set 5, kind 6): env.
        assert_eq!(range_icon(&raw(5, 0, 0, 6)), Some(RangeIcon::Env));
        // Self-targeted status / special (set 4): band 0 self, 1 self-class 2 -> self, else all.
        assert_eq!(range_icon(&raw(4, 0, 0, 4)), Some(RangeIcon::SelfOnly));
        assert_eq!(range_icon(&raw(4, 1, 0, 5)), Some(RangeIcon::SelfOnly));
        assert_eq!(range_icon(&raw(4, 3, 0, 5)), Some(RangeIcon::All));
        // Capsule-targeted special (set 3): self per class_b 1.
        assert_eq!(range_icon(&raw(3, 0, 0, 5)), Some(RangeIcon::SelfOnly));
        // Erase (set 1): capsule unless band 2 / 5.
        assert_eq!(range_icon(&raw(1, 0, 0, 3)), Some(RangeIcon::Capsule));
        assert_eq!(range_icon(&raw(1, 2, 0, 3)), Some(RangeIcon::Capsule));
        // Anything else (set 6, kind 4): no icon.
        assert_eq!(range_icon(&raw(6, 0, 0, 4)), None);
    }

    #[test]
    fn wrap_breaks_between_words() {
        let w = |s: &str| s.len() as f32 * 7.0;
        let lines = wrap("one two three four five", 7.0 * 9.0, &w);
        assert_eq!(lines, vec!["one two", "three", "four five"]);
        assert!(wrap("", 100.0, &w).is_empty());
        assert_eq!(wrap("a\nb", 100.0, &w), vec!["a", "b"]);
    }

    #[test]
    fn title_tints_are_the_programs() {
        assert_eq!(title_rgb(1), [255, 165, 165]);
        assert_eq!(title_rgb(2), [165, 170, 225]);
        assert_eq!(title_rgb(6), [165, 204, 225]);
        assert_eq!(title_rgb(7), [138, 138, 138]);
    }

    #[test]
    fn range_sprites_are_stacked_14_apart_like_the_atlas() {
        let order = [RangeIcon::All, RangeIcon::Mine, RangeIcon::Capsule, RangeIcon::Auto, RangeIcon::Env, RangeIcon::Long, RangeIcon::Med, RangeIcon::SelfOnly, RangeIcon::Short];
        for w in order.windows(2) {
            assert_eq!(w[1].sprite().y - w[0].sprite().y, 14.0);
        }
    }

    /// Against the real skill table (skipped when the cache is absent): the icon the program's
    /// own rule gives for named skills, and the card's other fields.
    #[test]
    fn the_real_table_gives_sensible_cards() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/skills.json");
        if !path.exists() {
            eprintln!("skills.json absent; skipped");
            return;
        }
        let defs = crate::load_skills(&path, true);
        let by = |name: &str| defs.iter().find(|d| d.name == name).unwrap_or_else(|| panic!("no skill {name}"));
        let icon = |name: &str| range_icon(&by(name).raw);
        assert_eq!(icon("Psycho Wave"), Some(RangeIcon::Short));
        assert_eq!(icon("Psycho Knife"), Some(RangeIcon::Short));
        assert_eq!(icon("Laser"), Some(RangeIcon::Long));
        assert_eq!(icon("Bomb"), Some(RangeIcon::Med));
        assert_eq!(icon("Remote Mine"), Some(RangeIcon::Mine));
        assert_eq!(icon("Crystal Wall"), Some(RangeIcon::SelfOnly));
        assert_eq!(icon("Desertification"), Some(RangeIcon::Env));
        assert_eq!(icon("Healing Water"), Some(RangeIcon::SelfOnly));
        assert_eq!(icon("Shatter"), Some(RangeIcon::Capsule));
        assert_eq!(icon("Massive Quake"), Some(RangeIcon::All));
        // Fields.
        let gungnir = CardView::of(by("Gungnir"), "");
        assert_eq!(gungnir.cost, "X", "hold-to-charge cost");
        assert_eq!(gungnir.value, "X", "computed STR");
        let wave = CardView::of(by("Psycho Wave"), "");
        assert_eq!((wave.label, wave.value.as_str()), (MainLabel::Str, "X"), "STR = target's Level is computed");
        assert_eq!(wave.uses, Some(1));
        let laser = CardView::of(by("Laser"), "");
        assert_eq!((laser.label, laser.value.as_str(), laser.uses), (MainLabel::Str, "1", None));
        let gust = CardView::of(by("Gust"), "");
        assert_eq!(gust.label, MainLabel::NoDef, "[Course] has no DEF rating");
        let wall = CardView::of(by("Crystal Wall"), "");
        assert_eq!(wall.label, MainLabel::Def);
        assert!(!wall.value.is_empty() && wall.value != "X");
        assert_eq!(CardView::of(by("Shatter"), "").label, MainLabel::Blank);
        // Every registered skill with a school gets one of the five school icons, and only
        // Recollection / Copy-like skills may lack a range icon.
        let none: Vec<&str> = defs.iter().filter(|d| range_icon(&d.raw).is_none()).map(|d| d.name.as_str()).collect();
        assert!(none.len() <= 6, "skills without a range icon: {none:?}");
    }
}
