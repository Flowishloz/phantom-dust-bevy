//! Shared look for the menu screens, rebuilt from the 2017 original's own
//! menu art (docs/MENUS.md): dark letterbox bands top and bottom, a header
//! in the game's pixel font, teal cloud backdrop with scan-lines and the
//! ghosted logo, wide teal row panels (selected = red with yellow text), a
//! description line and a prompt bar with the game's own button glyphs.
//!
//! Art comes from `cache/ui/menu/` (`pd_convert menus`); without it every
//! screen still works, drawn with plain panels and the HUD font.
//!
//! Every screen is laid out in a fixed 1280x720 design frame that
//! [`fit_frames`] scales to the window (`UiTransform`), so positions below
//! are design pixels.

use bevy::camera::ClearColorConfig;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::{FontSize, FontSource, FontWeight, Justify};
use bevy::ui::widget::NodeImageMode;
use std::path::Path;

/// Header / accent teal (the original's "MAIN MENU_").
pub const TEAL: Color = Color::srgb(0.45, 0.85, 0.72);
/// Unselected row text.
pub const ROW_TEXT: Color = Color::srgb(0.80, 0.98, 0.92);
/// Selected row text.
pub const YELLOW: Color = Color::srgb(1.0, 0.86, 0.22);
/// Body text (descriptions).
pub const WHITE: Color = Color::srgb(0.93, 0.95, 0.94);
pub const DIM: Color = Color::srgb(0.55, 0.62, 0.6);
pub const BAD: Color = Color::srgb(1.0, 0.5, 0.4);
const BG: Color = Color::srgb(0.0, 0.0, 0.0);
/// Row panel tints (the panel art is a grey gradient).
pub const PANEL: Color = Color::srgb(0.50, 0.86, 0.80);
pub const PANEL_SEL: Color = Color::srgb(0.62, 0.13, 0.20);
pub const PANEL_DIM: Color = Color::srgb(0.30, 0.46, 0.44);

/// Camera order for the frontend's own screens (Title/Setup/Arsenal/Result);
/// above the match's 3D views (0/1) and its HUD (5, `hud.rs`), below the
/// settings overlay (10, `menu.rs`) so Esc/Start still shows on top of any
/// of them.
pub const FRONTEND_ORDER: isize = 6;

pub const DESIGN: Vec2 = Vec2::new(1280.0, 720.0);
/// The design frame's bands (y, design pixels).
pub const BAND_TOP: f32 = 34.0;
pub const CONTENT_TOP: f32 = 104.0;
pub const CONTENT_BOTTOM: f32 = 612.0;
pub const BAND_BOTTOM: f32 = 684.0;

/// Pixel font: 16x16 cells, 64 per row, cell = Windows-1252 code. Letters
/// are spaced proportionally (their width + `GAP`, from `font.json`), which
/// matches the even letter gaps of the original's "MAIN MENU_"; without
/// `font.json` a fixed `ADVANCE` is used.
pub(crate) const CELL: f32 = 16.0;
const ADVANCE: f32 = 14.0;
const GAP: f32 = 4.0;
const SPACE: f32 = 9.0;

/// The game's row panel (`StMenu.alr` texture 1): a dark square tab on the
/// left of a grey gradient bar, tinted per state.
const ROW_RECT: Rect = Rect { min: Vec2::new(0.0, 0.0), max: Vec2::new(306.0, 38.0) };
/// Fraction of the row panel's width taken by the dark tab.
pub const TAB_FRAC: f32 = 46.0 / 306.0;
/// Ghosted "Phantom Dust" logo (`StMenu.alr` texture 2).
const LOGO_RECT: Rect = Rect { min: Vec2::new(12.0, 17.0), max: Vec2::new(245.0, 66.0) };

/// Button / arrow glyphs from `WusMnEn.alr` (X and Y sit below B at
/// 97,177 and 97,201; unused so far).
#[derive(Clone, Copy)]
pub enum Glyph {
    A,
    B,
    /// Up/down arrow ("Select").
    UpDown,
    /// Left/right arrow ("Change").
    LeftRight,
}

impl Glyph {
    /// (texture index, rect, drawn size in design pixels)
    fn art(self) -> (usize, Rect, Vec2) {
        let r = |x: f32, y: f32, w: f32, h: f32| Rect::new(x, y, x + w, y + h);
        match self {
            Glyph::A => (0, r(97.0, 129.0, 19.0, 19.0), Vec2::splat(27.0)),
            Glyph::B => (0, r(97.0, 153.0, 19.0, 19.0), Vec2::splat(27.0)),
            Glyph::UpDown => (3, r(474.0, 44.0, 11.0, 21.0), Vec2::new(14.0, 27.0)),
            Glyph::LeftRight => (3, r(6.0, 112.0, 21.0, 11.0), Vec2::new(27.0, 14.0)),
        }
    }

    fn fallback(self) -> &'static str {
        match self {
            Glyph::A => "[A]",
            Glyph::B => "[B]",
            Glyph::UpDown => "[Up/Down]",
            Glyph::LeftRight => "[Left/Right]",
        }
    }
}

/// A menu texture with a wrapping sampler (the scrolling cloud layers).
fn repeat_tex(assets: &AssetServer, path: &str) -> Handle<Image> {
    use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
    assets
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::Repeat, address_mode_v: ImageAddressMode::Repeat, ..ImageSamplerDescriptor::linear() }))
        .load(path.to_string())
}

/// Handles to the menu art, loaded once (the first screen that needs them
/// inserts this resource).
#[derive(Resource, Clone)]
pub struct MenuArt {
    /// False when `cache/ui/menu/` hasn't been converted: plain look.
    pub has_art: bool,
    font: Handle<Image>,
    pub ttf: Handle<Font>,
    row: Handle<Image>,
    logo: Handle<Image>,
    clouds: Handle<Image>,
    /// `StMenu.alr` textures 4 and 5: the cloud layer's two normal maps (frontend/cloud.rs).
    cloud_n: [Handle<Image>; 2],
    vs: [Handle<Image>; 4],
    wus: [Handle<Image>; 4],
    scan: Handle<Image>,
    fade: Handle<Image>,
    backdrop: Option<Handle<Image>>,
    /// Per code: (left edge in the cell, width) from `font.json`.
    glyphs: std::sync::Arc<Vec<(f32, f32)>>,
}

impl MenuArt {
    /// The shared art, loading it (and inserting the resource) on first use.
    pub fn get(commands: &mut Commands, existing: Option<Res<MenuArt>>, cache: &Path, assets: &AssetServer, images: &mut Assets<Image>) -> MenuArt {
        if let Some(a) = existing {
            return a.clone();
        }
        let art = MenuArt::load(cache, assets, images);
        commands.insert_resource(art.clone());
        art
    }

    fn load(cache: &Path, assets: &AssetServer, images: &mut Assets<Image>) -> MenuArt {
        let has_art = cache.join("ui/menu/font.png").exists();
        let img = |p: &str| if has_art { assets.load(format!("ui/menu/{p}")) } else { Handle::default() };
        let glyphs: Vec<(f32, f32)> = std::fs::read_to_string(cache.join("ui/menu/font.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["glyphs"].as_array().map(|a| a.iter().map(|g| (g[0].as_f64().unwrap_or(0.0) as f32, g[1].as_f64().unwrap_or(0.0) as f32)).collect()))
            .filter(|g: &Vec<(f32, f32)>| g.len() == 256)
            .unwrap_or_default();
        MenuArt {
            has_art,
            font: if has_art { assets.load_builder().with_settings(|s: &mut ImageLoaderSettings| s.sampler = ImageSampler::nearest()).load("ui/menu/font.png") } else { Handle::default() },
            ttf: if cache.join("ui/hud.ttf").exists() { assets.load("ui/hud.ttf") } else { Handle::default() },
            row: img("st1.png"),
            logo: img("logo.png"),
            clouds: if has_art { repeat_tex(assets, "ui/menu/st3.png") } else { Handle::default() },
            cloud_n: [4, 5].map(|i| if has_art { repeat_tex(assets, &format!("ui/menu/st{i}.png")) } else { Handle::default() }),
            vs: [0, 1, 2, 3].map(|i| img(&format!("vs{i}.png"))),
            wus: [0, 1, 2, 3].map(|i| img(&format!("wus{i}.png"))),
            scan: images.add(scanlines()),
            fade: images.add(side_fade()),
            backdrop: cache.join("ui/dck/hud1.png").exists().then(|| assets.load("ui/dck/hud1.png")),
            glyphs: std::sync::Arc::new(glyphs),
        }
    }

    /// The three textures of the scrolling cloud layer (frontend/cloud.rs).
    pub(super) fn cloud_layers(&self) -> (Handle<Image>, Handle<Image>, Handle<Image>) {
        (self.clouds.clone(), self.cloud_n[0].clone(), self.cloud_n[1].clone())
    }

    /// (source rect left, advance) of a character in font pixels.
    fn glyph_metrics(&self, code: u8) -> (f32, f32) {
        if self.glyphs.is_empty() {
            return (0.0, ADVANCE);
        }
        if code == b' ' {
            return (0.0, SPACE);
        }
        let (left, w) = self.glyphs[code as usize];
        if w == 0.0 {
            (0.0, SPACE)
        } else {
            (left, w + GAP)
        }
    }

    /// The font atlas (hud.rs's lock-on readout places the glyphs itself, at the
    /// program's own HUD advances rather than the menu's).
    pub(crate) fn font_image(&self) -> Handle<Image> {
        self.font.clone()
    }

    /// (atlas cell column, row, left edge of the ink, ink width) of a
    /// character in font pixels, or `None` when the art is not converted.
    pub(crate) fn glyph_ink(&self, ch: char) -> Option<(f32, f32, f32, f32)> {
        if self.glyphs.is_empty() {
            return None;
        }
        let code = cp1252(ch);
        let (left, w) = self.glyphs[code as usize];
        Some(((code % 64) as f32 * CELL, (code / 64) as f32 * CELL, left, w))
    }

    /// Width in design pixels of `text` in the pixel font at `scale`.
    pub fn text_width(&self, text: &str, scale: f32) -> f32 {
        if !self.has_art {
            return text.chars().count() as f32 * CELL * 0.6 * scale;
        }
        text.chars().map(|c| self.glyph_metrics(cp1252(c)).1).sum::<f32>() * scale
    }

    /// `text` cut to fit `width` design pixels at `scale` (ending "..").
    pub fn fit(&self, text: &str, width: f32, scale: f32) -> String {
        if self.text_width(text, scale) <= width {
            return text.to_string();
        }
        let mut out: String = text.to_string();
        while !out.is_empty() && self.text_width(&format!("{out}.."), scale) > width {
            out.pop();
        }
        out + ".."
    }

    /// The character's portrait from `VsMode.alr` (pc00a..pc10a in id order
    /// — matched by eye against renders of every model, docs/MENUS.md).
    pub fn portrait(&self, char_id: &str) -> Option<ImageNode> {
        let n: usize = char_id.strip_prefix("pc")?.get(..2)?.parse().ok()?;
        let (tex, y) = match n {
            0..=4 => (1, n as f32 * 98.0),
            5..=9 => (2, (n - 5) as f32 * 98.0),
            10 => (3, 0.0),
            _ => return None,
        };
        self.has_art.then(|| stretched(self.vs[tex].clone(), Rect::new(0.0, y, 134.0, y + 96.0), Color::WHITE))
    }

    /// The stage's preview picture from `VsMode.alr` texture 0 (254x112),
    /// where one is mapped (docs/MENUS.md — the mapping is unconfirmed).
    pub fn stage_preview(&self, stage_id: &str) -> Option<ImageNode> {
        let (x, y) = STAGE_PREVIEWS.iter().find(|(id, _)| *id == stage_id)?.1;
        self.has_art.then(|| stretched(self.vs[0].clone(), Rect::new(x, y, x + 254.0, y + 112.0), Color::WHITE))
    }

    /// The row panel image, tinted.
    pub fn panel(&self, tint: Color) -> ImageNode {
        if self.has_art {
            stretched(self.row.clone(), ROW_RECT, tint)
        } else {
            let c = tint.to_srgba();
            ImageNode { color: Color::srgba(c.red * 0.6, c.green * 0.6, c.blue * 0.6, 0.85), image_mode: NodeImageMode::Stretch, image: Handle::default(), ..default() }
        }
    }
}

fn stretched(image: Handle<Image>, rect: Rect, color: Color) -> ImageNode {
    ImageNode { image, rect: Some(rect), color, image_mode: NodeImageMode::Stretch, ..default() }
}

/// Stage id -> top-left of its 254x112 preview in `VsMode.alr` texture 0.
/// Families grouped by identical camera shots (3+3+3+3+2+2+1 = the 17
/// stages `stageset_en.dat` marks selectable); which recolour is which is
/// read from the colour words in the stage table's internal codes. All
/// unconfirmed — see docs/MENUS.md.
const STAGE_PREVIEWS: [(&str, (f32, f32)); 17] = [
    ("st02", (0.0, 382.0)),    // Spiral Highway (evening: orange)
    ("st12", (898.0, 114.0)),  // Lightning Highway (thunder: grey)
    ("st22", (1154.0, 342.0)), // Sunlight Highway (summer: blue sky)
    ("st16", (1154.0, 114.0)), // Sunset Refinery (yellow)
    ("st06", (642.0, 228.0)),  // Storm Refinery (blue)
    ("st26", (1410.0, 228.0)), // Acid Refinery (dark blue)
    ("st07", (642.0, 342.0)),  // Twilight Lane (orange)
    ("st17", (1154.0, 228.0)), // Silent Lane (grey-blue)
    ("st04", (642.0, 0.0)),    // Panorama Earthquake
    ("st14", (898.0, 342.0)),  // Panorama Building (fog)
    ("st08", (898.0, 0.0)),    // Sein (one-off)
    ("st05", (642.0, 114.0)),  // Dawn City
    ("st15", (1154.0, 0.0)),   // Dusk City (grey)
    ("st25", (1410.0, 114.0)), // Midnight City (blue)
    ("st03", (256.0, 382.0)),  // Lost Palace (blue)
    ("st13", (898.0, 228.0)),  // Light Palace
    ("st23", (1410.0, 0.0)),   // Noble Palace (red)
];

/// 1x4 tile: two dark lines, two clear — the original's scan-lines.
fn scanlines() -> Image {
    let data = vec![0, 0, 0, 120, 0, 0, 0, 70, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut img = Image::new(Extent3d { width: 1, height: 4, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, bevy::asset::RenderAssetUsages::default());
    img.sampler = ImageSampler::nearest();
    img
}

/// 256x1 black strip, opaque at both ends and clear in the middle: the dark
/// edges either side of the original's lit 4:3 middle.
fn side_fade() -> Image {
    let mut data = Vec::with_capacity(256 * 4);
    for x in 0..256 {
        let d = ((x as f32 + 0.5) / 256.0 - 0.5).abs() * 2.0; // 0 centre .. 1 edge
        let t = ((d - 0.55) / 0.3).clamp(0.0, 1.0);
        let a = t * t * (3.0 - 2.0 * t);
        data.extend_from_slice(&[0, 0, 0, (a * 255.0) as u8]);
    }
    Image::new(Extent3d { width: 256, height: 1, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, bevy::asset::RenderAssetUsages::default())
}

/// The fixed-size design frame every screen lays out in.
#[derive(Component)]
pub struct DesignFrame;

/// Scale every design frame to fit the window (letterboxed). Called each
/// frame from the screens' own update systems so resizing works.
pub fn fit_frames(windows: &Query<&Window>, frames: &mut Query<&mut UiTransform, With<DesignFrame>>) {
    // Fitted to the game area (the window unless a fixed aspect ratio leaves bars).
    let Some(a) = crate::display::ui_size(windows) else { return };
    let s = (a.x / DESIGN.x).min(a.y / DESIGN.y).max(0.01);
    for mut t in frames.iter_mut() {
        if (t.scale.x - s).abs() > 1e-4 {
            t.scale = Vec2::splat(s);
        }
    }
}

/// Absolutely positioned node in design pixels.
pub fn abs(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: px(x), top: px(y), width: px(w), height: px(h), ..default() }
}

/// A full-screen opaque camera, the original's backdrop and bands, and the
/// header ("MAIN MENU_") in the band. Returns (root, frame): add a screen's
/// content to `frame` with [`abs`] positions. The camera is a child of the
/// root so one recursive despawn on `OnExit` tears down both.
pub fn menu_screen(commands: &mut Commands, windows: &Query<&Window>, art: &MenuArt, header: &str, logo: bool) -> (Entity, Entity) {
    let cam = commands.spawn((Camera2d, Camera { order: FRONTEND_ORDER, clear_color: ClearColorConfig::Custom(BG), ..default() })).id();
    let root = commands.spawn((UiTargetCamera(cam), Node { width: percent(100), height: percent(100), ..default() }, BackgroundColor(BG))).add_child(cam).id();
    // Start at the right scale so the first frame is already laid out.
    let s = crate::display::ui_size(windows).map_or(1.0, |a| (a.x / DESIGN.x).min(a.y / DESIGN.y).max(0.01));
    let frame = commands
        .spawn((
            DesignFrame,
            UiTransform::from_scale(Vec2::splat(s)),
            Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), margin: UiRect { left: px(-DESIGN.x / 2.0), top: px(-DESIGN.y / 2.0), ..default() }, width: px(DESIGN.x), height: px(DESIGN.y), ..default() },
        ))
        .id();
    commands.entity(root).add_child(frame);
    let content_h = CONTENT_BOTTOM - CONTENT_TOP;
    commands.entity(frame).with_children(|f| {
        // Backdrop: the original's teal clouds (or the older stormy sky), dimmed.
        if art.has_art {
            // Scrolling cloud layer (a material node: frontend/cloud.rs gives it its material).
            f.spawn((abs(0.0, CONTENT_TOP, DESIGN.x, content_h), super::cloud::CloudBackdrop { tint: Color::srgb(0.30, 0.40, 0.40) }));
            // Ghosted logo, low in the middle (screens with room for it).
            if logo {
                f.spawn((abs(407.0, 452.0, 466.0, 98.0), stretched(art.logo.clone(), LOGO_RECT, Color::srgba(0.55, 0.85, 0.8, 0.28))));
            }
        } else if let Some(b) = &art.backdrop {
            f.spawn((abs(0.0, CONTENT_TOP, DESIGN.x, content_h), stretched(b.clone(), Rect::new(0.0, 0.0, 640.0, 380.0), Color::srgb(0.45, 0.5, 0.55))));
        }
        // Scan-lines over the whole middle, then dark sides.
        f.spawn((abs(0.0, CONTENT_TOP, DESIGN.x, content_h), ImageNode { image: art.scan.clone(), image_mode: NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 1.0 }, ..default() }));
        f.spawn((abs(0.0, CONTENT_TOP, DESIGN.x, content_h), ImageNode { image: art.fade.clone(), image_mode: NodeImageMode::Stretch, ..default() }));
        // Bands: black edge, grey band, (content), grey band, black edge.
        let band = Color::srgb(0.085, 0.09, 0.09);
        f.spawn((abs(0.0, BAND_TOP, DESIGN.x, CONTENT_TOP - BAND_TOP), BackgroundColor(band)));
        f.spawn((abs(0.0, CONTENT_BOTTOM, DESIGN.x, BAND_BOTTOM - CONTENT_BOTTOM), BackgroundColor(band)));
    });
    let h = spawn_pixel(commands, art, "", TEAL, 1.75);
    commands.entity(h).insert(HeaderReveal { text: header.to_uppercase(), t: 0.0 });
    commands.entity(h).insert(Node { position_type: PositionType::Absolute, left: px(150.0), top: px(CONTENT_TOP - 32.0), height: px(28.0), ..default() });
    commands.entity(frame).add_child(h);
    // Short fade-in reveal (smoother than the old instant cut between
    // screens): an opaque black sibling of `frame`, spawned after it so it
    // paints on top, fading itself out and despawning. `frame`'s content is
    // already fully laid out underneath by the time this starts, so the new
    // screen fades in already in place rather than popping.
    let fade = commands.spawn((Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 1.0)), FadeIn(0.0))).id();
    commands.entity(root).add_child(fade);
    (root, frame)
}

/// A fading-in black cover, removed once transparent. See [`update_fade`].
#[derive(Component)]
pub(crate) struct FadeIn(f32);

/// How long a screen takes to fade in from black.
/// READ `FUN_1401c6450` (the menu overlay's per-tick update, 30 Hz): its alpha moves +0x14 per tick up to 0xff, i.e. 13 ticks.
const FADE_IN_SECS: f32 = 13.0 / 30.0;

/// The header's type-on and blinking cursor. READ `FUN_1401c6630` (draw) + `FUN_1401c6450` (tick, 30 Hz): the number of
/// characters shown is `counter >> 1` where the counter resets to 0 when the text changes (15 chars/s), and a `_` is
/// appended while bit 3 of a free-running tick counter is set (8 ticks on, 8 off).
#[derive(Component)]
pub(crate) struct HeaderReveal {
    pub text: String,
    pub t: f32,
}

/// Characters visible after `t` seconds (`tick >> 1`, 30 Hz ticks) -- pure so a test pins it.
pub fn header_chars(t: f32, total: usize) -> usize {
    (((t * 30.0) as usize) >> 1).min(total)
}

/// Whether the header's trailing `_` is lit at `t` seconds (tick bit 3).
pub fn header_cursor_on(t: f32) -> bool {
    ((t * 30.0) as usize) & 8 != 0
}

/// Ticks every [`FadeIn`] overlay and despawns it once fully transparent.
/// Registered globally (`main.rs`): harmless when no screen just opened.
pub fn update_fade(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut FadeIn, &mut BackgroundColor)>,
    art: Option<Res<MenuArt>>,
    mut headers: Query<(Entity, &mut HeaderReveal, &mut PixelText)>,
) {
    if let Some(art) = art {
        for (e, mut h, mut pt) in &mut headers {
            h.t += time.delta_secs();
            let n = header_chars(h.t, h.text.chars().count());
            let mut shown: String = h.text.chars().take(n).collect();
            if header_cursor_on(h.t) {
                shown.push('_');
            }
            let (c, sc) = (pt.color, pt.scale);
            set_pixel(&mut commands, &art, e, &mut pt, &shown, c, sc);
        }
    }
    for (e, mut f, mut bg) in &mut q {
        f.0 += time.delta_secs();
        let t = (f.0 / FADE_IN_SECS).min(1.0);
        bg.0 = Color::srgba(0.0, 0.0, 0.0, 1.0 - t);
        if t >= 1.0 {
            commands.entity(e).despawn();
        }
    }
}

/// Menu text in the game's pixel font: a row of glyph images, rebuilt by
/// [`set_pixel`] when its text or colour changes.
#[derive(Component, Clone, PartialEq)]
pub struct PixelText {
    pub text: String,
    pub color: Color,
    pub scale: f32,
}

pub fn spawn_pixel(commands: &mut Commands, art: &MenuArt, text: &str, color: Color, scale: f32) -> Entity {
    let pt = PixelText { text: text.to_string(), color, scale };
    let e = commands.spawn((Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, height: px(CELL * scale), ..default() }, pt.clone())).id();
    build_glyphs(commands, art, e, &pt);
    e
}

/// Change a pixel text (no-op when nothing changed). `scale` is part of the
/// change check too (unlike text/colour it isn't obvious from the `&str`
/// call site) so a caller that only cares about content can pass
/// `pt.scale` back unchanged, and one that only cares about size (`hud.rs`,
/// rescaling every text field when the split layout changes) can pass
/// `pt.text.clone()`/`pt.color` back unchanged -- whichever last called this
/// wins its own concern without clobbering the other's.
pub fn set_pixel(commands: &mut Commands, art: &MenuArt, e: Entity, pt: &mut PixelText, text: &str, color: Color, scale: f32) {
    if pt.text == text && pt.color == color && (pt.scale - scale).abs() < 0.001 {
        return;
    }
    pt.text = text.to_string();
    pt.color = color;
    pt.scale = scale;
    commands.entity(e).despawn_children();
    build_glyphs(commands, art, e, pt);
}

fn build_glyphs(commands: &mut Commands, art: &MenuArt, e: Entity, pt: &PixelText) {
    glyph_children(&mut commands.entity(e), art, pt);
}

/// A design font size (px) as a [`spawn_pixel`]/[`glyphs_on`] `scale`
/// (glyph cells are [`CELL`] px tall at `scale` 1.0).
pub fn pixel_scale(font_px: f32) -> f32 {
    (font_px / CELL).max(0.05)
}

/// [`spawn_pixel`], but turning an entity the caller already holds
/// `EntityCommands` for into pixel-font text, instead of spawning a fresh
/// one -- for a caller (`hud.rs`) building each text entity through its own
/// per-element closure that only has `&mut EntityCommands`, not a top-level
/// `Commands` (which its own outer spawn loop is still holding a live
/// borrow of via another closure). The entity keeps `position_type`
/// `Absolute` (`hud.rs`'s only current caller places every element that
/// way; inserting a `Node` here would otherwise silently replace and lose
/// whatever position type the entity already had, since a second `insert`
/// of the same component type overwrites rather than merges it).
pub fn glyphs_on(e: &mut EntityCommands, art: &MenuArt, text: &str, color: Color, scale: f32) {
    let pt = PixelText { text: text.to_string(), color, scale };
    e.insert((Node { position_type: PositionType::Absolute, flex_direction: FlexDirection::Row, flex_shrink: 0.0, height: px(CELL * scale), ..default() }, pt.clone()));
    glyph_children(e, art, &pt);
}

/// Shared body of [`build_glyphs`]/[`glyphs_on`]: spawn `pt`'s glyph images
/// (or, without menu art, a plain `Text`) as children of `e`.
fn glyph_children(e: &mut EntityCommands, art: &MenuArt, pt: &PixelText) {
    let s = pt.scale;
    e.with_children(|c| {
        if !art.has_art {
            c.spawn((Text::new(pt.text.clone()), TextFont { font: FontSource::Handle(art.ttf.clone()), font_size: FontSize::Px(CELL * s * 0.95), weight: FontWeight(700), ..default() }, TextColor(pt.color)));
            return;
        }
        for ch in pt.text.chars() {
            let code = cp1252(ch);
            let (left, adv) = art.glyph_metrics(code);
            let node = Node { width: px(adv * s), height: px(CELL * s), flex_shrink: 0.0, ..default() };
            if code == b' ' {
                c.spawn(node);
                continue;
            }
            let (cx, cy) = ((code % 64) as f32 * CELL + left, (code / 64) as f32 * CELL);
            let w = adv.min(CELL - left);
            let node = Node { width: px(w * s), margin: UiRect::right(px((adv - w) * s)), ..node };
            c.spawn((node, stretched(art.font.clone(), Rect::new(cx, cy, cx + w, cy + CELL), pt.color)));
        }
    });
}

/// The font atlas's cell for a character (Windows-1252; unknown -> '?').
fn cp1252(ch: char) -> u8 {
    match ch as u32 {
        0x20..=0x7E | 0xA0..=0xFF => ch as u32 as u8,
        _ => match ch {
            '\u{2018}' | '\u{2019}' => b'\'',
            '\u{201C}' | '\u{201D}' => b'"',
            '\u{2013}' | '\u{2014}' => b'-',
            _ => b'?',
        },
    }
}

/// Body text in the smooth font (descriptions, notes, key hints).
pub fn body(text: &str, font: Handle<Font>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text.to_string()), TextFont { font: FontSource::Handle(font), font_size: FontSize::Px(size), ..default() }, TextColor(color), TextLayout { justify: Justify::Left, ..default() })
}

/// The pixel texts inside a row panel: the dark tab, the label (left) and
/// the value (right-aligned).
#[derive(Component, Clone, Copy)]
pub struct RowRefs {
    pub tab: Entity,
    pub left: Entity,
    pub right: Entity,
}

/// One wide row panel in the original's style. `node` must use px width
/// and height.
pub fn spawn_row(commands: &mut Commands, art: &MenuArt, node: Node, left: &str, right: &str, scale: f32) -> (Entity, RowRefs) {
    spawn_row_with(commands, art, node, ("", YELLOW), left, right, scale, PANEL, ROW_TEXT)
}

/// [`spawn_row`] with a tab text, panel tint and text colour.
#[allow(clippy::too_many_arguments)]
pub fn spawn_row_with(commands: &mut Commands, art: &MenuArt, node: Node, tab: (&str, Color), left: &str, right: &str, scale: f32, tint: Color, color: Color) -> (Entity, RowRefs) {
    let w = if let Val::Px(w) = node.width { w } else { 400.0 };
    let h = if let Val::Px(h) = node.height { h } else { 38.0 };
    let tab_w = w * TAB_FRAC;
    let pad = |scale: f32| ((h - CELL * scale) / 2.0).max(0.0);
    let panel = commands.spawn((node, art.panel(tint))).id();
    let tab_scale = scale.min(1.0);
    let tab = spawn_pixel(commands, art, tab.0, tab.1, tab_scale);
    commands.entity(tab).insert(Node { position_type: PositionType::Absolute, left: px(0.0), top: px(pad(tab_scale)), width: px(tab_w), height: px(CELL * tab_scale), justify_content: JustifyContent::Center, ..default() });
    let l = spawn_pixel(commands, art, left, color, scale);
    commands.entity(l).insert(Node { position_type: PositionType::Absolute, left: px(tab_w + 16.0), top: px(pad(scale)), height: px(CELL * scale), ..default() });
    let r = spawn_pixel(commands, art, right, color, scale);
    commands.entity(r).insert(Node { position_type: PositionType::Absolute, right: px(14.0), top: px(pad(scale)), height: px(CELL * scale), ..default() });
    let refs = RowRefs { tab, left: l, right: r };
    commands.entity(panel).add_children(&[tab, l, r]).insert(refs);
    (panel, refs)
}

/// Update a row's panel tint and its three texts. The tint eases toward its
/// target each call (screens call this once a frame from their `layout`/
/// `input` systems) instead of snapping, so the highlight moving between
/// rows reads as a smooth colour shift rather than an instant cut.
#[allow(clippy::too_many_arguments)]
pub fn set_row(commands: &mut Commands, art: &MenuArt, refs: RowRefs, panel: &mut ImageNode, texts: &mut Query<&mut PixelText>, tint: Color, tab: (&str, Color), left: (&str, Color), right: (&str, Color)) {
    let want = art.panel(tint).color;
    if panel.color != want {
        panel.color = ease_toward(panel.color, want);
    }
    for (e, (t, c)) in [(refs.tab, tab), (refs.left, left), (refs.right, right)] {
        if let Ok(mut pt) = texts.get_mut(e) {
            let s = pt.scale;
            set_pixel(commands, art, e, &mut pt, t, c, s);
        }
    }
}

/// One step of an exponential ease from `cur` to `target` (snaps once close
/// enough that another step would be imperceptible, so the highlight
/// actually settles instead of crawling forever).
fn ease_toward(cur: Color, target: Color) -> Color {
    const RATE: f32 = 0.35;
    let (c, t) = (cur.to_srgba(), target.to_srgba());
    let lerp = |a: f32, b: f32| a + (b - a) * RATE;
    let close = (c.red - t.red).abs() < 0.01 && (c.green - t.green).abs() < 0.01 && (c.blue - t.blue).abs() < 0.01 && (c.alpha - t.alpha).abs() < 0.01;
    if close {
        target
    } else {
        Color::srgba(lerp(c.red, t.red), lerp(c.green, t.green), lerp(c.blue, t.blue), lerp(c.alpha, t.alpha))
    }
}

/// The bottom prompt bar: glyph + label pairs on the left and right (like
/// the original's "↕Select … A Confirm  B Back"), plus a dim line of
/// keyboard keys underneath that only shows while the keyboard is the device
/// in use ([`KeyLine`]).
pub fn prompt_bar(commands: &mut Commands, art: &MenuArt, frame: Entity, left: &[(Glyph, &str)], right: &[(Glyph, &str)], keys: &str) {
    let ids = prompt_bar_parts(commands, art, left, right, keys);
    commands.entity(frame).add_children(&ids);
}

/// [`prompt_bar`]'s three entities (left prompts, right prompts, key line),
/// positioned in design pixels but not parented, for a screen that keeps
/// several bars and shows one at a time (the versus setup's two pages).
pub fn prompt_bar_parts(commands: &mut Commands, art: &MenuArt, left: &[(Glyph, &str)], right: &[(Glyph, &str)], keys: &str) -> [Entity; 3] {
    let y = CONTENT_BOTTOM + 9.0;
    let l = prompt_row(commands, art, left, 22.0, Node { position_type: PositionType::Absolute, left: px(150.0), top: px(y), height: px(30.0), align_items: AlignItems::Center, ..default() });
    let r = prompt_row(commands, art, right, 22.0, Node { position_type: PositionType::Absolute, right: px(126.0), top: px(y), height: px(30.0), align_items: AlignItems::Center, ..default() });
    let k = commands.spawn((body(keys, art.ttf.clone(), 14.0, DIM), Node { position_type: PositionType::Absolute, left: px(150.0), top: px(y + 38.0), ..default() }, KeyLine, Visibility::Hidden)).id();
    [l, r, k]
}

/// A row of glyph + label prompts in `node` (which should lay out as a row).
pub fn prompt_row(commands: &mut Commands, art: &MenuArt, items: &[(Glyph, &str)], size: f32, node: Node) -> Entity {
    let e = commands.spawn(node).id();
    let k = size / 22.0;
    commands.entity(e).with_children(|c| {
        for (g, label) in items {
            let (tex, rect, glyph) = g.art();
            if art.has_art {
                c.spawn((Node { width: px(glyph.x * k), height: px(glyph.y * k), margin: UiRect::right(px(4.0 * k)), ..default() }, stretched(art.wus[tex].clone(), rect, Color::WHITE)));
            }
            let text = if art.has_art { label.to_string() } else { format!("{} {label}", g.fallback()) };
            c.spawn((body(&text, art.ttf.clone(), size, WHITE), Node { margin: UiRect::right(px(24.0 * k)), ..default() }));
        }
    });
    e
}

/// The small keyboard-keys line under a prompt bar. Hidden until a keyboard
/// key is pressed, hidden again when a gamepad button is pressed (Laurence,
/// 2026-10-09: de-clutter; UI-DECLUTTER-PROPOSALS.md item 1).
#[derive(Component)]
pub struct KeyLine;

/// Whether the keyboard is the device in use on the menus (the last press
/// came from it rather than from a gamepad).
#[derive(Resource, Default)]
pub struct KeyboardInUse(pub bool);

/// Registers [`show_key_lines`].
pub fn plugin(app: &mut App) {
    app.init_resource::<KeyboardInUse>().add_systems(Update, show_key_lines);
}

/// Shows the [`KeyLine`]s while the keyboard is in use.
pub fn show_key_lines(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut using: ResMut<KeyboardInUse>, mut lines: Query<&mut Visibility, With<KeyLine>>) {
    if keys.get_just_pressed().next().is_some() {
        using.0 = true;
    } else if pads.iter().any(|p| p.get_just_pressed().next().is_some()) {
        using.0 = false;
    }
    let want = if using.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut lines {
        if *v != want {
            *v = want;
        }
    }
}

/// The title screen's keyboard line.
pub const KEYS_TITLE: &str = "Keyboard: W S select, Enter confirm";
/// The result screen's keyboard line.
pub const KEYS_RESULT: &str = "Keyboard: Enter continue";

/// The keyboard line for the screens every seat drives: the keyboard is seat
/// 1, each gamepad takes the next seat by pressing a button on the setup screen.
pub const KEYS_SEATS: &str = "Keyboard (seat 1): W S A D, Enter, Backspace, Tab = Start, Esc   Gamepads: D-pad, A, B, Start (seat 1)   Y add CPU, X remove it, LB/RB (Q/E) pick a seat";
/// The same for the game rules screen (nobody joins there).
pub const KEYS_RULES: &str = "Keyboard (seat 1): W S A D, Enter, Backspace = back, Esc = back      Gamepads: D-pad, A, B";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_previews_are_distinct_cells_inside_the_atlas() {
        let mut seen = std::collections::HashSet::new();
        for (id, (x, y)) in STAGE_PREVIEWS {
            assert!(x + 254.0 <= 2048.0 && y + 112.0 <= 512.0, "{id} outside VsMode texture 0");
            assert!(seen.insert((x as i32, y as i32)), "{id} shares a preview");
        }
        let ids: std::collections::HashSet<_> = STAGE_PREVIEWS.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), 17);
        assert!(!ids.contains("st01") && !ids.contains("st24"), "not selectable in the original's versus table");
    }

    #[test]
    fn font_cells_follow_windows_1252() {
        assert_eq!(cp1252('A'), 0x41);
        assert_eq!(cp1252('é'), 0xE9);
        assert_eq!(cp1252('\u{2019}'), b'\'');
        assert_eq!(cp1252('漢'), b'?');
    }
}

#[cfg(test)]
mod key_line_tests {
    use super::*;

    #[test]
    fn the_key_line_stays_hidden_until_a_keyboard_key_is_pressed() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        plugin(&mut app);
        let line = app.world_mut().spawn((KeyLine, Visibility::Hidden)).id();
        app.update();
        assert_eq!(app.world().get::<Visibility>(line), Some(&Visibility::Hidden), "no keyboard use yet");
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter);
        app.update();
        assert_eq!(app.world().get::<Visibility>(line), Some(&Visibility::Inherited), "the keyboard is in use");
    }
}

#[cfg(test)]
mod reveal_tests {
    use super::*;
    #[test]
    fn header_types_on_at_15_chars_per_second_and_blinks_8_ticks() {
        assert_eq!(header_chars(0.0, 9), 0);
        assert_eq!(header_chars(2.0 / 30.0, 9), 1);
        assert_eq!(header_chars(1.0, 9), 9);
        assert!(!header_cursor_on(0.0));
        assert!(header_cursor_on(8.0 / 30.0));
        assert!(!header_cursor_on(16.0 / 30.0));
    }
}
