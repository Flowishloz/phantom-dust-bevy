//! Arsenal builder, rebuilt after the original's own editor (`Menu/
//! arsenaledit_en.ssb`, `arsenal.rs` for the rules). Two pages:
//!
//! * **List**: your arsenals (up to 16) plus "New Arsenal". A opens one, X
//!   makes a new one (name, then "Number of Schools" 1-5), Y = options
//!   (Change Arsenal Name / Number of Schools / Randomize / Disassemble),
//!   Left/Right page the contents preview, B back.
//! * **Edit**: the 30 fixed slots (empty = Aura Particle) next to the stock
//!   list. On a slot: A Replace (pick from the stock), X Replace with Aura,
//!   Y Add Same Skill. In the stock: LB/RB switch the type tab (AURA, ATTACK,
//!   DEFENSE, ERASE, STATUS, SPECIAL, ENVIRONMENTAL), A place, X favourite,
//!   Y options (Sort / Mask School / Mark Illegal Skills). Start saves; B
//!   asks "Finish editing?" -> "Reflect changes?" -> "Undo edits?".
//!
//! Every action is reachable with a pad (D-pad, A B X Y, LB RB, Start; the
//! arsenal name has an on-screen keyboard); the keyboard stays: arrows/WASD,
//! Enter/Space = A, Esc = B, X, Y, Q/E = LB/RB, Ctrl+S or Tab = Start.
//!
//! The rules are `arsenal.rs`; this file is the editor's state machine
//! ([`Core`], pure and unit-tested with abstract [`Pulse`]s), the view it
//! produces ([`View`]) and the Bevy systems; `builder_ui.rs` draws the view.

use super::arsenals::{self, BuilderPrefs, SavedArsenal, SortKey};
use super::builder_ui as bui;
use super::sound::Cue;
use super::state::{AppState, SavedArsenals, SkillCatalog};
use super::ui::{self, MenuArt};
use crate::arsenal::{self, PlaceError, RuleMark, ARSENAL_SIZE, KINDS, MAX_ARSENALS, MAX_COPIES, MAX_NAME_LEN};
use crate::combat::{Capsule, SkillDef, AURA_PARTICLE};
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use pd_formats::skills::SCHOOL_NAMES;

/// Rows visible in each list column.
pub const VISIBLE: usize = 10;

// ---------------------------------------------------------------------------
// Input vocabulary
// ---------------------------------------------------------------------------

/// One abstract button press, from pad, keyboard or `--menu-script`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pulse {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    Lb,
    Rb,
    Start,
    /// A typed character (name entry only).
    Char(char),
    Backspace,
}

impl Pulse {
    /// The script token for a button (`--menu-script`): `up down left right
    /// a|confirm b|cancel x y lb rb start bs`.
    pub fn from_token(t: &str) -> Option<Pulse> {
        Some(match t {
            "up" => Pulse::Up,
            "down" => Pulse::Down,
            "left" => Pulse::Left,
            "right" => Pulse::Right,
            "a" | "confirm" => Pulse::A,
            "b" | "cancel" => Pulse::B,
            "x" => Pulse::X,
            "y" => Pulse::Y,
            "lb" => Pulse::Lb,
            "rb" => Pulse::Rb,
            "start" => Pulse::Start,
            "bs" => Pulse::Backspace,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Editor state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    List,
    Edit,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Slots,
    Stock,
}

/// The stock list's type pages (`arsenaledit_en.ssb`: AURA, ATTACK, DEFENSE,
/// ERASE, STATUS, SPECIAL, ENVIRONMENTAL).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Aura,
    Attack,
    Defense,
    Erase,
    Status,
    Special,
    Environmental,
}

impl Tab {
    pub const ALL: [Tab; 7] = [Tab::Aura, Tab::Attack, Tab::Defense, Tab::Erase, Tab::Status, Tab::Special, Tab::Environmental];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Aura => "AURA",
            Tab::Attack => "ATTACK",
            Tab::Defense => "DEFENSE",
            Tab::Erase => "ERASE",
            Tab::Status => "STATUS",
            Tab::Special => "SPECIAL",
            Tab::Environmental => "ENVIRONMENTAL",
        }
    }

    pub fn capsule(self) -> Option<Capsule> {
        match self {
            Tab::Aura => None,
            Tab::Attack => Some(Capsule::Attack),
            Tab::Defense => Some(Capsule::Defence),
            Tab::Erase => Some(Capsule::Erase),
            Tab::Status => Some(Capsule::Status),
            Tab::Special => Some(Capsule::Special),
            Tab::Environmental => Some(Capsule::Environment),
        }
    }

    fn step(self, forward: bool) -> Tab {
        let i = Tab::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Tab::ALL[(i + if forward { 1 } else { Tab::ALL.len() - 1 }) % Tab::ALL.len()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    /// "Finish editing your arsenal?" (ENDCHECK)
    Finish,
    /// "Reflect changes to your arsenal?" (SAVEEND)
    Reflect,
    /// "Undo edits made to your arsenal?" (RETDECK)
    Undo,
    /// "Arsenal " %s " Do you want to disassemble it?"
    Disassemble(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameFor {
    New,
    Rename(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KindFor {
    /// Creating an arsenal with this name.
    New(String),
    Change(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsOf {
    Arsenal(usize),
    Stock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    Confirm { what: Confirm, cursor: usize },
    Notice(String),
    Name { for_: NameFor, buf: String, caps: bool, cell: usize },
    Kind { for_: KindFor, cursor: usize },
    Options { of: OptionsOf, cursor: usize },
    Sort { row: usize },
    Mask { cursor: usize },
    Mark { cursor: usize },
    MarkSchool { cursor: usize },
}

/// Name entry: an on-screen keyboard so a pad can type (the original has
/// one too: Select / Backspace / Caps / Confirm Selection). 40 character
/// cells in 4 rows of 10, then Backspace, Caps, OK.
pub const KB_ROWS: [&str; 4] = ["ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZ0123", "456789-.' "];
pub const KB_CHARS: usize = 40;
pub const KB_BACKSPACE: usize = 40;
pub const KB_CAPS: usize = 41;
pub const KB_OK: usize = 42;

pub fn kb_char(cell: usize, caps: bool) -> Option<char> {
    let c = KB_ROWS.get(cell / 10)?.chars().nth(cell % 10)?;
    Some(if caps { c } else { c.to_ascii_lowercase() })
}

pub fn kb_label(cell: usize, caps: bool) -> String {
    match cell {
        KB_BACKSPACE => "Backspace".into(),
        KB_CAPS => if caps { "Caps ON" } else { "Caps off" }.into(),
        KB_OK => "OK".into(),
        _ => match kb_char(cell, caps) {
            Some(' ') => "Space".into(),
            Some(c) => c.to_string(),
            None => String::new(),
        },
    }
}

fn kb_move(cell: usize, dx: i32, dy: i32) -> usize {
    const ACTION_OF_COL: [usize; 10] = [0, 0, 0, 0, 1, 1, 1, 2, 2, 2];
    const COL_OF_ACTION: [usize; 3] = [1, 5, 8];
    if cell >= KB_CHARS {
        let a = cell - KB_CHARS;
        if dy < 0 {
            return 30 + COL_OF_ACTION[a.min(2)];
        }
        if dy > 0 {
            return COL_OF_ACTION[a.min(2)];
        }
        return KB_CHARS + (a as i32 + dx).rem_euclid(3) as usize;
    }
    let (r, c) = (cell / 10, cell % 10);
    if dx != 0 {
        return r * 10 + (c as i32 + dx).rem_euclid(10) as usize;
    }
    match dy.signum() {
        1 if r == 3 => KB_CHARS + ACTION_OF_COL[c],
        1 => cell + 10,
        -1 if r == 0 => KB_CHARS + ACTION_OF_COL[c],
        -1 => cell - 10,
        _ => cell,
    }
}

/// The arsenal being edited: a working copy, compared with the snapshot
/// taken when it was opened to know whether Back must ask.
#[derive(Clone, Debug)]
pub struct Edit {
    pub index: usize,
    pub name: String,
    pub kind: u8,
    pub deck: Vec<i16>,
    snapshot: (String, u8, Vec<i16>),
    pub slot: usize,
    pub stock_cursor: usize,
    pub tab: Tab,
    pub focus: Focus,
}

impl Edit {
    pub fn dirty(&self) -> bool {
        self.name != self.snapshot.0 || self.kind != self.snapshot.1 || self.deck != self.snapshot.2
    }
}

/// The whole editor, free of Bevy so every path is unit-tested.
#[derive(Clone, Debug)]
pub struct Core {
    pub screen: Screen,
    pub list_cursor: usize,
    pub preview_page: usize,
    pub edit: Option<Edit>,
    pub dialog: Option<Dialog>,
    pub prefs: BuilderPrefs,
    pub mark: RuleMark,
    pub message: String,
    /// Sound cues to play this frame (the system drains them).
    pub cues: Vec<Cue>,
    /// Go back to the title screen.
    pub leave: bool,
    /// The saved list changed: write `arsenals.json`.
    pub saved_changed: bool,
    pub prefs_changed: bool,
    seed: u64,
}

impl Default for Core {
    fn default() -> Self {
        Core { screen: Screen::List, list_cursor: 0, preview_page: 0, edit: None, dialog: None, prefs: BuilderPrefs::default(), mark: RuleMark::None, message: String::new(), cues: Vec::new(), leave: false, saved_changed: false, prefs_changed: false, seed: 0 }
    }
}

const NOTICE_COPIES: &str = "You cannot place more than 3 of the same skill in an arsenal.";

fn sort_value(key: SortKey, def: &SkillDef, deck: &[i16], prefs: &BuilderPrefs) -> i64 {
    match key {
        SortKey::None => 0,
        SortKey::School => def.school() as i64,
        SortKey::Cost => def.cost as i64,
        SortKey::Strength => (if def.capsule == Capsule::Defence { def.defence } else { def.strength }) as i64,
        // 0 uses = unlimited: last.
        SortKey::Uses => match def.raw[pd_formats::skills::field::USES] {
            0 => i64::MAX,
            n => n as i64,
        },
        // The range band (s44) exists for aimed attacks only (unconfirmed as a "Range" sort).
        SortKey::Range => {
            if def.capsule == Capsule::Attack {
                def.raw[pd_formats::skills::field::RANGE_BAND] as i64
            } else {
                0
            }
        }
        SortKey::Id => def.id as i64,
        SortKey::Rarity => def.raw[pd_formats::skills::field::RARITY] as i64,
        // Most copies first.
        SortKey::Amount => -(arsenal::copies(deck, def.id) as i64),
        SortKey::Favorite => i64::from(!prefs.is_favourite(def.id)),
    }
}

impl Core {
    fn list_len(&self, saved: &[SavedArsenal]) -> usize {
        saved.len() + usize::from(saved.len() < MAX_ARSENALS)
    }

    /// Ids shown in the stock list for the current tab, filtered by the
    /// school mask and ordered by the three sort keys (stable: the catalogue
    /// order breaks ties).
    pub fn stock_ids(&self, skills: &[SkillDef]) -> Vec<i16> {
        let Some(e) = &self.edit else { return vec![] };
        let Some(cap) = e.tab.capsule() else { return vec![AURA_PARTICLE] };
        // School-less skills (the enemy/object skills 701-741) are not
        // collectable: the program's own arsenal counts treat them as empty slots.
        let mut list: Vec<&SkillDef> = skills.iter().filter(|s| s.capsule == cap && (1..=5).contains(&s.school()) && !self.prefs.hidden[s.school() as usize - 1]).collect();
        let keys = self.prefs.sort;
        list.sort_by(|a, b| {
            let mut ord = std::cmp::Ordering::Equal;
            for k in keys {
                ord = ord.then(sort_value(k, a, &e.deck, &self.prefs).cmp(&sort_value(k, b, &e.deck, &self.prefs)));
            }
            ord
        });
        list.into_iter().map(|s| s.id).collect()
    }

    fn cue(&mut self, c: Cue) {
        self.cues.push(c);
    }

    fn notice(&mut self, text: impl Into<String>) {
        self.dialog = Some(Dialog::Notice(text.into()));
    }

    fn to_list(&mut self) {
        self.screen = Screen::List;
        self.edit = None;
    }

    fn open_edit(&mut self, idx: usize, skills: &[SkillDef], saved: &[SavedArsenal]) {
        let Some(a) = saved.get(idx) else { return };
        let mut deck = a.skills.clone();
        arsenal::compact(&mut deck);
        let kind = a.kind_or_default(skills);
        self.edit = Some(Edit { index: idx, name: a.name.clone(), kind, snapshot: (a.name.clone(), kind, deck.clone()), deck, slot: 0, stock_cursor: 0, tab: Tab::Attack, focus: Focus::Slots });
        self.screen = Screen::Edit;
    }

    /// "Reflect changes": the school limit is checked here, as the original
    /// does (SYSMAX: "This arsenal has skills from too many schools. You are
    /// limited to %d schools."). Returns whether it saved.
    fn try_save(&mut self, skills: &[SkillDef], saved: &mut [SavedArsenal]) -> bool {
        let Some(e) = &mut self.edit else { return false };
        if let Some(p) = arsenal::check_kind(&e.deck, skills, e.kind).into_iter().find(|p| matches!(p, arsenal::Problem::TooManySchools { .. })) {
            let text = p.describe(skills);
            self.notice(text);
            return false;
        }
        let Some(slot) = saved.get_mut(e.index) else { return false };
        *slot = SavedArsenal { name: e.name.clone(), skills: e.deck.clone(), kind: Some(e.kind) };
        e.snapshot = (e.name.clone(), e.kind, e.deck.clone());
        self.saved_changed = true;
        self.message = "Saved.".into();
        true
    }

    /// Feed one button press. `skills` is the whole catalogue; `saved` the
    /// profile's arsenals (edited in place, `saved_changed` tells the caller
    /// to persist).
    pub fn handle(&mut self, p: Pulse, skills: &[SkillDef], saved: &mut Vec<SavedArsenal>) {
        self.message.clear();
        match p {
            Pulse::Up | Pulse::Down => self.cue(Cue::Move),
            // READ `FUN_140161fa0`: left/right page switch plays common/4; LB/RB filter tabs are the same kind of switch.
            Pulse::Left | Pulse::Right | Pulse::Lb | Pulse::Rb => self.cue(Cue::List),
            Pulse::B => self.cue(Cue::Back),
            // READ `FUN_14018eb40`: name entry plays common/34 per character, common/11 per delete.
            Pulse::Char(_) => self.cue(Cue::Spinner),
            Pulse::Backspace => self.cue(Cue::Delete),
            _ => self.cue(Cue::Confirm),
        }
        if self.dialog.is_some() {
            self.dialog_input(p, skills, saved);
            return;
        }
        match self.screen {
            Screen::List => self.list_input(p, skills, saved),
            Screen::Edit => self.edit_input(p, skills, saved),
        }
    }

    fn start_new(&mut self, saved: &[SavedArsenal]) {
        if saved.len() >= MAX_ARSENALS {
            self.notice(format!("You cannot have more than {MAX_ARSENALS} arsenals."));
        } else {
            self.dialog = Some(Dialog::Name { for_: NameFor::New, buf: arsenals::default_name(saved), caps: true, cell: 0 });
        }
    }

    fn list_input(&mut self, p: Pulse, skills: &[SkillDef], saved: &mut Vec<SavedArsenal>) {
        let n = self.list_len(saved);
        match p {
            Pulse::Up => {
                self.list_cursor = (self.list_cursor + n - 1) % n;
                self.preview_page = 0;
            }
            Pulse::Down => {
                self.list_cursor = (self.list_cursor + 1) % n;
                self.preview_page = 0;
            }
            Pulse::Left => self.preview_page = self.preview_page.saturating_sub(1),
            Pulse::Right => self.preview_page = (self.preview_page + 1).min(ARSENAL_SIZE / VISIBLE - 1),
            Pulse::A => {
                if self.list_cursor < saved.len() {
                    self.open_edit(self.list_cursor, skills, saved);
                } else {
                    self.start_new(saved);
                }
            }
            Pulse::X => self.start_new(saved),
            Pulse::Y if self.list_cursor < saved.len() => self.dialog = Some(Dialog::Options { of: OptionsOf::Arsenal(self.list_cursor), cursor: 0 }),
            Pulse::B => self.leave = true,
            _ => {}
        }
    }

    /// Back from the editor: straight out when nothing changed, otherwise
    /// the original's chain of questions.
    fn request_back(&mut self) {
        match &self.edit {
            Some(e) if e.dirty() => self.dialog = Some(Dialog::Confirm { what: Confirm::Finish, cursor: 0 }),
            _ => self.to_list(),
        }
    }

    fn edit_input(&mut self, p: Pulse, skills: &[SkillDef], saved: &mut Vec<SavedArsenal>) {
        if p == Pulse::Start {
            self.try_save(skills, saved);
            return;
        }
        let stock_len = self.stock_ids(skills).len();
        let Some(e) = &mut self.edit else { return };
        match (e.focus, p) {
            (_, Pulse::Lb) | (_, Pulse::Rb) => {
                e.tab = e.tab.step(p == Pulse::Rb);
                e.stock_cursor = 0;
            }
            (Focus::Slots, Pulse::Up) => e.slot = (e.slot + ARSENAL_SIZE - 1) % ARSENAL_SIZE,
            (Focus::Slots, Pulse::Down) => e.slot = (e.slot + 1) % ARSENAL_SIZE,
            (Focus::Slots, Pulse::Left) => e.slot = e.slot.saturating_sub(VISIBLE),
            (Focus::Slots, Pulse::Right) => e.slot = (e.slot + VISIBLE).min(ARSENAL_SIZE - 1),
            // Replace: pick the replacement from the stock.
            (Focus::Slots, Pulse::A) => e.focus = Focus::Stock,
            (Focus::Slots, Pulse::X) => {
                let slot = e.slot;
                let _ = arsenal::clear_slot(&mut e.deck, slot);
            }
            (Focus::Slots, Pulse::Y) => {
                let slot = e.slot;
                match arsenal::add_same(&mut e.deck, slot) {
                    Err(PlaceError::TooManyCopies) => self.notice(NOTICE_COPIES),
                    Err(PlaceError::Full) => self.notice("There is no empty slot left in this arsenal."),
                    _ => {}
                }
            }
            (Focus::Slots, Pulse::B) => self.request_back(),
            (Focus::Stock, Pulse::Up) if stock_len > 0 => e.stock_cursor = (e.stock_cursor + stock_len - 1) % stock_len,
            (Focus::Stock, Pulse::Down) if stock_len > 0 => e.stock_cursor = (e.stock_cursor + 1) % stock_len,
            (Focus::Stock, Pulse::Left) => e.stock_cursor = e.stock_cursor.saturating_sub(VISIBLE),
            (Focus::Stock, Pulse::Right) if stock_len > 0 => e.stock_cursor = (e.stock_cursor + VISIBLE).min(stock_len - 1),
            (Focus::Stock, Pulse::A) => {
                let id = {
                    let cur = e.stock_cursor;
                    self.stock_ids(skills).get(cur).copied()
                };
                let Some(id) = id else { return };
                let Some(e) = &mut self.edit else { return };
                let slot = e.slot;
                let was_empty = e.deck[slot] == AURA_PARTICLE;
                match arsenal::place(&mut e.deck, slot, id) {
                    Ok(()) => {
                        e.focus = Focus::Slots;
                        // Chain straight on to the next empty slot when filling.
                        if was_empty && id != AURA_PARTICLE {
                            e.slot = e.deck.iter().position(|&x| x == AURA_PARTICLE).unwrap_or(ARSENAL_SIZE - 1);
                        }
                    }
                    Err(PlaceError::TooManyCopies) => self.notice(NOTICE_COPIES),
                    Err(_) => {}
                }
            }
            (Focus::Stock, Pulse::X) => {
                let cur = e.stock_cursor;
                if let Some(id) = self.stock_ids(skills).get(cur).copied().filter(|&i| i != AURA_PARTICLE) {
                    let now = self.prefs.toggle_favourite(id);
                    self.prefs_changed = true;
                    self.message = if now { "Favorite set.".into() } else { "Favorite cleared.".into() };
                }
            }
            (Focus::Stock, Pulse::Y) => self.dialog = Some(Dialog::Options { of: OptionsOf::Stock, cursor: 0 }),
            (Focus::Stock, Pulse::B) => e.focus = Focus::Slots,
            _ => {}
        }
    }

    fn dialog_input(&mut self, p: Pulse, skills: &[SkillDef], saved: &mut Vec<SavedArsenal>) {
        let Some(d) = self.dialog.take() else { return };
        let vertical = |cursor: usize, len: usize| match p {
            Pulse::Up => (cursor + len - 1) % len,
            Pulse::Down => (cursor + 1) % len,
            _ => cursor,
        };
        self.dialog = match d {
            Dialog::Notice(t) => match p {
                Pulse::A | Pulse::B => None,
                _ => Some(Dialog::Notice(t)),
            },
            Dialog::Confirm { what, cursor } => {
                let cursor = if matches!(p, Pulse::Up | Pulse::Down | Pulse::Left | Pulse::Right) { 1 - cursor } else { cursor };
                match p {
                    Pulse::B => None,
                    Pulse::A => match (&what, cursor) {
                        (Confirm::Finish, 0) => Some(Dialog::Confirm { what: Confirm::Reflect, cursor: 0 }),
                        (Confirm::Reflect, 0) => {
                            if self.try_save(skills, saved) {
                                self.to_list();
                                None
                            } else {
                                self.dialog.take()
                            }
                        }
                        (Confirm::Reflect, _) => Some(Dialog::Confirm { what: Confirm::Undo, cursor: 0 }),
                        (Confirm::Undo, 0) => {
                            self.to_list();
                            None
                        }
                        (Confirm::Disassemble(i), 0) => {
                            if *i < saved.len() {
                                saved.remove(*i);
                                self.saved_changed = true;
                                self.list_cursor = self.list_cursor.min(saved.len().saturating_sub(1)).min(self.list_len(saved) - 1);
                                self.message = "Disassembled.".into();
                            }
                            None
                        }
                        _ => None,
                    },
                    _ => Some(Dialog::Confirm { what, cursor }),
                }
            }
            Dialog::Name { for_, mut buf, mut caps, mut cell } => {
                let confirm = |buf: &str| arsenals::clean_name(buf);
                let mut done = false;
                match p {
                    Pulse::Char(c) => {
                        if !c.is_control() && buf.chars().count() < MAX_NAME_LEN {
                            buf.push(c);
                        }
                    }
                    Pulse::Backspace | Pulse::X => {
                        buf.pop();
                    }
                    Pulse::Y => caps = !caps,
                    Pulse::Up => cell = kb_move(cell, 0, -1),
                    Pulse::Down => cell = kb_move(cell, 0, 1),
                    Pulse::Left => cell = kb_move(cell, -1, 0),
                    Pulse::Right => cell = kb_move(cell, 1, 0),
                    Pulse::Start => done = true,
                    Pulse::A => match cell {
                        KB_BACKSPACE => {
                            buf.pop();
                        }
                        KB_CAPS => caps = !caps,
                        KB_OK => done = true,
                        _ => {
                            if let Some(c) = kb_char(cell, caps) {
                                if buf.chars().count() < MAX_NAME_LEN {
                                    buf.push(c);
                                }
                            }
                        }
                    },
                    Pulse::B => {
                        self.dialog = None;
                        return;
                    }
                    _ => {}
                }
                if done {
                    let name = confirm(&buf);
                    match for_ {
                        NameFor::New => Some(Dialog::Kind { for_: KindFor::New(name), cursor: (arsenal::DEFAULT_KIND - 1) as usize }),
                        NameFor::Rename(i) => {
                            if let Some(a) = saved.get_mut(i) {
                                a.name = name;
                                self.saved_changed = true;
                            }
                            None
                        }
                    }
                } else {
                    Some(Dialog::Name { for_, buf, caps, cell })
                }
            }
            Dialog::Kind { for_, cursor } => {
                let cursor = vertical(cursor, KINDS.len());
                match p {
                    Pulse::B => None,
                    Pulse::A => {
                        let kind = KINDS[cursor];
                        match for_ {
                            KindFor::New(name) => {
                                saved.push(SavedArsenal { name, skills: arsenal::empty_deck(), kind: Some(kind) });
                                self.saved_changed = true;
                                let idx = saved.len() - 1;
                                self.list_cursor = idx;
                                self.open_edit(idx, skills, saved);
                                None
                            }
                            KindFor::Change(i) => {
                                let have = saved.get(i).map_or(0, |a| arsenal::schools(&a.skills, skills).len());
                                if have > kind as usize {
                                    self.notice(arsenal::Problem::TooManySchools { have, limit: kind as usize }.describe(skills));
                                } else if let Some(a) = saved.get_mut(i) {
                                    a.kind = Some(kind);
                                    self.saved_changed = true;
                                }
                                self.dialog.take()
                            }
                        }
                    }
                    _ => Some(Dialog::Kind { for_, cursor }),
                }
            }
            Dialog::Options { of, cursor } => {
                let len = if of == OptionsOf::Stock { 3 } else { 4 };
                let cursor = vertical(cursor, len);
                match p {
                    Pulse::B => None,
                    Pulse::A => match (of, cursor) {
                        (OptionsOf::Arsenal(i), 0) => saved.get(i).map(|a| Dialog::Name { for_: NameFor::Rename(i), buf: a.name.clone(), caps: true, cell: 0 }),
                        (OptionsOf::Arsenal(i), 1) => saved.get(i).map(|a| Dialog::Kind { for_: KindFor::Change(i), cursor: (a.kind_or_default(skills) - 1) as usize }),
                        (OptionsOf::Arsenal(i), 2) => {
                            // Randomize: open it with a random legal deck, unsaved until Start.
                            self.open_edit(i, skills, saved);
                            self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
                            let seed = self.seed;
                            if let Some(e) = &mut self.edit {
                                e.deck = randomize(seed, skills, e.kind);
                            }
                            self.message = "Randomized. Start to keep it.".into();
                            None
                        }
                        (OptionsOf::Arsenal(i), _) => Some(Dialog::Confirm { what: Confirm::Disassemble(i), cursor: 1 }),
                        (OptionsOf::Stock, 0) => Some(Dialog::Sort { row: 0 }),
                        (OptionsOf::Stock, 1) => Some(Dialog::Mask { cursor: 0 }),
                        (OptionsOf::Stock, _) => Some(Dialog::Mark { cursor: 0 }),
                    },
                    _ => Some(Dialog::Options { of, cursor }),
                }
            }
            Dialog::Sort { row } => {
                let row = vertical(row, 3);
                match p {
                    Pulse::A | Pulse::B => {
                        self.reset_stock_cursor();
                        None
                    }
                    Pulse::Left | Pulse::Right => {
                        self.prefs.sort[row] = self.prefs.sort[row].step(p == Pulse::Right);
                        self.prefs_changed = true;
                        Some(Dialog::Sort { row })
                    }
                    // "Default": clear the three conditions.
                    Pulse::X => {
                        self.prefs.sort = [SortKey::None; 3];
                        self.prefs_changed = true;
                        Some(Dialog::Sort { row })
                    }
                    _ => Some(Dialog::Sort { row }),
                }
            }
            Dialog::Mask { cursor } => {
                let cursor = vertical(cursor, 5);
                match p {
                    Pulse::A => {
                        self.prefs.hidden[cursor] = !self.prefs.hidden[cursor];
                        self.prefs_changed = true;
                        Some(Dialog::Mask { cursor })
                    }
                    Pulse::B => {
                        self.reset_stock_cursor();
                        None
                    }
                    _ => Some(Dialog::Mask { cursor }),
                }
            }
            Dialog::Mark { cursor } => {
                let cursor = vertical(cursor, 5);
                match p {
                    Pulse::A => match cursor {
                        1 => Some(Dialog::MarkSchool { cursor: 0 }),
                        c => {
                            self.mark = [RuleMark::None, RuleMark::None, RuleMark::Highlander, RuleMark::PunyOnly, RuleMark::HeavyOnly][c];
                            None
                        }
                    },
                    Pulse::B => None,
                    _ => Some(Dialog::Mark { cursor }),
                }
            }
            Dialog::MarkSchool { cursor } => {
                let cursor = vertical(cursor, 5);
                match p {
                    Pulse::A => {
                        self.mark = RuleMark::SingleSchool(cursor as i16 + 1);
                        None
                    }
                    Pulse::B => Some(Dialog::Mark { cursor: 1 }),
                    _ => Some(Dialog::MarkSchool { cursor }),
                }
            }
        };
    }

    fn reset_stock_cursor(&mut self) {
        if let Some(e) = &mut self.edit {
            e.stock_cursor = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// Randomize (house feature; not in the original)
// ---------------------------------------------------------------------------

/// Tiny seeded xorshift64* generator -- deterministic (same seed, same
/// arsenal), local to this file, not shared with the sim's own RNG.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n` (n > 0).
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Skills in a random arsenal, like the original's own Quick decks (16 skills
/// and 14 Aura Particles, at most 2 copies: `gsdata` 0x33804).
const RANDOM_SKILLS: usize = 16;
const RANDOM_COPIES: usize = 2;

/// A random arsenal that is legal for `kind` schools: up to `kind` schools
/// present in `catalog`, [`RANDOM_SKILLS`] skills from them (fewer if the
/// pool is smaller), at most two copies each, the rest Aura Particles.
/// Seeded: the same seed always gives the same arsenal.
pub fn randomize(seed: u64, catalog: &[SkillDef], kind: u8) -> Vec<i16> {
    let mut rng = Rng(seed | 1);
    let mut present: Vec<i16> = catalog.iter().map(|s| s.school()).filter(|&s| (1..=5).contains(&s)).collect();
    present.sort_unstable();
    present.dedup();
    let mut deck = Vec::new();
    if !present.is_empty() {
        for i in (1..present.len()).rev() {
            let j = rng.below(i + 1);
            present.swap(i, j);
        }
        present.truncate((kind.max(1) as usize).min(present.len()));
        let pool: Vec<&SkillDef> = catalog.iter().filter(|s| present.contains(&s.school())).collect();
        let mut counts = std::collections::HashMap::<i16, usize>::new();
        // Bounded attempts: a small pool would otherwise spin forever.
        for _ in 0..ARSENAL_SIZE * 50 {
            if deck.len() >= RANDOM_SKILLS.min(pool.len() * RANDOM_COPIES) {
                break;
            }
            let pick = pool[rng.below(pool.len())];
            let c = counts.entry(pick.id).or_insert(0);
            if *c < RANDOM_COPIES.min(MAX_COPIES) {
                *c += 1;
                deck.push(pick.id);
            }
        }
    }
    arsenal::compact(&mut deck);
    deck
}

// ---------------------------------------------------------------------------
// View model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Empty,
    /// A row of the focused column.
    Focus,
    /// The cursor row of the focused column.
    Cursor,
    /// A row of the unfocused column.
    Dim,
    /// The cursor row of the unfocused column.
    DimCursor,
}

#[derive(Clone, Debug)]
pub struct RowView {
    pub tab: String,
    pub name: String,
    pub right: String,
    pub color: Color,
    pub tone: Tone,
}

impl RowView {
    fn empty() -> Self {
        RowView { tab: String::new(), name: String::new(), right: String::new(), color: ui::ROW_TEXT, tone: Tone::Empty }
    }
}

#[derive(Clone, Debug)]
pub struct DialogView {
    pub title: String,
    pub body: String,
    pub items: Vec<String>,
    pub cursor: usize,
    /// Name entry: (cell labels, cursor cell).
    pub keyboard: Option<(Vec<String>, usize)>,
    pub hint: String,
}

#[derive(Clone, Debug)]
pub struct View {
    pub header: String,
    pub header_color: Color,
    pub captions: [(String, bool); 2],
    pub infos: [String; 2],
    pub rows: [Vec<RowView>; 2],
    pub tabs: String,
    pub card: String,
    pub detail: String,
    pub problems: (String, Color),
    pub prompt: String,
    pub keys: String,
    pub dialog: Option<DialogView>,
}

fn capsule_name(c: Capsule) -> &'static str {
    match c {
        Capsule::Attack => "Attack",
        Capsule::Defence => "Defence",
        Capsule::Erase => "Erase",
        Capsule::Status => "Status",
        Capsule::Special => "Special",
        Capsule::Environment => "Environment",
    }
}

fn capsule_short(c: Capsule) -> &'static str {
    match c {
        Capsule::Attack => "ATK",
        Capsule::Defence => "DEF",
        Capsule::Erase => "ERS",
        Capsule::Status => "STS",
        Capsule::Special => "SPC",
        Capsule::Environment => "ENV",
    }
}

/// Capsule colours as the HUD uses them (docs/HUD.md; assumed mapping).
fn capsule_color(c: Capsule) -> Color {
    match c {
        Capsule::Attack => Color::srgb(1.0, 0.45, 0.4),
        Capsule::Defence => Color::srgb(0.5, 0.7, 1.0),
        Capsule::Erase => Color::srgb(0.4, 0.95, 1.0),
        Capsule::Status => Color::srgb(1.0, 0.5, 0.95),
        Capsule::Special => Color::srgb(1.0, 0.9, 0.35),
        Capsule::Environment => Color::srgb(0.5, 1.0, 0.5),
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn school_name(s: i16) -> String {
    capitalise(SCHOOL_NAMES.get(s as usize).copied().unwrap_or("?"))
}

/// Same content as the in-match D-pad skill card (`hud::skill_card`),
/// rebuilt here since that function is private to `hud.rs`.
fn skill_card(def: &SkillDef, notes: &std::collections::HashMap<i16, String>) -> String {
    if def.id == AURA_PARTICLE {
        return "Aura Particle\nFills an empty slot. The more of them, the faster your Level climbs.".to_string();
    }
    let mut stats = format!("Cost {}", def.cost);
    if def.capsule == Capsule::Defence {
        if def.defence > 0 {
            stats += &format!("    DEF {}", def.defence);
        }
    } else if def.strength > 0 && !matches!(def.strength_rule, 17 | 18 | 20) {
        stats += &format!("    STR {}", def.strength);
    }
    // The original's card ends its stats row with the range icon (short / med / long / self / ...): `hud_card.rs`.
    if let Some(range) = crate::hud_card::range_word(def) {
        stats += &format!("    Range {range}");
    }
    let note = notes.get(&def.id).map(String::as_str).unwrap_or("");
    format!("{}\n{} · {}\n{stats}\n{note}", def.name, school_name(def.school()), capsule_name(def.capsule))
}

/// First row of the visible window so the cursor stays in view.
pub fn window_start(cursor: usize, len: usize) -> usize {
    if len <= VISIBLE {
        0
    } else {
        cursor.saturating_sub(VISIBLE / 2).min(len - VISIBLE)
    }
}

fn kind_line(kind: u8) -> String {
    format!("{kind} school{}  (Aura recovery x{:.1})", if kind == 1 { "" } else { "s" }, arsenal::aura_handicap(kind))
}

/// The "ARSENAL DATA / DETAIL" panel (`FUN_14015a8d0` counts, `arsenaledit`
/// labels) plus the rules the arsenal qualifies for: the arsenal's name and
/// kind line, then [`detail_body`]. The list page shows this; the edit page's
/// header already carries the name and kind, so it shows only the body.
fn detail_text(name: &str, kind: u8, deck: &[i16], skills: &[SkillDef]) -> String {
    format!("{name}\n{}\n{}", kind_line(kind), detail_body(deck, skills))
}

/// Pairs of `label count` on lines of two, leaving out the zero counts
/// (Laurence, 2026-10-09: de-clutter; UI-DECLUTTER-PROPOSALS.md item 11).
fn count_lines(items: &[(&str, usize)]) -> String {
    let shown: Vec<String> = items.iter().filter(|(_, n)| *n > 0).map(|(l, n)| format!("{l} {n}")).collect();
    if shown.is_empty() {
        return "  none".into();
    }
    shown.chunks(2).map(|c| format!("  {}", c.join("    "))).collect::<Vec<_>>().join("\n")
}

/// The detail panel without its name / kind head: schools in use, the non-zero
/// TYPE and SCHOOL counts, and the rules the arsenal qualifies for.
fn detail_body(deck: &[i16], skills: &[SkillDef]) -> String {
    let d = arsenal::detail(deck, skills);
    let used: Vec<String> = arsenal::schools(deck, skills).into_iter().map(school_name).collect();
    let rules: Vec<&str> = arsenal::eligible_rules(deck, skills).into_iter().map(|r| arsenal::RULE_NAMES[r]).collect();
    let t = &d.by_type;
    let types = count_lines(&[("Attack", t[0]), ("Defense", t[1]), ("Erase", t[2]), ("Status", t[3]), ("Special", t[4]), ("Environmental", t[5]), ("Aura Particles", d.aura)]);
    let sc = &d.by_school;
    let schools = count_lines(&[("Psycho", sc[0]), ("Optical", sc[1]), ("Nature", sc[2]), ("Ki", sc[3]), ("Faith", sc[4])]);
    format!(
        "Using: {}\n\nTYPE\n{types}\n\nSCHOOL\n{schools}\n\nQualifies for: {}",
        if used.is_empty() { "no school yet".to_string() } else { used.join(", ") },
        if rules.is_empty() { "nothing yet (add a skill)".to_string() } else { rules.join(", ") },
    )
}

fn slot_row(i: usize, id: i16, skills: &[SkillDef], tone: Tone) -> RowView {
    let (name, right, color) = if id == AURA_PARTICLE {
        ("AURA PARTICLE".to_string(), "AURA".to_string(), ui::DIM)
    } else {
        match skills.iter().find(|s| s.id == id) {
            Some(s) => (s.name.to_uppercase(), capsule_short(s.capsule).to_string(), capsule_color(s.capsule)),
            None => (format!("#{id}"), "?".to_string(), ui::BAD),
        }
    };
    RowView { tab: format!("{:02}", i + 1), name, right, color, tone }
}

fn tone_of(focused: bool, on_cursor: bool) -> Tone {
    match (focused, on_cursor) {
        (true, true) => Tone::Cursor,
        (true, false) => Tone::Focus,
        (false, true) => Tone::DimCursor,
        (false, false) => Tone::Dim,
    }
}

impl Core {
    /// Everything the screen shows right now.
    pub fn view(&self, cat: &SkillCatalog, saved: &[SavedArsenal]) -> View {
        let skills = &cat.skills[..];
        let mut v = View {
            header: String::new(),
            header_color: ui::TEAL,
            captions: [(String::new(), false), (String::new(), false)],
            infos: [String::new(), String::new()],
            rows: [vec![RowView::empty(); 0], vec![]],
            tabs: String::new(),
            card: String::new(),
            detail: String::new(),
            problems: (String::new(), ui::TEAL),
            prompt: String::new(),
            keys: String::new(),
            dialog: None,
        };
        match (self.screen, &self.edit) {
            (Screen::Edit, Some(e)) => self.view_edit(&mut v, e, cat, skills),
            _ => self.view_list(&mut v, cat, saved),
        }
        v.dialog = self.dialog.as_ref().map(|d| self.dialog_view(d, saved));
        v
    }

    fn view_list(&self, v: &mut View, cat: &SkillCatalog, saved: &[SavedArsenal]) {
        let skills = &cat.skills[..];
        let n = self.list_len(saved);
        let start = window_start(self.list_cursor, n);
        v.header = if self.message.is_empty() { format!("{} / {MAX_ARSENALS} arsenals", saved.len()) } else { self.message.clone() };
        v.captions = [("ARSENAL LIST".into(), true), ("CONTENTS".into(), false)];
        let mut left = Vec::new();
        for slot in 0..VISIBLE {
            let i = start + slot;
            let tone = tone_of(true, i == self.list_cursor);
            left.push(match saved.get(i) {
                Some(a) => RowView { tab: format!("{:02}", i + 1), name: a.name.to_uppercase(), right: format!("{} SCHOOL{}", a.kind_or_default(skills), if a.kind_or_default(skills) == 1 { "" } else { "S" }), color: ui::ROW_TEXT, tone },
                None if i < n => RowView { tab: "+".into(), name: "NEW ARSENAL".into(), right: String::new(), color: ui::TEAL, tone },
                None => RowView::empty(),
            });
        }
        let cur = saved.get(self.list_cursor);
        let mut right = Vec::new();
        let page = self.preview_page;
        for slot in 0..VISIBLE {
            let i = page * VISIBLE + slot;
            right.push(match cur {
                Some(a) => {
                    let mut deck = a.skills.clone();
                    arsenal::compact(&mut deck);
                    slot_row(i, deck[i], skills, Tone::Dim)
                }
                None => RowView::empty(),
            });
        }
        v.rows = [left, right];
        v.infos[1] = if cur.is_some() { format!("slots {}-{}", page * VISIBLE + 1, page * VISIBLE + VISIBLE) } else { String::new() };
        if let Some(a) = cur {
            let mut deck = a.skills.clone();
            arsenal::compact(&mut deck);
            let kind = a.kind_or_default(skills);
            v.detail = detail_text(&a.name, kind, &deck, skills);
            let problems = arsenal::check_kind(&deck, skills, kind);
            v.problems = if problems.is_empty() { ("Legal arsenal.".into(), ui::TEAL) } else { (problems.iter().map(|p| p.describe(skills)).collect::<Vec<_>>().join("\n"), ui::BAD) };
        } else {
            v.detail = "NEW ARSENAL\nPress A or X to make one.\nYou pick a name, then how many schools it may use.".into();
        }
        v.prompt = "A: Open    X: New    Y: Options    Left/Right: Page    B: Back".into();
        v.keys = "Keyboard   W/S: move   Enter: A   Esc: B   X   Y   Ctrl+S or Tab: Start".into();
    }

    fn view_edit(&self, v: &mut View, e: &Edit, cat: &SkillCatalog, skills: &[SkillDef]) {
        let stock = self.stock_ids(skills);
        let sc = e.stock_cursor.min(stock.len().saturating_sub(1));
        let stock_focus = e.focus == Focus::Stock;
        v.header = {
            let base = format!("{}   {}{}", e.name, kind_line(e.kind), if e.dirty() { "   [unsaved]" } else { "" });
            if self.message.is_empty() { base } else { format!("{base}   {}", self.message) }
        };
        v.captions = [(format!("STOCK: {}", e.tab.label()), stock_focus), (format!("ARSENAL {}/{ARSENAL_SIZE}", e.deck.iter().filter(|&&x| x != AURA_PARTICLE).count()), !stock_focus)];
        let mask_note = if self.mark == RuleMark::None { String::new() } else { format!("   mark: {}", self.mark.label()) };
        v.infos[0] = format!("{}/{}{mask_note}", if stock.is_empty() { 0 } else { sc + 1 }, stock.len());
        v.infos[1] = format!("slot {}/{ARSENAL_SIZE}", e.slot + 1);
        v.tabs = Tab::ALL.iter().map(|&t| if t == e.tab { format!("[{}]", t.label()) } else { t.label().to_string() }).collect::<Vec<_>>().join("  ");
        let start = window_start(sc, stock.len());
        let mut left = Vec::new();
        for slot in 0..VISIBLE {
            let i = start + slot;
            let tone = tone_of(stock_focus, i == sc);
            left.push(match stock.get(i) {
                Some(&id) if id == AURA_PARTICLE => RowView { tab: String::new(), name: "AURA PARTICLE".into(), right: String::new(), color: ui::WHITE, tone },
                Some(&id) => match skills.iter().find(|s| s.id == id) {
                    Some(s) => {
                        let marked = self.mark.marks(s, &e.deck);
                        let fav = self.prefs.is_favourite(id);
                        let copies = arsenal::copies(&e.deck, id);
                        let right = format!("{}{}{}", if copies > 0 { format!("x{copies} ") } else { String::new() }, if fav { "* " } else { "" }, if marked { "NG" } else { capsule_short(s.capsule) });
                        RowView { tab: s.cost.to_string(), name: s.name.to_uppercase(), right, color: if marked { ui::BAD } else { capsule_color(s.capsule) }, tone }
                    }
                    None => RowView::empty(),
                },
                None => RowView::empty(),
            });
        }
        let sstart = window_start(e.slot, ARSENAL_SIZE);
        let right: Vec<RowView> = (0..VISIBLE).map(|slot| slot_row(sstart + slot, e.deck[sstart + slot], skills, tone_of(!stock_focus, sstart + slot == e.slot))).collect();
        v.rows = [left, right];
        // The card: the skill under the cursor of whichever list has the focus.
        let shown = if stock_focus { stock.get(sc).copied() } else { e.deck.get(e.slot).copied() };
        v.card = match shown {
            Some(id) if id == AURA_PARTICLE => skill_card(&aura_def(), &cat.notes),
            Some(id) => skills.iter().find(|s| s.id == id).map_or_else(String::new, |d| skill_card(d, &cat.notes)),
            None => "No skill here.".into(),
        };
        v.detail = detail_body(&e.deck, skills);
        let problems = arsenal::check_kind(&e.deck, skills, e.kind);
        let usable = arsenal::rule_flags(&e.deck, skills) & arsenal::flag::HAS_SKILL != 0;
        v.problems = if !problems.is_empty() {
            (problems.iter().map(|p| p.describe(skills)).collect::<Vec<_>>().join("\n"), ui::BAD)
        } else if !usable {
            ("All Aura Particles: add at least one skill before you can use it in a battle.".into(), ui::YELLOW)
        } else {
            ("Legal arsenal: ready for versus.".into(), ui::TEAL)
        };
        v.prompt = if stock_focus {
            "A: Place    X: Favorite    Y: Options    LB/RB: Type    Start: Save    B: Back".into()
        } else {
            "A: Replace    X: Replace with Aura    Y: Add Same Skill    LB/RB: Type    Start: Save    B: Back".into()
        };
        v.keys = "Keyboard   W/S/A/D: move   Enter: A   Esc: B   X   Y   Q/E: LB/RB   Ctrl+S or Tab: Start".into();
    }

    fn dialog_view(&self, d: &Dialog, saved: &[SavedArsenal]) -> DialogView {
        let mk = |title: &str, body: &str, items: Vec<String>, cursor: usize, hint: &str| DialogView { title: title.into(), body: body.into(), items, cursor, keyboard: None, hint: hint.into() };
        let yes_no = vec!["Yes".to_string(), "No".to_string()];
        let pick_hint = "Up/Down: choose    A: Confirm    B: Cancel";
        match d {
            Dialog::Notice(t) => mk("NOTICE", t, vec!["OK".into()], 0, "A: OK"),
            Dialog::Confirm { what, cursor } => {
                let body = match what {
                    Confirm::Finish => "Finish editing your arsenal?".to_string(),
                    Confirm::Reflect => "Reflect changes to your arsenal?".to_string(),
                    Confirm::Undo => "Undo edits made to your arsenal?".to_string(),
                    Confirm::Disassemble(i) => format!("Arsenal \" {} \"\nDo you want to disassemble it?", saved.get(*i).map_or("?", |a| a.name.as_str())),
                };
                mk("ARSENAL", &body, yes_no, *cursor, pick_hint)
            }
            Dialog::Name { buf, caps, cell, .. } => DialogView {
                title: "ARSENAL NAME".into(),
                body: format!("Input Arsenal Name:  {buf}_"),
                items: vec![],
                cursor: 0,
                keyboard: Some(((0..KB_OK + 1).map(|c| kb_label(c, *caps)).collect(), *cell)),
                hint: "D-pad: choose    A: Type    X: Backspace    Y: Caps    Start: Confirm    B: Cancel".into(),
            },
            Dialog::Kind { cursor, .. } => mk(
                "NUMBER OF SCHOOLS",
                "How many schools may this arsenal draw from?\nFewer schools recover Aura faster.",
                KINDS.iter().map(|&k| kind_line(k)).collect(),
                *cursor,
                pick_hint,
            ),
            Dialog::Options { of, cursor } => match of {
                OptionsOf::Arsenal(i) => mk(
                    "ARSENAL OPTIONS",
                    saved.get(*i).map_or("", |a| a.name.as_str()),
                    vec!["Change Arsenal Name".into(), "Number of Schools".into(), "Randomize Skills".into(), "Disassemble Arsenal".into()],
                    *cursor,
                    pick_hint,
                ),
                OptionsOf::Stock => mk("OPTIONS", "Stock list", vec!["Sort Skills".into(), "Mask School".into(), "Mark Illegal Skills".into()], *cursor, pick_hint),
            },
            Dialog::Sort { row } => mk(
                "SORT",
                "Change the order in which skills are displayed in the stock list.\nYou can specify three conditions for sorting.",
                (0..3).map(|i| format!("{}: {}", i + 1, self.prefs.sort[i].label())).collect(),
                *row,
                "Up/Down: condition    Left/Right: change    X: Default    A/B: Done",
            ),
            Dialog::Mask { cursor } => mk(
                "MASK SCHOOL",
                "Change the Display/Hide setting for each school.",
                (0..5).map(|i| format!("{}   {}", school_name(i as i16 + 1), if self.prefs.hidden[i] { "hidden" } else { "shown" })).collect(),
                *cursor,
                "Up/Down: choose    A: Show / Hide    B: Done",
            ),
            Dialog::Mark { cursor } => mk(
                "RULE MARK",
                "Use this when creating an arsenal for battle with special rules.\nWhen a skill does not match the rules, then it will be marked (NG).",
                ["No Mark", "Single school", "Highlander", "Puny only", "Heavy only"].iter().enumerate().map(|(i, s)| if self.mark.label() == *s || (i == 1 && matches!(self.mark, RuleMark::SingleSchool(_))) { format!("{s}   (current)") } else { s.to_string() }).collect(),
                *cursor,
                pick_hint,
            ),
            Dialog::MarkSchool { cursor } => mk("ATTRIBUTE", "Creating an arsenal comprised of a single school.\nSkills that do not meet the requirements will be marked.", (1..=5).map(school_name).collect(), *cursor, pick_hint),
        }
    }
}

/// The Aura Particle's own card text goes through `skill_card`'s Aura branch.
fn aura_def() -> SkillDef {
    crate::combat::SkillDef { id: AURA_PARTICLE, name: "Aura Particle".into(), capsule: Capsule::Special, family: 0, cost: 0, strength: 0, strength_rule: 0, requirement: 0, requirement_amount: 0, level_penalty: 0, health_cost: 0, defence: 0, raw: [0; 70], s70_73: [0; 4] }
}

// ---------------------------------------------------------------------------
// Bevy glue
// ---------------------------------------------------------------------------

#[derive(Resource)]
pub struct BuilderState {
    pub core: Core,
    repeat: super::input::Repeat,
    /// Shots requested by the script (`shot:<name>`), taken by `input`.
    shots: Vec<String>,
}

impl Default for BuilderState {
    fn default() -> Self {
        BuilderState { core: Core::default(), repeat: Default::default(), shots: Vec::new() }
    }
}

pub fn spawn(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<crate::Config>, mut state: ResMut<BuilderState>, windows: Query<&Window>, art: Option<Res<MenuArt>>, mut images: ResMut<Assets<Image>>) {
    *state = BuilderState::default();
    state.core.prefs = arsenals::ArsenalStore::new(&cfg.cache).load_prefs();
    let art = MenuArt::get(&mut commands, art, &cfg.cache, &assets, &mut images);
    let (root, frame) = ui::menu_screen(&mut commands, &windows, &art, "ARSENAL BUILDER", false);
    commands.entity(root).insert(DespawnOnExit(AppState::Arsenal));
    bui::spawn_chrome(&mut commands, &art, frame);
}

/// Every button pressed this frame, from every source.
#[allow(clippy::too_many_arguments)]
fn gather(
    keys: &ButtonInput<KeyCode>,
    key_events: &mut MessageReader<KeyboardInput>,
    pads: &Query<&Gamepad>,
    repeat: &mut super::input::Repeat,
    script: Option<&mut super::script::MenuScript>,
    typing: bool,
    shots: &mut Vec<String>,
) -> Vec<Pulse> {
    use GamepadButton as G;
    let mut out = Vec::new();
    let pad = |b: G| pads.iter().any(|p| p.just_pressed(b));
    // Direction + A/B with hold-to-repeat (the first pad and the keyboard,
    // as every menu), any other pad too.
    let a = super::input::read_repeating(0, keys, pads, None, repeat);
    let mut push = |cond: bool, p: Pulse| {
        if cond {
            out.push(p);
        }
    };
    if typing {
        // Letters are text here; only the arrows, Enter, Esc and Backspace act.
        for ev in key_events.read() {
            if ev.state != bevy::input::ButtonState::Pressed {
                continue;
            }
            match ev.key_code {
                KeyCode::ArrowUp => push(true, Pulse::Up),
                KeyCode::ArrowDown => push(true, Pulse::Down),
                KeyCode::ArrowLeft => push(true, Pulse::Left),
                KeyCode::ArrowRight => push(true, Pulse::Right),
                KeyCode::Enter | KeyCode::NumpadEnter => push(true, Pulse::Start),
                KeyCode::Escape => push(true, Pulse::B),
                KeyCode::Backspace => push(true, Pulse::Backspace),
                _ => {
                    if let Some(text) = &ev.text {
                        for c in text.chars() {
                            push(!c.is_control(), Pulse::Char(c));
                        }
                    }
                }
            }
        }
        push(pad(G::DPadUp), Pulse::Up);
        push(pad(G::DPadDown), Pulse::Down);
        push(pad(G::DPadLeft), Pulse::Left);
        push(pad(G::DPadRight), Pulse::Right);
    } else {
        key_events.clear();
        push(a.up || pad(G::DPadUp), Pulse::Up);
        push(a.down || pad(G::DPadDown), Pulse::Down);
        push(a.left || pad(G::DPadLeft), Pulse::Left);
        push(a.right || pad(G::DPadRight), Pulse::Right);
        push(a.confirm, Pulse::A);
        push(a.cancel, Pulse::B);
        push(keys.just_pressed(KeyCode::KeyX), Pulse::X);
        push(keys.just_pressed(KeyCode::KeyY), Pulse::Y);
        push(keys.just_pressed(KeyCode::KeyQ), Pulse::Lb);
        push(keys.just_pressed(KeyCode::KeyE), Pulse::Rb);
        let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        push((ctrl && keys.just_pressed(KeyCode::KeyS)) || keys.just_pressed(KeyCode::Tab), Pulse::Start);
    }
    push(pad(G::South) && !a.confirm, Pulse::A);
    push(pad(G::East) && !a.cancel, Pulse::B);
    push(pad(G::West), Pulse::X);
    push(pad(G::North), Pulse::Y);
    push(pad(G::LeftTrigger), Pulse::Lb);
    push(pad(G::RightTrigger), Pulse::Rb);
    push(pad(G::Start), Pulse::Start);
    if let Some(script) = script {
        if let Some(tok) = super::script::tick_token(script) {
            if let Some(name) = tok.strip_prefix("shot:") {
                shots.push(name.to_string());
            } else if let Some(text) = tok.strip_prefix("t:") {
                out.extend(text.chars().map(Pulse::Char));
            } else if let Some(p) = Pulse::from_token(&tok) {
                out.push(p);
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut key_events: MessageReader<KeyboardInput>,
    pads: Query<&Gamepad>,
    mut state: ResMut<BuilderState>,
    catalog: Res<SkillCatalog>,
    mut saved: ResMut<SavedArsenals>,
    cfg: Res<crate::Config>,
    mut next: ResMut<NextState<AppState>>,
    menu: Res<crate::menu::MenuState>,
    mut sfx: ResMut<super::sound::MenuSfxQueue>,
    mut script: Option<ResMut<super::script::MenuScript>>,
) {
    // The settings overlay has the keys while it's open (and on the frame it toggles).
    if menu.open || menu.is_changed() {
        key_events.clear();
        return;
    }
    let state = &mut *state;
    let typing = matches!(state.core.dialog, Some(Dialog::Name { .. }));
    let pulses = gather(&keys, &mut key_events, &pads, &mut state.repeat, script.as_deref_mut(), typing, &mut state.shots);
    for p in pulses {
        state.core.handle(p, &catalog.skills, &mut saved.0);
    }
    sfx.0.append(&mut state.core.cues);
    let store = arsenals::ArsenalStore::new(&cfg.cache);
    if std::mem::take(&mut state.core.saved_changed) {
        store.save_all(&saved.0, &catalog.skills);
    }
    if std::mem::take(&mut state.core.prefs_changed) {
        store.save_prefs(&state.core.prefs);
    }
    // Scripted screenshots (proof runs only): saved next to the `--screenshot` path.
    for name in state.shots.drain(..) {
        if let Some(base) = &cfg.screenshot {
            let stem = base.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let _ = stem;
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(base.with_file_name(format!("f1_{name}.png"))));
        }
    }
    if state.core.leave {
        state.core.leave = false;
        next.set(AppState::Title);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn layout(
    art: Option<Res<MenuArt>>,
    windows: Query<&Window>,
    mut frames: Query<&mut UiTransform, With<ui::DesignFrame>>,
    state: Res<BuilderState>,
    catalog: Res<SkillCatalog>,
    saved: Res<SavedArsenals>,
    mut widgets: bui::Widgets,
) {
    ui::fit_frames(&windows, &mut frames);
    let Some(art) = art else { return };
    let view = state.core.view(&catalog, &saved.0);
    widgets.apply(&art, &view);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::testkit::{def, f};

    /// Skills across 5 schools and several capsule types: (id, capsule,
    /// school, cost). Ids 100.. = school 1, 200.. = school 2, etc.
    fn catalog() -> Vec<SkillDef> {
        let mut v = Vec::new();
        for school in 1..=5i16 {
            for k in 0..6i16 {
                let capsule = [Capsule::Attack, Capsule::Attack, Capsule::Defence, Capsule::Erase, Capsule::Status, Capsule::Special][k as usize];
                v.push(def(school * 100 + k, capsule, 1 + k as i32, &[(f::SCHOOL, school)]));
            }
        }
        for (i, s) in v.iter_mut().enumerate() {
            s.name = format!("Skill {}", s.id);
            let _ = i;
        }
        v
    }

    fn cat() -> SkillCatalog {
        SkillCatalog { skills: catalog(), notes: Default::default() }
    }

    fn run(core: &mut Core, skills: &[SkillDef], saved: &mut Vec<SavedArsenal>, pulses: &[Pulse]) {
        for &p in pulses {
            core.handle(p, skills, saved);
        }
    }

    use Pulse::*;

    /// New arsenal from nothing with pad pulses only: name confirm, kind 1.
    fn make_new(core: &mut Core, skills: &[SkillDef], saved: &mut Vec<SavedArsenal>, kind_downs: usize) {
        // Start on the "New Arsenal" row (the last row of the list).
        run(core, skills, saved, &[X]); // New -> name dialog
        assert!(matches!(core.dialog, Some(Dialog::Name { .. })));
        run(core, skills, saved, &[Start]); // keep the default name
        assert!(matches!(core.dialog, Some(Dialog::Kind { .. })));
        // The cursor starts on "2 schools" (index 1).
        for _ in 0..kind_downs {
            run(core, skills, saved, &[Down]);
        }
        run(core, skills, saved, &[A]);
    }

    #[test]
    fn pad_only_journey_builds_a_single_school_arsenal_saves_reloads_and_deletes_it() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved: Vec<SavedArsenal> = Vec::new();
        // New arsenal, kind 1 (cursor 1 -> Up once).
        run(&mut core, &skills, &mut saved, &[X, Start, Up, A]);
        assert_eq!(core.screen, Screen::Edit);
        assert_eq!(saved.len(), 1, "the original creates the arsenal as soon as it is named");
        assert_eq!(saved[0].name, "Arsenal 01");
        assert_eq!(saved[0].kind, Some(1));
        assert!(core.edit.as_ref().unwrap().deck.iter().all(|&x| x == AURA_PARTICLE), "a new arsenal is 30 Aura Particles");
        // Replace slot 1 with the first Attack skill (Tab::Attack is the default): A, A.
        run(&mut core, &skills, &mut saved, &[A, A]);
        assert_eq!(core.edit.as_ref().unwrap().deck[0], 100);
        assert_eq!(core.edit.as_ref().unwrap().slot, 1, "the cursor chains to the next empty slot");
        // Two more: the second Attack skill, then a copy of it with Y (Add Same).
        run(&mut core, &skills, &mut saved, &[A, Down, A]);
        run(&mut core, &skills, &mut saved, &[Up, Y]);
        let deck = &core.edit.as_ref().unwrap().deck;
        assert_eq!(&deck[..3], &[100, 101, 101], "{deck:?}");
        // Start saves (kind 1, one school: fine).
        run(&mut core, &skills, &mut saved, &[Start]);
        assert!(core.saved_changed && !core.edit.as_ref().unwrap().dirty());
        assert_eq!(&saved[0].skills[..3], &[100, 101, 101]);
        assert_eq!(saved[0].kind, Some(1));
        // Back: nothing unsaved, straight to the list.
        run(&mut core, &skills, &mut saved, &[B]);
        assert_eq!(core.screen, Screen::List);
        assert!(core.dialog.is_none());
        // "Reload": a fresh editor over the round-tripped list sees the same arsenal.
        let text = arsenals::render(&saved, &skills);
        let mut reloaded = arsenals::parse(&text);
        let mut core2 = Core::default();
        run(&mut core2, &skills, &mut reloaded, &[A]);
        assert_eq!(core2.edit.as_ref().unwrap().deck[..3], [100, 101, 101]);
        assert_eq!(core2.edit.as_ref().unwrap().kind, 1);
        // Options -> Disassemble (default cursor No), choose Yes.
        run(&mut core2, &skills, &mut reloaded, &[B, Y, Down, Down, Down, A]);
        assert!(matches!(core2.dialog, Some(Dialog::Confirm { what: Confirm::Disassemble(0), cursor: 1 })));
        run(&mut core2, &skills, &mut reloaded, &[Up, A]);
        assert!(reloaded.is_empty());
    }

    #[test]
    fn a_fresh_arsenal_is_all_aura_and_the_aura_tab_offers_an_aura_particle() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = Vec::new();
        make_new(&mut core, &skills, &mut saved, 0);
        let e = core.edit.as_ref().unwrap();
        assert_eq!(e.kind, 2);
        assert_eq!(e.deck, vec![AURA_PARTICLE; 30]);
        // LB from ATTACK goes to AURA: the single stock row is the Aura Particle.
        run(&mut core, &skills, &mut saved, &[Lb]);
        assert_eq!(core.stock_ids(&skills), vec![AURA_PARTICLE]);
        let v = core.view(&cat(), &saved);
        assert_eq!(v.rows[0][0].name, "AURA PARTICLE");
        assert_eq!(v.rows[1][0].name, "AURA PARTICLE");
        assert!(v.tabs.contains("[AURA]"));
    }

    #[test]
    fn replace_with_aura_and_add_same_skill_on_the_slot_list() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "T".into(), skills: vec![100, 101, 102, 103], kind: Some(1) }];
        run(&mut core, &skills, &mut saved, &[A]);
        run(&mut core, &skills, &mut saved, &[Down, X]);
        assert_eq!(&core.edit.as_ref().unwrap().deck[..4], &[100, 102, 103, AURA_PARTICLE], "slot 2 became Aura and the rest moved up");
        run(&mut core, &skills, &mut saved, &[Y, Y]);
        run(&mut core, &skills, &mut saved, &[Y]);
        assert_eq!(copies_of(&core, 102), 3);
        run(&mut core, &skills, &mut saved, &[Y]);
        assert!(matches!(&core.dialog, Some(Dialog::Notice(t)) if t.contains("more than 3")), "a 4th copy is refused with the original's text: {:?}", core.dialog);
    }

    fn copies_of(core: &Core, id: i16) -> usize {
        arsenal::copies(&core.edit.as_ref().unwrap().deck, id)
    }

    #[test]
    fn the_school_limit_is_checked_when_reflecting_changes_like_sysmax() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "T".into(), skills: vec![100], kind: Some(1) }];
        run(&mut core, &skills, &mut saved, &[A]);
        // Put a school-2 skill (200) into slot 2: A (Replace) on an Aura slot, find 200 in the stock.
        run(&mut core, &skills, &mut saved, &[Down, A]);
        let pos = core.stock_ids(&skills).iter().position(|&id| id == 200).unwrap();
        for _ in 0..pos {
            run(&mut core, &skills, &mut saved, &[Down]);
        }
        run(&mut core, &skills, &mut saved, &[A]);
        assert_eq!(&core.edit.as_ref().unwrap().deck[..2], &[100, 200], "placing is not refused: the check is at save time");
        run(&mut core, &skills, &mut saved, &[Start]);
        assert!(matches!(&core.dialog, Some(Dialog::Notice(t)) if t.contains("limited to 1 schools")), "{:?}", core.dialog);
        assert_eq!(saved[0].skills, vec![100], "nothing was saved");
        assert!(!core.saved_changed);
        run(&mut core, &skills, &mut saved, &[A]);
        assert!(core.dialog.is_none());
    }

    #[test]
    fn back_with_edits_asks_finish_then_reflect_then_undo_and_discards() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "T".into(), skills: vec![100], kind: Some(2) }];
        run(&mut core, &skills, &mut saved, &[A, Down, X]); // a no-op edit (slot 2 is already Aura)
        run(&mut core, &skills, &mut saved, &[B]);
        assert_eq!(core.screen, Screen::List, "no change, no question");
        run(&mut core, &skills, &mut saved, &[A, X, B]); // clear slot 1: a real edit
        assert!(matches!(core.dialog, Some(Dialog::Confirm { what: Confirm::Finish, cursor: 0 })));
        run(&mut core, &skills, &mut saved, &[A]);
        assert!(matches!(core.dialog, Some(Dialog::Confirm { what: Confirm::Reflect, .. })));
        run(&mut core, &skills, &mut saved, &[Down, A]); // No: do not reflect
        assert!(matches!(core.dialog, Some(Dialog::Confirm { what: Confirm::Undo, .. })));
        run(&mut core, &skills, &mut saved, &[A]); // Yes: undo
        assert_eq!(core.screen, Screen::List);
        assert_eq!(saved[0].skills, vec![100], "the edit was discarded, the saved arsenal is untouched");
        // B answers "no, keep editing" at any question.
        run(&mut core, &skills, &mut saved, &[A, X, B, B]);
        assert_eq!(core.screen, Screen::Edit);
        assert!(core.dialog.is_none());
        // Yes, Yes: reflect and leave.
        run(&mut core, &skills, &mut saved, &[B, A, A]);
        assert_eq!(core.screen, Screen::List);
        assert_eq!(saved[0].skills, vec![AURA_PARTICLE; 30], "reflected: {:?}", &saved[0].skills[..3]);
        assert!(core.saved_changed);
    }

    #[test]
    fn placing_a_fourth_copy_from_the_stock_shows_the_original_error() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "T".into(), skills: vec![100, 100, 100], kind: Some(1) }];
        run(&mut core, &skills, &mut saved, &[A, Down, Down, Down, A, A]);
        assert!(matches!(&core.dialog, Some(Dialog::Notice(t)) if t.contains("more than 3")), "{:?}", core.dialog);
        assert_eq!(core.edit.as_ref().unwrap().deck[3], AURA_PARTICLE);
    }

    #[test]
    fn rule_mark_marks_skills_in_the_stock_and_shows_ng() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "T".into(), skills: vec![100], kind: Some(2) }];
        run(&mut core, &skills, &mut saved, &[A, A]); // into the stock
        // Y options -> Mark Illegal Skills (3rd) -> Puny only (index 3).
        run(&mut core, &skills, &mut saved, &[Y, Down, Down, A, Down, Down, Down, A]);
        assert_eq!(core.mark, RuleMark::PunyOnly);
        assert!(core.dialog.is_none());
        // Cost > 3 skills in the Attack tab are marked; none of cost <= 3 are.
        let v = core.view(&cat(), &saved);
        let marked: Vec<&str> = v.rows[0].iter().filter(|r| r.right.contains("NG")).map(|r| r.name.as_str()).collect();
        // Attack skills in `catalog()`: k = 0, 1 -> costs 1, 2: none are illegal for Puny Only.
        assert!(marked.is_empty(), "{marked:?}");
        // Heavy only: cost < 3 is marked.
        run(&mut core, &skills, &mut saved, &[Y, Down, Down, A, Down, Down, Down, Down, A]);
        assert_eq!(core.mark, RuleMark::HeavyOnly);
        let v = core.view(&cat(), &saved);
        assert!(v.rows[0].iter().filter(|r| !r.name.is_empty()).all(|r| r.right.contains("NG")), "all Attack skills cost < 3 here");
        // Single school: the picker chooses the school.
        run(&mut core, &skills, &mut saved, &[Y, Down, Down, A, Down, A, Down, A]);
        assert_eq!(core.mark, RuleMark::SingleSchool(2));
        let v = core.view(&cat(), &saved);
        assert!(v.rows[0].iter().filter(|r| r.name == "SKILL 100").all(|r| r.right.contains("NG")), "a school-1 skill is marked under a school-2 mark");
        assert!(v.rows[0].iter().filter(|r| r.name == "SKILL 200").all(|r| !r.right.contains("NG")));
    }

    #[test]
    fn sort_keys_mask_school_and_favorites_shape_the_stock_list() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "T".into(), skills: vec![100], kind: Some(2) }];
        run(&mut core, &skills, &mut saved, &[A, A]);
        let all = core.stock_ids(&skills);
        assert_eq!(all.len(), 10, "5 schools x 2 attack skills");
        // Sort 1 = Cost (Right x2 from None: School, Cost), descending ties by catalogue order.
        run(&mut core, &skills, &mut saved, &[Y, A, Right, Right, A]);
        assert_eq!(core.prefs.sort[0], SortKey::Cost);
        let by_cost = core.stock_ids(&skills);
        assert_eq!(&by_cost[..5], &[100, 200, 300, 400, 500], "cost 1 first, stable within a cost");
        assert!(core.prefs_changed);
        // Mask the first school (Psycho).
        run(&mut core, &skills, &mut saved, &[Y, Down, A, A, B]);
        assert!(core.prefs.hidden[0]);
        assert!(!core.stock_ids(&skills).contains(&100) && core.stock_ids(&skills).contains(&200));
        // X in the sort dialog is "Default": all three conditions back to None.
        run(&mut core, &skills, &mut saved, &[Y, A, X]);
        assert_eq!(core.prefs.sort, [SortKey::None; 3]);
        run(&mut core, &skills, &mut saved, &[A]);
        assert_eq!(core.stock_ids(&skills).len(), 8, "Psycho is still masked");
    }

    #[test]
    fn favorite_key_puts_favourites_first() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "T".into(), skills: vec![100], kind: Some(2) }];
        run(&mut core, &skills, &mut saved, &[A, A]);
        // Cursor on the 10th row -> favourite it (X), then sort by Favorite.
        for _ in 0..9 {
            run(&mut core, &skills, &mut saved, &[Down]);
        }
        let last = core.stock_ids(&skills)[9];
        run(&mut core, &skills, &mut saved, &[X]);
        assert!(core.prefs.is_favourite(last));
        core.prefs.sort[0] = SortKey::Favorite;
        assert_eq!(core.stock_ids(&skills)[0], last);
        // The list re-sorted under the cursor: go back to the top (page Left) and toggle it off.
        run(&mut core, &skills, &mut saved, &[Left, X]);
        assert!(!core.prefs.is_favourite(last), "X toggles it back");
    }

    #[test]
    fn name_entry_works_from_the_pad_keyboard_and_from_typing_and_caps_at_15() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved: Vec<SavedArsenal> = Vec::new();
        run(&mut core, &skills, &mut saved, &[X]);
        // Clear the default name with X (backspace), then type "AB" from the grid: A, then Right + A.
        for _ in 0.."Arsenal 01".len() {
            run(&mut core, &skills, &mut saved, &[X]);
        }
        run(&mut core, &skills, &mut saved, &[A, Right, A]);
        let Some(Dialog::Name { buf, .. }) = &core.dialog else { panic!("{:?}", core.dialog) };
        assert_eq!(buf, "AB");
        // Caps off (Y) -> lowercase from the grid.
        run(&mut core, &skills, &mut saved, &[Y, A]);
        let Some(Dialog::Name { buf, caps, .. }) = &core.dialog else { panic!() };
        assert_eq!((buf.as_str(), *caps), ("ABb", false), "the cursor is still on B");
        // Typed characters work too, up to 15.
        for c in "xxxxxxxxxxxxxxxxxxxx".chars() {
            run(&mut core, &skills, &mut saved, &[Char(c)]);
        }
        let Some(Dialog::Name { buf, .. }) = &core.dialog else { panic!() };
        assert_eq!(buf.chars().count(), MAX_NAME_LEN);
        // Down from the bottom character row reaches the action row: OK confirms.
        let Some(Dialog::Name { cell, .. }) = &core.dialog else { panic!() };
        assert_eq!(*cell, 1);
        run(&mut core, &skills, &mut saved, &[Down, Down, Down, Down]);
        let Some(Dialog::Name { cell, .. }) = &core.dialog else { panic!() };
        assert!(*cell >= KB_CHARS, "the action row: {cell}");
        run(&mut core, &skills, &mut saved, &[Right, Right, A]);
        assert!(matches!(core.dialog, Some(Dialog::Kind { for_: KindFor::New(ref n), .. }) if n.chars().count() == MAX_NAME_LEN));
    }

    #[test]
    fn rename_and_change_number_of_schools_from_the_options_menu() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "Old".into(), skills: vec![100, 200], kind: Some(3) }];
        // Y -> Change Arsenal Name: clear with X x3, type Char, Start.
        run(&mut core, &skills, &mut saved, &[Y, A, X, X, X, Char('N'), Char('e'), Char('w'), Start]);
        assert_eq!(saved[0].name, "New");
        assert!(core.saved_changed);
        // Y -> Number of Schools: 1 school is refused (two schools in use), 2 is fine.
        run(&mut core, &skills, &mut saved, &[Y, Down, A, Up, Up, A]);
        assert!(matches!(&core.dialog, Some(Dialog::Notice(t)) if t.contains("limited to 1")), "{:?}", core.dialog);
        assert_eq!(saved[0].kind, Some(3));
        run(&mut core, &skills, &mut saved, &[A]);
        run(&mut core, &skills, &mut saved, &[Y, Down, A, Up, A]);
        assert_eq!(saved[0].kind, Some(2));
    }

    #[test]
    fn the_list_holds_16_arsenals_and_refuses_a_seventeenth() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved: Vec<SavedArsenal> = (0..MAX_ARSENALS).map(|i| SavedArsenal { name: format!("A{i}"), skills: vec![100], kind: Some(1) }).collect();
        let v = core.view(&cat(), &saved);
        assert!(v.rows[0].iter().all(|r| r.name != "NEW ARSENAL"), "no New row when full");
        run(&mut core, &skills, &mut saved, &[X]);
        assert!(matches!(&core.dialog, Some(Dialog::Notice(t)) if t.contains("16")));
        run(&mut core, &skills, &mut saved, &[A]);
        saved.pop();
        let v = core.view(&cat(), &saved);
        assert!(v.rows[0].iter().any(|r| r.name == "NEW ARSENAL") || core.list_len(&saved) == 16, "room again");
    }

    #[test]
    fn list_preview_pages_and_leaving_the_list_goes_back_to_title() {
        let skills = catalog();
        let mut core = Core::default();
        let mut saved = vec![SavedArsenal { name: "T".into(), skills: (100..106).collect(), kind: Some(1) }];
        run(&mut core, &skills, &mut saved, &[Right, Right, Right]);
        assert_eq!(core.preview_page, 2, "30 slots = 3 pages of 10");
        run(&mut core, &skills, &mut saved, &[Left]);
        assert_eq!(core.preview_page, 1);
        run(&mut core, &skills, &mut saved, &[B]);
        assert!(core.leave);
    }

    #[test]
    fn randomize_is_deterministic_legal_for_its_kind_and_shaped_like_a_quick_deck() {
        let cat = catalog();
        for kind in 1..=3u8 {
            for seed in 0..30u64 {
                let deck = randomize(seed, &cat, kind);
                let problems = arsenal::check_kind(&deck, &cat, kind);
                assert!(problems.is_empty(), "kind {kind} seed {seed}: {problems:?}");
                // 16 skills, or every copy the chosen schools can offer (6 skills x 2 copies each here).
                assert_eq!(deck.iter().filter(|&&x| x != AURA_PARTICLE).count(), RANDOM_SKILLS.min(12 * kind as usize));
                assert!(deck.iter().all(|&id| id == AURA_PARTICLE || arsenal::copies(&deck, id) <= RANDOM_COPIES));
            }
        }
        assert_eq!(randomize(7, &cat, 2), randomize(7, &cat, 2));
        assert_ne!(randomize(1, &cat, 2), randomize(2, &cat, 2));
        assert_eq!(randomize(1, &[], 2), vec![AURA_PARTICLE; ARSENAL_SIZE]);
    }

    #[test]
    fn script_tokens_map_to_pulses() {
        assert_eq!(Pulse::from_token("confirm"), Some(A));
        assert_eq!(Pulse::from_token("lb"), Some(Lb));
        assert_eq!(Pulse::from_token("start"), Some(Start));
        assert_eq!(Pulse::from_token("nonsense"), None);
    }

    #[test]
    fn keyboard_navigation_visits_every_cell_and_the_action_row() {
        // Every character cell is reachable by moving right within its row.
        let mut seen = std::collections::HashSet::new();
        let mut cell = 0;
        for _ in 0..10 {
            seen.insert(cell);
            cell = kb_move(cell, 1, 0);
        }
        assert_eq!(seen.len(), 10);
        assert_eq!(kb_move(0, -1, 0), 9, "wraps within the row");
        assert_eq!(kb_move(35, 0, 1), KB_CAPS, "row 3, column 5 goes down to Caps");
        assert_eq!(kb_move(KB_OK, 0, -1), 38);
        assert_eq!(kb_char(0, true), Some('A'));
        assert_eq!(kb_char(0, false), Some('a'));
        assert_eq!(kb_char(39, true), Some(' '));
        assert_eq!(kb_label(KB_OK, true), "OK");
    }

    #[test]
    fn detail_panel_lists_type_school_counts_and_the_rules_the_arsenal_qualifies_for() {
        let skills = catalog();
        let text = detail_text("T", 1, &[100, 101, 102].iter().copied().chain(std::iter::repeat_n(AURA_PARTICLE, 27)).collect::<Vec<_>>(), &skills);
        assert!(text.contains("Attack 2") && text.contains("Defense 1") && text.contains("Aura Particles 27"), "{text}");
        assert!(text.contains("Psycho 3"), "{text}");
        assert!(text.contains("Single School") && text.contains("Highlander") && text.contains("Puny Only"), "{text}");
        assert!(text.contains("x1.3"), "kind 1 shows the 1.3 handicap: {text}");
    }

    #[test]
    fn detail_panel_leaves_out_zero_counts_and_the_edit_body_has_no_name_line() {
        let skills = catalog();
        let deck: Vec<i16> = [100, 101, 102].iter().copied().chain(std::iter::repeat_n(AURA_PARTICLE, 27)).collect();
        let body = detail_body(&deck, &skills);
        assert!(!body.contains(" 0"), "no zero counts: {body}");
        assert!(body.contains("Attack 2") && body.contains("Aura Particles 27"), "{body}");
        assert!(!body.contains("x1.3"), "the edit header carries the kind line: {body}");
    }
}
