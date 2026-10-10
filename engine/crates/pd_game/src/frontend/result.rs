//! Result screen: shown after a match ends (`main.rs::check_match_end`). The
//! 3D scene stays up (frozen -- `step_sim` only runs `in_state(Match)`)
//! behind this opaque screen. Same look as the other menus: the headline in
//! the pixel font, one card per player (portrait, place, wins), the winner's
//! panel red, and the fun stats for every player.
//!
//! The outcome is read off the sim (`Combat::round_outcome`, `team_wins`,
//! `placements`): a free-for-all names the winning player and ranks everyone
//! 1st-4th; a teams match names the winning team (both members win); a
//! drawn single battle says DRAW.

use super::state::{AppState, MatchStats};
use super::ui::{self, Glyph, MenuArt};
use crate::combat::{Combat, Outcome, MAX_PLAYERS};
use crate::seats::{seat_colour, seat_name, MatchPlayers};
use bevy::prelude::*;

/// Prop Hunt's result: the winner by kills (a tie at the top is a draw), every player placed by kills, `wins` = kills.
pub fn prop_hunt_results(ph: &crate::prophunt::PropHunt, roster: &crate::combat::Roster) -> Results {
    let headline = ph.winner().map_or(Headline::Draw, Headline::Player);
    let places = ph.places();
    let standings = roster.slots().map(|i| Standing { slot: i, team: roster.team[i], place: places[i], wins: ph.kills[i] }).collect();
    Results { headline, teams: false, standings }
}

/// What the headline says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Headline {
    Draw,
    /// Free for all: this player won.
    Player(usize),
    /// Teams: this team won.
    Team(u8),
}

/// One player's line of the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Standing {
    pub slot: usize,
    pub team: u8,
    /// 1 = best; 0 = no place (a draw).
    pub place: u8,
    /// Round wins: the player's own in a free for all, their team's in teams.
    pub wins: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Results {
    pub headline: Headline,
    /// At least one team has more than one member.
    pub teams: bool,
    /// The players in slot order.
    pub standings: Vec<Standing>,
}

/// How the match that just ended reads, from the sim's own state. Without a
/// decided round (the result screen opened directly) the team with the most
/// round wins is the winner, a tie is a draw.
pub fn results(c: &Combat) -> Results {
    let r = &c.roster;
    let teams = r.slots().any(|i| r.teammates_of(i).next().is_some());
    let outcome = c.round_outcome().unwrap_or_else(|| {
        let score = |i: usize| c.team_wins(r.team[i]);
        let best = r.slots().map(score).max().unwrap_or(0);
        let lead = r.slots().find(|&i| score(i) == best);
        let tied = lead.is_some_and(|l| r.slots().any(|i| score(i) == best && r.team[i] != r.team[l]));
        match lead {
            Some(slot) if best > 0 && !tied => Outcome::Winner { slot, team: r.team[slot] },
            _ => Outcome::Draw,
        }
    });
    let headline = match outcome {
        Outcome::Draw => Headline::Draw,
        Outcome::Winner { team, .. } if teams => Headline::Team(team),
        Outcome::Winner { slot, .. } => Headline::Player(slot),
    };
    let wins = |i: usize| if teams { c.team_wins(r.team[i]) } else { c.wins[i] };
    let finish = c.placements();
    // Free for all: most round wins, then who lasted longest in the last round.
    let key = |i: usize| (std::cmp::Reverse(wins(i)), finish[i]);
    let standings = r
        .slots()
        .map(|i| {
            let place = match outcome {
                Outcome::Draw => 0,
                Outcome::Winner { team, .. } if teams => 1 + u8::from(r.team[i] != team),
                Outcome::Winner { .. } => 1 + r.slots().filter(|&j| key(j) < key(i)).count() as u8,
            };
            Standing { slot: i, team: r.team[i], place, wins: wins(i) }
        })
        .collect();
    Results { headline, teams, standings }
}

fn ordinal(place: u8) -> &'static str {
    match place {
        1 => "1ST",
        2 => "2ND",
        3 => "3RD",
        4 => "4TH",
        _ => "-",
    }
}

fn headline_text(h: Headline) -> String {
    match h {
        Headline::Draw => "DRAW".into(),
        Headline::Player(i) => format!("{} WINS", seat_name(i)),
        Headline::Team(t) => format!("TEAM {} WINS", t + 1),
    }
}

/// One "fun stat" row: a label and each player's value, formatted from
/// `MatchStats` (counted render-side from sim state, `frontend/state.rs`
/// `observe`/`track_stats` -- no progression, nothing earned, just numbers
/// from the match just played).
fn stat_rows(stats: &MatchStats, catalog: &super::state::SkillCatalog, slots: &[usize]) -> Vec<(&'static str, Vec<String>)> {
    let skill_name = |id: i16| if id == crate::combat::AURA_PARTICLE { "Aura Particle".to_string() } else { catalog.skills.iter().find(|s| s.id == id).map_or_else(|| format!("#{id}"), |s| s.name.clone()) };
    let each = |f: &dyn Fn(usize) -> String| slots.iter().map(|&i| f(i)).collect::<Vec<_>>();
    vec![
        ("DAMAGE DEALT", each(&|i| stats.damage_dealt[i].to_string())),
        ("SKILLS USED", each(&|i| stats.skills_used[i].to_string())),
        ("KNOCK-DOWNS", each(&|i| stats.knockdowns[i].to_string())),
        ("LONGEST COMBO", each(&|i| stats.longest_combo[i].to_string())),
        ("FAVOURITE SKILL", each(&|i| stats.favourite(i).map_or("-".to_string(), skill_name))),
    ]
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    mut commands: Commands,
    assets: Res<AssetServer>,
    cfg: Res<crate::Config>,
    world: Res<crate::World>,
    players: Option<Res<MatchPlayers>>,
    data: Res<super::state::GameData>,
    stats: Res<MatchStats>,
    catalog: Res<super::state::SkillCatalog>,
    windows: Query<&Window>,
    art: Option<Res<MenuArt>>,
    mut images: ResMut<Assets<Image>>,
) {
    let art = MenuArt::get(&mut commands, art, &cfg.cache, &assets, &mut images);
    let combat = &world.0.combat;
    // Prop Hunt ranks by kills (scored as the seeker), every other match by round wins.
    let (res, score_word) = match &world.0.prophunt {
        Some(ph) => (prop_hunt_results(ph, &combat.roster), "KILLS"),
        None => (results(combat), "WINS"),
    };
    let (root, frame) = ui::menu_screen(&mut commands, &windows, &art, "BATTLE RESULT", true);
    commands.entity(root).insert(DespawnOnExit(AppState::Result));
    let scale = 3.0;
    let headline = headline_text(res.headline);
    let h = ui::spawn_pixel(&mut commands, &art, &headline, ui::YELLOW, scale);
    let hw = art.text_width(&headline, scale);
    commands.entity(h).insert(Node { position_type: PositionType::Absolute, left: px((ui::DESIGN.x - hw) / 2.0), top: px(ui::CONTENT_TOP + 14.0), height: px(48.0), ..default() });
    let mut kids = vec![h];
    let mut names = Vec::new();

    // One card per player.
    let n = res.standings.len().max(1);
    let (cw, gap) = if n >= 3 { (290.0, 16.0) } else { (330.0, 120.0) };
    let x0 = (ui::DESIGN.x - (n as f32 * cw + (n as f32 - 1.0) * gap)) / 2.0;
    let (pw, ph) = (cw * 0.81, cw * 0.81 * 192.0 / 268.0);
    let y = ui::CONTENT_TOP + 80.0;
    for (k, st) in res.standings.iter().enumerate() {
        let x = x0 + k as f32 * (cw + gap);
        let px0 = x + (cw - pw) / 2.0;
        let won = st.place == 1 && res.headline != Headline::Draw;
        kids.push(commands.spawn((ui::abs(px0 - 2.0, y - 2.0, pw + 4.0, ph + 4.0), BackgroundColor(Color::srgba(0.45, 0.85, 0.72, 0.55)))).id());
        kids.push(commands.spawn((ui::abs(px0, y, pw, ph), BackgroundColor(Color::srgb(0.02, 0.05, 0.05)))).id());
        let id = players.as_ref().and_then(|p| p.0.get(st.slot)).map_or("", |p| p.char.as_str());
        if let Some(img) = art.portrait(id) {
            let dim = if res.headline != Headline::Draw && !won { Color::srgb(0.45, 0.45, 0.45) } else { Color::WHITE };
            kids.push(commands.spawn((ui::abs(px0, y, pw, ph), ImageNode { color: dim, ..img })).id());
        }
        let name = data.characters.iter().find(|c| c.id == id).map_or(String::new(), |c| c.label.to_uppercase());
        let (tint, color) = if won { (ui::PANEL_SEL, ui::YELLOW) } else { (ui::PANEL, ui::ROW_TEXT) };
        // No "-" tab on a draw; the character's name heads its stats column
        // instead of a line under the card (Laurence, 2026-10-09: de-clutter, item 7).
        let tab = if res.headline == Headline::Draw { "" } else { ordinal(st.place) };
        let (panel, _) = ui::spawn_row_with(&mut commands, &art, ui::abs(x, y + ph + 14.0, cw, 36.0), (tab, ui::YELLOW), &seat_name(st.slot), &format!("{score_word} {}", st.wins), 1.0, tint, color);
        kids.push(panel);
        if res.teams {
            kids.push(commands.spawn((ui::body(&format!("TEAM {}", st.team + 1), art.ttf.clone(), 17.0, seat_colour(st.slot)), ui::abs(x + 4.0, y + ph + 54.0, cw, 22.0))).id());
        }
        names.push(name);
    }

    // Fun stats: no progression, nothing earned -- just what happened this match.
    let slots: Vec<usize> = res.standings.iter().map(|s| s.slot).collect();
    let header = ui::spawn_pixel(&mut commands, &art, "FUN STATS", ui::TEAL, 1.0);
    commands.entity(header).insert(Node { position_type: PositionType::Absolute, left: px(150.0), top: px(478.0), height: px(16.0), ..default() });
    kids.push(header);
    let (label_w, col_w) = (230.0, 190.0);
    for (c, &slot) in slots.iter().enumerate() {
        let who = names.get(c).filter(|n: &&String| !n.is_empty()).cloned().unwrap_or_else(|| seat_name(slot));
        let t = ui::spawn_pixel(&mut commands, &art, &who, seat_colour(slot), 0.75);
        commands.entity(t).insert(Node { position_type: PositionType::Absolute, left: px(150.0 + label_w + c as f32 * col_w + 6.0), top: px(480.0), height: px(12.0), ..default() });
        kids.push(t);
    }
    for (r, (label, values)) in stat_rows(&stats, &catalog, &slots).iter().enumerate() {
        let ry = 496.0 + r as f32 * 22.0;
        let (panel, _) = ui::spawn_row(&mut commands, &art, ui::abs(150.0, ry, label_w, 20.0), label, "", 0.75);
        kids.push(panel);
        kids.push(commands.spawn((ui::abs(150.0 + label_w + 2.0, ry, slots.len() as f32 * col_w - 2.0, 20.0), BackgroundColor(Color::srgba(0.30, 0.46, 0.44, 0.55)))).id());
        for (c, v) in values.iter().enumerate() {
            let text = art.fit(&v.to_uppercase(), col_w - 14.0, 0.75);
            let t = ui::spawn_pixel(&mut commands, &art, &text, ui::ROW_TEXT, 0.75);
            commands.entity(t).insert(Node { position_type: PositionType::Absolute, left: px(150.0 + label_w + c as f32 * col_w + 6.0), top: px(ry + 4.0), height: px(12.0), ..default() });
            kids.push(t);
        }
    }
    commands.entity(frame).add_children(&kids);
    // B continues too, so only the one prompt (item 7).
    ui::prompt_bar(&mut commands, &art, frame, &[], &[(Glyph::A, "Continue")], ui::KEYS_RESULT);
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut next: ResMut<NextState<AppState>>,
    mut script: Option<ResMut<super::script::MenuScript>>,
    mut setup: ResMut<super::setup::SetupState>,
    menu: Res<crate::menu::MenuState>,
    windows: Query<&Window>,
    mut frames: Query<&mut UiTransform, With<ui::DesignFrame>>,
    mut sfx: ResMut<super::sound::MenuSfxQueue>,
    online: Res<crate::online_match::OnlineGate>,
) {
    ui::fit_frames(&windows, &mut frames);
    // The settings overlay has the keys while it's open (and on the frame it toggles).
    if menu.open || menu.is_changed() {
        return;
    }
    let scripted = script.as_deref_mut().and_then(super::script::tick);
    let mut leave = false;
    // Any seat's pad (or the keyboard) continues.
    for i in 0..MAX_PLAYERS {
        let a = super::input::read(i, &keys, &pads, if i == 0 { scripted } else { None });
        sfx.0.extend(super::sound::cues_for(&a));
        leave |= a.confirm || a.cancel;
    }
    if leave {
        if online.was_online() {
            // An online match: back to the Online screen (host / join again),
            // not the local versus setup.
            next.set(AppState::Online);
        } else {
            // Back to setup with the same seats, for a rematch.
            setup.mark_rematch();
            next.set(AppState::Setup);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Roster;

    fn game(roster: Roster) -> Combat {
        Combat::with_roster(Vec::new(), roster, std::array::from_fn(|_| vec![crate::combat::AURA_PARTICLE; 30]))
    }

    #[test]
    fn a_free_for_all_names_the_player_and_ranks_everyone() {
        let mut c = game(Roster::free_for_all(4));
        c.round_ticks = 300;
        c.change_health(3, -999);
        c.round_ticks = 600;
        c.change_health(1, -999);
        c.round_ticks = 900;
        c.change_health(2, -999);
        let r = results(&c);
        assert_eq!(r.headline, Headline::Player(0));
        assert!(!r.teams);
        let place: Vec<u8> = r.standings.iter().map(|s| s.place).collect();
        assert_eq!(place, [1, 3, 2, 4], "slot 0 survived; slot 2 lasted longest of the rest; slot 3 fell first");
        assert_eq!(r.standings[0].wins, 1);
        assert_eq!(r.standings[1].wins, 0);
    }

    #[test]
    fn a_two_v_two_names_the_winning_team_and_both_members_win() {
        let mut c = game(Roster::with_teams(&[0, 0, 1, 1]));
        c.round_ticks = 100;
        c.change_health(0, -999);
        c.round_ticks = 400;
        c.change_health(2, -999);
        c.round_ticks = 500;
        c.change_health(3, -999);
        let r = results(&c);
        assert!(r.teams);
        assert_eq!(r.headline, Headline::Team(0), "team 1 (ids start at 0) wins with slot 1 still standing");
        let place: Vec<u8> = r.standings.iter().map(|s| s.place).collect();
        assert_eq!(place, [1, 1, 2, 2], "the partner of a knocked-out player still wins with the team");
        assert!(r.standings.iter().all(|s| s.wins == if s.team == 0 { 1 } else { 0 }), "teams show their team's wins");
        assert_eq!(headline_text(r.headline), "TEAM 1 WINS");
    }

    #[test]
    fn a_wipe_out_is_a_draw_and_places_nobody() {
        let mut c = game(Roster::duel());
        c.round_ticks = 100;
        c.change_health(0, -999);
        c.round_ticks = 130;
        c.change_health(1, -999);
        let r = results(&c);
        assert_eq!(r.headline, Headline::Draw);
        assert!(r.standings.iter().all(|s| s.place == 0));
        assert_eq!(headline_text(r.headline), "DRAW");
    }

    #[test]
    fn a_time_up_tie_is_a_draw_and_a_time_up_lead_wins() {
        let mut c = game(Roster::duel());
        c.time_limit = 600;
        c.round_ticks = crate::combat::COUNTDOWN_TICKS + 600;
        // Run the sim's own time-up resolution: step with nobody doing anything.
        let col = crate::combat::testkit::floor();
        let mut players = [crate::sim::Player::new(glam::Vec3::ZERO, 0.0), crate::sim::Player::new(glam::Vec3::new(0.0, 0.0, 10.0), 3.14)];
        c.step(&mut players, [0u8; 2], &col);
        assert!(c.time_up);
        assert_eq!(results(&c).headline, Headline::Draw, "equal Health at time-up scores nobody");
    }

    #[test]
    fn a_player_who_wins_a_longer_match_is_named_from_the_round_wins() {
        let mut c = game(Roster::free_for_all(3));
        c.wins = [0, 2, 1, 0];
        // No decided round (the screen opened on its own): the most wins decide.
        let r = results(&c);
        assert_eq!(r.headline, Headline::Player(1));
        assert_eq!(r.standings.iter().map(|s| s.place).collect::<Vec<_>>(), [3, 1, 2]);
        c.wins = [1, 1, 0, 0];
        assert_eq!(results(&c).headline, Headline::Draw, "a tie at the top is a draw");
    }
}
