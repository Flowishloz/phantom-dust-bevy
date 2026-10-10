//! Display options: window mode, resolution, aspect ratio and HUD size
//! (docs/FRONTEND.md "Display options").
//!
//! Everything here is presentation. It never touches the sim: it chooses the
//! window's mode and size, carves a **game area** out of the window (the whole
//! window by default, or the largest rectangle of a fixed aspect ratio,
//! centred, with black bars round it) and scales the battle HUD. Every system
//! that used to ask "how big is the window?" now asks [`GameArea`], so split
//! screens, the HUD, the menus and the field of view all lay out inside it.
//!
//! A window or area of the default shape (auto aspect, HUD 100%) touches no
//! camera: the area is the window. A first run with no saved display choice
//! starts borderless fullscreen on the monitor the window opens on, at that
//! monitor's own (physical) size ([`DisplaySettings::first_run`]).
//!
//! Sizes are PHYSICAL pixels everywhere in this file (the monitor, the video
//! modes, the window plan, the game area); only [`GameArea::logical`] and the
//! UI's design frame divide by the scale factor. Windows on a 4K TV often run
//! at 200 to 300 % scaling, where the logical size is a half or a third of the
//! real one.

use crate::menu::Settings;
use bevy::camera::{ClearColorConfig, RenderTarget, Viewport};
use bevy::prelude::*;
use bevy::window::{Monitor, MonitorSelection, OnMonitor, PrimaryMonitor, PrimaryWindow, VideoMode, VideoModeSelection, WindowMode, WindowPosition};
use std::sync::atomic::{AtomicU64, Ordering};

/// A window whose aspect is within this fraction of the chosen ratio counts
/// as already that ratio (no bars): a "21:9" monitor is 2560x1080, 3440x1440
/// or 3840x1600, a "16:9" laptop panel is 1366x768. 3% is well under the gap
/// between any two of the offered ratios (16:9 vs 16:10 is 11%).
pub const SNAP: f64 = 0.03;

// ---------------------------------------------------------------------------
// The options

/// Window, borderless fullscreen (a screen-sized window; instant, no mode
/// switch) or exclusive fullscreen (the game takes the monitor over and
/// switches its video mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DisplayMode {
    #[default]
    Windowed,
    Borderless,
    Exclusive,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 3] = [DisplayMode::Windowed, DisplayMode::Borderless, DisplayMode::Exclusive];

    pub fn label(self) -> &'static str {
        match self {
            DisplayMode::Windowed => "WINDOWED",
            DisplayMode::Borderless => "BORDERLESS",
            DisplayMode::Exclusive => "EXCLUSIVE",
        }
    }

    fn key(self) -> &'static str {
        match self {
            DisplayMode::Windowed => "windowed",
            DisplayMode::Borderless => "borderless",
            DisplayMode::Exclusive => "exclusive",
        }
    }

    fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == s)
    }
}

/// The size the window or the exclusive video mode uses.
///
/// `Native` is the monitor's own size: in exclusive fullscreen the mode the
/// monitor is already in; in a window "leave the window as it is" (so the
/// default look is unchanged); borderless always means native.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Resolution {
    #[default]
    Native,
    Fixed(u32, u32),
}

impl Resolution {
    /// The sizes offered, besides Native: 16:9 first (4K down to 720p), then
    /// 16:10, the ultrawides (21:9, 32:9) and 4:3. Each is shown only when
    /// the monitor can do it (`resolution_options`).
    pub const CANDIDATES: [(u32, u32); 21] = [
        (3840, 2160),
        (2560, 1440),
        (1920, 1080),
        (1600, 900),
        (1280, 720),
        (2560, 1600),
        (1920, 1200),
        (1680, 1050),
        (1440, 900),
        (1280, 800),
        (5120, 2160),
        (3840, 1600),
        (3440, 1440),
        (2560, 1080),
        (5120, 1440),
        (1600, 1200),
        (1400, 1050),
        (1280, 960),
        (1024, 768),
        (800, 600),
        (640, 480),
    ];

    /// The text in the settings row.
    pub fn label(self) -> String {
        match self {
            Resolution::Native => "NATIVE".to_string(),
            Resolution::Fixed(w, h) => match (w, h) {
                (3840, 2160) => "3840 X 2160 (4K)".to_string(),
                _ => format!("{w} X {h}"),
            },
        }
    }

    fn key(self) -> String {
        match self {
            Resolution::Native => "native".to_string(),
            Resolution::Fixed(w, h) => format!("{w}x{h}"),
        }
    }

    fn from_key(s: &str) -> Option<Self> {
        if s == "native" {
            return Some(Resolution::Native);
        }
        let (w, h) = s.split_once('x')?;
        let (w, h) = (w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?);
        (w >= 320 && h >= 200 && w <= 16384 && h <= 16384).then_some(Resolution::Fixed(w, h))
    }
}

/// The game area's aspect ratio. `Auto` fills the window; the others
/// letterbox / pillarbox the game, menus and HUD to that shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Aspect {
    #[default]
    Auto,
    W16H9,
    W16H10,
    W21H9,
    W4H3,
}

impl Aspect {
    pub const ALL: [Aspect; 5] = [Aspect::Auto, Aspect::W16H9, Aspect::W16H10, Aspect::W21H9, Aspect::W4H3];

    pub fn label(self) -> &'static str {
        match self {
            Aspect::Auto => "AUTO (FILL SCREEN)",
            Aspect::W16H9 => "16:9",
            Aspect::W16H10 => "16:10",
            Aspect::W21H9 => "21:9",
            Aspect::W4H3 => "4:3",
        }
    }

    /// (width, height) of the ratio; `None` for Auto.
    pub fn ratio(self) -> Option<(u32, u32)> {
        match self {
            Aspect::Auto => None,
            Aspect::W16H9 => Some((16, 9)),
            Aspect::W16H10 => Some((16, 10)),
            Aspect::W21H9 => Some((21, 9)),
            Aspect::W4H3 => Some((4, 3)),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Aspect::Auto => "auto",
            _ => self.label(),
        }
    }

    fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == s)
    }

    /// "auto", "16:9", "16:10", "21:9" or "4:3" (the settings file and the `PD_TEST_ASPECT` capture switch).
    pub fn parse(s: &str) -> Option<Self> {
        Self::from_key(s)
    }
}

/// Battle HUD size, for sitting across a room from a 55" TV (75% is for small
/// windows). It multiplies the HUD's own fit-to-view scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HudSize {
    P75,
    #[default]
    P100,
    P125,
    P150,
}

impl HudSize {
    pub const ALL: [HudSize; 4] = [HudSize::P75, HudSize::P100, HudSize::P125, HudSize::P150];

    pub fn percent(self) -> u32 {
        match self {
            HudSize::P75 => 75,
            HudSize::P100 => 100,
            HudSize::P125 => 125,
            HudSize::P150 => 150,
        }
    }

    pub fn scale(self) -> f32 {
        self.percent() as f32 / 100.0
    }

    pub fn label(self) -> String {
        format!("{}%", self.percent())
    }

    fn from_percent(p: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|h| h.percent() == p)
    }
}

/// Step through `all` from `cur` (wrapping); an unknown `cur` lands on the first.
pub fn step<T: Copy + PartialEq>(all: &[T], cur: T, forward: bool) -> T {
    let n = all.len();
    if n == 0 {
        return cur;
    }
    let i = all.iter().position(|x| *x == cur).unwrap_or(0);
    all[if forward { (i + 1) % n } else { (i + n - 1) % n }]
}

/// Which screen the game opens on. `Auto` is the screen that can show the most
/// pixels (a 4K TV beats a laptop panel, whatever Windows calls "main"), so a
/// TV plugged in as an extended display is the default. `Number(n)` is the nth
/// screen counted left to right in Windows' display layout (1 = leftmost).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Auto,
    Number(u8),
}

impl Screen {
    /// The text in the settings row; a numbered screen shows its current size so the TV is easy to tell from the laptop.
    pub fn label(self, screens: &Screens) -> String {
        match self {
            Screen::Auto => "AUTO (BIGGEST SCREEN)".to_string(),
            Screen::Number(n) => match screens.nth(n) {
                Some(s) => format!("SCREEN {n} ({} X {})", s.size.x, s.size.y),
                None => format!("SCREEN {n} (NOT FOUND)"),
            },
        }
    }

    fn key(self) -> String {
        match self {
            Screen::Auto => "auto".to_string(),
            Screen::Number(n) => n.to_string(),
        }
    }

    fn from_key(s: &str) -> Option<Self> {
        if s == "auto" {
            return Some(Screen::Auto);
        }
        s.trim().parse::<u8>().ok().filter(|n| (1..=16).contains(n)).map(Screen::Number)
    }

    /// The monitor this choice means right now. A numbered screen that is not connected (the TV is off) falls back to Auto.
    pub fn resolve(self, screens: &Screens) -> Option<Entity> {
        if let Screen::Number(n) = self {
            if let Some(s) = screens.nth(n) {
                return Some(s.entity);
            }
        }
        // Most pixels it can show, then the main screen on a tie.
        screens.0.iter().max_by_key(|s| (s.reach.x as u64 * s.reach.y as u64, s.primary)).map(|s| s.entity)
    }
}

/// One connected screen, as the Screen row lists them.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenInfo {
    pub entity: Entity,
    /// Top-left in Windows' virtual desktop (physical pixels); screens are numbered left to right by this.
    pub pos: IVec2,
    /// Its desktop size now, physical pixels.
    pub size: UVec2,
    /// The most it can show: the biggest width and height of any of its video modes (a TV left at 1080p still reaches 4K).
    pub reach: UVec2,
    pub primary: bool,
}

/// Every connected screen, leftmost first (`track_screens`).
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct Screens(pub Vec<ScreenInfo>);

impl Screens {
    /// The nth screen counted from 1.
    pub fn nth(&self, n: u8) -> Option<&ScreenInfo> {
        (n as usize).checked_sub(1).and_then(|i| self.0.get(i))
    }
}

/// The choices the Screen row steps through: Auto, then each connected screen.
pub fn screen_options(screens: &Screens) -> Vec<Screen> {
    std::iter::once(Screen::Auto).chain((1..=screens.0.len().min(16) as u8).map(Screen::Number)).collect()
}

/// Keeps [`Screens`] current (a TV switched on or off changes the list).
pub fn track_screens(monitors: Query<(Entity, &Monitor, Has<PrimaryMonitor>)>, mut screens: ResMut<Screens>) {
    let mut v: Vec<ScreenInfo> = monitors
        .iter()
        .map(|(entity, m, primary)| {
            let info = MonitorInfo::from_monitor(m);
            ScreenInfo { entity, pos: m.physical_position, size: info.size, reach: info.max_extent(), primary }
        })
        .collect();
    v.sort_by_key(|s| (s.pos.x, s.pos.y));
    if screens.0 != v {
        screens.0 = v;
    }
}

/// Everything the Display rows set. Stored in `settings.json` (see
/// [`DisplaySettings::from_json`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DisplaySettings {
    pub mode: DisplayMode,
    pub resolution: Resolution,
    pub aspect: Aspect,
    pub hud: HudSize,
    pub screen: Screen,
}

impl DisplaySettings {
    /// What a first run (no saved display choice) starts with: borderless
    /// fullscreen, the monitor's own size, the whole screen, HUD 100%. The
    /// monitor is the one the window opens on and "own size" its desktop's
    /// physical pixels, so a 4K TV at any Windows scaling gets all of its
    /// pixels without the player touching a thing. (`Default` stays windowed:
    /// it is what captures and older code paths mean by "no setting".)
    pub fn first_run() -> Self {
        DisplaySettings { mode: DisplayMode::Borderless, ..Default::default() }
    }

    /// Reads the keys `display_mode`, `resolution`, `aspect`, `hud_scale`, `screen`; an
    /// absent or unrecognised value keeps its default, so an old settings file
    /// loads as today's look.
    pub fn from_json(v: &serde_json::Value) -> Self {
        DisplaySettings {
            mode: v["display_mode"].as_str().and_then(DisplayMode::from_key).unwrap_or_default(),
            resolution: v["resolution"].as_str().and_then(Resolution::from_key).unwrap_or_default(),
            aspect: v["aspect"].as_str().and_then(Aspect::from_key).unwrap_or_default(),
            hud: v["hud_scale"].as_u64().and_then(|p| HudSize::from_percent(p as u32)).unwrap_or_default(),
            screen: v["screen"].as_str().and_then(Screen::from_key).unwrap_or_default(),
        }
    }

    /// Adds this struct's keys to a settings object.
    pub fn write_json(&self, v: &mut serde_json::Value) {
        v["display_mode"] = self.mode.key().into();
        v["resolution"] = self.resolution.key().into();
        v["aspect"] = self.aspect.key().into();
        v["hud_scale"] = self.hud.percent().into();
        v["screen"] = self.screen.key().into();
    }

    /// The part that changes the window itself (mode and size); aspect and HUD size only change the layout inside it.
    pub fn window_part(&self) -> (DisplayMode, Resolution) {
        (self.mode, self.resolution)
    }
}

// ---------------------------------------------------------------------------
// The monitor

/// What the settings screen and the mode planner need to know about the
/// monitor the window is on.
#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    /// The monitor's size in physical pixels: its desktop (the video mode it is in now), not the DPI-scaled logical size.
    pub size: UVec2,
    /// Physical pixels per logical pixel on it (Windows scaling: 1.0, 1.5, 2.0, 3.0 ...).
    pub scale: f32,
    /// Its video modes (what exclusive fullscreen can use).
    pub modes: Vec<VideoMode>,
}

/// The smallest size the Resolution row offers (the menus and HUD are not laid out for less).
pub const MIN_SIZE: UVec2 = UVec2::new(640, 480);

impl MonitorInfo {
    /// From Bevy's monitor entity. `physical_size()` is winit's `MonitorHandle::size`, physical pixels (winit makes the process per-monitor DPI aware); the scale factor is kept separately and never divides the size.
    pub fn from_monitor(m: &Monitor) -> Self {
        MonitorInfo { size: m.physical_size(), scale: if m.scale_factor > 0.0 { m.scale_factor as f32 } else { 1.0 }, modes: m.video_modes.clone() }
    }

    /// The monitor's biggest video mode (most pixels, then refresh rate, then colour depth).
    pub fn largest_mode(&self) -> Option<VideoMode> {
        self.modes.iter().copied().max_by_key(|m| (m.physical_size.x as u64 * m.physical_size.y as u64, m.refresh_rate_millihertz, m.bit_depth))
    }

    /// The biggest width and the biggest height the monitor can show, taking its desktop and every video mode: a 4K TV whose desktop runs at 1920x1080 still reaches 3840x2160.
    pub fn max_extent(&self) -> UVec2 {
        self.modes.iter().fold(self.size, |acc, m| acc.max(m.physical_size))
    }

    /// Every distinct video-mode size of at least [`MIN_SIZE`], biggest first.
    pub fn mode_sizes(&self) -> Vec<UVec2> {
        let mut v: Vec<UVec2> = self.modes.iter().map(|m| m.physical_size).filter(|s| s.x >= MIN_SIZE.x && s.y >= MIN_SIZE.y).collect();
        v.sort_by_key(|s| std::cmp::Reverse((s.x as u64 * s.y as u64, s.x)));
        v.dedup();
        v
    }

    /// The startup log line (`display: monitor 3840x2160 scale 3.0 ...`): what this monitor reports, so a run on a new TV says what it has.
    pub fn log_line(&self, window_scale: f32) -> String {
        let mut sizes: Vec<String> = Vec::new();
        for s in self.mode_sizes() {
            let hz: Vec<u32> = {
                let mut h: Vec<u32> = self.modes.iter().filter(|m| m.physical_size == s).map(|m| (m.refresh_rate_millihertz + 500) / 1000).collect();
                h.sort_unstable_by(|a, b| b.cmp(a));
                h.dedup();
                h
            };
            sizes.push(format!("{}x{}@{}", s.x, s.y, hz.iter().map(u32::to_string).collect::<Vec<_>>().join("/")));
        }
        let largest = self.largest_mode().map_or("none".to_string(), |m| format!("{}x{}@{}", m.physical_size.x, m.physical_size.y, (m.refresh_rate_millihertz + 500) / 1000));
        format!("display: monitor {}x{} scale {:.1} (window scale {:.1}) largest mode {} modes [{}]", self.size.x, self.size.y, self.scale, window_scale, largest, sizes.join(" "))
    }
}

/// The monitor under the game window (`None` until the OS has reported one).
#[derive(Resource, Default, Clone, Debug)]
pub struct CurrentMonitor(pub Option<MonitorInfo>);

/// The resolutions the Resolution row offers in `mode` on `monitor`:
/// Native first, then:
/// * Exclusive: **every video mode the monitor reports** (distinct sizes of at
///   least 640x480, biggest first), so whatever the TV says it can do is there,
///   3840x2160 included;
/// * Windowed: the usual sizes ([`Resolution::CANDIDATES`]) up to the monitor's
///   biggest mode (not only its current desktop: a TV left at 1080p still lists
///   4K), plus the desktop's own size if it is not a usual one;
/// * Borderless: Native only (it is always the desktop's own size).
///
/// With no monitor information every usual size is offered.
pub fn resolution_options(mode: DisplayMode, monitor: Option<&MonitorInfo>) -> Vec<Resolution> {
    let mut out = vec![Resolution::Native];
    if mode == DisplayMode::Borderless {
        return out;
    }
    let Some(m) = monitor else {
        out.extend(Resolution::CANDIDATES.iter().map(|&(w, h)| Resolution::Fixed(w, h)));
        return out;
    };
    match mode {
        DisplayMode::Exclusive => out.extend(m.mode_sizes().into_iter().map(|s| Resolution::Fixed(s.x, s.y))),
        _ => {
            let max = m.max_extent();
            if m.size.x >= MIN_SIZE.x && m.size.y >= MIN_SIZE.y && !Resolution::CANDIDATES.contains(&(m.size.x, m.size.y)) {
                out.push(Resolution::Fixed(m.size.x, m.size.y));
            }
            out.extend(Resolution::CANDIDATES.iter().filter(|&&(w, h)| w <= max.x && h <= max.y).map(|&(w, h)| Resolution::Fixed(w, h)));
        }
    }
    out
}

/// A plain window can be as big as the monitor, but not bigger than 90% of its
/// desktop when the size was not asked for (the default window, a window kept
/// "native"): shrinks `size` to fit, keeping its shape.
pub fn fit_window(size: UVec2, monitor: Option<&MonitorInfo>) -> UVec2 {
    let Some(m) = monitor else { return size };
    let (lim_x, lim_y) = (m.size.x as u64 * 9 / 10, m.size.y as u64 * 9 / 10);
    if lim_x == 0 || lim_y == 0 || (size.x as u64 <= lim_x && size.y as u64 <= lim_y) {
        return size;
    }
    let k = (lim_x as f64 / size.x as f64).min(lim_y as f64 / size.y as f64);
    UVec2::new(((size.x as f64 * k).round() as u32).max(MIN_SIZE.x), ((size.y as f64 * k).round() as u32).max(MIN_SIZE.y))
}

/// The window the game opens with (1600x900 LOGICAL: `main.rs` creates it from
/// `film_window()`, and winit reads that as logical), as physical pixels on
/// `monitor`, shrunk to fit. At 300% scaling that is 4800x2700 physical, bigger
/// than a 4K screen, so it is fitted.
pub fn default_window_size(monitor: &MonitorInfo, logical: (u32, u32)) -> UVec2 {
    let s = monitor.scale.max(0.1);
    fit_window(UVec2::new((logical.0 as f32 * s).round() as u32, (logical.1 as f32 * s).round() as u32), Some(monitor))
}

/// What to do to the window for a mode and resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPlan {
    pub mode: WindowMode,
    /// Physical size to ask for (windowed only); `None` = leave it.
    pub size: Option<UVec2>,
}

/// The window the game opens with, in logical pixels (`main.rs` `film_window()`'s default).
pub const DEFAULT_WINDOW_LOGICAL: (u32, u32) = (1600, 900);

/// The window's mode and size for `d` on `monitor`. `windowed_size` is the
/// size the window had last time it was a plain window, so "native" in a
/// window means "as it was" (shrunk to fit the monitor; with no size known
/// yet, the default window fitted to it). A chosen size is used as asked.
/// Exclusive with a size picks that size's best video
/// mode (highest refresh rate, then deepest colour); a size with no video mode
/// falls back to the mode the monitor is in now.
pub fn plan(d: &DisplaySettings, monitor: Option<&MonitorInfo>, windowed_size: Option<UVec2>) -> WindowPlan {
    match d.mode {
        DisplayMode::Windowed => WindowPlan {
            mode: WindowMode::Windowed,
            size: match d.resolution {
                Resolution::Native => match (windowed_size, monitor) {
                    (Some(s), m) => Some(fit_window(s, m)),
                    (None, Some(m)) => Some(default_window_size(m, DEFAULT_WINDOW_LOGICAL)),
                    (None, None) => None,
                },
                Resolution::Fixed(w, h) => Some(UVec2::new(w, h)),
            },
        },
        DisplayMode::Borderless => WindowPlan { mode: WindowMode::BorderlessFullscreen(MonitorSelection::Current), size: None },
        DisplayMode::Exclusive => {
            let pick = match d.resolution {
                Resolution::Native => VideoModeSelection::Current,
                Resolution::Fixed(w, h) => monitor
                    .and_then(|m| {
                        m.modes
                            .iter()
                            .filter(|vm| vm.physical_size == UVec2::new(w, h))
                            .max_by_key(|vm| (vm.refresh_rate_millihertz, vm.bit_depth))
                            .copied()
                    })
                    .map_or(VideoModeSelection::Current, VideoModeSelection::Specific),
            };
            WindowPlan { mode: WindowMode::Fullscreen(MonitorSelection::Current, pick), size: None }
        }
    }
}

/// The window mode to create the window with, so a saved fullscreen choice
/// starts fullscreen instead of flashing a window first. (An exclusive mode
/// with a chosen size needs the monitor's mode list, so that one is applied by
/// [`apply_display`] once the OS has reported the monitor.)
pub fn startup_mode(d: &DisplaySettings) -> WindowMode {
    match (d.mode, d.resolution) {
        (DisplayMode::Borderless, _) => WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
        (DisplayMode::Exclusive, Resolution::Native) => WindowMode::Fullscreen(MonitorSelection::Primary, VideoModeSelection::Current),
        _ => WindowMode::Windowed,
    }
}

// ---------------------------------------------------------------------------
// The game area

/// The largest rectangle of aspect `ratio` that fits in `window`, centred:
/// (position, size) in physical pixels. `None`, a zero window, or a window
/// already within [`SNAP`] of the ratio gives the whole window. Integer
/// arithmetic, so 16:9 in 1920x1080 is exactly the window.
pub fn letterbox(window: UVec2, ratio: Option<(u32, u32)>) -> (UVec2, UVec2) {
    let Some((n, d)) = ratio.filter(|&(n, d)| n > 0 && d > 0) else { return (UVec2::ZERO, window) };
    if window.x == 0 || window.y == 0 {
        return (UVec2::ZERO, window);
    }
    let (wx, wy, n, d) = (window.x as u64, window.y as u64, n as u64, d as u64);
    // Window aspect over the ratio: > 1 means the window is the wider one.
    let rel = (wx * d) as f64 / (wy * n) as f64;
    if (rel - 1.0).abs() <= SNAP {
        return (UVec2::ZERO, window);
    }
    let size = if wx * d > wy * n {
        UVec2::new(((wy * n + d / 2) / d).clamp(1, wx) as u32, window.y)
    } else {
        UVec2::new(window.x, ((wx * d + n / 2) / n).clamp(1, wy) as u32)
    };
    ((window - size) / 2, size)
}

/// Where the game is drawn inside the window.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Default)]
pub struct GameArea {
    /// Top-left corner in the window, physical pixels.
    pub pos: UVec2,
    /// Size in physical pixels (zero until the window has a size).
    pub size: UVec2,
    /// The whole window, physical pixels.
    pub window: UVec2,
    /// Physical pixels per logical pixel.
    pub scale: f32,
}

impl GameArea {
    pub fn from_window(window: UVec2, scale: f32, aspect: Aspect) -> Self {
        let (pos, size) = letterbox(window, aspect.ratio());
        GameArea { pos, size, window, scale: if scale > 0.0 { scale } else { 1.0 } }
    }

    /// Size in logical pixels (the HUD's and the menus' unit).
    pub fn logical(&self) -> Vec2 {
        self.size.as_vec2() / self.scale.max(0.01)
    }

    /// True when black bars are showing.
    pub fn bars(&self) -> bool {
        self.size != self.window
    }

    pub fn ready(&self) -> bool {
        self.size.x > 0 && self.size.y > 0
    }
}

// Code with no access to the resource (the shaders' per-material uniforms,
// `views::quadrant_of_origin`, the menu frames' fit) reads the area through
// these mirrors, which `update_area` rewrites every frame.
static UI_SIZE: AtomicU64 = AtomicU64::new(0);

fn pack(x: u32, y: u32) -> u64 {
    (x as u64) << 32 | y as u64
}

fn unpack(v: u64) -> (u32, u32) {
    ((v >> 32) as u32, v as u32)
}

/// The game area's top-left in the window (physical pixels).
pub fn area_origin() -> UVec2 {
    crate::effects::area_origin()
}

/// The game area's size in logical pixels, or the window's when the area has
/// not been published yet: what a 1280x720 design frame has to fit.
pub fn ui_size(windows: &Query<&Window>) -> Option<Vec2> {
    let (x, y) = unpack(UI_SIZE.load(Ordering::Relaxed));
    if x > 0 && y > 0 {
        return Some(Vec2::new(f32::from_bits(x), f32::from_bits(y)));
    }
    windows.iter().next().map(|w| Vec2::new(w.width(), w.height()))
}

/// Recomputes the [`GameArea`] from the window and the aspect setting.
pub fn update_area(windows: Query<&Window, With<PrimaryWindow>>, settings: Res<Settings>, mut area: ResMut<GameArea>) {
    let Ok(w) = windows.single() else { return };
    let new = GameArea::from_window(w.physical_size(), w.scale_factor(), settings.display.aspect);
    if *area != new {
        *area = new;
    }
    crate::effects::set_area_origin(new.pos);
    let l = new.logical();
    UI_SIZE.store(pack(l.x.to_bits(), l.y.to_bits()), Ordering::Relaxed);
}

/// Marks the camera that paints the bars black.
#[derive(Component)]
pub struct BarsCamera;

/// A camera below everything that clears the whole window black, so the area
/// outside the game is black bars and not whatever the last frame left. It is
/// switched on only while bars exist, so the default path is untouched.
pub fn spawn_bars_camera(mut commands: Commands) {
    commands.spawn((BarsCamera, Camera2d, Camera { order: -100, is_active: false, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() }));
}

/// Keeps every camera that is not a player view (the HUD, the menu screens,
/// the settings overlay, the character preview) inside the game area, and the
/// bars camera on exactly while bars exist. Player views are laid out by
/// `layout_viewports` (`ViewLayout::rect` inside the area).
#[allow(clippy::type_complexity)]
pub fn fit_cameras(area: Res<GameArea>, mut cams: Query<(&mut Camera, &RenderTarget, Has<BarsCamera>), Without<crate::PlayerCamera>>) {
    if !area.ready() {
        return;
    }
    let want = area.bars().then(|| (area.pos, area.size));
    for (mut cam, target, bars) in &mut cams {
        if bars {
            if cam.is_active != area.bars() {
                cam.is_active = area.bars();
            }
            continue;
        }
        if !matches!(target, RenderTarget::Window(_)) {
            continue;
        }
        let now = cam.viewport.as_ref().map(|v| (v.physical_position, v.physical_size));
        if now != want {
            cam.viewport = want.map(|(p, s)| Viewport { physical_position: p, physical_size: s, ..default() });
        }
    }
}

// ---------------------------------------------------------------------------
// Applying the window mode and size

/// Finds the monitor under the window and keeps [`CurrentMonitor`] current.
///
/// Runs every frame (it is a few monitors and a compare): the window's
/// `OnMonitor` link is re-inserted when the window is dragged to another
/// screen, which is a *change*, not an *addition*, so a system waiting for
/// `Added<OnMonitor>` stayed on the first monitor for the rest of the run
/// (docs/BUG-GRAVEYARD.md, "Resolution list stuck on the first monitor").
#[allow(clippy::type_complexity)]
pub fn track_monitor(
    windows: Query<Option<&OnMonitor>, With<PrimaryWindow>>,
    monitors: Query<(Entity, Ref<Monitor>, Has<PrimaryMonitor>)>,
    mut cur: ResMut<CurrentMonitor>,
    mut last: Local<Option<Entity>>,
) {
    let on = windows.iter().next().flatten().map(|m| m.0);
    let pick = monitors.iter().find(|(e, _, _)| Some(*e) == on).or_else(|| monitors.iter().find(|(_, _, p)| *p)).or_else(|| monitors.iter().next());
    let key = pick.as_ref().map(|(e, _, _)| *e);
    if *last == key && cur.0.is_some() == key.is_some() && !pick.as_ref().is_some_and(|(_, m, _)| m.is_changed()) {
        return;
    }
    *last = key;
    let new = pick.map(|(_, m, _)| MonitorInfo::from_monitor(&m));
    if cur.0 != new {
        cur.0 = new;
    }
}

/// Logs what the monitor reports, once per monitor the window is on (`display: monitor 3840x2160 scale 3.0 ...`), to
/// stderr and, next to `settings.json`, to `display_log.txt` (a double-clicked
/// game has no console), so a run on a new TV says what that TV reports.
/// Captures write no file.
pub fn log_display(monitor: Res<CurrentMonitor>, windows: Query<&Window, With<PrimaryWindow>>, settings: Res<Settings>, mut last: Local<Option<MonitorInfo>>) {
    let Some(m) = &monitor.0 else { return };
    let Ok(w) = windows.single() else { return };
    if last.as_ref() == Some(m) {
        return;
    }
    *last = Some(m.clone());
    let line = m.log_line(w.scale_factor());
    eprintln!("{line}");
    settings.write_sibling("display_log.txt", &format!("{line}\n"));
}

/// The size the window had the last time it was a plain window.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct WindowedSize(pub Option<UVec2>);

/// Where the "open on this screen" move has got to.
#[derive(Default)]
pub struct ScreenMove {
    /// The choice the window has been brought to (or given up on).
    done: Option<Screen>,
    /// A move has been asked for and the window has not reported being there yet.
    issued: bool,
    /// Frames spent waiting for it.
    waited: u32,
}

/// What `apply_display` does about the Screen choice this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenStep {
    /// Already on the right screen (or no screens listed to choose from): go on with the mode and size.
    Ready,
    /// Move the window to this monitor, then wait.
    Move(Entity),
    /// A move is under way.
    Wait,
}

/// Frames to wait for a requested move to show up before going on anyway (a window that never reports its monitor).
const MOVE_PATIENCE: u32 = 90;

/// One frame of the screen choice. It acts once per change of choice (and once at startup), so a window the player
/// drags to another screen by hand is left where it is.
pub fn screen_step(st: &mut ScreenMove, want: Screen, screens: &Screens, on: Option<Entity>) -> ScreenStep {
    if st.done == Some(want) {
        return ScreenStep::Ready;
    }
    // No screens listed (yet): nothing to choose between. Stay undone, so the move still happens when they appear.
    let Some(target) = want.resolve(screens) else { return ScreenStep::Ready };
    if on == Some(target) {
        *st = ScreenMove { done: Some(want), ..Default::default() };
        return ScreenStep::Ready;
    }
    if !st.issued && on.is_some() {
        st.issued = true;
        st.waited = 0;
        return ScreenStep::Move(target);
    }
    st.waited += 1;
    if st.waited > MOVE_PATIENCE {
        *st = ScreenMove { done: Some(want), ..Default::default() };
        return ScreenStep::Ready;
    }
    ScreenStep::Wait
}

/// Puts the window in the mode and size of `settings.display` whenever the
/// mode or resolution part changes (including once at startup), on the screen
/// the Screen row names. Automated screenshot runs never touch the display. A
/// windowed size and an exclusive fixed size wait until the monitor is known
/// (the first frames).
#[allow(clippy::too_many_arguments)]
pub fn apply_display(
    settings: Res<Settings>,
    monitor: Res<CurrentMonitor>,
    screens: Res<Screens>,
    cfg: Res<crate::Config>,
    mut remembered: ResMut<WindowedSize>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    mut applied: Local<Option<(DisplayMode, Resolution)>>,
    mut moving: Local<ScreenMove>,
) {
    if cfg.screenshot.is_some() {
        return;
    }
    let Ok((mut w, on)) = windows.single_mut() else { return };
    let on = on.map(|m| m.0);
    // Remember a plain window's size while it is one and nothing is changing.
    if w.mode == WindowMode::Windowed && applied.is_none_or(|a| a.0 == DisplayMode::Windowed) && w.physical_size().x > 0 && !moving.issued {
        let size = w.physical_size();
        if remembered.0 != Some(size) {
            remembered.0 = Some(size);
        }
    }
    // The screen first: move the window there, and only then pick its size (the new screen may differ in size and scaling).
    match screen_step(&mut moving, settings.display.screen, &screens, on) {
        ScreenStep::Ready => {}
        ScreenStep::Wait => return,
        ScreenStep::Move(target) => {
            let to = MonitorSelection::Entity(target);
            match w.mode {
                WindowMode::Windowed => w.position = WindowPosition::Centered(to),
                WindowMode::BorderlessFullscreen(_) => w.mode = WindowMode::BorderlessFullscreen(to),
                WindowMode::Fullscreen(..) => w.mode = WindowMode::Fullscreen(to, VideoModeSelection::Current),
            }
            *applied = None;
            return;
        }
    }
    let want = settings.display.window_part();
    if *applied == Some(want) {
        return;
    }
    let needs_monitor = match want {
        (DisplayMode::Exclusive, Resolution::Fixed(..)) | (DisplayMode::Windowed, _) => true,
        _ => false,
    };
    if needs_monitor && monitor.0.is_none() {
        return; // the monitor's mode list is not known yet
    }
    let p = plan(&settings.display, monitor.0.as_ref(), remembered.0);
    if w.mode != p.mode {
        w.mode = p.mode;
    }
    if let Some(s) = p.size {
        if w.resolution.physical_size() != s {
            w.resolution.set_physical_resolution(s.x, s.y);
        }
    }
    *applied = Some(want);
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(w: u32, h: u32, hz: u32, bits: u16) -> VideoMode {
        VideoMode { physical_size: UVec2::new(w, h), bit_depth: bits, refresh_rate_millihertz: hz * 1000 }
    }

    fn tv() -> MonitorInfo {
        // A 4K TV: 4K at 60 and 120, 1440p, 1080p and 720p, but no 16:10 or ultrawide.
        MonitorInfo { size: UVec2::new(3840, 2160), scale: 2.0, modes: vec![vm(3840, 2160, 60, 32), vm(3840, 2160, 120, 32), vm(2560, 1440, 60, 32), vm(1920, 1080, 60, 32), vm(1920, 1080, 120, 32), vm(1280, 720, 60, 32)] }
    }

    fn ultrawide() -> MonitorInfo {
        MonitorInfo { size: UVec2::new(3440, 1440), scale: 1.0, modes: vec![vm(3440, 1440, 100, 32), vm(2560, 1440, 60, 32), vm(2560, 1080, 60, 32), vm(1920, 1080, 60, 32), vm(1600, 900, 60, 32), vm(1280, 720, 60, 32), vm(1280, 800, 60, 32)] }
    }

    #[test]
    fn defaults_are_todays_look() {
        let d = DisplaySettings::default();
        assert_eq!((d.mode, d.resolution, d.aspect, d.hud), (DisplayMode::Windowed, Resolution::Native, Aspect::Auto, HudSize::P100));
        assert_eq!(d.hud.scale(), 1.0);
        assert_eq!(Aspect::Auto.ratio(), None);
        assert_eq!(startup_mode(&d), WindowMode::Windowed);
        // The default area is the whole window, any window.
        for w in [UVec2::new(1600, 900), UVec2::new(3840, 2160), UVec2::new(1281, 721), UVec2::new(2560, 1080)] {
            let a = GameArea::from_window(w, 1.0, Aspect::Auto);
            assert_eq!((a.pos, a.size, a.bars()), (UVec2::ZERO, w, false));
        }
    }

    #[test]
    fn option_lists_have_the_requested_values() {
        assert_eq!(DisplayMode::ALL.map(|m| m.label()), ["WINDOWED", "BORDERLESS", "EXCLUSIVE"]);
        assert_eq!(Aspect::ALL.map(|a| a.label()), ["AUTO (FILL SCREEN)", "16:9", "16:10", "21:9", "4:3"]);
        assert_eq!(HudSize::ALL.map(|h| h.percent()), [75, 100, 125, 150]);
        // With no monitor information: Native first, then the 16:9 ladder from 4K down.
        let all = resolution_options(DisplayMode::Windowed, None);
        assert_eq!(all[0], Resolution::Native);
        assert_eq!(&all[1..6], &[Resolution::Fixed(3840, 2160), Resolution::Fixed(2560, 1440), Resolution::Fixed(1920, 1080), Resolution::Fixed(1600, 900), Resolution::Fixed(1280, 720)]);
        assert!(all.contains(&Resolution::Fixed(2560, 1600)) && all.contains(&Resolution::Fixed(3440, 1440)) && all.contains(&Resolution::Fixed(1024, 768)));
        assert_eq!(Resolution::Fixed(3840, 2160).label(), "3840 X 2160 (4K)");
        assert_eq!(Resolution::Fixed(1600, 900).label(), "1600 X 900");
        assert_eq!(Resolution::Native.label(), "NATIVE");
    }

    #[test]
    fn exclusive_offers_only_what_the_monitor_can_do() {
        let tv = tv();
        let got = resolution_options(DisplayMode::Exclusive, Some(&tv));
        assert_eq!(got, vec![Resolution::Native, Resolution::Fixed(3840, 2160), Resolution::Fixed(2560, 1440), Resolution::Fixed(1920, 1080), Resolution::Fixed(1280, 720)]);
        // 1600x900 is a candidate but not a mode of this TV: hidden.
        assert!(!got.contains(&Resolution::Fixed(1600, 900)));
        let uw = ultrawide();
        let got = resolution_options(DisplayMode::Exclusive, Some(&uw));
        assert!(got.contains(&Resolution::Fixed(2560, 1080)) && got.contains(&Resolution::Fixed(3440, 1440)) && got.contains(&Resolution::Fixed(1280, 800)));
        assert!(!got.contains(&Resolution::Fixed(3840, 2160)), "bigger than the monitor");
    }

    #[test]
    fn a_window_offers_everything_that_fits_the_screen_and_borderless_only_native() {
        let uw = ultrawide();
        let got = resolution_options(DisplayMode::Windowed, Some(&uw));
        assert!(got.contains(&Resolution::Fixed(1600, 900)) && got.contains(&Resolution::Fixed(3440, 1440)));
        assert!(!got.contains(&Resolution::Fixed(3840, 2160)) && !got.contains(&Resolution::Fixed(2560, 1600)), "taller than 1440");
        assert_eq!(resolution_options(DisplayMode::Borderless, Some(&uw)), vec![Resolution::Native]);
        assert_eq!(resolution_options(DisplayMode::Borderless, None), vec![Resolution::Native]);
    }

    #[test]
    fn stepping_wraps_and_an_unknown_value_lands_on_the_first() {
        let l = resolution_options(DisplayMode::Exclusive, Some(&tv()));
        assert_eq!(step(&l, Resolution::Native, false), Resolution::Fixed(1280, 720), "left from the first wraps to the last");
        assert_eq!(step(&l, Resolution::Fixed(1280, 720), true), Resolution::Native);
        assert_eq!(step(&l, Resolution::Fixed(1600, 900), true), Resolution::Fixed(3840, 2160), "unknown counts as the first, then steps");
        assert_eq!(step(&Aspect::ALL, Aspect::W4H3, true), Aspect::Auto);
        assert_eq!(step(&HudSize::ALL, HudSize::P75, false), HudSize::P150);
    }

    #[test]
    fn plans_for_each_mode() {
        let tv = tv();
        let mut d = DisplaySettings::default();
        // Windowed native: the remembered size, else the default 1600x900 LOGICAL window fitted to the screen (scale 2.0 here = 3200x1800 physical).
        assert_eq!(plan(&d, Some(&tv), None), WindowPlan { mode: WindowMode::Windowed, size: Some(UVec2::new(3200, 1800)) });
        assert_eq!(plan(&d, None, None), WindowPlan { mode: WindowMode::Windowed, size: None }, "no monitor known: leave it");
        assert_eq!(plan(&d, Some(&tv), Some(UVec2::new(1600, 900))).size, Some(UVec2::new(1600, 900)));
        d.resolution = Resolution::Fixed(2560, 1440);
        assert_eq!(plan(&d, Some(&tv), Some(UVec2::new(1600, 900))).size, Some(UVec2::new(2560, 1440)));
        // Borderless ignores the resolution.
        d.mode = DisplayMode::Borderless;
        assert_eq!(plan(&d, Some(&tv), None), WindowPlan { mode: WindowMode::BorderlessFullscreen(MonitorSelection::Current), size: None });
        // Exclusive: the best video mode of that size (120 Hz, not 60).
        d.mode = DisplayMode::Exclusive;
        d.resolution = Resolution::Fixed(1920, 1080);
        match plan(&d, Some(&tv), None).mode {
            WindowMode::Fullscreen(_, VideoModeSelection::Specific(m)) => assert_eq!((m.physical_size, m.refresh_rate_millihertz), (UVec2::new(1920, 1080), 120_000)),
            other => panic!("{other:?}"),
        }
        // Exclusive native, or a size the monitor lacks: the current mode.
        d.resolution = Resolution::Native;
        assert_eq!(plan(&d, Some(&tv), None).mode, WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current));
        d.resolution = Resolution::Fixed(1600, 900);
        assert_eq!(plan(&d, Some(&tv), None).mode, WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current));
    }

    /// The 4K TV Laurence plays on, in the ways Windows can present it: the desktop at 4K or left at 1080p, and any scaling.
    /// Every list must reach 3840x2160 and never be cut down by the scale factor.
    fn tv_4k(desktop: (u32, u32), scale: f32) -> MonitorInfo {
        MonitorInfo { size: UVec2::new(desktop.0, desktop.1), scale, modes: tv().modes }
    }

    #[test]
    fn a_4k_tv_lists_and_switches_to_4k_whatever_its_desktop_or_scale() {
        for desktop in [(3840, 2160), (1920, 1080)] {
            for scale in [1.0, 2.0, 3.0] {
                let m = tv_4k(desktop, scale);
                let k4 = Resolution::Fixed(3840, 2160);
                // Exclusive: every mode the monitor reports, biggest first, 4K included.
                let ex = resolution_options(DisplayMode::Exclusive, Some(&m));
                assert_eq!(ex[..3], [Resolution::Native, k4, Resolution::Fixed(2560, 1440)], "desktop {desktop:?} scale {scale}");
                assert_eq!(ex.len(), 1 + 4, "all four distinct sizes the TV reports, no more");
                // Windowed: up to the biggest mode, not only the current desktop.
                assert!(resolution_options(DisplayMode::Windowed, Some(&m)).contains(&k4), "windowed 4K on a {desktop:?} desktop at scale {scale}");
                // Borderless is the desktop, Native only.
                assert_eq!(resolution_options(DisplayMode::Borderless, Some(&m)), vec![Resolution::Native]);
                // Switching to 4K in exclusive picks the 120 Hz 4K mode.
                let d = DisplaySettings { mode: DisplayMode::Exclusive, resolution: k4, ..Default::default() };
                match plan(&d, Some(&m), None).mode {
                    WindowMode::Fullscreen(_, VideoModeSelection::Specific(v)) => assert_eq!((v.physical_size, v.refresh_rate_millihertz), (UVec2::new(3840, 2160), 120_000)),
                    other => panic!("{other:?}"),
                }
                // A windowed 4K is asked for in physical pixels, as chosen.
                let w = DisplaySettings { resolution: k4, ..Default::default() };
                assert_eq!(plan(&w, Some(&m), None).size, Some(UVec2::new(3840, 2160)));
            }
        }
    }

    #[test]
    fn monitor_info_keeps_physical_pixels_and_the_scale_separately() {
        for scale in [1.0f64, 2.0, 3.0] {
            let m = Monitor { name: None, physical_height: 2160, physical_width: 3840, physical_position: IVec2::ZERO, refresh_rate_millihertz: Some(60_000), scale_factor: scale, video_modes: tv().modes };
            let info = MonitorInfo::from_monitor(&m);
            assert_eq!((info.size, info.scale), (UVec2::new(3840, 2160), scale as f32), "the size is never divided by the scale");
            assert_eq!(info.largest_mode().map(|v| (v.physical_size, v.refresh_rate_millihertz)), Some((UVec2::new(3840, 2160), 120_000)));
            assert_eq!(info.max_extent(), UVec2::new(3840, 2160));
            // The game area at that scale: physical size kept, logical is size / scale.
            let a = GameArea::from_window(info.size, info.scale, Aspect::Auto);
            assert_eq!((a.size, a.logical()), (UVec2::new(3840, 2160), Vec2::new(3840.0, 2160.0) / scale as f32));
        }
    }

    #[test]
    fn the_startup_log_line_names_size_scale_and_largest_mode() {
        let line = tv_4k((3840, 2160), 3.0).log_line(3.0);
        assert!(line.starts_with("display: monitor 3840x2160 scale 3.0 "), "{line}");
        assert!(line.contains("largest mode 3840x2160@120") && line.contains("3840x2160@120/60") && line.contains("1280x720@60"), "{line}");
    }

    #[test]
    fn the_default_window_is_fitted_to_the_screen() {
        // 1600x900 LOGICAL at 300% is 4800x2700 physical: bigger than a 4K screen, so it is shrunk to 90% of the desktop.
        let m = tv_4k((3840, 2160), 3.0);
        assert_eq!(default_window_size(&m, DEFAULT_WINDOW_LOGICAL), UVec2::new(3456, 1944));
        assert_eq!(default_window_size(&tv_4k((3840, 2160), 2.0), DEFAULT_WINDOW_LOGICAL), UVec2::new(3200, 1800), "already fits");
        assert_eq!(default_window_size(&tv_4k((1920, 1080), 1.0), DEFAULT_WINDOW_LOGICAL), UVec2::new(1600, 900));
        // A window kept "native" that overflowed (the start-up size at 300%) is shrunk; a chosen size never is.
        let d = DisplaySettings::default();
        assert_eq!(plan(&d, Some(&m), Some(UVec2::new(4800, 2700))).size, Some(UVec2::new(3456, 1944)));
        let fixed = DisplaySettings { resolution: Resolution::Fixed(3840, 2160), ..d };
        assert_eq!(plan(&fixed, Some(&tv_4k((1920, 1080), 1.0)), None).size, Some(UVec2::new(3840, 2160)));
    }

    #[test]
    fn a_first_run_is_borderless_native_auto() {
        let d = DisplaySettings::first_run();
        assert_eq!((d.mode, d.resolution, d.aspect, d.hud), (DisplayMode::Borderless, Resolution::Native, Aspect::Auto, HudSize::P100));
        assert_eq!(startup_mode(&d), WindowMode::BorderlessFullscreen(MonitorSelection::Primary));
        assert_eq!(plan(&d, Some(&tv_4k((3840, 2160), 3.0)), None), WindowPlan { mode: WindowMode::BorderlessFullscreen(MonitorSelection::Current), size: None });
    }

    /// The monitor follows the window: dragged to another screen, the lists change with it.
    #[test]
    fn track_monitor_follows_the_window_to_another_screen() {
        let mut app = App::new();
        app.init_resource::<CurrentMonitor>().add_systems(Update, track_monitor);
        let mk = |w: u32, h: u32, scale: f64| Monitor { name: None, physical_height: h, physical_width: w, physical_position: IVec2::ZERO, refresh_rate_millihertz: Some(60_000), scale_factor: scale, video_modes: vec![vm(w, h, 60, 32)] };
        let laptop = app.world_mut().spawn((mk(2880, 1800, 2.0), PrimaryMonitor)).id();
        let tvm = app.world_mut().spawn(mk(3840, 2160, 3.0)).id();
        let win = app.world_mut().spawn((Window::default(), PrimaryWindow, OnMonitor(laptop))).id();
        app.update();
        assert_eq!(app.world().resource::<CurrentMonitor>().0.as_ref().map(|m| m.size), Some(UVec2::new(2880, 1800)));
        // Windows re-inserts OnMonitor when the window is dragged: a change, not an addition.
        app.world_mut().entity_mut(win).insert(OnMonitor(tvm));
        app.update();
        let m = app.world().resource::<CurrentMonitor>().0.clone().unwrap();
        assert_eq!((m.size, m.scale), (UVec2::new(3840, 2160), 3.0), "now the TV");
        assert!(resolution_options(DisplayMode::Windowed, Some(&m)).contains(&Resolution::Fixed(3840, 2160)));
    }

    #[test]
    fn startup_mode_restores_a_saved_fullscreen() {
        let mut d = DisplaySettings { mode: DisplayMode::Borderless, ..Default::default() };
        assert_eq!(startup_mode(&d), WindowMode::BorderlessFullscreen(MonitorSelection::Primary));
        d.mode = DisplayMode::Exclusive;
        assert_eq!(startup_mode(&d), WindowMode::Fullscreen(MonitorSelection::Primary, VideoModeSelection::Current));
        d.resolution = Resolution::Fixed(1920, 1080);
        assert_eq!(startup_mode(&d), WindowMode::Windowed, "a chosen exclusive size is applied once the monitor's modes are known");
    }

    #[test]
    fn letterbox_matches_each_ratio() {
        let w = UVec2::new(3840, 2160);
        // 16:9 in a 16:9 window: no bars.
        assert_eq!(letterbox(w, Some((16, 9))), (UVec2::ZERO, w));
        // 21:9 on 16:9: bars above and below.
        assert_eq!(letterbox(w, Some((21, 9))), (UVec2::new(0, 257), UVec2::new(3840, 1646)));
        // 4:3 on 16:9: pillars left and right.
        assert_eq!(letterbox(w, Some((4, 3))), (UVec2::new(480, 0), UVec2::new(2880, 2160)));
        // 16:10 on 16:9: pillars.
        assert_eq!(letterbox(w, Some((16, 10))), (UVec2::new(192, 0), UVec2::new(3456, 2160)));
        // 16:9 on an ultrawide 3440x1440: pillars; 21:9 fills it (snap).
        let uw = UVec2::new(3440, 1440);
        assert_eq!(letterbox(uw, Some((16, 9))), (UVec2::new(440, 0), UVec2::new(2560, 1440)));
        assert_eq!(letterbox(uw, Some((21, 9))), (UVec2::ZERO, uw));
        // Auto and a bad ratio fill the window; a zero window is left alone.
        assert_eq!(letterbox(w, None), (UVec2::ZERO, w));
        assert_eq!(letterbox(w, Some((0, 9))), (UVec2::ZERO, w));
        assert_eq!(letterbox(UVec2::ZERO, Some((16, 9))), (UVec2::ZERO, UVec2::ZERO));
    }

    #[test]
    fn the_area_never_leaves_the_window_and_stays_centred() {
        for window in [UVec2::new(1600, 900), UVec2::new(1281, 721), UVec2::new(3840, 2160), UVec2::new(2560, 1080), UVec2::new(800, 600), UVec2::new(1000, 3000)] {
            for a in Aspect::ALL {
                let (pos, size) = letterbox(window, a.ratio());
                assert!(size.x >= 1 && size.y >= 1);
                assert!(pos.x + size.x <= window.x && pos.y + size.y <= window.y, "{a:?} in {window}");
                // Centred to within a pixel.
                assert!((window.x - size.x - 2 * pos.x) <= 1 && (window.y - size.y - 2 * pos.y) <= 1, "{a:?} in {window}");
                // The shape is the ratio's, or the window's (snapped / Auto).
                if let (Some((n, d)), true) = (a.ratio(), size != window) {
                    let r = size.x as f64 / size.y as f64;
                    assert!((r / (n as f64 / d as f64) - 1.0).abs() < 0.01, "{a:?} in {window}: {r}");
                }
            }
        }
    }

    #[test]
    fn logical_size_follows_the_scale_factor() {
        let a = GameArea::from_window(UVec2::new(3840, 2160), 1.5, Aspect::W4H3);
        assert_eq!(a.size, UVec2::new(2880, 2160));
        assert_eq!(a.logical(), Vec2::new(1920.0, 1440.0));
        assert!(a.ready() && a.bars());
        assert!(!GameArea::default().ready());
    }

    #[test]
    fn json_round_trips_and_old_files_load_as_defaults() {
        let d = DisplaySettings { mode: DisplayMode::Exclusive, resolution: Resolution::Fixed(3840, 2160), aspect: Aspect::W21H9, hud: HudSize::P150, screen: Screen::Number(2) };
        let mut v = serde_json::json!({ "split": "top_bottom" });
        d.write_json(&mut v);
        assert_eq!(v["split"], "top_bottom", "other keys survive");
        assert_eq!(DisplaySettings::from_json(&v), d);
        assert_eq!(DisplaySettings::from_json(&serde_json::json!({})), DisplaySettings::default());
        // Junk keeps the default per field.
        let junk = serde_json::json!({ "display_mode": "sideways", "resolution": "9x9", "aspect": "7:5", "hud_scale": 137, "screen": "0" });
        assert_eq!(DisplaySettings::from_json(&junk), DisplaySettings::default());
        for a in Aspect::ALL {
            assert_eq!(Aspect::from_key(a.key()), Some(a));
        }
    }

    /// The real file: save, load, an old file without the display keys, and what counts as a first run.
    #[test]
    fn settings_file_round_trip_keeps_old_settings_and_the_new_ones() {
        let path = std::env::temp_dir().join(format!("pd_settings_rt_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let fresh = Settings::load(path.clone());
        assert_eq!((fresh.display, fresh.display_chosen), (DisplaySettings::first_run(), false), "no file: a first run, borderless native auto");
        let mut s = Settings::load(path.clone());
        s.fov_deg = 70.0;
        s.display = DisplaySettings { mode: DisplayMode::Borderless, resolution: Resolution::Fixed(2560, 1440), aspect: Aspect::W16H10, hud: HudSize::P125, screen: Screen::Number(2) };
        s.display_chosen = true;
        s.save();
        let back = Settings::load(path.clone());
        assert_eq!((back.display, back.display_chosen), (s.display, true));
        assert_eq!(back.fov_deg, 70.0, "the old keys are still written");
        // A file from before the display keys: still a first run.
        std::fs::write(&path, r#"{"split":"top_bottom","fov_deg":60.0,"cam_drop":4.0}"#).unwrap();
        let old = Settings::load(path.clone());
        assert_eq!((old.fov_deg, old.display, old.display_chosen), (crate::menu::FOV_DEFAULT, DisplaySettings::first_run(), false), "the old fov_deg key means something else and is ignored");
        // An older build wrote every display key on any save, all at their defaults, without the player choosing: still a first run.
        std::fs::write(&path, r#"{"display_mode":"windowed","resolution":"native","aspect":"auto","hud_scale":100,"fov_deg":60.0}"#).unwrap();
        assert_eq!((Settings::load(path.clone()).display, Settings::load(path.clone()).display_chosen), (DisplaySettings::first_run(), false));
        // Any non-default display value was a choice; so is the marker.
        std::fs::write(&path, r#"{"display_mode":"exclusive","resolution":"native"}"#).unwrap();
        let chosen = Settings::load(path.clone());
        assert_eq!((chosen.display.mode, chosen.display_chosen), (DisplayMode::Exclusive, true));
        std::fs::write(&path, r#"{"display_mode":"windowed","display_chosen":true}"#).unwrap();
        let chosen = Settings::load(path.clone());
        assert_eq!((chosen.display, chosen.display_chosen), (DisplaySettings::default(), true), "the player picked windowed on purpose");
        // A capture run never writes the file.
        let mut quiet = Settings::load(path.clone());
        quiet.disable_saving();
        quiet.fov_deg = 99.0;
        quiet.save();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("99"));
        let _ = std::fs::remove_file(&path);
    }

    /// `apply_display` against a bare window entity (no OS involved): borderless
    /// on, then back to a window at its remembered size. Automated screenshot
    /// runs are left alone.
    #[test]
    fn apply_display_sets_the_window_and_remembers_its_size() {
        let mk = |screenshot: bool| {
            let mut app = App::new();
            let path = std::env::temp_dir().join(format!("pd_apply_test_{}_{screenshot}.json", std::process::id()));
            let mut settings = Settings::load(path);
            settings.display = DisplaySettings::default();
            app.insert_resource(settings)
                .insert_resource(CurrentMonitor(Some(tv())))
                .init_resource::<Screens>()
                .init_resource::<WindowedSize>()
                .insert_resource(crate::Config { stage: String::new(), chars: [String::new(), String::new()], demo: false, screenshot: screenshot.then(std::path::PathBuf::new), shot_frame: 0, cache: std::path::PathBuf::new(), legacy: false })
                .add_systems(Update, apply_display);
            app.world_mut().spawn((Window { resolution: (1600, 900).into(), ..default() }, PrimaryWindow));
            app
        };
        let mode = |app: &mut App| app.world_mut().query::<&Window>().single(app.world()).unwrap().mode;
        let size = |app: &mut App| app.world_mut().query::<&Window>().single(app.world()).unwrap().resolution.physical_size();

        let mut app = mk(false);
        app.update();
        assert_eq!((mode(&mut app), size(&mut app)), (WindowMode::Windowed, UVec2::new(1600, 900)), "defaults change nothing");
        app.world_mut().resource_mut::<Settings>().display.mode = DisplayMode::Borderless;
        app.update();
        assert_eq!(mode(&mut app), WindowMode::BorderlessFullscreen(MonitorSelection::Current));
        // Back to a window with a chosen size.
        {
            let mut s = app.world_mut().resource_mut::<Settings>();
            s.display.mode = DisplayMode::Windowed;
            s.display.resolution = Resolution::Fixed(1280, 720);
        }
        app.update();
        assert_eq!((mode(&mut app), size(&mut app)), (WindowMode::Windowed, UVec2::new(1280, 720)));
        // Windowed + native after fullscreen: the size before the fullscreen visit comes back.
        app.world_mut().resource_mut::<Settings>().display.mode = DisplayMode::Borderless;
        app.update();
        {
            let mut s = app.world_mut().resource_mut::<Settings>();
            s.display.mode = DisplayMode::Windowed;
            s.display.resolution = Resolution::Native;
        }
        app.update();
        assert_eq!((mode(&mut app), size(&mut app)), (WindowMode::Windowed, UVec2::new(1280, 720)));
        // An exclusive fixed size waits for the monitor list, then applies.
        {
            let mut s = app.world_mut().resource_mut::<Settings>();
            s.display.mode = DisplayMode::Exclusive;
            s.display.resolution = Resolution::Fixed(1920, 1080);
        }
        app.world_mut().resource_mut::<CurrentMonitor>().0 = None;
        app.update();
        assert_eq!(mode(&mut app), WindowMode::Windowed, "no monitor yet");
        app.world_mut().resource_mut::<CurrentMonitor>().0 = Some(tv());
        app.update();
        assert!(matches!(mode(&mut app), WindowMode::Fullscreen(_, VideoModeSelection::Specific(_))));

        // A screenshot run never changes the display.
        let mut shot = mk(true);
        shot.world_mut().resource_mut::<Settings>().display.mode = DisplayMode::Exclusive;
        shot.update();
        assert_eq!(mode(&mut shot), WindowMode::Windowed);
    }

    fn screen(e: u32, x: i32, size: (u32, u32), reach: (u32, u32), primary: bool) -> ScreenInfo {
        ScreenInfo { entity: Entity::from_raw_u32(e).unwrap(), pos: IVec2::new(x, 0), size: UVec2::new(size.0, size.1), reach: UVec2::new(reach.0, reach.1), primary }
    }

    /// Laurence's rig: the laptop panel (main, 2880x1800) and the 4K TV (extended to its right, left at 1080p on the desktop).
    fn rig() -> Screens {
        Screens(vec![screen(1, 0, (2880, 1800), (2880, 1800), true), screen(2, 2880, (1920, 1080), (3840, 2160), false)])
    }

    #[test]
    fn auto_picks_the_screen_that_can_show_the_most_even_if_it_is_not_main() {
        let r = rig();
        assert_eq!(Screen::Auto.resolve(&r), Some(r.0[1].entity), "the TV, though it is not the main screen and its desktop is only 1080p");
        assert_eq!(Screen::Number(1).resolve(&r), Some(r.0[0].entity));
        assert_eq!(Screen::Number(2).resolve(&r), Some(r.0[1].entity));
        assert_eq!(Screen::Number(3).resolve(&r), Some(r.0[1].entity), "a screen that is not connected falls back to Auto");
        assert_eq!(Screen::Auto.resolve(&Screens::default()), None);
        // Two the same: the main one.
        let twins = Screens(vec![screen(1, 0, (1920, 1080), (1920, 1080), false), screen(2, 1920, (1920, 1080), (1920, 1080), true)]);
        assert_eq!(Screen::Auto.resolve(&twins), Some(twins.0[1].entity));
    }

    #[test]
    fn the_screen_row_lists_auto_then_each_screen_with_its_size() {
        let r = rig();
        assert_eq!(screen_options(&r), vec![Screen::Auto, Screen::Number(1), Screen::Number(2)]);
        assert_eq!(screen_options(&Screens::default()), vec![Screen::Auto]);
        assert_eq!(Screen::Auto.label(&r), "AUTO (BIGGEST SCREEN)");
        assert_eq!(Screen::Number(2).label(&r), "SCREEN 2 (1920 X 1080)");
        assert_eq!(Screen::Number(3).label(&r), "SCREEN 3 (NOT FOUND)");
        assert_eq!(step(&screen_options(&r), Screen::Number(2), true), Screen::Auto, "wraps");
        assert_eq!((Screen::from_key("auto"), Screen::from_key("2"), Screen::from_key("0"), Screen::from_key("tv")), (Some(Screen::Auto), Some(Screen::Number(2)), None, None));
        assert_eq!(DisplaySettings::default().screen, Screen::Auto, "an old settings file opens on the biggest screen");
    }

    #[test]
    fn the_window_is_moved_once_then_left_alone() {
        let r = rig();
        let (laptop, tv) = (r.0[0].entity, r.0[1].entity);
        let mut st = ScreenMove::default();
        // Starts on the laptop, wants the TV: asks for the move once, then waits for the window to report it.
        assert_eq!(screen_step(&mut st, Screen::Auto, &r, Some(laptop)), ScreenStep::Move(tv));
        assert_eq!(screen_step(&mut st, Screen::Auto, &r, Some(laptop)), ScreenStep::Wait);
        assert_eq!(screen_step(&mut st, Screen::Auto, &r, Some(tv)), ScreenStep::Ready);
        // The player drags it back by hand: nothing fights that.
        assert_eq!(screen_step(&mut st, Screen::Auto, &r, Some(laptop)), ScreenStep::Ready);
        // Choosing another screen in Options moves it again.
        assert_eq!(screen_step(&mut st, Screen::Number(1), &r, Some(laptop)), ScreenStep::Ready, "already there");
        assert_eq!(screen_step(&mut st, Screen::Number(2), &r, Some(laptop)), ScreenStep::Move(tv));
        // A window that never reports arriving is given up on, not retried for ever.
        let mut st = ScreenMove::default();
        assert_eq!(screen_step(&mut st, Screen::Auto, &r, Some(laptop)), ScreenStep::Move(tv));
        let mut last = ScreenStep::Wait;
        for _ in 0..=MOVE_PATIENCE {
            last = screen_step(&mut st, Screen::Auto, &r, Some(laptop));
        }
        assert_eq!(last, ScreenStep::Ready);
        // No screens listed yet: carry on as before, and move once they appear.
        let mut st = ScreenMove::default();
        assert_eq!(screen_step(&mut st, Screen::Auto, &Screens::default(), None), ScreenStep::Ready);
        assert_eq!(screen_step(&mut st, Screen::Auto, &r, Some(laptop)), ScreenStep::Move(tv));
    }

    #[test]
    fn apply_display_moves_a_borderless_window_to_the_chosen_screen_first() {
        let path = std::env::temp_dir().join(format!("pd_screen_move_{}.json", std::process::id()));
        let mut app = App::new();
        let mut settings = Settings::load(path);
        settings.disable_saving();
        settings.display = DisplaySettings { mode: DisplayMode::Borderless, ..DisplaySettings::default() };
        let r = rig();
        let (laptop, tv_screen) = (r.0[0].entity, r.0[1].entity);
        app.insert_resource(settings)
            .insert_resource(CurrentMonitor(Some(tv())))
            .insert_resource(r)
            .init_resource::<WindowedSize>()
            .insert_resource(crate::Config { stage: String::new(), chars: [String::new(), String::new()], demo: false, screenshot: None, shot_frame: 0, cache: std::path::PathBuf::new(), legacy: false })
            .add_systems(Update, apply_display);
        let win = app.world_mut().spawn((Window { mode: WindowMode::BorderlessFullscreen(MonitorSelection::Primary), ..default() }, PrimaryWindow, OnMonitor(laptop))).id();
        app.update();
        assert_eq!(app.world().get::<Window>(win).unwrap().mode, WindowMode::BorderlessFullscreen(MonitorSelection::Entity(tv_screen)));
        // The OS reports the window arrived: the normal mode planning carries on.
        app.world_mut().entity_mut(win).insert(OnMonitor(tv_screen));
        app.update();
        assert_eq!(app.world().get::<Window>(win).unwrap().mode, WindowMode::BorderlessFullscreen(MonitorSelection::Current));
    }

    #[test]
    fn the_bars_camera_and_the_other_cameras_follow_the_area() {
        let mut app = App::new();
        let path = std::env::temp_dir().join(format!("pd_cams_test_{}.json", std::process::id()));
        app.insert_resource(Settings::load(path)).init_resource::<GameArea>().add_systems(Update, (update_area, fit_cameras).chain());
        app.world_mut().spawn((Window { resolution: (1600, 900).into(), ..default() }, PrimaryWindow));
        let bars = app.world_mut().spawn((BarsCamera, Camera { is_active: false, ..default() }, RenderTarget::default())).id();
        let hud = app.world_mut().spawn((Camera::default(), RenderTarget::default())).id();
        app.update();
        assert!(app.world().get::<Camera>(hud).unwrap().viewport.is_none(), "Auto: no camera is touched");
        assert!(!app.world().get::<Camera>(bars).unwrap().is_active);
        app.world_mut().resource_mut::<Settings>().display.aspect = Aspect::W4H3;
        app.update();
        let v = app.world().get::<Camera>(hud).unwrap().viewport.clone().expect("a viewport");
        assert_eq!((v.physical_position, v.physical_size), (UVec2::new(200, 0), UVec2::new(1200, 900)));
        assert!(app.world().get::<Camera>(bars).unwrap().is_active, "bars show");
        app.world_mut().resource_mut::<Settings>().display.aspect = Aspect::Auto;
        app.update();
        assert!(app.world().get::<Camera>(hud).unwrap().viewport.is_none(), "Auto again: back to the whole window");
        assert!(!app.world().get::<Camera>(bars).unwrap().is_active);
    }
}
