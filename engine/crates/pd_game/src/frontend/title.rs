//! Title screen, laid out like the original's MAIN MENU: Versus, Arsenal
//! (deck builder), Options (settings overlay), Exit. Either player's
//! controls drive the single cursor (nobody's picked a side yet).

use super::state::AppState;
use super::ui::{self, Glyph, MenuArt, PixelText, RowRefs};
use bevy::prelude::*;

// One short line per row (Laurence, 2026-10-09: de-clutter; UI-DECLUTTER-PROPOSALS.md item 3).
const ROWS: [(&str, &str); 5] = [
    ("VERSUS MODE", "One to four players on one screen, with or without CPU opponents."),
    ("ONLINE", "Play a friend on another PC over Steam or a local network."),
    ("ARSENAL", "Build and save arsenals of 30 skills."),
    ("OPTIONS MODE", "Video, sound and controls."),
    ("EXIT GAME", "Close Phantom Dust."),
];

#[derive(Resource, Default)]
pub struct TitleState {
    row: usize,
    repeat: [super::input::Repeat; 2],
}

#[derive(Component)]
pub(crate) struct TitleRow(usize);

#[derive(Component)]
pub(crate) struct TitleDesc;

pub fn spawn(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<crate::Config>, mut state: ResMut<TitleState>, windows: Query<&Window>, art: Option<Res<MenuArt>>, mut images: ResMut<Assets<Image>>) {
    state.row = 0;
    state.repeat = Default::default();
    let art = MenuArt::get(&mut commands, art, &cfg.cache, &assets, &mut images);
    let (root, frame) = ui::menu_screen(&mut commands, &windows, &art, "MAIN MENU", true);
    commands.entity(root).insert(DespawnOnExit(AppState::Title));
    for (i, (label, _)) in ROWS.iter().enumerate() {
        let (panel, _) = ui::spawn_row(&mut commands, &art, ui::abs(150.0, ui::CONTENT_TOP + 8.0 + i as f32 * 54.0, 448.0, 42.0), label, "", 1.5);
        commands.entity(panel).insert(TitleRow(i));
        commands.entity(frame).add_child(panel);
    }
    let desc = commands.spawn((ui::body(ROWS[0].1, art.ttf.clone(), 22.0, ui::WHITE), ui::abs(150.0, ui::CONTENT_BOTTOM - 42.0, 1000.0, 30.0), TitleDesc)).id();
    commands.entity(frame).add_child(desc);
    // B does nothing here, so no B prompt; the key line only shows while the keyboard is in use (item 1).
    ui::prompt_bar(&mut commands, &art, frame, &[(Glyph::UpDown, "Select")], &[(Glyph::A, "Confirm")], ui::KEYS_TITLE);
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut script: Option<ResMut<super::script::MenuScript>>,
    mut state: ResMut<TitleState>,
    mut next: ResMut<NextState<AppState>>,
    mut menu: ResMut<crate::menu::MenuState>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    art: Option<Res<MenuArt>>,
    windows: Query<&Window>,
    mut frames: Query<&mut UiTransform, With<ui::DesignFrame>>,
    mut rows: Query<(&TitleRow, &RowRefs, &mut ImageNode)>,
    mut texts: Query<&mut PixelText>,
    mut desc: Query<&mut Text, With<TitleDesc>>,
    mut sfx: ResMut<super::sound::MenuSfxQueue>,
) {
    ui::fit_frames(&windows, &mut frames);
    // The settings overlay has the keys while it's open (and on the frame it toggles).
    if !(menu.open || menu.is_changed()) {
        let scripted = script.as_deref_mut().and_then(super::script::tick);
        let [ra, rb] = &mut state.repeat;
        let a = super::input::read_repeating(0, &keys, &pads, scripted, ra);
        let b = super::input::read_repeating(1, &keys, &pads, None, rb);
        sfx.0.extend(super::sound::cues_for(&a));
        sfx.0.extend(super::sound::cues_for(&b));
        if a.up || b.up {
            state.row = (state.row + ROWS.len() - 1) % ROWS.len();
        }
        if a.down || b.down {
            state.row = (state.row + 1) % ROWS.len();
        }
        if a.confirm || b.confirm {
            match state.row {
                0 => next.set(AppState::Setup),
                1 => next.set(AppState::Online),
                2 => next.set(AppState::Arsenal),
                3 => menu.open = true,
                _ => {
                    exit.write(AppExit::Success);
                }
            }
        }
    }
    let Some(art) = art else { return };
    for (row, refs, mut panel) in &mut rows {
        let on = row.0 == state.row;
        let (tint, color) = if on { (ui::PANEL_SEL, ui::YELLOW) } else { (ui::PANEL, ui::ROW_TEXT) };
        ui::set_row(&mut commands, &art, *refs, &mut panel, &mut texts, tint, ("", color), (ROWS[row.0].0, color), ("", color));
    }
    for mut t in &mut desc {
        if t.0 != ROWS[state.row].1 {
            t.0 = ROWS[state.row].1.to_string();
        }
    }
}
