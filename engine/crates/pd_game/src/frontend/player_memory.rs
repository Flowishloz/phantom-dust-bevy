//! Player memory (Laurence 2026-10-08: "remember what player 1, 2 etc had selected previously (character, arsenal,
//! colors etc)"): each seat's last picks on the versus setup, kept in `cache/players.json` so they survive closing the
//! game. Within one run the setup already keeps its picks between visits (`SetupState`); this file only fills them in
//! on the first visit after the game starts.
//!
//! Written when a match starts from the setup (people's seats only; a CPU seat keeps the seat's older entry). Read
//! once. A missing or broken file, an unknown character or an arsenal that no longer exists is simply skipped: that
//! seat opens on the defaults. Screenshot runs and runs with `PD_SETTINGS` set (test captures) neither read nor
//! write it, so they never touch the player's own memory.

use super::setup::SEATS;
use std::path::{Path, PathBuf};

/// One seat's remembered picks, by stable names (a character id and an arsenal name), not list positions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeatPick {
    pub character: String,
    pub colour: u8,
    pub arsenal: String,
    pub team: u8,
}

/// Every seat's remembered picks (`None`: nothing remembered for that seat).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerMemory {
    pub seats: [Option<SeatPick>; SEATS],
}

impl PlayerMemory {
    /// The file a normal run uses, or `None` for screenshot / test runs (they must not read or write it).
    pub fn path(cache: &Path, screenshot_run: bool) -> Option<PathBuf> {
        (!screenshot_run && std::env::var_os("PD_SETTINGS").is_none()).then(|| cache.join("players.json"))
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).map_or_else(Self::default, |v| Self::from_json(&v))
    }

    pub fn save(&self, path: &Path) {
        if let Ok(text) = serde_json::to_string_pretty(&self.to_json()) {
            let _ = std::fs::write(path, text);
        }
    }

    fn from_json(v: &serde_json::Value) -> Self {
        let mut m = Self::default();
        for (i, slot) in m.seats.iter_mut().enumerate() {
            let s = &v["seats"][i];
            let Some(character) = s["character"].as_str().filter(|c| !c.is_empty()) else { continue };
            *slot = Some(SeatPick {
                character: character.to_string(),
                colour: s["colour"].as_u64().unwrap_or(0).min(u64::from(u8::MAX)) as u8,
                arsenal: s["arsenal"].as_str().unwrap_or_default().to_string(),
                team: s["team"].as_u64().unwrap_or(0).min(1) as u8,
            });
        }
        m
    }

    fn to_json(&self) -> serde_json::Value {
        let seats: Vec<serde_json::Value> = self
            .seats
            .iter()
            .map(|s| match s {
                Some(p) => serde_json::json!({ "character": p.character, "colour": p.colour, "arsenal": p.arsenal, "team": p.team }),
                None => serde_json::Value::Null,
            })
            .collect();
        serde_json::json!({ "seats": seats })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(c: &str, colour: u8, a: &str, team: u8) -> SeatPick {
        SeatPick { character: c.into(), colour, arsenal: a.into(), team }
    }

    #[test]
    fn it_round_trips_through_the_file() {
        let dir = std::env::temp_dir().join(format!("pd_player_memory_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("players.json");
        let m = PlayerMemory { seats: [Some(pick("pc03a", 2, "My deck", 1)), None, Some(pick("pc00a", 0, "Mixed (one of each)", 0)), None] };
        m.save(&path);
        assert_eq!(PlayerMemory::load(&path), m);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_or_broken_file_remembers_nothing() {
        assert_eq!(PlayerMemory::load(Path::new("/nonexistent/players.json")), PlayerMemory::default());
        assert_eq!(PlayerMemory::from_json(&serde_json::json!({ "seats": [{ "colour": 2 }, 7, null] })), PlayerMemory::default());
    }

    #[test]
    fn screenshot_runs_have_no_file() {
        assert_eq!(PlayerMemory::path(Path::new("cache"), true), None);
    }
}
