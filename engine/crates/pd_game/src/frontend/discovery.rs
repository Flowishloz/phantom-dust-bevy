//! Finding what a match can be played with: characters, stages, and the
//! built-in starter arsenals. Pure functions over the cache folder (or a
//! parsed `skills.json`), so they're unit-testable without Bevy.

use crate::combat::{SkillDef, AURA_PARTICLE};
use pd_formats::skills::SCHOOL_NAMES;
use std::collections::HashMap;
use std::path::Path;

/// A playable character: its cache id (`pc00a`) and a label. The label is
/// the name of the character's battle-voice bank, which the game's own
/// character table (`setup/charaset/charaset_en.dat`) assigns per id —
/// `pd_convert sounds` writes that mapping to `cache/sfx/index.json`
/// ("voices": pc00a → BattleVo_Hero_en, pc01a → BattleVo_Edgar_en...).
/// Without it, the id is the label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterEntry {
    pub id: String,
    pub label: String,
}

/// Every `chars/pc*.glb` in the cache, sorted by id.
pub fn list_characters(cache: &Path) -> Vec<CharacterEntry> {
    let mut ids: Vec<String> = std::fs::read_dir(cache.join("chars"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".glb")).filter(|id| id.starts_with("pc")).map(String::from))
        .collect();
    ids.sort();
    let index: serde_json::Value = std::fs::read_to_string(cache.join("sfx/index.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    ids.into_iter()
        .map(|id| {
            let label = index["voices"][id.as_str()].as_str().map(voice_bank_name).unwrap_or_else(|| id.to_uppercase());
            CharacterEntry { label, id }
        })
        .collect()
}

/// "BattleVo_CuffButton_en" -> "Cuff Button", "BattleVo_PH_en" -> "pH".
fn voice_bank_name(bank: &str) -> String {
    let core = bank.trim_start_matches("BattleVo_").trim_end_matches("_en").replace('_', " ");
    if core == "PH" {
        return "pH".into();
    }
    let mut out = String::new();
    for (k, c) in core.chars().enumerate() {
        if k > 0 && c.is_ascii_uppercase() && core.as_bytes()[k - 1].is_ascii_lowercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageEntry {
    pub id: String,
    pub name: String,
}

/// Every stage with a collision file, a model and bounds, named from
/// `cache/stages.json` (written by the stages lane) when present, else the
/// id in caps, sorted by id.
pub fn list_stages(cache: &Path) -> Vec<StageEntry> {
    let has_all = |id: &str| cache.join(format!("{id}.pdcl")).exists() && cache.join(format!("{id}.glb")).exists() && cache.join(format!("{id}.bounds.json")).exists();
    let json = std::fs::read_to_string(cache.join("stages.json")).unwrap_or_default();
    if let Some(list) = parse_stage_list(&json, has_all) {
        if !list.is_empty() {
            return list;
        }
    }
    let mut ids: Vec<String> = std::fs::read_dir(cache)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".pdcl")).map(String::from))
        .filter(|id| has_all(id))
        .collect();
    ids.sort();
    ids.into_iter().map(|id| StageEntry { name: id.to_uppercase(), id }).collect()
}

/// `[{"id":"st06","name":"..."}]`, in the file's own order; entries whose
/// files are missing (a stale list) are dropped. `exists` is injected so the
/// parsing itself is unit-testable without a real cache folder.
fn parse_stage_list(json: &str, exists: impl Fn(&str) -> bool) -> Option<Vec<StageEntry>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = v.as_array()?;
    let mut out = Vec::new();
    for s in arr {
        let id = s["id"].as_str()?;
        // Hub pieces and test planes are marked "versus": false.
        if exists(id) && s["versus"].as_bool() != Some(false) {
            out.push(StageEntry { id: id.to_string(), name: s["name"].as_str().unwrap_or(id).to_string() });
        }
    }
    Some(out)
}

/// Deck entries are TABLE INDICES, not skill ids: the program hands each one
/// to `FUN_140261a00` -> `FUN_140262670(slot, index)`, which reads the skill
/// table at `DAT_1404bdc50 + index * 0x90` (index 0 = the built-in Aura
/// Particle, then our record r = index r + 1). `skills.json`'s `skills[]`
/// carry both the table `record` and the s3 `id` every other part of this
/// game keys by, so this builds `index -> id`. Only the FIRST record of an id
/// counts: ids 257 and 294 appear again at records 749 / 750, which are the
/// characters' dodge moves (FORMATS-SKILLS.md "Phase 3"), not deck skills.
/// Entries naming no such record (512, 768, 23428...) are simply absent.
pub fn index_to_id(json: &serde_json::Value) -> HashMap<i64, i16> {
    let mut by_id: HashMap<i64, i64> = HashMap::new(); // id -> lowest record
    for s in json["skills"].as_array().into_iter().flatten() {
        if let (Some(id), Some(rec)) = (s["id"].as_i64(), s["record"].as_i64()) {
            let e = by_id.entry(id).or_insert(rec);
            *e = (*e).min(rec);
        }
    }
    by_id.into_iter().filter(|&(id, _)| (1..=i16::MAX as i64).contains(&id)).map(|(id, rec)| (rec + 1, id as i16)).collect()
}

/// One deck entry (a table index) as a skill id the catalog knows;
/// `AURA_PARTICLE` for index 0 (the Aura Particle), -1 (empty) and anything
/// that names no catalog skill.
fn deck_entry_to_id(index: i64, map: &HashMap<i64, i16>, catalog: &[SkillDef]) -> i16 {
    match map.get(&index) {
        Some(&id) if index > 0 && catalog.iter().any(|s| s.id == id) => id,
        _ => AURA_PARTICLE,
    }
}

/// A named 30-skill arsenal offered without building one: the sim's own test
/// set, `--arsenal mixed` (one of every family), and one starter deck per
/// school.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltinArsenal {
    pub name: String,
    pub skills: Vec<i16>,
    /// The declared "Number of Schools" when the data gives one (the Quick
    /// decks store 2); `None` = worked out from the contents.
    pub kind: Option<u8>,
}

/// The game's own starter decks (`skills.json` "decks": named ones with real
/// skills in them — Psycho/Optical/Ki/Nature at the time this was written;
/// no Faith deck was found in the data) padded to 30 with Aura Particles,
/// plus a Faith deck we build the same way the game's own decks read (its
/// registered skills, cheapest first, up to 3 copies, topped up with Aura
/// Particles) since the data has none. `catalog` should be every registered
/// skill (see `load_registered_skills`).
pub fn school_decks(decks_json: &serde_json::Value, catalog: &[SkillDef]) -> Vec<BuiltinArsenal> {
    let mut out = Vec::new();
    let map = index_to_id(decks_json);
    let mut covered = [false; SCHOOL_NAMES.len()]; // index = school id (0 = none/unused)
    for d in decks_json["decks"].as_array().into_iter().flatten() {
        let name = d["name"].as_str().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let entries: Vec<i64> = d["skills"].as_array().into_iter().flatten().map(|v| v.as_i64().unwrap_or(-1)).collect();
        if entries.iter().all(|&e| e < 0) {
            continue;
        }
        // Entries are table indices (`index_to_id`), -1 = empty.
        let skills: Vec<i16> = entries.iter().map(|&e| deck_entry_to_id(e, &map, catalog)).collect();
        if let Some(idx) = SCHOOL_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            covered[idx] = true;
        }
        out.push(BuiltinArsenal { name: capitalise(name), skills, kind: None });
    }
    // Fill in any school (1..=5; 0 is "none") the data didn't cover.
    for (school, &name) in SCHOOL_NAMES.iter().enumerate() {
        if school == 0 || covered[school] {
            continue;
        }
        out.push(BuiltinArsenal { name: capitalise(name), skills: default_school_deck(school as i16, catalog), kind: None });
    }
    // The original's eight pre-made Quick arsenals (`skills.json` "quick_decks").
    out.extend(quick_decks_from_json(decks_json, catalog).into_iter().map(|q| BuiltinArsenal { name: format!("Quick {}", q.name), skills: q.skills, kind: Some(q.kind) }));
    out
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c.flat_map(|c| c.to_lowercase()))).into_iter().flatten().collect()
}

/// A legal 30-skill arsenal from a single school's cheapest registered
/// skills (up to 3 copies each), topped up with Aura Particles.
fn default_school_deck(school: i16, catalog: &[SkillDef]) -> Vec<i16> {
    let mut pool: Vec<&SkillDef> = catalog.iter().filter(|s| s.school() == school).collect();
    pool.sort_by_key(|s| s.cost);
    let mut deck = Vec::new();
    'fill: for s in &pool {
        for _ in 0..3 {
            if deck.len() >= crate::arsenal::ARSENAL_SIZE {
                break 'fill;
            }
            deck.push(s.id);
        }
    }
    while deck.len() < crate::arsenal::ARSENAL_SIZE {
        deck.push(AURA_PARTICLE);
    }
    deck
}

/// One of the original's eight pre-made "Quick" arsenals (`Q000`-`Q007`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickDeck {
    pub name: String,
    pub skills: Vec<i16>,
    /// The kind the program stores for it (2 in all eight).
    pub kind: u8,
}

/// The Quick decks `pd_convert skills` wrote into `skills.json`
/// (`"quick_decks": [{"name","kind","skills":[30 table indices, -1 = empty]}]`),
/// translated by `index_to_id`; indices the
/// catalog doesn't know (Q003's slot 6 holds 23428, which the program counts
/// as empty) become empty and the deck is compacted like every deck. The
/// setup lists them as built-in arsenals ("Quick Q000"...), with the stored
/// kind (2) as their declared number of schools.
pub fn quick_decks_from_json(json: &serde_json::Value, skills: &[SkillDef]) -> Vec<QuickDeck> {
    let map = index_to_id(json);
    json["quick_decks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let name = d["name"].as_str()?.trim().to_string();
            let mut ids: Vec<i16> = d["skills"].as_array()?.iter().map(|v| v.as_i64().unwrap_or(-1)).map(|e| deck_entry_to_id(e, &map, skills)).collect();
            ids.resize(crate::arsenal::ARSENAL_SIZE, AURA_PARTICLE);
            ids.truncate(crate::arsenal::ARSENAL_SIZE);
            crate::arsenal::compact(&mut ids);
            let kind = d["kind"].as_u64().filter(|k| (1..=5).contains(k)).unwrap_or(2) as u8;
            (!name.is_empty() && ids.iter().any(|&i| i != AURA_PARTICLE)).then_some(QuickDeck { name, skills: ids, kind })
        })
        .collect()
}

/// Read the Quick decks out of `gsdata_en.dat`: 8 records of 0x80 bytes from
/// 0x33804 (`FUN_14017df00`), name[16] at +4, 30 x i16 table indices at +0x14
/// (-1 = empty), declared kind at +0x50. Indices `skills` doesn't know (Q003's
/// slot 6 holds 23428, which the program counts as empty) become empty, and
/// the deck is compacted like every deck (`arsenal::compact`). The game reads
/// the same decks from `skills.json` (`quick_decks_from_json`); this is the
/// raw-file reader `pd_convert` mirrors.
#[allow(dead_code)]
pub fn quick_decks(gsdata: &[u8], skills_json: &serde_json::Value, skills: &[SkillDef]) -> Vec<QuickDeck> {
    let map = index_to_id(skills_json);
    const BASE: usize = 0x33804;
    const STRIDE: usize = 0x80;
    (0..8)
        .filter_map(|i| {
            let rec = gsdata.get(BASE + i * STRIDE..BASE + (i + 1) * STRIDE)?;
            let name = String::from_utf8_lossy(rec[4..0x14].split(|&b| b == 0).next().unwrap_or(&[])).to_string();
            let mut ids: Vec<i16> = (0..crate::arsenal::ARSENAL_SIZE)
                .map(|k| i16::from_le_bytes([rec[0x14 + 2 * k], rec[0x15 + 2 * k]]))
                .map(|e| deck_entry_to_id(e as i64, &map, skills))
                .collect();
            crate::arsenal::compact(&mut ids);
            Some(QuickDeck { name, skills: ids, kind: u16::from_le_bytes([rec[0x50], rec[0x51]]) as u8 })
        })
        .collect()
}

/// Notes text for the skill card (the game's own one-line tip per skill);
/// `hud::SkillNotes` reads the same field but keeps it private, so this is a
/// small duplicate rather than a change to a file this lane doesn't own.
pub fn load_notes(path: &Path) -> HashMap<i16, String> {
    let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut map = HashMap::new();
    for s in v["skills"].as_array().into_iter().flatten() {
        if let Some(id) = s["id"].as_i64() {
            map.insert(id as i16, s["note"].as_str().unwrap_or("").trim().to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Capsule;

    fn skill(id: i16, school: i16, cost: i32) -> SkillDef {
        let mut raw = [0i16; 70];
        raw[pd_formats::skills::field::SCHOOL] = school;
        SkillDef { id, name: format!("Skill {id}"), capsule: Capsule::Attack, family: 0, cost, strength: 1, strength_rule: 0, requirement: 0, requirement_amount: 0, level_penalty: 0, health_cost: 0, defence: 0, raw, s70_73: [0; 4] }
    }

    /// A `skills.json` stand-in: `(record, id, school)` rows.
    fn json_skills(rows: &[(i64, i64, i64)]) -> serde_json::Value {
        let mut v = Vec::new();
        for &(record, id, school) in rows {
            v.push(serde_json::json!({"record": record, "id": id, "school": school}));
        }
        serde_json::json!({ "skills": v })
    }

    #[test]
    fn deck_entries_are_table_indices_record_plus_one() {
        // FUN_140262670(slot, index) reads the table at DAT_1404bdc50 + index * 0x90:
        // index 0 = Aura Particle, index r + 1 = our record r. Record 40 holds id 18
        // (Psycho Kinesis is table index 41, MEASURED live), so entry 41 is id 18, not 41.
        let mut json = json_skills(&[(0, 1, 1), (40, 18, 1), (41, 99, 2), (76, 257, 4), (242, 294, 4), (749, 257, 0), (750, 294, 0)]);
        let map = index_to_id(&json);
        assert_eq!(map.get(&41), Some(&18));
        assert_eq!(map.get(&1), Some(&1));
        assert_eq!(map.get(&77), Some(&257), "id 257 is its FIRST record (76), index 77");
        assert_eq!(map.get(&243), Some(&294));
        assert!(map.get(&750).is_none() && map.get(&751).is_none(), "the dodge records 749 / 750 (second 257 / 294) are not deck skills");
        assert!(map.get(&0).is_none() && map.get(&512).is_none() && map.get(&768).is_none());
        let catalog = vec![skill(1, 1, 1), skill(18, 1, 1), skill(99, 2, 1), skill(257, 4, 1), skill(294, 4, 1)];
        let mut entries = vec![41, 0, -1, 512, 768, 77, 750, 23428, 42, 2];
        entries.resize(crate::arsenal::ARSENAL_SIZE, -1);
        json["quick_decks"] = serde_json::json!([{"name": "QT", "kind": 2, "skills": entries}]);
        let q = quick_decks_from_json(&json, &catalog);
        // index 2 names no record (record 1 absent); 512, 768, 750 and 23428 are ignored; 0 and -1 are empty.
        assert_eq!(&q[0].skills[..4], &[18, 257, 99, AURA_PARTICLE], "41 -> 18, 77 -> 257, 42 -> 99; the rest compact away");
    }

    #[test]
    fn quick_decks_read_the_record_layout_and_drop_indices_the_table_does_not_know() {
        let mut data = vec![0u8; 0x33804 + 8 * 0x80];
        let rec = 0x33804 + 3 * 0x80; // Q003
        data[rec + 4..rec + 8].copy_from_slice(b"Q003");
        let ids: [i16; 30] = std::array::from_fn(|k| match k {
            0 => 55,    // index 55 = record 54
            1 => 23428, // out of range: empty
            2 => 52,
            _ => -1,
        });
        for (k, id) in ids.iter().enumerate() {
            data[rec + 0x14 + 2 * k..rec + 0x16 + 2 * k].copy_from_slice(&id.to_le_bytes());
        }
        data[rec + 0x50] = 2;
        let json = json_skills(&[(54, 154, 1), (51, 151, 2)]);
        let known = vec![skill(154, 1, 2), skill(151, 2, 3)];
        let decks = quick_decks(&data, &json, &known);
        assert_eq!(decks.len(), 8);
        let q3 = &decks[3];
        assert_eq!((q3.name.as_str(), q3.kind), ("Q003", 2));
        assert_eq!(&q3.skills[..3], &[154, 151, AURA_PARTICLE], "unknown index dropped, deck compacted");
        assert_eq!(q3.skills.len(), crate::arsenal::ARSENAL_SIZE);
        assert!(quick_decks(&data[..100], &json, &known).is_empty(), "a short file yields nothing");
    }

    #[test]
    fn quick_decks_from_the_skills_json_become_builtin_arsenals_with_their_kind() {
        let known = vec![skill(154, 1, 2), skill(151, 2, 3)];
        let mut ids = vec![54, 23428, 51];
        ids.resize(crate::arsenal::ARSENAL_SIZE, -1);
        let mut json = serde_json::json!({
            "decks": [],
            "quick_decks": [{"name": "Q003", "kind": 2, "skills": ids}, {"name": "Q009", "kind": 2, "skills": vec![-1; 30]}]
        });
        json["skills"] = json_skills(&[(53, 154, 1), (50, 151, 2)])["skills"].clone();
        let q = quick_decks_from_json(&json, &known);
        assert_eq!(q.len(), 1, "an all-empty deck is left out");
        assert_eq!(&q[0].skills[..3], &[154, 151, AURA_PARTICLE]);
        assert_eq!(q[0].skills.len(), crate::arsenal::ARSENAL_SIZE);
        let built = school_decks(&json, &known);
        let quick = built.iter().find(|b| b.name == "Quick Q003").expect("listed with the built-ins");
        assert_eq!(quick.kind, Some(2));
        assert!(built.iter().filter(|b| b.name.starts_with("Quick")).count() == 1);
        assert!(school_decks(&serde_json::json!({}), &known).iter().all(|b| !b.name.starts_with("Quick")), "no quick_decks key: none listed");
    }

    /// The real converted `skills.json` (skipped when the cache is absent, like
    /// the pd_formats real-data tests): every skill the roster knows, as a
    /// catalog with its real school.
    fn real_skills_json() -> Option<(serde_json::Value, Vec<SkillDef>)> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/skills.json");
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let mut catalog: Vec<SkillDef> = Vec::new();
        for s in json["skills"].as_array()? {
            let id = s["id"].as_i64()? as i16;
            if s["registered"].as_bool() != Some(true) || !(1..=340).contains(&id) || catalog.iter().any(|d| d.id == id) {
                continue;
            }
            let mut raw = [0i16; 70];
            for (k, r) in raw.iter_mut().enumerate() {
                *r = s["raw"][k].as_i64().unwrap_or(0) as i16;
            }
            let mut def = skill(id, raw[pd_formats::skills::field::SCHOOL], 1);
            def.raw = raw;
            catalog.push(def);
        }
        Some((json, catalog))
    }

    #[test]
    fn real_quick_decks_hold_two_schools_and_starter_decks_one() {
        // `FUN_140262670` (deck entry = table index, record + 1): read that way, the 8
        // Quick decks hold exactly 2 schools each (the declared kind 2) and each of the
        // 4 starter decks one (SKILL-BY-SKILL.md index-space section). Read as s3 ids
        // they held 5/5/5/5/5/4/5/5 schools and 38-50 % pure starters.
        let Some((json, catalog)) = real_skills_json() else {
            eprintln!("skipping: engine/cache/skills.json not found");
            return;
        };
        let built = school_decks(&json, &catalog);
        let quick: Vec<_> = built.iter().filter(|b| b.name.starts_with("Quick")).collect();
        assert_eq!(quick.len(), 8);
        for q in &quick {
            let schools = crate::arsenal::schools(&q.skills, &catalog);
            assert_eq!(schools.len(), 2, "{} holds schools {schools:?}", q.name);
            assert_eq!(q.kind, Some(2));
        }
        // The starter decks as the data stores them: Optical / Ki / Nature are one school
        // (100 %). The data's PSYCHO deck is 14 of 16 entries Psycho plus index 61 twice,
        // which is Bullet of Fire (id 130, Nature) -- the same index the NATURE deck holds
        // three times. That is what the original's data says (88 % purity measured in
        // SKILL-BY-SKILL.md), kept as read, not "corrected".
        for name in ["Optical", "Ki", "Nature"] {
            let d = built.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("{name} missing"));
            let schools = crate::arsenal::schools(&d.skills, &catalog);
            assert_eq!(schools.len(), 1, "{name} starter holds schools {schools:?}");
            assert_eq!(schools[0], SCHOOL_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).unwrap() as i16, "{name} is its own school");
        }
        let psycho = built.iter().find(|b| b.name == "Psycho").expect("Psycho starter");
        let in_school = |sc: i16| psycho.skills.iter().filter(|&&id| catalog.iter().find(|s| s.id == id).is_some_and(|s| s.school() == sc)).count();
        assert_eq!((in_school(1), in_school(3)), (14, 2), "Psycho starter: 14 Psycho + 2 x index 61 (Bullet of Fire)");
        assert_eq!(psycho.skills.iter().filter(|&&id| id == 130).count(), 2);
        // Table index 41 is Psycho Kinesis = id 18 (MEASURED live), not id 41.
        assert_eq!(index_to_id(&json).get(&41), Some(&18));
    }

    #[test]
    fn character_names_come_from_voice_banks() {
        assert_eq!(voice_bank_name("BattleVo_CuffButton_en"), "Cuff Button");
        assert_eq!(voice_bank_name("BattleVo_Edgar_en"), "Edgar");
        assert_eq!(voice_bank_name("BattleVo_PH_en"), "pH");
        assert_eq!(voice_bank_name("BattleVo_Freia_Dement_en"), "Freia Dement");
    }

    #[test]
    fn non_versus_stages_are_left_out() {
        let json = r#"[{"id":"st06","name":"Storm Refinery","versus":true},{"id":"st09","name":"st09","versus":false}]"#;
        let list = parse_stage_list(json, |_| true).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn stage_list_from_json_drops_missing_files() {
        let json = r#"[{"id":"st06","name":"Neutral Zone"},{"id":"st99","name":"Gone"}]"#;
        let list = parse_stage_list(json, |id| id == "st06").unwrap();
        assert_eq!(list, vec![StageEntry { id: "st06".into(), name: "Neutral Zone".into() }]);
    }

    #[test]
    fn stage_list_empty_json_falls_back() {
        assert!(parse_stage_list("not json", |_| true).is_none());
        assert_eq!(parse_stage_list("[]", |_| true), Some(vec![]));
    }

    #[test]
    fn school_decks_cover_every_school_including_missing_faith() {
        // The real data's deck field is a fixed 30-slot array (-1 = unused);
        // reproduce that shape here rather than a shortened stand-in.
        // Entries are table indices: index 4 = record 3 = id 1 here.
        let mut psycho_skills = vec![4, 4];
        psycho_skills.resize(crate::arsenal::ARSENAL_SIZE, -1);
        let mut decks_json: serde_json::Value = serde_json::json!({
            "decks": [
                {"name": "PSYCHO", "skills": psycho_skills},
                {"name": "", "skills": vec![-1; crate::arsenal::ARSENAL_SIZE]},
            ]
        });
        decks_json["skills"] = json_skills(&[(3, 1, 1)])["skills"].clone();
        let catalog = vec![skill(1, 1, 4), skill(300, 5, 2), skill(301, 5, 9)];
        let decks = school_decks(&decks_json, &catalog);
        let names: Vec<&str> = decks.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"Psycho"), "named deck kept: {names:?}");
        assert!(names.contains(&"Faith"), "missing school gets a built one: {names:?}");
        let psycho = decks.iter().find(|d| d.name == "Psycho").unwrap();
        assert_eq!(psycho.skills.len(), crate::arsenal::ARSENAL_SIZE);
        assert_eq!(psycho.skills[0], 1);
        assert_eq!(psycho.skills[2], AURA_PARTICLE, "the -1 padding becomes Aura Particles");
        let faith = decks.iter().find(|d| d.name == "Faith").unwrap();
        assert_eq!(faith.skills.len(), crate::arsenal::ARSENAL_SIZE);
        assert_eq!(&faith.skills[0..3], &[300, 300, 300], "cheapest Faith skill first, up to 3 copies");
        assert!(crate::arsenal::check(&faith.skills, &catalog).is_empty(), "the built deck must itself be legal");
    }
}
