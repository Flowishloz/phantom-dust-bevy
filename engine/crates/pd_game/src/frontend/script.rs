//! `--menu-script up,down,confirm,...`: a hidden switch that clicks through
//! the menus by itself, so a screenshot of "a match started through the
//! menus" is repeatable without a human at the controls (proof, not a
//! feature). One token is applied every `TOKEN_SPACING` frames -- spaced out
//! so each screen's `OnEnter` has time to spawn before the next press lands
//! on it.
//!
//! Tokens, comma separated:
//! - `up down left right confirm cancel start x y lb rb` (aliases `a` = confirm, `b` =
//!   cancel; `start` = the pad's Start button; `x y lb rb` = the pad's X, Y and shoulder buttons: the setup's CPU seats): a button press on a virtual pad. Unprefixed = pad 1 (the
//!   keyboard seat); `p2:down` is the second pad, up to `p4:`. On the setup
//!   screen a virtual pad takes the next free seat with its first press, like
//!   a real pad (`p1:a,p2:a,p3:a` seats three players);
//! - `wait` / `wait:N` waits the usual spacing / N frames;
//! - `shot:<name>` saves `<prefix><name>.png` next to the `--screenshot` file
//!   (prefix = the file name up to its first `_`, e.g. `s4p_`);
//! - `mshot@<tick>:<name>` the same during a match once the sim has reached
//!   that tick (match tokens are only taken by `match_tokens`, so a script
//!   can queue them before the result screen's tokens);
//! - `quit` exits the game a moment later (after the last shot is written).
//!
//! Also here, `PD_TEST_KO=<tick>:<slots>[;<tick>:<slots>]`: during a match,
//! knocks the 1-based slots out (`1,2` or `3`) once the sim reaches the tick
//! -- how a scripted run finishes a match without anyone fighting.
//!
//! The arsenal builder reads its own richer vocabulary through
//! [`tick_token`] (`a b x y lb rb start bs t:<text> shot:<name>`).

use super::input::MenuAxis;
use bevy::prelude::*;
use std::collections::VecDeque;

const TOKEN_SPACING: u32 = 12;
/// Frames the game keeps running after `quit` so the last screenshot lands.
const QUIT_DELAY: u32 = 45;

#[derive(Resource, Default)]
pub struct MenuScript {
    tokens: VecDeque<String>,
    cooldown: u32,
    /// Screenshots asked for (names), written by [`drive`].
    shots: Vec<String>,
    /// `burst:<name>:<n>`: a screenshot on each of the next `n` frames, named
    /// `<name>_<k>` (frame-exact proof of a one-frame flicker).
    burst: Option<(String, u32, u32)>,
    quit_in: Option<u32>,
}

impl MenuScript {
    pub fn new(spec: &str) -> Self {
        MenuScript { tokens: spec.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect(), ..default() }
    }
}

/// One virtual pad's press, split out of a token: which pad (0-based) and the
/// action name (`up`, `confirm`, ...).
fn split_pad(token: &str) -> (usize, &str) {
    if let Some((p, rest)) = token.split_once(':') {
        if let Some(n) = p.strip_prefix('p').and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=4).contains(n)) {
            return (n - 1, rest);
        }
    }
    (0, token)
}

fn axis_of(action: &str) -> Option<MenuAxis> {
    let mut a = MenuAxis::default();
    match action {
        "up" => a.up = true,
        "down" => a.down = true,
        "left" => a.left = true,
        "right" => a.right = true,
        "confirm" | "a" => a.confirm = true,
        "cancel" | "b" => a.cancel = true,
        "start" => a.start = true,
        "x" => a.x = true,
        "y" => a.y = true,
        "lb" => a.lb = true,
        "rb" => a.rb = true,
        _ => return None,
    }
    Some(a)
}

/// Tokens that act on the match rather than on a menu.
fn is_match_token(token: &str) -> bool {
    token.starts_with("mshot@")
}

/// The next press this frame, if the script has one ready: (virtual pad
/// 0..3, what it pressed). `shot:`, `wait` and `quit` are acted on here (or
/// queued for [`drive`]) and give nothing back. Match tokens wait their turn.
pub fn tick_pad(script: &mut MenuScript) -> Option<(usize, MenuAxis)> {
    if script.cooldown > 0 {
        script.cooldown -= 1;
        return None;
    }
    if script.tokens.front().is_some_and(|t| is_match_token(t)) {
        return None;
    }
    let token = script.tokens.pop_front()?;
    if let Some(rest) = token.strip_prefix("burst:") {
        // No cooldown: the next token (the press being studied) lands next frame.
        if let Some((name, n)) = rest.rsplit_once(':').and_then(|(a, b)| Some((a, b.parse::<u32>().ok()?))) {
            script.burst = Some((name.to_string(), n, 0));
        }
        return None;
    }
    script.cooldown = TOKEN_SPACING;
    if let Some(name) = token.strip_prefix("shot:") {
        script.shots.push(name.to_string());
        return None;
    }
    if token == "quit" {
        script.quit_in = Some(QUIT_DELAY);
        return None;
    }
    if let Some(frames) = token.strip_prefix("wait:").and_then(|n| n.parse::<u32>().ok()) {
        script.cooldown = frames;
        return None;
    }
    let (pad, action) = split_pad(&token);
    axis_of(action).map(|a| (pad, a))
}

/// This frame's scripted pulse for the single-cursor screens (title, result):
/// any virtual pad's press counts.
pub fn tick(script: &mut MenuScript) -> Option<MenuAxis> {
    tick_pad(script).map(|(_, a)| a)
}

/// The raw token itself (same spacing and cooldown as [`tick`]), for screens
/// whose scripted vocabulary is richer than a [`MenuAxis`] -- the arsenal
/// builder reads `a b x y lb rb start bs t:<text> shot:<name>` this way
/// (`frontend::builder`). Do not mix with [`tick`] on one screen.
pub fn tick_token(script: &mut MenuScript) -> Option<String> {
    if script.cooldown > 0 {
        script.cooldown -= 1;
        return None;
    }
    let token = script.tokens.pop_front()?;
    script.cooldown = TOKEN_SPACING;
    Some(token)
}

/// Tokens still waiting (tests, and a screen deciding whether a script is running).
#[allow(dead_code)]
pub fn remaining(script: &MenuScript) -> usize {
    script.tokens.len()
}

/// `<dir>/<prefix><name>.png` for a `shot:` of `name`, beside the
/// `--screenshot` file `base` (prefix = its stem up to and including the first
/// `_`, else the whole stem and a `_`).
pub fn shot_path(base: &std::path::Path, name: &str) -> std::path::PathBuf {
    let stem = base.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let prefix = stem.find('_').map_or_else(|| format!("{stem}_"), |i| stem[..=i].to_string());
    base.with_file_name(format!("{prefix}{name}.png"))
}

/// Every frame, on every screen: writes the screenshots the script asked for
/// and exits after `quit`.
pub fn drive(mut commands: Commands, cfg: Res<crate::Config>, script: Option<ResMut<MenuScript>>, mut exit: MessageWriter<AppExit>) {
    let Some(mut script) = script else { return };
    for name in std::mem::take(&mut script.shots) {
        if let Some(base) = &cfg.screenshot {
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(shot_path(base, &name)));
        }
    }
    if let Some((name, n, k)) = script.burst.clone() {
        if let Some(base) = &cfg.screenshot {
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(shot_path(base, &format!("{name}_{k:02}"))));
        }
        script.burst = (k + 1 < n).then(|| (name, n, k + 1));
    }
    if let Some(n) = script.quit_in {
        if n == 0 {
            exit.write(AppExit::Success);
        } else {
            script.quit_in = Some(n - 1);
        }
    }
}

/// During a match: takes the `mshot@<tick>:<name>` tokens at the front of the
/// script (and nothing else) once the sim has reached their tick.
pub fn match_tokens(world: Res<crate::World>, script: Option<ResMut<MenuScript>>) {
    let Some(mut script) = script else { return };
    if script.cooldown > 0 {
        script.cooldown -= 1;
        return;
    }
    let Some(front) = script.tokens.front() else { return };
    let Some(rest) = front.strip_prefix("mshot@") else { return };
    let Some((tick, name)) = rest.split_once(':') else {
        script.tokens.pop_front();
        return;
    };
    if tick.parse::<u64>().map_or(true, |t| world.0.tick >= t) {
        let name = name.to_string();
        script.tokens.pop_front();
        script.shots.push(name);
        script.cooldown = TOKEN_SPACING;
    }
}

/// `PD_TEST_KO=<tick>:<slots>[;<tick>:<slots>]` (slots 1-based, comma
/// separated): knocks those players out once the sim reaches the tick.
pub fn test_ko(mut world: ResMut<crate::World>, mut done: Local<Vec<usize>>, mut last_tick: Local<u64>) {
    let Ok(spec) = std::env::var("PD_TEST_KO") else { return };
    // A new match starts its ticks again: the knock-outs happen again in it.
    if world.0.tick < *last_tick {
        done.clear();
    }
    *last_tick = world.0.tick;
    for (k, (tick, slots)) in parse_ko(&spec).into_iter().enumerate() {
        if done.contains(&k) || world.0.tick < tick {
            continue;
        }
        done.push(k);
        for s in slots {
            if world.0.combat.roster.is_active(s) {
                world.0.combat.change_health(s, -999);
            }
        }
    }
}

/// `"600:3,4;900:2"` -> `[(600, [2, 3]), (900, [1])]` (slots become 0-based).
fn parse_ko(spec: &str) -> Vec<(u64, Vec<usize>)> {
    spec.split(';')
        .filter_map(|part| {
            let (tick, slots) = part.split_once(':')?;
            let slots: Vec<usize> = slots.split(',').filter_map(|s| s.trim().parse::<usize>().ok()).filter(|s| (1..=crate::combat::MAX_PLAYERS).contains(s)).map(|s| s - 1).collect();
            Some((tick.trim().parse().ok()?, slots))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the script until it is empty, collecting each frame's press.
    fn drain(script: &mut MenuScript) -> Vec<(usize, MenuAxis)> {
        let mut out = Vec::new();
        for _ in 0..2000 {
            out.extend(tick_pad(script));
        }
        out
    }

    #[test]
    fn tokens_name_a_virtual_pad_and_default_to_pad_one() {
        let mut s = MenuScript::new("confirm, p2:down ,p4:a,p3:cancel,bogus");
        let pressed = drain(&mut s);
        let who: Vec<usize> = pressed.iter().map(|p| p.0).collect();
        assert_eq!(who, [0, 1, 3, 2], "unknown tokens are skipped");
        assert!(pressed[0].1.confirm && pressed[1].1.down && pressed[2].1.confirm && pressed[3].1.cancel);
    }

    #[test]
    fn shots_waits_and_quit_are_handled_without_a_press() {
        let mut s = MenuScript::new("shot:one,wait:30,a,quit");
        assert!(tick_pad(&mut s).is_none());
        assert_eq!(s.shots, ["one"]);
        // The wait token holds the script for 30 frames after its own spacing.
        let mut frames = 0;
        let press = loop {
            frames += 1;
            if let Some(p) = tick_pad(&mut s) {
                break p;
            }
            assert!(frames < 200);
        };
        assert!(press.1.confirm);
        assert!(frames >= 30 + TOKEN_SPACING, "{frames}");
        drain(&mut s);
        assert!(s.quit_in.is_some());
    }

    #[test]
    fn match_tokens_wait_for_the_match_and_are_not_taken_by_menus() {
        let mut s = MenuScript::new("mshot@600:in_match,confirm");
        assert!(drain(&mut s).is_empty(), "the menus leave a match token (and what follows it) alone");
        assert_eq!(remaining(&s), 2);
    }

    #[test]
    fn shot_files_sit_beside_the_screenshot_with_its_prefix() {
        let p = shot_path(std::path::Path::new("C:/x/shots/s4p_main.png"), "four_seats");
        assert_eq!(p, std::path::Path::new("C:/x/shots/s4p_four_seats.png"));
        assert_eq!(shot_path(std::path::Path::new("out.png"), "a"), std::path::Path::new("out_a.png"));
    }

    #[test]
    fn ko_specs_parse_to_ticks_and_zero_based_slots() {
        assert_eq!(parse_ko("600:3,4;900:2"), vec![(600, vec![2, 3]), (900, vec![1])]);
        assert_eq!(parse_ko("5:9,0,1"), vec![(5, vec![0])], "slots outside 1-4 are ignored");
        assert!(parse_ko("nonsense").is_empty());
    }
}
