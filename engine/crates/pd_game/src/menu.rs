//! Settings menu (the Options overlay), in three tabs switched with LB / RB (keyboard Q / E):
//! * VIDEO SETTINGS: split-screen layout, field of view and camera distance (sliders), camera
//!   height, the display options (window mode, resolution, aspect ratio, HUD size: `display.rs`)
//!   and a second page of lighting and look options (`look.rs`);
//! * SOUND: master, music and effects volume (sliders; `mixer.rs` applies them);
//! * CONTROLS: vibration, invert camera Y, camera sensitivity (slider; applied in
//!   `gather_input` by [`Settings::shape_look`]) and BUTTON MAPPING, a page of its own
//!   (`Page::Buttons`): each remappable action's pad button and key (`bindings.rs`); A on a
//!   row listens for the next button or key, RESET TO DEFAULTS puts the old layout back.
//!
//! Quit to title is the last row of every tab in a match.
//!
//! Open/close: Esc (keyboard) or Start (any gamepad). Up/Down picks a row,
//! Left/Right changes it (keyboard arrows/WASD, or any pad's D-pad or left stick). The match pauses
//! while the menu is open, but every row takes effect at once, in a match too:
//! * split, camera height, the sliders and every Lighting and look row apply as you move;
//! * the four display rows (mode, resolution, aspect, HUD size) *browse*: Left/Right shows
//!   the next choice marked "not applied", **A applies it at once**, and moving to another
//!   row drops it. There is no confirm button and no keep-these-settings countdown.
//!
//! Settings persist to `settings.json` in the cache folder (a slider's value when the
//! direction is let go; capture runs never write it).

use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    SideBySide,
    TopBottom,
}

/// Five camera-height positions, from the "you can only see the top of your
/// head" end to the "you are looking down on the stage" end.
///
/// These are the `TB_CAM_HEIGHT_DROP` values (world units the eye sits BELOW
/// the usual 30u-above-the-player), sampled at the two extremes Laurence
/// reported plus the midpoint, and captured on top/bottom split — the
/// top/bottom strip is the tight one, since it only holds about 17 units of
/// world height at the 60u boom, and that is where the extremes bite.
///
/// `Centre` is the default: Laurence asked for the middle of the two framing
/// extremes ("too high", then "too low" at 14), so 7 is his stated
/// preference rather than a computed value.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraHeight {
    /// 0: eye at its normal 30u. Your own character falls off the bottom of
    /// the top/bottom strip; only the opponent shows.
    High,
    /// 4: a small drop; the character sits with its feet on the lower edge.
    Slight,
    /// 7: the default.
    Centre,
    /// 11: noticeably lower and more level.
    Low,
    /// 14: the lowest offered; the camera is close to the character's own
    /// height and the stage reads almost side-on.
    VeryLow,
}

impl CameraHeight {
    /// The eye drop this position means, in world units.
    pub fn drop(self) -> f32 {
        match self {
            CameraHeight::High => 0.0,
            CameraHeight::Slight => 4.0,
            CameraHeight::Centre => 7.0,
            CameraHeight::Low => 11.0,
            CameraHeight::VeryLow => 14.0,
        }
    }

    /// The name shown in the settings row.
    pub fn label(self) -> &'static str {
        match self {
            CameraHeight::High => "HIGH",
            CameraHeight::Slight => "SLIGHTLY LOW",
            CameraHeight::Centre => "MID",
            CameraHeight::Low => "LOW",
            CameraHeight::VeryLow => "VERY LOW",
        }
    }

    fn next(self) -> Self {
        match self {
            CameraHeight::High => CameraHeight::Slight,
            CameraHeight::Slight => CameraHeight::Centre,
            CameraHeight::Centre => CameraHeight::Low,
            CameraHeight::Low => CameraHeight::VeryLow,
            CameraHeight::VeryLow => CameraHeight::High,
        }
    }

    fn prev(self) -> Self {
        match self {
            CameraHeight::High => CameraHeight::VeryLow,
            CameraHeight::Slight => CameraHeight::High,
            CameraHeight::Centre => CameraHeight::Slight,
            CameraHeight::Low => CameraHeight::Centre,
            CameraHeight::VeryLow => CameraHeight::Low,
        }
    }

    /// The stored form: the eye drop itself, so an old settings file with an
    /// unrecognised value falls back to the default rather than snapping to
    /// an extreme. Nearest-match, so a hand-edited 10 still lands on `Low`.
    fn from_drop(drop: f32) -> Self {
        const ALL: [CameraHeight; 5] = [
            CameraHeight::High,
            CameraHeight::Slight,
            CameraHeight::Centre,
            CameraHeight::Low,
            CameraHeight::VeryLow,
        ];
        ALL.into_iter().min_by(|a, b| (a.drop() - drop).abs().total_cmp(&(b.drop() - drop).abs())).unwrap_or(CameraHeight::Centre)
    }
}

#[derive(Resource, Clone, Debug)]
pub struct Settings {
    pub split: Split,
    /// Field of view in degrees: the HORIZONTAL angle of a 16:9 view (the original's own picture is
    /// 75 degrees across, LIVE, GAME.md). `views::fov_for` turns it into each view's vertical angle
    /// from that view's own aspect. Saved as `fov_h16`; the old `fov_deg` key meant something else
    /// (a vertical angle of a side-by-side half) and is ignored.
    pub fov_deg: f32,
    /// Which of the five camera-height positions to use.
    pub camera_height: CameraHeight,
    /// Camera distance as a percentage of the original's boom (100 = the
    /// original's distance; Laurence 2026-10-03 asked for a slider).
    pub cam_dist_pct: f32,
    /// Window mode, resolution, aspect ratio and HUD size (`display.rs`).
    /// This is what is applied now: a choice only gets here when the player
    /// presses A on its row (the row shows a candidate while browsing, kept in
    /// `MenuState`), and it is saved at that moment.
    pub display: crate::display::DisplaySettings,
    /// True once the player has applied a display choice; until then (a first
    /// run) `display` is `DisplaySettings::first_run()`: borderless, native size.
    pub display_chosen: bool,
    /// The Lighting and look page's choices (`look.rs`); every default is the original's look.
    pub look: crate::look::LookSettings,
    /// Controller vibration (`rumble.rs`, docs/RUMBLE.md); on by default, json key `"rumble"`.
    pub rumble: bool,
    /// The SOUND tab's sliders, 0-100 (100 = the game's own loudness; `mixer.rs`). json keys
    /// `vol_master`, `vol_music`, `vol_effects`.
    pub master_vol: f32,
    pub music_vol: f32,
    pub effects_vol: f32,
    /// Free camera: right stick up / keyboard R looks DOWN (json `invert_cam_y`, off by default).
    pub invert_cam_y: bool,
    /// Free-camera sensitivity in percent (json `cam_sens_pct`, 100 = the original's).
    pub cam_sens_pct: f32,
    /// The button mapping (CONTROLS > BUTTON MAPPING, `bindings.rs`; json `bindings`, missing = the old fixed buttons).
    pub bindings: crate::bindings::Bindings,
    path: PathBuf,
    /// False in capture runs: they must never overwrite the player's settings file.
    persist: bool,
}

/// Range and default of the FOV setting (the slider reads these). The default is the original's
/// horizontal angle: the LIVE projection's x scale 1.3032 = cot(37.5 deg), i.e. 75.00 degrees
/// (GAME.md quoted 74.7; that was an arithmetic slip, 2 atan(1/1.3032) = 75.00).
pub const FOV_MIN: f32 = 50.0;
pub const FOV_MAX: f32 = 110.0;
pub const FOV_DEFAULT: f32 = 75.0;
pub const CAM_DIST_MIN: f32 = 70.0;
pub const CAM_DIST_MAX: f32 = 160.0;
pub const CAM_DIST_DEFAULT: f32 = 100.0;
/// Volume sliders (SOUND tab): 0 = silent, 100 = the game's own loudness (the default).
pub const VOL_MIN: f32 = 0.0;
pub const VOL_MAX: f32 = 100.0;
pub const VOL_DEFAULT: f32 = 100.0;
/// Camera sensitivity (CONTROLS tab), percent of the original's turning speed.
pub const SENS_MIN: f32 = 50.0;
pub const SENS_MAX: f32 = 200.0;
pub const SENS_DEFAULT: f32 = 100.0;

impl Settings {
    pub fn load(path: PathBuf) -> Self {
        let v: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        // A first run is a file without a display choice. Older builds wrote every display key at its default on any
        // save, so "all defaults and no marker" is also a first run; any other value was the player's choice.
        let shown = crate::display::DisplaySettings::from_json(&v);
        let display_chosen = v["display_chosen"].as_bool().unwrap_or(false) || shown != crate::display::DisplaySettings::default();
        Settings {
            split: if v["split"] == "top_bottom" { Split::TopBottom } else { Split::SideBySide },
            fov_deg: v["fov_h16"].as_f64().map(|f| f as f32).unwrap_or(FOV_DEFAULT).clamp(FOV_MIN, FOV_MAX),
            camera_height: v["cam_drop"].as_f64().map(|f| CameraHeight::from_drop(f as f32)).unwrap_or(CameraHeight::Centre),
            cam_dist_pct: v["cam_dist_pct"].as_f64().map(|f| f as f32).unwrap_or(CAM_DIST_DEFAULT).clamp(CAM_DIST_MIN, CAM_DIST_MAX),
            display: if display_chosen { shown } else { crate::display::DisplaySettings::first_run() },
            display_chosen,
            look: crate::look::LookSettings::from_json(&v),
            rumble: v["rumble"].as_bool().unwrap_or(true),
            master_vol: pct(&v["vol_master"], VOL_DEFAULT, VOL_MIN, VOL_MAX),
            music_vol: pct(&v["vol_music"], VOL_DEFAULT, VOL_MIN, VOL_MAX),
            effects_vol: pct(&v["vol_effects"], VOL_DEFAULT, VOL_MIN, VOL_MAX),
            invert_cam_y: v["invert_cam_y"].as_bool().unwrap_or(false),
            cam_sens_pct: pct(&v["cam_sens_pct"], SENS_DEFAULT, SENS_MIN, SENS_MAX),
            bindings: crate::bindings::Bindings::from_json(&v["bindings"]),
            path,
            persist: true,
        }
    }

    /// Never write the settings file (screenshot runs).
    pub fn disable_saving(&mut self) {
        self.persist = false;
    }

    pub(crate) fn save(&self) {
        if !self.persist {
            return;
        }
        let mut v = serde_json::json!({
            "split": if self.split == Split::TopBottom { "top_bottom" } else { "side_by_side" },
            "fov_h16": self.fov_deg,
            "cam_drop": self.camera_height.drop(),
            "cam_dist_pct": self.cam_dist_pct,
            "display_chosen": self.display_chosen,
            "rumble": self.rumble,
            "vol_master": self.master_vol,
            "vol_music": self.music_vol,
            "vol_effects": self.effects_vol,
            "invert_cam_y": self.invert_cam_y,
            "cam_sens_pct": self.cam_sens_pct,
            "bindings": self.bindings.to_json(),
        });
        self.display.write_json(&mut v);
        self.look.write_json(&mut v);
        let _ = std::fs::write(&self.path, serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    /// The free camera's look input after the CONTROLS tab: `cam` = turn (right stick x,
    /// keyboard Q / E, -1..1), `pitch` = look up / down (right stick y, keyboard R / F).
    ///
    /// The turn goes into the sim (`sim::Input::cam` / `cam_fast`, quantised, so both PCs of
    /// an online match step on the same number): it is scaled here, BEFORE quantising, and
    /// held to -2..2 (1 = the original's top turning speed, the sim's own `CAM_TURN_SPEED`), so
    /// above 100 % full stick turns faster than the original (Laurence 2026-10-08: "Allow
    /// faster"; `Input::cam_value` keeps every turn up to the top speed as before). The pitch is render-only
    /// (`CamPitchInput`): scaled in full, and flipped by INVERT CAMERA Y. At 100 % and off
    /// both come back unchanged.
    pub fn shape_look(&self, cam: f32, pitch: f32) -> (f32, f32) {
        let k = self.cam_sens_pct.clamp(SENS_MIN, SENS_MAX) / 100.0;
        let pitch = if self.invert_cam_y { -pitch } else { pitch };
        ((cam * k).clamp(-2.0, 2.0), pitch * k)
    }

    /// Writes `text` to `name` next to the settings file (not in capture runs).
    pub(crate) fn write_sibling(&self, name: &str, text: &str) {
        if self.persist {
            let _ = std::fs::write(self.path.with_file_name(name), text);
        }
    }
}

/// A percent-style number from the settings file: `default` when missing or not a number, held to its range.
fn pct(v: &serde_json::Value, default: f32, min: f32, max: f32) -> f32 {
    v.as_f64().map(|f| f as f32).filter(|f| f.is_finite()).unwrap_or(default).clamp(min, max)
}

/// How far one slider step goes after the direction has been held for
/// `frames` frames in a row: 1 for a tap and the first ~0.7 s, then 2, 4 and 8
/// (Left/Right pulses every 6 frames once held, `frontend::input`).
pub fn slider_step(frames: u32) -> f32 {
    match frames {
        0..=39 => 1.0,
        40..=79 => 2.0,
        80..=139 => 4.0,
        _ => 8.0,
    }
}

#[derive(Resource, Default)]
pub struct MenuState {
    pub open: bool,
    row: usize,
    repeat: crate::frontend::input::Repeat,
    /// The display choice being browsed and the row it belongs to: shown on
    /// that row as "not applied" until A applies it. Dropped when the cursor
    /// leaves the row, the page changes or the overlay closes.
    browse: Option<(Row, crate::display::DisplaySettings)>,
    /// A slider moved and `settings.json` has not been written yet (it is
    /// written when the direction is let go, or the cursor leaves).
    dirty: bool,
    /// Which page of rows is showing.
    page: Page,
    /// The main-page row to come back to from the Lighting and look page.
    back_row: usize,
    /// Which tab is showing (LB / RB). Kept while the overlay is closed: it opens where it was left.
    tab: Tab,
    /// The mapping page is waiting for a new button / key for this action (frames waited so far).
    listen: Option<(crate::bindings::Action, u32)>,
    /// The mapping page's last message (a refused button), shown on the help line until the cursor moves.
    note: Option<String>,
}

/// The overlay's three tabs, left to right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Video,
    Sound,
    Controls,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Video, Tab::Sound, Tab::Controls];

    /// The name on the tab strip.
    pub fn label(self) -> &'static str {
        match self {
            Tab::Video => "VIDEO SETTINGS",
            Tab::Sound => "SOUND",
            Tab::Controls => "CONTROLS",
        }
    }

    /// The tab's own rows, top to bottom (Quit to title follows them in a match).
    fn rows(self) -> &'static [Row] {
        match self {
            Tab::Video => &VIDEO_ROWS,
            Tab::Sound => &SOUND_ROWS,
            Tab::Controls => &CONTROLS_ROWS,
        }
    }

    /// The next tab to the right (`forward`, RB) or left (LB), wrapping at both ends.
    pub fn step(self, forward: bool) -> Tab {
        let i = Tab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        let n = Tab::ALL.len();
        Tab::ALL[if forward { (i + 1) % n } else { (i + n - 1) % n }]
    }

    /// `video|sound|controls` (capture switch).
    pub fn parse(s: &str) -> Option<Tab> {
        match s.to_ascii_lowercase().as_str() {
            "video" => Some(Tab::Video),
            "sound" => Some(Tab::Sound),
            "controls" => Some(Tab::Controls),
            _ => None,
        }
    }
}

/// The overlay's pages: a tab's own rows, Lighting and look (`look.rs`, from VIDEO SETTINGS)
/// and button mapping (`bindings.rs`, from CONTROLS).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Main,
    Look,
    Buttons,
}

/// The button-mapping page's rows: the eight actions and RESET TO DEFAULTS. No BACK row: B / Backspace go back
/// (Laurence, 2026-10-09: de-clutter; UI-DECLUTTER-PROPOSALS.md item 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonRow {
    Action(crate::bindings::Action),
    Reset,
}

pub const BUTTON_ROWS: usize = crate::bindings::Action::ALL.len() + 1;

pub fn button_row(i: usize) -> ButtonRow {
    match crate::bindings::Action::ALL.get(i) {
        Some(a) => ButtonRow::Action(*a),
        None => ButtonRow::Reset,
    }
}

/// How long the mapping page listens for a new button (frames, about five seconds at 60 Hz).
pub const LISTEN_FRAMES: u32 = 300;

impl MenuState {
    /// The highlighted row (tests, and the pause-input regression).
    #[allow(dead_code)]
    pub(crate) fn row(&self) -> usize {
        self.row
    }

    /// Which page is showing (tests).
    #[allow(dead_code)]
    pub(crate) fn page(&self) -> Page {
        self.page
    }

    /// Which tab is showing (tests).
    #[allow(dead_code)]
    pub(crate) fn tab(&self) -> Tab {
        self.tab
    }

    /// The action the mapping page is listening for (tests).
    #[allow(dead_code)]
    pub(crate) fn listening(&self) -> Option<crate::bindings::Action> {
        self.listen.map(|(a, _)| a)
    }

    /// Capture switch (`PD_TEST_OPTIONS_TAB=buttons`): the overlay on the button-mapping page.
    pub fn show_buttons_page(&mut self) {
        self.open = true;
        self.tab = Tab::Controls;
        self.page = Page::Buttons;
        self.back_row = CONTROLS_ROWS.iter().position(|r| *r == Row::Buttons).unwrap_or(0);
        self.row = 0;
    }

    /// Capture switch (`PD_TEST_OPTIONS_TAB=video|sound|controls`): the overlay on that tab's first row.
    pub fn show_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.page = Page::Main;
        self.row = 0;
    }

    /// The display choice being browsed, if any (tests).
    #[allow(dead_code)]
    pub(crate) fn browsing(&self) -> Option<crate::display::DisplaySettings> {
        self.browse.map(|(_, d)| d)
    }

    /// Capture switch (`PD_TEST_LOOK_PAGE=<row>`): the overlay open on the Lighting and look page with `row` highlighted.
    pub fn show_look_page(&mut self, row: usize) {
        self.open = true;
        self.page = Page::Look;
        self.tab = Tab::Video;
        self.back_row = VIDEO_ROWS.iter().position(|r| *r == Row::Look).unwrap_or(0);
        self.row = row.min(crate::look::LOOK_ROWS.len() - 1);
    }

    pub fn opened(open: bool) -> Self {
        MenuState { open, ..Default::default() }
    }
}

#[derive(Component)]
pub struct MenuRoot;

#[derive(Component)]
pub struct MenuRow(usize);

/// Every row of every tab. Each tab lists its own (`VIDEO_ROWS`, `SOUND_ROWS`,
/// `CONTROLS_ROWS`); Quit to title follows them only in a match. Keying everything off
/// those lists (not bare numbers) keeps a row inserted above another from silently
/// hiding or moving the wrong one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Split,
    /// A slider.
    Fov,
    /// A slider.
    CameraDistance,
    CameraHeight,
    /// The five display rows browse: left/right shows a candidate, A applies it.
    DisplayMode,
    /// Which screen the game opens on (a browsing row).
    Screen,
    Resolution,
    Aspect,
    HudSize,
    /// Opens the Lighting and look page.
    Look,
    /// Sliders (SOUND tab).
    MasterVolume,
    MusicVolume,
    EffectsVolume,
    /// Controller vibration on / off (applies at once; CONTROLS tab).
    Vibration,
    /// Free camera look up / down flipped (CONTROLS tab).
    InvertY,
    /// A slider (CONTROLS tab).
    CamSensitivity,
    /// Opens the button-mapping page (CONTROLS tab).
    Buttons,
    Quit,
}

/// VIDEO SETTINGS: the overlay's old main page, Vibration aside (it moved to CONTROLS).
const VIDEO_ROWS: [Row; 10] = [Row::Split, Row::Fov, Row::CameraDistance, Row::CameraHeight, Row::DisplayMode, Row::Screen, Row::Resolution, Row::Aspect, Row::HudSize, Row::Look];
const SOUND_ROWS: [Row; 3] = [Row::MasterVolume, Row::MusicVolume, Row::EffectsVolume];
const CONTROLS_ROWS: [Row; 4] = [Row::Vibration, Row::InvertY, Row::CamSensitivity, Row::Buttons];
/// The VIDEO SETTINGS tab's rows in a match (its rows plus Quit to title): the longest list.
pub(crate) const ROWS: usize = VIDEO_ROWS.len() + 1;
/// Row entities the overlay keeps (each tab and the Look page reuse them from the top).
const MAX_ROWS: usize = ROWS;

/// How many rows `tab` has (Quit to title only in a match).
fn row_count(tab: Tab, in_match: bool) -> usize {
    tab.rows().len() + in_match as usize
}

/// The row at `i` on `tab`, if there is one.
fn row_at(tab: Tab, i: usize, in_match: bool) -> Option<Row> {
    let rows = tab.rows();
    rows.get(i).copied().or_else(|| (in_match && i == rows.len()).then_some(Row::Quit))
}

impl Row {
    /// The five rows whose left/right only browse (A applies).
    fn browses(self) -> bool {
        matches!(self, Row::DisplayMode | Row::Screen | Row::Resolution | Row::Aspect | Row::HudSize)
    }

    /// A slider's (min, max).
    fn slider(self) -> Option<(f32, f32)> {
        match self {
            Row::Fov => Some((FOV_MIN, FOV_MAX)),
            Row::CameraDistance => Some((CAM_DIST_MIN, CAM_DIST_MAX)),
            Row::MasterVolume | Row::MusicVolume | Row::EffectsVolume => Some((VOL_MIN, VOL_MAX)),
            Row::CamSensitivity => Some((SENS_MIN, SENS_MAX)),
            _ => None,
        }
    }
}

fn slider_value(row: Row, s: &Settings) -> f32 {
    match row {
        Row::Fov => s.fov_deg,
        Row::MasterVolume => s.master_vol,
        Row::MusicVolume => s.music_vol,
        Row::EffectsVolume => s.effects_vol,
        Row::CamSensitivity => s.cam_sens_pct,
        _ => s.cam_dist_pct,
    }
}

fn set_slider(row: Row, s: &mut Settings, v: f32) {
    match row {
        Row::Fov => s.fov_deg = v,
        Row::MasterVolume => s.master_vol = v,
        Row::MusicVolume => s.music_vol = v,
        Row::EffectsVolume => s.effects_vol = v,
        Row::CamSensitivity => s.cam_sens_pct = v,
        _ => s.cam_dist_pct = v,
    }
}

/// Moves a slider by `steps` units (whole degrees / percent) in a direction;
/// true when the value changed. The settings are only touched when it did
/// (a `ResMut` write marks them changed for every system that watches).
fn nudge_slider(row: Row, forward: bool, steps: f32, settings: &mut Settings) -> bool {
    let Some((min, max)) = row.slider() else { return false };
    let cur = slider_value(row, settings);
    let new = (cur + if forward { steps } else { -steps }).clamp(min, max);
    if new == cur {
        return false;
    }
    set_slider(row, settings, new);
    true
}

/// The slider's fill, 0 to 1.
pub fn slider_fraction(value: f32, min: f32, max: f32) -> f32 {
    if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 }
}

/// The panel in the 1280x720 design frame: the header, the tab strip, up to ten 42px rows, the prompts and the key / help lines.
const PANEL: (f32, f32, f32, f32) = (300.0, 36.0, 680.0, 648.0);
const ROW_PITCH: f32 = 41.0;
/// The tab strip under the header (offset from the panel top, height), then the first row's offset.
const TAB_Y: f32 = 58.0;
const TAB_H: f32 = 32.0;
const ROWS_Y: f32 = 100.0;
/// The tab strip across the row width (632): "LB", the three tabs, "RB".
const TAB_SIDE: f32 = 44.0;
const TAB_GAP: f32 = 8.0;
const TAB_TEXT: f32 = 0.875;

/// The help line at the bottom of the panel: the keys on the main page, what the highlighted choice does on the Lighting and look page.
#[derive(Component)]
pub struct HelpLine;

// One short line about the highlighted row; the keys are on the prompt line and the tab strip, and saving is automatic
// (Laurence, 2026-10-09: de-clutter; UI-DECLUTTER-PROPOSALS.md item 2).
const MAIN_HELP: &str = "Changes at once and is saved.";
const BUTTONS_HELP: &str = "A: give this action a new pad button or key (every pad, and keyboard player 1). Sticks, D-pad, Start and the menu buttons stay fixed.";

/// Which prompts a page shows: VIDEO SETTINGS (A applies a browsed display choice), the tabs whose rows change at
/// once, the Lighting and look page, the button-mapping page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompts {
    Apply,
    Live,
    Look,
    Buttons,
}

/// The overlay parts that move or resize with the page's row count: the panel ends under its last row (item 13).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelPart {
    Border,
    Back,
    Prompts(Prompts),
    Help,
}

/// The panel's height for `rows` rows: the rows, the prompt line, two help lines and a margin.
pub fn panel_height(rows: usize) -> f32 {
    (ROWS_Y + rows as f32 * ROW_PITCH + PROMPT_GAP + 34.0 + 40.0 + 10.0).min(PANEL.3)
}
/// Space between the last row and the prompt line.
const PROMPT_GAP: f32 = 12.0;

/// One tab on the strip and its pixel-text label.
#[derive(Component)]
pub struct TabButton {
    tab: Tab,
    text: Entity,
}


/// The help line for the highlighted row of a tab (the Video tab's display and Look rows have their own, in `menu_view`).
fn tab_help(tab: Tab, row: Option<Row>) -> &'static str {
    match (tab, row) {
        (_, Some(Row::Vibration)) => "ON plays the original's rumble on hits, guards and casts.",
        (_, Some(Row::InvertY)) => "ON: right stick up looks down with the free camera (not the locked-on one).",
        (_, Some(Row::CamSensitivity)) => "How fast the right stick turns the free camera. 100 % is the original.",
        (_, Some(Row::Buttons)) => "A: choose the button for each action. DEFAULT is the original layout.",
        (_, Some(Row::Split)) => "How the screen splits between two players.",
        (_, Some(Row::Fov)) => "How wide each player's view is. 75 is the default.",
        (_, Some(Row::CameraDistance)) => "How far behind the fighter the camera sits. 100 % is the original.",
        (_, Some(Row::CameraHeight)) => "How high the camera sits behind the fighter.",
        (_, Some(Row::Screen)) => "Which screen the game opens on, and moves to now. AUTO is the biggest one (your 4K TV). Left / Right shows a choice; A applies it.",
        (_, Some(Row::DisplayMode | Row::Resolution | Row::Aspect | Row::HudSize)) => "Left / Right shows a choice; A applies it.",
        (_, Some(Row::MasterVolume)) => "All sound. 100 is the game's own loudness, 0 is silent.",
        (_, Some(Row::MusicVolume)) => "The background music.",
        (_, Some(Row::EffectsVolume)) => "The fight's sounds, voices and menu sounds.",
        (_, Some(Row::Quit)) => "Leave the match for the main menu.",
        _ => MAIN_HELP,
    }
}

/// A slider's three drawn parts, children of its row.
#[derive(Component)]
pub struct SliderPart {
    row: usize,
    kind: SliderKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SliderKind {
    Track,
    Fill,
    Knob,
}

/// The bar sits between a slider row's label and its value (design px inside the 632 px row).
const SLIDER_X: f32 = 364.0;
const SLIDER_W: f32 = 146.0;
const KNOB_W: f32 = 10.0;

/// A browsed display choice that is not applied yet: a dull yellow value and an "A: APPLY" badge.
const CANDIDATE: Color = Color::srgb(0.80, 0.74, 0.50);
/// The tab strip: the showing tab in the selected-row red, the others a dark teal.
const TAB_OFF: Color = Color::srgb(0.07, 0.22, 0.21);

#[allow(clippy::too_many_arguments)]
pub fn spawn_menu(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<crate::Config>, windows: Query<&Window>, mut images: ResMut<Assets<Image>>, art: Option<Res<crate::frontend::ui::MenuArt>>) {
    use crate::frontend::ui::{self, Glyph, MenuArt};
    let art = MenuArt::get(&mut commands, art, &cfg.cache, &assets, &mut images);
    // An overlay camera that draws only UI, on top of both player views and
    // the menu screens.
    let cam = commands.spawn((Camera2d, Camera { order: 10, clear_color: ClearColorConfig::None, ..default() }, crate::SleepWhenEmpty)).id();
    let root = commands
        .spawn((MenuRoot, UiTargetCamera(cam), Visibility::Hidden, Node { width: percent(100), height: percent(100), ..default() }))
        .id();
    // Dim whatever is underneath (a match or a menu screen).
    let dim = commands.spawn((Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)))).id();
    commands.entity(root).add_child(dim);
    // Same 1280x720 design frame as the menu screens (docs/MENUS.md), fitted to the game area.
    let s = crate::display::ui_size(&windows).map_or(1.0, |a| (a.x / ui::DESIGN.x).min(a.y / ui::DESIGN.y).max(0.01));
    let frame = commands
        .spawn((
            ui::DesignFrame,
            UiTransform::from_scale(Vec2::splat(s)),
            Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), margin: UiRect { left: px(-ui::DESIGN.x / 2.0), top: px(-ui::DESIGN.y / 2.0), ..default() }, width: px(ui::DESIGN.x), height: px(ui::DESIGN.y), ..default() },
        ))
        .id();
    commands.entity(root).add_child(frame);
    // Ten 46px rows plus header, the bottom prompt line and the key lines.
    let (x, y, w, h) = PANEL;
    let border = commands.spawn((PanelPart::Border, ui::abs(x - 2.0, y - 2.0, w + 4.0, h + 4.0), BackgroundColor(Color::srgba(0.45, 0.85, 0.72, 0.6)))).id();
    let panel = commands.spawn((PanelPart::Back, ui::abs(x, y, w, h), BackgroundColor(Color::srgb(0.02, 0.06, 0.06)))).id();
    let header = ui::spawn_pixel(&mut commands, &art, "OPTIONS_", ui::TEAL, 1.75);
    commands.entity(header).insert(Node { position_type: PositionType::Absolute, left: px(x + 24.0), top: px(y + 20.0), height: px(28.0), ..default() });
    let mut kids = vec![border, panel, header];
    // The tab strip: "LB", VIDEO SETTINGS / SOUND / CONTROLS, "RB" (the shoulder buttons switch, Q / E on the keyboard).
    let (strip_x, strip_w) = (x + 24.0, w - 48.0);
    let tab_w = (strip_w - 2.0 * TAB_SIDE - 2.0 * TAB_GAP) / Tab::ALL.len() as f32;
    for (label, left) in [("LB", strip_x), ("RB", strip_x + strip_w - TAB_SIDE)] {
        let e = ui::spawn_pixel(&mut commands, &art, label, ui::DIM, TAB_TEXT);
        commands.entity(e).insert(Node { position_type: PositionType::Absolute, left: px(left), top: px(y + TAB_Y + (TAB_H - 16.0 * TAB_TEXT) / 2.0), width: px(TAB_SIDE), height: px(16.0 * TAB_TEXT), justify_content: JustifyContent::Center, ..default() });
        kids.push(e);
    }
    for (i, tab) in Tab::ALL.into_iter().enumerate() {
        let left = strip_x + TAB_SIDE + i as f32 * (tab_w + TAB_GAP);
        let text = ui::spawn_pixel(&mut commands, &art, tab.label(), ui::ROW_TEXT, TAB_TEXT);
        let node = Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..ui::abs(left, y + TAB_Y, tab_w, TAB_H) };
        let b = commands.spawn((TabButton { tab, text }, node, BackgroundColor(TAB_OFF))).add_child(text).id();
        kids.push(b);
    }
    for r in 0..MAX_ROWS {
        let (row, _) = ui::spawn_row(&mut commands, &art, ui::abs(x + 24.0, y + ROWS_Y + r as f32 * ROW_PITCH, w - 48.0, ROW_PITCH - 2.0), "", "", 1.25);
        commands.entity(row).insert(MenuRow(r));
        // Every row can be a slider on some tab: a dark track, the filled part and a knob, shown only there.
        let track = commands.spawn((SliderPart { row: r, kind: SliderKind::Track }, ui::abs(SLIDER_X, 15.0, SLIDER_W, 10.0), BackgroundColor(Color::srgb(0.02, 0.10, 0.10)), Visibility::Hidden)).id();
        let fill = commands.spawn((SliderPart { row: r, kind: SliderKind::Fill }, ui::abs(SLIDER_X, 15.0, 0.0, 10.0), BackgroundColor(ui::TEAL), Visibility::Hidden)).id();
        let knob = commands.spawn((SliderPart { row: r, kind: SliderKind::Knob }, ui::abs(SLIDER_X, 7.0, KNOB_W, 26.0), BackgroundColor(ui::YELLOW), Visibility::Hidden)).id();
        commands.entity(row).add_children(&[track, fill, knob]);
        kids.push(row);
    }
    // Prompts inside the panel (the screen underneath has its own bar): one set per kind of page, only what works there
    // (item 2). They and the help line sit under the last row, and the panel ends under them (item 13).
    for (kind, items) in [
        (Prompts::Apply, &[(Glyph::UpDown, "Select"), (Glyph::LeftRight, "Change"), (Glyph::A, "Apply"), (Glyph::B, "Close")][..]),
        (Prompts::Live, &[(Glyph::UpDown, "Select"), (Glyph::LeftRight, "Change"), (Glyph::B, "Close")][..]),
        (Prompts::Look, &[(Glyph::UpDown, "Select"), (Glyph::LeftRight, "Change"), (Glyph::B, "Back")][..]),
        (Prompts::Buttons, &[(Glyph::UpDown, "Select"), (Glyph::A, "Change"), (Glyph::B, "Back")][..]),
    ] {
        let e = ui::prompt_row(&mut commands, &art, items, 18.0, Node { position_type: PositionType::Absolute, left: px(x + 26.0), top: px(y + h - 108.0), height: px(26.0), align_items: AlignItems::Center, ..default() });
        commands.entity(e).insert((PanelPart::Prompts(kind), Visibility::Hidden));
        kids.push(e);
    }
    kids.push(commands.spawn((HelpLine, PanelPart::Help, ui::body(MAIN_HELP, art.ttf.clone(), 13.0, ui::DIM), ui::abs(x + 26.0, y + h - 74.0, w - 48.0, 60.0))).id());
    commands.entity(frame).add_children(&kids);
}

/// The display choice `row` would make from `cur` (the candidate being browsed, or what is applied), one step on.
fn browse_step(row: Row, forward: bool, applied: crate::display::DisplaySettings, cur: Option<crate::display::DisplaySettings>, monitor: Option<&crate::display::MonitorInfo>, screens: &crate::display::Screens) -> crate::display::DisplaySettings {
    use crate::display::{step, Aspect, DisplayMode, HudSize};
    let mut c = cur.unwrap_or(applied);
    match row {
        Row::DisplayMode => c.mode = step(&DisplayMode::ALL, c.mode, forward),
        Row::Screen => c.screen = step(&crate::display::screen_options(screens), c.screen, forward),
        // The sizes this mode (the applied one) can do; borderless has none to choose.
        Row::Resolution => {
            if applied.mode != DisplayMode::Borderless {
                c.resolution = step(&crate::display::resolution_options(applied.mode, monitor), c.resolution, forward);
            }
        }
        Row::Aspect => c.aspect = step(&Aspect::ALL, c.aspect, forward),
        Row::HudSize => c.hud = step(&HudSize::ALL, c.hud, forward),
        _ => {}
    }
    c
}

/// A on a browsed row: the candidate becomes the applied setting and is saved. A new mode keeps the size if that mode
/// offers it (borderless ignores it, so a window size survives a borderless visit) and otherwise falls back to native.
fn apply_browse(settings: &mut Settings, cand: crate::display::DisplaySettings, monitor: Option<&crate::display::MonitorInfo>) {
    use crate::display::{DisplayMode, Resolution};
    let mut d = cand;
    if d.mode != DisplayMode::Borderless && !crate::display::resolution_options(d.mode, monitor).contains(&d.resolution) {
        d.resolution = Resolution::Native;
    }
    settings.display = d;
    settings.display_chosen = true;
    settings.save();
}

/// The help line while a display choice is being browsed.
fn browse_help(row: Row, cand: crate::display::DisplaySettings, applied: crate::display::DisplaySettings, screens: &crate::display::Screens) -> String {
    let (now, then) = match row {
        Row::DisplayMode => (applied.mode.label().to_string(), cand.mode.label().to_string()),
        Row::Screen => (applied.screen.label(screens), cand.screen.label(screens)),
        Row::Resolution => (applied.resolution.label(), cand.resolution.label()),
        Row::Aspect => (applied.aspect.label().to_string(), cand.aspect.label().to_string()),
        _ => (applied.hud.label(), cand.hud.label()),
    };
    format!("Showing {then}: not applied yet. Press A / Enter to apply it. Moving to another row drops it, and {now} stays as it is.")
}

/// Left/right on a row that applies at once (split, camera height).
fn change_live(row: Row, forward: bool, settings: &mut Settings) {
    match row {
        Row::Split => settings.split = if settings.split == Split::SideBySide { Split::TopBottom } else { Split::SideBySide },
        // Cycles rather than steps: five discrete positions, not a
        // continuum, so Left and Right both move to the "adjacent" one.
        Row::CameraHeight => settings.camera_height = if forward { settings.camera_height.next() } else { settings.camera_height.prev() },
        Row::Vibration => settings.rumble = !settings.rumble,
        Row::InvertY => settings.invert_cam_y = !settings.invert_cam_y,
        _ => return,
    }
    settings.save();
}

#[allow(clippy::too_many_arguments)]
pub fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut state: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    app_state: Res<State<crate::frontend::state::AppState>>,
    mut next: ResMut<NextState<crate::frontend::state::AppState>>,
    mut sfx: ResMut<crate::frontend::sound::MenuSfxQueue>,
    monitor: Res<crate::display::CurrentMonitor>,
    screens: Res<crate::display::Screens>,
    mut script: Option<ResMut<crate::frontend::script::MenuScript>>,
) {
    use crate::frontend::state::AppState;
    let in_match = *app_state.get() == AppState::Match;
    // `--menu-script` presses (captures): taken while the overlay is open, or in a match where nothing else reads them.
    let scripted = if state.open || in_match { script.as_deref_mut().and_then(crate::frontend::script::tick) } else { None };
    let sc = scripted.unwrap_or_default();
    let pad = |b: GamepadButton| pads.iter().any(|p| p.just_pressed(b));
    let confirm = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) || pad(GamepadButton::South) || sc.confirm;
    // The mapping page is listening: the next pad button or key is the action's new one (Esc / Start cancel, as does
    // waiting LISTEN_FRAMES). Nothing else reads the press, so a menu button (A, B, LB, RB ...) can be bound like any other.
    if state.open && state.listen.is_some() {
        listen_for_button(&keys, &pads, &sc, &mut state, &mut settings, &mut sfx);
        return;
    }
    // Esc / Start opens the overlay during a match; in the menus Esc means
    // "back" there, so it only closes the overlay (Settings on the title opens it).
    if keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::Start) || sc.start {
        if state.open || in_match {
            let closing_in_match = state.open && in_match;
            if state.dirty {
                settings.save();
            }
            state.open = !state.open;
            state.browse = None;
            state.dirty = false;
            state.page = Page::Main;
            state.note = None;
            state.listen = None;
            // O5 (docs/FRONTIER.md): unpausing plays common id 1 in the
            // original (Laurence's ear, heard on P2's unpause); opening, and
            // any menu-screen close, keeps the survey-guessed Back id.
            sfx.0.push(if closing_in_match { crate::frontend::sound::Cue::Unpause } else { crate::frontend::sound::Cue::Back });
        }
        return;
    }
    if !state.open {
        return;
    }
    // LB / RB (keyboard Q / E; any pad): the tab to the left / right, wrapping. From the Lighting and look page too
    // (it belongs to VIDEO SETTINGS): the new tab opens on its first row. A slider not yet written is saved first.
    let lb = keys.just_pressed(KeyCode::KeyQ) || pad(GamepadButton::LeftTrigger) || sc.lb;
    let rb = keys.just_pressed(KeyCode::KeyE) || pad(GamepadButton::RightTrigger) || sc.rb;
    if lb != rb {
        if state.dirty {
            settings.save();
            state.dirty = false;
        }
        state.tab = state.tab.step(rb);
        state.page = Page::Main;
        state.row = 0;
        state.browse = None;
        state.note = None;
        sfx.0.push(crate::frontend::sound::Cue::List);
        return;
    }
    // The Lighting and look page: its own short list of rows; everything on it applies at once.
    if state.page == Page::Look {
        let rows = crate::look::LOOK_ROWS.len();
        state.row = state.row.min(rows - 1);
        let row = crate::look::LOOK_ROWS[state.row];
        let back = |state: &mut MenuState, sfx: &mut crate::frontend::sound::MenuSfxQueue| {
            state.page = Page::Main;
            state.row = state.back_row;
            sfx.0.push(crate::frontend::sound::Cue::Back);
        };
        if confirm && row == crate::look::LookRow::Back {
            back(&mut state, &mut sfx);
            return;
        }
        if pad(GamepadButton::East) || keys.just_pressed(KeyCode::Backspace) || sc.cancel {
            back(&mut state, &mut sfx);
            return;
        }
        let all: Vec<&Gamepad> = pads.iter().collect();
        let mut a = crate::frontend::input::read_pause(&keys, &all, &mut state.repeat);
        a.up |= sc.up;
        a.down |= sc.down;
        a.left |= sc.left;
        a.right |= sc.right;
        sfx.0.extend(crate::frontend::sound::cues_for(&a));
        if a.up {
            state.row = (state.row + rows - 1) % rows;
        }
        if a.down {
            state.row = (state.row + 1) % rows;
        }
        if a.left || a.right {
            crate::look::change_row(row, a.right, &mut settings.look);
            settings.save();
        }
        return;
    }
    if state.page == Page::Buttons {
        buttons_page_input(&keys, &pads, &sc, confirm, &mut state, &mut settings, &mut sfx);
        return;
    }
    let tab = state.tab;
    let rows = row_count(tab, in_match);
    state.row = state.row.min(rows - 1);
    let Some(row) = row_at(tab, state.row, in_match) else { return };
    // "Quit to title" (matches only).
    if confirm && in_match && row == Row::Quit {
        state.open = false;
        sfx.0.push(crate::frontend::sound::Cue::Confirm);
        next.set(AppState::Title);
        return;
    }
    // "Button mapping": open its page.
    if confirm && row == Row::Buttons {
        state.back_row = state.row;
        state.page = Page::Buttons;
        state.row = 0;
        state.note = None;
        sfx.0.push(crate::frontend::sound::Cue::Confirm);
        return;
    }
    // "Lighting and look": open the second page.
    if confirm && row == Row::Look {
        state.back_row = state.row;
        state.page = Page::Look;
        state.row = 0;
        state.browse = None;
        sfx.0.push(crate::frontend::sound::Cue::Confirm);
        return;
    }
    // A on a display row applies the choice being browsed, at once (no "keep these settings" prompt).
    if confirm && row.browses() {
        if let Some((r, cand)) = state.browse.take() {
            if r == row && cand != settings.display {
                apply_browse(&mut settings, cand, monitor.0.as_ref());
                sfx.0.push(crate::frontend::sound::Cue::Confirm);
            }
        }
        return;
    }
    if pad(GamepadButton::East) || sc.cancel {
        if state.dirty {
            settings.save();
        }
        state.open = false;
        state.browse = None;
        state.dirty = false;
        // Closing the in-match overlay with B counts as unpausing (O5).
        sfx.0.push(if in_match { crate::frontend::sound::Cue::Unpause } else { crate::frontend::sound::Cue::Back });
        return;
    }
    // Hold-to-repeat like every other menu screen (`frontend::input`), so
    // holding Up/Down or Left/Right flies through rows and steps instead
    // of needing one press each.
    let all: Vec<&Gamepad> = pads.iter().collect();
    let mut a = crate::frontend::input::read_pause(&keys, &all, &mut state.repeat);
    a.up |= sc.up;
    a.down |= sc.down;
    a.left |= sc.left;
    a.right |= sc.right;
    // Up / down move the cursor (the program's common/3); left / right change a value on
    // a spinner row: silent in the original (`FUN_14024b000`; common/34 is text entry only).
    sfx.0.extend(crate::frontend::sound::cues_for(&a));
    if a.up || a.down {
        // Leaving a row drops what was being browsed on it, and writes a slider's value.
        if state.dirty {
            settings.save();
            state.dirty = false;
        }
        state.browse = None;
        state.row = if a.up { (state.row + rows - 1) % rows } else { (state.row + 1) % rows };
        return;
    }
    if a.left || a.right {
        let forward = a.right;
        match row {
            r if r.slider().is_some() => {
                let (hl, hr) = state.repeat.held_left_right();
                let held = if forward { hr } else { hl };
                if nudge_slider(r, forward, slider_step(held), &mut settings) {
                    state.dirty = true;
                }
            }
            Row::Split | Row::CameraHeight | Row::Vibration | Row::InvertY => change_live(row, forward, &mut settings),
            r if r.browses() => {
                let cand = browse_step(r, forward, settings.display, state.browse.map(|(_, d)| d), monitor.0.as_ref(), &screens);
                // Back at what is applied is no candidate at all.
                state.browse = (cand != settings.display).then_some((r, cand));
            }
            _ => {}
        }
    }
    // A slider's value is written to disk once the direction is let go.
    if state.dirty && state.repeat.held_left_right() == (0, 0) {
        settings.save();
        state.dirty = false;
    }
}

/// The button-mapping page's keys while it is not listening: up / down, A on a row (listen, reset, back), B / Backspace back.
#[allow(clippy::too_many_arguments)]
fn buttons_page_input(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, sc: &crate::frontend::input::MenuAxis, confirm: bool, state: &mut MenuState, settings: &mut Settings, sfx: &mut crate::frontend::sound::MenuSfxQueue) {
    use crate::frontend::sound::Cue;
    let rows = BUTTON_ROWS;
    state.row = state.row.min(rows - 1);
    let back = |state: &mut MenuState, sfx: &mut crate::frontend::sound::MenuSfxQueue| {
        state.page = Page::Main;
        state.row = state.back_row;
        state.note = None;
        sfx.0.push(Cue::Back);
    };
    if confirm {
        match button_row(state.row) {
            ButtonRow::Action(a) => {
                state.listen = Some((a, 0));
                state.note = None;
                sfx.0.push(Cue::Confirm);
            }
            ButtonRow::Reset => {
                if !settings.bindings.is_default() {
                    settings.bindings = crate::bindings::Bindings::default();
                    settings.save();
                }
                state.note = Some("Back to the original buttons.".to_string());
                sfx.0.push(Cue::Confirm);
            }
        }
        return;
    }
    if pads.iter().any(|p| p.just_pressed(GamepadButton::East)) || keys.just_pressed(KeyCode::Backspace) || sc.cancel {
        back(state, sfx);
        return;
    }
    let all: Vec<&Gamepad> = pads.iter().collect();
    let mut a = crate::frontend::input::read_pause(keys, &all, &mut state.repeat);
    a.up |= sc.up;
    a.down |= sc.down;
    sfx.0.extend(crate::frontend::sound::cues_for(&crate::frontend::input::MenuAxis { left: false, right: false, ..a }));
    if a.up || a.down {
        state.note = None;
        state.row = if a.up { (state.row + rows - 1) % rows } else { (state.row + 1) % rows };
    }
}

/// The mapping page listening for `state.listen`'s action: the first pad button (any pad) or keyboard key pressed
/// this frame becomes its new one (a swap if another action had it, `Bindings::bind_pad` / `bind_key`). Esc and Start
/// cancel; a fixed button or key is refused with the "denied" sound and listening goes on; after `LISTEN_FRAMES` it
/// stops by itself. `--menu-script` presses count as pad buttons (a = A, b = B, x, y, lb, rb; start cancels).
fn listen_for_button(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, sc: &crate::frontend::input::MenuAxis, state: &mut MenuState, settings: &mut Settings, sfx: &mut crate::frontend::sound::MenuSfxQueue) {
    use crate::bindings::{key_label, pad_label, BindError};
    use crate::frontend::sound::Cue;
    let Some((action, waited)) = state.listen else { return };
    let start = pads.iter().any(|p| p.just_pressed(GamepadButton::Start)) || sc.start;
    if keys.just_pressed(KeyCode::Escape) || start || waited >= LISTEN_FRAMES {
        state.listen = None;
        state.note = Some(if waited >= LISTEN_FRAMES { "Nothing pressed: unchanged." } else { "Cancelled: unchanged." }.to_string());
        sfx.0.push(Cue::Back);
        return;
    }
    let scripted = [(sc.confirm, GamepadButton::South), (sc.cancel, GamepadButton::East), (sc.x, GamepadButton::West), (sc.y, GamepadButton::North), (sc.lb, GamepadButton::LeftTrigger), (sc.rb, GamepadButton::RightTrigger)];
    let pressed_pad = pads.iter().flat_map(|p| p.get_just_pressed().copied().collect::<Vec<_>>()).chain(scripted.iter().filter(|(on, _)| *on).map(|(_, b)| *b)).next();
    let pressed_key = keys.get_just_pressed().copied().next();
    let result = match (pressed_pad, pressed_key) {
        (Some(b), _) => Some(settings.bindings.bind_pad(action, b).map(|_| format!("{} is now {} on the pad.", action.label(), pad_label(b)))),
        (None, Some(k)) => Some(settings.bindings.bind_key(action, k).map(|_| format!("{} is now {} on the keyboard.", action.label(), key_label(Some(k))))),
        (None, None) => None,
    };
    match result {
        None => state.listen = Some((action, waited + 1)),
        Some(Ok(msg)) => {
            state.listen = None;
            state.note = Some(msg);
            settings.save();
            sfx.0.push(Cue::Confirm);
        }
        Some(Err(e)) => {
            state.note = Some(match e {
                BindError::Reserved => "That one is fixed (sticks, D-pad, Start, menus, player 2): press another.".to_string(),
                BindError::WouldUnbind(other) => format!("That key is {}'s, and {} has no key to swap: press another.", other.label(), action.label()),
            });
            state.listen = Some((action, waited + 1));
            sfx.0.push(Cue::Denied);
        }
    }
}

/// A row's label and value text on the main pages (`shown`: the display choice to show on it).
fn row_text(kind: Row, settings: &Settings, shown: crate::display::DisplaySettings, monitor: Option<&crate::display::MonitorInfo>, screens: &crate::display::Screens) -> (&'static str, String) {
    use crate::display::{DisplayMode, Resolution};
    let applied = settings.display;
    let on_off = |b: bool| format!("< {} >", if b { "ON" } else { "OFF" });
    match kind {
        Row::Split => ("SPLIT SCREEN", format!("< {} >", if settings.split == Split::SideBySide { "SIDE BY SIDE" } else { "TOP AND BOTTOM" })),
        Row::Fov => ("FIELD OF VIEW", format!("{:.0} DEG", settings.fov_deg)),
        Row::CameraDistance => ("CAMERA DISTANCE", format!("{:.0} %", settings.cam_dist_pct)),
        Row::CameraHeight => ("CAMERA HEIGHT", format!("< {} >", settings.camera_height.label())),
        Row::DisplayMode => ("DISPLAY MODE", format!("< {} >", shown.mode.label())),
        Row::Screen => ("SCREEN", format!("< {} >", shown.screen.label(screens))),
        Row::Resolution => {
            let text = match (shown.resolution, applied.mode, monitor) {
                // Borderless is always the desktop's own size, and a fullscreen "native" is too: say what that is.
                (_, DisplayMode::Borderless, Some(m)) | (Resolution::Native, DisplayMode::Exclusive, Some(m)) => format!("NATIVE ({} X {})", m.size.x, m.size.y),
                (_, DisplayMode::Borderless, None) => "NATIVE".to_string(),
                (r, _, _) => r.label(),
            };
            // Borderless has nothing to choose, so no arrows.
            let arrows = applied.mode != DisplayMode::Borderless;
            ("RESOLUTION", if arrows { format!("< {text} >") } else { text })
        }
        Row::Aspect => ("ASPECT RATIO", format!("< {} >", shown.aspect.label())),
        Row::HudSize => ("HUD AND TEXT SIZE", format!("< {} >", shown.hud.label())),
        Row::Look => ("LIGHTING AND LOOK", format!("{}  >", settings.look.preset().label())),
        Row::MasterVolume => ("MASTER VOLUME", format!("{:.0}", settings.master_vol)),
        Row::MusicVolume => ("MUSIC VOLUME", format!("{:.0}", settings.music_vol)),
        Row::EffectsVolume => ("EFFECTS VOLUME", format!("{:.0}", settings.effects_vol)),
        Row::Vibration => ("VIBRATION", on_off(settings.rumble)),
        Row::InvertY => ("INVERT CAMERA Y", on_off(settings.invert_cam_y)),
        // "CAMERA SENSITIVITY" in full is wider than the space left of the slider bar (the help line spells it out).
        Row::CamSensitivity => ("CAM SENSITIVITY", format!("{:.0} %", settings.cam_sens_pct)),
        Row::Buttons => ("BUTTON MAPPING", format!("{}  >", if settings.bindings.is_default() { "DEFAULT" } else { "CUSTOM" })),
        Row::Quit => ("QUIT TO TITLE", String::new()),
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn menu_view(
    mut commands: Commands,
    state: Res<MenuState>,
    settings: Res<Settings>,
    monitor: Res<crate::display::CurrentMonitor>,
    screens: Res<crate::display::Screens>,
    app_state: Res<State<crate::frontend::state::AppState>>,
    art: Option<Res<crate::frontend::ui::MenuArt>>,
    windows: Query<&Window>,
    mut frames: Query<&mut UiTransform, With<crate::frontend::ui::DesignFrame>>,
    mut root: Query<&mut Visibility, (With<MenuRoot>, Without<MenuRow>, Without<SliderPart>)>,
    mut rows: Query<(&MenuRow, &crate::frontend::ui::RowRefs, &mut ImageNode, &mut Visibility), (Without<MenuRoot>, Without<SliderPart>)>,
    mut parts: Query<(&SliderPart, &mut Node, &mut Visibility, &mut BackgroundColor), (Without<MenuRoot>, Without<MenuRow>, Without<TabButton>)>,
    mut tabs: Query<(&TabButton, &mut BackgroundColor), Without<SliderPart>>,
    mut texts: Query<&mut crate::frontend::ui::PixelText>,
    mut help: Query<&mut Text, With<HelpLine>>,
    mut panel_parts: Query<(&PanelPart, &mut Node, &mut Visibility), (Without<MenuRoot>, Without<MenuRow>, Without<SliderPart>, Without<TabButton>)>,
) {
    use crate::frontend::ui;
    let in_match = *app_state.get() == crate::frontend::state::AppState::Match;
    for mut v in &mut root {
        let want = if state.open { Visibility::Visible } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if !state.open {
        return;
    }
    ui::fit_frames(&windows, &mut frames);
    let Some(art) = art else { return };
    let applied = settings.display;
    let main = state.page == Page::Main;
    let tab = state.tab;
    // The tab strip: the showing tab lit like a selected row.
    for (b, mut bg) in &mut tabs {
        let on = b.tab == tab;
        let want = if on { ui::PANEL_SEL } else { TAB_OFF };
        if bg.0 != want {
            bg.0 = want;
        }
        if let Ok(mut pt) = texts.get_mut(b.text) {
            let s = pt.scale;
            ui::set_pixel(&mut commands, &art, b.text, &mut pt, b.tab.label(), if on { ui::YELLOW } else { ui::ROW_TEXT }, s);
        }
    }
    let cur_row = row_at(tab, state.row, in_match);
    // The candidate on the highlighted display row, if one is being browsed.
    let candidate = state.browse.filter(|(r, d)| main && cur_row == Some(*r) && *d != applied).map(|(_, d)| d);
    // The help line: the keys / what the tab is about on the main pages (what a browsed choice does while one is shown),
    // the highlighted choice's help on the Lighting and look page.
    let help_now = match (state.page, candidate) {
        (Page::Main, Some(c)) => browse_help(cur_row.unwrap_or(Row::DisplayMode), c, applied, &screens),
        (Page::Main, None) if cur_row == Some(Row::Look) => "Optional modern lighting: bloom, shadow quality, tone curve and more effect lights. Everything is OFF / ORIGINAL by default.".to_string(),
        (Page::Main, None) => tab_help(tab, cur_row).to_string(),
        (Page::Look, _) => crate::look::help_text(crate::look::LOOK_ROWS[state.row.min(crate::look::LOOK_ROWS.len() - 1)], &settings.look).to_string(),
        (Page::Buttons, _) => match (state.listen, &state.note) {
            (Some((a, _)), note) => format!("{}Press the new pad button or keyboard key for {}.  One already in use swaps with it.  Esc / Start cancel (or wait five seconds).", note.as_ref().map_or(String::new(), |n| format!("{n}  ")), a.label()),
            (None, Some(note)) => format!("{note}  {BUTTONS_HELP}"),
            (None, None) => BUTTONS_HELP.to_string(),
        },
    };
    for mut t in &mut help {
        if t.0 != help_now {
            t.0 = help_now.clone();
        }
    }
    // The panel ends under this page's rows, with the page's own prompts (items 2 and 13).
    let (n_rows, prompts) = match state.page {
        Page::Main => (row_count(tab, in_match), if tab == Tab::Video { Prompts::Apply } else { Prompts::Live }),
        Page::Look => (crate::look::LOOK_ROWS.len(), Prompts::Look),
        Page::Buttons => (BUTTON_ROWS, Prompts::Buttons),
    };
    let (_, y, _, _) = PANEL;
    let h = panel_height(n_rows);
    let prompt_top = y + ROWS_Y + n_rows as f32 * ROW_PITCH + PROMPT_GAP;
    for (part, mut node, mut vis) in &mut panel_parts {
        let (top, height, shown) = match *part {
            PanelPart::Border => (y - 2.0, Some(h + 4.0), true),
            PanelPart::Back => (y, Some(h), true),
            PanelPart::Prompts(k) => (prompt_top, None, k == prompts),
            PanelPart::Help => (prompt_top + 34.0, None, true),
        };
        if node.top != Val::Px(top) {
            node.top = Val::Px(top);
        }
        if let Some(hh) = height {
            if node.height != Val::Px(hh) {
                node.height = Val::Px(hh);
            }
        }
        let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    for (row, refs, mut panel, mut vis) in &mut rows {
        // The Lighting and look page uses the first rows of the same list.
        if state.page == Page::Look {
            let want = if row.0 < crate::look::LOOK_ROWS.len() { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != want {
                *vis = want;
            }
            let Some(&kind) = crate::look::LOOK_ROWS.get(row.0) else { continue };
            let selected = row.0 == state.row;
            let (tint, color) = if selected { (ui::PANEL_SEL, ui::YELLOW) } else { (ui::PANEL, ui::ROW_TEXT) };
            let value = if kind == crate::look::LookRow::Back { String::new() } else { format!("< {} >", crate::look::value_text(kind, &settings.look)) };
            ui::set_row(&mut commands, &art, *refs, &mut panel, &mut texts, tint, ("", color), (kind.label(), color), (&value, color));
            continue;
        }
        // The button-mapping page: every row, the action with its pad button / key ("PRESS ..." while listening).
        if state.page == Page::Buttons {
            let want = if row.0 < BUTTON_ROWS { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != want {
                *vis = want;
            }
            if row.0 >= BUTTON_ROWS {
                continue;
            }
            let selected = row.0 == state.row;
            let (tint, color) = if selected { (ui::PANEL_SEL, ui::YELLOW) } else { (ui::PANEL, ui::ROW_TEXT) };
            let (label, value, badge) = match button_row(row.0) {
                ButtonRow::Action(a) if state.listen.map(|(l, _)| l) == Some(a) => (a.label(), "PRESS A BUTTON OR KEY".to_string(), "LISTEN"),
                ButtonRow::Action(a) => (a.label(), format!("{}  /  {}", crate::bindings::pad_label(settings.bindings.pad(a)), crate::bindings::key_label(settings.bindings.key(a))), ""),
                ButtonRow::Reset => ("RESET TO DEFAULTS", String::new(), ""),
            };
            let value_color = if badge.is_empty() { color } else { CANDIDATE };
            ui::set_row(&mut commands, &art, *refs, &mut panel, &mut texts, tint, (badge, ui::YELLOW), (label, color), (&value, value_color));
            continue;
        }
        // Each tab shows its own rows from the top (Quit to title last, in a match); the rest are hidden.
        let kind = row_at(tab, row.0, in_match);
        let want = if kind.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let Some(kind) = kind else { continue };
        let selected = row.0 == state.row;
        // What this row shows: the candidate being browsed on it, else what is applied.
        let shown = if selected { candidate.unwrap_or(applied) } else { applied };
        let pending = selected && candidate.is_some();
        let (label, value) = row_text(kind, &settings, shown, monitor.0.as_ref(), &screens);
        let (tint, color) = if selected { (ui::PANEL_SEL, ui::YELLOW) } else { (ui::PANEL, ui::ROW_TEXT) };
        // A browsed choice is dimmed, and the tab says what A does.
        let (value_color, badge) = if pending { (CANDIDATE, ("A: APPLY", ui::YELLOW)) } else { (color, ("", color)) };
        ui::set_row(&mut commands, &art, *refs, &mut panel, &mut texts, tint, badge, (label, color), (&value, value_color));
    }
    // The slider bars: on the rows that are sliders on this tab (main pages only; the Look page reuses the same row entities).
    for (part, mut node, mut vis, mut bg) in &mut parts {
        let range = if main { row_at(tab, part.row, in_match).and_then(|k| k.slider().map(|r| (k, r))) } else { None };
        let want = if range.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let Some((kind, (min, max))) = range else { continue };
        let f = slider_fraction(slider_value(kind, &settings), min, max);
        let selected = part.row == state.row;
        match part.kind {
            SliderKind::Track => {}
            SliderKind::Fill => {
                let want = Val::Px((SLIDER_W * f).round());
                if node.width != want {
                    node.width = want;
                }
                let c = if selected { ui::YELLOW } else { ui::TEAL };
                if bg.0 != c {
                    bg.0 = c;
                }
            }
            SliderKind::Knob => {
                let want = Val::Px((SLIDER_X + (SLIDER_W - KNOB_W) * f).round());
                if node.left != want {
                    node.left = want;
                }
                let c = if selected { ui::WHITE } else { ui::ROW_TEXT };
                if bg.0 != c {
                    bg.0 = c;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{Aspect, DisplayMode, DisplaySettings, HudSize, MonitorInfo, Resolution};
    use bevy::window::VideoMode;

    fn tv_1080_desktop() -> MonitorInfo {
        let vm = |w, h, hz: u32| VideoMode { physical_size: UVec2::new(w, h), bit_depth: 32, refresh_rate_millihertz: hz * 1000 };
        MonitorInfo { size: UVec2::new(1920, 1080), scale: 1.0, modes: vec![vm(3840, 2160, 60), vm(2560, 1440, 60), vm(1920, 1080, 60), vm(1280, 720, 60)] }
    }

    fn settings() -> Settings {
        let mut s = Settings::load(std::env::temp_dir().join(format!("pd_menu_unit_{}.json", std::process::id())));
        s.disable_saving();
        s.display = DisplaySettings::default();
        s
    }

    #[test]
    fn a_tap_moves_a_slider_one_unit_and_holding_speeds_up() {
        assert_eq!([0, 1, 39].map(slider_step), [1.0; 3], "a tap and the first 39 frames: one unit");
        assert_eq!([40, 79, 80, 139, 140, 600].map(slider_step), [2.0, 2.0, 4.0, 4.0, 8.0, 8.0]);
        let mut s = settings();
        s.fov_deg = 60.0;
        assert!(nudge_slider(Row::Fov, true, 1.0, &mut s));
        assert_eq!(s.fov_deg, 61.0);
        assert!(nudge_slider(Row::Fov, false, 8.0, &mut s));
        assert_eq!(s.fov_deg, 53.0);
        // The ends hold, and report no change.
        s.fov_deg = FOV_MAX;
        assert!(!nudge_slider(Row::Fov, true, 8.0, &mut s));
        assert!(nudge_slider(Row::Fov, false, 100.0, &mut s) && s.fov_deg == FOV_MIN);
        s.cam_dist_pct = CAM_DIST_DEFAULT;
        assert!(nudge_slider(Row::CameraDistance, true, 4.0, &mut s) && s.cam_dist_pct == 104.0);
        assert!(!nudge_slider(Row::Split, true, 1.0, &mut s), "only sliders slide");
        assert_eq!((slider_fraction(FOV_MIN, FOV_MIN, FOV_MAX), slider_fraction(FOV_MAX, FOV_MIN, FOV_MAX), slider_fraction(5000.0, 0.0, 10.0)), (0.0, 1.0, 1.0));
        assert!((slider_fraction(100.0, CAM_DIST_MIN, CAM_DIST_MAX) - 30.0 / 90.0).abs() < 1e-6);
    }

    #[test]
    fn browsing_changes_the_candidate_and_never_the_applied_setting() {
        let m = tv_1080_desktop();
        let sc = crate::display::Screens::default();
        let applied = DisplaySettings::default();
        // Resolution, windowed on a 1080p desktop: the candidates reach 4K (the biggest mode), one step at a time.
        let c1 = browse_step(Row::Resolution, true, applied, None, Some(&m), &sc);
        assert_eq!((c1.resolution, applied.resolution), (Resolution::Fixed(3840, 2160), Resolution::Native));
        let c2 = browse_step(Row::Resolution, true, applied, Some(c1), Some(&m), &sc);
        assert_eq!(c2.resolution, Resolution::Fixed(2560, 1440));
        assert_eq!(browse_step(Row::Resolution, false, applied, Some(c2), Some(&m), &sc).resolution, Resolution::Fixed(3840, 2160));
        // Each of the other rows touches only its own field.
        assert_eq!(browse_step(Row::DisplayMode, true, applied, None, Some(&m), &sc).mode, DisplayMode::Borderless);
        assert_eq!(browse_step(Row::Aspect, true, applied, None, Some(&m), &sc).aspect, Aspect::W16H9);
        assert_eq!(browse_step(Row::HudSize, true, applied, None, Some(&m), &sc).hud, HudSize::P125);
        // Borderless has no sizes to browse.
        let b = DisplaySettings { mode: DisplayMode::Borderless, ..applied };
        assert_eq!(browse_step(Row::Resolution, true, b, None, Some(&m), &sc), b);
    }

    #[test]
    fn applying_a_choice_keeps_a_size_the_new_mode_offers_and_marks_it_chosen() {
        let m = tv_1080_desktop();
        let mut s = settings();
        s.display = DisplaySettings { resolution: Resolution::Fixed(1280, 720), ..DisplaySettings::default() };
        assert!(!s.display_chosen);
        // Window 1280x720 -> exclusive keeps 1280x720 (a video mode); -> borderless keeps it stored (ignored there).
        let d = s.display;
        apply_browse(&mut s, DisplaySettings { mode: DisplayMode::Exclusive, ..d }, Some(&m));
        assert_eq!((s.display.mode, s.display.resolution, s.display_chosen), (DisplayMode::Exclusive, Resolution::Fixed(1280, 720), true));
        let d = s.display;
        apply_browse(&mut s, DisplaySettings { mode: DisplayMode::Borderless, ..d }, Some(&m));
        assert_eq!(s.display.resolution, Resolution::Fixed(1280, 720));
        // A size the new mode does not offer falls back to native.
        let d = s.display;
        apply_browse(&mut s, DisplaySettings { mode: DisplayMode::Exclusive, resolution: Resolution::Fixed(1600, 900), ..d }, Some(&m));
        assert_eq!(s.display.resolution, Resolution::Native);
    }

    #[test]
    fn the_browse_help_says_it_is_not_applied() {
        let a = DisplaySettings::default();
        let c = DisplaySettings { resolution: Resolution::Fixed(3840, 2160), ..a };
        let t = browse_help(Row::Resolution, c, a, &crate::display::Screens::default());
        assert!(t.contains("3840 X 2160 (4K)") && t.contains("not applied") && t.contains("NATIVE"), "{t}");
    }

    #[test]
    fn the_video_tab_keeps_the_old_rows_in_order_with_the_screen_row_after_display_mode() {
        // The old main page was split, FOV, distance, height, mode, resolution, aspect, HUD, look, vibration, quit:
        // the Video tab is that list without vibration (it moved to CONTROLS) and with SCREEN under DISPLAY MODE.
        assert_eq!(VIDEO_ROWS, [Row::Split, Row::Fov, Row::CameraDistance, Row::CameraHeight, Row::DisplayMode, Row::Screen, Row::Resolution, Row::Aspect, Row::HudSize, Row::Look]);
        assert_eq!(ROWS, 11, "ten rows and Quit to title in a match");
        assert_eq!(VIDEO_ROWS.iter().position(|r| *r == Row::Look), Some(9));
        assert_eq!(VIDEO_ROWS.iter().filter(|r| r.browses()).count(), 5);
        assert_eq!(VIDEO_ROWS.iter().filter(|r| r.slider().is_some()).count(), 2);
        assert!(!VIDEO_ROWS.contains(&Row::Vibration));
    }

    #[test]
    fn rows_per_tab_and_quit_only_in_a_match() {
        assert_eq!(SOUND_ROWS, [Row::MasterVolume, Row::MusicVolume, Row::EffectsVolume]);
        assert_eq!(CONTROLS_ROWS, [Row::Vibration, Row::InvertY, Row::CamSensitivity, Row::Buttons]);
        assert!(BUTTON_ROWS <= MAX_ROWS);
        assert_eq!((button_row(0), button_row(8)), (ButtonRow::Action(crate::bindings::Action::Dodge), ButtonRow::Reset));
        assert_eq!(BUTTON_ROWS, 9, "the eight actions and RESET; B goes back");
        // The panel fits its rows: the Sound tab (3 rows) is much shorter than VIDEO SETTINGS in a match (11).
        assert!(panel_height(3) + 200.0 < panel_height(11));
        assert!(panel_height(MAX_ROWS) <= PANEL.3);
        for tab in Tab::ALL {
            let n = tab.rows().len();
            assert_eq!((row_count(tab, false), row_count(tab, true)), (n, n + 1), "{tab:?}");
            assert_eq!(row_at(tab, n, true), Some(Row::Quit), "{tab:?}: quit is last in a match");
            assert_eq!(row_at(tab, n, false), None, "{tab:?}: no quit on the title");
            assert!(row_count(tab, true) <= MAX_ROWS, "{tab:?} fits the row entities");
            // No row is on two tabs.
            for other in Tab::ALL.into_iter().filter(|t| *t != tab) {
                assert!(tab.rows().iter().all(|r| !other.rows().contains(r)));
            }
        }
        assert!(SOUND_ROWS.iter().chain(&[Row::CamSensitivity]).all(|r| r.slider().is_some()));
        assert!(Row::Buttons.slider().is_none() && !Row::Buttons.browses());
        assert_eq!((Row::MasterVolume.slider(), Row::CamSensitivity.slider()), (Some((0.0, 100.0)), Some((50.0, 200.0))));
        assert!(panel_height(MAX_ROWS) < PANEL.3 + 0.5 && ROWS_Y + MAX_ROWS as f32 * ROW_PITCH + PROMPT_GAP + 84.0 <= PANEL.3, "eleven rows and the prompts fit the panel");
    }

    #[test]
    fn tabs_step_both_ways_and_wrap() {
        assert_eq!(Tab::default(), Tab::Video);
        assert_eq!([Tab::Video, Tab::Sound, Tab::Controls].map(|t| t.step(true)), [Tab::Sound, Tab::Controls, Tab::Video]);
        assert_eq!([Tab::Video, Tab::Sound, Tab::Controls].map(|t| t.step(false)), [Tab::Controls, Tab::Video, Tab::Sound]);
        assert_eq!(Tab::ALL.map(Tab::label), ["VIDEO SETTINGS", "SOUND", "CONTROLS"]);
        assert_eq!(["video", "Sound", "CONTROLS", "x"].map(Tab::parse), [Some(Tab::Video), Some(Tab::Sound), Some(Tab::Controls), None]);
    }

    #[test]
    fn the_new_settings_default_to_today_and_round_trip_through_settings_json() {
        let path = std::env::temp_dir().join(format!("pd_menu_tabs_rt_{}.json", std::process::id()));
        // An old file (before the tabs): none of the new keys. Everything new loads at its default.
        std::fs::write(&path, r#"{"split":"top_bottom","fov_h16":80.0,"rumble":false}"#).unwrap();
        let s = Settings::load(path.clone());
        assert_eq!((s.split, s.fov_deg, s.rumble), (Split::TopBottom, 80.0, false), "old keys still load");
        assert_eq!((s.master_vol, s.music_vol, s.effects_vol, s.invert_cam_y, s.cam_sens_pct), (100.0, 100.0, 100.0, false, 100.0));
        // Changed and saved, then loaded again.
        let mut s = s;
        (s.master_vol, s.music_vol, s.effects_vol, s.invert_cam_y, s.cam_sens_pct) = (80.0, 35.0, 0.0, true, 150.0);
        assert!(s.bindings.is_default(), "an old file plays with the old buttons");
        s.bindings.bind_pad(crate::bindings::Action::Dodge, GamepadButton::South).unwrap();
        s.save();
        let text = std::fs::read_to_string(&path).unwrap();
        for key in ["vol_master", "vol_music", "vol_effects", "invert_cam_y", "cam_sens_pct", "rumble", "fov_h16", "bindings", "dodge"] {
            assert!(text.contains(key), "{key} in {text}");
        }
        let back = Settings::load(path.clone());
        assert_eq!((back.master_vol, back.music_vol, back.effects_vol, back.invert_cam_y, back.cam_sens_pct), (80.0, 35.0, 0.0, true, 150.0));
        assert_eq!((back.split, back.fov_deg, back.rumble), (Split::TopBottom, 80.0, false));
        assert_eq!(back.bindings, s.bindings, "the mapping round-trips");
        assert_eq!(back.bindings.pad(crate::bindings::Action::SkillBottom), GamepadButton::LeftThumb, "the swap was saved");
        // Junk and out-of-range values: defaults or the nearest end.
        std::fs::write(&path, r#"{"vol_master":"loud","vol_music":250,"vol_effects":-5,"invert_cam_y":"yes","cam_sens_pct":10}"#).unwrap();
        let j = Settings::load(path.clone());
        assert_eq!((j.master_vol, j.music_vol, j.effects_vol, j.invert_cam_y, j.cam_sens_pct), (100.0, 100.0, 0.0, false, 50.0));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sensitivity_and_invert_shape_the_look_input_and_the_defaults_change_nothing() {
        let mut s = settings();
        // 100 % and off: bit-identical to the raw input, for every value the stick / keys can give.
        for cam in [-1.0f32, -0.73, -0.2, 0.0, 0.21, 0.5, 1.0] {
            for pitch in [-1.0f32, -0.4, 0.0, 0.33, 1.0] {
                let (c, p) = s.shape_look(cam, pitch);
                assert_eq!((c.to_bits(), p.to_bits()), (cam.to_bits(), pitch.to_bits()));
            }
        }
        // Invert flips look up / down only; the turn is untouched.
        s.invert_cam_y = true;
        assert_eq!(s.shape_look(0.5, 0.8), (0.5, -0.8));
        s.invert_cam_y = false;
        // 50 %: half of both.
        s.cam_sens_pct = 50.0;
        assert_eq!(s.shape_look(0.8, -0.6), (0.4, -0.3));
        // 200 %: the turn and the pitch both double in full (the turn past the original's top speed: Laurence 2026-10-08).
        s.cam_sens_pct = 200.0;
        assert_eq!(s.shape_look(0.3, 0.3), (0.6, 0.6));
        assert_eq!(s.shape_look(0.8, 0.8), (1.6, 1.6));
        assert_eq!(s.shape_look(-1.0, -1.0), (-2.0, -2.0));
    }

    #[test]
    fn the_new_rows_change_their_settings() {
        let mut s = settings();
        assert!(nudge_slider(Row::MasterVolume, false, 1.0, &mut s) && s.master_vol == 99.0);
        assert!(!nudge_slider(Row::MusicVolume, true, 1.0, &mut s), "100 is the top");
        assert!(nudge_slider(Row::EffectsVolume, false, 200.0, &mut s) && s.effects_vol == 0.0);
        assert!(nudge_slider(Row::CamSensitivity, true, 8.0, &mut s) && s.cam_sens_pct == 108.0);
        assert!(nudge_slider(Row::CamSensitivity, false, 500.0, &mut s) && s.cam_sens_pct == SENS_MIN);
        change_live(Row::InvertY, true, &mut s);
        assert!(s.invert_cam_y);
        change_live(Row::InvertY, false, &mut s);
        assert!(!s.invert_cam_y);
        let r = s.rumble;
        change_live(Row::Vibration, true, &mut s);
        assert_eq!(s.rumble, !r);
        // Each row has a label and a value.
        for row in SOUND_ROWS.iter().chain(&CONTROLS_ROWS) {
            let (label, value) = row_text(*row, &s, s.display, None, &crate::display::Screens::default());
            assert!(!label.is_empty() && !value.is_empty(), "{row:?}");
        }
        assert_eq!(row_text(Row::CamSensitivity, &s, s.display, None, &crate::display::Screens::default()).1, "50 %");
    }
}
