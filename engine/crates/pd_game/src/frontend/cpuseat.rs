//! What a CPU seat on the character-select screen is made of (lane `cpuseat`, decisions 2026-10-01):
//! strength = five grades, each one a row of the original's `Data/com/param/enemyparam.epa`; arsenal = the original's own
//! enemy decks (`Data/com/deck/DECK_*`) or any of the player's saved arsenals; character = picked on the seat like a human's.
//!
//! Pure data and rules, no Bevy: `setup.rs` draws and drives it, `seats::SeatKind::Cpu { epa_row }` carries the grade to the match.
//!
//! # The five grades (docs/FRONTEND.md "CPU seats" has the same text)
//! The file holds 82 rows (P0-AI 2.5, FORMATS-CPU 6). Laurence picked the **"nearest" ladder**, rows 60-64: target rule 4
//! (the nearest opponent), idle style 2 (camp at the target's start point), capsule strategy 0, think 30. Within the block the
//! four percentages never rise and the defence ones fall strictly, so the order weakest to strongest is read off the file:
//! attack / react-to-melee 66, 77, 77, 77, 88 and react-to-bullet / dodge 11, 22, 33, 44, 55 (wander % 15, 12, 9, 6, 3).
//! So the grades are rows **64, 63, 62, 61, 60** (grade 1 = row 64, the weakest; grade 5 = row 60, the strongest; grade 3 =
//! row 62 is the default). The target rule is whatever the grade's own row says (rule 4 for all five); there is no separate
//! targeting option. In a free-for-all "nearest" makes CPUs fight each other, not only the person. The rows are READ from the
//! file when it is there (the screen prints its numbers); the row numbers are fixed here and checked against the file by
//! `tests::the_five_grades_are_the_nearest_ladder_of_the_shipped_file`.
//! (Rows 0-4, the equal-percentage ladder with target rule 0, were the first pick and are superseded.)
//!
//! # The decks
//! The quests pair no deck with a row (rows 0-4 appear with 40+ different decks), and the school decks go with the story's
//! enemy models (quest character ids 11-26 = `enm00a`..), not with a versus fighter. So the default arsenal is one of the
//! five school decks the quests use most, `DECK_PSYCHO / OPTICAL / NATURE / KI / RELIGION` (15 skills, three copies of each
//! of five skills, the rest Aura Particles; the declared kind at +0x50 is 5). The player's saved arsenals follow them.
//! The original also has per-character decks (`DECK_EDGAR_0`, `DECK_JD_0`...; quest character id = the `pcNNa` number), not
//! offered here.

use super::setup::ArsenalOption;
use crate::combat::AURA_PARTICLE;
use std::path::Path;

/// Grades the screen offers.
pub const GRADES: usize = 5;
/// `enemyparam.epa` row of grade 1 (weakest) .. grade 5 (strongest).
pub const GRADE_ROWS: [u8; GRADES] = [64, 63, 62, 61, 60];
/// The grade a new CPU seat starts at: the middle one (row 62: attack 77 %, guard 33 %).
pub const DEFAULT_GRADE: u8 = 2;

/// The `epa` row (0..81) a grade plays with.
pub fn grade_row(grade: u8) -> u8 {
    GRADE_ROWS[(grade as usize).min(GRADES - 1)]
}

/// "GRADE 3".
pub fn grade_name(grade: u8) -> String {
    format!("GRADE {}", grade as usize % GRADES + 1)
}

/// One grade's numbers as the file gives them. The description line uses only the target rule since 2026-10-09 (item 6);
/// the percentages stay for the tests that check the five rows against the shipped file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct GradeInfo {
    pub row: u8,
    /// v30: 0 lowest slot, 1 highest slot, 2 lowest Health, 3 highest Health, 4 nearest, 5 farthest, 6 random.
    pub target_rule: i8,
    pub attack: i8,
    pub bullet: i8,
    pub melee: i8,
    pub dodge: i8,
    pub wander: i8,
}

/// The five grades from the file's bytes, `None` when the file isn't there or doesn't hold the rows.
pub fn load_grades(cache: &Path) -> Option<[GradeInfo; GRADES]> {
    let epa = pd_formats::epa::EnemyParams::parse(&crate::cpu::com_file(cache, "param/enemyparam.epa")?).ok()?;
    grades_from(&epa)
}

pub fn grades_from(epa: &pd_formats::epa::EnemyParams) -> Option<[GradeInfo; GRADES]> {
    let mut out = [GradeInfo { row: 0, target_rule: 0, attack: 0, bullet: 0, melee: 0, dodge: 0, wander: 0 }; GRADES];
    for (g, &row) in GRADE_ROWS.iter().enumerate() {
        let r = epa.row(row as usize)?;
        out[g] = GradeInfo { row, target_rule: r.target_rule(), attack: r.attack_pct(), bullet: r.react_bullet_pct(), melee: r.react_melee_pct(), dodge: r.dodge_pct(), wander: r.wander_pct() };
    }
    Some(out)
}

/// Who the CPU goes for, in words (v30 of its row).
pub fn target_rule_text(rule: i8) -> &'static str {
    match rule {
        0 => "the lowest-numbered opponent",
        1 => "the highest-numbered opponent",
        2 => "the weakest opponent",
        3 => "the healthiest opponent",
        4 => "the nearest opponent",
        5 => "the farthest opponent",
        6 => "a random opponent",
        _ => "an opponent",
    }
}

/// The description line of a grade: its row's numbers.
pub fn describe_grade(grade: u8, info: &GradeInfo) -> String {
    // Plain words, not the file's tuning numbers (Laurence, 2026-10-09: de-clutter; UI-DECLUTTER-PROPOSALS.md item 6).
    const WORDS: [&str; GRADES] = ["easiest", "easy", "average", "hard", "hardest"];
    let g = grade as usize % GRADES;
    format!("Grade {} of {GRADES}: {}. Goes for {}.", g + 1, WORDS[g], target_rule_text(info.target_rule))
}

/// The original's enemy decks a CPU can play: file under `Data/com/deck`, the name shown, and (the fallback when the file
/// isn't beside the cache) the skills read from it: 15 ids, three of each of five skills.
pub struct OriginalDeck {
    pub file: &'static str,
    pub name: &'static str,
    pub skills: [i16; 15],
}

/// In the order the quests use them (PSYCHO x20, KI x17, NATURE x15, OPTICAL x8, RELIGION x1 of the 331 `eneprog006` quest records).
pub const ORIGINAL_DECKS: [OriginalDeck; 5] = [
    OriginalDeck { file: "DECK_PSYCHO", name: "ENEMY PSYCHO", skills: [1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5] },
    OriginalDeck { file: "DECK_KI", name: "ENEMY KI", skills: [13, 13, 13, 14, 14, 14, 15, 15, 15, 16, 16, 16, 17, 17, 17] },
    OriginalDeck { file: "DECK_NATURE", name: "ENEMY NATURE", skills: [8, 8, 8, 9, 9, 9, 10, 10, 10, 11, 11, 11, 12, 12, 12] },
    OriginalDeck { file: "DECK_OPTICAL", name: "ENEMY OPTICAL", skills: [42, 42, 42, 43, 43, 43, 44, 44, 44, 45, 45, 45, 46, 46, 46] },
    OriginalDeck { file: "DECK_RELIGION", name: "ENEMY FAITH", skills: [26, 26, 26, 27, 27, 27, 28, 28, 28, 29, 29, 29, 30, 30, 30] },
];

/// A deck file's skills (30 u16 at +0x14, -1 = empty, which the match deals as an Aura Particle) and its declared kind
/// (u32 at +0x50, the program's deck struct image: `deck + 0x50` is the "Number of Schools", `arsenal::aura_handicap`).
pub fn parse_deck(bytes: &[u8]) -> Option<(Vec<i16>, u8)> {
    if bytes.len() < 0x54 {
        return None;
    }
    let skills = (0..crate::arsenal::ARSENAL_SIZE).map(|i| i16::from_le_bytes([bytes[0x14 + 2 * i], bytes[0x15 + 2 * i]])).map(|id| if id < 0 { AURA_PARTICLE } else { id }).collect();
    Some((skills, bytes[0x50]))
}

/// The original's enemy decks as arsenal options: the deck file when it is found (beside the cache, `PD_COM_DIR`, the game's
/// own data), else the skills read from it that are kept in `ORIGINAL_DECKS`.
pub fn original_decks(cache: &Path) -> Vec<ArsenalOption> {
    ORIGINAL_DECKS
        .iter()
        .map(|d| {
            let (skills, kind) = crate::cpu::com_file(cache, &format!("deck/{}", d.file)).and_then(|b| parse_deck(&b)).unwrap_or_else(|| {
                let mut s: Vec<i16> = d.skills.to_vec();
                s.resize(crate::arsenal::ARSENAL_SIZE, AURA_PARTICLE);
                (s, 5)
            });
            ArsenalOption { name: d.name.to_string(), skills, kind: Some(kind) }
        })
        .collect()
}

/// What a CPU seat has picked besides what every seat picks (character, colour, team).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuSeat {
    /// 0..5: `GRADE_ROWS` index.
    pub grade: u8,
    /// Index into the CPU arsenal list (the original's decks, then the player's saved arsenals).
    pub deck: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The shipped `Data/com` of this checkout, if it can be found (`PD_COM_DIR`, else the repository's `game_files`).
    fn com_dir() -> Option<PathBuf> {
        let from_env = std::env::var_os("PD_COM_DIR").map(PathBuf::from);
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../game_files/Assets/Data/com");
        from_env.into_iter().chain([repo]).find(|d| d.join("param/enemyparam.epa").is_file())
    }

    #[test]
    fn grades_run_weakest_to_strongest_over_rows_sixty_four_to_sixty() {
        assert_eq!(GRADE_ROWS, [64, 63, 62, 61, 60]);
        assert_eq!((grade_row(0), grade_row(2), grade_row(4)), (64, 62, 60));
        assert_eq!(grade_row(200), 60, "out of range clamps to the strongest");
        assert_eq!(grade_name(0), "GRADE 1");
        assert_eq!(grade_name(DEFAULT_GRADE), "GRADE 3");
    }

    #[test]
    fn the_five_grades_are_the_nearest_ladder_of_the_shipped_file() {
        let Some(dir) = com_dir() else {
            eprintln!("skipped: no Data/com beside this checkout (set PD_COM_DIR)");
            return;
        };
        let epa = pd_formats::epa::EnemyParams::parse(&std::fs::read(dir.join("param/enemyparam.epa")).unwrap()).unwrap();
        let g = grades_from(&epa).expect("rows 60-64 exist");
        // Never rising weakest to strongest, the defence ones strictly rising; wander falls; one target rule (4, nearest).
        let attack: Vec<i8> = g.iter().map(|i| i.attack).collect();
        let bullet: Vec<i8> = g.iter().map(|i| i.bullet).collect();
        assert_eq!(attack, [66, 77, 77, 77, 88]);
        assert_eq!(bullet, [11, 22, 33, 44, 55]);
        for i in &g {
            assert_eq!((i.melee, i.dodge), (i.attack, i.bullet), "row {}: melee = attack %, dodge = bullet %", i.row);
        }
        assert!(attack.windows(2).all(|w| w[0] <= w[1]) && bullet.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(g.iter().map(|i| i.wander).collect::<Vec<_>>(), [15, 12, 9, 6, 3]);
        assert!(g.iter().all(|i| i.target_rule == 4), "every grade carries its own row's rule: 4 here (nearest)");
        for row in GRADE_ROWS {
            let r = epa.row(row as usize).unwrap();
            assert_eq!((r.idle_style(), r.capsule_hunt(), r.think_interval()), (2, 0, 30), "row {row}");
        }
    }

    #[test]
    fn the_original_decks_match_their_files() {
        let decks = original_decks(Path::new("/nonexistent/cache"));
        assert_eq!(decks.len(), 5);
        assert!(decks.iter().all(|d| d.skills.len() == crate::arsenal::ARSENAL_SIZE && d.kind == Some(5)));
        assert_eq!(&decks[0].skills[..4], &[1, 1, 1, 2]);
        assert!(decks[0].skills[15..].iter().all(|&s| s == AURA_PARTICLE), "the empty half is Aura Particles");
        let Some(dir) = com_dir() else {
            eprintln!("skipped: no Data/com beside this checkout (set PD_COM_DIR)");
            return;
        };
        for (d, o) in ORIGINAL_DECKS.iter().zip(&decks) {
            let (skills, kind) = parse_deck(&std::fs::read(dir.join("deck").join(d.file)).unwrap()).unwrap();
            assert_eq!((&skills, kind), (o.skills.as_ref(), 5), "{}", d.file);
        }
    }

    #[test]
    fn a_short_or_missing_deck_file_is_not_a_deck() {
        assert!(parse_deck(&[0; 10]).is_none());
        let mut b = vec![0u8; 100];
        b[0x14] = 7;
        b[0x16] = 0xff;
        b[0x17] = 0xff;
        b[0x50] = 3;
        let (s, kind) = parse_deck(&b).unwrap();
        assert_eq!((s[0], s[1], kind), (7, AURA_PARTICLE, 3));
        assert_eq!(s.len(), 30);
    }

    #[test]
    fn a_grade_describes_itself_in_plain_words() {
        let i = GradeInfo { row: 62, target_rule: 4, attack: 77, bullet: 33, melee: 77, dodge: 33, wander: 9 };
        let t = describe_grade(2, &i);
        assert_eq!(t, "Grade 3 of 5: average. Goes for the nearest opponent.");
        assert!(describe_grade(4, &i).starts_with("Grade 5 of 5: hardest"));
    }
}
