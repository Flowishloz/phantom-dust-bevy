//! `pd_convert walkprobe <Map dir> <stNN> [--start K] [--from x,y,z] [--secs S] [--dirs N]`
//!
//! Walks a player-sized body out of a start point in N compass directions on
//! the stage's collision (built exactly as `stages` builds it) and reports
//! where each walk stops, and which triangles stop it: model / draw (and
//! whether the draw is visible) or the placed object. No renderer needed.
//!
//! The walk copies the sim's grounded movement (`pd_game/src/sim.rs`
//! `step_player_at`): run speed 72/s, wall push above step height, step up
//! 6, stick down small steps, gravity, coyote ticks.

use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use pd_formats::alr::Alr;
use pd_formats::collision::{Collision, NO_OBJECT};
use pd_formats::gltf;
use std::path::Path;

const HZ: f32 = 60.0;
const RUN_SPEED: f32 = 72.0;
const GRAVITY: f32 = 300.0;
const STEP_HEIGHT: f32 = 6.0;
const BODY_RADIUS: f32 = 4.5;
const BODY_HEIGHT: f32 = 20.0;
const COYOTE_TICKS: u8 = 6;
const FALL_RESPAWN_BELOW: f32 = 150.0;

/// Where a collision triangle came from.
#[derive(Clone, Copy, Debug)]
pub enum Source {
    Stage { model: usize, draw: usize },
    Object(u32),
}

/// A stage's collision triangles with their source, in the order
/// `stage_collision` writes them. `all_draws`: also the draws the converter
/// leaves out (what the collision was before that filter).
pub fn sourced_triangles(a: &Alr, p: Option<&pd_formats::stagedat::StageProps>, all_draws: bool) -> (Vec<[Vec3; 3]>, Vec<u32>, Vec<Source>) {
    let skip: &[usize] = p.map(|p| p.skip_models.as_slice()).unwrap_or(&[]);
    let (mut tris, mut src, mut solid) = (Vec::new(), Vec::new(), Vec::new());
    for mi in 0..a.models.len() {
        if gltf::collision_skips_model(a, mi, skip) {
            continue;
        }
        gltf::model_triangles_each(a, mi, Mat4::IDENTITY, &mut |di, t| {
            tris.push(t);
            src.push(Source::Stage { model: mi, draw: di });
            solid.push(gltf::collision_keeps_draw(a, mi, di));
        });
    }
    // The converter's own list must be exactly the solid ones.
    let conv = gltf::world_triangles_skip(a, skip);
    assert!(conv.iter().eq(tris.iter().zip(&solid).filter(|(_, s)| **s).map(|(t, _)| t)), "probe and converter disagree");
    if !all_draws {
        let mut k = solid.iter();
        tris.retain(|_| *k.next().unwrap());
        let mut k = solid.iter();
        src.retain(|_| *k.next().unwrap());
    }
    let mut owners = vec![NO_OBJECT; tris.len()];
    if let Some(p) = p {
        tris.extend_from_slice(&p.collision);
        owners.extend_from_slice(&p.collision_owner);
        src.extend(p.collision_owner.iter().map(|&o| Source::Object(o)));
    }
    (tris, owners, src)
}
/// Walk from `start` along `dir` (unit XZ) for `ticks` sim ticks.
pub fn walk(col: &Collision, start: Vec3, dir: Vec3, ticks: u32) -> (Vec3, &'static str, f32) {
    let mut pos = start;
    if let Some(g) = col.ground_below(pos.x, pos.z, pos.y + 12.0) {
        pos.y = g;
    }
    let spawn = pos;
    let mut vel = Vec3::ZERO;
    let (mut grounded, mut coyote) = (true, 0u8);
    let dt = 1.0 / HZ;
    let mut hist: Vec<Vec3> = Vec::new();
    let mut travelled = 0.0;
    for _ in 0..ticks {
        let wish = dir * RUN_SPEED;
        let control = if grounded { 1.0 } else { 0.6 };
        vel.x += (wish.x - vel.x) * control;
        vel.z += (wish.z - vel.z) * control;
        if !grounded {
            vel.y -= GRAVITY * dt;
        }
        let mut next = pos + vel * dt;
        next = col.push_out_of_walls(next, BODY_RADIUS, STEP_HEIGHT + 1.0, BODY_HEIGHT);
        let ground = col.ground_below(next.x, next.z, pos.y.max(next.y) + STEP_HEIGHT);
        let was = grounded;
        let prev = pos;
        pos = next;
        match ground {
            Some(g) if pos.y <= g + 0.01 && vel.y <= 0.0 => {
                pos.y = g;
                vel.y = 0.0;
                grounded = true;
                coyote = 0;
            }
            Some(g) if was && pos.y - g <= STEP_HEIGHT && vel.y <= 0.0 => {
                pos.y = g;
                vel.y = 0.0;
                coyote = 0;
            }
            _ if was && vel.y <= 0.0 && coyote < COYOTE_TICKS => coyote += 1,
            _ => grounded = false,
        }
        travelled += Vec3::new(pos.x - prev.x, 0.0, pos.z - prev.z).length();
        if pos.y < spawn.y - FALL_RESPAWN_BELOW {
            return (pos, "fell", travelled);
        }
        hist.push(pos);
        if hist.len() > 30 {
            let old = hist[hist.len() - 31];
            if Vec3::new(pos.x - old.x, 0.0, pos.z - old.z).length() < 1.0 {
                return (pos, "BLOCKED", travelled);
            }
        }
    }
    (pos, "open", travelled)
}

/// Walls within reach of a body standing at `pos` facing `dir`.
pub fn blockers(tris: &[[Vec3; 3]], pos: Vec3, dir: Vec3) -> Vec<(usize, f32)> {
    let probe = pos + dir * 2.0;
    let mut out = Vec::new();
    for (i, t) in tris.iter().enumerate() {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let len = n.length();
        if len < 1e-6 || (n.y / len).abs() >= pd_formats::collision::WALKABLE_NORMAL_Y {
            continue;
        }
        let mut best = f32::MAX;
        for h in [STEP_HEIGHT + 1.0, (STEP_HEIGHT + 1.0 + BODY_HEIGHT) * 0.5, BODY_HEIGHT] {
            let c = Vec3::new(probe.x, probe.y + h, probe.z);
            let q = closest(c, t);
            if (c.y - q.y).abs() < 1.0 {
                best = best.min(Vec3::new(c.x - q.x, 0.0, c.z - q.z).length());
            }
        }
        if best < BODY_RADIUS + 1.0 {
            out.push((i, best));
        }
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

/// Closest point on a triangle (brute force over the plane / edges).
fn closest(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let n = (b - a).cross(c - a).normalize_or_zero();
    let q = p - n * (p - a).dot(n);
    let inside = |q: Vec3| {
        let s0 = (b - a).cross(q - a).dot(n);
        let s1 = (c - b).cross(q - b).dot(n);
        let s2 = (a - c).cross(q - c).dot(n);
        s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0
    };
    if inside(q) {
        return q;
    }
    let seg = |p: Vec3, x: Vec3, y: Vec3| {
        let d = y - x;
        let t = ((p - x).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
        x + d * t
    };
    [seg(p, a, b), seg(p, b, c), seg(p, c, a)].into_iter().min_by(|x, y| (*x - p).length_squared().total_cmp(&(*y - p).length_squared())).unwrap()
}

fn describe(a: &Alr, s: Source) -> String {
    match s {
        Source::Object(o) => format!("object #{o}"),
        Source::Stage { model, draw } => {
            let m = &a.models[model];
            let d = &m.draws[draw];
            let vb = m.vertbufs.get(d.vertex_buf as usize);
            let uv = vb.and_then(|vb| a.read_vertices(vb).ok()).is_some_and(|v| v.first().is_some_and(|v| v.uv.is_some()));
            let mat = m.materials.get(d.material_idx as usize);
            format!(
                "model {model} draw {draw} ({}, vb fmt {:?}, material {} tex {:?} flags {:?}, joint {})",
                if uv { "visible, has UVs" } else { "POSITION-ONLY" },
                vb.map(|v| v.format),
                d.material_idx,
                mat.map(|m| m.texture_idx),
                mat.map(|m| m.flags),
                d.transform_idx
            )
        }
    }
}

pub fn run(args: &[String]) -> Result<()> {
    let map_dir = Path::new(&args[1]);
    let id = &args[2];
    let opt = |k: &str| args.iter().position(|x| x == k).and_then(|i| args.get(i + 1)).cloned();
    let secs: f32 = opt("--secs").and_then(|v| v.parse().ok()).unwrap_or(6.0);
    let ndirs: usize = opt("--dirs").and_then(|v| v.parse().ok()).unwrap_or(8);
    let quiet = args.iter().any(|x| x == "--quiet");
    let alr = map_dir.join(format!("{id}.alr"));
    let a = Alr::load(&alr).with_context(|| format!("loading {}", alr.display()))?;
    let props = super::stage_props(&alr, &a);
    let (mut tris, mut owners, mut src) = sourced_triangles(&a, props.as_ref(), args.iter().any(|x| x == "--all-draws"));
    // Experiments: --drop 0:1,2:0 leaves those model:draw pairs out; --only
    // keeps just them (plus nothing else).
    let pairs = |s: String| -> Vec<(usize, usize)> { s.split(',').filter_map(|p| p.split_once(':')).filter_map(|(m, d)| Some((m.parse().ok()?, d.parse().ok()?))).collect() };
    let drop = opt("--drop").map(pairs).unwrap_or_default();
    let only = opt("--only").map(pairs);
    // --rule flag2000 | fmt: experiment with which draws count (with --all-draws).
    let rule = opt("--rule");
    let fmt_of = |m: usize, d: usize| a.models[m].vertbufs.get(a.models[m].draws[d].vertex_buf as usize).map(|v| v.format).unwrap_or(0);
    let flags_of = |m: usize, d: usize| a.models[m].materials.get(a.models[m].draws[d].material_idx as usize).map(|x| x.flags).unwrap_or(0);
    let by_rule = |m: usize, d: usize| match rule.as_deref() {
        Some("flag2000") => !(matches!(fmt_of(m, d), 0x07 | 0x10) && flags_of(m, d) & 0x2000 != 0),
        Some("fmt") => !matches!(fmt_of(m, d), 0x07 | 0x10),
        _ => true,
    };
    let keep: Vec<bool> = src
        .iter()
        .map(|s| match s {
            Source::Stage { model, draw } => by_rule(*model, *draw) && !drop.contains(&(*model, *draw)) && only.as_ref().is_none_or(|o| o.contains(&(*model, *draw))),
            Source::Object(_) => only.is_none(),
        })
        .collect();
    let mut k = keep.iter();
    tris.retain(|_| *k.next().unwrap());
    let mut k = keep.iter();
    owners.retain(|_| *k.next().unwrap());
    let mut k = keep.iter();
    src.retain(|_| *k.next().unwrap());
    // Research: --histo bins the surviving triangles (after --only/--drop) by
    // a 200-unit XZ grid and prints triangle count + floor/wall split + y
    // range per bin, to tell a real floor patch from a disjoint outlier.
    if args.iter().any(|x| x == "--histo") {
        use std::collections::BTreeMap;
        let mut bins: BTreeMap<(i32, i32), (usize, usize, f32, f32)> = BTreeMap::new();
        for t in &tris {
            let c = (t[0] + t[1] + t[2]) / 3.0;
            let key = ((c.x / 200.0).floor() as i32, (c.z / 200.0).floor() as i32);
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            let is_wall = n.length() > 1e-6 && (n.y / n.length()).abs() < pd_formats::collision::WALKABLE_NORMAL_Y;
            let e = bins.entry(key).or_insert((0, 0, f32::MAX, f32::MIN));
            e.0 += 1;
            e.1 += is_wall as usize;
            let (ylo, yhi) = (t[0].y.min(t[1].y).min(t[2].y), t[0].y.max(t[1].y).max(t[2].y));
            e.2 = e.2.min(ylo);
            e.3 = e.3.max(yhi);
        }
        for ((bx, bz), (n, walls, ylo, yhi)) in &bins {
            println!("  bin x[{},{}) z[{},{}): {n:4} tris ({walls} walls) y [{ylo:.0}, {yhi:.0}]", bx * 200, bx * 200 + 200, bz * 200, bz * 200 + 200);
        }
        println!("{id}: {} bins, {} triangles total", bins.len(), tris.len());
    }
    let col = Collision::new_tagged(tris.clone(), &owners);
    if args.iter().any(|x| x == "--compare") {
        return compare(&a, props.as_ref(), id, opt("--map"));
    }
    if let Some(at) = opt("--at") {
        let v: Vec<f32> = at.split(',').filter_map(|x| x.parse().ok()).collect();
        let (x, z) = (v[0], v[1]);
        // Everything over / under (x, z): every model (also the ones the
        // collision leaves out) and every object hull.
        for mi in 0..a.models.len() {
            gltf::model_triangles_each(&a, mi, Mat4::IDENTITY, &mut |di, t| {
                if let Some(y) = height_in(&t, x, z) {
                    let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
                    println!("  at ({x}, {z}): y {y:8.2} n.y {:5.2} {}{}", n.y, describe(&a, Source::Stage { model: mi, draw: di }), if gltf::collision_skips_model(&a, mi, props.as_ref().map(|p| p.skip_models.as_slice()).unwrap_or(&[])) { " [model not in collision]" } else { "" });
                }
            });
        }
        for (t, s) in tris.iter().zip(&src) {
            if let (Some(y), Source::Object(o)) = (height_in(t, x, z), s) {
                println!("  at ({x}, {z}): y {y:8.2} object #{o} hull");
            }
        }
    }
    if args.iter().any(|x| x == "--draws") {
        list_draws(&a, props.as_ref().map(|p| p.skip_models.as_slice()).unwrap_or(&[]));
    }

    let mut starts: Vec<(String, Vec3)> = Vec::new();
    if let Some(f) = opt("--from") {
        let v: Vec<f32> = f.split(',').filter_map(|x| x.parse().ok()).collect();
        starts.push(("from".into(), Vec3::new(v[0], v[1], v[2])));
    } else if let Some(p) = &props {
        for set in p.json["start_sets"].as_array().into_iter().flatten().take(1) {
            for (k, s) in set["starts"].as_array().into_iter().flatten().enumerate() {
                let q = &s["position"];
                let v = |i: usize| q[i].as_f64().unwrap_or(0.0) as f32;
                if opt("--start").is_none_or(|w| w == k.to_string()) {
                    starts.push((format!("start {k}"), Vec3::new(v(0), v(1), v(2))));
                }
            }
        }
    }
    let ticks = (secs * HZ) as u32;
    let mut total_blocked = 0;
    for (name, s) in &starts {
        let g = col.ground_below(s.x, s.z, s.y + 12.0);
        println!("{id} {name} at ({:.1}, {:.1}, {:.1}), ground {:?}", s.x, s.y, s.z, g);
        for k in 0..ndirs {
            let ang = k as f32 / ndirs as f32 * std::f32::consts::TAU;
            // 0 deg = +Z, 90 = +X.
            let dir = Vec3::new(ang.sin(), 0.0, ang.cos());
            let (end, outcome, dist) = walk(&col, *s, dir, ticks);
            if outcome == "BLOCKED" {
                total_blocked += 1;
            }
            println!("  {:5.1} deg: {outcome:7} after {dist:6.1} at ({:.1}, {:.1}, {:.1})", ang.to_degrees(), end.x, end.y, end.z);
            if outcome == "BLOCKED" && !quiet {
                for (i, d) in blockers(&tris, end, dir).into_iter().take(6) {
                    let t = tris[i];
                    let lo = t[0].min(t[1]).min(t[2]);
                    let hi = t[0].max(t[1]).max(t[2]);
                    println!("      tri {i} d {d:.2} box ({:.0},{:.0},{:.0})..({:.0},{:.0},{:.0}) {}", lo.x, lo.y, lo.z, hi.x, hi.y, hi.z, describe(&a, src[i]));
                }
            }
        }
    }
    println!("{id}: {total_blocked} blocked walks");

    // Flood fill: every spot a player can walk to from the first start, and
    // whether the other starts are among them.
    if let Some((_, s0)) = starts.first() {
        let (reach, void_cells) = flood(&col, *s0, 2500.0);
        let void = void_cells.len();
        if args.iter().any(|x| x == "--voids") {
            for p in &void_cells {
                println!("  void edge at ({:.0}, {:.1}, {:.0}), floors under the next 8 units: {:?}", p.x, p.y, p.z, [(8.0f32, 0.0f32), (-8.0, 0.0), (0.0, 8.0), (0.0, -8.0)].map(|(dx, dz)| col.layers_at(p.x + dx, p.z + dz).into_iter().filter(|y| *y < p.y + 50.0).take(3).map(|y| y.round()).collect::<Vec<_>>()));
            }
        }
        println!("{id}: {} cells ({:.0} square units) reachable on foot from {}, {void} cells at an edge into the void", reach.len(), reach.len() as f32 * FILL * FILL, starts[0].0);
        let mut ok = 0;
        for (name, s) in starts.iter().skip(1) {
            let g = col.ground_below(s.x, s.z, s.y + 12.0).unwrap_or(s.y);
            let near = reach.iter().any(|(&(ix, iz, _), &y)| (ix as f32 * FILL - s.x).abs() <= FILL && (iz as f32 * FILL - s.z).abs() <= FILL && (y - g).abs() < 3.0);
            ok += near as usize;
            println!("  {name}: {}", if near { "reachable" } else { "NOT reachable" });
        }
        println!("SUMMARY {id}: blocked walks {total_blocked}, reachable area {:.0}, other starts reachable {ok}/{}, void-edge cells {void}", reach.len() as f32 * FILL * FILL, starts.len() - 1);
        if let Some(out) = opt("--map") {
            std::fs::write(&out, map_png(&col, &tris, &reach, &void_cells, s0, &starts)?)?;
            println!("map -> {out}");
        }
    }
    Ok(())
}

/// `--compare`: the collision with every draw (as before the shadow-volume
/// filter) against the converter's collision, walked from every start point.
/// Reports area reachable on foot, which start points can reach which, and
/// "new holes": spots reachable both ways where only the new collision lets
/// a player walk off into the void.
fn compare(a: &Alr, props: Option<&pd_formats::stagedat::StageProps>, id: &str, map: Option<String>) -> Result<()> {
    type Key = (i32, i32, i32);
    let starts: Vec<Vec3> = props
        .map(|p| {
            p.json["start_sets"][0]["starts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|s| Vec3::new(s["position"][0].as_f64().unwrap_or(0.0) as f32, s["position"][1].as_f64().unwrap_or(0.0) as f32, s["position"][2].as_f64().unwrap_or(0.0) as f32))
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for all in [true, false] {
        let (tris, owners, _) = sourced_triangles(a, props, all);
        let col = Collision::new_tagged(tris.clone(), &owners);
        let mut reach: std::collections::HashMap<Key, f32> = Default::default();
        let mut void: std::collections::HashMap<Key, Vec3> = Default::default();
        let mut pairs = 0;
        let mut solo_area = Vec::new();
        for (i, s) in starts.iter().enumerate() {
            let (r, v) = flood(&col, *s, 2500.0);
            solo_area.push(r.len() as f32 * FILL * FILL);
            for (j, o) in starts.iter().enumerate() {
                let g = col.ground_below(o.x, o.z, o.y + 12.0).unwrap_or(o.y);
                if i != j && r.iter().any(|(&(ix, iz, _), &y)| (ix as f32 * FILL - o.x).abs() <= FILL && (iz as f32 * FILL - o.z).abs() <= FILL && (y - g).abs() < 3.0) {
                    pairs += 1;
                }
            }
            reach.extend(r);
            for p in v {
                void.insert(((p.x / FILL).round() as i32, (p.z / FILL).round() as i32, (p.y / 8.0).round() as i32), p);
            }
        }
        out.push((tris, col, reach, void, pairs, solo_area));
    }
    let (b, n) = (&out[0], &out[1]);
    let new_holes: Vec<Vec3> = n.3.iter().filter(|(k, _)| b.2.contains_key(k) && !b.3.contains_key(k)).map(|(_, p)| *p).collect();
    let pairs_total = starts.len() * starts.len().saturating_sub(1);
    let area = |v: &[f32]| v.iter().map(|a| format!("{a:.0}")).collect::<Vec<_>>().join("/");
    println!(
        "COMPARE {id}: before: area from each start {} , start pairs connected {}/{pairs_total}, void-edge cells {} | after: area {} , pairs {}/{pairs_total}, void-edge cells {} | new holes {}",
        area(&b.5),
        b.4,
        b.3.len(),
        area(&n.5),
        n.4,
        n.3.len(),
        new_holes.len()
    );
    for p in new_holes.iter().take(12) {
        println!("  new hole at ({:.0}, {:.1}, {:.0})", p.x, p.y, p.z);
    }
    if let (Some(m), Some(s0)) = (map, starts.first()) {
        let named: Vec<(String, Vec3)> = starts.iter().map(|s| (String::new(), *s)).collect();
        let voids: Vec<Vec3> = n.3.values().copied().collect();
        std::fs::write(&m, map_png(&n.1, &n.0, &n.2, &voids, s0, &named)?)?;
        let voids_b: Vec<Vec3> = b.3.values().copied().collect();
        let mb = m.replace(".png", "_before.png");
        std::fs::write(&mb, map_png(&b.1, &b.0, &b.2, &voids_b, s0, &named)?)?;
        println!("maps -> {mb}, {m}");
    }
    Ok(())
}

/// Every stage draw that goes into the collision: vertex format, material,
/// joint, triangle count, box, and how many of its triangles are walls.
fn list_draws(a: &Alr, skip: &[usize]) {
    for mi in 0..a.models.len() {
        if skip.contains(&mi) {
            continue;
        }
        let mut per: std::collections::BTreeMap<usize, (usize, usize, Vec3, Vec3)> = Default::default();
        gltf::model_triangles_each(a, mi, Mat4::IDENTITY, &mut |di, t| {
            let e = per.entry(di).or_insert((0, 0, Vec3::MAX, Vec3::MIN));
            e.0 += 1;
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            if n.length() > 1e-6 && (n.y / n.length()).abs() < pd_formats::collision::WALKABLE_NORMAL_Y {
                e.1 += 1;
            }
            for v in t {
                e.2 = e.2.min(v);
                e.3 = e.3.max(v);
            }
        });
        for (di, (n, walls, lo, hi)) in per {
            let m = &a.models[mi];
            let d = &m.draws[di];
            let vb = m.vertbufs.get(d.vertex_buf as usize);
            let mat = m.materials.get(d.material_idx as usize);
            println!(
                "  model {mi:3} draw {di:3}: fmt {:#04x} mat {:2} tex {:?} flags {:#06x} joint {:3} tris {n:5} walls {walls:5} box ({:.0},{:.0},{:.0})..({:.0},{:.0},{:.0}) {}",
                vb.map(|v| v.format).unwrap_or(0),
                d.material_idx,
                mat.map(|m| m.texture_idx),
                mat.map(|m| m.flags).unwrap_or(0),
                d.transform_idx,
                lo.x,
                lo.y,
                lo.z,
                hi.x,
                hi.y,
                hi.z,
                if gltf::collision_keeps_draw(a, mi, di) { "" } else { "(not solid)" }
            );
        }
    }
}

pub(crate) const FILL: f32 = 4.0;

/// Y of `t` at (x, z) if (x, z) is inside it seen from above.
fn height_in(t: &[Vec3; 3], x: f32, z: f32) -> Option<f32> {
    let (a, b, c) = (t[0], t[1], t[2]);
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-9 {
        return None;
    }
    let w0 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let w1 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    let w2 = 1.0 - w0 - w1;
    (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0).then(|| w0 * a.y + w1 * b.y + w2 * c.y)
}

/// Cells (x, z, height band) reachable by walking from `start`, with their
/// floor height. `limit`: how far from the start to look.
pub(crate) fn flood(col: &Collision, start: Vec3, limit: f32) -> (std::collections::HashMap<(i32, i32, i32), f32>, Vec<Vec3>) {
    use std::collections::{HashMap, VecDeque};
    let key = |p: Vec3| ((p.x / FILL).round() as i32, (p.z / FILL).round() as i32, (p.y / 8.0).round() as i32);
    let mut seen: HashMap<(i32, i32, i32), f32> = HashMap::new();
    let mut q = VecDeque::new();
    let g = col.ground_below(start.x, start.z, start.y + 12.0).unwrap_or(start.y);
    let p0 = col.push_out_of_walls(Vec3::new(start.x, g, start.z), BODY_RADIUS, STEP_HEIGHT + 1.0, BODY_HEIGHT);
    let p0 = Vec3::new(p0.x, col.ground_below(p0.x, p0.z, g + STEP_HEIGHT).unwrap_or(g), p0.z);
    seen.insert(key(p0), p0.y);
    q.push_back(p0);
    let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    let mut void: std::collections::HashMap<(i32, i32, i32), Vec3> = Default::default();
    while let Some(p) = q.pop_front() {
        for (dx, dz) in dirs {
            let target = Vec3::new(p.x + dx as f32 * FILL, p.y, p.z + dz as f32 * FILL);
            if (target - start).length() > limit {
                continue;
            }
            let end = match walk_to(col, p, target) {
                Ok(e) => e,
                Err(true) => {
                    void.insert(key(p), p);
                    continue;
                }
                Err(false) => continue,
            };
            let k = key(end);
            if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(k) {
                e.insert(end.y);
                q.push_back(end);
            }
        }
    }
    (seen, void.into_values().collect())
}

/// Walk in a straight line from `from` (standing) to `to` (XZ); the landing
/// point if the body gets there (within a unit) without dropping more than a
/// step. `Err(true)`: the path runs off into the void (no floor, or a drop
/// deep enough to respawn): a hole in the arena edge.
fn walk_to(col: &Collision, from: Vec3, to: Vec3) -> std::result::Result<Vec3, bool> {
    let d = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    let len = d.length();
    let dir = d / len;
    let step = RUN_SPEED / HZ;
    let mut pos = from;
    let mut left = len;
    while left > 1e-3 {
        let s = step.min(left);
        left -= s;
        let mut next = pos + dir * s;
        next = col.push_out_of_walls(next, BODY_RADIUS, STEP_HEIGHT + 1.0, BODY_HEIGHT);
        // A hairline gap between floor triangles is bridged (the sim's
        // coyote ticks): look up to 6 units further on.
        let ground = |k: i32| col.ground_below(next.x + dir.x * k as f32, next.z + dir.z * k as f32, pos.y + STEP_HEIGHT);
        let Some(g) = (0..=6).find_map(|k| ground(k).filter(|g| pos.y - g <= STEP_HEIGHT)) else {
            // Walking off a ledge taller than a step: not "walking" (a drop
            // is a one-way trip, and the fill is about barriers). A drop
            // deep enough to respawn, or no floor at all, is the void.
            return Err(ground(0).is_none_or(|g| pos.y - g > FALL_RESPAWN_BELOW));
        };
        next.y = g;
        pos = next;
    }
    if Vec3::new(pos.x - to.x, 0.0, pos.z - to.z).length() < 1.5 {
        Ok(pos)
    } else {
        Err(false)
    }
}

/// Top-down picture: reachable floor green (brighter = higher), other floor
/// near the start's height grey, walls at body height red, starts white,
/// spots where a player can walk off into the void magenta.
fn map_png(col: &Collision, tris: &[[Vec3; 3]], reach: &std::collections::HashMap<(i32, i32, i32), f32>, void: &[Vec3], s0: &Vec3, starts: &[(String, Vec3)]) -> Result<Vec<u8>> {
    let px = 2.0f32;
    let (lo, hi) = (col.min.max(Vec3::splat(-700.0)), col.max.min(Vec3::splat(700.0)));
    let (w, h) = (((hi.x - lo.x) / px) as i32 + 1, ((hi.z - lo.z) / px) as i32 + 1);
    let mut img = vec![0u8; (w * h * 4) as usize];
    let base = col.ground_below(s0.x, s0.z, s0.y + 12.0).unwrap_or(s0.y);
    let mut put = |x: f32, z: f32, c: [u8; 3]| {
        let (ix, iy) = (((x - lo.x) / px) as i32, ((hi.z - z) / px) as i32);
        if ix >= 0 && iy >= 0 && ix < w && iy < h {
            let o = ((iy * w + ix) * 4) as usize;
            img[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    };
    for iy in 0..h {
        for ix in 0..w {
            let (x, z) = (lo.x + ix as f32 * px, hi.z - iy as f32 * px);
            if let Some(g) = col.ground_below(x, z, base + 60.0) {
                let v = (100.0 + (g - base) * 2.0).clamp(30.0, 220.0) as u8;
                put(x, z, [v / 2, v / 2, v / 2]);
            }
        }
    }
    for (&(ix, iz, _), &y) in reach {
        let v = (150.0 + (y - base) * 2.0).clamp(60.0, 255.0) as u8;
        for (ox, oz) in [(0.0, 0.0), (px, 0.0), (0.0, px), (px, px)] {
            put(ix as f32 * FILL - FILL / 2.0 + ox, iz as f32 * FILL - FILL / 2.0 + oz, [0, v, 0]);
        }
    }
    for t in tris {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let len = n.length();
        if len < 1e-6 || (n.y / len).abs() >= pd_formats::collision::WALKABLE_NORMAL_Y {
            continue;
        }
        let (ylo, yhi) = (t[0].y.min(t[1].y).min(t[2].y), t[0].y.max(t[1].y).max(t[2].y));
        if yhi < base + STEP_HEIGHT + 1.0 || ylo > base + BODY_HEIGHT {
            continue;
        }
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let n = ((b - a).length() / px).ceil().max(1.0) as i32;
            for k in 0..=n {
                let p = a + (b - a) * (k as f32 / n as f32);
                put(p.x, p.z, [230, 40, 40]);
            }
        }
    }
    for p in void {
        for dx in -2..=2 {
            for dz in -2..=2 {
                put(p.x + dx as f32, p.z + dz as f32, [255, 0, 255]);
            }
        }
    }
    for (_, s) in starts {
        for dx in -3..=3 {
            for dz in -3..=3 {
                put(s.x + dx as f32, s.z + dz as f32, [255, 255, 255]);
            }
        }
    }
    pd_formats::texture::Rgba { width: w as u32, height: h as u32, pixels: img }.to_png()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> [[Vec3; 3]; 2] {
        [[a, b, c], [a, c, d]]
    }

    /// A 200 x 200 floor at y 0, optionally fenced by 30-unit walls 50 from the centre.
    fn arena(fenced: bool) -> Collision {
        let s = 100.0;
        let mut t: Vec<[Vec3; 3]> = quad(Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)).into();
        if fenced {
            let r = 50.0;
            let c = [Vec3::new(-r, 0.0, -r), Vec3::new(r, 0.0, -r), Vec3::new(r, 0.0, r), Vec3::new(-r, 0.0, r)];
            for i in 0..4 {
                let (a, b) = (c[i], c[(i + 1) % 4]);
                t.extend(quad(a, b, b + Vec3::Y * 30.0, a + Vec3::Y * 30.0));
            }
        }
        Collision::new(t)
    }

    #[test]
    fn walls_stop_the_walk_and_bound_the_fill() {
        let col = arena(true);
        for k in 0..8 {
            let a = k as f32 / 8.0 * std::f32::consts::TAU;
            let (end, outcome, _) = walk(&col, Vec3::ZERO, Vec3::new(a.sin(), 0.0, a.cos()), 360);
            assert_eq!(outcome, "BLOCKED", "direction {k}");
            assert!(end.x.abs() <= 50.0 && end.z.abs() <= 50.0);
        }
        let (reach, void) = flood(&col, Vec3::ZERO, 2500.0);
        let area = reach.len() as f32 * FILL * FILL;
        assert!((8000.0..=10400.0).contains(&area), "about the 100 x 100 pen: {area}");
        assert!(void.is_empty());
    }

    #[test]
    fn an_unfenced_edge_is_a_void_edge() {
        let col = arena(false);
        let (_, outcome, _) = walk(&col, Vec3::ZERO, Vec3::X, 360);
        assert_eq!(outcome, "fell");
        let (reach, void) = flood(&col, Vec3::ZERO, 2500.0);
        assert!(reach.len() as f32 * FILL * FILL > 35000.0);
        assert!(!void.is_empty());
    }
}
