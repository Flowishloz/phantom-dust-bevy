//! `pd_convert tracecheck <a.pdcl> [<b.pdcl>] --takes <dir> [--list N]`
//!
//! Does a stage collision agree with where the ORIGINAL's fighters stood?
//! The move recorder's per-tick traces (`takes_full/*_trace.csv`, columns
//! `p0_x p0_y p0_z` / `p1_*`, 30 Hz) are positions of the original's own
//! fighters in the program's world, which is the stage `.dat` as stored: our
//! export is z-mirrored (COLLISION.md "The takes are in the .dat's space"),
//! so a trace point (x, y, z) is (x, y, -z) here.
//!
//! Every trace sample, and every 0.5 u step of the straight line between two
//! consecutive samples (so a thin wall cannot be skipped), is tested the way
//! the sim moves a body: a cylinder (radius 4.5, step 7 .. height 20) pushed
//! out of wall triangles. A fighter of the original standing inside a wall of
//! OUR collision means the wall is not in the original. With two files, the
//! report is the difference: spots where `a` pushes the body out and `b` does
//! not (what `a` adds over `b`).
//!
//! Teleports (a step over 25 u: the recorder placing the fighters, a respawn)
//! are not interpolated.

use anyhow::{bail, Context, Result};
use glam::Vec3;
use pd_formats::collision::Collision;
use std::collections::BTreeMap;
use std::path::Path;

const BODY_R: f32 = 4.5;
const BODY_BOTTOM: f32 = 7.0;
const BODY_TOP: f32 = 20.0;
const SUB: f32 = 0.5;
const TELEPORT: f32 = 25.0;
const DEEP: f32 = 1.0;

fn push(col: &Collision, p: Vec3) -> f32 {
    col.push_out_of_walls(p, BODY_R, BODY_BOTTOM, BODY_TOP).distance(p)
}

struct Fighter {
    take: String,
    who: &'static str,
    /// (tick, position in export space)
    pts: Vec<(usize, Vec3)>,
    /// The fighter's state (`p?_state`) at each sample.
    state: Vec<i32>,
}

fn load_takes(dir: &Path) -> Result<Vec<Fighter>> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_trace.csv"))).collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f)?;
        let mut lines = text.lines();
        let head: Vec<&str> = lines.next().context("empty trace")?.split(',').collect();
        let col = |n: &str| head.iter().position(|h| *h == n);
        let name = f.file_name().unwrap().to_string_lossy().trim_end_matches("_trace.csv").to_string();
        for (who, px, py, pz, ps) in [("p0", "p0_x", "p0_y", "p0_z", "p0_state"), ("p1", "p1_x", "p1_y", "p1_z", "p1_state")] {
            let (Some(ix), Some(iy), Some(iz)) = (col(px), col(py), col(pz)) else { continue };
            let is = col(ps);
            let mut pts = Vec::new();
            let mut state = Vec::new();
            for (t, line) in text.lines().skip(1).enumerate() {
                let c: Vec<&str> = line.split(',').collect();
                let g = |i: usize| c.get(i).and_then(|s| s.parse::<f32>().ok());
                if let (Some(x), Some(y), Some(z)) = (g(ix), g(iy), g(iz)) {
                    pts.push((t, Vec3::new(x, y, -z)));
                    state.push(is.and_then(|i| g(i)).map_or(-1, |v| v as i32));
                }
            }
            out.push(Fighter { take: name.clone(), who, pts, state });
        }
        let _ = &mut lines;
    }
    Ok(out)
}

/// Every point to test along a fighter's samples: (tick of the later sample, position).
fn walk(f: &Fighter) -> Vec<(usize, Vec3, i32)> {
    let mut v = Vec::new();
    for (k, (t, p)) in f.pts.iter().enumerate() {
        if k > 0 {
            let (_, q) = f.pts[k - 1];
            let d = p.distance(q);
            if d <= TELEPORT && d > SUB {
                let n = (d / SUB).ceil() as usize;
                for s in 1..n {
                    v.push((*t, q.lerp(*p, s as f32 / n as f32), f.state[k]));
                }
            }
        }
        v.push((*t, *p, f.state[k]));
    }
    v
}

/// Real moves (a step of 0.5 .. 25 u between two samples) whose body-height
/// segments cross a face of `col`: (steps crossing, by (stage/object, class)).
fn crossings(col: &Collision, fighters: &[Fighter]) -> (usize, usize, BTreeMap<(u32, u8), usize>, Vec<String>) {
    let mut by: BTreeMap<(u32, u8), usize> = BTreeMap::new();
    let (mut n, mut moves) = (0usize, 0usize);
    let mut notes = Vec::new();
    for f in fighters {
        let mut per = 0usize;
        for k in 1..f.pts.len() {
            let (t, p) = f.pts[k];
            let (_, q) = f.pts[k - 1];
            let d = p.distance(q);
            if !(0.5..=TELEPORT).contains(&d) {
                continue;
            }
            moves += 1;
            let mut hit = None;
            for h in [BODY_BOTTOM + 2.0, 14.0, BODY_TOP] {
                let (a, b) = (q + Vec3::Y * h, p + Vec3::Y * h);
                let v = b - a;
                let len = v.length();
                if let Some((_, o, c)) = col.raycast_class(a, v / len, len) {
                    hit = Some((o, c));
                    break;
                }
            }
            if let Some(key) = hit {
                n += 1;
                per += 1;
                *by.entry(key).or_default() += 1;
                if per == 1 {
                    notes.push(format!("{} {} tick {t}: step ({:.1}, {:.1}, {:.1}) -> ({:.1}, {:.1}, {:.1}) crosses {} class {:#x}", f.take, f.who, q.x, q.y, q.z, p.x, p.y, p.z, if key.0 == pd_formats::collision::NO_OBJECT { "stage".to_string() } else { format!("object {}", key.0) }, key.1));
                }
            }
        }
    }
    (n, moves, by, notes)
}

/// The original's fighters that FELL (a drop of 4+ u between two samples, not a
/// teleport): would `col` have stood them up (a floor within reach under the
/// first falling sample) or walled them in on the way (the step into the fall
/// crosses a face at body height)? Returns (falls, caught, walled, notes).
fn falls(col: &Collision, fighters: &[Fighter]) -> (usize, usize, usize, Vec<String>) {
    let (mut n, mut caught, mut walled) = (0usize, 0usize, 0usize);
    let mut notes = Vec::new();
    for f in fighters {
        let mut in_fall = false;
        for k in 1..f.pts.len() {
            let (t, p) = f.pts[k];
            let (_, q) = f.pts[k - 1];
            if p.distance(q) > TELEPORT {
                in_fall = false;
                continue;
            }
            let dropping = p.y < q.y - 4.0;
            if dropping && !in_fall {
                n += 1;
                let stood = col.ground_below(p.x, p.z, p.y + 6.0).filter(|g| *g >= p.y - 1.5);
                let mut crossed = None;
                // The step that leaves the deck, at body height (ankle to head).
                for h in [BODY_BOTTOM + 2.0, 14.0, BODY_TOP] {
                    let (a, b) = (q + Vec3::Y * h, p + Vec3::Y * h);
                    let v = b - a;
                    if v.length() > 1e-3 {
                        if let Some((_, o, c)) = col.raycast_class(a, v / v.length(), v.length()) {
                            crossed = Some((o, c));
                            break;
                        }
                    }
                }
                caught += stood.is_some() as usize;
                walled += crossed.is_some() as usize;
                notes.push(format!("{} {} tick {t}: falls at ({:.1}, {:.1}, {:.1}) from y {:.1}: floor under it {}; leaving step {}", f.take, f.who, p.x, p.y, p.z, q.y, stood.map_or("none".to_string(), |g| format!("at {g:.1}")), crossed.map_or("is clear".to_string(), |(o, c)| format!("crosses {} class {:#x}", if o == pd_formats::collision::NO_OBJECT { "stage".to_string() } else { format!("object {o}") }, c))));
            }
            in_fall = dropping;
        }
    }
    (n, caught, walled, notes)
}

pub fn run(args: &[String]) -> Result<()> {
    let files: Vec<&String> = args[1..].iter().take_while(|a| !a.starts_with("--")).collect();
    let takes = args.iter().position(|a| a == "--takes").and_then(|i| args.get(i + 1)).context("--takes <dir> needed")?;
    let list: usize = args.iter().position(|a| a == "--list").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(25);
    if files.is_empty() || files.len() > 2 {
        bail!("usage: tracecheck <a.pdcl> [<b.pdcl>] --takes <dir> [--list N]");
    }
    let a = Collision::from_bytes(&std::fs::read(files[0])?)?;
    let b = match files.get(1) {
        Some(f) => Some(Collision::from_bytes(&std::fs::read(f)?)?),
        None => None,
    };
    let fighters = load_takes(Path::new(takes))?;
    let mut total = 0usize;
    let (mut deep_a, mut deep_b, mut only_a, mut only_b) = (0usize, 0usize, 0usize, 0usize);
    let mut worst_a = 0.0f32;
    // What the deep contacts of `a` are: (owner, class) -> steps.
    let mut culprits: BTreeMap<(u32, u8), usize> = BTreeMap::new();
    let mut states: BTreeMap<i32, usize> = BTreeMap::new();
    let mut rows: Vec<(f32, String)> = Vec::new();
    for f in &fighters {
        let mut first: Option<(usize, Vec3, f32, f32)> = None;
        let mut n_only = 0usize;
        for (t, p, st) in walk(f) {
            total += 1;
            let pa = push(&a, p);
            let pb = b.as_ref().map_or(0.0, |b| push(b, p));
            worst_a = worst_a.max(pa);
            if pa > DEEP {
                deep_a += 1;
                *states.entry(st).or_default() += 1;
                if let Some(c) = a.wall_contacts(p, BODY_R, BODY_BOTTOM, BODY_TOP).into_iter().max_by(|x, y| x.2.total_cmp(&y.2)) {
                    *culprits.entry((c.0, c.1)).or_default() += 1;
                }
            }
            if b.is_some() && pb > DEEP {
                deep_b += 1;
            }
            let diff_a = pa > DEEP && pb <= 0.5;
            let diff_b = b.is_some() && pb > DEEP && pa <= 0.5;
            if b.is_some() {
                only_a += diff_a as usize;
                only_b += diff_b as usize;
            }
            let hit = if b.is_some() { diff_a } else { pa > DEEP };
            if hit {
                n_only += 1;
                if first.is_none_or(|x| pa > x.2) {
                    first = Some((t, p, pa, pb));
                }
            }
        }
        if let Some((t, p, pa, pb)) = first {
            rows.push((pa, format!("{} {} tick {t}: {n_only} steps inside; worst at export ({:.1}, {:.1}, {:.1}) = trace z {:.1}: a pushes {pa:.1}, b {pb:.1}", f.take, f.who, p.x, p.y, p.z, -p.z)));
        }
    }
    println!("samples + sub-steps tested: {total} ({} fighter traces)", fighters.len());
    println!("a: {} steps with the body {DEEP}+ u inside a wall (worst {worst_a:.1} u)", deep_a);
    if b.is_some() {
        println!("b: {deep_b} such steps");
        println!("a only (a pushes, b does not): {only_a}; b only: {only_b}");
    }
    let mut cs: Vec<_> = culprits.into_iter().collect();
    cs.sort_by(|x, y| y.1.cmp(&x.1));
    println!("deepest contact of a, by (stage/object, class): {}", cs.iter().take(12).map(|((o, c), n)| format!("{}{} class {:#x}: {n}", if *o == pd_formats::collision::NO_OBJECT { "stage" } else { "object " }, if *o == pd_formats::collision::NO_OBJECT { String::new() } else { o.to_string() }, c)).collect::<Vec<_>>().join("; "));
    println!("fighter state during those steps (state: steps): {states:?}");
    rows.sort_by(|x, y| y.0.total_cmp(&x.0));
    for (_, r) in rows.iter().take(list) {
        println!("  {r}");
    }
    println!("{} fighter traces affected", rows.len());
    if args.iter().any(|a| a == "--falls") {
        for (name, col) in [("a", Some(&a)), ("b", b.as_ref())] {
            let Some(col) = col else { continue };
            let (n, caught, walled, notes) = falls(col, &fighters);
            println!("{name}: the original's fighters FELL {n} times; this collision would have caught {caught} of them on a floor and walled {walled} in on the way out");
            for r in notes.iter().take(list) {
                println!("    {r}");
            }
        }
    }
    if args.iter().any(|a| a == "--cross") {
        for (name, col) in [("a", Some(&a)), ("b", b.as_ref())] {
            let Some(col) = col else { continue };
            let (n, moves, by, notes) = crossings(col, &fighters);
            let mut cs: Vec<_> = by.into_iter().collect();
            cs.sort_by(|x, y| y.1.cmp(&x.1));
            println!("{name}: {n} of {moves} real moves cross one of its faces at body height; by class: {}", cs.iter().take(8).map(|((o, c), k)| format!("{}{} {:#x}: {k}", if *o == pd_formats::collision::NO_OBJECT { "stage" } else { "object " }, if *o == pd_formats::collision::NO_OBJECT { String::new() } else { o.to_string() }, c)).collect::<Vec<_>>().join("; "));
            for r in notes.iter().take(list) {
                println!("    {r}");
            }
        }
    }
    Ok(())
}
