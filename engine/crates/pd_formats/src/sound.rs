//! STH2 sound banks (`Data/Sound/*.bin`): sound effects and battle voices.
//!
//! Layout (docs/FORMATS-AUDIO.md has the evidence for each field):
//! - 0x40-byte `STH2` header; `u32 @0x24` = offset of the `REAL` table,
//!   `u32 @0x28` = offset of the `WAVE` block.
//! - `REAL`: `u32 count @+8`, then `count` × `i32` offsets (relative to REAL,
//!   −1 = no sound). The index in this table is the **sound id** the skill
//!   table and the game use.
//! - Each sound is a `REAT`: `u32 track count @+8`, `u32 volume @+0x10`, then
//!   from +0x30 one `u32` offset per track (relative to the REAT, −1 = empty).
//!   Tracks play together (layers).
//! - Each track is a `TRAT`: `u32 event count @+8`, `u32 volume @+0x10`, then
//!   from +0x30 the events, 0x100 bytes each (`EVNT`).
//! - `WAVE`: `u32 size @+4` (excludes the 8-byte tag + size), `u32 count @+8`,
//!   then `count` × `u32` clip offsets relative to WAVE. Clips are raw
//!   little-endian 16-bit PCM (stereo interleaved).
//!
//! Rendering (`render`) mixes one sound id's layers into a stereo buffer, the
//! way we read the event fields (delay, length, pitch, speaker levels, loop).

use crate::bytes::*;
use anyhow::{bail, Result};

/// One event of a track (an `EVNT` record).
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// `@0x20`: 0 = start a clip. 2/3/5/6 appear on follow-up events with no
    /// clip that sweep pan/pitch (echo tails); not played.
    pub kind: u32,
    /// `@0x18`: start delay after the sound is triggered, ms.
    pub delay_ms: i32,
    /// `@0x1C`: how long it plays, ms (0 = the clip's own length, or forever
    /// for a looped clip). Shorter than the clip = cut off.
    pub duration_ms: i32,
    /// `@0x24`: wave format tag, 1 = PCM.
    pub format: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits: u32,
    /// `@0x34/@0x38`: loop start / end in sample frames (−1 = not looped).
    pub loop_start: i32,
    pub loop_end: i32,
    /// `@0x3C`: clip size in bytes.
    pub size: u32,
    /// `@0x58`: pan (−100..100), unused here (speaker levels carry it).
    pub pan: i32,
    /// `@0x5C`: pitch shift in semitones (unconfirmed).
    pub pitch: i32,
    /// Speaker levels 0–127 from the (key, value) pairs at 0x68..0xA8:
    /// keys 0–5 = front left, front right, centre, LFE, rear left, rear right
    /// (unconfirmed reading; centre and LFE are always 0).
    pub levels: [u8; 6],
    /// `@0xF0`: clip index in WAVE, None for −1.
    pub clip: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// TRAT `@0x10`, 0–127 (reading).
    pub volume: u32,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// REAT `@0x10`, 0–127 (reading).
    pub volume: u32,
    pub tracks: Vec<Option<Track>>,
}

#[derive(Clone, Debug)]
pub struct Bank {
    /// Byte range of each clip in the file.
    pub clips: Vec<(usize, usize)>,
    /// Indexed by sound id.
    pub sounds: Vec<Option<Sound>>,
}

const EVENT_SIZE: usize = 0x100;

pub fn parse(d: &[u8]) -> Result<Bank> {
    if slice(d, 0, 4)? != b"STH2" {
        bail!("not an STH2 sound bank");
    }
    let real = u32_at(d, 0x24)? as usize;
    let wave = u32_at(d, 0x28)? as usize;
    if slice(d, real, 4)? != b"REAL" || slice(d, wave, 4)? != b"WAVE" {
        bail!("STH2: REAL/WAVE tags not where the header says");
    }
    // Clips.
    let wave_end = (wave + 8 + u32_at(d, wave + 4)? as usize).min(d.len());
    let n_clips = u32_at(d, wave + 8)? as usize;
    let mut starts = Vec::with_capacity(n_clips);
    for i in 0..n_clips {
        starts.push(wave + u32_at(d, wave + 16 + 4 * i)? as usize);
    }
    let clips = (0..n_clips).map(|i| (starts[i], starts.get(i + 1).copied().unwrap_or(wave_end).max(starts[i]))).collect();
    // Sounds.
    let n = u32_at(d, real + 8)? as usize;
    let mut sounds = Vec::with_capacity(n);
    for id in 0..n {
        let off = i32_at(d, real + 16 + 4 * id)?;
        if off < 0 {
            sounds.push(None);
            continue;
        }
        let r = real + off as usize;
        if slice(d, r, 4)? != b"REAT" {
            bail!("sound {id}: no REAT at 0x{r:X}");
        }
        let mut tracks = Vec::new();
        for k in 0..u32_at(d, r + 8)? as usize {
            let to = u32_at(d, r + 0x30 + 4 * k)?;
            if to == u32::MAX {
                tracks.push(None);
                continue;
            }
            let t = r + to as usize;
            if slice(d, t, 4)? != b"TRAT" {
                bail!("sound {id} track {k}: no TRAT at 0x{t:X}");
            }
            let mut events = Vec::new();
            for e in 0..u32_at(d, t + 8)? as usize {
                let ev = t + 0x30 + e * EVENT_SIZE;
                if slice(d, ev, 4)? != b"EVNT" {
                    bail!("sound {id} track {k} event {e}: no EVNT at 0x{ev:X}");
                }
                events.push(read_event(d, ev)?);
            }
            tracks.push(Some(Track { volume: u32_at(d, t + 0x10)?, events }));
        }
        sounds.push(Some(Sound { volume: u32_at(d, r + 0x10)?, tracks }));
    }
    Ok(Bank { clips, sounds })
}

fn read_event(d: &[u8], ev: usize) -> Result<Event> {
    let mut levels = [0u8; 6];
    for j in 0..8 {
        let key = i32_at(d, ev + 0x68 + 8 * j)?;
        let val = i32_at(d, ev + 0x6C + 8 * j)?;
        if (0..6).contains(&key) {
            levels[key as usize] = val.clamp(0, 127) as u8;
        }
    }
    let clip = u32_at(d, ev + 0xF0)?;
    Ok(Event {
        kind: u32_at(d, ev + 0x20)?,
        delay_ms: i32_at(d, ev + 0x18)?,
        duration_ms: i32_at(d, ev + 0x1C)?,
        format: u32_at(d, ev + 0x24)?,
        sample_rate: u32_at(d, ev + 0x28)?,
        channels: u32_at(d, ev + 0x2C)?,
        bits: u32_at(d, ev + 0x30)?,
        loop_start: i32_at(d, ev + 0x34)?,
        loop_end: i32_at(d, ev + 0x38)?,
        size: u32_at(d, ev + 0x3C)?,
        pan: i32_at(d, ev + 0x58)?,
        pitch: i32_at(d, ev + 0x5C)?,
        levels,
        clip: (clip != u32::MAX).then_some(clip),
    })
}

impl Event {
    /// Plays a clip we can decode (16-bit PCM, 1–2 channels).
    pub fn playable(&self) -> bool {
        self.kind == 0 && self.clip.is_some() && self.format == 1 && self.bits == 16 && (1..=2).contains(&self.channels) && self.sample_rate > 0
    }

    pub fn looped(&self) -> bool {
        self.loop_start >= 0 && self.loop_end > self.loop_start
    }

    /// Left/right gain 0..1 from the speaker levels (front + rear, centre in both).
    pub fn gain(&self) -> [f32; 2] {
        let l = &self.levels;
        let c = l[2] as f32 * 0.7;
        [((l[0].max(l[4]) as f32) + c).min(127.0) / 127.0, ((l[1].max(l[5]) as f32) + c).min(127.0) / 127.0]
    }
}

/// The clip's sample frames as stereo f32 (−1..1); mono is copied to both sides.
pub fn clip_frames(d: &[u8], bank: &Bank, ev: &Event) -> Option<Vec<[f32; 2]>> {
    if !ev.playable() {
        return None;
    }
    let &(start, end) = bank.clips.get(ev.clip? as usize)?;
    let len = (end - start).min(ev.size as usize);
    let bytes = d.get(start..start + len)?;
    let s: Vec<f32> = bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
    Some(if ev.channels == 2 { s.chunks_exact(2).map(|c| [c[0], c[1]]).collect() } else { s.iter().map(|&v| [v, v]).collect() })
}

/// The raw clip as interleaved i16 plus its rate and channel count (for
/// writing the original clip unchanged).
pub fn clip_pcm(d: &[u8], bank: &Bank, clip: usize, bytes: u32) -> Option<Vec<i16>> {
    let &(start, end) = bank.clips.get(clip)?;
    let len = (end - start).min(bytes as usize);
    Some(d.get(start..start + len)?.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect())
}

/// One sound id mixed to stereo at `rate`.
#[derive(Clone, Debug, Default)]
pub struct Rendered {
    pub rate: u32,
    pub frames: Vec<[f32; 2]>,
    /// Layers that loop forever (duration 0 + loop points), each as its loop
    /// body resampled to `rate` with gain applied: play them on repeat.
    pub loops: Vec<Vec<[f32; 2]>>,
}

/// Fade applied where an event is cut short (avoids clicks), seconds.
const CUT_FADE: f32 = 0.005;

/// Mix every playable event of sound `id`. None if the id is empty or has
/// nothing we can play.
pub fn render(d: &[u8], bank: &Bank, id: usize, rate: u32) -> Option<Rendered> {
    let sound = bank.sounds.get(id)?.as_ref()?;
    let mut out = Rendered { rate, ..Default::default() };
    let mut any = false;
    for track in sound.tracks.iter().flatten() {
        let vol = (sound.volume.min(127) as f32 / 127.0) * (track.volume.min(127) as f32 / 127.0);
        for ev in &track.events {
            let Some(src) = clip_frames(d, bank, ev) else { continue };
            if src.is_empty() {
                continue;
            }
            any = true;
            let g = ev.gain();
            let gain = [g[0] * vol, g[1] * vol];
            // Source frames per output frame (pitch in semitones).
            let step = ev.sample_rate as f64 * 2f64.powf(ev.pitch as f64 / 12.0) / rate as f64;
            let lp = ev.looped().then(|| (ev.loop_start as usize, (ev.loop_end as usize).min(src.len())));
            let lp = lp.filter(|(a, b)| b > a);
            let natural = (src.len() as f64 / step).floor() as usize;
            let (len, cut) = match (ev.duration_ms, lp) {
                (ms, _) if ms <= 0 => (natural, false),
                (ms, Some(_)) => (ms_frames(ms, rate), true),
                (ms, None) => {
                    let want = ms_frames(ms, rate);
                    (want.min(natural), want < natural)
                }
            };
            if ev.duration_ms <= 0 {
                // The one-shot mix below still gets a single pass of the clip.
                if let Some((a, b)) = lp {
                    out.loops.push(loop_body(&src, a, b, step, gain));
                }
            }
            let at = ms_frames(ev.delay_ms.max(0), rate);
            if out.frames.len() < at + len {
                out.frames.resize(at + len, [0.0; 2]);
            }
            let fade = if cut { ((CUT_FADE * rate as f32) as usize).max(1) } else { 0 };
            let mut pos = 0f64;
            for n in 0..len {
                let s = sample_at(&src, pos, if cut { lp } else { None });
                let Some(s) = s else { break };
                let env = if fade > 0 && n + fade > len { (len - n) as f32 / fade as f32 } else { 1.0 };
                let o = &mut out.frames[at + n];
                o[0] += s[0] * gain[0] * env;
                o[1] += s[1] * gain[1] * env;
                pos += step;
            }
        }
    }
    any.then_some(out)
}

fn ms_frames(ms: i32, rate: u32) -> usize {
    (ms.max(0) as u64 * rate as u64 / 1000) as usize
}

/// Linear interpolation at fractional frame `pos`; wraps inside `lp` if given.
fn sample_at(src: &[[f32; 2]], mut pos: f64, lp: Option<(usize, usize)>) -> Option<[f32; 2]> {
    if let Some((a, b)) = lp {
        if pos >= b as f64 {
            pos = a as f64 + (pos - a as f64) % (b - a) as f64;
        }
        let i = pos as usize;
        let f = (pos - i as f64) as f32;
        let j = if i + 1 >= b { a } else { i + 1 };
        let (x, y) = (src[i.min(src.len() - 1)], src[j.min(src.len() - 1)]);
        return Some([x[0] + (y[0] - x[0]) * f, x[1] + (y[1] - x[1]) * f]);
    }
    let i = pos as usize;
    if i >= src.len() {
        return None;
    }
    let f = (pos - i as f64) as f32;
    let (x, y) = (src[i], src[(i + 1).min(src.len() - 1)]);
    Some([x[0] + (y[0] - x[0]) * f, x[1] + (y[1] - x[1]) * f])
}

/// The loop body [a, b) resampled by `step`, wrapping so it repeats seamlessly.
fn loop_body(src: &[[f32; 2]], a: usize, b: usize, step: f64, gain: [f32; 2]) -> Vec<[f32; 2]> {
    let len = (((b - a) as f64 / step).round() as usize).max(1);
    // Stretch the step slightly so exactly `len` output frames cover the body.
    let step = (b - a) as f64 / len as f64;
    (0..len)
        .map(|n| {
            let s = sample_at(src, a as f64 + n as f64 * step, Some((a, b))).unwrap_or([0.0; 2]);
            [s[0] * gain[0], s[1] * gain[1]]
        })
        .collect()
}

/// f32 stereo frames → interleaved i16 (clamped).
pub fn to_i16(frames: &[[f32; 2]]) -> Vec<i16> {
    frames.iter().flat_map(|f| f.map(|v| (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16)).collect()
}

/// A PCM WAV file (16-bit, interleaved).
pub fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let block = channels as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * block).to_le_bytes());
    out.extend_from_slice(&(block as u16).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Signal checks that tell real audio from noise or garbage without ears.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub rms: f32,
    pub peak: f32,
    /// Sign changes per second (left channel).
    pub zero_cross_per_s: f32,
    /// Lag-1 autocorrelation (left): ~1 for smooth audio, ~0 for white noise.
    pub lag1: f32,
    /// Frames at or beyond full scale.
    pub clipped: usize,
}

pub fn stats(frames: &[[f32; 2]], rate: u32) -> Stats {
    if frames.is_empty() {
        return Stats::default();
    }
    let n = frames.len() as f64;
    let mut sq = 0f64;
    let mut peak = 0f32;
    let mut clipped = 0;
    let mut zc = 0usize;
    for (i, f) in frames.iter().enumerate() {
        for v in f {
            sq += (*v as f64) * (*v as f64);
            peak = peak.max(v.abs());
        }
        if f[0].abs() >= 1.0 || f[1].abs() >= 1.0 {
            clipped += 1;
        }
        if i > 0 && (frames[i - 1][0] < 0.0) != (f[0] < 0.0) {
            zc += 1;
        }
    }
    let mean = frames.iter().map(|f| f[0] as f64).sum::<f64>() / n;
    let (mut num, mut den) = (0f64, 0f64);
    for i in 0..frames.len() {
        let x = frames[i][0] as f64 - mean;
        den += x * x;
        if i + 1 < frames.len() {
            num += x * (frames[i + 1][0] as f64 - mean);
        }
    }
    Stats {
        rms: (sq / (2.0 * n)).sqrt() as f32,
        peak,
        zero_cross_per_s: (zc as f64 / (n / rate as f64)) as f32,
        lag1: if den > 0.0 { (num / den) as f32 } else { 0.0 },
        clipped,
    }
}

/// Min/max waveform image (RGB PNG), left channel on top, right below.
pub fn waveform_png(frames: &[[f32; 2]], width: u32, height: u32) -> Result<Vec<u8>> {
    let (w, h) = (width.max(1) as usize, height.max(2) as usize);
    let mut px = vec![24u8; w * h * 3];
    let half = h / 2;
    for ch in 0..2 {
        let (top, mid) = (ch * half, ch * half + half / 2);
        for x in 0..w {
            px[((mid) * w + x) * 3..][..3].copy_from_slice(&[70, 70, 70]);
            let a = x * frames.len() / w;
            let b = ((x + 1) * frames.len() / w).max(a + 1).min(frames.len());
            if a >= frames.len() {
                continue;
            }
            let (mut lo, mut hi) = (1f32, -1f32);
            for f in &frames[a..b] {
                lo = lo.min(f[ch]);
                hi = hi.max(f[ch]);
            }
            let y = |v: f32| (top + ((1.0 - v.clamp(-1.0, 1.0)) * 0.5 * (half - 1) as f32) as usize).min(top + half - 1);
            for yy in y(hi)..=y(lo) {
                px[(yy * w + x) * 3..][..3].copy_from_slice(if ch == 0 { &[90, 200, 255] } else { &[255, 170, 80] });
            }
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&px)?;
    }
    Ok(out)
}

/// Character model → battle-voice bank, from `setup/charaset/charaset_<lang>.dat`
/// (0x10 header: size, entry count, string-pool offset; 0x4C-byte entries
/// with the model file name at +0 and the voice bank name at +0x3C, both
/// offsets into the pool). Bank names come without the `_en`/`_ja` suffix,
/// e.g. ("pc00a", "BattleVo_Hero.bin"). Entries without a bank are skipped.
pub fn voice_banks(charaset: &[u8]) -> Result<Vec<(String, String)>> {
    let count = u32_at(charaset, 4)? as usize;
    let pool = u32_at(charaset, 8)? as usize;
    let text = |off: u32| -> Option<String> {
        let p = pool.checked_add(off as usize)?;
        let rest = charaset.get(p..)?;
        let end = rest.iter().position(|&c| c == 0)?;
        let s = std::str::from_utf8(&rest[..end]).ok()?;
        (!s.is_empty()).then(|| s.to_string())
    };
    let mut out = Vec::new();
    for i in 0..count {
        let e = 0x10 + i * 0x4C;
        let (Some(model), Some(bank)) = (text(u32_at(charaset, e)?), text(u32_at(charaset, e + 0x3C)?)) else { continue };
        if bank.ends_with(".bin") && !out.iter().any(|(m, _)| *m == model) {
            out.push((model, bank));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic bank: clip 0 = ramp mono 1000 Hz rate, clip 1 = stereo.
    struct Ev {
        clip: i32,
        delay: i32,
        dur: i32,
        pitch: i32,
        channels: u32,
        rate: u32,
        size: u32,
        lp: (i32, i32),
        kind: u32,
    }

    fn ev(clip: i32, size: u32) -> Ev {
        Ev { clip, delay: 0, dur: 0, pitch: 0, channels: 1, rate: 1000, size, lp: (-1, -1), kind: 0 }
    }

    fn put(b: &mut Vec<u8>, at: usize, v: u32) {
        if b.len() < at + 4 {
            b.resize(at + 4, 0);
        }
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn build(sounds: &[Option<Vec<Option<Vec<Ev>>>>], clips: &[Vec<i16>]) -> Vec<u8> {
        let mut b = vec![0u8; 0x40];
        b[..4].copy_from_slice(b"STH2");
        let real = 0x100;
        b.resize(real, 0);
        b.extend_from_slice(b"REAL");
        put(&mut b, real + 8, sounds.len() as u32);
        let mut at = real + 16 + 4 * sounds.len();
        for (id, s) in sounds.iter().enumerate() {
            let Some(tracks) = s else {
                put(&mut b, real + 16 + 4 * id, u32::MAX);
                continue;
            };
            let r = at;
            put(&mut b, real + 16 + 4 * id, (r - real) as u32);
            b.resize(r + 0x30 + 4 * tracks.len(), 0);
            b[r..r + 4].copy_from_slice(b"REAT");
            put(&mut b, r + 8, tracks.len() as u32);
            put(&mut b, r + 0x10, 127);
            at = b.len();
            for (k, t) in tracks.iter().enumerate() {
                let Some(evs) = t else {
                    put(&mut b, r + 0x30 + 4 * k, u32::MAX);
                    continue;
                };
                let t = at;
                put(&mut b, r + 0x30 + 4 * k, (t - r) as u32);
                b.resize(t + 0x30 + 0x100 * evs.len(), 0);
                b[t..t + 4].copy_from_slice(b"TRAT");
                put(&mut b, t + 8, evs.len() as u32);
                put(&mut b, t + 0x10, 127);
                for (e, v) in evs.iter().enumerate() {
                    let o = t + 0x30 + 0x100 * e;
                    b[o..o + 4].copy_from_slice(b"EVNT");
                    put(&mut b, o + 0x18, v.delay as u32);
                    put(&mut b, o + 0x1C, v.dur as u32);
                    put(&mut b, o + 0x20, v.kind);
                    put(&mut b, o + 0x24, 1);
                    put(&mut b, o + 0x28, v.rate);
                    put(&mut b, o + 0x2C, v.channels);
                    put(&mut b, o + 0x30, 16);
                    put(&mut b, o + 0x34, v.lp.0 as u32);
                    put(&mut b, o + 0x38, v.lp.1 as u32);
                    put(&mut b, o + 0x3C, v.size);
                    put(&mut b, o + 0x5C, v.pitch as u32);
                    // Speaker levels: FL 127, FR 127, the rest keyed but 0.
                    for (j, key) in [0u32, 1, 2, 3, 4, 5, 11, 12].iter().enumerate() {
                        put(&mut b, o + 0x68 + 8 * j, *key);
                        put(&mut b, o + 0x6C + 8 * j, if *key < 2 { 127 } else { 0 });
                    }
                    put(&mut b, o + 0xF0, v.clip as u32);
                }
                at = b.len();
            }
        }
        let wave = b.len();
        put(&mut b, 0x24, real as u32);
        put(&mut b, 0x28, wave as u32);
        b.extend_from_slice(b"WAVE");
        let head = 16 + 4 * clips.len();
        b.resize(wave + head, 0);
        put(&mut b, wave + 8, clips.len() as u32);
        for (i, c) in clips.iter().enumerate() {
            let off = (b.len() - wave) as u32;
            put(&mut b, wave + 16 + 4 * i, off);
            for s in c {
                b.extend_from_slice(&s.to_le_bytes());
            }
        }
        let size = (b.len() - wave - 8) as u32;
        put(&mut b, wave + 4, size);
        b
    }

    fn ramp(n: usize) -> Vec<i16> {
        (0..n).map(|i| (i as i16) * 100).collect()
    }

    #[test]
    fn parses_ids_tracks_and_clips() {
        let d = build(&[None, Some(vec![Some(vec![ev(0, 200)]), None, Some(vec![ev(1, 40)])])], &[ramp(100), vec![1000, -1000, 2000, -2000]]);
        let bank = parse(&d).unwrap();
        assert_eq!(bank.sounds.len(), 2);
        assert!(bank.sounds[0].is_none());
        let s = bank.sounds[1].as_ref().unwrap();
        assert_eq!(s.tracks.len(), 3);
        assert!(s.tracks[1].is_none());
        let e = &s.tracks[0].as_ref().unwrap().events[0];
        assert_eq!((e.clip, e.sample_rate, e.channels, e.bits, e.levels[0], e.levels[2]), (Some(0), 1000, 1, 16, 127, 0));
        assert_eq!(bank.clips.len(), 2);
        assert_eq!(bank.clips[0].1 - bank.clips[0].0, 200);
        // The clip decodes as the ramp we wrote.
        let f = clip_frames(&d, &bank, e).unwrap();
        assert_eq!(f.len(), 100);
        assert!((f[10][0] - 1000.0 / 32768.0).abs() < 1e-6 && f[10][0] == f[10][1]);
    }

    #[test]
    fn stereo_clips_are_interleaved() {
        let mut e = ev(0, 8);
        e.channels = 2;
        let d = build(&[Some(vec![Some(vec![e])])], &[vec![1000, -1000, 2000, -2000]]);
        let bank = parse(&d).unwrap();
        let f = clip_frames(&d, &bank, &bank.sounds[0].as_ref().unwrap().tracks[0].as_ref().unwrap().events[0]).unwrap();
        assert_eq!(f.len(), 2);
        assert!(f[0][0] > 0.0 && f[0][1] < 0.0 && f[1][0] > f[0][0]);
    }

    #[test]
    fn render_places_delay_pitch_and_cut() {
        // Layer A: ramp at 1000 Hz rendered at 1000 Hz. Layer B: same clip,
        // 50 ms later, one octave up (half as long). Layer C: cut to 30 ms.
        let a = ev(0, 200);
        let mut b = ev(0, 200);
        b.delay = 50;
        b.pitch = 12;
        let mut c = ev(0, 200);
        c.dur = 30;
        let d = build(&[Some(vec![Some(vec![a])]), Some(vec![Some(vec![b])]), Some(vec![Some(vec![c])])], &[ramp(100)]);
        let bank = parse(&d).unwrap();
        let ra = render(&d, &bank, 0, 1000).unwrap();
        assert_eq!(ra.frames.len(), 100);
        assert!((ra.frames[40][0] - 4000.0 / 32768.0).abs() < 1e-4);
        let rb = render(&d, &bank, 1, 1000).unwrap();
        assert_eq!(rb.frames.len(), 50 + 50);
        assert_eq!(rb.frames[49], [0.0, 0.0]);
        // One octave up: output frame 10 after the delay = source frame 20.
        assert!((rb.frames[60][0] - 2000.0 / 32768.0).abs() < 1e-4);
        let rc = render(&d, &bank, 2, 1000).unwrap();
        assert_eq!(rc.frames.len(), 30);
        assert!(rc.loops.is_empty());
    }

    #[test]
    fn looped_events_give_a_loop_body_and_duration_loops() {
        let mut forever = ev(0, 200);
        forever.lp = (20, 60);
        let mut timed = ev(0, 200);
        timed.lp = (20, 60);
        timed.dur = 300;
        let d = build(&[Some(vec![Some(vec![forever])]), Some(vec![Some(vec![timed])])], &[ramp(100)]);
        let bank = parse(&d).unwrap();
        let r = render(&d, &bank, 0, 1000).unwrap();
        assert_eq!(r.loops.len(), 1);
        assert_eq!(r.loops[0].len(), 40);
        assert!((r.loops[0][0][0] - 2000.0 / 32768.0).abs() < 1e-4);
        let t = render(&d, &bank, 1, 1000).unwrap();
        assert!(t.loops.is_empty());
        assert_eq!(t.frames.len(), 300);
        // Wrapped back into the loop: frame 60 = source 20 again.
        assert!((t.frames[60][0] - 2000.0 / 32768.0).abs() < 1e-4);
    }

    #[test]
    fn follow_up_events_without_clips_are_skipped() {
        let mut tail = ev(-1, 0);
        tail.kind = 5;
        tail.delay = 500;
        let d = build(&[Some(vec![Some(vec![ev(0, 200), tail])]), Some(vec![Some(vec![ev(-1, 0)])])], &[ramp(100)]);
        let bank = parse(&d).unwrap();
        assert_eq!(render(&d, &bank, 0, 1000).unwrap().frames.len(), 100);
        assert!(render(&d, &bank, 1, 1000).is_none());
    }

    #[test]
    fn stats_and_wav() {
        let sine: Vec<[f32; 2]> = (0..22050).map(|i| [(i as f32 * 440.0 * std::f32::consts::TAU / 22050.0).sin() * 0.5; 2]).collect();
        let s = stats(&sine, 22050);
        assert!((s.rms - 0.3536).abs() < 0.01, "{s:?}");
        assert!((s.zero_cross_per_s - 880.0).abs() < 5.0, "{s:?}");
        assert!(s.lag1 > 0.9 && s.clipped == 0);
        let w = wav(22050, 2, &to_i16(&sine));
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(w.len(), 44 + 22050 * 4);
        assert!(waveform_png(&sine, 64, 32).unwrap().starts_with(b"\x89PNG"));
    }

    #[test]
    fn voice_banks_from_charaset() {
        let mut b = vec![0u8; 0x10 + 3 * 0x4C];
        let pool = b.len();
        let strings = b"pc00a\0BattleVo_Hero.bin\0npc00a\0";
        b.extend_from_slice(strings);
        let total = b.len() as u32;
        put(&mut b, 0, total);
        put(&mut b, 4, 3);
        put(&mut b, 8, pool as u32);
        put(&mut b, 0x10, 0);
        put(&mut b, 0x10 + 0x3C, 6);
        put(&mut b, 0x10 + 0x4C, 24);
        put(&mut b, 0x10 + 0x4C + 0x3C, 0xFFBB_D6B0); // no bank (as in the game's NPC rows)
        put(&mut b, 0x10 + 2 * 0x4C, 0);
        put(&mut b, 0x10 + 2 * 0x4C + 0x3C, 6); // duplicate model: first wins
        assert_eq!(voice_banks(&b).unwrap(), vec![("pc00a".to_string(), "BattleVo_Hero.bin".to_string())]);
    }
}
