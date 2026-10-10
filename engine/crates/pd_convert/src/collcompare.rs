//! `pd_convert collcompare <old cache> <new cache> <out dir> [stNN ...]`
//!
//! Where did a stage's collision change between two caches (old: built from
//! ALR render geometry; new: the `.dat` header-0x94 mesh)? Both `.pdcl`
//! files are loaded as the game loads them and compared on a 4-unit grid,
//! layer by layer:
//!
//! * a floor layer the old file has at (x, z) and the new one has no layer
//!   within `TOL` of: "lost". Explained by what the new file has there:
//!   `under` a surface that is still there (a soffit, a hollow, the inside of
//!   a pillar -- old render geometry a fighter could stand on inside a
//!   solid), `shifted` (a floor within 12 u, i.e. stair treads / bevels
//!   replaced by the program's smooth ramp, or a little higher/lower), or
//!   `none` (no floor anywhere near: render-only geometry the program's
//!   collision does not have);
//! * a new layer with no old layer near: "gained" (`over` = above an old
//!   layer, else `none`);
//! * where both have the floor: does a standing body meet a wall (cylinder
//!   push above step height) in one and not the other: wall added / removed;
//! * walking connectivity from the original start points (the walk probe's
//!   flood fill): area each start reaches, start pairs connected, void-edge
//!   cells, and "new holes" (spots a player can now walk off into the void).
//!
//! Output: one line per stage and its biggest regions on stdout, and one
//! top-down picture per stage (`<out>/<stage>_cmp.png`).

use crate::walkprobe::{flood, FILL};
use anyhow::{Context, Result};
use glam::Vec3;
use pd_formats::collision::Collision;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;

const CELL: f32 = 4.0;
const TOL: f32 = 3.5;
const LIMIT: f32 = 900.0;
const BODY_R: f32 = 4.5;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Kind {
    LostUnder,
    LostShifted,
    LostNone,
    GainedOver,
    GainedNone,
    WallAdded,
    WallRemoved,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::LostUnder => "floor lost, under a surface that remains",
            Kind::LostShifted => "floor lost, floor within 12 u (steps/bevels -> smooth)",
            Kind::LostNone => "floor lost, nothing near (render-only geometry)",
            Kind::GainedOver => "floor gained, above an old layer",
            Kind::GainedNone => "floor gained, nothing there before",
            Kind::WallAdded => "wall added",
            Kind::WallRemoved => "wall removed",
        }
    }
    fn colour(self) -> [u8; 3] {
        match self {
            Kind::LostUnder => [120, 0, 0],
            Kind::LostShifted => [255, 140, 0],
            Kind::LostNone => [255, 40, 40],
            Kind::GainedOver => [0, 170, 170],
            Kind::GainedNone => [40, 230, 40],
            Kind::WallAdded => [255, 255, 0],
            Kind::WallRemoved => [80, 140, 255],
        }
    }
}

fn blocked(col: &Collision, x: f32, z: f32, h: f32) -> bool {
    let p = Vec3::new(x, h, z);
    let q = col.push_out_of_walls(p, BODY_R, 7.0, 20.0);
    Vec3::new(q.x - p.x, 0.0, q.z - p.z).length() > 1.0
}

struct Region {
    kind: Kind,
    cells: usize,
    lo: Vec3,
    hi: Vec3,
}

pub fn run(args: &[String]) -> Result<()> {
    let (old_dir, new_dir, out_dir) = (Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3]));
    std::fs::create_dir_all(out_dir)?;
    // --walk x,y,z: walk 16 compass directions for 3 s from that spot on the
    // old and the new file (the walk probe's movement) and print where each
    // ends, for spot-checking a rail or an edge.
    if let Some(i) = args.iter().position(|a| a == "--walk") {
        let v: Vec<f32> = args.get(i + 1).map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
        anyhow::ensure!(v.len() == 3 && args.len() > 4 && !args[4].starts_with("--"), "usage: collcompare <old> <new> <out> stNN --walk x,y,z");
        let id = &args[4];
        let cols = [("old", Collision::from_bytes(&std::fs::read(old_dir.join(format!("{id}.pdcl")))?)?), ("new", Collision::from_bytes(&std::fs::read(new_dir.join(format!("{id}.pdcl")))?)?)];
        for k in 0..16 {
            let ang = k as f32 / 16.0 * std::f32::consts::TAU;
            let dir = Vec3::new(ang.sin(), 0.0, ang.cos());
            let mut line = format!("{id} from ({:.0}, {:.0}, {:.0}) heading {:5.1} deg:", v[0], v[1], v[2], ang.to_degrees());
            for (name, col) in &cols {
                let (end, outcome, dist) = crate::walkprobe::walk(col, Vec3::new(v[0], v[1], v[2]), dir, 180);
                line += &format!("  {name}: {outcome:7} {dist:5.0} u to y {:.0}", end.y);
            }
            println!("{line}");
        }
        return Ok(());
    }
    let mut ids: Vec<String> = args[4..].iter().filter(|a| !a.starts_with("--")).cloned().collect();
    if ids.is_empty() {
        ids = std::fs::read_dir(new_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .filter(|n| n.starts_with("st") && n.ends_with(".pdcl"))
            .map(|n| n.trim_end_matches(".pdcl").to_string())
            .collect();
        ids.sort();
    }
    // One picture/line per distinct collision file pair (st12/st22 repeat st02...).
    let mut seen: HashMap<(Vec<u8>, Vec<u8>), String> = HashMap::new();
    for id in &ids {
        let ob = std::fs::read(old_dir.join(format!("{id}.pdcl"))).with_context(|| format!("{id} old"))?;
        let nb = std::fs::read(new_dir.join(format!("{id}.pdcl"))).with_context(|| format!("{id} new"))?;
        if ob == nb {
            println!("CMP {id}: file identical");
            continue;
        }
        if let Some(first) = seen.get(&(ob.clone(), nb.clone())) {
            println!("CMP {id}: same change as {first}");
            continue;
        }
        seen.insert((ob.clone(), nb.clone()), id.clone());
        let (old, new) = (Collision::from_bytes(&ob)?, Collision::from_bytes(&nb)?);
        let starts = starts_of(new_dir, id);
        compare(id, &old, &new, &starts, new_dir, out_dir)?;
    }
    Ok(())
}

fn spots_of(dir: &Path, id: &str) -> Vec<(f32, f32, f32)> {
    let Ok(t) = std::fs::read_to_string(dir.join(format!("{id}.props.json"))) else { return Vec::new() };
    let Ok(j) = serde_json::from_str::<serde_json::Value>(&t) else { return Vec::new() };
    let pt = |v: &serde_json::Value| Some((v[0].as_f64()? as f32, v[1].as_f64()? as f32, v[2].as_f64()? as f32));
    let mut out = Vec::new();
    for set in j["start_sets"].as_array().into_iter().flatten() {
        for s in set["starts"].as_array().into_iter().flatten() {
            out.extend(pt(&s["position"]));
            for c in s["capsules"].as_array().into_iter().flatten() {
                out.extend(pt(c));
            }
        }
    }
    out
}

fn starts_of(dir: &Path, id: &str) -> Vec<Vec3> {
    let Ok(t) = std::fs::read_to_string(dir.join(format!("{id}.props.json"))) else { return Vec::new() };
    let Ok(j) = serde_json::from_str::<serde_json::Value>(&t) else { return Vec::new() };
    j["start_sets"][0]["starts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| Vec3::new(s["position"][0].as_f64().unwrap_or(0.0) as f32, s["position"][1].as_f64().unwrap_or(0.0) as f32, s["position"][2].as_f64().unwrap_or(0.0) as f32))
        .collect()
}

fn compare(id: &str, old: &Collision, new: &Collision, starts: &[Vec3], new_dir: &Path, out_dir: &Path) -> Result<()> {
    let lo = old.min.min(new.min).max(Vec3::splat(-LIMIT));
    let hi = old.max.max(new.max).min(Vec3::splat(LIMIT));
    // Walking connectivity from the original start points first: only floor
    // a fighter can walk onto (in the old or the new file) is compared; the
    // old render geometry has roofs, sky shells and outdoor ground no one
    // can reach, which would drown the picture.
    let conn = |col: &Collision| {
        let (mut pairs, mut areas, mut void) = (0, Vec::new(), HashMap::new());
        let mut reach: HashMap<(i32, i32), Vec<f32>> = HashMap::new();
        for (i, s) in starts.iter().enumerate() {
            let (r, v) = flood(col, *s, 2500.0);
            areas.push(r.len() as f32 * FILL * FILL);
            for (j, o) in starts.iter().enumerate() {
                let g = col.ground_below(o.x, o.z, o.y + 12.0).unwrap_or(o.y);
                if i != j && r.iter().any(|(&(ix, iz, _), &y)| (ix as f32 * FILL - o.x).abs() <= FILL && (iz as f32 * FILL - o.z).abs() <= FILL && (y - g).abs() < 3.0) {
                    pairs += 1;
                }
            }
            for (&(ix, iz, _), &y) in &r {
                reach.entry((ix, iz)).or_default().push(y);
            }
            for p in v {
                void.insert(((p.x / FILL).round() as i32, (p.z / FILL).round() as i32, (p.y / 8.0).round() as i32), p);
            }
        }
        (pairs, areas, void, reach)
    };
    let (po, ao, vo, ro) = conn(old);
    let (pn, an, vn, rn) = conn(new);
    let walkable = |ix: i32, iz: i32, h: f32| [&ro, &rn].iter().any(|r| r.get(&(ix, iz)).is_some_and(|v| v.iter().any(|y| (y - h).abs() <= TOL + 1.0)));
    let (ix0, iz0) = ((lo.x / CELL).floor() as i32, (lo.z / CELL).floor() as i32);
    let (nx, nz) = (((hi.x - lo.x) / CELL) as i32 + 2, ((hi.z - lo.z) / CELL) as i32 + 2);
    // (ix, iz) (world cell = ix * CELL) -> [(height, kind)]
    let mut changes: HashMap<(i32, i32), Vec<(f32, Kind)>> = HashMap::new();
    let mut same: HashMap<(i32, i32), f32> = HashMap::new();
    let (mut old_cells, mut new_cells) = (0usize, 0usize);
    for jz in 0..nz {
        for jx in 0..nx {
            let (ix, iz) = (ix0 + jx, iz0 + jz);
            let (x, z) = (ix as f32 * CELL, iz as f32 * CELL);
            let (lo_l, ln_l) = (old.layers_at(x, z), new.layers_at(x, z));
            let near = |v: &[f32], h: f32, t: f32| v.iter().any(|y| (y - h).abs() <= t);
            for &h in &lo_l {
                if !walkable(ix, iz, h) {
                    continue;
                }
                old_cells += 1;
                if near(&ln_l, h, TOL) {
                    let hn = ln_l.iter().copied().min_by(|a, b| (a - h).abs().total_cmp(&(b - h).abs())).unwrap();
                    let (bo, bn) = (blocked(old, x, z, h), blocked(new, x, z, hn));
                    let c = same.entry((ix, iz)).or_insert(h);
                    *c = c.max(h);
                    if bn && !bo {
                        changes.entry((ix, iz)).or_default().push((h, Kind::WallAdded));
                    } else if bo && !bn {
                        changes.entry((ix, iz)).or_default().push((h, Kind::WallRemoved));
                    }
                } else {
                    let kind = if ln_l.iter().any(|y| *y > h && *y - h <= 40.0) {
                        Kind::LostUnder
                    } else if near(&ln_l, h, 12.0) {
                        Kind::LostShifted
                    } else {
                        Kind::LostNone
                    };
                    changes.entry((ix, iz)).or_default().push((h, kind));
                }
            }
            for &h in &ln_l {
                if !walkable(ix, iz, h) {
                    continue;
                }
                new_cells += 1;
                if !near(&lo_l, h, TOL) {
                    let kind = if lo_l.iter().any(|y| *y < h && h - *y <= 40.0) { Kind::GainedOver } else { Kind::GainedNone };
                    changes.entry((ix, iz)).or_default().push((h, kind));
                }
            }
        }
    }
    // Area per kind, and regions (connected cells of one kind at similar height).
    let mut area: BTreeMap<Kind, usize> = BTreeMap::new();
    let mut per_kind: HashMap<Kind, HashMap<(i32, i32), f32>> = HashMap::new();
    for (&(ix, iz), v) in &changes {
        for &(h, k) in v {
            *area.entry(k).or_default() += 1;
            per_kind.entry(k).or_default().insert((ix, iz), h);
        }
    }
    let mut regions: Vec<Region> = Vec::new();
    for (&k, cells) in &per_kind {
        let mut done: std::collections::HashSet<(i32, i32)> = Default::default();
        for (&c0, &h0) in cells {
            if done.contains(&c0) {
                continue;
            }
            let (mut q, mut n) = (VecDeque::from([(c0, h0)]), 0usize);
            done.insert(c0);
            let (mut rlo, mut rhi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            while let Some(((ix, iz), h)) = q.pop_front() {
                n += 1;
                let p = Vec3::new(ix as f32 * CELL, h, iz as f32 * CELL);
                rlo = rlo.min(p);
                rhi = rhi.max(p);
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let c = (ix + dx, iz + dz);
                    if let Some(&hh) = cells.get(&c) {
                        if !done.contains(&c) && (hh - h).abs() < 8.0 {
                            done.insert(c);
                            q.push_back((c, hh));
                        }
                    }
                }
            }
            regions.push(Region { kind: k, cells: n, lo: rlo, hi: rhi });
        }
    }
    regions.sort_by_key(|r| std::cmp::Reverse(r.cells));

    let new_holes: Vec<Vec3> = vn.iter().filter(|(k, _)| !vo.contains_key(k)).map(|(_, p)| *p).collect();
    let fmt = |v: &[f32]| v.iter().map(|a| format!("{a:.0}")).collect::<Vec<_>>().join("/");
    let a = |k: Kind| area.get(&k).copied().unwrap_or(0) as f32 * CELL * CELL;
    println!("CMP {id}: walkable floor cells (either file can walk there) old {old_cells} new {new_cells} (cell {CELL} u)");
    for k in [Kind::LostUnder, Kind::LostShifted, Kind::LostNone, Kind::GainedOver, Kind::GainedNone, Kind::WallAdded, Kind::WallRemoved] {
        println!("    {:58} {:9.0} sq u", k.name(), a(k));
    }
    println!(
        "    walk reach per start old {} new {} | start pairs connected old {po} new {pn} of {} | void-edge cells old {} new {} | spots newly open to the void {}",
        fmt(&ao),
        fmt(&an),
        starts.len() * starts.len().saturating_sub(1),
        vo.len(),
        vn.len(),
        new_holes.len()
    );
    for r in regions.iter().take(10) {
        println!(
            "    region {:58} {:7.0} sq u  x {:.0}..{:.0} z {:.0}..{:.0} y {:.0}..{:.0}",
            r.kind.name(),
            r.cells as f32 * CELL * CELL,
            r.lo.x,
            r.hi.x,
            r.lo.z,
            r.hi.z,
            r.lo.y,
            r.hi.y
        );
    }
    // Every start point and capsule spot the .dat places (all layouts): is
    // there still ground under it, at about the same height?
    let (mut spots, mut gone, mut moved) = (0, Vec::new(), 0);
    for (x, y, z) in spots_of(new_dir, id) {
        spots += 1;
        let (go, gn) = (old.ground_below(x, z, y + 12.0), new.ground_below(x, z, y + 12.0));
        match (go, gn) {
            (Some(_), None) => gone.push((x, y, z)),
            (Some(a), Some(b)) if (a - b).abs() > 3.0 => moved += 1,
            _ => {}
        }
    }
    println!("    start/capsule spots {spots}: ground lost under {} , ground moved > 3 u under {moved}", gone.len());
    for (x, y, z) in gone.iter().take(6) {
        println!("      no ground under ({x:.0}, {y:.0}, {z:.0})");
    }
    // Of those, how many were closed by an old (render) wall right beside
    // them: a rail / edge closer the 0x94 mesh does not have.
    let is_railed = |p: &&Vec3| [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.7, 0.7), (0.7, -0.7), (-0.7, 0.7), (-0.7, -0.7)].iter().any(|(dx, dz)| blocked(old, p.x + dx * 6.0, p.z + dz * 6.0, p.y));
    let railed = new_holes.iter().filter(is_railed).count();
    for p in new_holes.iter().filter(is_railed).take(4) {
        println!("      railed in old, open now: ({:.0}, {:.1}, {:.0})", p.x, p.y, p.z);
    }
    println!("    of the {} new void-edge spots, {railed} have an old render wall within 6 u (rail / edge closer the 0x94 mesh lacks)", new_holes.len());
    for p in new_holes.iter().take(8) {
        println!("    new hole at ({:.0}, {:.1}, {:.0})", p.x, p.y, p.z);
    }

    // Picture: grey = walkable floor in both (brighter = higher); the change
    // kinds painted over it, low layers first; new holes white; starts ringed.
    // Crop to what there is to see (walkable floor in either file), plus a margin.
    let keys = || same.keys().chain(changes.keys()).copied();
    let (ix0, iz0, ix1, iz1) = keys().fold((i32::MAX, i32::MAX, i32::MIN, i32::MIN), |a, (x, z)| (a.0.min(x), a.1.min(z), a.2.max(x), a.3.max(z)));
    let (ix0, iz0, nx, nz) = if ix0 <= ix1 { (ix0 - 6, iz0 - 6, ix1 - ix0 + 13, iz1 - iz0 + 13) } else { (ix0, iz0, nx, nz) };
    // Pixels per cell side: as large as keeps the picture within ~1400 px.
    let px = (1400 / nx.max(nz).max(1)).clamp(2, 8);
    let (w, h) = (nx * px, nz * px);
    let mut img = vec![20u8; (w * h * 4) as usize];
    for p in img.chunks_exact_mut(4) {
        p[3] = 255;
    }
    let mut put = |ix: i32, iz: i32, c: [u8; 3]| {
        let (jx, jz) = (ix - ix0, iz - iz0);
        for dy in 0..px {
            for dx in 0..px {
                let (x, y) = (jx * px + dx, (nz - 1 - jz) * px + dy);
                if x >= 0 && y >= 0 && x < w && y < h {
                    let o = ((y * w + x) * 4) as usize;
                    img[o..o + 3].copy_from_slice(&c);
                }
            }
        }
    };
    for (&(ix, iz), &y) in &same {
        let v = (70.0 + (y - new.min.y) * 0.4).clamp(60.0, 170.0) as u8;
        put(ix, iz, [v, v, v]);
    }
    let mut painted: Vec<((i32, i32), f32, Kind)> = changes.iter().flat_map(|(&c, v)| v.iter().map(move |&(h, k)| (c, h, k))).collect();
    painted.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (c, _, k) in painted {
        put(c.0, c.1, k.colour());
    }
    let cell_of = |v: f32| (v / CELL).round() as i32;
    for p in &new_holes {
        let (ix, iz) = (cell_of(p.x), cell_of(p.z));
        for d in -1..=1 {
            put(ix + d, iz, [255, 255, 255]);
            put(ix, iz + d, [255, 255, 255]);
        }
    }
    for st in starts {
        let (ix, iz) = (cell_of(st.x), cell_of(st.z));
        for d in -2..=2 {
            put(ix + d, iz - 2, [0, 0, 0]);
            put(ix + d, iz + 2, [0, 0, 0]);
            put(ix - 2, iz + d, [0, 0, 0]);
            put(ix + 2, iz + d, [0, 0, 0]);
        }
    }
    let png = pd_formats::texture::Rgba { width: w as u32, height: h as u32, pixels: img }.to_png()?;
    let path = out_dir.join(format!("{id}_cmp.png"));
    std::fs::write(&path, png)?;
    println!("    picture -> {} (x {:.0}..{:.0}, z {:.0}..{:.0}, {px} px per 4 u, north = +z up)", path.display(), ix0 as f32 * CELL, (ix0 + nx) as f32 * CELL, iz0 as f32 * CELL, (iz0 + nz) as f32 * CELL);
    Ok(())
}
