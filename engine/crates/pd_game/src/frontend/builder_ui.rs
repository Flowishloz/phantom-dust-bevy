//! Drawing for the arsenal builder (`builder.rs` decides what is shown as a
//! [`View`]; this file spawns the widgets once and pushes a view into them
//! every frame). Layout is in the 1280x720 design frame of `ui.rs`:
//!
//! ```text
//!   | STOCK / ARSENAL LIST (x 40, 450 wide) | ARSENAL / CONTENTS (x 510, 380) | DETAIL (x 910, 330) |
//!   |   10 rows                             |   10 rows                       |                     |
//!   | skill card                            | problems                        |                     |
//!   | prompt line + keyboard line (bottom band)                                                     |
//! ```
//!
//! A modal dialog panel (confirm / notice / options / sort / mask / mark /
//! kind / name entry with an on-screen keyboard) is drawn over everything.

use super::builder::{DialogView, RowView, Tone, View, VISIBLE};
use super::ui::{self, MenuArt, PixelText, RowRefs};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::text::{FontSize, FontSource, Justify};

/// The two list columns (design pixels): (x, width).
pub const COLS: [(f32, f32); 2] = [(40.0, 450.0), (510.0, 380.0)];
const DETAIL: (f32, f32, f32, f32) = (910.0, ui::CONTENT_TOP + 6.0, 330.0, 360.0);
const LIST_TOP: f32 = ui::CONTENT_TOP + 66.0;
const ROW_H: f32 = 26.0;
const ROW_GAP: f32 = 3.0;
const BELOW: f32 = LIST_TOP + VISIBLE as f32 * (ROW_H + ROW_GAP) + 8.0;
/// Rows a dialog can list (Kind has five, Sort three, Options four).
const DIALOG_ROWS: usize = 6;
const DIALOG: (f32, f32, f32, f32) = (250.0, 130.0, 780.0, 410.0);
const KB_CELLS: usize = 43;

/// One visible row of a list column.
#[derive(Component)]
pub struct ListRow {
    col: usize,
    slot: usize,
}

#[derive(Component)]
pub struct Caption(usize);

/// Body-text lines whose content changes.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Header,
    Info0,
    Info1,
    Tabs,
    Detail,
    Card,
    Problems,
    Prompt,
    Keys,
    DialogBody,
    DialogHint,
}

#[derive(Component)]
pub struct DialogPanel;

#[derive(Component)]
pub struct DialogTitle;

#[derive(Component)]
pub struct DialogRow(usize);

#[derive(Component)]
pub struct KeyCell(usize);

fn text_font(art: &MenuArt, size: f32) -> TextFont {
    TextFont { font: FontSource::Handle(art.ttf.clone()), font_size: FontSize::Px(size), ..default() }
}

/// Spawn every widget once, under `frame`.
pub fn spawn_chrome(commands: &mut Commands, art: &MenuArt, frame: Entity) {
    let mut kids = Vec::new();
    kids.push(commands.spawn((ui::body("", art.ttf.clone(), 17.0, ui::TEAL), Node { position_type: PositionType::Absolute, left: px(540.0), right: px(40.0), top: px(ui::CONTENT_TOP - 28.0), ..default() }, Line::Header)).id());
    for (col, (x, w)) in COLS.into_iter().enumerate() {
        let cap = ui::spawn_pixel(commands, art, "", ui::TEAL, 1.25);
        commands.entity(cap).insert((Node { position_type: PositionType::Absolute, left: px(x), top: px(ui::CONTENT_TOP + 6.0), height: px(20.0), ..default() }, Caption(col)));
        kids.push(cap);
        let info = if col == 0 { Line::Info0 } else { Line::Info1 };
        kids.push(commands.spawn((ui::body("", art.ttf.clone(), 14.0, ui::DIM), ui::abs(x + 4.0, ui::CONTENT_TOP + 30.0, w, 16.0), info)).id());
        for slot in 0..VISIBLE {
            let (panel, _) = ui::spawn_row(commands, art, ui::abs(x, LIST_TOP + slot as f32 * (ROW_H + ROW_GAP), w, ROW_H), "", "", 1.0);
            commands.entity(panel).insert(ListRow { col, slot });
            kids.push(panel);
        }
    }
    kids.push(commands.spawn((ui::body("", art.ttf.clone(), 13.0, ui::TEAL), ui::abs(COLS[0].0 + 4.0, ui::CONTENT_TOP + 48.0, COLS[0].1, 16.0), Line::Tabs)).id());
    kids.push(commands.spawn((ui::body("", art.ttf.clone(), 15.0, ui::WHITE), ui::abs(DETAIL.0, DETAIL.1, DETAIL.2, DETAIL.3), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.28)), Line::Detail)).id());
    kids.push(commands.spawn((ui::body("", art.ttf.clone(), 16.0, ui::WHITE), ui::abs(COLS[0].0 + 4.0, BELOW, COLS[0].1, 600.0 - BELOW), Line::Card)).id());
    kids.push(commands.spawn((ui::body("", art.ttf.clone(), 15.0, ui::WHITE), ui::abs(COLS[1].0 + 4.0, BELOW, COLS[1].1, 600.0 - BELOW), Line::Problems)).id());
    let y = ui::CONTENT_BOTTOM + 9.0;
    kids.push(commands.spawn((ui::body("", art.ttf.clone(), 17.0, ui::WHITE), Node { position_type: PositionType::Absolute, left: px(150.0), top: px(y), ..default() }, Line::Prompt)).id());
    kids.push(commands.spawn((ui::body("", art.ttf.clone(), 14.0, ui::DIM), Node { position_type: PositionType::Absolute, left: px(150.0), top: px(y + 34.0), ..default() }, Line::Keys, ui::KeyLine, Visibility::Hidden)).id());
    commands.entity(frame).add_children(&kids);

    // The dialog, last so it paints over everything else.
    let (dx, dy, dw, dh) = DIALOG;
    let panel = commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: px(dx), top: px(dy), width: px(dw), height: px(dh), border: UiRect::all(px(2.0)), ..default() },
            BackgroundColor(Color::srgb(0.02, 0.07, 0.08)),
            BorderColor::all(ui::TEAL),
            Visibility::Hidden,
            DialogPanel,
        ))
        .id();
    let mut inner = Vec::new();
    let title = ui::spawn_pixel(commands, art, "", ui::TEAL, 1.25);
    commands.entity(title).insert((Node { position_type: PositionType::Absolute, left: px(22.0), top: px(14.0), height: px(20.0), ..default() }, DialogTitle));
    inner.push(title);
    inner.push(commands.spawn((ui::body("", art.ttf.clone(), 17.0, ui::WHITE), ui::abs(22.0, 50.0, dw - 50.0, 70.0), Line::DialogBody)).id());
    for slot in 0..DIALOG_ROWS {
        let (row, _) = ui::spawn_row(commands, art, ui::abs(22.0, 128.0 + slot as f32 * 38.0, dw - 50.0, 34.0), "", "", 1.0);
        commands.entity(row).insert((DialogRow(slot), Visibility::Hidden));
        inner.push(row);
    }
    for i in 0..KB_CELLS {
        let (x, y, w) = kb_cell_rect(i);
        inner.push(
            commands
                .spawn((
                    Node { position_type: PositionType::Absolute, left: px(x), top: px(y), width: px(w), height: px(38.0), padding: UiRect::top(px(9.0)), ..default() },
                    BackgroundColor(Color::NONE),
                    Text::new(""),
                    text_font(art, 17.0),
                    TextColor(ui::ROW_TEXT),
                    TextLayout { justify: Justify::Center, ..default() },
                    Visibility::Hidden,
                    KeyCell(i),
                ))
                .id(),
        );
    }
    inner.push(commands.spawn((ui::body("", art.ttf.clone(), 14.0, ui::DIM), ui::abs(22.0, dh - 34.0, dw - 50.0, 20.0), Line::DialogHint)).id());
    commands.entity(panel).add_children(&inner);
    commands.entity(frame).add_child(panel);
}

/// (left, top, width) of keyboard cell `i` inside the dialog panel.
fn kb_cell_rect(i: usize) -> (f32, f32, f32) {
    let top = 124.0;
    if i < 40 {
        (38.0 + (i % 10) as f32 * 71.0, top + (i / 10) as f32 * 44.0, 65.0)
    } else {
        (38.0 + (i - 40) as f32 * 236.0, top + 4.0 * 44.0 + 4.0, 224.0)
    }
}

/// Every query the per-frame update needs, bundled.
#[derive(SystemParam)]
pub struct Widgets<'w, 's> {
    commands: Commands<'w, 's>,
    rows: Query<'w, 's, (&'static ListRow, &'static RowRefs, &'static mut ImageNode), Without<DialogRow>>,
    dialog_rows: Query<'w, 's, (&'static DialogRow, &'static RowRefs, &'static mut ImageNode, &'static mut Visibility), (Without<ListRow>, Without<KeyCell>)>,
    captions: Query<'w, 's, (Entity, &'static Caption)>,
    title: Query<'w, 's, Entity, With<DialogTitle>>,
    texts: Query<'w, 's, &'static mut PixelText>,
    lines: Query<'w, 's, (&'static Line, &'static mut Text, &'static mut TextColor), Without<KeyCell>>,
    cells: Query<'w, 's, (&'static KeyCell, &'static mut Text, &'static mut TextColor, &'static mut BackgroundColor, &'static mut Visibility), (Without<Line>, Without<DialogRow>, Without<DialogPanel>)>,
    panel: Query<'w, 's, &'static mut Visibility, (With<DialogPanel>, Without<DialogRow>, Without<KeyCell>)>,
}

fn tint_of(tone: Tone) -> Color {
    match tone {
        Tone::Cursor => ui::PANEL_SEL,
        Tone::Focus | Tone::DimCursor => ui::PANEL,
        Tone::Dim => ui::PANEL_DIM,
        Tone::Empty => Color::srgba(0.2, 0.3, 0.3, 0.35),
    }
}

impl Widgets<'_, '_> {
    /// Push `view` into the widgets (cheap when nothing changed: every setter
    /// compares first).
    pub fn apply(&mut self, art: &MenuArt, view: &View) {
        for (cap, c) in &self.captions {
            let (text, focused) = &view.captions[c.0];
            let color = if *focused { ui::YELLOW } else { ui::TEAL };
            if let Ok(mut pt) = self.texts.get_mut(cap) {
                let s = pt.scale;
                ui::set_pixel(&mut self.commands, art, cap, &mut pt, text, color, s);
            }
        }
        for (row, refs, mut panel) in &mut self.rows {
            let data = view.rows[row.col].get(row.slot).cloned().unwrap_or_else(|| RowView { tab: String::new(), name: String::new(), right: String::new(), color: ui::ROW_TEXT, tone: Tone::Empty });
            let w = COLS[row.col].1;
            let name = art.fit(&data.name, w * (1.0 - ui::TAB_FRAC) - 30.0 - art.text_width(&data.right, 1.0), 1.0);
            let cursor = data.tone == Tone::Cursor;
            let text_color = if cursor { ui::YELLOW } else { ui::ROW_TEXT };
            let right_color = if cursor { ui::YELLOW } else { data.color };
            // A dim column's names keep the capsule colour too, but softer.
            let name_color = if data.tone == Tone::Dim { Color::srgb(0.66, 0.78, 0.74) } else { text_color };
            ui::set_row(&mut self.commands, art, *refs, &mut panel, &mut self.texts, tint_of(data.tone), (&data.tab, ui::YELLOW), (&name, name_color), (&data.right, right_color));
        }
        for (line, mut text, mut color) in &mut self.lines {
            // Under a dialog the page's own texts are blanked (text is not
            // reliably hidden by a panel drawn over it).
            let covered = view.dialog.is_some() && matches!(line, Line::Detail | Line::Card | Line::Problems | Line::Info0 | Line::Info1 | Line::Tabs);
            let (body, tint): (&str, Color) = match line {
                _ if covered => ("", ui::WHITE),
                Line::Header => (&view.header, view.header_color),
                Line::Info0 => (&view.infos[0], ui::DIM),
                Line::Info1 => (&view.infos[1], ui::DIM),
                Line::Tabs => (&view.tabs, ui::TEAL),
                Line::Detail => (&view.detail, ui::WHITE),
                Line::Card => (&view.card, ui::WHITE),
                Line::Problems => (&view.problems.0, view.problems.1),
                Line::Prompt => (&view.prompt, ui::WHITE),
                Line::Keys => (&view.keys, ui::DIM),
                Line::DialogBody => (view.dialog.as_ref().map_or("", |d| d.body.as_str()), ui::WHITE),
                Line::DialogHint => (view.dialog.as_ref().map_or("", |d| d.hint.as_str()), ui::DIM),
            };
            if text.0 != body {
                text.0 = body.to_string();
            }
            if color.0 != tint {
                color.0 = tint;
            }
        }
        self.apply_dialog(art, view.dialog.as_ref());
    }

    fn apply_dialog(&mut self, art: &MenuArt, dialog: Option<&DialogView>) {
        for mut v in &mut self.panel {
            let want = if dialog.is_some() { Visibility::Visible } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
        for e in &self.title {
            if let Ok(mut pt) = self.texts.get_mut(e) {
                let s = pt.scale;
                ui::set_pixel(&mut self.commands, art, e, &mut pt, dialog.map_or("", |d| d.title.as_str()), ui::TEAL, s);
            }
        }
        for (row, refs, mut panel, mut vis) in &mut self.dialog_rows {
            let item = dialog.and_then(|d| d.items.get(row.0));
            let want = if item.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != want {
                *vis = want;
            }
            let Some(item) = item else { continue };
            let cursor = dialog.is_some_and(|d| d.cursor == row.0);
            let tint = if cursor { ui::PANEL_SEL } else { ui::PANEL };
            let color = if cursor { ui::YELLOW } else { ui::ROW_TEXT };
            let tab = (row.0 + 1).to_string();
            let name = art.fit(&item.to_uppercase(), 780.0 * (1.0 - ui::TAB_FRAC) - 80.0, 1.0);
            ui::set_row(&mut self.commands, art, *refs, &mut panel, &mut self.texts, tint, (&tab, ui::YELLOW), (&name, color), ("", color));
        }
        let keyboard = dialog.and_then(|d| d.keyboard.as_ref());
        for (cell, mut text, mut color, mut bg, mut vis) in &mut self.cells {
            let want = if keyboard.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != want {
                *vis = want;
            }
            let Some((labels, cursor)) = keyboard else { continue };
            let label = labels.get(cell.0).map_or("", String::as_str);
            if text.0 != label {
                text.0 = label.to_string();
            }
            let on = *cursor == cell.0;
            let (fg, back) = if on { (ui::YELLOW, ui::PANEL_SEL) } else { (ui::ROW_TEXT, Color::srgba(0.30, 0.46, 0.44, 0.75)) };
            if color.0 != fg {
                color.0 = fg;
            }
            if bg.0 != back {
                bg.0 = back;
            }
        }
    }
}
