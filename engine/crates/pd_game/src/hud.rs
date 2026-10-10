//! The original battle HUD, rebuilt from the game's own HUD textures
//! (`cache/ui/hud0..2.png`, converted from `Menu/BtMenuEn.alr`) and laid out
//! after screenshots of the 2017 release (docs/HUD.md). One HUD per player
//! view; reads sim state only.
//!
//! Layout is written in a 1280x720 design space per player view and scaled
//! to the view (`layout_hud`), so it fits both split-screen layouts.

use crate::combat::{Capsule, SkillDef, AURA_PARTICLE, CHEST, COUNTDOWN_TICKS};
use crate::frontend::ui::{self, MenuArt, PixelText};
use crate::hud_card as hc;
use std::collections::HashMap;
use crate::{menu, render_pose, PlayerCamera, World};
use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use bevy::text::{FontSize, FontSource, FontWeight, Justify, LetterSpacing, TextLayoutInfo};

// Geometry that needs no entities (scales per layout, the other-player rows,
// health-bar ticks, the capture hint's span) lives in `hud_layout.rs`.
#[path = "hud_layout.rs"]
mod hud_layout;
use hud_layout as hl;
// The lock-on marker (sprites, intro, band colours) lives in `hud_marker.rs`.
#[path = "hud_marker.rs"]
pub mod hud_marker;

const ORANGE: Color = Color::srgb(0.96, 0.56, 0.26);
const ORANGE_DIM: Color = Color::srgb(0.55, 0.3, 0.14);
const TICK_OFF: Color = Color::srgb(0.22, 0.11, 0.05);
const LABEL: Color = Color::srgb(0.95, 0.85, 0.75);
/// How long "START" stays up after the countdown (ticks).
const START_SHOW_TICKS: u32 = 45;
/// Ticks spawned per health bar: one per Health point, up to the Heavy rule's 40
/// (`Fighter::max_health`); the ones past the fighter's maximum stay hidden.
const TICKS: usize = hl::MAX_TICKS;
/// Health bar geometry in design px (own panel, the target's big panel, a
/// compact row): bar length, tick width at the original 20-tick pitch, and the
/// widest a tick may get (KO Battle's 5 chunky ticks).
const OWN_BAR: (f32, f32, f32) = (140.0, 4.0, 10.0);
/// Names in the list of other players, in a quarter (design px; 17 elsewhere).
const QUAD_NAME_FONT: f32 = 20.0;
/// A teammate's colour in the list and over their head: the program's name
/// colour for a teammate, 0xff89bc18 (`FUN_140155310`; READ).
const ALLY: Color = Color::srgb(0x89 as f32 / 255.0, 0xbc as f32 / 255.0, 0x18 as f32 / 255.0);
/// The soft white glow shape (notched like the panel, 147x34) the original
/// tints orange behind the locked player's panel, stretched to the panel plus
/// 7 units each side (`FUN_140155310`: sprite at x+1, y-7).
const GLOW: Piece = (0, 0.0, 0.0, 147.0, 34.0);
/// The pill column in hud0: 7x4 each, 6 apart, from y=402 (red, blue, aqua,
/// magenta, yellow, green; `hud_layout::pill_row`).
const PILL: Piece = (0, 78.0, 402.0, 7.0, 4.0);
const PILL_STEP: f32 = 6.0;
/// The Aura Particle's pill: the program's pill table (0x1402f1348, kind 0 ->
/// sprite record 1) points at hud0 (46,504), 7x4 like the others (READ).
const AURA_PILL: Piece = (0, 46.0, 504.0, 7.0, 4.0);
/// The program draws other players' names in 0xff6f3c14 over a black shadow pass; without
/// the shadow the same colour is unreadable on bright scenery, so it is lifted a little.
const NAME_FOE: Color = Color::srgb(0.62, 0.36, 0.16);
/// Colour of the dots for Health just lost (program 0xffc80000).
const TRAIL_RED: Color = Color::srgb(0.784, 0.0, 0.0);

/// Pieces of the HUD atlases: (texture, x, y, w, h) in texture pixels.
/// Found with `pd_convert sprites Menu/BtMenuEn.alr <tex>`.
type Piece = (usize, f32, f32, f32, f32);
const PANEL_OWN: Piece = (0, 0.0, 36.0, 141.0, 25.0);
const PANEL_FOE: Piece = (0, 0.0, 78.0, 134.0, 36.0);
const PANEL_SMALL: Piece = (0, 0.0, 358.0, 75.0, 19.0);
const DIAL: Piece = (0, 0.0, 236.0, 96.0, 97.0);
const GAUGE: Piece = (0, 0.0, 380.0, 56.0, 56.0);
const BEZEL: Piece = (0, 101.0, 237.0, 31.0, 32.0);
const DISC: Piece = (0, 26.0, 492.0, 18.0, 18.0);
/// DIAL's and BEZEL's own hole (where GAUGE/DISC show through) isn't
/// centred in the piece rect above -- measured on `cache/ui/hud0.png` by
/// scanning for the alpha/luminance edge along the rect's own centre
/// row/column (bypassing the pieces' asymmetric drop shadow, which skews a
/// plain alpha bounding box): DIAL's black centre spans texture x14-77/
/// y15-77, i.e. its middle is 2.5 texture px above-left of the rect's own
/// centre; BEZEL's transparent hole spans x8-19/y9-20, 2.0/1.5 texture px
/// above-left of its rect centre. Converted to design px at each piece's own
/// draw scale (116/96, 117/97 for DIAL; `BEZEL_SCALE` for BEZEL, below) and
/// applied as a placement offset below, since GAUGE and DISC are themselves centred in
/// their own rects (verified the same way) -- moving the ring/bezel to meet
/// the fill, not the other way round.
/// Round 4 (2026-09-27, HUD dial lane 4): replaced by Laurence's own pick
/// from the A-J calibration sheet, `PD_DIAL_OFF=-1.5,-1.5` -- the measured
/// value above (-3.02,-3.02) overshot. `PD_DIAL_OFF` still overrides this at
/// runtime for further calibration.
const DIAL_HOLE_OFFSET: Vec2 = Vec2::new(-2.0, -2.0);
/// Knob bezel draw size and its hole offset, in design px (round 4 fix,
/// 2026-09-27, HUD dial lane 4): rounds 1-3 drew BEZEL at ~30x30 (scale
/// 30/31 ~= 0.97, close to the art's native texture size) while DISC was
/// drawn at 24x24 (scale 24/18 = 1.333) -- two different scale factors for
/// pieces from the *same knob art family*, unlike DIAL+GAUGE (drawn 116x117
/// / 68x68, scale ~1.21 for both, which preserves their native rect ratio
/// 56/96=0.583 almost exactly as drawn, 68/116=0.586 -- the one Laurence
/// picked as correct). BEZEL's native rect (31x32) to DISC's native rect
/// (18x18) is the same family ratio (18/31=0.581); drawing them at
/// mismatched scales (0.97 vs 1.333) is why the rings read as
/// disproportionately thin against their fill even after the hole-centring
/// fix in round 3 (which only fixed position, not size) -- matches
/// Laurence's call from the calibration sheet ("colour circles are
/// marginally off, but the rings are further off"). Fixed by giving BEZEL
/// the *same* scale as DISC (24/18 = 1.333) instead of its own ~0.97,
/// keeping DISC unchanged since it was judged only marginally off. Cross-
/// checked against measured footage (`Psycho Knife - Opponent POV.mp4`,
/// frame with a static idle dial, `.pdtools/footage/hud4/`): the Rail Gun
/// knob's disc/bezel outer-diameter ratio measured ~0.65-0.68 (radial scan
/// from the disc centre, clean angles only, avoiding the ring's connecting
/// spoke and neighbouring text) -- much closer to the native-ratio fix
/// (0.581) than the old drawn ratio (24/30 = 0.8), within the noise of a
/// compressed phone-recorded video at this zoom.
const BEZEL_SCALE: f32 = 24.0 / 18.0;
const BEZEL_W: f32 = 31.0 * BEZEL_SCALE;
const BEZEL_H: f32 = 32.0 * BEZEL_SCALE;
/// The rim (not the rect, which also holds a drop shadow down-right) is centred at
/// texture (11.5,11.5) of the 31x32 piece, hole 18 px = DISC's own 18 px (re-read 2026-09-27).
const BEZEL_HOLE_OFFSET: Vec2 = Vec2::new(-3.25 * BEZEL_SCALE, -4.5 * BEZEL_SCALE);
// (then 2 px up-left, 1 px back right/down, by Laurence's eye, 2026-09-27)
/// Calibration override for the Aâ€“J rim sheet: `PD_DIAL_OFF=x,y` /
/// `PD_BEZEL_OFF=x,y` replace the offsets above (design px).
fn hole_offset(var: &str, default: Vec2) -> Vec2 {
    std::env::var(var)
        .ok()
        .and_then(|v| {
            let mut it = v.split(',').map(|n| n.trim().parse::<f32>().ok());
            Some(Vec2::new(it.next()??, it.next()??))
        })
        .unwrap_or(default)
}
/// Where a teammate's marker floats: a little above the head (render only).
const ALLY_MARK_HEIGHT: f32 = CHEST * 2.1;
/// Set true to draw the teammate marker (blue orb + name above a teammate's
/// head) in split views too. The original draws no such label there.
const WORLD_LABEL_IN_SPLIT: bool = false;
/// `WORLD_LABEL_IN_SPLIT`, or `PD_TEST_WORLD_LABEL_SPLIT=1` (screenshots only).
fn world_label_in_split() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    WORLD_LABEL_IN_SPLIT || *ON.get_or_init(|| std::env::var_os("PD_TEST_WORLD_LABEL_SPLIT").is_some())
}
const START: Piece = (2, 538.0, 6.0, 183.0, 39.0);
const YOU_WIN: Piece = (2, 6.0, 163.0, 233.0, 38.0);
const YOU_LOSE: Piece = (2, 7.0, 111.0, 270.0, 38.0);
/// Next to YOU LOSE in the same atlas (piece box 288,107 166x45; same 5-px inset as the others).
const DRAW: Piece = (2, 293.0, 111.0, 156.0, 38.0);
const DIGITS: [Piece; 3] = [(2, 624.0, 61.0, 32.0, 53.0), (2, 542.0, 62.0, 61.0, 51.0), (2, 543.0, 134.0, 60.0, 52.0)];

/// Skill slot knobs around the dial: X left, B right, Y top, A bottom (the pad's face buttons).
/// Each knob sits on its own socket in the DIAL art, which are not symmetric:
/// socket centres read on `hud0.png` at texture (8.3,44.5) left, (83.7,45.2)
/// right, (47.5,8.3) top, (47.8,84) bottom, scaled by 142/96 x 143/97 from the
/// ring's drawn rect. Ring sized by overlaying = Laurence's full-screen frame
/// of the original (2026-09-27; split-screen views scale it up, `hud_layout::scales`).
const KNOBS: [Vec2; 4] = [Vec2::new(-56.9, -4.0), Vec2::new(54.6, -3.1), Vec2::new(1.1, -57.8), Vec2::new(1.5, 54.5)];
/// Top-left of the skill card, from the bottom-right corner of the view (design px).
const CARD_AT: Vec2 = Vec2::new(-640.0, -416.0);
/// Dial centre, from the bottom-right corner of the view.
const DIAL_AT: Vec2 = Vec2::new(-314.5, -154.0);
/// Room reserved between a knob's bezel and its name text for the "xN"
/// uses-left count (comfortably fits "x" + one digit at the name's own font
/// size, with a couple of px to spare on each side; uses counts run 0-9 in
/// the game's own data -- FORMATS-SKILLS.md s26).
const USES_GAP: f32 = 30.0;
/// Skill name text width, trimmed from the knob's full ribbon width to make
/// room for [`USES_GAP`] on the side nearest the knob.
const NAME_W: f32 = 135.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomRight,
    Center,
}

/// Where an element sits: `pos` is its top-left corner relative to the
/// anchor, in design pixels; `font` is a design font size for text.
#[derive(Component, Clone, Copy)]
pub struct Place {
    anchor: Anchor,
    pos: Vec2,
    size: Vec2,
    font: f32,
    /// Keep fractional pixel positions (health ticks, whose pitch is not a whole pixel).
    crisp: bool,
}

#[derive(Component, Clone, Copy, PartialEq)]
pub enum Bind {
    Static,
    OwnHealth,
    OwnTick(usize),
    /// One part of row `r` of the list of the other players (top left; the
    /// player's seat is the r-th other active slot). The lock target's row is
    /// the big panel, the others compact.
    Row(usize, RowPart),
    /// Over a teammate's head in your view (r-th other player): orb + name.
    AllyOrb(usize),
    AllyName(usize),
    /// "PAUSED / PLAYER 2 PAD DISCONNECTED" (a seat's pad is missing).
    PauseBack,
    PauseTitle,
    PauseLine,
    PauseHint,
    Arsenal,
    NextOrb,
    Aura,
    Level,
    /// One of the ten Aura recharge segments round the dial's centre gauge (`hud_dial.rs`).
    DialSeg(usize),
    Disc(usize),
    /// A knob's bezel: hidden on an empty slot so the dial's bare socket shows, as in the original.
    Bezel(usize),
    Cost(usize),
    /// Uses left on a limited skill (a small badge on the knob; none = unlimited).
    Uses(usize),
    Ribbon(usize),
    Name(usize),
    Countdown,
    Result,
    /// "Laser â€” press a skill button to capture" while at one of your capsules.
    Capture,
    /// Attribute changes and conditions next to a health panel ("When these
    /// change, an icon will be displayed next to your Health bar" â€” text
    /// stand-ins for the icons).
    OwnStatus,
    /// The environment skill in force, top centre.
    Environment,
    /// Battle time left, lower right ("As the time counts down, it will be
    /// displayed on the lower-right of the screen" â€” guide).
    Timer,
    /// One match clock for the whole screen, at the split (item 15; default with two or more views, `PD_HUD_SHARED_TIMER=0` turns it off).
    SharedTimer,
    /// The dark plate behind it (it sits across the split, over two views).
    SharedTimerBack,
    /// One part of the skill card shown while a D-pad direction is held
    /// (`hud_card.rs`): window, title bar, name, school, stats row, range
    /// icon, description lines.
    Card(CardPart),
}

/// The parts of the skill card (`hud_card.rs` has the layout and rules).
#[derive(Clone, Copy, PartialEq)]
pub enum CardPart {
    Window,
    Title,
    Name,
    School,
    CostLabel,
    Cost,
    MainLabel,
    MainValue,
    Shot,
    Uses,
    /// The infinity sign (unlimited uses).
    Infinity,
    /// The range icon (short / med / long / self / all / mine / capsule / auto / env).
    Range,
    /// The description's `k`-th line and its shadow.
    Note(usize),
    NoteShadow(usize),
}

/// The parts of a row in the list of the other players.
#[derive(Clone, Copy, PartialEq)]
pub enum RowPart {
    Name,
    /// The bullseye beside the player you are locked on.
    /// The orange glow behind the lock target's panel.
    Glow,
    Panel,
    Health,
    /// One of the 20 Health dots.
    Tick(usize),
    /// A pill for the skill on button `n` (coloured by capsule kind).
    Pill(usize),
    /// Arsenal left (bottom middle of the panel).
    Arsenal,
    Aura,
    Status,
}

/// Which slot each player is viewing with the D-pad (render-only input).
#[derive(Resource, Default)]
pub struct SkillInfoView(pub [Option<usize>; crate::combat::MAX_PLAYERS]);

/// School and the game's own note for each skill id (from skills.json).
#[derive(Resource, Default)]
pub struct SkillNotes(HashMap<i16, (String, String)>);

impl SkillNotes {
    pub fn load(path: &std::path::Path) -> Self {
        let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let mut map = HashMap::new();
        for s in v["skills"].as_array().into_iter().flatten() {
            let Some(id) = s["id"].as_i64() else { continue };
            let text = |k: &str| s[k].as_str().unwrap_or("").trim().to_string();
            map.entry(id as i16).or_insert_with(|| (text("school"), text("note")));
        }
        // The Aura Particle has no record in the skill table, only text 301 ("Increases Level by 1.").
        if let Some(t) = v["texts"].as_array().into_iter().flatten().find(|t| t["id"].as_i64() == Some(301)) {
            map.entry(AURA_PARTICLE).or_insert_with(|| (String::new(), t["note"].as_str().unwrap_or("").trim().to_string()));
        }
        SkillNotes(map)
    }

    /// The game's own description of skill `id` ("" when it has none).
    pub fn note(&self, id: i16) -> &str {
        self.0.get(&id).map_or("", |(_, n)| n.as_str())
    }
}

/// The infinity sign (unlimited uses): the original draws a glyph of its font
/// (16 x 12, `0x1402f8580`); this is a hand-made stand-in of the same size,
/// white so the tint is the whole colour.
fn infinity_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const ROWS: [&str; 12] = [
        "................",
        "................",
        "................",
        "..####....####..",
        ".##..##..##..##.",
        "##....####....##",
        "##....####....##",
        ".##..##..##..##.",
        "..####....####..",
        "................",
        "................",
        "................",
    ];
    let mut data = Vec::with_capacity(16 * 12 * 4);
    for row in ROWS {
        for c in row.chars() {
            data.extend_from_slice(&[255, 255, 255, if c == '#' { 255 } else { 0 }]);
        }
    }
    let mut img = Image::new(Extent3d { width: 16, height: 12, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    img.sampler = bevy::image::ImageSampler::nearest();
    img
}

const STATUS: Color = Color::srgb(0.55, 1.0, 0.55);
const ENV: Color = Color::srgb(0.45, 1.0, 0.95);

/// Short tags for player `i`'s changed attributes and conditions.
fn status_tags(c: &crate::combat::Combat, i: usize) -> String {
    use crate::moves::effects::Stat;
    let f = &c.fighters[i];
    let st = &f.status;
    let mut tags = Vec::new();
    for (stat, name, pct) in [(Stat::Attack, "ATK", false), (Stat::Speed, "SPD", true), (Stat::Homing, "HOMING", true), (Stat::AuraRate, "AURA", true)] {
        let v = c.stat(i, stat);
        if v != 0 {
            tags.push(format!("{name}{v:+}{}", if pct { "%" } else { "" }));
        }
    }
    for (on, name) in [
        (st.bound_ticks > 0, "BOUND"),
        (f.frozen_ticks > 0, "FROZEN"),
        (f.poison_ticks > 0, "POISON"),
        (st.invisible_ticks > 0, "CAMO"),
        (st.immune_ticks > 0, "VOID"),
        (c.inverted(i), "INVERTED"),
        (st.scrambled_ticks > 0, "SCRAMBLED"),
        (st.locked_ticks > 0, "BUTTON LOCKED"),
        (st.hover_ticks > 0, "FLOATING"),
        (st.regen_every > 0, "REGEN"),
        (st.still_immune, "INVULNERABLE"),
    ] {
        if on {
            tags.push(name.to_string());
        }
    }
    tags.join("  ")
}

#[derive(Component)]
pub struct Owner(usize);

#[derive(Component)]
pub struct HudRoot(usize);

/// The HUD's whole-screen layer (the game area, not one player's view): the pad-lost pause banner once for
/// everyone (Laurence, 2026-10-09: de-clutter, UI-DECLUTTER-PROPOSALS.md item 12) and the optional shared timer.
#[derive(Component)]
pub struct HudScreenRoot;

/// An item on [`HudScreenRoot`]: placed in the whole game area at the largest view's scale.
#[derive(Component)]
pub struct ScreenItem;

/// `PD_HUD_SLIM_FOES=1`: the other players' rows as slim strips except the lock target's (item 14's B; read once).
pub fn slim_foes() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("PD_HUD_SLIM_FOES").is_ok_and(|v| v == "1"))
}

/// One match clock at the split instead of one per view (item 15's B, Laurence's pick 2026-10-09; on by
/// default, `PD_HUD_SHARED_TIMER=0` gives the original's clock in every view; read once). Only used with two
/// or more views: a single full-window view keeps its own clock in the corner.
pub fn shared_timer() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("PD_HUD_SHARED_TIMER").map_or(true, |v| v != "0"))
}

/// Whether the shared clock replaces the per-view ones with `views` player views on screen.
pub fn use_shared_timer(views: usize) -> bool {
    views > 1 && shared_timer()
}

/// The UI camera the HUD (and the three-player map quarter, `views.rs`) draws on.
#[derive(Resource)]
pub struct HudCamera(pub Entity);

#[derive(Resource)]
pub struct HudAssets {
    tex: [Handle<Image>; 3],
    font: Handle<Font>,
    /// `WusMnEn.alr` textures 2, 3 and 7 (`hud_card::Tex::ALL`): the skill card's art.
    card: [Handle<Image>; 3],
    /// The infinity sign for unlimited uses (drawn here; the original uses a font glyph).
    infinity: Handle<Image>,
}

fn capsule_color(def: Option<&SkillDef>) -> Color {
    match def {
        Some(d) if d.id == AURA_PARTICLE => Color::srgb(0.85, 0.88, 0.95),
        Some(d) => match d.capsule {
            Capsule::Attack => Color::srgb(0.85, 0.3, 0.18),
            Capsule::Defence => Color::srgb(0.25, 0.38, 0.95),
            // Erase purple, status green, environment aqua: the program's own
            // capsule colour table (0x1402dca00, CAPSULE-VISUALS.md "Program read").
            Capsule::Erase => Color::srgb(0.65, 0.3, 0.9),
            Capsule::Status => Color::srgb(0.35, 0.8, 0.3),
            Capsule::Special => Color::srgb(0.9, 0.78, 0.25),
            Capsule::Environment => Color::srgb(0.3, 0.8, 0.85),
        },
        None => Color::srgb(0.12, 0.12, 0.12),
    }
}

fn rgb8(c: u32) -> Color {
    Color::srgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// The program's palette for the other players' panel (`FUN_140155310`, table
/// `0x1402f2a40`): the Health number and dots by tier (Health 1-20, 21-40, 41+),
/// brighter on the lock target; the arsenal / aura / level text; the panel tint.
/// A teammate is green (READ, same function: panel 0x89bc18 on the lock
/// target and 0x829f2a otherwise, tiers 0x89bc18 / 0x7fb20e / 0x75a804 lit and
/// 0x5a8102 / 0x466d02 / 0x325902 dim, text 0x89bc18 lit / 0x5a8102 dim).
pub(crate) struct FoePalette {
    pub tier: [Color; 3],
    pub text: Color,
    pub panel: Color,
    pub glow: Color,
    pub name: Color,
}

pub(crate) fn foe_palette(lit: bool, ally: bool) -> FoePalette {
    let p = if lit {
        FoePalette { tier: [rgb8(0xad3a03), rgb8(0xb06b05), rgb8(0xcc9835)], text: rgb8(0xad3a03), panel: rgb8(0xf57e27), glow: rgb8(0x824600), name: NAME_FOE }
    } else {
        FoePalette { tier: [rgb8(0x642604), rgb8(0x613b03), rgb8(0x8a6625)], text: rgb8(0x642604), panel: rgb8(0xe98646), glow: rgb8(0x824600), name: NAME_FOE }
    };
    if ally {
        if lit {
            FoePalette { tier: [rgb8(0x89bc18), rgb8(0x7fb20e), rgb8(0x75a804)], text: rgb8(0x89bc18), panel: rgb8(0x89bc18), glow: rgb8(0x824600), name: ALLY }
        } else {
            FoePalette { tier: [rgb8(0x5a8102), rgb8(0x466d02), rgb8(0x325902)], text: rgb8(0x5a8102), panel: rgb8(0x829f2a), glow: rgb8(0x824600), name: ALLY }
        }
    } else {
        p
    }
}

/// The skill table's capsule number (s9) of a skill, 0 for the Aura Particle.
fn capsule_number(def: &SkillDef) -> u8 {
    if def.id == AURA_PARTICLE {
        return 0;
    }
    match def.capsule {
        Capsule::Attack => 1,
        Capsule::Defence => 2,
        Capsule::Erase => 3,
        Capsule::Status => 4,
        Capsule::Special => 5,
        Capsule::Environment => 6,
    }
}

/// A colour darkened to `k` of itself (a player who is down).
fn shade(c: Color, k: f32) -> Color {
    let s = c.to_srgba();
    Color::srgba(s.red * k, s.green * k, s.blue * k, s.alpha)
}

/// The atlas the skill pills are cut from.
#[derive(Component, Clone)]
pub struct PillArt {
    atlas: Handle<Image>,
}

/// Render-only trail of each player's Health for the red "just lost" dots
/// (`hud_layout::lag_step`, stepped at the program's 30 Hz).
#[derive(Default)]
pub struct HealthLag {
    lag: [f32; crate::combat::MAX_PLAYERS],
    hold: [u32; crate::combat::MAX_PLAYERS],
    seen: [bool; crate::combat::MAX_PLAYERS],
    frame: i64,
}

impl HealthLag {
    /// Advance to sim time `now` (seconds) with each player's Health.
    fn advance(&mut self, now: f64, health: &[i32]) {
        let frame = (now * 30.0).floor() as i64;
        let steps = (frame - self.frame).clamp(0, 30);
        self.frame = frame;
        for (j, &h) in health.iter().enumerate().take(crate::combat::MAX_PLAYERS) {
            if !self.seen[j] {
                self.seen[j] = true;
                self.lag[j] = h.max(0) as f32;
                self.hold[j] = 0;
                continue;
            }
            for _ in 0..steps {
                (self.lag[j], self.hold[j]) = hl::lag_step(self.lag[j], self.hold[j], h);
            }
        }
    }
}

fn rect(p: Piece) -> Rect {
    Rect::new(p.1, p.2, p.1 + p.3, p.2 + p.4)
}

pub fn spawn_hud(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<crate::Config>, art: Option<Res<MenuArt>>, mut images: ResMut<Assets<Image>>) {
    // Names, skill names and the timer use the menu's own pixel font
    // (docs/HUD.md); everything else keeps the Bahnschrift stand-in. Loads
    // the same shared `MenuArt` the menu screens use (harmless if a screen
    // already loaded it; `--demo`/legacy runs that skip the menus entirely
    // load it here for the first time).
    let art = MenuArt::get(&mut commands, art, &cfg.cache, &assets, &mut images);
    let font = if cfg.cache.join("ui/hud.ttf").exists() { assets.load("ui/hud.ttf") } else { Handle::default() };
    let hud = HudAssets {
        tex: [0, 1, 2].map(|i| assets.load(format!("ui/hud{i}.png"))),
        font,
        card: hc::Tex::ALL.map(|t| assets.load(t.file())),
        infinity: images.add(infinity_image()),
    };
    // UI overlay above both player views (the settings menu draws above this).
    let cam = commands.spawn((Camera2d, Camera { order: 5, clear_color: ClearColorConfig::None, ..default() })).id();
    commands.insert_resource(HudCamera(cam));
    // One HUD per possible view; `layout_hud` hides the ones with no player.
    for i in 0..crate::combat::MAX_PLAYERS {
        let root = commands.spawn((HudRoot(i), UiTargetCamera(cam), Node { position_type: PositionType::Absolute, overflow: Overflow::clip(), ..default() })).id();
        hud_marker::spawn(&mut commands, root, i, &hud.tex[1], &art);
        let mut add = |bundle: (Place, Bind), extra: &mut dyn FnMut(&mut EntityCommands)| {
            let mut e = commands.spawn((bundle.0, bundle.1, Owner(i), Node { position_type: PositionType::Absolute, ..default() }));
            extra(&mut e);
            let id = e.id();
            commands.entity(root).add_child(id);
        };
        let at = |anchor, x: f32, y: f32, w: f32, h: f32| Place { anchor, pos: Vec2::new(x, y), size: Vec2::new(w, h), font: 0.0, crisp: false };
        let tick_at = |x: f32, y: f32, w: f32, h: f32| Place { anchor: TopLeft, pos: Vec2::new(x, y), size: Vec2::new(w, h), font: 0.0, crisp: true };
        let text_at = |anchor, x: f32, y: f32, w: f32, h: f32, font: f32| Place { anchor, pos: Vec2::new(x, y), size: Vec2::new(w, h), font, crisp: false };
        let sprite = |p: Piece, tint: Color| {
            let img = ImageNode { image: hud.tex[p.0].clone(), rect: Some(rect(p)), color: tint, ..default() };
            move |e: &mut EntityCommands| {
                e.insert(img.clone());
            }
        };
        let font = hud.font.clone();
        let font_digits = font.clone();
        let text = move |s: &str, color: Color, justify: Justify| {
            let (s, font) = (s.to_string(), font.clone());
            move |e: &mut EntityCommands| {
                e.insert((
                    Text::new(s.clone()),
                    TextFont { font: FontSource::Handle(font.clone()), font_size: FontSize::Px(16.0), weight: FontWeight(700), ..default() },
                    TextColor(color),
                    TextLayout { justify, ..default() },
                    TextShadow { offset: Vec2::splat(1.5), color: Color::srgba(0.0, 0.0, 0.0, 0.85) },
                ));
            }
        };
        // The original's digits are wide and square; Bahnschrift's are narrow, so
        // the panel's numbers are spaced out a little to span the same width.
        let digits = move |s: &str, color: Color| {
            let (s, font) = (s.to_string(), font_digits.clone());
            move |e: &mut EntityCommands| {
                e.insert((
                    Text::new(s.clone()),
                    TextFont { font: FontSource::Handle(font.clone()), font_size: FontSize::Px(16.0), weight: FontWeight(700), ..default() },
                    TextColor(color),
                    LetterSpacing::Rem(0.07),
                    TextLayout { justify: Justify::Left, ..default() },
                    TextShadow { offset: Vec2::splat(1.5), color: Color::srgba(0.0, 0.0, 0.0, 0.85) },
                ));
            }
        };
        // Names, skill names and the timer: the menu's own pixel font
        // instead of the Bahnschrift stand-in (docs/HUD.md). `update_hud`
        // rescales/retexts these through `ui::set_pixel` (`place.font` here
        // is only the *initial* size baked at spawn).
        let art_for_pixel = art.clone();
        let pixel = move |s: &str, color: Color, font_px: f32| {
            let (s, art) = (s.to_string(), art_for_pixel.clone());
            move |e: &mut EntityCommands| {
                ui::glyphs_on(e, &art, &s, color, ui::pixel_scale(font_px));
            }
        };
        use Anchor::*;
        let me = crate::seats::seat_name(i);
        // Placeholder until the first `update_hud`: the target's name follows the lock target.
        let foe = crate::seats::seat_name(if i == 0 { 1 } else { 0 });

        // Your panel (top left): name, health number, one tick per health point.
        add((text_at(TopLeft, 74.0, 58.0, 220.0, 24.0, 20.0), Bind::Static), &mut pixel(&me, ORANGE, 20.0));
        add((at(TopLeft, 66.0, 82.0, 224.0, 40.0), Bind::Static), &mut sprite(PANEL_OWN, Color::srgb(1.0, 0.75, 0.55)));
        add((text_at(TopLeft, 72.0, 84.0, 56.0, 36.0, 32.0), Bind::OwnHealth), &mut text("20", ORANGE, Justify::Center));
        for k in 0..TICKS {
            add((tick_at(134.0 + k as f32 * 7.0, 91.0, 4.0, 22.0), Bind::OwnTick(k)), &mut |e| {
                e.insert(BackgroundColor(ORANGE));
            });
        }
        // The other players, one row each (`update_hud` places and fills them):
        // the original's panel for every row, the lock target's lit with a glow:
        // name, Health number and 20 dots, a pill per skill bound on their
        // buttons (a diamond, coloured by capsule kind), arsenal left and
        // aura / level. A teammate is blue.
        for r in 0..crate::combat::MAX_PLAYERS - 1 {
            use RowPart::*;
            let row = |part: RowPart| Bind::Row(r, part);
            add((text_at(TopLeft, 0.0, 0.0, 220.0, 22.0, 17.0), row(Name)), &mut pixel(&foe, ORANGE, 17.0));
            add((at(TopLeft, 0.0, 0.0, 148.0, 50.0), row(Glow)), &mut sprite(GLOW, Color::WHITE));
            add((at(TopLeft, 0.0, 0.0, 134.0, 36.0), row(Panel)), &mut sprite(PANEL_FOE, Color::WHITE));
            add((text_at(TopLeft, 0.0, 0.0, 44.0, 24.0, 21.0), row(Health)), &mut digits("20", ORANGE));
            for k in 0..hl::DOTS {
                add((tick_at(0.0, 0.0, 3.0, 4.0), row(Tick(k))), &mut |e| {
                    e.insert(BackgroundColor(ORANGE));
                });
            }
            for k in 0..crate::combat::SLOTS {
                let art = PillArt { atlas: hud.tex[PILL.0].clone() };
                add((at(TopLeft, 0.0, 0.0, 7.0, 4.0), row(Pill(k))), &mut move |e| {
                    e.insert((art.clone(), ImageNode { image: art.atlas.clone(), rect: Some(rect(PILL)), ..default() }));
                });
            }
            add((text_at(TopLeft, 0.0, 0.0, 40.0, 24.0, 21.0), row(Arsenal)), &mut digits("30", ORANGE_DIM));
            add((text_at(TopLeft, 0.0, 0.0, 112.0, 24.0, 21.0), row(Aura)), &mut digits("01/01", ORANGE));
            add((text_at(TopLeft, 0.0, 0.0, 420.0, 22.0, 15.0), row(Status)), &mut text("", STATUS, Justify::Left));
            // A teammate's marker over their head, in your view.
            add((at(TopLeft, 0.0, 0.0, 20.0, 20.0), Bind::AllyOrb(r)), &mut sprite(DISC, ALLY));
            add((text_at(TopLeft, 0.0, 0.0, 160.0, 20.0, 17.0), Bind::AllyName(r)), &mut pixel("", ALLY, 17.0));
        }
        // Status tags beside your panel, and the environment in force.
        add((text_at(TopLeft, 298.0, 92.0, 420.0, 24.0, 17.0), Bind::OwnStatus), &mut text("", STATUS, Justify::Left));
        add((text_at(Center, -300.0, -300.0, 600.0, 26.0, 19.0), Bind::Environment), &mut text("", ENV, Justify::Center));
        add((text_at(BottomRight, DIAL_AT.x - 60.0, DIAL_AT.y - 104.0, 120.0, 26.0, 22.0), Bind::Timer), &mut pixel("15:00", ORANGE, 22.0));

        // Arsenal left + the colour of the next capsule (top right).
        add((at(TopRight, -206.0, 84.0, 136.0, 34.0), Bind::Static), &mut sprite(PANEL_SMALL, Color::WHITE));
        add((text_at(TopRight, -200.0, 87.0, 60.0, 28.0, 22.0), Bind::Arsenal), &mut text("30", ORANGE, Justify::Center));
        add((at(TopRight, -118.0, 89.0, 24.0, 24.0), Bind::NextOrb), &mut sprite(DISC, Color::WHITE));

        // Skill dial (bottom right): aura over level in the middle, a knob per slot.
        for (k, off) in KNOBS.iter().enumerate() {
            let c = DIAL_AT + *off;
            let left = off.x < 0.0;
            let rx = if left { c.x - 184.0 } else { c.x + 10.0 };
            add((at(BottomRight, rx, c.y - 11.0, 174.0, 22.0), Bind::Ribbon(k)), &mut |e| {
                e.insert(BackgroundGradient::default());
            });
            // Left knob's name sits at the ribbon's outer edge, ending
            // `USES_GAP` short of the bezel; the other three sit
            // `USES_GAP` out from the bezel, both leaving the same-size gap
            // for the uses count next to the knob (`Bind::Uses`, below).
            let tx = if left { rx + 4.0 } else { c.x + 15.0 + USES_GAP };
            add((text_at(BottomRight, tx, c.y - 10.0, NAME_W, 22.0, 16.0), Bind::Name(k)), &mut pixel("", LABEL, 16.0));
        }
        // Placed `DIAL_HOLE_OFFSET` past the naive centred position so the
        // ring's own hole (not its rect) lands on GAUGE, which is centred.
        let dial_pos = DIAL_AT - Vec2::new(71.0, 71.5) - hole_offset("PD_DIAL_OFF", DIAL_HOLE_OFFSET);
        add((at(BottomRight, dial_pos.x, dial_pos.y, 142.0, 143.0), Bind::Static), &mut sprite(DIAL, Color::WHITE));
        // The gauge ring is tinted 0xff6e2500 (program `FUN_140151f90`); the Aura
        // recharge segments (`hud_dial.rs`) light over it, clockwise.
        add((at(BottomRight, DIAL_AT.x - 41.5, DIAL_AT.y - 41.5, 83.0, 83.0), Bind::Static), &mut sprite(GAUGE, Color::srgb_u8(0x6e, 0x25, 0x00)));
        for k in 0..crate::hud_dial::SEGMENTS {
            let (x, y, w, h) = crate::hud_dial::rect(k, (DIAL_AT.x - 41.5, DIAL_AT.y - 41.5), 83.0 / crate::hud_dial::GAUGE_SIZE);
            let a = crate::hud_dial::TABLE[k].atlas;
            let lit = Color::srgb_u8(crate::hud_dial::LIT_RGB[0], crate::hud_dial::LIT_RGB[1], crate::hud_dial::LIT_RGB[2]);
            add((at(BottomRight, x, y, w, h), Bind::DialSeg(k)), &mut sprite((0, a.0, a.1, a.2, a.3), lit));
        }
        add((text_at(BottomRight, DIAL_AT.x - 37.0, DIAL_AT.y - 25.0, 74.0, 33.0, 27.0), Bind::Aura), &mut text("01", ORANGE, Justify::Center));
        add((text_at(BottomRight, DIAL_AT.x - 37.0, DIAL_AT.y + 4.0, 74.0, 24.0, 19.0), Bind::Level), &mut text("01", ORANGE_DIM, Justify::Center));
        for (k, off) in KNOBS.iter().enumerate() {
            let c = DIAL_AT + *off;
            let left = off.x < 0.0;
            add((at(BottomRight, c.x - 12.0, c.y - 12.0, 24.0, 24.0), Bind::Disc(k)), &mut sprite(DISC, Color::WHITE));
            // Placed `BEZEL_HOLE_OFFSET` past the naive centred position so
            // the bezel's own hole (not its rect) lands on the disc above.
            let bezel_pos = c - Vec2::new(BEZEL_W / 2.0, BEZEL_H / 2.0) - hole_offset("PD_BEZEL_OFF", BEZEL_HOLE_OFFSET);
            add((at(BottomRight, bezel_pos.x, bezel_pos.y, BEZEL_W, BEZEL_H), Bind::Bezel(k)), &mut sprite(BEZEL, Color::WHITE));
            // The cost digit is re-centred on the disc every frame in
            // `update_hud` from its own measured glyph height (Bevy UI text
            // draws from the top of its box, not centred in it, so a fixed
            // box position over- or under-shoots depending on the font's
            // metrics) -- this box is just a sane starting point before the
            // first measurement lands.
            add((text_at(BottomRight, c.x - 12.0, c.y - 11.0, 24.0, 22.0, 16.0), Bind::Cost(k)), &mut text("", Color::WHITE, Justify::Center));
            // Uses left ("xN"): same row and font as the skill name, in the
            // name's own colour for contrast against the ribbon (an
            // Aura-orange digit read poorly against the yellow ribbons).
            // Positioned and sized every frame in `update_hud` from the
            // string's own measured width, right against the knob's bezel
            // on whichever side is clear of the ribbon.
            let bx = if left { c.x - 32.0 } else { c.x + 15.0 };
            add((text_at(BottomRight, bx, c.y - 10.0, 24.0, 22.0, 16.0), Bind::Uses(k)), &mut pixel("", LABEL, 16.0));
        }

        // Skill card (D-pad), above the dial: the original's own window, built
        // from its sprites (`hud_card.rs`). Placement is ours (UNCONFIRMED).
        {
            use CardPart::*;
            let (cs, ox, oy) = (hc::S, CARD_AT.x, CARD_AT.y);
            let rgb = |c: [u8; 3]| Color::srgb_u8(c[0], c[1], c[2]);
            let card_tex = |t: hc::Tex| hc::Tex::ALL.iter().position(|&x| x == t).unwrap_or(0);
            let mut card_sprite = |part: CardPart, s: hc::Sprite, at_px: (f32, f32), tint: [u8; 3]| {
                let node = ImageNode { image: hud.card[card_tex(s.tex)].clone(), rect: Some(Rect::new(s.x, s.y, s.x + s.w, s.y + s.h)), color: rgb(tint), ..default() };
                add((at(BottomRight, ox + at_px.0 * cs, oy + at_px.1 * cs, s.w * cs, s.h * cs), Bind::Card(part)), &mut |e| {
                    e.insert((node.clone(), Visibility::Hidden));
                });
            };
            card_sprite(Window, hc::WINDOW, (0.0, 0.0), [255, 255, 255]);
            card_sprite(Title, hc::TITLE, hc::TITLE_AT, [255, 255, 255]);
            card_sprite(School, hc::school_sprite(1), hc::SCHOOL_AT, hc::NAME_RGB);
            card_sprite(CostLabel, hc::COST_LABEL, (hc::COST_LABEL_X, hc::ROW_Y), hc::LABEL_RGB);
            card_sprite(MainLabel, hc::BLANK_LABEL, (hc::MAIN_LABEL_X, hc::ROW_Y), hc::LABEL_RGB);
            card_sprite(Shot, hc::SHOT_BOX, (hc::SHOT_X, hc::ROW_Y), hc::LABEL_RGB);
            card_sprite(Range, hc::RangeIcon::Short.sprite(), (hc::RANGE_X, hc::ROW_Y), hc::LABEL_RGB);
            let inf = ImageNode { image: hud.infinity.clone(), color: rgb(hc::LABEL_RGB), ..default() };
            add((at(BottomRight, ox + hc::USES_X * cs, oy + hc::ROW_Y * cs, 16.0 * cs, 12.0 * cs), Bind::Card(Infinity)), &mut |e| {
                e.insert((inf.clone(), Visibility::Hidden));
            });
            let name_px = 16.0 * cs;
            let mut card_text = |part: CardPart, at_px: (f32, f32), w: f32, font: f32, tint: [u8; 3]| {
                add((text_at(BottomRight, ox + at_px.0 * cs, oy + at_px.1 * cs, w * cs, font, font), Bind::Card(part)), &mut |e| {
                    pixel("", rgb(tint), font)(e);
                    e.insert(Visibility::Hidden);
                });
            };
            card_text(Name, hc::NAME_AT, 170.0, name_px, hc::NAME_RGB);
            card_text(Cost, (hc::COST_X, hc::VALUE_Y), 28.0, name_px, hc::LABEL_RGB);
            card_text(MainValue, (hc::MAIN_X, hc::VALUE_Y), 28.0, name_px, hc::LABEL_RGB);
            card_text(Uses, (hc::USES_X, hc::VALUE_Y), 28.0, name_px, hc::LABEL_RGB);
            for k in 0..hc::NOTE_LINES {
                let y = hc::NOTE_AT.1 + hc::NOTE_LINE * k as f32;
                let note_px = 14.0 * cs;
                card_text(NoteShadow(k), (hc::NOTE_AT.0 + hc::NOTE_SHADOW.0, y + hc::NOTE_SHADOW.1), hc::NOTE_WRAP, note_px, hc::NOTE_SHADOW_RGB);
                card_text(Note(k), (hc::NOTE_AT.0, y), hc::NOTE_WRAP, note_px, hc::NOTE_RGB);
            }
        }

        // Capture prompt (the original shows the skill's panel at a capsule);
        // `update_hud` keeps it clear of the dial's ribbons in a short view.
        add((text_at(Center, -320.0, 150.0, 640.0, 30.0, 22.0), Bind::Capture), &mut text("", LABEL, Justify::Center));

        // Countdown and round result banners.
        add((at(Center, 0.0, 0.0, 0.0, 0.0), Bind::Countdown), &mut sprite(START, Color::WHITE));
        add((at(Center, 0.0, 0.0, 0.0, 0.0), Bind::Result), &mut sprite(YOU_WIN, Color::WHITE));
    }
    // The whole-screen layer: "Paused: a pad is missing" once over the middle of the screen (it was drawn in every
    // view), and the shared timer when that switch is on.
    let screen = commands.spawn((HudScreenRoot, UiTargetCamera(cam), Node { position_type: PositionType::Absolute, overflow: Overflow::clip(), ..default() })).id();
    let place = |x: f32, y: f32, w: f32, h: f32, font: f32| Place { anchor: Anchor::Center, pos: Vec2::new(x, y), size: Vec2::new(w, h), font, crisp: false };
    let pixel_item = |commands: &mut Commands, p: Place, bind: Bind, s: &str, color: Color, font: f32| {
        let mut e = commands.spawn((p, bind, Owner(0), ScreenItem, Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden));
        ui::glyphs_on(&mut e, &art, s, color, ui::pixel_scale(font));
        e.id()
    };
    let back = commands.spawn((place(-290.0, -80.0, 580.0, 128.0, 0.0), Bind::PauseBack, Owner(0), ScreenItem, Node { position_type: PositionType::Absolute, ..default() }, BackgroundColor(Color::srgba(0.04, 0.02, 0.01, 0.88)), Visibility::Hidden)).id();
    let title = pixel_item(&mut commands, place(-100.0, -66.0, 200.0, 44.0, 40.0), Bind::PauseTitle, "PAUSED", ORANGE, 40.0);
    let line = pixel_item(&mut commands, place(-270.0, -8.0, 540.0, 24.0, 20.0), Bind::PauseLine, "", LABEL, 20.0);
    let hint = pixel_item(&mut commands, place(-270.0, 24.0, 540.0, 20.0, 16.0), Bind::PauseHint, "", LABEL, 16.0);
    let plate = commands.spawn((place(-62.0, -22.0, 124.0, 44.0, 0.0), Bind::SharedTimerBack, Owner(0), ScreenItem, Node { position_type: PositionType::Absolute, ..default() }, BackgroundColor(Color::srgba(0.04, 0.02, 0.01, 0.85)), Visibility::Hidden)).id();
    let timer = pixel_item(&mut commands, place(-60.0, -15.0, 120.0, 30.0, 30.0), Bind::SharedTimer, "15:00", ORANGE, 30.0);
    commands.entity(screen).add_children(&[back, title, line, hint, plate, timer]);
    commands.insert_resource(hud);
}

/// Fill the HUD from the sim state (text, colours, what's shown).
#[allow(clippy::too_many_arguments)]
pub fn update_hud(
    mut commands: Commands,
    art: Option<Res<MenuArt>>,
    mut world: ResMut<World>,
    seats: Res<crate::seats::Seats>,
    info: Res<SkillInfoView>,
    notes: Res<SkillNotes>,
    fixed: Res<Time<Fixed>>,
    scale: Res<HudScale>,
    local: Res<crate::seats::LocalViews>,
    mut lagging: Local<HealthLag>,
    mut hooks_done: Local<bool>,
    cams: Query<(&PlayerCamera, &Camera, &GlobalTransform)>,
    mut q: Query<(Entity, &Owner, &Bind, &mut Place, &mut Visibility, Option<&mut Text>, Option<&mut PixelText>, Option<&mut ImageNode>, Option<&mut BackgroundColor>, Option<&mut BackgroundGradient>, Option<&mut TextColor>, Option<&TextLayoutInfo>, Option<&PillArt>)>,
) {
    // Item 15: one clock at the split once two or more player views are on screen.
    let shared = use_shared_timer(cams.iter().filter(|(_, cam, _)| cam.is_active).count());
    // Test hook (screenshots only): `PD_TEST_RULE=<0-11>` starts the match
    // under that rule once (4 = Heavy: 40 Health), as the setup screen would.
    if !*hooks_done {
        *hooks_done = true;
        // Test hook (screenshots only): `PD_TEST_AURA=<player 1-4>,<aura>,<level>` sets
        // that fighter's Aura (points, may be fractional) and Level once at the start, so
        // the recharge gauge can be photographed across a refill (`hud_dial.rs`).
        if let Some((slot, aura, level)) = std::env::var("PD_TEST_AURA").ok().and_then(|v| {
            let mut it = v.split(',').map(|n| n.trim().parse::<f32>().ok());
            Some((it.next()??, it.next()??, it.next()??))
        }) {
            if (1.0..=crate::combat::MAX_PLAYERS as f32).contains(&slot) {
                let f = &mut world.0.combat.fighters[(slot - 1.0) as usize];
                f.level = level as i32;
                f.aura_ticks = (aura * crate::combat::AURA_TICKS_PER_POINT as f32) as i32;
            }
        }
        if let Some(rule) = std::env::var("PD_TEST_RULE").ok().and_then(|v| v.parse::<usize>().ok()) {
            world.0.combat.set_rules(crate::combat::Rules { set: crate::combat::RuleSet::new(rule), ..crate::combat::Rules::default() });
        }
    }
    // Test hook (screenshots only): `PD_TEST_HEALTH=<player 1-4>,<health>` pins that
    // fighter's Health (to show a Health the demo does not reach, e.g. 37 under rule 4).
    if let Some((slot, hp)) = crate::effects::env_once("PD_TEST_HEALTH").and_then(|v| {
        let mut it = v.split(',').map(|n| n.trim().parse::<i32>().ok());
        Some((it.next()??, it.next()??))
    }) {
        if (1..=crate::combat::MAX_PLAYERS as i32).contains(&slot) {
            world.0.combat.fighters[(slot - 1) as usize].health = hp;
        }
    }
    let c = &world.0.combat;
    let players_n = c.roster.count();
    // Pads that went missing (seats.rs); `PD_TEST_PAD_LOST=2,3` (seat numbers,
    // from 1) fakes it for captures without pausing the sim.
    let lost: Vec<usize> = match crate::effects::env_once("PD_TEST_PAD_LOST") {
        Some(v) => v.split(',').filter_map(|n| n.trim().parse::<usize>().ok()).filter(|&n| n >= 1).map(|n| n - 1).collect(),
        None => (0..players_n).filter(|&k| seats.lost[k]).collect(),
    };
    let paused = !lost.is_empty();
    let pause_line = hl::lost_line(&lost);
    // The other players per view and who is the lock target (drawn big).
    let active: Vec<bool> = c.roster.active.to_vec();
    let rows: Vec<Vec<hl::Row>> = (0..crate::combat::MAX_PLAYERS)
        .map(|i| {
            if i >= players_n {
                return Vec::new();
            }
            let others = hl::others_of(&active, i);
            let target = (!others.is_empty()).then(|| c.aim_at(i, world.0.players[i].target));
            if slim_foes() { hl::rows_slim(&others, target) } else { hl::rows(&others, target) }
        })
        .collect();
    // The skill card each player is looking at (a held D-pad direction), if any.
    let cards: Vec<Option<(hc::CardView, Vec<String>)>> = (0..crate::combat::MAX_PLAYERS)
        .map(|i| {
            if i >= players_n {
                return None;
            }
            let id = c.fighters[i].slots[info.0[i]?]?;
            let def = c.skill(id)?;
            let note = notes.note(id);
            let v = if id == AURA_PARTICLE { hc::CardView::aura_particle(&def.name, note) } else { hc::CardView::of(def, note) };
            let width = |s: &str| art.as_ref().map_or(s.len() as f32 * 8.0 * hc::NOTE_SCALE, |a| a.text_width(s, hc::NOTE_SCALE));
            let lines = hc::wrap(&v.note, hc::NOTE_WRAP, &width);
            Some((v, lines))
        })
        .collect();
    let show = |v: &mut Visibility, on: bool| {
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    };
    let set_text = |t: Option<Mut<Text>>, s: String| {
        if let Some(mut t) = t {
            if t.0 != s {
                t.0 = s;
            }
        }
    };
    // Names/skill names/timer (pixel font): reuse the field's current
    // colour and scale, only content changes here (`layout_hud` owns
    // rescaling when the split layout changes).
    let set_pixel_text = |commands: &mut Commands, entity: Entity, pt: Option<Mut<PixelText>>, s: String| {
        let (Some(art), Some(mut pt)) = (&art, pt) else { return };
        let (color, scale) = (pt.color, pt.scale);
        ui::set_pixel(commands, art, entity, &mut pt, &s, color, scale);
    };
    // As above but with a colour change too (a teammate's name is blue).
    let set_pixel_coloured = |commands: &mut Commands, entity: Entity, pt: Option<Mut<PixelText>>, s: String, color: Color| {
        let (Some(art), Some(mut pt)) = (&art, pt) else { return };
        let scale = pt.scale;
        ui::set_pixel(commands, art, entity, &mut pt, &s, color, scale);
    };
    let now = fixed.elapsed_secs() as f64;
    let healths: Vec<i32> = (0..crate::combat::MAX_PLAYERS).map(|j| c.fighters[j].health).collect();
    lagging.advance(now, &healths);
    // Above each other player's head, per viewer: [viewer][who].
    let mut head_at = [[None::<Vec2>; crate::combat::MAX_PLAYERS]; crate::combat::MAX_PLAYERS];
    for (pc, cam, gt) in &cams {
        // world_to_viewport answers in window coordinates; make it relative to this view.
        let origin = cam.logical_viewport_rect().map_or(Vec2::ZERO, |r| r.min);
        for who in (0..crate::combat::MAX_PLAYERS).filter(|&j| j != pc.0 && c.roster.is_active(j)) {
            let (p, _) = render_pose(&world, &fixed, who);
            head_at[pc.0][who] = cam.world_to_viewport(gt, p + Vec3::Y * ALLY_MARK_HEIGHT).ok().map(|p| p - origin);
        }
    }
    for (entity, owner, bind, mut place, mut vis, text, pixel, image, bg, gradient, color, layout_info, pill_art) in &mut q {
        let i = owner.0;
        // This player's own view scale (the third player's view is not the size of the others').
        let scale = scale.of(i);
        // The fighter this HUD's player has locked (their lock target; a duel's only other fighter).
        let foe = c.aim_at(i, world.0.players[i].target);
        let f = &c.fighters[i];
        match *bind {
            Bind::Static => {}
            Bind::OwnHealth => set_text(text, f.health.to_string()),
            Bind::OwnStatus => set_text(text, status_tags(c, i)),
            Bind::Timer => {
                // In a quarter the timer drops below the dial: above it, it ran into the lock-on name.
                place.pos.y = if scale.dial_text > 1.0 { DIAL_AT.y + 72.0 } else { DIAL_AT.y - 104.0 };
                // Prop Hunt: the seeker's countdown, then the hunt clock (the sim's own time limit is off).
                let left = match &world.0.prophunt {
                    Some(ph) => ph.clock_ticks(),
                    None => c.time_left(),
                };
                show(&mut vis, left.is_some() && !shared);
                let secs = left.unwrap_or(0).div_ceil(crate::sim::HZ as u32);
                set_pixel_text(&mut commands, entity, pixel, format!("{}:{:02}", secs / 60, secs % 60));
            }
            Bind::SharedTimerBack => {
                let on = match &world.0.prophunt {
                    Some(ph) => ph.clock_ticks().is_some(),
                    None => c.time_left().is_some(),
                };
                show(&mut vis, on && shared);
            }
            Bind::SharedTimer => {
                let left = match &world.0.prophunt {
                    Some(ph) => ph.clock_ticks(),
                    None => c.time_left(),
                };
                show(&mut vis, left.is_some() && shared);
                let secs = left.unwrap_or(0).div_ceil(crate::sim::HZ as u32);
                let s = format!("{}:{:02}", secs / 60, secs % 60);
                // The pixel font is left-aligned: centre it by its own width.
                if let Some(a) = art.as_ref() {
                    let width = a.text_width(&s, ui::pixel_scale(place.font));
                    place.size.x = width;
                    place.pos.x = -width / 2.0;
                }
                set_pixel_text(&mut commands, entity, pixel, s);
            }
            Bind::Environment => {
                let name = c.env_def().map(|d| d.name.to_uppercase());
                show(&mut vis, name.is_some());
                set_text(text, name.map(|n| format!("ENVIRONMENT: {n}")).unwrap_or_default());
            }
            Bind::OwnTick(k) => {
                show(&mut vis, (k as i32) < f.max_health.max(1));
                let (x, w) = hl::tick(k, f.max_health, OWN_BAR.0, OWN_BAR.1, OWN_BAR.2);
                // Whole-pixel snapping as ever at 20 Health; fractional pitches (10, 40) stay exact.
                place.crisp = f.max_health != 20;
                place.pos.x = 134.0 + x;
                place.size.x = w;
                if let Some(mut bg) = bg {
                    // Health lost a moment ago stays as red ticks (the original's trail).
                    let want = if (k as i32) < f.health {
                        ORANGE
                    } else if (k as i32) < lagging.lag[i] as i32 {
                        TRAIL_RED
                    } else {
                        TICK_OFF
                    };
                    if bg.0 != want {
                        bg.0 = want;
                    }
                }
            }
            Bind::Row(r, part) => {
                let Some(row) = rows[i].get(r).copied() else {
                    show(&mut vis, false);
                    continue;
                };
                let (who, big) = (row.who, row.big);
                let o = &c.fighters[who];
                let ally = c.roster.same_team(i, who);
                // A player at 0 Health is drawn like any other: "00", no dots, the
                // pills and numbers as they were (Laurence's Laser clip: JD at 00
                // measures the same brightness as the live panels; the program has
                // no per-player dim for it, its `+0x21c` fade is a script-driven
                // fade of flagged parts). So no dim and no DOWN tag.
                let k_shade = 1.0;
                let pal = foe_palette(big, ally);
                let dots = hl::dots(o.health, lagging.lag[who]);
                // Panel units -> design px under the row's name line.
                let s = hl::FOE_S;
                let origin = hl::LIST_ORIGIN + Vec2::new(0.0, row.y);
                let unit = |x: f32, y: f32| Vec2::new(x * s, hl::PANEL_DY + y * s);
                let at = |place: &mut Place, p: Vec2, w: f32, h: f32, font: f32| {
                    place.pos = origin + p;
                    place.size = Vec2::new(w, h);
                    place.font = font;
                };
                let tier_col = |t: usize| shade(pal.tier[t.min(2)], k_shade);
                // A slim row (item 14's B) is its name line alone, with the Health and Level on it.
                if row.slim {
                    show(&mut vis, part == RowPart::Name);
                    if part == RowPart::Name {
                        let tag = if ally { " ALLY" } else { "" };
                        at(&mut place, Vec2::ZERO, 320.0, 22.0, if scale.dial_text > 1.0 { QUAD_NAME_FONT } else { 17.0 });
                        let text = format!("{}{tag}   {:02} / LV {}", crate::seats::seat_name(who), o.health.max(0), o.level);
                        set_pixel_coloured(&mut commands, entity, pixel, text, shade(pal.name, k_shade));
                    }
                    continue;
                }
                show(&mut vis, true);
                match part {
                    RowPart::Name => {
                        let tag = if ally { " ALLY" } else { "" };
                        let col = shade(pal.name, k_shade);
                        at(&mut place, Vec2::ZERO, 220.0, 22.0, if scale.dial_text > 1.0 { QUAD_NAME_FONT } else { 17.0 });
                        set_pixel_coloured(&mut commands, entity, pixel, format!("{}{tag}", crate::seats::seat_name(who)), col);
                    }
                    RowPart::Glow => {
                        show(&mut vis, big);
                        at(&mut place, unit(-7.0, -7.0), 148.0 * s, 50.0 * s, 0.0);
                        if let Some(mut img) = image {
                            if img.color != pal.glow {
                                img.color = pal.glow;
                            }
                        }
                    }
                    RowPart::Panel => {
                        at(&mut place, unit(0.0, 0.0), 134.0 * s, 36.0 * s, 0.0);
                        if let Some(mut img) = image {
                            let tint = shade(pal.panel, k_shade);
                            if img.color != tint {
                                img.color = tint;
                            }
                        }
                    }
                    RowPart::Health => {
                        at(&mut place, unit(12.5, 0.0), 30.0 * s, 22.0 * s, 26.0);
                        set_text(text, format!("{:02}", o.health.max(0)));
                        if let Some(mut col) = color {
                            let want = tier_col(dots.tier);
                            if col.0 != want {
                                col.0 = want;
                            }
                        }
                    }
                    RowPart::Tick(k) => {
                        // Painted in the program's order: the tier below under the
                        // rest of the bar, this tier's dots, then the red trail.
                        let col = if k < dots.filled {
                            Some(tier_col(dots.tier))
                        } else if k < dots.trail_end {
                            Some(shade(TRAIL_RED, k_shade))
                        } else if dots.lower {
                            Some(tier_col(dots.tier.saturating_sub(1)))
                        } else {
                            None
                        };
                        show(&mut vis, col.is_some());
                        at(&mut place, unit(45.0 + 4.0 * k as f32, 4.0), 3.0 * s, 3.5 * s, 0.0);
                        place.crisp = true;
                        if let (Some(want), Some(mut bg)) = (col, bg) {
                            if bg.0 != want {
                                bg.0 = want;
                            }
                        }
                    }
                    RowPart::Pill(slot) => {
                        let def = o.slots[slot].and_then(|id| c.skill(id));
                        show(&mut vis, def.is_some());
                        let (px, py) = hl::pill_unit(hl::SLOT_COMPASS[slot]);
                        at(&mut place, unit(px, py), 7.0 * s, 4.0 * s, 0.0);
                        if let (Some(mut img), Some(art), Some(def)) = (image, pill_art, def) {
                            match hl::pill_row(capsule_number(def)) {
                                Some(row) => {
                                    let r = rect((PILL.0, PILL.1, PILL.2 + PILL_STEP * row as f32, PILL.3, PILL.4));
                                    if img.rect != Some(r) || img.image != art.atlas {
                                        img.rect = Some(r);
                                        img.image = art.atlas.clone();
                                    }
                                }
                                None => {
                                    // The Aura Particle: kind 0 of the program's pill table (0x1402f1348) is sprite
                                    // record 1, hud0 (46,504) 8x4, the atlas's own grey pill.
                                    let r = rect(AURA_PILL);
                                    if img.rect != Some(r) || img.image != art.atlas {
                                        img.rect = Some(r);
                                        img.image = art.atlas.clone();
                                    }
                                }
                            }
                            let want = shade(Color::WHITE, k_shade);
                            if img.color != want {
                                img.color = want;
                            }
                        }
                    }
                    RowPart::Arsenal => {
                        at(&mut place, unit(46.5, 11.0), 24.0 * s, 22.0 * s, 24.0);
                        set_text(text, format!("{:02}", o.arsenal.len()));
                        if let Some(mut col) = color {
                            let want = shade(pal.text, k_shade);
                            if col.0 != want {
                                col.0 = want;
                            }
                        }
                    }
                    RowPart::Aura => {
                        at(&mut place, unit(76.5, 11.0), 56.0 * s, 22.0 * s, 24.0);
                        set_text(text, format!("{:02}/{:02}", o.aura(), o.level));
                        if let Some(mut col) = color {
                            let want = shade(pal.text, k_shade);
                            if col.0 != want {
                                col.0 = want;
                            }
                        }
                    }
                    RowPart::Status => {
                        at(&mut place, unit(140.0, 8.0), 420.0, 22.0, 16.0);
                        set_text(text, status_tags(c, who));
                    }
                }
            }
            Bind::AllyOrb(r) | Bind::AllyName(r) => {
                let who = rows[i].get(r).map(|row| row.who);
                // The program draws its per-player world name only in its
                // full (unsplit) view (`FUN_140150c30`, the branch reached
                // when the view mode `+0x2a0` is 0); in both split modes the
                // loop never reaches it. So in a split view there is none,
                // which also keeps it off the opponent list (docs/HUD.md).
                let allowed = world_label_in_split() || hud_marker::Set::for_players(local.slots(&c.roster).len()) == hud_marker::Set::Full;
                let marker = who
                    .filter(|_| allowed)
                    .filter(|&w| c.roster.same_team(i, w) && c.fighters[w].health > 0 && !(world.0.players[i].locked && foe == w))
                    .and_then(|w| head_at[i][w].map(|p| (w, p)))
                    .filter(|(_, p)| p.is_finite());
                show(&mut vis, marker.is_some());
                if let Some((w, p)) = marker {
                    let p = p / scale.main;
                    if matches!(*bind, Bind::AllyOrb(_)) {
                        place.pos = p - Vec2::splat(place.size.x / 2.0);
                    } else {
                        let name = crate::seats::seat_name(w);
                        let width = art.as_ref().map_or(80.0, |a| a.text_width(&name, ui::pixel_scale(17.0)));
                        place.size.x = width;
                        place.pos = p - Vec2::new(width / 2.0, 36.0);
                        set_pixel_text(&mut commands, entity, pixel, name);
                    }
                }
            }
            Bind::PauseBack | Bind::PauseTitle | Bind::PauseLine | Bind::PauseHint => {
                show(&mut vis, paused);
                if matches!(*bind, Bind::PauseBack) {
                    // Wide enough for the longest line (several seats lost).
                    let widest = art.as_ref().map_or(540.0, |a| a.text_width(&pause_line, ui::pixel_scale(20.0)));
                    place.size.x = (widest + 60.0).max(580.0);
                    place.pos.x = -place.size.x / 2.0;
                }
                let line = match *bind {
                    Bind::PauseTitle => Some(("PAUSED".to_string(), 40.0)),
                    Bind::PauseLine => Some((pause_line.clone(), 20.0)),
                    Bind::PauseHint => Some(("RECONNECT THE PAD TO CONTINUE".to_string(), 16.0)),
                    _ => None,
                };
                if let (true, Some((s, font))) = (paused, line) {
                    // The pixel font is left-aligned: centre it by its own width.
                    let width = art.as_ref().map_or(300.0, |a| a.text_width(&s, ui::pixel_scale(font)));
                    place.size.x = width;
                    place.pos.x = -width / 2.0;
                    set_pixel_text(&mut commands, entity, pixel, s);
                }
            }
            Bind::Arsenal => set_text(text, f.arsenal.len().to_string()),
            Bind::NextOrb => {
                show(&mut *vis, !f.arsenal.is_empty());
                if let (Some(mut img), Some(&next)) = (image, f.arsenal.last()) {
                    img.color = capsule_color(c.skill(next));
                }
            }
            Bind::Aura => set_text(text, format!("{:02}", f.aura())),
            Bind::Level => set_text(text, format!("{:02}", f.level)),
            Bind::DialSeg(k) => show(&mut vis, k < crate::hud_dial::lit_count(f.aura_ticks, f.level)),
            Bind::Bezel(k) => show(&mut vis, f.slots[k].and_then(|id| c.skill(id)).is_some()),
            Bind::Disc(k) => {
                show(&mut vis, f.slots[k].and_then(|id| c.skill(id)).is_some());
                if let Some(mut img) = image {
                    let want = capsule_color(f.slots[k].and_then(|id| c.skill(id)));
                    if img.color != want {
                        img.color = want;
                    }
                }
            }
            // Includes "Aura cost +1 per use" (and Knife Grind's -1) so far.
            Bind::Cost(k) => {
                set_text(text, f.slots[k].and_then(|id| c.skill(id)).map_or(String::new(), |d| (d.cost + d.cost_per_use() * f.spent[k] as i32).max(0).to_string()));
                // Bevy UI text draws from the top of its box, not centred in
                // it, so centre the digit on the disc from its own measured
                // height each frame (screen-space `TextLayoutInfo.size`,
                // converted back to design pixels) instead of a fixed
                // offset -- holds for 1- and 2-digit costs and every scale.
                if let Some(li) = layout_info {
                    if li.size.y > 0.0 && scale.dial > 0.0 {
                        let knob = DIAL_AT + KNOBS[k];
                        place.pos.y = knob.y - (li.size.y / scale.dial) / 2.0;
                    }
                }
                // Pressed without enough Aura: the cost blinks red.
                if let Some(mut col) = color {
                    let want = if f.denied_ticks[k] > 0 && (f.denied_ticks[k] / 4) % 2 == 1 { Color::srgb(1.0, 0.15, 0.1) } else { Color::WHITE };
                    if col.0 != want {
                        col.0 = want;
                    }
                }
            }
            Bind::Uses(k) => {
                let left = f.slots[k].and_then(|id| c.skill(id)).and_then(|d| d.uses()).map(|n| n.saturating_sub(f.spent[k]).max(1));
                show(&mut vis, left.is_some());
                let s = left.map_or(String::new(), |n| format!("x{n}"));
                // A couple of px clear of the knob's bezel, on whichever
                // side is clear of the ribbon (`USES_GAP`, reserved from the
                // name text at spawn), sized to the string's own measured
                // width (the pixel font's real glyph metrics) instead of a
                // guessed fixed width.
                if let Some(art) = &art {
                    let w = art.text_width(&s, ui::pixel_scale(16.0 * scale.dial_text));
                    let knob = DIAL_AT + KNOBS[k];
                    place.pos.x = if KNOBS[k].x < 0.0 { knob.x - 17.0 - w } else { knob.x + 17.0 };
                    place.size.x = w;
                }
                set_pixel_text(&mut commands, entity, pixel, s);
            }
            Bind::Ribbon(k) => {
                let def = f.slots[k].and_then(|id| c.skill(id));
                show(&mut *vis, def.is_some());
                // A bar in the capsule's colour, strongest at the knob, fading outwards.
                if let (Some(mut g), Some(d)) = (gradient, def) {
                    let c = capsule_color(Some(d));
                    let stops = vec![ColorStop::auto(c.with_alpha(0.8)), ColorStop::auto(c.with_alpha(0.5)), ColorStop::auto(c.with_alpha(0.0))];
                    let want = BackgroundGradient::from(if KNOBS[k].x < 0.0 { LinearGradient::to_left(stops) } else { LinearGradient::to_right(stops) });
                    if *g != want {
                        *g = want;
                    }
                }
            }
            Bind::Name(k) => {
                let name = f.slots[k].and_then(|id| c.skill(id)).map_or(String::new(), |d| d.name.clone());
                let knob = DIAL_AT + KNOBS[k];
                if KNOBS[k].x >= 0.0 {
                    // The room kept for "xN" grows with the dial's text in a quarter.
                    place.pos.x = knob.x + 15.0 + USES_GAP * scale.dial_text;
                } else if let Some(art) = &art {
                    // A left knob's name starts at the ribbon's outer end; when the
                    // (larger) text would run into the "xN" beside the bezel it
                    // moves in so it ends just short of it instead.
                    let ps = ui::pixel_scale(16.0 * scale.dial_text);
                    let uses = f.slots[k].and_then(|id| c.skill(id)).and_then(|d| d.uses()).map_or(String::new(), |n| format!("x{}", n.saturating_sub(f.spent[k]).max(1)));
                    let uses_w = if uses.is_empty() { 0.0 } else { art.text_width(&uses, ps) + 6.0 };
                    let outer = knob.x - 184.0 + 4.0;
                    place.pos.x = outer.min(knob.x - 17.0 - uses_w - art.text_width(&name, ps));
                }
                set_pixel_text(&mut commands, entity, pixel, name);
            }
            Bind::Countdown => {
                let t = c.round_ticks;
                let piece = if t < COUNTDOWN_TICKS {
                    Some(DIGITS[(2 - (t * 3 / COUNTDOWN_TICKS).min(2)) as usize])
                } else if t < COUNTDOWN_TICKS + START_SHOW_TICKS {
                    Some(START)
                } else {
                    None
                };
                banner(piece, 1.6, &mut *place, &mut *vis, image);
            }
            Bind::Card(part) => {
                // An empty button draws no card (`FUN_140155160` passes skill -1, `FUN_140168410` returns).
                let Some((v, lines)) = &cards[i] else {
                    show(&mut *vis, false);
                    continue;
                };
                use CardPart::*;
                let mut on = true;
                let sprite = |image: Option<Mut<ImageNode>>, s: hc::Sprite, tint: [u8; 3]| {
                    if let Some(mut img) = image {
                        let rect = Some(Rect::new(s.x, s.y, s.x + s.w, s.y + s.h));
                        if img.rect != rect {
                            img.rect = rect;
                        }
                        let col = Color::srgb_u8(tint[0], tint[1], tint[2]);
                        if img.color != col {
                            img.color = col;
                        }
                    }
                };
                match part {
                    Window | Shot | CostLabel => {}
                    Title => sprite(image, hc::TITLE, hc::title_rgb(v.capsule)),
                    School => sprite(image, hc::school_sprite(v.school), hc::NAME_RGB),
                    MainLabel => {
                        let s = match v.label {
                            hc::MainLabel::Str => hc::STR_LABEL,
                            hc::MainLabel::Def => hc::DEF_LABEL,
                            hc::MainLabel::Blank | hc::MainLabel::NoDef => hc::BLANK_LABEL,
                        };
                        place.size = Vec2::new(s.w, s.h) * hc::S;
                        sprite(image, s, hc::LABEL_RGB);
                    }
                    Range => match v.range {
                        Some(r) => sprite(image, r.sprite(), hc::LABEL_RGB),
                        None => on = false,
                    },
                    Infinity => on = v.uses.is_none(),
                    Name => set_pixel_text(&mut commands, entity, pixel, v.title.clone()),
                    Cost => set_pixel_text(&mut commands, entity, pixel, v.cost.clone()),
                    MainValue => set_pixel_text(&mut commands, entity, pixel, v.value.clone()),
                    Uses => {
                        on = v.uses.is_some();
                        set_pixel_text(&mut commands, entity, pixel, v.uses.map(|n| n.to_string()).unwrap_or_default());
                    }
                    Note(k) | NoteShadow(k) => set_pixel_text(&mut commands, entity, pixel, lines.get(k).cloned().unwrap_or_default()),
                }
                show(&mut *vis, on);
            }
            Bind::Capture => {
                let cap = c.capsule_in_reach(i, &world.0.players[i]).map(|k| c.capsules[k].skill);
                // Test hook: `PD_TEST_HINT=1` shows the prompt for the skill on button 0
                // (checks where it sits against the dial without walking to a capsule).
                let cap = cap.or_else(|| std::env::var_os("PD_TEST_HINT").and_then(|_| f.slots[0]));
                show(&mut *vis, cap.is_some());
                if let Some(d) = cap.and_then(|id| c.skill(id)) {
                    let cost = if d.id == AURA_PARTICLE { String::new() } else { format!("  (aura {})", d.cost) };
                    set_text(text, format!("{}{cost} - press a skill button to capture", d.name));
                    // Centred in the free width beside the dial's ribbons (the
                    // whole view when it is tall enough to be clear of them).
                    let sc = hl::Scales { main: scale.main, dial: scale.dial, dial_text: scale.dial_text };
                    let (x0, x1) = hl::hint_span(scale.view, sc, 30.0);
                    place.pos.x = (x0 - scale.view.x / 2.0) / scale.main;
                    place.size.x = (x1 - x0) / scale.main;
                }
            }
            Bind::Result => {
                // Time up with equal Health: the game's own DRAW banner.
                // A draw (the `DRAW` slot), the winner's team wins, everyone else loses.
                let piece = c.round_outcome().map(|o| match o {
                    crate::combat::Outcome::Draw => DRAW,
                    crate::combat::Outcome::Winner { team, .. } if team == c.roster.team[i] => YOU_WIN,
                    crate::combat::Outcome::Winner { .. } => YOU_LOSE,
                });
                banner(piece, 1.4, &mut *place, &mut *vis, image);
            }
        }
    }
}

fn banner(piece: Option<Piece>, zoom: f32, place: &mut Place, vis: &mut Visibility, image: Option<Mut<ImageNode>>) {
    let want = if piece.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    let (Some(p), Some(mut img)) = (piece, image) else { return };
    let r = rect(p);
    if img.rect != Some(r) {
        img.rect = Some(r);
    }
    place.size = Vec2::new(p.3, p.4) * zoom;
    place.pos = -place.size / 2.0 - Vec2::new(0.0, 60.0);
}

/// Design pixels -> screen pixels for ONE player's view: `main` for the
/// panels, `dial` for the bottom-right dial cluster (`hud_layout::scales`),
/// and the size of the view in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewScale {
    main: f32,
    dial: f32,
    /// Extra size for the dial's own text (skill names, costs, uses, timer) in
    /// a quarter, where the dial itself cannot grow as much as the panels.
    dial_text: f32,
    view: Vec2,
}

impl Default for ViewScale {
    fn default() -> Self {
        ViewScale { main: 1.0, dial: 1.0, dial_text: 1.0, view: DESIGN_VIEW }
    }
}

/// Every player's HUD scale. The views are not all the same size (with three
/// players the third is the whole bottom half), so each player's HUD is fitted
/// to ITS view: ask [`HudScale::of`] with the player's slot.
#[derive(Resource, Default)]
pub struct HudScale {
    per: [ViewScale; crate::combat::MAX_PLAYERS],
}

impl HudScale {
    /// Player `i`'s view scale (a slot past the last gets the last one's).
    pub fn of(&self, i: usize) -> ViewScale {
        self.per[i.min(crate::combat::MAX_PLAYERS - 1)]
    }
}

const DESIGN_VIEW: Vec2 = hl::DESIGN;

/// Size each player's HUD to THEIR view and place every element. The view
/// rectangles come from `views::ViewLayout` (one per player; with three players
/// the third's is the whole bottom half, so the views differ in size); HUDs of
/// slots with no player are not displayed.
#[allow(clippy::too_many_arguments)]
pub fn layout_hud(
    mut commands: Commands,
    art: Option<Res<MenuArt>>,
    area: Res<crate::display::GameArea>,
    settings: Res<menu::Settings>,
    world: Res<World>,
    local: Res<crate::seats::LocalViews>,
    mut scale: ResMut<HudScale>,
    mut roots: Query<(&HudRoot, &mut Node), Without<Place>>,
    mut screen_root: Query<&mut Node, (With<HudScreenRoot>, Without<Place>, Without<HudRoot>)>,
    mut items: Query<(Entity, &Owner, &Place, &Bind, &mut Node, Option<&mut TextFont>, Option<&mut PixelText>, Has<ScreenItem>), Without<HudRoot>>,
) {
    if !area.ready() {
        return;
    }
    // The HUD lives in the game area (the HUD camera's viewport), not the window.
    let (ww, wh) = (area.logical().x, area.logical().y);
    let layout = local.layout(&world.0.combat.roster, settings.split);
    let players = local.slots(&world.0.combat.roster).len();
    // The HUD size option (75-150%) multiplies the panel and dial scales together. Each slot's HUD is
    // fitted to its own view (a slot with no view here borrows view 0's, it is not displayed).
    let hud_k = settings.display.hud.scale();
    let roster = &world.0.combat.roster;
    let views: [hl::HudView; crate::combat::MAX_PLAYERS] = std::array::from_fn(|i| hl::hud_view(&layout, local.view_of(i, roster).unwrap_or(0), Vec2::new(ww, wh), hud_k));
    if !views.iter().all(|v| v.scales.main > 0.0 && v.scales.dial > 0.0) {
        return; // window not sized yet
    }
    *scale = HudScale { per: std::array::from_fn(|i| ViewScale { main: views[i].scales.main, dial: views[i].scales.dial, dial_text: views[i].scales.dial_text, view: views[i].size }) };
    let set = |v: &mut Val, px_: f32| {
        let want = Val::Px(px_.round());
        if *v != want {
            *v = want;
        }
    };
    for (r, mut n) in &mut roots {
        // The HUD root of slot `r.0` sits in that slot's view; slots with no view here have no HUD.
        let cell = local.view_of(r.0, roster);
        let v = &views[r.0.min(crate::combat::MAX_PLAYERS - 1)];
        let origin = if cell.is_some() { v.origin } else { layout.rect_f(0, Vec2::new(ww, wh)).pos };
        let want = if cell.is_some() { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
        set(&mut n.left, origin.x);
        set(&mut n.top, origin.y);
        set(&mut n.width, v.size.x);
        set(&mut n.height, v.size.y);
    }
    // The whole-screen layer covers the game area; its items use the largest view's scale.
    for mut n in &mut screen_root {
        set(&mut n.left, 0.0);
        set(&mut n.top, 0.0);
        set(&mut n.width, ww);
        set(&mut n.height, wh);
    }
    let biggest = views.iter().take(players.max(1)).map(|v| v.scales).fold(views[0].scales, |a, b| if b.main > a.main { b } else { a });
    for (entity, owner, p, bind, mut n, font, pixel, on_screen) in &mut items {
        // This item belongs to one player's HUD: their view's size and scales (a screen item: the whole area).
        let own = &views[owner.0.min(crate::combat::MAX_PLAYERS - 1)];
        let (view, sc) = if on_screen { (Vec2::new(ww, wh), biggest) } else { (own.size, own.scales) };
        let anchor = match p.anchor {
            Anchor::TopLeft => Vec2::ZERO,
            Anchor::TopRight => Vec2::new(view.x, 0.0),
            Anchor::BottomRight => view,
            Anchor::Center => view / 2.0,
        };
        // The dial cluster scales on its own (smaller than the panels in a quarter).
        let s = if p.anchor == Anchor::BottomRight { sc.dial } else { sc.main };
        let at = anchor + p.pos * s;
        let set = |v: &mut Val, px_: f32| {
            let want = Val::Px(if p.crisp { px_ } else { px_.round() });
            if *v != want {
                *v = want;
            }
        };
        set(&mut n.left, at.x);
        set(&mut n.top, at.y);
        set(&mut n.width, p.size.x * s);
        set(&mut n.height, p.size.y * s);
        let font_px = p.font * if matches!(bind, Bind::Timer | Bind::Name(_) | Bind::Cost(_) | Bind::Uses(_)) { sc.dial_text } else { 1.0 };
        if let Some(mut f) = font {
            let want = FontSize::Px((font_px * s).max(6.0));
            if f.font_size != want {
                f.font_size = want;
            }
        }
        // Pixel-font fields (names/skill names/timer): rescale to match --
        // `update_hud` owns their content, this owns their size, each
        // passing the other's current value back through `set_pixel`
        // unchanged (docs comment on `ui::set_pixel`).
        if let (Some(art), Some(mut pt)) = (&art, pixel) {
            let (text, color) = (pt.text.clone(), pt.color);
            ui::set_pixel(&mut commands, art, entity, &mut pt, &text, color, ui::pixel_scale(font_px * s));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Laurence's pick (2026-10-09): one shared clock with two or more views; a lone full-window view keeps
    /// its own clock in the corner, never one in the middle of the action.
    #[test]
    fn the_shared_clock_needs_two_views() {
        if std::env::var_os("PD_HUD_SHARED_TIMER").is_some() {
            return;
        }
        assert!(!use_shared_timer(1));
        assert!(use_shared_timer(2));
        assert!(use_shared_timer(4));
    }

    #[test]
    fn capsule_numbers_are_the_skill_tables() {
        // `capsule_number` + `hud_layout::pill_row` = the pill colour; the six kinds are distinct rows.
        let kinds = [Capsule::Attack, Capsule::Defence, Capsule::Erase, Capsule::Status, Capsule::Special, Capsule::Environment];
        let number = |c: Capsule| match c {
            Capsule::Attack => 1u8,
            Capsule::Defence => 2,
            Capsule::Erase => 3,
            Capsule::Status => 4,
            Capsule::Special => 5,
            Capsule::Environment => 6,
        };
        let mut rows: Vec<usize> = kinds.iter().map(|&c| hl::pill_row(number(c)).unwrap()).collect();
        rows.sort();
        assert_eq!(rows, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn the_panel_palette_is_the_programs() {
        let near = |a: Color, b: Color| (a.to_srgba().red - b.to_srgba().red).abs() < 1e-3 && (a.to_srgba().blue - b.to_srgba().blue).abs() < 1e-3;
        // Target: tiers 0xad3a03 / 0xb06b05 / 0xcc9835, panel 0xf57e27; others dimmer.
        let lit = foe_palette(true, false);
        assert!(near(lit.tier[0], rgb8(0xad3a03)) && near(lit.tier[2], rgb8(0xcc9835)) && near(lit.panel, rgb8(0xf57e27)));
        let dim = foe_palette(false, false);
        assert!(near(dim.tier[0], rgb8(0x642604)) && near(dim.text, rgb8(0x642604)) && near(dim.panel, rgb8(0xe98646)));
        assert!(dim.tier[0].to_srgba().red < lit.tier[0].to_srgba().red);
        // A teammate is green (the program's own values).
        assert!(near(foe_palette(true, true).panel, rgb8(0x89bc18)) && near(foe_palette(false, true).panel, rgb8(0x829f2a)));
        assert!(near(foe_palette(true, true).tier[2], rgb8(0x75a804)) && near(foe_palette(false, true).text, rgb8(0x5a8102)));
    }

    #[test]
    fn the_health_trail_lags_a_drop_then_catches_up() {
        let mut t = HealthLag::default();
        t.advance(0.0, &[7, 20, 20, 20]);
        t.advance(0.1, &[7, 20, 20, 20]); // steady: the hold counter is full
        assert_eq!(t.lag[0], 7.0);
        // Dropped to 3: the trail holds about a second, then drains in half a second.
        t.advance(0.5, &[3, 20, 20, 20]);
        assert_eq!(t.lag[0], 7.0);
        t.advance(1.0, &[3, 20, 20, 20]);
        assert_eq!(t.lag[0], 7.0, "still holding");
        t.advance(1.4, &[3, 20, 20, 20]);
        assert!(t.lag[0] > 3.0 && t.lag[0] < 7.0, "draining: {}", t.lag[0]);
        t.advance(1.7, &[3, 20, 20, 20]);
        assert_eq!(t.lag[0], 3.0);
    }
}
