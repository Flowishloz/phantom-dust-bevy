//! Dev-only frame-cost probe: where the time goes, per render pass.
//!
//! `PD_GPU_TIMING=1` adds Bevy's `RenderDiagnosticsPlugin` (GPU timestamp
//! queries on Vulkan / DX12; the device already has the feature, `WgpuSettings`
//! defaults to every feature the adapter has) and, when the run ends (the
//! `PD_FRAME_STATS` exit), prints every pass's cost per frame:
//!
//! `[gpu-timing] <pass path> gpu 1.234 ms cpu 0.056 ms spans/frame 4.0`
//!
//! "Per frame" is the SUM over the frame's spans of that name, so a pass that
//! runs once per camera shows the cost of all the cameras together, and
//! `spans/frame` tells how many ran. Warm-up frames (`PD_FRAME_STATS`' second
//! number, default 200) are skipped. Measurement only: it reads the render
//! world's diagnostics and changes nothing about what is drawn, and without the
//! env var this module does nothing at all.

use bevy::app::AppExit;
use bevy::diagnostic::DiagnosticsStore;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::time::Instant;

/// Whether the probe is requested.
pub fn wanted() -> bool {
    std::env::var_os("PD_GPU_TIMING").is_some()
}

pub fn plugin(app: &mut App) {
    // The CPU split (fixed ticks, main schedules) rides on `PD_FRAME_STATS`: no extra switch.
    if std::env::var_os("PD_FRAME_STATS").is_some() {
        app.init_resource::<CpuSplit>()
            .init_resource::<Churn>()
            .add_systems(Last, churn_count)
            .add_systems(First, split_first)
            .add_systems(Last, (split_last, split_report).chain())
            .add_systems(FixedFirst, split_fixed_begin)
            .add_systems(FixedLast, split_fixed_end);
    }
    if std::env::var_os("PD_FRAME_STATS").is_some() {
        render_markers(app);
    }
    if !wanted() {
        return;
    }
    app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin).init_resource::<Totals>().add_systems(Last, (collect, report).chain());
}

/// Where a frame's wall-clock time goes on the CPU: the fixed (sim) ticks, the
/// rest of the main world's schedules (`First` .. `Last`), and what is left
/// (extract, waiting for the render thread, presenting).
#[derive(Resource, Default)]
struct CpuSplit {
    frame: u32,
    frame_start: Option<Instant>,
    prev_start: Option<Instant>,
    fixed_start: Option<Instant>,
    /// This frame's fixed ticks and the time they took, ms.
    ticks: u32,
    fixed_ms: f64,
    /// Per measured frame: (wall ms, main ms First..Last, fixed ms, ticks).
    rows: Vec<(f64, f64, f64, u32)>,
}

fn split_first(mut s: ResMut<CpuSplit>) {
    s.frame_start = Some(Instant::now());
}

fn split_fixed_begin(mut s: ResMut<CpuSplit>) {
    s.fixed_start = Some(Instant::now());
}

fn split_fixed_end(mut s: ResMut<CpuSplit>) {
    if let Some(t) = s.fixed_start.take() {
        s.fixed_ms += t.elapsed().as_secs_f64() * 1000.0;
        s.ticks += 1;
    }
}

fn split_last(mut s: ResMut<CpuSplit>) {
    s.frame += 1;
    let (Some(start), Some(now)) = (s.frame_start, Some(Instant::now())) else { return };
    let main_ms = (now - start).as_secs_f64() * 1000.0;
    let wall_ms = s.prev_start.map_or(0.0, |p| (start - p).as_secs_f64() * 1000.0);
    s.prev_start = Some(start);
    if s.frame > skip_frames() {
        let row = (wall_ms, main_ms, s.fixed_ms, s.ticks);
        s.rows.push(row);
        // `PD_FRAME_STATS`' third number: split every slow frame (main schedules vs sim vs the rest).
        if spike_ms().is_some_and(|ms| wall_ms > ms) {
            eprintln!("[split-spike] frame {} wall {:.1} ms: main {:.1} (sim {:.1}, {} ticks), other {:.1}", s.frame, wall_ms, main_ms, s.fixed_ms, s.ticks, wall_ms - main_ms);
        }
    }
    s.ticks = 0;
    s.fixed_ms = 0.0;
}

fn split_report(mut exits: MessageReader<AppExit>, s: Res<CpuSplit>, churn: Res<Churn>, cache: Option<Res<crate::effects::FxMatCache>>, warm: Option<Res<crate::effects::FxWarmup>>, rows: Option<Res<crate::effects::FxInstances>>) {
    if exits.read().count() == 0 || s.rows.is_empty() {
        return;
    }
    render_report();
    churn_report(&churn, cache.as_deref(), warm.as_deref());
    if let Some(r) = rows {
        eprintln!("[fx-rows] per-instance rows: live at exit {}, most at once {}, pieces that found no free row {}", r.live(), r.high, r.overflow);
    }
    let n = s.rows.len() as f64;
    let mean = |f: fn(&(f64, f64, f64, u32)) -> f64| s.rows.iter().map(f).sum::<f64>() / n;
    let mut med: Vec<f64> = s.rows.iter().map(|r| r.0).collect();
    med.sort_by(|a, b| a.total_cmp(b));
    eprintln!(
        "[cpu-split] frames {} wall mean {:.2} ms (median {:.2}) | main schedules First..Last {:.2} ms of which fixed ticks {:.2} ms ({:.2} ticks/frame, {:.2} ms/tick) | rest (extract, render wait, present) {:.2} ms",
        s.rows.len(),
        mean(|r| r.0),
        med[med.len() / 2],
        mean(|r| r.1),
        mean(|r| r.2),
        mean(|r| r.3 as f64),
        mean(|r| r.2) / mean(|r| r.3 as f64).max(0.001),
        (mean(|r| r.0) - mean(|r| r.1)).max(0.0)
    );
}

/// One pass's running totals over the measured frames.
#[derive(Default, Clone, Copy)]
struct Acc {
    /// Frames this path was measured in.
    frames: u32,
    /// Sum over frames of (sum of the frame's spans), ms.
    total: f64,
    /// Spans seen across those frames.
    spans: u64,
    /// Time of the newest measurement already counted.
    last: Option<Instant>,
}

#[derive(Resource, Default)]
struct Totals {
    seen_frames: u32,
    gpu: HashMap<String, Acc>,
    cpu: HashMap<String, Acc>,
}

fn skip_frames() -> u32 {
    std::env::var("PD_FRAME_STATS").ok().and_then(|v| v.split(',').nth(1).and_then(|s| s.trim().parse().ok())).unwrap_or(200)
}

/// `PD_FRAME_STATS`' third number: the slow-frame threshold in ms, if given.
fn spike_ms() -> Option<f64> {
    std::env::var("PD_FRAME_STATS").ok().and_then(|v| v.split(',').nth(2).and_then(|s| s.trim().parse().ok()))
}

fn collect(store: Res<DiagnosticsStore>, mut totals: ResMut<Totals>) {
    totals.seen_frames += 1;
    if totals.seen_frames <= skip_frames() {
        return;
    }
    for d in store.iter() {
        let path = d.path().as_str();
        let (is_gpu, name) = if let Some(n) = path.strip_suffix("/elapsed_gpu") {
            (true, n)
        } else if let Some(n) = path.strip_suffix("/elapsed_cpu") {
            (false, n)
        } else {
            continue;
        };
        let Some(newest) = d.measurement() else { continue };
        let stamp = newest.time;
        // The measurements of one frame share one timestamp (`sync_diagnostics`).
        let (sum, n) = d.measurements().filter(|m| m.time == stamp).fold((0.0, 0u64), |(s, n), m| (s + m.value, n + 1));
        let map = if is_gpu { &mut totals.gpu } else { &mut totals.cpu };
        let acc = map.entry(name.to_string()).or_default();
        if acc.last == Some(stamp) {
            continue;
        }
        acc.last = Some(stamp);
        acc.frames += 1;
        acc.total += sum;
        acc.spans += n;
    }
}

fn report(mut exits: MessageReader<AppExit>, totals: Res<Totals>) {
    if exits.read().count() == 0 {
        return;
    }
    // Frames measured: the most any pass saw.
    let frames = totals.gpu.values().chain(totals.cpu.values()).map(|a| a.frames).max().unwrap_or(0).max(1) as f64;
    let mut rows: Vec<(&String, f64, f64, f64)> = totals
        .gpu
        .iter()
        .map(|(k, a)| {
            let cpu = totals.cpu.get(k).map_or(0.0, |c| c.total / frames);
            (k, a.total / frames, cpu, a.spans as f64 / a.frames.max(1) as f64)
        })
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    let sum: f64 = rows.iter().map(|r| r.1).sum();
    eprintln!("[gpu-timing] frames {frames} passes {} (sums of spans; nested spans overlap, so the total is only a guide) total {sum:.3} ms", rows.len());
    for (k, gpu, cpu, spans) in rows.iter().take(40) {
        eprintln!("[gpu-timing] {k} gpu {gpu:.3} ms cpu {cpu:.3} ms spans/frame {spans:.1}");
    }
}

// ---- Where the render thread's time goes (CPU), by the render schedule's own phases ----

use bevy::render::{Render, RenderApp, RenderSystems};
use std::sync::Mutex;

/// Phase names in the order the `Render` schedule runs them (the interval AFTER each set).
const PHASES: [&str; 16] = [
    "extract-commands",
    "prepare-assets",
    "prepare-meshes",
    "create-views",
    "specialize",
    "prepare-views",
    "queue",
    "phase-sort",
    "prep-resources",
    "prep-batch-phases",
    "prep-write-phase-buffers",
    "prep-collect-phase-buffers",
    "prep-flush",
    "prep-bind-groups",
    "render(encode+submit)",
    "cleanup",
];

struct RenderTimes {
    last: Option<Instant>,
    frames: u32,
    ms: [f64; PHASES.len()],
}

static RENDER_TIMES: Mutex<RenderTimes> = Mutex::new(RenderTimes { last: None, frames: 0, ms: [0.0; PHASES.len()] });

fn render_mark(i: usize) {
    let Ok(mut t) = RENDER_TIMES.lock() else { return };
    let now = Instant::now();
    if i == 0 {
        t.last = Some(now);
        t.frames += 1;
        return;
    }
    if let Some(last) = t.last {
        // Intervals after the first `skip_frames` frames only.
        if t.frames > skip_frames() {
            let ms = (now - last).as_secs_f64() * 1000.0;
            t.ms[i - 1] += ms;
            // With a slow-frame threshold: any one render phase taking over half of it.
            if spike_ms().is_some_and(|s| ms > s * 0.5) {
                eprintln!("[render-spike] render frame {} {} {ms:.1} ms", t.frames, PHASES[i - 1]);
            }
        }
    }
    t.last = Some(now);
}

fn render_markers(app: &mut App) {
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    let sets = [
        RenderSystems::ExtractCommands,
        RenderSystems::PrepareAssets,
        RenderSystems::PrepareMeshes,
        RenderSystems::CreateViews,
        RenderSystems::Specialize,
        RenderSystems::PrepareViews,
        RenderSystems::Queue,
        RenderSystems::PhaseSort,
        RenderSystems::PrepareResources,
        RenderSystems::PrepareResourcesBatchPhases,
        RenderSystems::PrepareResourcesWritePhaseBuffers,
        RenderSystems::PrepareResourcesCollectPhaseBuffers,
        RenderSystems::PrepareResourcesFlush,
        RenderSystems::PrepareBindGroups,
        RenderSystems::Render,
        RenderSystems::Cleanup,
    ];
    // Marker 0 runs before the first phase; marker k+1 after phase k and before phase k+1.
    render.add_systems(Render, (|| render_mark(0)).before(RenderSystems::ExtractCommands));
    for (k, set) in sets.iter().cloned().enumerate() {
        let mut m = (move || render_mark(k + 1)).after(set);
        if let Some(next) = sets.get(k + 1).cloned() {
            m = m.before(next);
        }
        render.add_systems(Render, m);
    }
}

fn render_report() {
    let Ok(t) = RENDER_TIMES.lock() else { return };
    let measured = t.frames.saturating_sub(skip_frames()).max(1) as f64;
    let parts: Vec<String> = PHASES.iter().zip(t.ms.iter()).map(|(n, ms)| format!("{n} {:.2}", ms / measured)).collect();
    let total: f64 = t.ms.iter().sum::<f64>() / measured;
    eprintln!("[render-split] frames {} ms/frame: {} | total {total:.2} ms", measured as u32, parts.join(", "));
}

// ---- Material churn: how many material assets are rewritten each frame (each one is re-prepared by the render thread) ----

#[derive(Resource, Default)]
struct Churn {
    frames: u32,
    fx: u64,
    dissolve: u64,
    standard: u64,
    stage: u64,
    pieces: u64,
    // Lane fxperf (2026-10-08): what a cast costs in asset churn, so a PC run shows the material
    // cache / mesh reuse before and after (`PD_FX_MATCACHE=0`, `PD_FX_MESHREUSE=0`).
    /// Fx materials created / freed (their last handle gone) and the most created in one frame.
    fx_added: u64,
    fx_freed: u64,
    fx_added_max: u64,
    /// Meshes created / rewritten / freed.
    mesh_added: u64,
    mesh_modified: u64,
    mesh_freed: u64,
    /// Effect slots started (each cast, hit, impact ... starts one or more).
    effects: u64,
    /// The material cache's own counters when measuring began (made, reused, shared hits).
    cache_base: Option<(u64, u64, u64)>,
}

#[allow(clippy::too_many_arguments)]
fn churn_count(
    mut c: ResMut<Churn>,
    mut fx: MessageReader<AssetEvent<crate::effects::FxMaterial>>,
    mut dissolve: MessageReader<AssetEvent<crate::cam_fade::DissolveMaterial>>,
    mut standard: MessageReader<AssetEvent<StandardMaterial>>,
    mut stage: MessageReader<AssetEvent<crate::stage_material::StageMaterial>>,
    pieces: Query<(), With<crate::effects::FxPiece>>,
    mut mesh_ev: MessageReader<AssetEvent<Mesh>>,
    new_fx: Query<(), Added<crate::effects::FxInstance>>,
    cache: Option<Res<crate::effects::FxMatCache>>,
) {
    let (mut fx_added, mut fx_freed, mut fx_modified) = (0u64, 0u64, 0u64);
    for ev in fx.read() {
        match ev {
            AssetEvent::Added { .. } => fx_added += 1,
            AssetEvent::Unused { .. } => fx_freed += 1,
            AssetEvent::Modified { .. } => fx_modified += 1,
            _ => {}
        }
    }
    let (mut mesh_added, mut mesh_modified, mut mesh_freed) = (0u64, 0u64, 0u64);
    for ev in mesh_ev.read() {
        match ev {
            AssetEvent::Added { .. } => mesh_added += 1,
            AssetEvent::Unused { .. } => mesh_freed += 1,
            AssetEvent::Modified { .. } => mesh_modified += 1,
            _ => {}
        }
    }
    let (a, b, d, e) = (
        fx_modified,
        dissolve.read().filter(|e| matches!(e, AssetEvent::Modified { .. })).count() as u64,
        standard.read().filter(|e| matches!(e, AssetEvent::Modified { .. })).count() as u64,
        stage.read().filter(|e| matches!(e, AssetEvent::Modified { .. })).count() as u64,
    );
    c.frames += 1;
    if c.frames <= skip_frames() {
        return;
    }
    if c.cache_base.is_none() {
        c.cache_base = Some(cache.as_ref().map_or((0, 0, 0), |k| (k.made, k.reused, k.shared_hits)));
    }
    c.fx_added += fx_added;
    c.fx_freed += fx_freed;
    c.fx_added_max = c.fx_added_max.max(fx_added);
    c.mesh_added += mesh_added;
    c.mesh_modified += mesh_modified;
    c.mesh_freed += mesh_freed;
    c.effects += new_fx.iter().count() as u64;
    c.fx += a;
    c.dissolve += b;
    c.standard += d;
    c.stage += e;
    c.pieces += pieces.iter().count() as u64;
}

fn churn_report(c: &Churn, cache: Option<&crate::effects::FxMatCache>, warm: Option<&crate::effects::FxWarmup>) {
    let n = c.frames.saturating_sub(skip_frames()).max(1) as f64;
    let per_fx = |v: u64| v as f64 / c.effects.max(1) as f64;
    let (m0, r0, s0) = c.cache_base.unwrap_or((0, 0, 0));
    let (made, reused, hits) = cache.map_or((0, 0, 0), |k| (k.made.saturating_sub(m0), k.reused.saturating_sub(r0), k.shared_hits.saturating_sub(s0)));
    eprintln!(
        "[fx-churn] effects started {} | Fx materials created {} ({:.1} per effect, most in one frame {}), freed {} | cache: made {made} reused {reused} shared {hits} idle {} | meshes created {:.1}/frame, rewritten {:.1}/frame, freed {:.1}/frame | warm-up draws {}",
        c.effects,
        c.fx_added,
        per_fx(c.fx_added),
        c.fx_added_max,
        c.fx_freed,
        cache.map_or(0, |k| k.idle()),
        c.mesh_added as f64 / n,
        c.mesh_modified as f64 / n,
        c.mesh_freed as f64 / n,
        warm.map_or(0, |w| w.draws),
    );
    eprintln!("[churn] per frame: Fx materials modified {:.1}, dissolve (fighter) {:.1}, standard {:.1}, stage {:.1}; live FxPiece entities {:.1}", c.fx as f64 / n, c.dissolve as f64 / n, c.standard as f64 / n, c.stage as f64 / n, c.pieces as f64 / n);
}
