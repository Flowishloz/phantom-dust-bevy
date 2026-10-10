//! Skeleton posing: bind pose and sampled animation poses.
//!
//! Joint locals are translate * rotate(euler) * scale, in the game's
//! left-handed space. Animation keys replace the bind values for the axes
//! they store; axes they don't store keep the bind value.

use crate::alr::{Joint, Model};
use crate::anim::{Animation, Key, Track};
use glam::{EulerRot, Mat4, Quat, Vec3};

/// Euler application order. The game data is authored for one of these; kept
/// switchable so it can be verified visually.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// R = Rz * Ry * Rx (X applied first)
    Zyx,
    /// R = Rx * Ry * Rz (Z applied first)
    Xyz,
    /// Any convention: `apply` lists axes in the order they are applied to a
    /// vector (first = innermost), `neg` bit k negates the angle on axis k.
    Custom { apply: [u8; 3], neg: u8 },
}

impl Order {
    /// The convention the game uses: X applied first, then Y, then Z
    /// (R = Rz * Ry * Rx), no sign changes. `pd_convert eulertest` narrows
    /// the field (knee/elbow bend direction) but can't separate the top
    /// candidates; this one was picked by comparing idle and attack poses
    /// against the real game (2026-09-26). Stage transforms use the same.
    pub const GAME: Order = Order::Custom { apply: [0, 1, 2], neg: 0 };

    /// Parse "zyx", "xyz", "game", or "c:<apply digits>:<neg>" e.g. "c:210:3".
    pub fn parse(s: &str) -> Option<Order> {
        match s {
            "zyx" => Some(Order::Zyx),
            "xyz" => Some(Order::Xyz),
            "game" => Some(Order::GAME),
            _ => {
                let mut it = s.strip_prefix("c:")?.split(':');
                let d: Vec<u8> = it.next()?.bytes().map(|b| b - b'0').collect();
                let neg = it.next()?.parse().ok()?;
                (d.len() == 3).then(|| Order::Custom { apply: [d[0], d[1], d[2]], neg })
            }
        }
    }

    /// All 48 conventions (6 axis orders x 8 sign patterns).
    pub fn all() -> Vec<Order> {
        let perms = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        let mut v = Vec::new();
        for p in perms {
            for neg in 0..8u8 {
                v.push(Order::Custom { apply: p, neg });
            }
        }
        v
    }
}

fn axis_quat(axis: u8, a: f32) -> Quat {
    match axis {
        0 => Quat::from_rotation_x(a),
        1 => Quat::from_rotation_y(a),
        _ => Quat::from_rotation_z(a),
    }
}

pub fn euler_quat(r: [f32; 3], order: Order) -> Quat {
    match order {
        Order::Zyx => Quat::from_euler(EulerRot::ZYX, r[2], r[1], r[0]),
        Order::Xyz => Quat::from_euler(EulerRot::XYZ, r[0], r[1], r[2]),
        Order::Custom { apply, neg } => {
            let mut q = Quat::IDENTITY;
            for &ax in &apply {
                let a = if neg & (1 << ax) != 0 { -r[ax as usize] } else { r[ax as usize] };
                // Later rotations are applied after (outside) earlier ones.
                q = axis_quat(ax, a) * q;
            }
            q
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Local {
    pub t: Vec3,
    pub r: [f32; 3],
    pub s: Vec3,
}

impl Local {
    pub fn from_joint(j: &Joint) -> Self {
        let s = Vec3::from(j.scale);
        Local { t: Vec3::from(j.position), r: j.rotation, s: if s.length_squared() < 1e-12 { Vec3::ONE } else { s } }
    }

    pub fn matrix(&self, order: Order) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.s, euler_quat(self.r, order), self.t)
    }
}

/// Which axes a key stores, from the track's settings byte.
/// Translation codes 0..=6 and rotation codes 7..=13 share one ordering:
/// xyz, xy, xz, yz, x, y, z.
pub fn axes_for_code(code: u8) -> &'static [usize] {
    match code % 7 {
        0 => &[0, 1, 2],
        1 => &[0, 1],
        2 => &[0, 2],
        3 => &[1, 2],
        4 => &[0],
        5 => &[1],
        _ => &[2],
    }
}

fn wrap_pi(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a % tau;
    if a > std::f32::consts::PI {
        a -= tau;
    } else if a < -std::f32::consts::PI {
        a += tau;
    }
    a
}

/// Sample keys at `frame`, writing stored axes into `out`.
fn sample_keys(keys: &[Key], code: u8, frame: f32, angular: bool, out: &mut [f32; 3]) {
    if keys.is_empty() {
        return;
    }
    let axes = axes_for_code(code);
    let (a, b, f) = match keys.iter().position(|k| k.frame > frame) {
        Some(0) => (&keys[0], &keys[0], 0.0),
        Some(i) => {
            let (a, b) = (&keys[i - 1], &keys[i]);
            let span = (b.frame - a.frame).max(1e-6);
            (a, b, ((frame - a.frame) / span).clamp(0.0, 1.0))
        }
        None => (keys.last().unwrap(), keys.last().unwrap(), 0.0),
    };
    for (ci, &axis) in axes.iter().enumerate().take(a.components as usize) {
        let (va, vb) = (a.value[ci], b.value[ci]);
        out[axis] = if angular { va + wrap_pi(vb - va) * f } else { va + (vb - va) * f };
    }
}

pub fn track_local(bind: Local, t: &Track, frame: f32) -> Local {
    let mut l = bind;
    let mut tr = l.t.to_array();
    sample_keys(&t.translation, t.settings[1], frame, false, &mut tr);
    l.t = Vec3::from(tr);
    sample_keys(&t.rotation, t.settings[0], frame, true, &mut l.r);
    l
}

pub fn bind_locals(m: &Model) -> Vec<Local> {
    m.joints.iter().map(Local::from_joint).collect()
}

pub fn animated_locals(m: &Model, anim: &Animation, frame: f32) -> Vec<Local> {
    let mut locals = bind_locals(m);
    for t in &anim.tracks {
        if let Some(l) = locals.get_mut(t.joint as usize) {
            *l = track_local(*l, t, frame);
        }
    }
    locals
}

/// Model-space matrices for every joint.
pub fn world_matrices(m: &Model, locals: &[Local], order: Order) -> Vec<Mat4> {
    let mut world: Vec<Mat4> = vec![Mat4::IDENTITY; locals.len()];
    // Joints are stored parent-before-child, but don't rely on it.
    let mut done = vec![false; locals.len()];
    fn resolve(i: usize, m: &Model, locals: &[Local], order: Order, world: &mut [Mat4], done: &mut [bool], depth: usize) -> Mat4 {
        if done[i] || depth > 256 {
            return world[i];
        }
        let p = m.joints[i].parent;
        let parent = if p >= 0 && (p as usize) < locals.len() && p as usize != i {
            resolve(p as usize, m, locals, order, world, done, depth + 1)
        } else {
            Mat4::IDENTITY
        };
        world[i] = parent * locals[i].matrix(order);
        done[i] = true;
        world[i]
    }
    for i in 0..locals.len() {
        resolve(i, m, locals, order, &mut world, &mut done, 0);
    }
    world
}
