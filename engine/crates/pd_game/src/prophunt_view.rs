//! Prop Hunt's presentation (`prophunt.rs` holds the rules): reads the sim, never writes it.
//!
//! - **Views** (`drive_views`): while the seeker hunts, `seats::LocalViews` is the seeker alone, so `layout_viewports`
//!   gives the seeker's camera the whole window and switches the hiders' cameras off ("The seeker's screen becomes
//!   full screen - hider's screens vanish"). Before the release and between rounds it is the match's own list again
//!   (everyone's split view). Leaving the match puts the list back as it was (`restore_views`).
//! - **Overlay** (`spawn_overlay`, `update_overlay`): on the HUD camera, above the HUD. During the countdown the
//!   seeker's view is covered black with the seconds left (they must not see where the others hide); each hider's view
//!   has a line with the countdown, the controls and whether they are disguised / frozen. During the hunt the seeker's
//!   view has a line with the round, the hunt clock, knives left, hiders left and kills. Between rounds a card covers
//!   the window: why the round ended, everyone's kills, the next seeker (or MATCH OVER).
//! - **Disguise** (`sync_disguises`): a disguised hider's fighter is hidden (`disguised`, read by `sync_visuals`) and a
//!   copy of the object's placed meshes (`props_view::PropView::nodes_of`) follows the hider, turned by how far the
//!   hider has turned since taking it. Caught: the copy goes and the fighter (lying down) shows again.
//! - The seeker's release rumble is in `rumble.rs`, the HUD clock in `hud.rs` (Bind::Timer).

use crate::frontend::state::MatchScene;
use crate::prophunt::{Phase, PropHunt, RoundEnd, KNIFE_ATTEMPTS};
use crate::seats::{seat_name, LocalViews};
use crate::stage_material::StageMaterial;
use crate::{menu, render_pose, World};
use bevy::prelude::*;

/// Whether slot `slot`'s fighter is hidden behind a disguise this frame.
pub fn disguised(m: &crate::sim::Match, slot: usize) -> bool {
    m.prophunt.as_ref().is_some_and(|ph| ph.disguise_of(slot).is_some())
}

/// The views this machine shows for the match's own list `base`: the seeker alone while hunting (if this machine
/// shows the seeker at all), else `base`.
pub fn views_for(ph: &PropHunt, base: &LocalViews, roster: &crate::combat::Roster) -> LocalViews {
    if ph.hunting() && base.slots(roster).contains(&ph.seeker) {
        LocalViews(Some(vec![ph.seeker]))
    } else {
        base.clone()
    }
}

/// The match's own `LocalViews`, kept while Prop Hunt swaps them (None = nothing swapped).
#[derive(Resource, Default)]
pub struct ViewBase(pub Option<LocalViews>);

/// Each frame of a match: the views Prop Hunt's phase wants.
pub fn drive_views(world: Res<World>, mut local: ResMut<LocalViews>, mut base: ResMut<ViewBase>) {
    let Some(ph) = &world.0.prophunt else {
        if let Some(b) = base.0.take() {
            if *local != b {
                *local = b;
            }
        }
        return;
    };
    let b = base.0.get_or_insert_with(|| local.clone()).clone();
    let want = views_for(ph, &b, &world.0.combat.roster);
    if *local != want {
        *local = want;
    }
}

/// Leaving the match: the views go back to the match's own list.
pub fn restore_views(mut local: ResMut<LocalViews>, mut base: ResMut<ViewBase>) {
    if let Some(b) = base.0.take() {
        *local = b;
    }
}

// ---- overlay ----

/// The overlay's root (the whole game area, on the HUD camera).
#[derive(Component)]
pub struct OverlayRoot;

/// What an overlay node is.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Slot `n`'s view covered black (the seeker during the countdown).
    Cover(usize),
    /// The big text in the middle of a cover.
    CoverText(usize),
    /// The line at the top of slot `n`'s view.
    Banner(usize),
    /// The between-rounds card (the whole window).
    Card,
    CardText,
}

const BLACK: Color = Color::BLACK;
const CARD_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.92);
const BANNER_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const TEXT: Color = Color::srgb(0.96, 0.86, 0.62);

/// Spawns the overlay once per Prop Hunt match (tagged `MatchScene`: the next match's clean-up removes it).
pub fn spawn_overlay(mut commands: Commands, world: Res<World>, hud: Option<Res<crate::hud::HudCamera>>, existing: Query<(), With<OverlayRoot>>) {
    if world.0.prophunt.is_none() || !existing.is_empty() {
        return;
    }
    let Some(hud) = hud else { return };
    let root = commands
        .spawn((OverlayRoot, MatchScene, UiTargetCamera(hud.0), GlobalZIndex(50), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }))
        .id();
    let text = |s: &str| (Text::new(s.to_string()), TextFont { font_size: bevy::text::FontSize::Px(20.0), ..default() }, TextColor(TEXT), TextLayout { justify: bevy::text::Justify::Center, ..default() });
    let abs = || Node { position_type: PositionType::Absolute, display: Display::None, flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() };
    for slot in 0..crate::combat::MAX_PLAYERS {
        let cover = commands.spawn((Part::Cover(slot), abs(), BackgroundColor(BLACK))).id();
        let big = commands.spawn((Part::CoverText(slot), text(""))).id();
        commands.entity(cover).add_child(big);
        let banner = commands.spawn((Part::Banner(slot), Node { justify_content: JustifyContent::Center, padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)), ..abs() }, BackgroundColor(BANNER_BG))).id();
        let line = commands.spawn((Part::Banner(slot), text(""))).id();
        commands.entity(banner).add_child(line);
        commands.entity(root).add_children(&[cover, banner]);
    }
    let card = commands.spawn((Part::Card, abs(), BackgroundColor(CARD_BG))).id();
    let card_text = commands.spawn((Part::CardText, text(""))).id();
    commands.entity(card).add_child(card_text);
    commands.entity(root).add_child(card);
}

fn clock(ticks: u32) -> String {
    let secs = ticks.div_ceil(crate::sim::HZ as u32);
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn why_text(why: RoundEnd) -> &'static str {
    match why {
        RoundEnd::TimeUp => "TIME UP - THE HIDERS SURVIVED",
        RoundEnd::AllCaught => "EVERY HIDER CAUGHT",
        RoundEnd::SeekerSpent => "THE SEEKER IS OUT OF KNIVES",
    }
}

/// The big text on the seeker's cover during the countdown.
pub fn cover_text(ph: &PropHunt) -> String {
    let left = ph.clock_ticks().unwrap_or(0);
    format!("YOU ARE THE SEEKER\n\n{}\n\nThe others are hiding. You get {KNIFE_ATTEMPTS} Psycho Knives (X).", left.div_ceil(crate::sim::HZ as u32))
}

/// The line at the top of slot `slot`'s view, if it has one now.
pub fn banner_text(ph: &PropHunt, slot: usize) -> Option<String> {
    let round = format!("ROUND {}", ph.round);
    match ph.phase {
        Phase::Countdown { left } if ph.is_hider(slot) => {
            let h = &ph.hiders[slot];
            let mut s = format!("{round}   HIDE!  {} IS THE SEEKER - RELEASED IN {}   A / X: BECOME AN OBJECT   B: FREEZE", seat_name(ph.seeker), left.div_ceil(crate::sim::HZ as u32));
            if h.disguise.is_some() {
                s.push_str("   [DISGUISED]");
            }
            if h.frozen.is_some() {
                s.push_str("   [FROZEN]");
            }
            if h.nothing_in_reach > 0 {
                s.push_str("   NOTHING IN REACH - GET CLOSER TO AN OBJECT");
            }
            Some(s)
        }
        Phase::Hunt { left } if slot == ph.seeker => Some(format!(
            "{round}   TIME {}   KNIVES {}/{KNIFE_ATTEMPTS}   HIDERS LEFT {}   KILLS {}",
            clock(left),
            ph.knives_left,
            ph.hiders_left(),
            ph.kills[slot]
        )),
        _ => None,
    }
}

/// The between-rounds card, if it shows now.
pub fn card_text(ph: &PropHunt, roster: &crate::combat::Roster) -> Option<String> {
    let (why, over) = match ph.phase {
        Phase::Card { why, .. } => (why, ph.decided()),
        Phase::Over { why } => (why, true),
        _ => return None,
    };
    let mut s = format!("ROUND {} OVER\n{}\n\n", ph.round, why_text(why));
    for i in roster.slots() {
        let role = if i == ph.seeker { "  (seeker)" } else { "" };
        s.push_str(&format!("{}   {} KILL{}{role}\n", seat_name(i), ph.kills[i], if ph.kills[i] == 1 { "" } else { "S" }));
    }
    s.push('\n');
    if over {
        s.push_str(&match ph.winner() {
            Some(w) => format!("MATCH OVER - {} WINS", seat_name(w)),
            None => "MATCH OVER - DRAW".to_string(),
        });
    } else if let Some(n) = ph.next_seeker() {
        s.push_str(&format!("NEXT SEEKER: {}", seat_name(n)));
    }
    Some(s)
}

/// Places and fills the overlay every frame (the views move when the seeker is released).
#[allow(clippy::type_complexity)]
pub fn update_overlay(
    world: Res<World>,
    area: Res<crate::display::GameArea>,
    settings: Res<menu::Settings>,
    local: Res<LocalViews>,
    mut nodes: Query<(&Part, &mut Node, Option<&mut Text>, Option<&mut TextFont>)>,
) {
    let Some(ph) = &world.0.prophunt else { return };
    if !area.ready() {
        return;
    }
    let roster = &world.0.combat.roster;
    let size = area.logical();
    let layout = local.layout(roster, settings.split);
    // Text grows with the window (designed at 720 lines high).
    let k = (size.y / 720.0).max(0.5);
    let card = card_text(ph, roster);
    for (part, mut node, text, font) in &mut nodes {
        let (show, rect, words, px) = match *part {
            Part::Cover(s) | Part::CoverText(s) => {
                let on = ph.seeker_blind() && s == ph.seeker && card.is_none();
                let view = local.view_of(s, roster).filter(|_| on);
                (view.is_some(), view.map(|v| layout.rect_f(v, size)), on.then(|| cover_text(ph)), 30.0)
            }
            Part::Banner(s) => {
                let words = banner_text(ph, s).filter(|_| card.is_none());
                let view = local.view_of(s, roster).filter(|_| words.is_some());
                let rect = view.map(|v| {
                    let r = layout.rect_f(v, size);
                    crate::views::ViewRect { pos: r.pos + Vec2::new(0.0, 8.0 * k), size: Vec2::new(r.size.x, 36.0 * k) }
                });
                (view.is_some(), rect, words, 16.0)
            }
            Part::Card | Part::CardText => (card.is_some(), Some(crate::views::ViewRect { pos: Vec2::ZERO, size }), card.clone(), 26.0),
        };
        // Text children only carry words; their boxes follow the parent's layout.
        let is_text = text.is_some();
        if let (Some(mut t), Some(w)) = (text, words) {
            if t.0 != w {
                t.0 = w;
            }
        }
        if let Some(mut f) = font {
            let want = bevy::text::FontSize::Px((px * k).round());
            if f.font_size != want {
                f.font_size = want;
            }
        }
        if is_text {
            continue;
        }
        let display = if show { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if let Some(r) = rect.filter(|_| show) {
            let set = |v: &mut Val, x: f32| {
                let want = Val::Px(x.round());
                if *v != want {
                    *v = want;
                }
            };
            set(&mut node.left, r.pos.x);
            set(&mut node.top, r.pos.y);
            set(&mut node.width, r.size.x);
            set(&mut node.height, r.size.y);
        }
    }
}

// ---- disguise ----

/// The object a hider became, drawn where the hider is (the root of copied meshes).
#[derive(Component, Debug)]
pub struct DisguiseModel {
    pub slot: usize,
    pub prop: u32,
}

/// Keeps one object copy per disguised hider, following them.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn sync_disguises(
    mut commands: Commands,
    world: Res<World>,
    fixed: Res<Time<Fixed>>,
    view: Option<Res<crate::props_view::PropView>>,
    children: Query<&Children>,
    meshes: Query<(&Mesh3d, Option<&MeshMaterial3d<StandardMaterial>>, Option<&MeshMaterial3d<StageMaterial>>, &GlobalTransform)>,
    mut models: Query<(Entity, &DisguiseModel, &mut Transform)>,
) {
    let ph = world.0.prophunt.as_ref();
    let want = |slot: usize| ph.and_then(|p| p.disguise_of(slot));
    let mut have = [false; crate::combat::MAX_PLAYERS];
    for (e, m, mut t) in &mut models {
        match want(m.slot) {
            Some(d) if d.prop == m.prop && !have[m.slot] => {
                have[m.slot] = true;
                *t = disguise_transform(&world, &fixed, m.slot, d);
            }
            _ => commands.entity(e).despawn(),
        }
    }
    let Some(view) = view else { return };
    for slot in 0..crate::combat::MAX_PLAYERS {
        let Some(d) = want(slot).filter(|_| !have[slot]) else { continue };
        // Every mesh of the object's placed nodes, in stage space (their placement baked in).
        let mut pieces = Vec::new();
        for &node in view.nodes_of(d.prop) {
            let mut stack = vec![node];
            while let Some(e) = stack.pop() {
                if let Ok(piece) = meshes.get(e) {
                    pieces.push((piece.0.clone(), piece.1.cloned(), piece.2.cloned(), piece.3.compute_transform()));
                }
                if let Ok(c) = children.get(e) {
                    stack.extend(c.iter());
                }
            }
        }
        if pieces.is_empty() {
            continue; // the stage hasn't loaded yet: try again next frame
        }
        let root = commands.spawn((DisguiseModel { slot, prop: d.prop }, disguise_transform(&world, &fixed, slot, d), Visibility::Visible, MatchScene)).id();
        for (mesh, standard, stage, local) in pieces {
            let mut e = commands.spawn((mesh, local, ChildOf(root)));
            if let Some(m) = standard {
                e.insert(m);
            }
            if let Some(m) = stage {
                e.insert(m);
            }
        }
    }
}

/// Where the copy of the disguise sits: the object's base centre on the hider's feet, turned by how far the hider has
/// turned since taking it (so it first appears exactly as the object stands).
pub fn disguise_transform(world: &World, fixed: &Time<Fixed>, slot: usize, d: crate::prophunt::Disguise) -> Transform {
    let (pos, yaw) = render_pose(world, fixed, slot);
    let anchor = world.0.combat.props.iter().find(|p| p.index == d.prop).map_or(Vec3::ZERO, |p| Vec3::new((p.min.x + p.max.x) * 0.5, p.min.y, (p.min.z + p.max.z) * 0.5));
    Transform::from_matrix(Mat4::from_translation(pos) * Mat4::from_rotation_y(yaw - d.yaw) * Mat4::from_translation(-anchor))
}

pub fn plugin(app: &mut App) {
    use crate::frontend::state::AppState;
    app.init_resource::<ViewBase>()
        .add_systems(Update, (drive_views, spawn_overlay, update_overlay, sync_disguises).chain().run_if(in_state(AppState::Match)))
        .add_systems(OnExit(AppState::Match), restore_views);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Combat, Roster, MAX_PLAYERS};
    use crate::prophunt::{Config, Disguise, SEEKER_COUNTDOWN_TICKS};
    use crate::props::Prop;
    use crate::sim::Match;

    fn prop_hunt(n: usize) -> Match {
        let mut c = Combat::with_roster(Vec::new(), Roster::free_for_all(n), std::array::from_fn(|_| Vec::new()));
        c.set_rules(crate::frontend::modes::MODES[1].rules());
        c.props = vec![Prop::new(7, 2, 3, Vec3::new(10.0, 0.0, 10.0), Vec3::new(20.0, 20.0, 20.0), true, true)];
        let spawns: Vec<Vec3> = (0..n).map(|i| Vec3::new(i as f32 * 100.0, 0.0, 0.0)).collect();
        let mut m = Match::with_spawns(&spawns, c);
        m.start_prop_hunt(Config::default());
        m
    }

    fn release(m: &mut Match) {
        m.prophunt.as_mut().unwrap().phase = Phase::Hunt { left: 1000 };
    }

    #[test]
    fn the_seeker_alone_fills_the_window_while_hunting() {
        let mut m = prop_hunt(3);
        let base = LocalViews(None);
        let roster = m.combat.roster;
        assert_eq!(views_for(m.prophunt.as_ref().unwrap(), &base, &roster), base, "countdown: everyone's split view");
        release(&mut m);
        assert_eq!(views_for(m.prophunt.as_ref().unwrap(), &base, &roster), LocalViews(Some(vec![0])));
        assert_eq!(LocalViews(Some(vec![0])).layout(&roster, menu::Split::SideBySide).kind, crate::views::Kind::Full);
        // A machine that doesn't show the seeker keeps its own views (never happens locally; a guard).
        assert_eq!(views_for(m.prophunt.as_ref().unwrap(), &LocalViews(Some(vec![1])), &roster), LocalViews(Some(vec![1])));
        // Between rounds: back to everyone.
        m.prophunt.as_mut().unwrap().phase = Phase::Card { left: 10, why: RoundEnd::TimeUp };
        assert_eq!(views_for(m.prophunt.as_ref().unwrap(), &base, &roster), base);
    }

    /// The real systems: views switch on release, come back between rounds, and leaving the match restores them.
    #[test]
    fn drive_views_switches_on_release_and_restores_on_exit() {
        let mut app = App::new();
        app.insert_resource(World(prop_hunt(2))).init_resource::<LocalViews>().init_resource::<ViewBase>().add_systems(Update, drive_views);
        app.update();
        assert_eq!(*app.world().resource::<LocalViews>(), LocalViews(None));
        release(&mut app.world_mut().resource_mut::<World>().0);
        app.update();
        assert_eq!(*app.world().resource::<LocalViews>(), LocalViews(Some(vec![0])), "full screen for the seeker");
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().phase = Phase::Card { left: 10, why: RoundEnd::AllCaught };
        app.update();
        assert_eq!(*app.world().resource::<LocalViews>(), LocalViews(None));
        release(&mut app.world_mut().resource_mut::<World>().0);
        app.update();
        app.world_mut().run_system_once(restore_views).unwrap();
        assert_eq!(*app.world().resource::<LocalViews>(), LocalViews(None), "the match's own list again");
        // A normal match never touches them.
        let mut m = prop_hunt(2);
        m.prophunt = None;
        app.insert_resource(World(m));
        app.insert_resource(LocalViews(Some(vec![1])));
        app.update();
        assert_eq!(*app.world().resource::<LocalViews>(), LocalViews(Some(vec![1])));
    }

    use bevy::ecs::system::RunSystemOnce;

    fn overlay_app(m: Match) -> App {
        let mut app = App::new();
        let path = std::env::temp_dir().join(format!("pd_ph_overlay_{}.json", std::process::id()));
        let mut settings = menu::Settings::load(path);
        settings.disable_saving();
        let cam = app.world_mut().spawn_empty().id();
        app.insert_resource(World(m))
            .insert_resource(settings)
            .insert_resource(crate::hud::HudCamera(cam))
            .insert_resource(crate::display::GameArea::from_window(UVec2::new(1280, 720), 1.0, crate::display::Aspect::Auto))
            .init_resource::<LocalViews>()
            .init_resource::<ViewBase>()
            .add_systems(Update, (drive_views, spawn_overlay, update_overlay).chain());
        app.update();
        app.update();
        app
    }

    /// (part, shown, left/top/width/height, text) for every overlay node.
    fn parts(app: &mut App) -> Vec<(Part, bool, [Val; 4], Option<String>)> {
        let mut q = app.world_mut().query::<(&Part, &Node, Option<&Text>)>();
        q.iter(app.world()).map(|(p, n, t)| (*p, n.display != Display::None, [n.left, n.top, n.width, n.height], t.map(|t| t.0.clone()))).collect()
    }

    fn box_of(app: &mut App, part: Part) -> Option<[Val; 4]> {
        parts(app).into_iter().find(|p| p.0 == part && p.3.is_none() && p.1).map(|p| p.2)
    }

    fn text_of(app: &mut App, part: Part) -> String {
        parts(app).into_iter().find_map(|p| if p.0 == part { p.3.filter(|t| !t.is_empty()) } else { None }).unwrap_or_default()
    }

    #[test]
    fn the_countdown_covers_the_seekers_view_black_and_tells_the_hiders_to_hide() {
        let mut app = overlay_app(prop_hunt(2));
        // Side by side at 1280x720: the seeker (slot 0) is the left half, covered.
        assert_eq!(box_of(&mut app, Part::Cover(0)), Some([Val::Px(0.0), Val::Px(0.0), Val::Px(640.0), Val::Px(720.0)]));
        assert!(text_of(&mut app, Part::CoverText(0)).contains("YOU ARE THE SEEKER\n\n20"), "{}", text_of(&mut app, Part::CoverText(0)));
        assert_eq!(box_of(&mut app, Part::Cover(1)), None, "the hider sees");
        let banner = text_of(&mut app, Part::Banner(1));
        assert!(banner.contains("HIDE!") && banner.contains("A / X: BECOME AN OBJECT") && banner.contains("B: FREEZE"), "{banner}");
        assert_eq!(box_of(&mut app, Part::Card), None);
        // Disguised and frozen show on the hider's line.
        {
            let mut w = app.world_mut().resource_mut::<World>();
            let h = &mut w.0.prophunt.as_mut().unwrap().hiders[1];
            h.disguise = Some(Disguise { prop: 7, yaw: 0.0 });
            h.frozen = Some(crate::prophunt::Frozen { x: 0.0, z: 0.0, yaw: 0.0 });
        }
        app.update();
        let banner = text_of(&mut app, Part::Banner(1));
        assert!(banner.contains("[DISGUISED]") && banner.contains("[FROZEN]"), "{banner}");
        // The seconds count down.
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().phase = Phase::Countdown { left: SEEKER_COUNTDOWN_TICKS / 2 };
        app.update();
        assert!(text_of(&mut app, Part::CoverText(0)).contains("\n10\n"));
    }

    #[test]
    fn released_the_cover_goes_and_the_seekers_line_fills_the_window_width() {
        let mut m = prop_hunt(3);
        release(&mut m);
        m.prophunt.as_mut().unwrap().knives_left = 7;
        let mut app = overlay_app(m);
        assert_eq!(box_of(&mut app, Part::Cover(0)), None);
        let line = text_of(&mut app, Part::Banner(0));
        assert!(line.contains("KNIVES 7/10") && line.contains("HIDERS LEFT 2") && line.contains("TIME 0:17"), "{line}");
        let b = box_of(&mut app, Part::Banner(0)).unwrap();
        assert_eq!(b[2], Val::Px(1280.0), "the seeker's view is the whole window");
        assert_eq!(box_of(&mut app, Part::Banner(1)), None, "the hiders' views are gone");
    }

    #[test]
    fn between_rounds_a_card_shows_the_kills_and_the_next_seeker() {
        let mut m = prop_hunt(3);
        {
            let ph = m.prophunt.as_mut().unwrap();
            ph.kills = [2, 0, 1, 0];
            ph.phase = Phase::Card { left: 100, why: RoundEnd::AllCaught };
        }
        let mut app = overlay_app(m);
        assert_eq!(box_of(&mut app, Part::Card), Some([Val::Px(0.0), Val::Px(0.0), Val::Px(1280.0), Val::Px(720.0)]));
        let card = text_of(&mut app, Part::CardText);
        assert!(card.contains("ROUND 1 OVER") && card.contains("EVERY HIDER CAUGHT"), "{card}");
        assert!(card.contains("PLAYER 1   2 KILLS") && card.contains("PLAYER 3   1 KILL\n"), "{card}");
        assert!(card.contains("NEXT SEEKER: PLAYER 2"), "{card}");
        assert_eq!(box_of(&mut app, Part::Cover(0)), None);
        // The end: the winner (or a draw).
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().phase = Phase::Over { why: RoundEnd::TimeUp };
        app.update();
        assert!(text_of(&mut app, Part::CardText).contains("MATCH OVER - PLAYER 1 WINS"));
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().kills = [1, 1, 0, 0];
        app.update();
        assert!(text_of(&mut app, Part::CardText).contains("MATCH OVER - DRAW"));
    }

    #[test]
    fn a_disguise_draws_a_copy_of_the_object_at_the_hider_and_goes_when_caught() {
        let mut app = App::new();
        let mut m = prop_hunt(2);
        m.players[1].pos = Vec3::new(100.0, 0.0, 0.0);
        m.players[1].prev_pos = m.players[1].pos;
        m.players[1].yaw = 0.5;
        m.players[1].prev_yaw = 0.5;
        app.insert_resource(World(m)).init_resource::<Time<Fixed>>().add_systems(Update, sync_disguises);
        // The object's placed node: one mesh piece in stage space.
        let node = app.world_mut().spawn((Mesh3d(Handle::default()), MeshMaterial3d::<StandardMaterial>(Handle::default()), GlobalTransform::from_translation(Vec3::new(15.0, 10.0, 15.0)))).id();
        app.insert_resource(crate::props_view::PropView::with_nodes([(7, vec![node])]));
        app.update();
        let count = |app: &mut App| app.world_mut().query::<&DisguiseModel>().iter(app.world()).count();
        assert_eq!(count(&mut app), 0, "not disguised yet");
        assert!(!disguised(&app.world().resource::<World>().0, 1));
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().hiders[1].disguise = Some(Disguise { prop: 7, yaw: 0.5 });
        app.update();
        app.update();
        assert!(disguised(&app.world().resource::<World>().0, 1), "the fighter is hidden (sync_visuals reads this)");
        let mut q = app.world_mut().query::<(&DisguiseModel, &Transform, &Children)>();
        let (d, t, kids) = q.single(app.world()).unwrap();
        assert_eq!((d.slot, d.prop, kids.len()), (1, 7, 1));
        // The object's base centre (15, 0, 15) lands on the hider's feet, unturned (the hider hasn't turned since).
        let base = t.transform_point(Vec3::new(15.0, 0.0, 15.0));
        assert!(base.distance(Vec3::new(100.0, 0.0, 0.0)) < 1e-3, "{base:?}");
        // The hider turns: the copy turns with them.
        {
            let mut w = app.world_mut().resource_mut::<World>();
            w.0.players[1].yaw = 0.5 + std::f32::consts::FRAC_PI_2;
            w.0.players[1].prev_yaw = w.0.players[1].yaw;
        }
        app.update();
        let mut q = app.world_mut().query::<(&DisguiseModel, &Transform)>();
        let t = *q.single(app.world()).unwrap().1;
        let ahead = t.transform_point(Vec3::new(15.0, 0.0, 25.0)) - Vec3::new(100.0, 0.0, 0.0);
        assert!((ahead - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-3, "a quarter turn: {ahead:?}");
        // Caught: the copy goes, the fighter shows again.
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().hiders[1].caught = true;
        app.update();
        app.update();
        assert_eq!(count(&mut app), 0);
        assert!(!disguised(&app.world().resource::<World>().0, 1));
        // Any number of frames: still one model per disguised hider.
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().hiders[1] = crate::prophunt::Hider { disguise: Some(Disguise { prop: 7, yaw: 0.0 }), ..Default::default() };
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(count(&mut app), 1);
        let _ = MAX_PLAYERS;
    }
}
