//! Button remapping (the Options overlay's CONTROLS tab > BUTTON MAPPING page, `menu.rs`).
//!
//! One shared mapping for every pad, plus keyboard player 1's keys, saved in `settings.json`
//! under `"bindings"`. `gather_input` (main.rs) reads the buttons through it before anything
//! is quantised into `sim::Input`, so the sim, replays and the online lockstep only ever see
//! the resulting `Input`, exactly as before. The defaults are the game's old fixed buttons:
//! a file without the key (or no file) plays exactly as before.
//!
//! What is NOT remappable, and why: moving (the left stick / W A S D) and the camera (the
//! right stick / Q E R F) are sticks and axes, not buttons; the D-pad / arrow keys show the
//! skill cards and drive every menu; Start / Esc pause, so nobody can lock themselves out of
//! the menus; clicking the right stick recentres the camera (left fixed, not asked). The menus
//! never read this mapping: A / B / Start / D-pad (Enter / Space / Backspace / Esc / arrows)
//! always navigate them. The keyboard's second player (Enter, Right Shift, numpad, a test
//! fallback) stays fixed too.

use bevy::prelude::*;

/// The remappable actions, in the page's row order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// The character's dodge (jump / tumble / cartwheel): click the left stick by default.
    Dodge,
    /// The second dodge button the Xbox One / PC re-release added: LB by default.
    Dodge2,
    /// The four skill slots, the dial's left / right / top / bottom (also capture a capsule).
    SkillLeft,
    SkillRight,
    SkillTop,
    SkillBottom,
    /// Lock on / off.
    LockOn,
    /// Switch the lock to the next target.
    SwitchTarget,
}

impl Action {
    pub const ALL: [Action; 8] = [Action::Dodge, Action::Dodge2, Action::SkillLeft, Action::SkillRight, Action::SkillTop, Action::SkillBottom, Action::LockOn, Action::SwitchTarget];

    fn index(self) -> usize {
        Action::ALL.iter().position(|a| *a == self).unwrap_or(0)
    }

    /// The row's label.
    pub fn label(self) -> &'static str {
        match self {
            Action::Dodge => "DODGE",
            Action::Dodge2 => "DODGE (2ND)",
            Action::SkillLeft => "SKILL LEFT",
            Action::SkillRight => "SKILL RIGHT",
            Action::SkillTop => "SKILL TOP",
            Action::SkillBottom => "SKILL BOTTOM",
            Action::LockOn => "LOCK ON",
            Action::SwitchTarget => "SWITCH TARGET",
        }
    }

    /// The settings.json name.
    fn key_name(self) -> &'static str {
        match self {
            Action::Dodge => "dodge",
            Action::Dodge2 => "dodge2",
            Action::SkillLeft => "skill_left",
            Action::SkillRight => "skill_right",
            Action::SkillTop => "skill_top",
            Action::SkillBottom => "skill_bottom",
            Action::LockOn => "lock_on",
            Action::SwitchTarget => "switch_target",
        }
    }
}

/// Pad buttons a remappable action may have, with the name shown (Xbox layout).
pub const PAD_BUTTONS: [(GamepadButton, &str); 11] = [
    (GamepadButton::South, "A"),
    (GamepadButton::East, "B"),
    (GamepadButton::West, "X"),
    (GamepadButton::North, "Y"),
    (GamepadButton::LeftTrigger, "LB"),
    (GamepadButton::RightTrigger, "RB"),
    (GamepadButton::LeftTrigger2, "LT"),
    (GamepadButton::RightTrigger2, "RT"),
    (GamepadButton::LeftThumb, "L3"),
    (GamepadButton::RightThumb, "R3"),
    (GamepadButton::Select, "VIEW"),
];

/// Keys keyboard player 1 may bind. Left out on purpose: W A S D (move), Q E R F (camera),
/// the arrows (skill cards), Esc (pause), Enter / Backspace / Tab (menus), the numpad and
/// Right Shift (keyboard player 2), F1-F12 (dev tools). Right stick click (recentre) has no key.
pub const KEYS: [(KeyCode, &str); 50] = [
    (KeyCode::Space, "SPACE"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyG, "G"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyJ, "J"),
    (KeyCode::KeyK, "K"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyO, "O"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyT, "T"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::ShiftLeft, "LEFT SHIFT"),
    (KeyCode::ControlLeft, "LEFT CTRL"),
    (KeyCode::ControlRight, "RIGHT CTRL"),
    (KeyCode::AltLeft, "LEFT ALT"),
    (KeyCode::AltRight, "RIGHT ALT"),
    (KeyCode::CapsLock, "CAPS LOCK"),
    (KeyCode::Comma, ","),
    (KeyCode::Period, "."),
    (KeyCode::Slash, "/"),
    (KeyCode::Semicolon, ";"),
    (KeyCode::Quote, "'"),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Backslash, "\\"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
    (KeyCode::Backquote, "`"),
    (KeyCode::Insert, "INSERT"),
    (KeyCode::Delete, "DELETE"),
    (KeyCode::Home, "HOME"),
    (KeyCode::End, "END"),
];

/// The settings.json name of a pad button / key (Bevy's own variant name).
fn pad_id(b: GamepadButton) -> String {
    format!("{b:?}")
}
fn key_id(k: KeyCode) -> String {
    format!("{k:?}")
}

pub fn pad_label(b: GamepadButton) -> &'static str {
    PAD_BUTTONS.iter().find(|(p, _)| *p == b).map_or("?", |(_, n)| n)
}

pub fn key_label(k: Option<KeyCode>) -> &'static str {
    k.and_then(|k| KEYS.iter().find(|(c, _)| *c == k)).map_or("-", |(_, n)| n)
}

/// Why a press could not be bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindError {
    /// A button / key the game keeps fixed (menus, pause, sticks, player 2).
    Reserved,
    /// The key belongs to another action and this action has no key to swap it for:
    /// the other action would be left without one.
    WouldUnbind(Action),
}

/// The mapping: each action's pad button (every action always has one) and keyboard key
/// (`None` = no key; only actions that never had one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bindings {
    pad: [GamepadButton; 8],
    key: [Option<KeyCode>; 8],
}

impl Default for Bindings {
    /// The game's old fixed buttons (the original's own layout, GAME.md "Controls").
    fn default() -> Self {
        Bindings {
            pad: [
                GamepadButton::LeftThumb,
                GamepadButton::LeftTrigger,
                GamepadButton::West,
                GamepadButton::East,
                GamepadButton::North,
                GamepadButton::South,
                GamepadButton::RightTrigger2,
                GamepadButton::LeftTrigger2,
            ],
            key: [Some(KeyCode::Space), None, Some(KeyCode::KeyJ), Some(KeyCode::KeyK), Some(KeyCode::KeyU), Some(KeyCode::KeyI), Some(KeyCode::KeyY), Some(KeyCode::KeyT)],
        }
    }
}

impl Bindings {
    pub fn pad(&self, a: Action) -> GamepadButton {
        self.pad[a.index()]
    }

    pub fn key(&self, a: Action) -> Option<KeyCode> {
        self.key[a.index()]
    }

    pub fn is_default(&self) -> bool {
        *self == Bindings::default()
    }

    /// Gives `a` the pad button `b`. An action that already had `b` takes `a`'s old button
    /// (a swap), so no action is ever without one.
    pub fn bind_pad(&mut self, a: Action, b: GamepadButton) -> Result<(), BindError> {
        if !PAD_BUTTONS.iter().any(|(p, _)| *p == b) {
            return Err(BindError::Reserved);
        }
        let old = self.pad(a);
        if let Some(other) = Action::ALL.into_iter().find(|o| *o != a && self.pad(*o) == b) {
            self.pad[other.index()] = old;
        }
        self.pad[a.index()] = b;
        Ok(())
    }

    /// Gives `a` the key `k`, swapping with an action that had it (refused when `a` has no
    /// key to give it, so the other action is never left without one).
    pub fn bind_key(&mut self, a: Action, k: KeyCode) -> Result<(), BindError> {
        if !KEYS.iter().any(|(c, _)| *c == k) {
            return Err(BindError::Reserved);
        }
        let old = self.key(a);
        if let Some(other) = Action::ALL.into_iter().find(|o| *o != a && self.key(*o) == Some(k)) {
            match old {
                Some(_) => self.key[other.index()] = old,
                None => return Err(BindError::WouldUnbind(other)),
            }
        }
        self.key[a.index()] = Some(k);
        Ok(())
    }

    /// `"bindings"` in settings.json: `{ "dodge": { "pad": "LeftThumb", "key": "Space" }, ... }`.
    /// A missing action or field keeps its default; an unknown name, or a file that would give
    /// two actions one button / key, loads the defaults whole (never a broken mapping).
    pub fn from_json(v: &serde_json::Value) -> Self {
        let mut b = Bindings::default();
        if !v.is_object() {
            return b;
        }
        for a in Action::ALL {
            let e = &v[a.key_name()];
            if let Some(name) = e["pad"].as_str() {
                match PAD_BUTTONS.iter().find(|(p, _)| pad_id(*p) == name) {
                    Some((p, _)) => b.pad[a.index()] = *p,
                    None => return Bindings::default(),
                }
            }
            match &e["key"] {
                serde_json::Value::Null if e.get("key").is_some() => b.key[a.index()] = None,
                serde_json::Value::String(name) => match KEYS.iter().find(|(k, _)| key_id(*k) == *name) {
                    Some((k, _)) => b.key[a.index()] = Some(*k),
                    None => return Bindings::default(),
                },
                _ => {}
            }
        }
        if b.valid() { b } else { Bindings::default() }
    }

    /// No pad button and no key used twice.
    fn valid(&self) -> bool {
        let pads_unique = self.pad.iter().enumerate().all(|(i, p)| !self.pad[..i].contains(p));
        let keys_unique = self.key.iter().enumerate().all(|(i, k)| k.is_none() || !self.key[..i].contains(k));
        pads_unique && keys_unique
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for a in Action::ALL {
            m.insert(a.key_name().to_string(), serde_json::json!({ "pad": pad_id(self.pad(a)), "key": self.key(a).map(key_id) }));
        }
        serde_json::Value::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_old_fixed_buttons() {
        let b = Bindings::default();
        let pads = Action::ALL.map(|a| pad_label(b.pad(a)));
        assert_eq!(pads, ["L3", "LB", "X", "B", "Y", "A", "RT", "LT"]);
        let keys = Action::ALL.map(|a| key_label(b.key(a)));
        assert_eq!(keys, ["SPACE", "-", "J", "K", "U", "I", "Y", "T"]);
        assert!(b.valid() && b.is_default());
        // Every default is itself bindable (a reset can always be re-made by hand).
        for a in Action::ALL {
            assert!(PAD_BUTTONS.iter().any(|(p, _)| *p == b.pad(a)));
            assert!(b.key(a).is_none_or(|k| KEYS.iter().any(|(c, _)| *c == k)));
        }
    }

    #[test]
    fn binding_a_used_button_swaps_and_nothing_is_ever_unbound() {
        let mut b = Bindings::default();
        // Dodge onto X: Skill left takes L3.
        assert_eq!(b.bind_pad(Action::Dodge, GamepadButton::West), Ok(()));
        assert_eq!((b.pad(Action::Dodge), b.pad(Action::SkillLeft)), (GamepadButton::West, GamepadButton::LeftThumb));
        // A free button: just moves.
        assert_eq!(b.bind_pad(Action::LockOn, GamepadButton::RightTrigger), Ok(()));
        assert_eq!(b.pad(Action::LockOn), GamepadButton::RightTrigger);
        // Fixed buttons are refused and change nothing.
        let before = b;
        for fixed in [GamepadButton::Start, GamepadButton::DPadUp, GamepadButton::DPadLeft, GamepadButton::Mode] {
            assert_eq!(b.bind_pad(Action::SkillTop, fixed), Err(BindError::Reserved));
        }
        assert_eq!(b, before);
        // Keys: a swap between two keyed actions.
        assert_eq!(b.bind_key(Action::SkillLeft, KeyCode::KeyK), Ok(()));
        assert_eq!((b.key(Action::SkillLeft), b.key(Action::SkillRight)), (Some(KeyCode::KeyK), Some(KeyCode::KeyJ)));
        // Dodge 2 has no key: taking Skill top's U would leave Skill top without one, so it is refused.
        assert_eq!(b.bind_key(Action::Dodge2, KeyCode::KeyU), Err(BindError::WouldUnbind(Action::SkillTop)));
        assert_eq!((b.key(Action::Dodge2), b.key(Action::SkillTop)), (None, Some(KeyCode::KeyU)));
        // ... but a free key is fine.
        assert_eq!(b.bind_key(Action::Dodge2, KeyCode::KeyC), Ok(()));
        assert_eq!(b.key(Action::Dodge2), Some(KeyCode::KeyC));
        // Movement, camera, menu and player-2 keys are refused.
        for fixed in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::ArrowUp, KeyCode::Escape, KeyCode::Enter, KeyCode::Backspace, KeyCode::ShiftRight, KeyCode::Numpad1, KeyCode::F8] {
            assert_eq!(b.bind_key(Action::Dodge, fixed), Err(BindError::Reserved), "{fixed:?}");
        }
        assert!(b.valid());
    }

    #[test]
    fn the_mapping_round_trips_and_bad_files_load_the_defaults() {
        let mut b = Bindings::default();
        b.bind_pad(Action::Dodge, GamepadButton::South).unwrap();
        b.bind_key(Action::LockOn, KeyCode::ShiftLeft).unwrap();
        b.bind_key(Action::Dodge2, KeyCode::KeyC).unwrap();
        let v = b.to_json();
        assert_eq!(Bindings::from_json(&v), b);
        // Through text, as settings.json holds it.
        let text = serde_json::to_string(&v).unwrap();
        assert_eq!(Bindings::from_json(&serde_json::from_str(&text).unwrap()), b);
        // Missing (an old file): the defaults. A partial entry keeps the rest at default.
        assert_eq!(Bindings::from_json(&serde_json::Value::Null), Bindings::default());
        let partial = serde_json::json!({ "lock_on": { "key": "KeyL" } });
        let p = Bindings::from_json(&partial);
        assert_eq!((p.key(Action::LockOn), p.pad(Action::LockOn), p.key(Action::Dodge)), (Some(KeyCode::KeyL), GamepadButton::RightTrigger2, Some(KeyCode::Space)));
        // Junk or a doubled button: the defaults whole.
        assert_eq!(Bindings::from_json(&serde_json::json!({ "dodge": { "pad": "Banana" } })), Bindings::default());
        assert_eq!(Bindings::from_json(&serde_json::json!({ "dodge": { "pad": "West" } })), Bindings::default(), "X twice");
        assert_eq!(Bindings::from_json(&serde_json::json!({ "dodge": { "key": "KeyW" } })), Bindings::default(), "a fixed key");
    }
}
