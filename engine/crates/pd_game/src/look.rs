//! Lighting and look options: the optional modern enhancements on the
//! Options screen's "Lighting and look" page (docs/FRONTEND.md "Look options",
//! docs/RESEARCH-LIGHTING.md Part B).
//!
//! Everything here is presentation. It never touches the sim. Every option's
//! DEFAULT is the original's look (nothing on, the original's hard shadows, Bevy's default tone curve, the
//! original's one effect light per fighter), so a settings file from before
//! this page, or no settings file, plays exactly as it did.
//!
//! The options, and what each one switches (all at runtime, all per view, so
//! split screens of 1 to 4 views are covered by the same code):
//!
//! | Row            | Original (default)          | Enhanced choices                                  |
//! |----------------|-----------------------------|---------------------------------------------------|
//! | Bloom          | off                         | LOW, HIGH: `glow.rs` per-view glow, two strengths |
//! | Shadow quality | ORIGINAL (2x2 hardware PCF) | HIGH: Gaussian PCF + 4096 px shadow maps          |
//! | Tone curve     | ORIGINAL (Bevy's default)   | FILMIC: AgX                                         |
//! | Effect lights  | ORIGINAL (1 per fighter)    | 4 PER FIGHTER (`char_light.rs`)                   |
//!
//! plus a preset row (ORIGINAL / ENHANCED / MAX) that sets the others at once
//! and reads CUSTOM when the rows no longer match one.
//!
//! Why these and not others (checked against Bevy 0.19.1's source):
//! * SSAO (`ScreenSpaceAmbientOcclusion`) is not offered: it only darkens the
//!   ambient term of Bevy's PBR lighting (`apply_pbr_lighting`), which the
//!   stage (unlit, baked vertex colours) and the fighters (the original's
//!   SWBOS port in `char_light.wgsl`) never use, and it needs MSAA off plus a
//!   depth/normal prepass for every custom material. It would cost frame time
//!   and change nothing.
//! * No anti-aliasing row (Laurence, 2026-10-03): the game keeps the 4x MSAA it
//!   has always shipped (`main.rs` `spawn_scene`), and this page never touches
//!   a camera's `Msaa`. (FXAA / SMAA would also each cost a full-window pass per
//!   view on the shared window target; TAA needs motion vectors.)
//! * Bevy's contact shadows, volumetric fog and clustered lights are PBR-light
//!   features of the same kind: the stage and fighters are lit by our own
//!   shaders, so they would not see them.

use crate::display::step;
use crate::glow::{Glow, GlowOn, GlowParams};
use crate::menu::Settings;
use crate::PlayerCamera;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{DirectionalLightShadowMap, ShadowFilteringMethod};
use bevy::prelude::*;

/// One option row's values: a fixed list with a label and a settings-file key.
pub trait Choice: Copy + PartialEq + std::fmt::Debug + 'static {
    const ALL: &'static [Self];
    fn label(self) -> &'static str;
    fn key(self) -> &'static str;
    fn from_key(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.key() == s)
    }
}

macro_rules! choice {
    ($name:ident { $($var:ident => ($label:literal, $key:literal)),+ $(,)? }) => {
        impl Choice for $name {
            const ALL: &'static [Self] = &[$($name::$var),+];
            fn label(self) -> &'static str {
                match self { $($name::$var => $label),+ }
            }
            fn key(self) -> &'static str {
                match self { $($name::$var => $key),+ }
            }
        }
    };
}

/// Per-view bloom (`glow.rs`): a soft halo round the brightest things.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Bloom {
    #[default]
    Off,
    Low,
    High,
}
choice!(Bloom { Off => ("OFF", "off"), Low => ("LOW", "low"), High => ("HIGH", "high") });

impl Bloom {
    /// The glow's strengths for this level; `None` = no glow. High is the
    /// glow lane's own tuned defaults (`Glow::default()`); Low is half of both
    /// halo layers, the same threshold.
    pub fn glow(self) -> Option<Glow> {
        match self {
            Bloom::Off => None,
            Bloom::High => Some(Glow::default()),
            Bloom::Low => {
                let g = Glow::default();
                Some(Glow { strength: g.strength * 0.5, wide_strength: g.wide_strength * 0.5, ..g })
            }
        }
    }
}

/// Shadow quality. The shadows themselves (the original's darkening) stay on
/// whichever you pick; this is only how soft and how sharp their edges are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShadowQuality {
    #[default]
    Original,
    High,
}
choice!(ShadowQuality { Original => ("ORIGINAL", "original"), High => ("HIGH", "high") });

impl ShadowQuality {
    /// The shadow-map side in pixels for this quality, given what the original
    /// setting (`PD_SHADOW_MAP`, default 2048) is.
    pub fn map_size(self, original: usize) -> usize {
        match self {
            ShadowQuality::Original => original,
            ShadowQuality::High => original.max(4096),
        }
    }
}

/// The tone curve the picture goes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tone {
    /// Bevy's default (what the game has always used; `PD_TONEMAP=none` still turns it off for A/B).
    #[default]
    Original,
    /// AgX (Blender's default filmic curve): a little more contrast, a gentle roll-off in the highlights and slightly cooler, less saturated colour. Chosen over ACES-fitted (a hard 25-level darkening of this game's already-clamped picture) and Blender Filmic / SBDT / Reinhard (almost no visible change) from a six-way sheet.
    Filmic,
}
choice!(Tone { Original => ("ORIGINAL", "original"), Filmic => ("FILMIC", "filmic") });

impl Tone {
    pub fn tonemapping(self) -> Tonemapping {
        match self {
            Tone::Original => original_tonemapping(),
            Tone::Filmic => Tonemapping::AgX,
        }
    }
}

/// The tone curve of the original look: `PD_TONEMAP=none` or Bevy's default.
pub fn original_tonemapping() -> Tonemapping {
    if std::env::var("PD_TONEMAP").is_ok_and(|v| v == "none") {
        Tonemapping::None
    } else {
        Tonemapping::default()
    }
}

/// How many effect lights light a fighter at once. The original's character
/// shader has ONE point-light slot (slot 2 of the light table, `FUN_140199970`):
/// the most recent effect light replaces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EffectLights {
    #[default]
    Original,
    More,
}
choice!(EffectLights { Original => ("ORIGINAL (1)", "original"), More => ("4 PER FIGHTER", "more") });

impl EffectLights {
    /// Effect lights per fighter.
    pub fn count(self) -> usize {
        match self {
            EffectLights::Original => 1,
            EffectLights::More => crate::char_light::MAX_EFFECT_LIGHTS,
        }
    }
}

/// The preset row: sets every other row. CUSTOM is what the row reads when the
/// others no longer match one of the three (it can't be chosen).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Preset {
    #[default]
    Original,
    Enhanced,
    Max,
    Custom,
}

impl Preset {
    /// The three the row can pick.
    pub const PICKABLE: [Preset; 3] = [Preset::Original, Preset::Enhanced, Preset::Max];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Original => "ORIGINAL",
            Preset::Enhanced => "ENHANCED",
            Preset::Max => "MAX",
            Preset::Custom => "CUSTOM",
        }
    }

    /// The settings this preset means.
    pub fn settings(self) -> Option<LookSettings> {
        match self {
            Preset::Original => Some(LookSettings::default()),
            Preset::Enhanced => Some(LookSettings { bloom: Bloom::Low, shadows: ShadowQuality::High, tone: Tone::Original, lights: EffectLights::More }),
            // Laurence 2026-10-03: no filmic curve in MAX (it greys the dark stages); it stays its own row.
            Preset::Max => Some(LookSettings { bloom: Bloom::High, shadows: ShadowQuality::High, tone: Tone::Original, lights: EffectLights::More }),
            Preset::Custom => None,
        }
    }
}

/// Everything the Lighting and look rows set. Stored in `settings.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LookSettings {
    pub bloom: Bloom,
    pub shadows: ShadowQuality,
    pub tone: Tone,
    pub lights: EffectLights,
}

impl LookSettings {
    /// Reads `look_bloom`, `look_shadows`, `look_tone`, `look_lights`;
    /// an absent or unknown value keeps its default, so an old settings file
    /// loads as the original look.
    pub fn from_json(v: &serde_json::Value) -> Self {
        fn get<C: Choice + Default>(v: &serde_json::Value, key: &str) -> C {
            v[key].as_str().and_then(C::from_key).unwrap_or_default()
        }
        LookSettings { bloom: get(v, "look_bloom"), shadows: get(v, "look_shadows"), tone: get(v, "look_tone"), lights: get(v, "look_lights") }
    }

    /// Adds this struct's keys to a settings object.
    pub fn write_json(&self, v: &mut serde_json::Value) {
        v["look_bloom"] = self.bloom.key().into();
        v["look_shadows"] = self.shadows.key().into();
        v["look_tone"] = self.tone.key().into();
        v["look_lights"] = self.lights.key().into();
    }

    /// Which preset these settings are (CUSTOM when none).
    pub fn preset(&self) -> Preset {
        Preset::PICKABLE.into_iter().find(|p| p.settings() == Some(*self)).unwrap_or(Preset::Custom)
    }

    /// Capture switches (never saved): `PD_TEST_LOOK=original|enhanced|max`, then
    /// `PD_TEST_BLOOM`, `PD_TEST_SHADOWS`, `PD_TEST_TONE`,
    /// `PD_TEST_LIGHTS` override single rows (the keys above, e.g. `high`).
    pub fn with_test_env(mut self) -> Self {
        if let Ok(p) = std::env::var("PD_TEST_LOOK") {
            if let Some(s) = Preset::PICKABLE.into_iter().find(|x| x.label().eq_ignore_ascii_case(p.trim())).and_then(|x| x.settings()) {
                self = s;
            }
        }
        fn pick<C: Choice>(var: &str, into: &mut C) {
            if let Some(c) = std::env::var(var).ok().and_then(|v| C::from_key(v.trim())) {
                *into = c;
            }
        }
        pick("PD_TEST_BLOOM", &mut self.bloom);
        pick("PD_TEST_SHADOWS", &mut self.shadows);
        pick("PD_TEST_TONE", &mut self.tone);
        pick("PD_TEST_LIGHTS", &mut self.lights);
        self
    }
}

// ---------------------------------------------------------------------------
// The rows (the menu page)

/// The rows of the Lighting and look page, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookRow {
    Preset,
    Bloom,
    Shadows,
    Tone,
    Lights,
    Back,
}

pub const LOOK_ROWS: [LookRow; 6] = [LookRow::Preset, LookRow::Bloom, LookRow::Shadows, LookRow::Tone, LookRow::Lights, LookRow::Back];

impl LookRow {
    pub fn label(self) -> &'static str {
        match self {
            LookRow::Preset => "LOOK PRESET",
            LookRow::Bloom => "BLOOM",
            LookRow::Shadows => "SHADOW QUALITY",
            LookRow::Tone => "TONE CURVE",
            LookRow::Lights => "EFFECT LIGHTS",
            LookRow::Back => "BACK",
        }
    }
}

/// Left / right on a Look row (`forward` = right).
pub fn change_row(row: LookRow, forward: bool, look: &mut LookSettings) {
    match row {
        LookRow::Preset => {
            // From CUSTOM, right goes to the first preset and left to the last.
            let cur = look.preset();
            let next = if cur == Preset::Custom {
                if forward { Preset::PICKABLE[0] } else { Preset::PICKABLE[Preset::PICKABLE.len() - 1] }
            } else {
                step(&Preset::PICKABLE, cur, forward)
            };
            if let Some(s) = next.settings() {
                *look = s;
            }
        }
        LookRow::Bloom => look.bloom = step(Bloom::ALL, look.bloom, forward),
        LookRow::Shadows => look.shadows = step(ShadowQuality::ALL, look.shadows, forward),
        LookRow::Tone => look.tone = step(Tone::ALL, look.tone, forward),
        LookRow::Lights => look.lights = step(EffectLights::ALL, look.lights, forward),
        LookRow::Back => {}
    }
}

/// The value text of a row (without arrows).
pub fn value_text(row: LookRow, look: &LookSettings) -> String {
    match row {
        LookRow::Preset => look.preset().label().to_string(),
        LookRow::Bloom => look.bloom.label().to_string(),
        LookRow::Shadows => look.shadows.label().to_string(),
        LookRow::Tone => look.tone.label().to_string(),
        LookRow::Lights => look.lights.label().to_string(),
        LookRow::Back => String::new(),
    }
}

/// The help line under the rows: what the highlighted choice does and a rough
/// frame-time cost (docs/FRONTEND.md "Look options" has the measurements).
pub fn help_text(row: LookRow, look: &LookSettings) -> &'static str {
    match row {
        LookRow::Preset => "Sets every row below at once. ORIGINAL is the 2004 look, nothing added. ENHANCED: soft bloom, high shadows, more effect lights. MAX: everything on, for a strong PC.",
        LookRow::Bloom => match look.bloom {
            Bloom::Off => "No glow, like the original. Bloom adds a soft halo round bright effects and lights.",
            Bloom::Low => "A faint halo round bright things. Costs about half of HIGH.",
            Bloom::High => "A strong halo round bright things. Costs about 0.5 to 1.5 ms with four views at 1080p; not measurable with two.",
        },
        LookRow::Shadows => match look.shadows {
            ShadowQuality::Original => "Hard shadow edges like the original's, 2048 px shadow maps. The shadows themselves stay on.",
            ShadowQuality::High => "Soft, smoother shadow edges and 4096 px maps. Costs about 0.3 to 1 ms with four views; not measurable with two.",
        },
        LookRow::Tone => match look.tone {
            Tone::Original => "The game's usual tone curve.",
            Tone::Filmic => "A filmic curve: a touch more contrast, softer highlights, slightly cooler colour. Negligible frame cost.",
        },
        LookRow::Lights => match look.lights {
            EffectLights::Original => "One effect light lights each fighter at a time, as in the original.",
            EffectLights::More => "Up to four effect lights light each fighter at once, so overlapping skills all show. Negligible frame cost.",
        },
        LookRow::Back => "Back to the main options.",
    }
}

// ---------------------------------------------------------------------------
// Applying the settings (render-side only)

/// Pushes `settings.look` into the renderer: glow, shadow filter and map, the
/// tone curve, the effect-light count. Changes an asset or component only when
/// the wanted value differs. Never touches a camera's MSAA.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn apply_look(
    settings: Res<Settings>,
    mut commands: Commands,
    mut glow_on: Option<ResMut<GlowOn>>,
    mut glow_params: Option<ResMut<GlowParams>>,
    mut shadow_map: Option<ResMut<DirectionalLightShadowMap>>,
    mut char_light: Option<ResMut<crate::char_light::CharLight>>,
    players: Query<(Entity, Option<&ShadowFilteringMethod>, Option<&Tonemapping>), With<PlayerCamera>>,
) {
    let look = settings.look;
    if std::env::var_os("PD_LOOK_LOG").is_some() && (settings.is_changed() || players.iter().any(|(_, _, t)| t.is_none())) {
        eprintln!("[look] {look:?} cameras {:?}", players.iter().map(|(_, f, t)| (f.copied(), t.copied())).collect::<Vec<_>>());
    }
    // The test and A/B switches still win where they exist: `PD_GLOW` for the bloom.
    if std::env::var_os("PD_GLOW").is_none() {
        if let (Some(on), Some(params)) = (glow_on.as_mut(), glow_params.as_mut()) {
            let (want_on, want) = match look.bloom.glow() {
                Some(g) => (true, g),
                None => (false, params.0.clone()),
            };
            if on.0 != want_on {
                on.0 = want_on;
            }
            if want_on && params.0 != want {
                params.0 = want;
            }
        }
    }
    // Shadows: the filter on every view, the map size on the lights. Only when
    // the shadow lane is on (`PD_SHADOWS=0` leaves it alone).
    let shadows_on = crate::shadows::enabled();
    if shadows_on {
        if let Some(map) = shadow_map.as_mut() {
            let want = look.shadows.map_size(crate::shadows::map_size());
            if map.size != want {
                map.size = want;
            }
        }
    }
    let want_tone = look.tone.tonemapping();
    let want_filter = if look.shadows == ShadowQuality::High { ShadowFilteringMethod::Gaussian } else { crate::shadows::original_filter() };
    for (e, filter, tone) in &players {
        if shadows_on && filter != Some(&want_filter) {
            commands.entity(e).insert(want_filter);
        }
        if tone != Some(&want_tone) {
            commands.entity(e).insert(want_tone);
        }
    }
    if let Some(l) = char_light.as_mut() {
        let n = look.lights.count();
        if l.max_lights != n {
            l.max_lights = n;
        }
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_original_look() {
        let d = LookSettings::default();
        assert_eq!((d.bloom, d.shadows, d.tone, d.lights), (Bloom::Off, ShadowQuality::Original, Tone::Original, EffectLights::Original));
        // Original = no glow, the original's map size, Bevy's default curve, one light.
        assert_eq!(d.bloom.glow(), None);
        assert_eq!(d.shadows.map_size(2048), 2048);
        assert_eq!(d.tone.tonemapping(), Tonemapping::default());
        assert_eq!(d.lights.count(), 1);
        assert_eq!(d.preset(), Preset::Original);
        assert_eq!(Preset::Original.settings(), Some(d));
    }

    #[test]
    fn option_lists_have_the_requested_values() {
        assert_eq!(Bloom::ALL.iter().map(|c| c.label()).collect::<Vec<_>>(), ["OFF", "LOW", "HIGH"]);
        assert_eq!(ShadowQuality::ALL.iter().map(|c| c.label()).collect::<Vec<_>>(), ["ORIGINAL", "HIGH"]);
        assert_eq!(Tone::ALL.iter().map(|c| c.label()).collect::<Vec<_>>(), ["ORIGINAL", "FILMIC"]);
        assert_eq!(EffectLights::ALL.iter().map(|c| c.label()).collect::<Vec<_>>(), ["ORIGINAL (1)", "4 PER FIGHTER"]);
        assert_eq!(LOOK_ROWS.len(), 6);
        assert_eq!(LOOK_ROWS[0], LookRow::Preset);
        assert_eq!(*LOOK_ROWS.last().unwrap(), LookRow::Back);
        assert!(LOOK_ROWS.iter().all(|r| !r.label().contains("ANTI")), "no anti-aliasing row (Laurence: keep the 4x MSAA as is)");
        // Every key round-trips and is unique per row.
        fn keys<C: Choice>() {
            for c in C::ALL {
                assert_eq!(C::from_key(c.key()), Some(*c));
            }
            let mut k: Vec<_> = C::ALL.iter().map(|c| c.key()).collect();
            k.sort();
            k.dedup();
            assert_eq!(k.len(), C::ALL.len());
        }
        keys::<Bloom>();
        keys::<ShadowQuality>();
        keys::<Tone>();
        keys::<EffectLights>();
    }

    #[test]
    fn presets_map_to_settings_and_back() {
        let enhanced = Preset::Enhanced.settings().unwrap();
        assert_eq!((enhanced.bloom, enhanced.shadows, enhanced.tone, enhanced.lights), (Bloom::Low, ShadowQuality::High, Tone::Original, EffectLights::More));
        let max = Preset::Max.settings().unwrap();
        assert_eq!((max.bloom, max.shadows, max.tone, max.lights), (Bloom::High, ShadowQuality::High, Tone::Original, EffectLights::More));
        assert_eq!(Preset::Custom.settings(), None);
        for p in Preset::PICKABLE {
            assert_eq!(p.settings().unwrap().preset(), p);
        }
        // Changing one row of a preset makes it CUSTOM; changing it back restores the name.
        let mut l = enhanced;
        l.bloom = Bloom::Off;
        assert_eq!(l.preset(), Preset::Custom);
        l.bloom = Bloom::Low;
        assert_eq!(l.preset(), Preset::Enhanced);
    }

    #[test]
    fn the_preset_row_steps_through_the_three_and_leaves_custom_sensibly() {
        let mut l = LookSettings::default();
        change_row(LookRow::Preset, true, &mut l);
        assert_eq!(l.preset(), Preset::Enhanced);
        change_row(LookRow::Preset, true, &mut l);
        assert_eq!(l.preset(), Preset::Max);
        change_row(LookRow::Preset, true, &mut l);
        assert_eq!(l.preset(), Preset::Original, "wraps");
        change_row(LookRow::Preset, false, &mut l);
        assert_eq!(l.preset(), Preset::Max);
        // From CUSTOM: right = first preset, left = last.
        let mut custom = LookSettings { bloom: Bloom::High, ..Default::default() };
        assert_eq!(custom.preset(), Preset::Custom);
        change_row(LookRow::Preset, true, &mut custom);
        assert_eq!(custom, LookSettings::default());
        let mut custom = LookSettings { bloom: Bloom::High, ..Default::default() };
        change_row(LookRow::Preset, false, &mut custom);
        assert_eq!(custom.preset(), Preset::Max);
    }

    #[test]
    fn each_row_steps_its_own_value_and_wraps() {
        let mut l = LookSettings::default();
        change_row(LookRow::Bloom, true, &mut l);
        assert_eq!(l.bloom, Bloom::Low);
        change_row(LookRow::Bloom, false, &mut l);
        change_row(LookRow::Bloom, false, &mut l);
        assert_eq!(l.bloom, Bloom::High, "left from off wraps to the last");
        change_row(LookRow::Shadows, true, &mut l);
        assert_eq!(l.shadows, ShadowQuality::High);
        change_row(LookRow::Tone, true, &mut l);
        change_row(LookRow::Lights, true, &mut l);
        assert_eq!((l.tone, l.lights), (Tone::Filmic, EffectLights::More));
        let before = l;
        change_row(LookRow::Back, true, &mut l);
        assert_eq!(l, before);
    }

    #[test]
    fn bloom_levels_are_graded_and_high_is_the_glow_lanes_default() {
        let low = Bloom::Low.glow().unwrap();
        let high = Bloom::High.glow().unwrap();
        assert_eq!(high, Glow::default());
        assert!(low.strength < high.strength && low.wide_strength < high.wide_strength && low.threshold == high.threshold);
    }

    #[test]
    fn high_shadows_never_shrink_a_map_the_player_already_enlarged() {
        assert_eq!(ShadowQuality::High.map_size(2048), 4096);
        assert_eq!(ShadowQuality::High.map_size(8192), 8192);
        assert_eq!(ShadowQuality::Original.map_size(1024), 1024);
    }

    #[test]
    fn json_round_trips_and_old_files_load_as_the_original_look() {
        let l = LookSettings { bloom: Bloom::High, shadows: ShadowQuality::High, tone: Tone::Filmic, lights: EffectLights::More };
        let mut v = serde_json::json!({ "split": "top_bottom" });
        l.write_json(&mut v);
        assert_eq!(v["split"], "top_bottom", "other keys survive");
        assert_eq!(LookSettings::from_json(&v), l);
        assert_eq!(LookSettings::from_json(&serde_json::json!({})), LookSettings::default());
        let junk = serde_json::json!({ "look_bloom": "ultra", "look_shadows": 3, "look_tone": "", "look_lights": null });
        assert_eq!(LookSettings::from_json(&junk), LookSettings::default());
        // One bad value keeps the good ones; a stale AA key is ignored.
        let mixed = serde_json::json!({ "look_bloom": "high", "look_tone": "sepia", "look_aa": "fxaa" });
        let m = LookSettings::from_json(&mixed);
        assert_eq!((m.bloom, m.tone), (Bloom::High, Tone::Original));
        assert!(v.get("look_aa").is_none(), "nothing writes an AA key");
    }

    /// The real file: save, load, and an old file without the new keys.
    #[test]
    fn settings_file_round_trip_keeps_old_settings_and_the_look() {
        let path = std::env::temp_dir().join(format!("pd_look_rt_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(Settings::load(path.clone()).look, LookSettings::default(), "no file: the original look");
        let mut s = Settings::load(path.clone());
        s.fov_deg = 70.0;
        s.look = Preset::Max.settings().unwrap();
        s.save();
        let back = Settings::load(path.clone());
        assert_eq!(back.look, s.look);
        assert_eq!(back.fov_deg, 70.0, "the FOV is still saved (as fov_h16)");
        std::fs::write(&path, r#"{"split":"top_bottom","fov_deg":60.0,"hud_scale":125}"#).unwrap();
        let old = Settings::load(path.clone());
        assert_eq!((old.fov_deg, old.look), (crate::menu::FOV_DEFAULT, LookSettings::default()), "the old fov_deg key means something else and is ignored");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn every_row_has_a_label_a_help_line_and_a_cost_note_where_it_costs() {
        let l = LookSettings::default();
        for r in LOOK_ROWS {
            assert!(!r.label().is_empty() && !help_text(r, &l).is_empty(), "{r:?}");
        }
        let costs = [help_text(LookRow::Bloom, &LookSettings { bloom: Bloom::High, ..l }), help_text(LookRow::Shadows, &LookSettings { shadows: ShadowQuality::High, ..l })];
        assert!(costs.iter().all(|t| t.contains(" ms")), "the costly choices say what they cost: {costs:?}");
    }

    #[test]
    fn test_env_overrides_do_not_touch_a_clean_environment() {
        // No PD_TEST_* set in the test process: the settings come back unchanged.
        let l = LookSettings { bloom: Bloom::Low, ..Default::default() };
        if ["PD_TEST_LOOK", "PD_TEST_BLOOM", "PD_TEST_SHADOWS", "PD_TEST_TONE", "PD_TEST_LIGHTS"].iter().all(|v| std::env::var_os(v).is_none()) {
            assert_eq!(l.with_test_env(), l);
        }
    }
}
