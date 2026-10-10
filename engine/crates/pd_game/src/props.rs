//! Stage objects in a match (arena objects, phase B): desks, drums, signs,
//! railings, floor sections, bridges… placed from `stNN.props.json`
//! (docs/FORMATS-STAGES.md). Part of the deterministic simulation.
//!
//! An object is intact, damaged, broken, being thrown, or gone. Attacks that
//! hit it wear it down; at half its toughness it shows its damaged model, at
//! zero its broken model (and stops blocking: its collision is switched off,
//! so a broken floor or bridge drops whoever stood on it). Psycho Kinesis /
//! Power ("[Object] Throws nearby object; STR depends on object") pick up the
//! nearest small object and throw it.
//!
//! Neither toughness, weight nor a "throwable" flag is in the stage file
//! (FORMATS-STAGES.md), so both come from the object's size — all
//! **unconfirmed** named constants below.

use glam::Vec3;

/// Size (largest side of the box, units) per point of toughness. A chair or
/// drum (~20) takes 2, a desk 2, a floor section (~70) 6, a container (~113) 9.
pub const SIZE_PER_TOUGHNESS: f32 = 12.0;
/// Objects larger than this can't be picked up (containers, the heli, floors).
pub const THROW_MAX_SIZE: f32 = 60.0;
/// Panels thinner than this (railings, fences, stair sides) are fixed in place.
pub const THROW_MIN_THICKNESS: f32 = 4.0;
/// How far from the caster Psycho Kinesis finds an object (units; ~5 m).
pub const THROW_REACH: f32 = 70.0;
/// …and on about the same level: its box within this much of the caster's
/// feet-to-head height (not on the floor above or below).
pub const THROW_LEVEL_SLACK: f32 = 15.0;
/// A thrown object's STR: one point per this much size, 1..=THROW_MAX_STR.
pub const SIZE_PER_THROW_STR: f32 = 10.0;
/// Psycho Kinesis lifts the object this far above its top before throwing it…
pub const THROW_LIFT: f32 = 15.0;
/// …and holds it there this many ticks (0.25 s).
pub const THROW_HANG_TICKS: u16 = 15;
pub const THROW_MAX_STR: i32 = 6;
/// A shell brushed aside by a [Brush] destroys what it meets ("anything in
/// the way of the brushed shell will be destroyed"): wear enough for anything.
pub const BRUSHED_WEAR: i32 = 999;

/// Prop Hunt (`prophunt.rs`): how far from a hider an object can be and still be taken as a disguise with X (units;
/// about two metres, "X on a prop"). UNCONFIRMED: picked for feel, not from the game.
pub const DISGUISE_REACH: f32 = 30.0;
/// Prop Hunt: the largest object a hider can become (largest side, units). The same limit as the objects Psycho
/// Kinesis can lift (chairs, drums, desks, crates up to a car), so floors, containers and the heli are out.
/// UNCONFIRMED: a believability call, not a game rule.
pub const DISGUISE_MAX_SIZE: f32 = THROW_MAX_SIZE;

/// Behaviour codes (the placement's kind byte) of structural pieces: floor
/// sections and bridges (23), road plates (26), pillars (28, 48), the
/// collapsing container (6). They break but are never picked up.
pub const STRUCTURAL_KINDS: [u8; 5] = [6, 23, 26, 28, 48];
/// Kinds whose behaviour is scripted in ways not decoded yet (st08's
/// collapsing ground blocks 58 with a collapse order; st01's tower pieces
/// 64+): left intact so those arenas keep their floor.
pub fn scripted_kind(kind: u8) -> bool {
    kind == 58 || kind >= 64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropState {
    Intact,
    Damaged,
    Broken,
    /// Picked up by Psycho Kinesis / Power, flying at someone.
    Thrown,
    /// Thrown and shattered: nothing left.
    Gone,
}

#[derive(Clone, Debug)]
pub struct Prop {
    /// Placement index: `props.json` `index`, the collision owner tag, the
    /// stage glTF node `propNNN_*`.
    pub index: u32,
    pub class: u8,
    pub kind: u8,
    /// Box in the right-handed stage space (as placed).
    pub min: Vec3,
    pub max: Vec3,
    pub has_damaged: bool,
    pub has_broken: bool,
    pub state: PropState,
    pub toughness: i32,
    pub wear: i32,
    /// Thinnest side of its own (unrotated) shape: railings and panels are thin.
    pub thinnest: f32,
}

impl Prop {
    pub fn new(index: u32, class: u8, kind: u8, min: Vec3, max: Vec3, has_damaged: bool, has_broken: bool) -> Self {
        let size = (max - min).max_element();
        let toughness = ((size / SIZE_PER_TOUGHNESS).round() as i32).max(1);
        let thinnest = (max - min).min_element();
        Prop { index, class, kind, min, max, has_damaged, has_broken, state: PropState::Intact, toughness, wear: 0, thinnest }
    }

    pub fn centre(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> f32 {
        (self.max - self.min).max_element()
    }

    /// Can attacks change it at all (it has another model to show)?
    pub fn breakable(&self) -> bool {
        self.class == 2 && (self.has_damaged || self.has_broken) && !scripted_kind(self.kind)
    }

    /// Still standing where it was placed (and blocking).
    pub fn standing(&self) -> bool {
        matches!(self.state, PropState::Intact | PropState::Damaged)
    }

    /// Psycho Kinesis / Power can pick it up: a small loose object that
    /// shatters (has a broken model). Guardrails and railings only bend
    /// (damaged model only), so they stay put.
    pub fn throwable(&self) -> bool {
        let thinnest = self.thinnest;
        self.standing() && self.breakable() && self.has_broken && !STRUCTURAL_KINDS.contains(&self.kind) && self.size() <= THROW_MAX_SIZE && thinnest >= THROW_MIN_THICKNESS
    }

    /// Prop Hunt: a hider can become this object (X): standing as placed, not a structural or scripted piece, small
    /// enough to be believable (`DISGUISE_MAX_SIZE`) and not a thin panel that belongs to a railing or stairs
    /// (`THROW_MIN_THICKNESS`). Unlike [`throwable`](Self::throwable) it need not be breakable: a sign or a bin that
    /// never breaks is still a good hiding place.
    pub fn disguisable(&self) -> bool {
        self.state == PropState::Intact && !STRUCTURAL_KINDS.contains(&self.kind) && !scripted_kind(self.kind) && self.size() <= DISGUISE_MAX_SIZE && self.thinnest >= THROW_MIN_THICKNESS
    }

    /// "STR depends on object": bigger hits harder.
    pub fn throw_strength(&self) -> i32 {
        ((self.size() / SIZE_PER_THROW_STR).round() as i32).clamp(1, THROW_MAX_STR)
    }

    /// An attack of strength `str` hits it. True if its collision should go
    /// (it just broke).
    pub fn wear_down(&mut self, str: i32) -> bool {
        if !self.standing() || !self.breakable() || str <= 0 {
            return false;
        }
        self.wear += str;
        if self.wear >= self.toughness && self.has_broken {
            self.state = PropState::Broken;
            return true;
        }
        if self.has_damaged && self.wear * 2 >= self.toughness {
            self.state = PropState::Damaged;
        }
        false
    }

    /// Distance from `p` to the box (0 inside).
    pub fn distance_to(&self, p: Vec3) -> f32 {
        (p.clamp(self.min, self.max) - p).length()
    }

    /// Back as placed (a new round).
    pub fn restore(&mut self) {
        self.state = PropState::Intact;
        self.wear = 0;
    }
}

/// The nearest object a player standing at `at` can throw, if one is in reach.
pub fn nearest_throwable(props: &[Prop], at: Vec3) -> Option<usize> {
    let (low, high) = (at.y - THROW_LEVEL_SLACK, at.y + crate::sim::BODY_HEIGHT + THROW_LEVEL_SLACK);
    props
        .iter()
        .enumerate()
        .filter(|(_, p)| p.throwable() && p.min.y <= high && p.max.y >= low)
        .map(|(i, p)| (i, p.distance_to(at)))
        .filter(|&(_, d)| d <= THROW_REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(i, _)| i)
}

/// Prop Hunt: the nearest object a hider standing at `at` can become, if one is in reach (`DISGUISE_REACH`) on about
/// the same level (the rule Psycho Kinesis uses: not on the floor above or below). Ties go to the lower index.
pub fn nearest_disguise(props: &[Prop], at: Vec3) -> Option<usize> {
    let (low, high) = (at.y - THROW_LEVEL_SLACK, at.y + crate::sim::BODY_HEIGHT + THROW_LEVEL_SLACK);
    props
        .iter()
        .enumerate()
        .filter(|(_, p)| p.disguisable() && p.min.y <= high && p.max.y >= low)
        .map(|(i, p)| (i, p.distance_to(at)))
        .filter(|&(_, d)| d <= DISGUISE_REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(i, _)| i)
}

/// The stage's objects from its `stNN.props.json` document (solid ones only).
pub fn from_json(v: &serde_json::Value) -> Vec<Prop> {
    let vec3 = |a: &serde_json::Value| Some(Vec3::new(a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32));
    // The type's own hull box (object space, before rotation).
    let own_thinnest = |ty: &serde_json::Value| {
        let t = v["types"].as_array()?.iter().find(|t| t["id"] == *ty)?;
        Some((vec3(&t["hull"]["max_game"])? - vec3(&t["hull"]["min_game"])?).min_element())
    };
    v["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["solid"] == true)
        .filter_map(|o| {
            let mut p = Prop::new(
                o["index"].as_u64()? as u32,
                o["class"].as_u64()? as u8,
                o["kind"].as_u64()? as u8,
                vec3(&o["bbox_min"])?,
                vec3(&o["bbox_max"])?,
                !o["damaged_model"].is_null(),
                !o["broken_model"].is_null(),
            );
            if let Some(t) = own_thinnest(&o["type"]) {
                p.thinnest = t;
            }
            Some(p)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chair() -> Prop {
        Prop::new(0, 2, 3, Vec3::ZERO, Vec3::new(8.0, 20.0, 8.0), true, true)
    }

    #[test]
    fn wear_shows_damaged_then_broken() {
        let mut p = Prop::new(0, 2, 3, Vec3::ZERO, Vec3::new(10.0, 48.0, 10.0), true, true);
        assert_eq!(p.toughness, 4);
        assert!(!p.wear_down(1));
        assert_eq!(p.state, PropState::Intact);
        assert!(!p.wear_down(1));
        assert_eq!(p.state, PropState::Damaged, "half worn");
        assert!(p.wear_down(2), "broken: collision goes");
        assert_eq!(p.state, PropState::Broken);
        assert!(!p.wear_down(5), "already broken");
    }

    #[test]
    fn objects_without_other_models_or_scripted_ones_never_change() {
        let mut fence = Prop::new(0, 2, 1, Vec3::ZERO, Vec3::splat(20.0), false, false);
        assert!(!fence.wear_down(10));
        assert_eq!(fence.state, PropState::Intact);
        let mut block = Prop::new(0, 2, 58, Vec3::ZERO, Vec3::splat(140.0), false, true);
        assert!(!block.wear_down(99), "st08 ground blocks: scripted, left alone");
        // Damaged-only (guardrails): damaged, never broken, stays solid.
        let mut rail = Prop::new(0, 2, 1, Vec3::ZERO, Vec3::new(30.0, 10.0, 2.0), true, false);
        assert!(!rail.wear_down(99));
        assert_eq!(rail.state, PropState::Damaged);
    }

    #[test]
    fn only_small_standing_non_structural_objects_are_throwable() {
        let c = chair();
        assert!(c.throwable());
        assert_eq!(c.throw_strength(), 2);
        let floor = Prop::new(1, 2, 23, Vec3::ZERO, Vec3::new(70.0, 5.0, 40.0), false, true);
        assert!(!floor.throwable());
        let container = Prop::new(2, 2, 3, Vec3::ZERO, Vec3::splat(113.0), true, true);
        assert!(!container.throwable());
        let rail = Prop::new(3, 2, 1, Vec3::ZERO, Vec3::new(30.0, 10.0, 2.0), true, false);
        assert!(!rail.throwable(), "bends, doesn't come loose");
        let panel = Prop::new(4, 2, 1, Vec3::ZERO, Vec3::new(2.2, 20.0, 21.0), true, true);
        assert!(!panel.throwable(), "a stair-side panel is part of the stairs");
        let mut broken = chair();
        broken.state = PropState::Broken;
        assert!(!broken.throwable());
    }

    #[test]
    fn a_disguise_is_a_small_intact_object_and_never_a_floor_container_or_panel() {
        assert!(chair().disguisable());
        let sign = Prop::new(0, 1, 2, Vec3::ZERO, Vec3::new(10.0, 30.0, 6.0), false, false);
        assert!(sign.disguisable(), "an unbreakable object is still a hiding place");
        let floor = Prop::new(1, 2, 23, Vec3::ZERO, Vec3::new(40.0, 5.0, 40.0), false, true);
        assert!(!floor.disguisable(), "structural");
        let container = Prop::new(2, 2, 3, Vec3::ZERO, Vec3::splat(113.0), true, true);
        assert!(!container.disguisable(), "too big");
        let heli = Prop::new(3, 2, 3, Vec3::ZERO, Vec3::new(150.0, 40.0, 60.0), false, false);
        assert!(!heli.disguisable());
        let panel = Prop::new(4, 2, 1, Vec3::ZERO, Vec3::new(2.2, 20.0, 21.0), true, true);
        assert!(!panel.disguisable(), "part of the stairs");
        let block = Prop::new(5, 2, 58, Vec3::ZERO, Vec3::splat(20.0), false, true);
        assert!(!block.disguisable(), "scripted");
        for state in [PropState::Damaged, PropState::Broken, PropState::Thrown, PropState::Gone] {
            assert!(!Prop { state, ..chair() }.disguisable(), "{state:?}: not intact");
        }
    }

    #[test]
    fn the_nearest_disguise_in_reach_on_the_same_level() {
        let near = Prop { min: Vec3::new(10.0, 0.0, 0.0), max: Vec3::new(18.0, 20.0, 8.0), ..chair() };
        let nearer = Prop { min: Vec3::new(-8.0, 0.0, 0.0), max: Vec3::new(-2.0, 20.0, 8.0), ..chair() };
        let above = Prop { min: Vec3::new(-1.0, 60.0, -1.0), max: Vec3::new(1.0, 80.0, 1.0), ..chair() };
        let big = Prop { min: Vec3::new(-1.0, 0.0, -1.0), max: Vec3::new(120.0, 20.0, 120.0), ..chair() };
        let props = [big, near.clone(), nearer, above];
        assert_eq!(nearest_disguise(&props, Vec3::ZERO), Some(2), "not the container standing on top of them, not the one upstairs");
        assert_eq!(nearest_disguise(&[near.clone()], Vec3::new(-DISGUISE_REACH - 11.0, 0.0, 0.0)), None, "out of reach");
        assert_eq!(nearest_disguise(&[near], Vec3::new(10.0 - DISGUISE_REACH + 1.0, 0.0, 4.0)), Some(0));
    }

    #[test]
    fn nearest_throwable_in_reach_ties_to_the_lower_index() {
        let mut a = chair();
        let mut b = chair();
        (a.index, b.index) = (0, 1);
        let far = Prop { min: Vec3::splat(500.0), max: Vec3::splat(510.0), ..chair() };
        let props = [far, a, b];
        assert_eq!(nearest_throwable(&props, Vec3::new(-10.0, 0.0, 0.0)), Some(1));
        assert_eq!(nearest_throwable(&props, Vec3::new(300.0, 0.0, 0.0)), None);
        assert_eq!(nearest_throwable(&props, Vec3::new(-10.0, 50.0, 0.0)), None, "on the floor below");
    }
}
