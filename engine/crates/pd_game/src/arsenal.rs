//! Arsenal (deck) rules, shared by the arsenal builder (menus), the versus
//! setup and the sim.
//!
//! Rebuilt from the original's program (`PDUWP.exe`, decompile in
//! `~/.pdtools/re-out/decomp_all.c`) rather than from fan guides -- the
//! method of `docs/METHOD-FIDELITY.md`: read the rule, build it exactly,
//! cite the address. Every rule below says where it was read.
//!
//! The deck record (100 bytes, `FUN_14015afe0`): name[16] at +4, **30 skill
//! ids** (i16, -1 = empty) at +0x14, the **declared "Number of Schools"**
//! (`kind`, 1-5) at +0x50, computed rule flags at +0x52. An empty slot *is* an
//! Aura Particle (`FUN_14014f830` counts slot 0 as 30 minus the real skills;
//! `FUN_14015a800` compacts the real skills to the front and pads with -1),
//! so in this crate `AURA_PARTICLE` (0) stands for "empty". At most 16 decks
//! per profile (`FUN_14015afe0` refuses at 16), names 15 characters
//! (`strncpy(.., 0xf)`).
//!
//! Pure functions, deterministic.

use crate::combat::{Capsule, SkillDef, AURA_PARTICLE};

pub const ARSENAL_SIZE: usize = 30;
/// "You cannot place more than 3 of the same skill in an arsenal"
/// (`arsenaledit_en.ssb`, ERRSZ/STEMAX); Aura Particles are exempt (they are
/// the empty slots).
pub const MAX_COPIES: usize = 3;
/// A profile holds at most 16 arsenals (`FUN_14015afe0`: `0xf < count` -> refuse).
pub const MAX_ARSENALS: usize = 16;
/// Arsenal names are 15 characters (`FUN_14015afe0`: `strncpy(.., name, 0xf)`).
pub const MAX_NAME_LEN: usize = 15;
/// "Number of Schools" choices in the creation dialog (AAKIND: 1..5).
pub const KINDS: [u8; 5] = [1, 2, 3, 4, 5];
/// What the dialog's cursor starts on (two schools is "normal", guide text).
pub const DEFAULT_KIND: u8 = 2;

/// Aura-speed multiplier by declared kind 1 / 2 / 3: the three floats at
/// `gsdata_en.dat + 0x33034` (read from the file: 1.3, 1.0, 0.7; indexed
/// `kind - 1` at `FUN_14012a750`, `FUN_140187710`, `FUN_1401d0750`,
/// `FUN_140192a50`; any other kind -> 1.0). A data-dependent test below
/// re-reads the file when it is present.
pub const AURA_HANDICAP_BY_KIND: [f32; 3] = [1.3, 1.0, 0.7];

/// The multiplicative aura-recovery handicap of an arsenal declared for
/// `kind` schools. It multiplies the refill rate together with the
/// `(100 + Aura-rate status) / 100` factor (`FUN_14024de20`:
/// `(status + 100) * (handicap * max(0.25, 1.25 - aura / 10)) / ..`), it is
/// not an additive percentage. For lane F2 (sim): store it per fighter.
pub fn aura_handicap(kind: u8) -> f32 {
    match kind {
        1..=3 => AURA_HANDICAP_BY_KIND[kind as usize - 1],
        _ => 1.0,
    }
}

/// The schools (1 Psycho, 2 Optical, 3 Nature, 4 Ki, 5 Faith) an arsenal
/// uses, sorted. Aura Particles and school-less skills don't count.
pub fn schools(arsenal: &[i16], skills: &[SkillDef]) -> Vec<i16> {
    let mut out: Vec<i16> = arsenal.iter().filter(|&&id| id != AURA_PARTICLE).filter_map(|&id| skills.iter().find(|s| s.id == id)).map(|s| s.school()).filter(|&s| s > 0).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The declared kind an arsenal gets when it has none stored (old saves,
/// starter decks): the tightest that fits its contents -- its school count
/// (1..=5), or the normal 2 for an arsenal with no schooled skill at all.
pub fn default_kind(arsenal: &[i16], skills: &[SkillDef]) -> u8 {
    match schools(arsenal, skills).len() {
        0 => DEFAULT_KIND,
        n => n.min(5) as u8,
    }
}

/// The one school of a kind-1 arsenal (`FUN_14015bbf0`: first skill whose
/// school is 1..=5), which the program also uses for the aura colour.
#[allow(dead_code)] // for the aura colour of a kind-1 arsenal (lane F2 / school_aura)
pub fn single_school(arsenal: &[i16], skills: &[SkillDef]) -> Option<i16> {
    schools(arsenal, skills).first().copied()
}

// ---------------------------------------------------------------------------
// Rule legality (FUN_14015b830 / FUN_14015b9e0 / table at 0x1402dbc78)
// ---------------------------------------------------------------------------

/// Bits of the computed flags at deck +0x52 (`FUN_14015b830`).
pub mod flag {
    /// Every skill with a school has the same one (and there is at least one).
    pub const SINGLE_SCHOOL: u16 = 0x20;
    /// No skill appears twice (Highlander).
    pub const NO_DUPLICATES: u16 = 0x40;
    /// Every skill costs <= 3 and is not hold/charge (s37 != 2): Puny Only.
    pub const PUNY_ONLY: u16 = 0x80;
    /// Every skill costs >= 3 and is not hold/charge: Heavy Only.
    pub const HEAVY_ONLY: u16 = 0x100;
    /// At least one real skill.
    pub const HAS_SKILL: u16 = 0x4000;
}

/// The 12 game rules, in the program's rule-index order (`DAT_1404bc5dc`,
/// names from `vsmenu_en.ssb`).
pub const RULE_NAMES: [&str; 12] = ["Standard", "Ante", "Quick", "Puny", "Heavy", "Special Environment", "Single School", "Highlander", "Puny Only", "Heavy Only", "KO Battle", "Annihilate the Odd One"];

/// Required flag per rule, read from the PE at `0x1402dbc78` (12 x i32:
/// 0x4000, 0x4000, 0, 0x4000, 0x4000, 0x4000, 0x20, 0x40, 0x80, 0x100,
/// 0x4000, 0x4000). Quick needs no arsenal (its decks are pre-made).
pub const RULE_MASKS: [u16; 12] = [flag::HAS_SKILL, flag::HAS_SKILL, 0, flag::HAS_SKILL, flag::HAS_SKILL, flag::HAS_SKILL, flag::SINGLE_SCHOOL, flag::NO_DUPLICATES, flag::PUNY_ONLY, flag::HEAVY_ONLY, flag::HAS_SKILL, flag::HAS_SKILL];

/// Hold/charge skills (s37 == 2: Gungnir, Ki Palm, Ki Cannon in the data):
/// their cost depends on the charge, so Puny/Heavy Only exclude them
/// (`FUN_14015b830`: `*(short*)(skill + 0x46) == 2`; program offset 0x46 =
/// our s37, SKILL-FIELD-CROSSWALK.md).
pub fn is_charge_skill(def: &SkillDef) -> bool {
    def.charged()
}

/// The rule flags of an arsenal, exactly as `FUN_14015b830` computes them
/// (ids the skill table doesn't know, and `AURA_PARTICLE`, are empty slots).
/// Quirks kept on purpose: `SINGLE_SCHOOL` needs at least one schooled skill;
/// the duplicate scan starts from slots 0..=28 only, so `NO_DUPLICATES` is
/// not set for a deck whose only skill sits in slot 29 (decks are compacted
/// to the front, so that never occurs in practice).
pub fn rule_flags(deck: &[i16], skills: &[SkillDef]) -> u16 {
    let slots: Vec<Option<&SkillDef>> = (0..ARSENAL_SIZE).map(|i| deck.get(i).copied().filter(|&id| id != AURA_PARTICLE).and_then(|id| skills.iter().find(|s| s.id == id))).collect();
    let mut flags: u16 = 0;
    // 1. Single school: every skill with school > 0 shares it.
    let mut first = 0i16;
    for s in slots.iter().flatten() {
        let school = s.school();
        if school > 0 {
            if first == 0 {
                first = school;
                flags = flag::SINGLE_SCHOOL;
            } else if first != school {
                flags = 0;
                break;
            }
        }
    }
    // 2. No duplicates (Highlander).
    for i in 0..ARSENAL_SIZE - 1 {
        let Some(a) = slots[i] else { continue };
        flags |= flag::NO_DUPLICATES;
        if slots[i + 1..].iter().flatten().any(|b| b.id == a.id) {
            flags &= !flag::NO_DUPLICATES;
            break;
        }
    }
    // 3. Puny Only: cost <= 3, not a charge skill.
    for s in slots.iter().flatten() {
        flags |= flag::PUNY_ONLY;
        if is_charge_skill(s) || s.cost > 3 {
            flags &= !flag::PUNY_ONLY;
            break;
        }
    }
    // 4. Heavy Only: cost >= 3, not a charge skill.
    for s in slots.iter().flatten() {
        flags |= flag::HEAVY_ONLY;
        if is_charge_skill(s) || s.cost < 3 {
            flags &= !flag::HEAVY_ONLY;
            break;
        }
    }
    // 5. At least one real skill.
    if slots.iter().any(Option::is_some) {
        flags |= flag::HAS_SKILL;
    }
    flags
}

/// Does `deck` qualify for game rule `rule` (0..12)? `FUN_14015b9e0`: the
/// deck's flags ANDed with the rule's required mask must be non-zero. Quick
/// (mask 0) uses no arsenal, so no arsenal "qualifies" for it (see
/// [`rule_needs_arsenal`] / [`rule_available`]).
pub fn eligible(rule: usize, deck: &[i16], skills: &[SkillDef]) -> bool {
    RULE_MASKS.get(rule).is_some_and(|&mask| rule_flags(deck, skills) & mask != 0)
}

/// False only for Quick, whose pre-made arsenals replace the player's own.
#[allow(dead_code)] // the rule picker (lane F3)
pub fn rule_needs_arsenal(rule: usize) -> bool {
    RULE_MASKS.get(rule).is_some_and(|&m| m != 0)
}

/// Can `rule` be picked at all, given a profile's arsenals? Quick always
/// can; every other rule needs at least one qualifying arsenal
/// (`FUN_1401831f0`, the rule-list enable bits).
#[allow(dead_code)] // the rule picker (lane F3)
pub fn rule_available<'a>(rule: usize, decks: impl IntoIterator<Item = &'a [i16]>, skills: &[SkillDef]) -> bool {
    !rule_needs_arsenal(rule) || decks.into_iter().any(|d| eligible(rule, d, skills))
}

/// Which rules `deck` qualifies for (for the detail panel).
pub fn eligible_rules(deck: &[i16], skills: &[SkillDef]) -> Vec<usize> {
    (0..RULE_MASKS.len()).filter(|&r| eligible(r, deck, skills)).collect()
}

/// The builder's "Mark Illegal Skills" choice (`arsenaledit_en.ssb` RULE
/// MARK: No Mark / Single school / Highlander / Puny only / Heavy only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RuleMark {
    #[default]
    None,
    /// Single school: skills of any other school are marked (the ATTRIBUTE
    /// dialog picks which school).
    SingleSchool(i16),
    Highlander,
    PunyOnly,
    HeavyOnly,
}

impl RuleMark {
    pub fn label(self) -> &'static str {
        match self {
            RuleMark::None => "No Mark",
            RuleMark::SingleSchool(_) => "Single school",
            RuleMark::Highlander => "Highlander",
            RuleMark::PunyOnly => "Puny only",
            RuleMark::HeavyOnly => "Heavy only",
        }
    }

    /// The matching game rule index, for the tests that tie a mark to the
    /// flag test it mirrors.
    #[cfg(test)]
    pub fn rule(self) -> Option<usize> {
        match self {
            RuleMark::None => None,
            RuleMark::SingleSchool(_) => Some(6),
            RuleMark::Highlander => Some(7),
            RuleMark::PunyOnly => Some(8),
            RuleMark::HeavyOnly => Some(9),
        }
    }

    /// Would placing `def` break this rule? (Highlander: it is already in
    /// `deck`.) Never true for `RuleMark::None`.
    pub fn marks(self, def: &SkillDef, deck: &[i16]) -> bool {
        match self {
            RuleMark::None => false,
            RuleMark::SingleSchool(s) => def.school() != s,
            RuleMark::Highlander => deck.contains(&def.id),
            RuleMark::PunyOnly => is_charge_skill(def) || def.cost > 3,
            RuleMark::HeavyOnly => is_charge_skill(def) || def.cost < 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Editing a fixed 30-slot deck
// ---------------------------------------------------------------------------

/// A fresh arsenal: 30 empty slots (= Aura Particles).
pub fn empty_deck() -> Vec<i16> {
    vec![AURA_PARTICLE; ARSENAL_SIZE]
}

/// Real skills to the front in their order, Aura Particles after, exactly 30
/// slots (`FUN_14015a800`, run by the program after every edit).
pub fn compact(deck: &mut Vec<i16>) {
    deck.retain(|&id| id != AURA_PARTICLE);
    deck.truncate(ARSENAL_SIZE);
    deck.resize(ARSENAL_SIZE, AURA_PARTICLE);
}

pub fn copies(deck: &[i16], id: i16) -> usize {
    deck.iter().filter(|&&x| x == id).count()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// Already 3 of this skill in the arsenal.
    TooManyCopies,
    /// "Add Same Skill" with no empty slot left.
    Full,
    /// Slot index outside the arsenal.
    NoSuchSlot,
}

/// "Replace": put `id` into `slot` (an Aura Particle clears it). Refuses a
/// 4th copy (copies in the *other* slots count), then re-compacts.
pub fn place(deck: &mut Vec<i16>, slot: usize, id: i16) -> Result<(), PlaceError> {
    if slot >= deck.len() {
        return Err(PlaceError::NoSuchSlot);
    }
    if id != AURA_PARTICLE && copies(deck, id) - usize::from(deck[slot] == id) >= MAX_COPIES {
        return Err(PlaceError::TooManyCopies);
    }
    deck[slot] = id;
    compact(deck);
    Ok(())
}

/// "Replace with Aura": empty the slot.
pub fn clear_slot(deck: &mut Vec<i16>, slot: usize) -> Result<(), PlaceError> {
    place(deck, slot, AURA_PARTICLE)
}

/// "Add Same Skill": another copy of `slot`'s skill into the first empty slot
/// (`FUN_14015ab90` fills the first slot whose id is < 1).
pub fn add_same(deck: &mut Vec<i16>, slot: usize) -> Result<(), PlaceError> {
    let id = *deck.get(slot).ok_or(PlaceError::NoSuchSlot)?;
    if id == AURA_PARTICLE {
        return Ok(());
    }
    if copies(deck, id) >= MAX_COPIES {
        return Err(PlaceError::TooManyCopies);
    }
    let Some(empty) = deck.iter().position(|&x| x == AURA_PARTICLE) else { return Err(PlaceError::Full) };
    deck[empty] = id;
    compact(deck);
    Ok(())
}

// ---------------------------------------------------------------------------
// Detail panel (FUN_14015a8d0)
// ---------------------------------------------------------------------------

/// The arsenal detail counts: per capsule type, per school, and the Aura
/// Particle count. As `FUN_14015a8d0` does, a known skill with no school
/// counts as an Aura Particle, not toward a type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detail {
    pub aura: usize,
    /// Attack, Defence, Erase, Status, Special, Environment.
    pub by_type: [usize; 6],
    /// Psycho, Optical, Nature, Ki, Faith (school ids 1..=5).
    pub by_school: [usize; 5],
}

pub fn capsule_index(c: Capsule) -> usize {
    match c {
        Capsule::Attack => 0,
        Capsule::Defence => 1,
        Capsule::Erase => 2,
        Capsule::Status => 3,
        Capsule::Special => 4,
        Capsule::Environment => 5,
    }
}

pub fn detail(deck: &[i16], skills: &[SkillDef]) -> Detail {
    let mut d = Detail::default();
    for slot in 0..ARSENAL_SIZE {
        let def = deck.get(slot).copied().filter(|&id| id != AURA_PARTICLE).and_then(|id| skills.iter().find(|s| s.id == id));
        match def {
            Some(s) if (1..=5).contains(&s.school()) => {
                d.by_type[capsule_index(s.capsule)] += 1;
                d.by_school[s.school() as usize - 1] += 1;
            }
            _ => d.aura += 1,
        }
    }
    d
}

// ---------------------------------------------------------------------------
// Problems
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Not exactly 30 slots.
    WrongSize(usize),
    /// More than three copies of a skill (Aura Particles excepted).
    TooManyCopies(i16),
    /// More schools than the arsenal's declared "Number of Schools"
    /// ("This arsenal has skills from too many schools. You are limited to
    /// %d schools.", `arsenaledit_en.ssb` SYSMAX, checked when the edit is
    /// reflected/saved).
    TooManySchools { have: usize, limit: usize },
    /// Not in the skill table.
    Unknown(i16),
}

impl Problem {
    pub fn describe(&self, skills: &[SkillDef]) -> String {
        let name = |id: i16| skills.iter().find(|s| s.id == id).map_or(format!("#{id}"), |s| s.name.clone());
        match self {
            Problem::WrongSize(n) => format!("An arsenal holds {ARSENAL_SIZE} skills ({n} now)."),
            Problem::TooManyCopies(id) => format!("Only {MAX_COPIES} of the same skill ({}).", name(*id)),
            Problem::TooManySchools { limit, .. } => format!("This arsenal has skills from too many schools. You are limited to {limit} schools."),
            Problem::Unknown(id) => format!("Unknown skill #{id}."),
        }
    }
}

/// Everything wrong with `arsenal` declared for `kind` schools, empty when it
/// is legal.
pub fn check_kind(arsenal: &[i16], skills: &[SkillDef], kind: u8) -> Vec<Problem> {
    let mut out = Vec::new();
    if arsenal.len() != ARSENAL_SIZE {
        out.push(Problem::WrongSize(arsenal.len()));
    }
    let mut seen: Vec<i16> = Vec::new();
    for &id in arsenal {
        if id != AURA_PARTICLE && !skills.iter().any(|s| s.id == id) {
            if !out.contains(&Problem::Unknown(id)) {
                out.push(Problem::Unknown(id));
            }
            continue;
        }
        if id != AURA_PARTICLE && arsenal.iter().filter(|&&x| x == id).count() > MAX_COPIES && !seen.contains(&id) {
            out.push(Problem::TooManyCopies(id));
        }
        seen.push(id);
    }
    let n = schools(arsenal, skills).len();
    if n > kind as usize {
        out.push(Problem::TooManySchools { have: n, limit: kind as usize });
    }
    out
}

/// [`check_kind`] with the kind worked out from the contents
/// ([`default_kind`]) -- for arsenals with no stored kind (the versus setup's
/// filter over old saves and starter decks). The school limit can then never
/// trip; size, copies and unknown ids still do.
#[allow(dead_code)] // the versus setup filters saved arsenals with `check_kind` and their declared kind
pub fn check(arsenal: &[i16], skills: &[SkillDef]) -> Vec<Problem> {
    check_kind(arsenal, skills, default_kind(arsenal, skills))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::testkit::{def, f};

    /// (id, school, cost, s37)
    fn skill(id: i16, school: i16, cost: i32, s37: i16) -> SkillDef {
        def(id, Capsule::Attack, cost, &[(f::SCHOOL, school), (37, s37)])
    }

    fn table() -> Vec<SkillDef> {
        [(1, 1), (65, 2), (138, 3), (200, 4), (251, 5), (26, 1)].iter().map(|&(id, school)| skill(id, school, 1, 0)).collect()
    }

    fn deck_of(ids: &[i16]) -> Vec<i16> {
        let mut d = ids.to_vec();
        d.resize(ARSENAL_SIZE, AURA_PARTICLE);
        d
    }

    #[test]
    fn a_legal_two_school_arsenal() {
        let deck: Vec<i16> = std::iter::repeat_n(AURA_PARTICLE, 24).chain([1, 1, 1, 65, 65, 26]).collect();
        assert!(check_kind(&deck, &table(), 2).is_empty());
        assert_eq!(schools(&deck, &table()), vec![1, 2]);
        assert_eq!(default_kind(&deck, &table()), 2);
    }

    #[test]
    fn rules_catch_size_copies_and_the_declared_school_limit() {
        // "limited to %d schools" is the declared kind, not a global 3 (SYSMAX).
        let deck: Vec<i16> = vec![1, 1, 1, 1, 65, 138, 200];
        let p = check_kind(&deck, &table(), 3);
        assert!(p.contains(&Problem::WrongSize(7)));
        assert!(p.contains(&Problem::TooManyCopies(1)));
        assert!(p.contains(&Problem::TooManySchools { have: 4, limit: 3 }));
        let ok = check_kind(&deck_of(&[1, 65, 138, 200]), &table(), 4);
        assert!(ok.is_empty(), "four schools fit a kind-4 arsenal: {ok:?}");
        let one = check_kind(&deck_of(&[1, 65]), &table(), 1);
        assert_eq!(one, vec![Problem::TooManySchools { have: 2, limit: 1 }]);
    }

    #[test]
    fn aura_particles_can_fill_the_arsenal_but_it_is_not_a_usable_one() {
        // Structurally fine (FUN_14015a800 pads with empties)...
        let deck = empty_deck();
        assert!(check(&deck, &table()).is_empty());
        // ...but every rule except Quick needs at least one skill (0x4000).
        assert_eq!(rule_flags(&deck, &table()) & flag::HAS_SKILL, 0);
        for rule in 0..12 {
            assert!(!eligible(rule, &deck, &table()), "{}", RULE_NAMES[rule]);
        }
        assert!(!rule_needs_arsenal(2) && rule_needs_arsenal(0));
        assert!(rule_available(2, [deck.as_slice()], &table()), "Quick needs no arsenal");
        assert!(!rule_available(0, [deck.as_slice()], &table()));
    }

    // ---- FUN_14015b830 -------------------------------------------------

    #[test]
    fn single_school_ignores_empties_and_school_less_skills_but_needs_one_schooled_skill() {
        let mut t = table();
        t.push(skill(701, 0, 1, 0)); // a school-less skill (like the enemy skills)
        // 0x20 needs >= 1 schooled skill; empties and school 0 are ignored.
        assert_ne!(rule_flags(&deck_of(&[1, 26, 701]), &t) & flag::SINGLE_SCHOOL, 0);
        assert_eq!(rule_flags(&deck_of(&[701]), &t) & flag::SINGLE_SCHOOL, 0, "no schooled skill -> not set");
        assert_eq!(rule_flags(&empty_deck(), &t) & flag::SINGLE_SCHOOL, 0);
        assert_eq!(rule_flags(&deck_of(&[1, 65]), &t) & flag::SINGLE_SCHOOL, 0, "two schools");
    }

    #[test]
    fn highlander_means_no_repeats_and_ignores_empties() {
        let t = table();
        assert_ne!(rule_flags(&deck_of(&[1, 65, 138]), &t) & flag::NO_DUPLICATES, 0);
        assert_eq!(rule_flags(&deck_of(&[1, 65, 1]), &t) & flag::NO_DUPLICATES, 0);
        // The 29 empties (many "equal" -1s in the original) are not repeats.
        assert_ne!(rule_flags(&deck_of(&[1]), &t) & flag::NO_DUPLICATES, 0);
        // A lone skill in slot 29 is never scanned (the loop starts at 0..=28).
        let mut odd = empty_deck();
        odd[29] = 1;
        assert_eq!(rule_flags(&odd, &t) & flag::NO_DUPLICATES, 0);
    }

    #[test]
    fn puny_and_heavy_only_use_cost_3_both_ways_and_exclude_charge_skills() {
        let t = vec![skill(1, 1, 2, 0), skill(2, 1, 3, 0), skill(3, 1, 4, 0), skill(4, 1, 1, 2), skill(5, 1, 7, 2)];
        let puny = |ids: &[i16]| rule_flags(&deck_of(ids), &t) & flag::PUNY_ONLY != 0;
        let heavy = |ids: &[i16]| rule_flags(&deck_of(ids), &t) & flag::HEAVY_ONLY != 0;
        assert!(puny(&[1, 2]), "cost 2 and 3 are both <= 3");
        assert!(!puny(&[1, 3]), "cost 4 breaks Puny Only");
        assert!(heavy(&[2, 3]), "cost 3 and 4 are both >= 3");
        assert!(!heavy(&[1, 2]), "cost 2 breaks Heavy Only");
        assert!(puny(&[2]) && heavy(&[2]), "a cost-3 deck is both");
        assert!(!puny(&[4]) && !heavy(&[5]), "charge skills (s37 = 2) are excluded from both");
        assert!(!puny(&[]) && !heavy(&[]), "an empty deck sets neither");
    }

    #[test]
    fn has_skill_bit_needs_a_real_skill() {
        let t = table();
        assert_eq!(rule_flags(&empty_deck(), &t) & flag::HAS_SKILL, 0);
        assert_ne!(rule_flags(&deck_of(&[1]), &t) & flag::HAS_SKILL, 0);
        // An id the table doesn't know is an empty slot (Quick deck Q003 slot 6 = 23428).
        assert_eq!(rule_flags(&deck_of(&[23428]), &t) & flag::HAS_SKILL, 0);
    }

    #[test]
    fn rule_table_matches_the_programs_mask_array_and_eligibility_is_and_nonzero() {
        // 0x1402dbc78: 0x4000 0x4000 0 0x4000 0x4000 0x4000 0x20 0x40 0x80 0x100 0x4000 0x4000.
        assert_eq!(RULE_MASKS, [0x4000, 0x4000, 0, 0x4000, 0x4000, 0x4000, 0x20, 0x40, 0x80, 0x100, 0x4000, 0x4000]);
        assert_eq!(RULE_NAMES.len(), RULE_MASKS.len());
        let t = table();
        let d = deck_of(&[1, 1, 26]); // one school, repeats, cost 1
        let names: Vec<&str> = eligible_rules(&d, &t).into_iter().map(|r| RULE_NAMES[r]).collect();
        assert!(names.contains(&"Standard") && names.contains(&"Single School") && names.contains(&"Puny Only"));
        assert!(!names.contains(&"Highlander"), "skill 1 twice");
        assert!(!names.contains(&"Heavy Only") && !names.contains(&"Quick"));
        assert!(eligible(6, &d, &t) && !eligible(7, &d, &t) && !eligible(99, &d, &t));
    }

    #[test]
    fn rule_marks_mirror_their_flag_tests() {
        let t = vec![skill(1, 1, 2, 0), skill(2, 2, 5, 0), skill(3, 1, 3, 2)];
        let deck = deck_of(&[1]);
        assert!(RuleMark::SingleSchool(1).marks(&t[1], &deck) && !RuleMark::SingleSchool(1).marks(&t[0], &deck));
        assert!(RuleMark::Highlander.marks(&t[0], &deck) && !RuleMark::Highlander.marks(&t[1], &deck));
        assert!(RuleMark::PunyOnly.marks(&t[1], &deck) && RuleMark::PunyOnly.marks(&t[2], &deck) && !RuleMark::PunyOnly.marks(&t[0], &deck));
        assert!(RuleMark::HeavyOnly.marks(&t[0], &deck) && !RuleMark::HeavyOnly.marks(&t[1], &deck));
        assert!(!RuleMark::None.marks(&t[1], &deck));
        // A skill the mark lets through, placed, keeps the deck eligible for that rule.
        for mark in [RuleMark::SingleSchool(1), RuleMark::Highlander, RuleMark::PunyOnly] {
            let ok: Vec<i16> = t.iter().filter(|s| !mark.marks(s, &[])).map(|s| s.id).collect();
            if !ok.is_empty() {
                assert!(eligible(mark.rule().unwrap(), &deck_of(&ok[..1]), &t), "{mark:?}");
            }
        }
    }

    // ---- aura handicap ---------------------------------------------------

    #[test]
    fn handicap_is_multiplicative_by_declared_kind_and_one_otherwise() {
        assert_eq!([aura_handicap(1), aura_handicap(2), aura_handicap(3)], [1.3, 1.0, 0.7]);
        for kind in [0u8, 4, 5, 9] {
            assert_eq!(aura_handicap(kind), 1.0, "kind {kind}");
        }
    }

    /// `gsdata_en.dat + 0x33034` holds the three floats (skipped when the game
    /// files are not on this machine; `PD_GAME_FILES` overrides the search).
    #[test]
    fn handicap_table_equals_the_three_floats_in_gsdata() {
        let rel = "game_files/Assets/Data/gstorage/gsdata_en.dat";
        let mut roots: Vec<std::path::PathBuf> = std::env::var_os("PD_GAME_FILES").map(|p| vec![std::path::PathBuf::from(p)]).unwrap_or_default();
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for _ in 0..6 {
            roots.push(dir.join(rel));
            if !dir.pop() {
                break;
            }
        }
        let Some(data) = roots.iter().find_map(|p| std::fs::read(p).ok()) else {
            eprintln!("skipped: gsdata_en.dat not found (set PD_GAME_FILES)");
            return;
        };
        for (i, want) in AURA_HANDICAP_BY_KIND.iter().enumerate() {
            let o = 0x33034 + 4 * i;
            let got = f32::from_le_bytes(data[o..o + 4].try_into().unwrap());
            assert!((got - want).abs() < 1e-6, "float {i}: file {got}, table {want}");
        }
    }

    // ---- editing ---------------------------------------------------------

    #[test]
    fn compact_keeps_skill_order_and_pads_with_aura() {
        let mut d = vec![0, 5, 0, 7, 0];
        compact(&mut d);
        assert_eq!(d.len(), ARSENAL_SIZE);
        assert_eq!(&d[..3], &[5, 7, 0]);
        assert!(d[2..].iter().all(|&x| x == AURA_PARTICLE));
        let mut long: Vec<i16> = (1..=40).collect();
        compact(&mut long);
        assert_eq!(long.len(), ARSENAL_SIZE);
    }

    #[test]
    fn replace_respects_the_three_copy_cap_counting_other_slots_only() {
        let mut d = deck_of(&[1, 1, 1, 65]);
        assert_eq!(place(&mut d, 3, 1), Err(PlaceError::TooManyCopies), "a 4th copy");
        assert_eq!(place(&mut d, 0, 1), Ok(()), "replacing a copy with itself is fine");
        assert_eq!(place(&mut d, 3, 138), Ok(()));
        assert_eq!(&d[..4], &[1, 1, 1, 138]);
        assert_eq!(place(&mut d, 40, 1), Err(PlaceError::NoSuchSlot));
        assert_eq!(place(&mut d, 10, AURA_PARTICLE), Ok(()), "aura is uncapped");
    }

    #[test]
    fn replace_with_aura_empties_the_slot_and_the_rest_moves_up() {
        let mut d = deck_of(&[1, 65, 138]);
        clear_slot(&mut d, 1).unwrap();
        assert_eq!(&d[..3], &[1, 138, AURA_PARTICLE]);
    }

    #[test]
    fn add_same_fills_the_first_empty_slot_until_three_or_full() {
        let mut d = deck_of(&[1, 65]);
        add_same(&mut d, 0).unwrap();
        add_same(&mut d, 0).unwrap();
        assert_eq!(&d[..4], &[1, 65, 1, 1]);
        assert_eq!(add_same(&mut d, 0), Err(PlaceError::TooManyCopies));
        assert_eq!(add_same(&mut d, 25), Ok(()), "on an Aura slot it does nothing");
        let mut full: Vec<i16> = (1..=30).collect();
        assert_eq!(add_same(&mut full, 0), Err(PlaceError::Full));
    }

    #[test]
    fn detail_counts_types_schools_and_aura_like_the_program() {
        let mut t = table();
        t[1].capsule = Capsule::Defence; // id 65, school 2
        t.push(skill(701, 0, 1, 0)); // school-less: counts as Aura, like FUN_14015a8d0
        let d = detail(&deck_of(&[1, 1, 65, 701]), &t);
        assert_eq!(d.by_type, [2, 1, 0, 0, 0, 0]);
        assert_eq!(d.by_school, [2, 1, 0, 0, 0]);
        assert_eq!(d.aura, 27);
        assert_eq!(d.aura + d.by_type.iter().sum::<usize>(), ARSENAL_SIZE);
    }

    #[test]
    fn default_kind_is_the_tightest_fit_and_normal_for_an_aura_only_deck() {
        let t = table();
        assert_eq!(default_kind(&deck_of(&[1, 26]), &t), 1);
        assert_eq!(default_kind(&deck_of(&[1, 65, 138]), &t), 3);
        assert_eq!(default_kind(&empty_deck(), &t), DEFAULT_KIND);
        assert_eq!(single_school(&deck_of(&[65]), &t), Some(2));
    }
}
