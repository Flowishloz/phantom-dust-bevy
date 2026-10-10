//! Saved arsenals: the deck builder's list, persisted next to the cache
//! folder (`<cache>/../userdata/arsenals.json`) so a cache rebuild doesn't
//! erase them. Human-readable JSON with both skill ids and names, per the
//! brief. Pure editing/parsing functions are unit-tested below; only
//! `ArsenalStore` touches disk.
//!
//! Saved format (2026-10-01, builder lane F1): each arsenal gains an optional
//! `"kind"` (1-5, the original's declared "Number of Schools", deck +0x50);
//! a file without it (every save made before this lane) loads with `kind:
//! None` and the builder works the default out from the contents
//! (`arsenal::default_kind`). At most 16 arsenals load (`arsenal::MAX_ARSENALS`)
//! and names are cut to 15 characters, as the original's deck record does.

use crate::arsenal::{MAX_ARSENALS, MAX_NAME_LEN};
use crate::combat::{SkillDef, AURA_PARTICLE};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedArsenal {
    pub name: String,
    pub skills: Vec<i16>,
    /// Declared "Number of Schools" 1-5; `None` = not stored (old save).
    pub kind: Option<u8>,
}

impl SavedArsenal {
    /// The declared kind, or the one the contents imply.
    pub fn kind_or_default(&self, skills: &[SkillDef]) -> u8 {
        self.kind.filter(|k| (1..=5).contains(k)).unwrap_or_else(|| crate::arsenal::default_kind(&self.skills, skills))
    }
}

/// A name the original would store: trimmed, at most 15 characters, never
/// empty ("Unnamed").
pub fn clean_name(name: &str) -> String {
    let n: String = name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME_LEN).collect();
    let n = n.trim().to_string();
    if n.is_empty() {
        "Unnamed".into()
    } else {
        n
    }
}

/// The default name for a new arsenal: the first unused "Arsenal %02d"
/// (`arsenaledit_en.ssb`).
pub fn default_name(existing: &[SavedArsenal]) -> String {
    (1..=MAX_ARSENALS + 1).map(|n| format!("Arsenal {n:02}")).find(|n| !existing.iter().any(|a| a.name == *n)).unwrap_or_else(|| "Arsenal".into())
}

fn name_of(id: i16, skills: &[SkillDef]) -> String {
    if id == AURA_PARTICLE {
        "Aura Particle".to_string()
    } else {
        skills.iter().find(|s| s.id == id).map_or_else(|| format!("#{id}"), |s| s.name.clone())
    }
}

/// `{"arsenals":[{"name":..,"kind":..,"skills":[{"id":..,"name":..}, ...]}]}` --
/// ids are authoritative on load; names are for a human reading the file.
pub fn render(list: &[SavedArsenal], skills: &[SkillDef]) -> String {
    let arsenals: Vec<serde_json::Value> = list
        .iter()
        .map(|a| {
            let skills_json: Vec<serde_json::Value> = a.skills.iter().map(|&id| serde_json::json!({"id": id, "name": name_of(id, skills)})).collect();
            let mut v = serde_json::json!({"name": a.name, "skills": skills_json});
            if let Some(k) = a.kind {
                v["kind"] = serde_json::json!(k);
            }
            v
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({"arsenals": arsenals})).unwrap_or_default()
}

pub fn parse(text: &str) -> Vec<SavedArsenal> {
    let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    v["arsenals"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_ARSENALS)
        .map(|a| {
            let name = clean_name(a["name"].as_str().unwrap_or("Unnamed"));
            let skills = a["skills"].as_array().into_iter().flatten().filter_map(|s| s["id"].as_i64().map(|n| n as i16)).collect();
            let kind = a["kind"].as_u64().filter(|k| (1..=5).contains(k)).map(|k| k as u8);
            SavedArsenal { name, skills, kind }
        })
        .collect()
}

pub struct ArsenalStore {
    path: PathBuf,
    prefs: PathBuf,
}

impl ArsenalStore {
    /// `cache`'s parent (the engine folder) gets a `userdata/arsenals.json`
    /// (and `userdata/builder.json` for the builder's own preferences).
    pub fn new(cache: &Path) -> Self {
        let base = cache.parent().map(Path::to_path_buf).unwrap_or_else(|| cache.to_path_buf());
        ArsenalStore { path: base.join("userdata").join("arsenals.json"), prefs: base.join("userdata").join("builder.json") }
    }

    pub fn load(&self) -> Vec<SavedArsenal> {
        std::fs::read_to_string(&self.path).map(|t| parse(&t)).unwrap_or_default()
    }

    pub fn save_all(&self, list: &[SavedArsenal], skills: &[SkillDef]) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.path, render(list, skills));
    }

    pub fn load_prefs(&self) -> BuilderPrefs {
        std::fs::read_to_string(&self.prefs).map(|t| BuilderPrefs::parse(&t)).unwrap_or_default()
    }

    pub fn save_prefs(&self, prefs: &BuilderPrefs) {
        if let Some(dir) = self.prefs.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.prefs, prefs.render());
    }
}

/// The stock list's sort conditions (`arsenaledit_en.ssb` SORT: up to three
/// of None / School / Cost / Strength / Number of Uses / Range / ID / Rarity /
/// Amount / Favorite, applied in order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    None,
    School,
    Cost,
    Strength,
    Uses,
    Range,
    Id,
    Rarity,
    Amount,
    Favorite,
}

impl SortKey {
    pub const ALL: [SortKey; 10] = [SortKey::None, SortKey::School, SortKey::Cost, SortKey::Strength, SortKey::Uses, SortKey::Range, SortKey::Id, SortKey::Rarity, SortKey::Amount, SortKey::Favorite];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::None => "None",
            SortKey::School => "School",
            SortKey::Cost => "Cost",
            SortKey::Strength => "Strength",
            SortKey::Uses => "Number of Uses",
            SortKey::Range => "Range",
            SortKey::Id => "ID",
            SortKey::Rarity => "Rarity",
            SortKey::Amount => "Amount",
            SortKey::Favorite => "Favorite",
        }
    }

    /// The next / previous key in the list, wrapping.
    pub fn step(self, forward: bool) -> SortKey {
        let i = Self::ALL.iter().position(|&k| k == self).unwrap_or(0);
        Self::ALL[(i + if forward { 1 } else { Self::ALL.len() - 1 }) % Self::ALL.len()]
    }

    fn from_label(s: &str) -> SortKey {
        Self::ALL.iter().copied().find(|k| k.label() == s).unwrap_or(SortKey::None)
    }
}

/// The builder's remembered choices: favourite skills, the three sort keys
/// and which schools are hidden in the stock list (the original keeps the
/// same three per profile; "Default" resets the sort).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderPrefs {
    pub favourites: Vec<i16>,
    pub sort: [SortKey; 3],
    /// Index = school id - 1 (Psycho .. Faith); true = hidden ("Mask School").
    pub hidden: [bool; 5],
}

impl Default for BuilderPrefs {
    fn default() -> Self {
        BuilderPrefs { favourites: Vec::new(), sort: [SortKey::None; 3], hidden: [false; 5] }
    }
}

impl BuilderPrefs {
    pub fn is_favourite(&self, id: i16) -> bool {
        self.favourites.contains(&id)
    }

    pub fn toggle_favourite(&mut self, id: i16) -> bool {
        if let Some(p) = self.favourites.iter().position(|&x| x == id) {
            self.favourites.remove(p);
            false
        } else {
            self.favourites.push(id);
            true
        }
    }

    pub fn render(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "favourites": self.favourites,
            "sort": self.sort.iter().map(|k| k.label()).collect::<Vec<_>>(),
            "hidden_schools": self.hidden,
        }))
        .unwrap_or_default()
    }

    pub fn parse(text: &str) -> BuilderPrefs {
        let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
        let mut p = BuilderPrefs::default();
        p.favourites = v["favourites"].as_array().into_iter().flatten().filter_map(|x| x.as_i64().map(|n| n as i16)).collect();
        for (i, k) in v["sort"].as_array().into_iter().flatten().take(3).enumerate() {
            p.sort[i] = SortKey::from_label(k.as_str().unwrap_or(""));
        }
        for (i, h) in v["hidden_schools"].as_array().into_iter().flatten().take(5).enumerate() {
            p.hidden[i] = h.as_bool().unwrap_or(false);
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arsenal::ARSENAL_SIZE;
    use crate::combat::Capsule;

    fn skills() -> Vec<SkillDef> {
        vec![SkillDef { id: 1, name: "Psycho Wave".into(), capsule: Capsule::Attack, family: 0, cost: 4, strength: 1, strength_rule: 0, requirement: 0, requirement_amount: 0, level_penalty: 0, health_cost: 0, defence: 0, raw: [0; 70], s70_73: [0; 4] }]
    }

    fn arsenal(name: &str, skills: Vec<i16>, kind: Option<u8>) -> SavedArsenal {
        SavedArsenal { name: name.into(), skills, kind }
    }

    #[test]
    fn save_and_load_round_trip_by_id_with_the_declared_kind() {
        let list = vec![arsenal("My Deck", vec![1, 1, AURA_PARTICLE], Some(1)), arsenal("Old", vec![1], None)];
        let text = render(&list, &skills());
        assert!(text.contains("Psycho Wave"), "human-readable names are written: {text}");
        assert!(text.contains("Aura Particle"));
        assert_eq!(parse(&text), list);
    }

    #[test]
    fn a_save_from_before_the_kind_field_still_loads_and_gets_a_default() {
        let old = r#"{"arsenals":[{"name":"Old","skills":[{"id":1,"name":"Psycho Wave"},{"id":0,"name":"Aura Particle"}]}]}"#;
        let list = parse(old);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, None);
        assert_eq!(list[0].skills, vec![1, 0]);
        // No stored kind -> the tightest fit (one school here -> 1).
        let mut sk = skills();
        sk[0].raw[pd_formats::skills::field::SCHOOL] = 1;
        assert_eq!(list[0].kind_or_default(&sk), 1);
        assert_eq!(arsenal("x", vec![1], Some(3)).kind_or_default(&sk), 3, "a stored kind wins");
        assert_eq!(arsenal("x", vec![1], Some(9)).kind_or_default(&sk), 1, "an out-of-range kind is ignored");
    }

    #[test]
    fn at_most_16_arsenals_and_15_character_names_load() {
        let many: Vec<SavedArsenal> = (0..20).map(|i| arsenal(&format!("A rather long arsenal name {i}"), vec![1], None)).collect();
        let back = parse(&render(&many, &skills()));
        assert_eq!(back.len(), MAX_ARSENALS);
        assert!(back.iter().all(|a| a.name.chars().count() <= MAX_NAME_LEN), "{:?}", back[0].name);
        assert_eq!(clean_name("  "), "Unnamed");
        assert_eq!(clean_name("0123456789abcdefXYZ"), "0123456789abcde");
    }

    #[test]
    fn default_name_is_the_first_free_arsenal_number() {
        assert_eq!(default_name(&[]), "Arsenal 01");
        let list = vec![arsenal("Arsenal 01", vec![], None), arsenal("Arsenal 03", vec![], None)];
        assert_eq!(default_name(&list), "Arsenal 02");
    }

    #[test]
    fn store_round_trips_arsenals_and_prefs_through_real_files() {
        let dir = std::env::temp_dir().join(format!("pd_arsenal_test_{}_{:?}", std::process::id(), std::thread::current().id()));
        let cache = dir.join("cache");
        let store = ArsenalStore::new(&cache);
        let list = vec![arsenal("Test", vec![AURA_PARTICLE; ARSENAL_SIZE], Some(2))];
        store.save_all(&list, &skills());
        assert_eq!(store.load(), list);
        assert!(store.path.to_string_lossy().replace('\\', "/").ends_with("userdata/arsenals.json"));
        let mut prefs = BuilderPrefs::default();
        prefs.toggle_favourite(62);
        prefs.sort = [SortKey::School, SortKey::Cost, SortKey::None];
        prefs.hidden[2] = true;
        store.save_prefs(&prefs);
        assert_eq!(store.load_prefs(), prefs);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(store.load_prefs(), BuilderPrefs::default(), "missing file -> defaults");
    }

    #[test]
    fn sort_key_steps_wrap_and_labels_round_trip() {
        assert_eq!(SortKey::Favorite.step(true), SortKey::None);
        assert_eq!(SortKey::None.step(false), SortKey::Favorite);
        for k in SortKey::ALL {
            assert_eq!(SortKey::from_label(k.label()), k);
        }
    }
}
