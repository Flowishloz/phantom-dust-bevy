//! Menu navigation for two players at once: P1 keyboard WASD/Enter/Esc plus
//! the first gamepad; P2 has no keyboard movement in a match either, so the
//! numpad stands in for a keyboard P2, plus the second gamepad. Edge
//! triggered (one pulse per press), like the settings menu (`menu.rs`).

use bevy::prelude::*;

#[derive(Clone, Copy, Default, Debug)]
pub struct MenuAxis {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub confirm: bool,
    pub cancel: bool,
    /// The pad's Start button (keyboard: Tab, seat 1 only). The versus setup's
    /// seat 1 uses it to go on to the game rules.
    pub start: bool,
    /// X / Y / LB / RB (keyboard seat 1: X, Y, Q, E). The character-select screen uses them for CPU seats: Y adds a CPU, X
    /// removes the one being edited, LB / RB pick which seat's column the cursor edits. Only [`read_seat`] fills them.
    pub x: bool,
    pub y: bool,
    pub lb: bool,
    pub rb: bool,
}

impl MenuAxis {
    pub fn any(&self) -> bool {
        self.up || self.down || self.left || self.right || self.confirm || self.cancel || self.start || self.x || self.y || self.lb || self.rb
    }
}

/// Player `i`'s menu input this frame (0 = P1, 1 = P2). Also folds in a
/// scripted pulse (`--menu-script`, see `frontend::script`) so screenshots
/// can be reproduced without a human at the controls.
pub fn read(i: usize, keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, scripted: Option<MenuAxis>) -> MenuAxis {
    let mut a = MenuAxis::default();
    if i == 0 {
        a.up |= keys.just_pressed(KeyCode::KeyW) || keys.just_pressed(KeyCode::ArrowUp);
        a.down |= keys.just_pressed(KeyCode::KeyS) || keys.just_pressed(KeyCode::ArrowDown);
        a.left |= keys.just_pressed(KeyCode::KeyA) || keys.just_pressed(KeyCode::ArrowLeft);
        a.right |= keys.just_pressed(KeyCode::KeyD) || keys.just_pressed(KeyCode::ArrowRight);
        a.confirm |= keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space);
        a.cancel |= keys.just_pressed(KeyCode::Escape);
    } else {
        a.up |= keys.just_pressed(KeyCode::Numpad8);
        a.down |= keys.just_pressed(KeyCode::Numpad2);
        a.left |= keys.just_pressed(KeyCode::Numpad4);
        a.right |= keys.just_pressed(KeyCode::Numpad6);
        a.confirm |= keys.just_pressed(KeyCode::Numpad5) || keys.just_pressed(KeyCode::NumpadEnter);
        a.cancel |= keys.just_pressed(KeyCode::NumpadDecimal) || keys.just_pressed(KeyCode::Numpad0);
    }
    if let Some(pad) = pads.iter().nth(i) {
        a.up |= pad.just_pressed(GamepadButton::DPadUp);
        a.down |= pad.just_pressed(GamepadButton::DPadDown);
        a.left |= pad.just_pressed(GamepadButton::DPadLeft);
        a.right |= pad.just_pressed(GamepadButton::DPadRight);
        a.confirm |= pad.just_pressed(GamepadButton::South);
        a.cancel |= pad.just_pressed(GamepadButton::East);
    }
    if let Some(s) = scripted {
        a.up |= s.up;
        a.down |= s.down;
        a.left |= s.left;
        a.right |= s.right;
        a.confirm |= s.confirm;
        a.cancel |= s.cancel;
    }
    a
}

/// How far the left stick must be pushed before it counts as a menu
/// direction (a stick rests near 0 and drifts; half travel is a deliberate
/// push). The match's own movement deadzone is 0.2, which is too touchy for a
/// list cursor.
pub const STICK_DEADZONE: f32 = 0.5;

/// The left stick as held menu directions `(up, down, left, right)`: the
/// stronger axis wins, so a diagonal push moves one way, never two at once.
pub fn stick_dirs(s: Vec2) -> (bool, bool, bool, bool) {
    if s.length() < STICK_DEADZONE {
        return (false, false, false, false);
    }
    if s.y.abs() >= s.x.abs() {
        (s.y > 0.0, s.y < 0.0, false, false)
    } else {
        (false, false, s.x < 0.0, s.x > 0.0)
    }
}

/// Level (held, not edge) state of the four directions this frame, for
/// hold-to-repeat. Confirm/cancel stay one-shot (`read`) everywhere: only
/// navigation repeats.
fn held(i: usize, keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>) -> (bool, bool, bool, bool) {
    let (mut up, mut down, mut left, mut right) = (false, false, false, false);
    if i == 0 {
        up |= keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp);
        down |= keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown);
        left |= keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft);
        right |= keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight);
    } else {
        up |= keys.pressed(KeyCode::Numpad8);
        down |= keys.pressed(KeyCode::Numpad2);
        left |= keys.pressed(KeyCode::Numpad4);
        right |= keys.pressed(KeyCode::Numpad6);
    }
    if let Some(pad) = pads.iter().nth(i) {
        up |= pad.pressed(GamepadButton::DPadUp);
        down |= pad.pressed(GamepadButton::DPadDown);
        left |= pad.pressed(GamepadButton::DPadLeft);
        right |= pad.pressed(GamepadButton::DPadRight);
        let (u, d, l, r) = stick_dirs(pad.left_stick());
        (up, down, left, right) = (up | u, down | d, left | l, right | r);
    }
    (up, down, left, right)
}

/// Per-player, per-direction held-frame counters driving [`read_repeating`].
/// Owned by each screen's own state resource (one per player where the
/// screen has two independent cursors).
#[derive(Clone, Copy, Default, Debug)]
pub struct Repeat {
    up: u32,
    down: u32,
    left: u32,
    right: u32,
}

impl Repeat {
    /// How many frames Left and Right have been held in a row (0 = not held),
    /// for a slider that speeds up the longer it is held (`menu.rs`).
    pub fn held_left_right(&self) -> (u32, u32) {
        (self.left, self.right)
    }
}

/// Frames held before the first repeat (~0.35s at 60 Hz) and between repeats
/// after that (~0.1s) -- fast enough to fly down a long list, slow enough
/// not to overshoot a one-tap move.
const REPEAT_DELAY: u32 = 21;
const REPEAT_EVERY: u32 = 6;

/// True the frame a held direction should pulse again (frame 1 = the normal
/// edge-triggered first press already `read` reports; the initial delay,
/// then steady repeats).
fn tick(frames: &mut u32, is_held: bool) -> bool {
    if !is_held {
        *frames = 0;
        return false;
    }
    *frames += 1;
    *frames == 1 || (*frames > REPEAT_DELAY && (*frames - REPEAT_DELAY) % REPEAT_EVERY == 0)
}

/// [`read`] plus hold-to-repeat on the four directions (confirm/cancel and
/// scripted input are unaffected -- those already come through `read`).
pub fn read_repeating(i: usize, keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, scripted: Option<MenuAxis>, repeat: &mut Repeat) -> MenuAxis {
    let mut a = read(i, keys, pads, scripted);
    let (hu, hd, hl, hr) = held(i, keys, pads);
    a.up |= tick(&mut repeat.up, hu);
    a.down |= tick(&mut repeat.down, hd);
    a.left |= tick(&mut repeat.left, hl);
    a.right |= tick(&mut repeat.right, hr);
    a
}

/// The in-match pause menu's navigation: the keyboard (W A S D / arrows) AND
/// every connected pad's D-pad and left stick, whichever seat or entity order
/// they have, with the same hold-to-repeat as every other menu. (It used to
/// read "the first pad Bevy lists" D-pad only, so a seat-1 pad that was not
/// first -- or a stick -- did nothing.) Confirm / cancel / Start are the
/// caller's (`menu::menu_input`).
pub fn read_pause(keys: &ButtonInput<KeyCode>, pads: &[&Gamepad], repeat: &mut Repeat) -> MenuAxis {
    let mut a = MenuAxis::default();
    let (mut hu, mut hd, mut hl, mut hr) = (false, false, false, false);
    a.up |= keys.just_pressed(KeyCode::KeyW) || keys.just_pressed(KeyCode::ArrowUp);
    a.down |= keys.just_pressed(KeyCode::KeyS) || keys.just_pressed(KeyCode::ArrowDown);
    a.left |= keys.just_pressed(KeyCode::KeyA) || keys.just_pressed(KeyCode::ArrowLeft);
    a.right |= keys.just_pressed(KeyCode::KeyD) || keys.just_pressed(KeyCode::ArrowRight);
    a.confirm |= keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space);
    hu |= keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp);
    hd |= keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown);
    hl |= keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft);
    hr |= keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight);
    for pad in pads {
        a.up |= pad.just_pressed(GamepadButton::DPadUp);
        a.down |= pad.just_pressed(GamepadButton::DPadDown);
        a.left |= pad.just_pressed(GamepadButton::DPadLeft);
        a.right |= pad.just_pressed(GamepadButton::DPadRight);
        hu |= pad.pressed(GamepadButton::DPadUp);
        hd |= pad.pressed(GamepadButton::DPadDown);
        hl |= pad.pressed(GamepadButton::DPadLeft);
        hr |= pad.pressed(GamepadButton::DPadRight);
        let (u, d, l, r) = stick_dirs(pad.left_stick());
        (hu, hd, hl, hr) = (hu | u, hd | d, hl | l, hr | r);
    }
    a.up |= tick(&mut repeat.up, hu);
    a.down |= tick(&mut repeat.down, hd);
    a.left |= tick(&mut repeat.left, hl);
    a.right |= tick(&mut repeat.right, hr);
    a
}

/// A pad's directions and buttons as menu input: D-pad, A (confirm), B (cancel).
fn pad_axis(pad: &Gamepad, a: &mut MenuAxis) {
    a.up |= pad.just_pressed(GamepadButton::DPadUp);
    a.down |= pad.just_pressed(GamepadButton::DPadDown);
    a.left |= pad.just_pressed(GamepadButton::DPadLeft);
    a.right |= pad.just_pressed(GamepadButton::DPadRight);
    a.confirm |= pad.just_pressed(GamepadButton::South);
    a.cancel |= pad.just_pressed(GamepadButton::East);
    a.start |= pad.just_pressed(GamepadButton::Start);
    a.x |= pad.just_pressed(GamepadButton::West);
    a.y |= pad.just_pressed(GamepadButton::North);
    a.lb |= pad.just_pressed(GamepadButton::LeftTrigger);
    a.rb |= pad.just_pressed(GamepadButton::RightTrigger);
}

/// Seat `seat`'s menu input this frame, for the setup screen where pads are
/// bound to seats (`seats::Seats`) rather than taken in the order Bevy lists
/// them: `pad` is the seat's own pad, if it has one. Seat 0 also answers to
/// the keyboard (W A S D / arrows, Enter or Space, Backspace = B, Tab = Start; Esc is the
/// screen's "back to title", not a seat's). Directions repeat when held.
pub fn read_seat(seat: usize, keys: &ButtonInput<KeyCode>, pad: Option<&Gamepad>, scripted: Option<MenuAxis>, repeat: &mut Repeat) -> MenuAxis {
    let mut a = MenuAxis::default();
    let (mut hu, mut hd, mut hl, mut hr) = (false, false, false, false);
    if seat == 0 {
        a.up |= keys.just_pressed(KeyCode::KeyW) || keys.just_pressed(KeyCode::ArrowUp);
        a.down |= keys.just_pressed(KeyCode::KeyS) || keys.just_pressed(KeyCode::ArrowDown);
        a.left |= keys.just_pressed(KeyCode::KeyA) || keys.just_pressed(KeyCode::ArrowLeft);
        a.right |= keys.just_pressed(KeyCode::KeyD) || keys.just_pressed(KeyCode::ArrowRight);
        a.confirm |= keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space);
        a.cancel |= keys.just_pressed(KeyCode::Backspace);
        a.start |= keys.just_pressed(KeyCode::Tab);
        a.x |= keys.just_pressed(KeyCode::KeyX);
        a.y |= keys.just_pressed(KeyCode::KeyY);
        a.lb |= keys.just_pressed(KeyCode::KeyQ);
        a.rb |= keys.just_pressed(KeyCode::KeyE);
        hu |= keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp);
        hd |= keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown);
        hl |= keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft);
        hr |= keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight);
    }
    if let Some(pad) = pad {
        pad_axis(pad, &mut a);
        hu |= pad.pressed(GamepadButton::DPadUp);
        hd |= pad.pressed(GamepadButton::DPadDown);
        hl |= pad.pressed(GamepadButton::DPadLeft);
        hr |= pad.pressed(GamepadButton::DPadRight);
        let (u, d, l, r) = stick_dirs(pad.left_stick());
        (hu, hd, hl, hr) = (hu | u, hd | d, hl | l, hr | r);
    }
    a.up |= tick(&mut repeat.up, hu);
    a.down |= tick(&mut repeat.down, hd);
    a.left |= tick(&mut repeat.left, hl);
    a.right |= tick(&mut repeat.right, hr);
    if let Some(s) = scripted {
        a.up |= s.up;
        a.down |= s.down;
        a.left |= s.left;
        a.right |= s.right;
        a.confirm |= s.confirm;
        a.cancel |= s.cancel;
        a.start |= s.start;
        a.x |= s.x;
        a.y |= s.y;
        a.lb |= s.lb;
        a.rb |= s.rb;
    }
    a
}

/// Whether the keyboard did something that joins seat 1 (a menu key, not Esc).
pub fn keyboard_joins(keys: &ButtonInput<KeyCode>) -> bool {
    [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::Enter, KeyCode::Space].iter().any(|&k| keys.just_pressed(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_pulses_once_immediately_then_after_a_delay_then_steadily() {
        let mut f = 0u32;
        // Held frames 1..=30: pulse on 1, nothing until the delay passes, then steady.
        let mut pulses = Vec::new();
        for frame in 1..=40u32 {
            if tick(&mut f, true) {
                pulses.push(frame);
            }
        }
        assert_eq!(pulses[0], 1);
        assert_eq!(pulses[1], REPEAT_DELAY + REPEAT_EVERY);
        assert_eq!(pulses[2], REPEAT_DELAY + REPEAT_EVERY * 2);
    }

    #[test]
    fn stick_has_a_deadzone_and_picks_one_direction() {
        assert_eq!(stick_dirs(Vec2::new(0.3, 0.3)), (false, false, false, false));
        assert_eq!(stick_dirs(Vec2::new(0.0, 1.0)), (true, false, false, false));
        assert_eq!(stick_dirs(Vec2::new(0.0, -0.8)), (false, true, false, false));
        assert_eq!(stick_dirs(Vec2::new(-0.9, 0.2)), (false, false, true, false));
        assert_eq!(stick_dirs(Vec2::new(0.7, 0.6)), (false, false, false, true), "a diagonal moves one way only");
    }

    #[test]
    fn releasing_resets_the_repeat_timer() {
        let mut f = 5;
        assert!(!tick(&mut f, false));
        assert_eq!(f, 0);
        assert!(tick(&mut f, true), "first frame back down pulses immediately");
    }
}
