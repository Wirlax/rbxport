//! The golden gate: our analysis against rekordbox's own stamps, on the
//! curated `RBX-BPM-GRID-TEST` playlist. READ-ONLY against the library.
//!
//! Two modes, because a full decode per experiment is what makes tuning slow:
//!
//! `cargo run --release -p rbl-analysis --example golden -- cache [playlist]`
//!   decodes every track in the playlist once and keeps the mono PCM beside
//!   what rekordbox recorded for it (BPM, key, the `PQTZ` grid) under
//!   `target/golden/`.
//!
//! `cargo run --release -p rbl-analysis --example golden -- eval [filter]`
//!   analyses every cached track and scores three things, each pass/fail per
//!   track, and prints every failure:
//!
//!   - **bpm**: within `BPM_TOLERANCE` of rekordbox's, no octave folding.
//!   - **downbeat**: our downbeats fall on rekordbox's, i.e. the offset
//!     between the two grids, taken modulo one bar, is within `MS_TOLERANCE`.
//!   - **grid**: at least `GRID_PASS` of rekordbox's beats have one of ours
//!     within `MS_TOLERANCE` carrying the same beat number. This is the one
//!     that fails when the tempo is right to 0.05 BPM and still drifts a beat
//!     off by the end of a track, and the one a variable-tempo track needs.
//!   - **key**: the same name rekordbox chose.
//!
//! `filter` is a substring of the title, to look at one track.
#![allow(clippy::pedantic, clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rbl_analysis::tempo::Beat;

const MAGIC: &[u8; 4] = b"RBGS";
const VERSION: u32 = 1;
const DEFAULT_PLAYLIST: &str = "RBX-BPM-GRID-TEST";

/// How far from rekordbox's BPM still counts.
const BPM_TOLERANCE: f64 = 0.05;
/// How far a beat may sit from rekordbox's and still be the same beat.
const MS_TOLERANCE: f64 = 25.0;
/// The share of rekordbox's beats that must be matched for the grid to pass.
const GRID_PASS: f64 = 0.98;

/// Under `target/`, not the system temp directory: macOS empties that on its
/// own schedule.
fn cache_dir() -> PathBuf {
    std::env::var("RB_LITE_GOLDEN").map_or_else(
        |_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/golden"),
        PathBuf::from,
    )
}

/// One track, decoded, with what rekordbox said about it.
struct Track {
    id: u64,
    title: String,
    extension: String,
    sample_rate: u32,
    samples: Vec<f32>,
    bpm: f64,
    key: String,
    grid: Vec<Beat>,
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "eval".to_owned());
    match mode.as_str() {
        "cache" => cache(),
        "eval" => eval(),
        "downbeat" => downbeat_experiment(),
        "key" => key_experiment(),
        "bassroot" => bassroot_experiment(),
        other => println!("unknown mode {other:?}; use `cache` or `eval`"),
    }
}

// ---------------------------------------------------------------- caching

fn cache() {
    let name = std::env::args().nth(2).unwrap_or_else(|| DEFAULT_PLAYLIST.to_owned());
    let out = cache_dir();
    std::fs::create_dir_all(&out).expect("cache dir");

    let db = match rbl_db::Library::open_installed_read_only() {
        Ok(db) => db,
        Err(e) => {
            println!("cannot open library: {e}");
            return;
        }
    };
    let share = db.location().share_root.clone();
    let (library, _stats) = rbl_index::load(&db).expect("index");
    let playlists = library.playlists();
    let Some(index) = (0..playlists.len()).find(|&i| playlists.name(i) == name) else {
        println!("no playlist named {name:?}");
        return;
    };
    let members = playlists.members[index].clone();
    println!("playlist {name:?}: {} tracks", members.len());

    let started = Instant::now();
    let mut done = 0usize;
    let mut skipped = 0usize;
    for &row in &members {
        let i = row as usize;
        let id = library.ids[i];
        let target = out.join(format!("{id}.gold"));
        if target.exists() {
            done += 1;
            continue;
        }
        let path = Path::new(library.folder_path.get(i));
        let dat = rbl_anlz::resolve(&share, library.analysis_path.get(i));
        let Some(grid) = rbl_anlz::Anlz::read(&dat).ok().and_then(|d| d.beat_grid()) else {
            println!("  no grid for {}", library.title.get(i));
            skipped += 1;
            continue;
        };
        let Ok(audio) = rbl_audio::decode_mono(path, None) else {
            println!("  cannot decode {}", path.display());
            skipped += 1;
            continue;
        };
        let track = Track {
            id,
            title: library.title.get(i).to_owned(),
            extension: path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase(),
            sample_rate: audio.sample_rate,
            samples: audio.samples,
            bpm: f64::from(library.bpm_x100[i]) / 100.0,
            key: library.key_name(row).to_owned(),
            grid: grid
                .iter()
                .map(|b| Beat { beat_number: b.beat_number, tempo_x100: b.tempo_x100, time_ms: b.time_ms })
                .collect(),
        };
        write_track(&target, &track);
        done += 1;
        if done % 10 == 0 {
            println!("  … {done} cached ({:.0}s)", started.elapsed().as_secs_f64());
            let _ = std::io::stdout().flush();
        }
    }
    println!("cached {done} tracks in {} ({skipped} skipped)", out.display());
}

fn write_track(path: &Path, track: &Track) {
    let mut bytes = Vec::with_capacity(track.samples.len() * 2 + 1024);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&track.id.to_le_bytes());
    bytes.extend_from_slice(&track.sample_rate.to_le_bytes());
    bytes.extend_from_slice(&track.bpm.to_le_bytes());
    write_str(&mut bytes, &track.title);
    write_str(&mut bytes, &track.extension);
    write_str(&mut bytes, &track.key);
    bytes.extend_from_slice(&(track.grid.len() as u32).to_le_bytes());
    for beat in &track.grid {
        bytes.extend_from_slice(&beat.time_ms.to_le_bytes());
        bytes.extend_from_slice(&beat.beat_number.to_le_bytes());
        bytes.extend_from_slice(&beat.tempo_x100.to_le_bytes());
    }
    bytes.extend_from_slice(&(track.samples.len() as u64).to_le_bytes());
    for &s in &track.samples {
        bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write cache");
}

fn write_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn read_track(path: &Path) -> Option<Track> {
    let bytes = std::fs::read(path).ok()?;
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Option<&[u8]> {
        let s = bytes.get(*at..*at + n)?;
        *at += n;
        Some(s)
    };
    if take(&mut at, 4)? != MAGIC {
        return None;
    }
    let u32_at = |at: &mut usize| take(at, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let u16_at = |at: &mut usize| take(at, 2).map(|b| u16::from_le_bytes(b.try_into().unwrap()));
    let u64_at = |at: &mut usize| take(at, 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()));
    let f64_at = |at: &mut usize| take(at, 8).map(|b| f64::from_le_bytes(b.try_into().unwrap()));
    let str_at = |at: &mut usize| -> Option<String> {
        let n = u32_at(at)? as usize;
        Some(String::from_utf8_lossy(take(at, n)?).into_owned())
    };
    if u32_at(&mut at)? != VERSION {
        return None;
    }
    let id = u64_at(&mut at)?;
    let sample_rate = u32_at(&mut at)?;
    let bpm = f64_at(&mut at)?;
    let title = str_at(&mut at)?;
    let extension = str_at(&mut at)?;
    let key = str_at(&mut at)?;
    let beats = u32_at(&mut at)? as usize;
    let mut grid = Vec::with_capacity(beats);
    for _ in 0..beats {
        let time_ms = u32_at(&mut at)?;
        let beat_number = u16_at(&mut at)?;
        let tempo_x100 = u16_at(&mut at)?;
        grid.push(Beat { beat_number, tempo_x100, time_ms });
    }
    let n = u64_at(&mut at)? as usize;
    let raw = take(&mut at, n * 2)?;
    let samples = raw
        .chunks_exact(2)
        .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32767.0)
        .collect();
    Some(Track { id, title, extension, sample_rate, samples, bpm, key, grid })
}

// ---------------------------------------------------------------- scoring

/// How one track did.
struct Score {
    title: String,
    extension: String,
    rb_bpm: f64,
    our_bpm: f64,
    /// Signed offset of our downbeats from rekordbox's, modulo a bar, in ms.
    downbeat_offset_ms: f64,
    /// Signed offset of our beats from rekordbox's, modulo a beat, in ms.
    beat_offset_ms: f64,
    /// Which of rekordbox's beat numbers our downbeat lands on.
    beat_in_bar: u16,
    grid_matched: f64,
    rb_key: String,
    our_key: String,
    elapsed_ms: u128,
}

impl Score {
    fn bpm_ok(&self) -> bool {
        (self.our_bpm - self.rb_bpm).abs() <= BPM_TOLERANCE
    }
    fn downbeat_ok(&self) -> bool {
        self.downbeat_offset_ms.abs() <= MS_TOLERANCE
    }
    fn grid_ok(&self) -> bool {
        self.grid_matched >= GRID_PASS
    }
    fn key_ok(&self) -> bool {
        self.our_key == self.rb_key
    }
}

fn score(track: &Track) -> Score {
    let started = Instant::now();
    let analysis = rbl_analysis::analyse(&track.samples, track.sample_rate);
    if std::env::var("RB_LITE_CANDIDATES").is_ok() {
        let onsets = rbl_analysis::onset::onset_envelope(&track.samples, track.sample_rate);
        let table = rbl_analysis::tempo::tempo_candidates(&onsets, rbl_analysis::tempo::TempoOptions::default());
        println!("candidates for {} (rb {:.2}):", track.title, track.bpm);
        for c in table.iter().take(12) {
            println!("  {:>7.2}  acf {:.3}  fourier {:.3}  prior {:.3}  score {:.4}", c.bpm, c.acf, c.fourier, c.prior, c.score);
        }
        for s in &analysis.tempo.segments {
            println!("  segment {:.3}s..{:.3}s: first beat {:.3}s period {:.5}s ({:.3} BPM) beats {}", s.from_secs, s.to_secs, s.start_secs(), s.period_secs, s.bpm(), s.beats());
        }
        // Where the downbeat stage put the bar, and the first beats side by side.
        let beat_secs: Vec<f64> = analysis.tempo.beats.iter().map(|b| f64::from(b.time_ms) / 1000.0).collect();
        let mut halves = Vec::new();
        for pair in beat_secs.windows(2) { halves.push(pair[0]); halves.push((pair[0] + pair[1]) / 2.0); }
        let profiles = rbl_analysis::downbeat::beat_profiles(&track.samples, track.sample_rate, &halves);
        let scores = rbl_analysis::downbeat::position_scores(&profiles, 8, &[8, 16, 32, 64]);
        println!("  half-beat position scores (from our first beat): {}", scores.iter().map(|v| format!("{v:.3}")).collect::<Vec<_>>().join(" "));
        let ours: Vec<String> = analysis.tempo.beats.iter().take(6).map(|b| format!("{}@{}", b.beat_number, b.time_ms)).collect();
        let theirs: Vec<String> = track.grid.iter().take(6).map(|b| format!("{}@{}", b.beat_number, b.time_ms)).collect();
        println!("  first beats ours {} | rb {}", ours.join(" "), theirs.join(" "));
        // The incoming grid's strength per bar across each tempo change, to
        // see where rekordbox places the switch relative to the drop.
        if analysis.tempo.segments.len() > 1 {
            let onsets = rbl_analysis::onset::onset_envelope(&track.samples, track.sample_rate);
            for pair in analysis.tempo.segments.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let rb_change = track.grid.windows(2).find(|w| w[0].tempo_x100 != w[1].tempo_x100).map_or(0.0, |w| f64::from(w[1].time_ms) / 1000.0);
                println!("  change ours {:.3}s rb {:.3}s; incoming grid ({:.2} BPM) strength per bar, and outgoing ({:.2} BPM):", b.from_secs, rb_change, b.bpm(), a.bpm());
                let strength = |seg: rbl_analysis::tempo::Segment, t: f64| -> f64 {
                    let x = (t - onsets.origin_secs) * onsets.rate;
                    let reach = seg.period_secs * onsets.rate * 0.2;
                    let lo = (x - reach).max(0.0) as usize;
                    let hi = ((x + reach) as usize).min(onsets.len().saturating_sub(1));
                    (lo..=hi).map(|i| onsets.values[i] as f64).fold(0.0, f64::max)
                };
                let from = (b.from_secs - 40.0).max(0.0);
                let to = (b.from_secs + 130.0).min(onsets.time_of(onsets.len() as f64));
                let mut line = String::new();
                let mut t = b.phase_secs + ((from - b.phase_secs) / b.period_secs).ceil() * b.period_secs;
                while t < to {
                    let bar: f64 = (0..4).map(|k| strength(b, t + k as f64 * b.period_secs)).sum::<f64>() / 4.0;
                    let bar_a: f64 = (0..4).map(|k| strength(a, t + k as f64 * a.period_secs)).sum::<f64>() / 4.0;
                    line.push_str(&format!(" {:.1}:{:.2}/{:.2}", t, bar, bar_a));
                    t += 4.0 * b.period_secs;
                }
                println!("   {line}");
            }
        }
        let report = rbl_analysis::tempo::fit_report(&onsets, table.first().map_or(120.0, |c| c.bpm), rbl_analysis::tempo::TempoOptions::default());
        println!("  fit: comb {:.4} then {}", report[0], report[1..].iter().map(|b| format!("{b:.4}")).collect::<Vec<_>>().join(" -> "));
        let windows = rbl_analysis::tempo::local_tempos(&onsets, table.first().map_or(120.0, |c| c.bpm), rbl_analysis::tempo::TempoOptions::default());
        let line: Vec<String> = windows.iter().map(|(t, r)| format!("{:.0}s:{}", t, r.map_or("-".to_owned(), |r| format!("{r:.3}")))).collect();
        println!("  windows: {}", line.join(" "));
    }
    let elapsed_ms = started.elapsed().as_millis();
    let ours = &analysis.tempo.beats;
    let rb = &track.grid;

    let (downbeat_offset_ms, beat_offset_ms, beat_in_bar) = offsets(ours, rb);
    let grid_matched = grid_match(ours, rb);

    Score {
        title: track.title.clone(),
        extension: track.extension.clone(),
        rb_bpm: track.bpm,
        our_bpm: analysis.tempo.bpm,
        downbeat_offset_ms,
        beat_offset_ms,
        beat_in_bar,
        grid_matched,
        rb_key: track.key.clone(),
        our_key: analysis.key.map(|k| k.name).unwrap_or_default(),
        elapsed_ms,
    }
}

/// Signed distance to the nearest multiple of `period`.
fn wrap(offset: f64, period: f64) -> f64 {
    if period <= 0.0 {
        return offset;
    }
    let d = offset.rem_euclid(period);
    if d > period / 2.0 { d - period } else { d }
}

/// The offset of our grid from rekordbox's, at rekordbox's first downbeat.
///
/// Compared where the two grids overlap: rekordbox's first downbeat is the
/// anchor, and the nearest downbeat of ours to it gives the bar offset. Taken
/// modulo a bar of rekordbox's tempo there, so "we chose the wrong beat of
/// the bar" shows up as a multiple of a beat, and "we chose the right beat a
/// little late" as a small number. Also returned: the same thing modulo a
/// beat, and which of rekordbox's beat numbers our downbeat coincides with.
fn offsets(ours: &[Beat], rb: &[Beat]) -> (f64, f64, u16) {
    let Some(anchor) = rb.iter().find(|b| b.beat_number == 1) else {
        return (f64::INFINITY, f64::INFINITY, 0);
    };
    let tempo = f64::from(anchor.tempo_x100) / 100.0;
    if tempo <= 0.0 {
        return (f64::INFINITY, f64::INFINITY, 0);
    }
    let beat_ms = 60_000.0 / tempo;
    let bar_ms = beat_ms * 4.0;
    let anchor_ms = f64::from(anchor.time_ms);
    // Our downbeat nearest rekordbox's anchor, by bar-wrapped distance.
    let mut best: Option<(f64, f64)> = None;
    for beat in ours.iter().filter(|b| b.beat_number == 1) {
        let raw = f64::from(beat.time_ms) - anchor_ms;
        // Only downbeats within a few bars of the anchor: further away, a
        // tempo difference would leak into the phase.
        if raw.abs() > bar_ms * 2.5 {
            continue;
        }
        let wrapped = wrap(raw, bar_ms);
        if best.is_none_or(|(b, _)| wrapped.abs() < b.abs()) {
            best = Some((wrapped, raw));
        }
    }
    let Some((down, _)) = best else {
        return (f64::INFINITY, f64::INFINITY, 0);
    };
    let beat = wrap(down, beat_ms);
    // Which rekordbox beat number our downbeat sits on: 1 when the downbeats
    // agree, else 2..=4.
    let steps = ((down - beat) / beat_ms).round() as i64;
    let beat_in_bar = (steps.rem_euclid(4) + 1) as u16;
    (down, beat, beat_in_bar)
}

/// The share of rekordbox's beats that one of ours matches, in time and number.
fn grid_match(ours: &[Beat], rb: &[Beat]) -> f64 {
    if rb.is_empty() || ours.is_empty() {
        return 0.0;
    }
    let mut j = 0usize;
    let mut matched = 0usize;
    for beat in rb {
        let t = f64::from(beat.time_ms);
        // Advance to the first of ours not before this beat's window.
        while j + 1 < ours.len() && f64::from(ours[j].time_ms) < t - MS_TOLERANCE {
            j += 1;
        }
        let hit = ours[j.saturating_sub(1)..(j + 2).min(ours.len())]
            .iter()
            .any(|o| (f64::from(o.time_ms) - t).abs() <= MS_TOLERANCE && o.beat_number == beat.beat_number);
        if hit {
            matched += 1;
        }
    }
    matched as f64 / rb.len() as f64
}

// ---------------------------------------------------------------- eval

fn eval() {
    let filter = std::env::args().nth(2).map(|s| s.to_lowercase());
    let dir = cache_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "gold")).collect())
        .unwrap_or_default();
    paths.sort();
    if paths.is_empty() {
        println!("nothing cached in {}; run `cache` first", dir.display());
        return;
    }

    let started = Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let scores = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(path) = paths.get(i) else { break };
                let Some(track) = read_track(path) else { continue };
                if let Some(f) = &filter {
                    if !track.title.to_lowercase().contains(f.as_str()) {
                        continue;
                    }
                }
                let s = score(&track);
                scores.lock().unwrap().push(s);
            });
        }
    });
    let mut scores = scores.into_inner().unwrap();
    scores.sort_by(|a, b| a.title.cmp(&b.title));
    let n = scores.len();
    if n == 0 {
        println!("no track matched");
        return;
    }

    let verbose = filter.is_some() || std::env::var("RB_LITE_VERBOSE").is_ok();
    println!("{:<52} {:>7} {:>7} | {:>7} {:>6} b | {:>5} | {:<4} {:<4} | ms", "title", "rb", "ours", "down", "beat", "grid", "rb", "ours");
    for s in &scores {
        let failed = !s.bpm_ok() || !s.downbeat_ok() || !s.grid_ok() || !s.key_ok();
        if !failed && !verbose {
            continue;
        }
        println!(
            "{:<52} {:>7.2} {:>7.2}{} | {:>+7.1}{} {:>+6.1} {} | {:>4.0}%{} | {:<4} {:<4}{} | {} {}",
            truncate(&s.title, 52),
            s.rb_bpm,
            s.our_bpm,
            if s.bpm_ok() { " " } else { "!" },
            s.downbeat_offset_ms,
            if s.downbeat_ok() { " " } else { "!" },
            s.beat_offset_ms,
            s.beat_in_bar,
            s.grid_matched * 100.0,
            if s.grid_ok() { " " } else { "!" },
            s.rb_key,
            s.our_key,
            if s.key_ok() { " " } else { "!" },
            s.elapsed_ms,
            s.extension,
        );
    }

    // Every failure by metric, so a run reads as a to-do list.
    let failures = |name: &str, f: fn(&Score) -> bool| {
        let failed: Vec<&str> = scores.iter().filter(|s| !f(s)).map(|s| s.title.as_str()).collect();
        if !failed.is_empty() {
            println!("\n{name} failures ({}):", failed.len());
            for title in failed {
                println!("  {}", truncate(title, 70));
            }
        }
    };
    failures("bpm", Score::bpm_ok);
    failures("downbeat", Score::downbeat_ok);
    failures("grid", Score::grid_ok);
    let count = |f: fn(&Score) -> bool| scores.iter().filter(|s| f(s)).count();
    let line = |name: &str, ok: usize| {
        println!("  {name:<9} {ok:>3} / {n}  ({:.1}%){}", ok as f64 / n as f64 * 100.0, if ok as f64 / n as f64 >= 0.99 { "" } else { "  <-- below 99%" });
    };
    println!("\n== {n} tracks, {:.1}s ==", started.elapsed().as_secs_f64());
    line("bpm", count(Score::bpm_ok));
    line("downbeat", count(Score::downbeat_ok));
    line("grid", count(Score::grid_ok));
    line("key", count(Score::key_ok));
    let mut offsets: Vec<f64> = scores.iter().filter(|s| s.downbeat_ok()).map(|s| s.downbeat_offset_ms).collect();
    offsets.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if !offsets.is_empty() {
        let pct = |p: f64| offsets[((offsets.len() as f64 * p) as usize).min(offsets.len() - 1)];
        println!("  downbeat offset among passes: p10 {:+.1}  p50 {:+.1}  p90 {:+.1} ms", pct(0.1), pct(0.5), pct(0.9));
    }
    let total_ms: u128 = scores.iter().map(|s| s.elapsed_ms).sum();
    println!("  analysis time: mean {} ms per track", total_ms / n as u128);
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}

// ---------------------------------------------------------------- experiments

/// On rekordbox's own grid, does the novelty pick rekordbox's downbeat out
/// of the eight half-beat positions in the bar? This separates the downbeat
/// stage from the grid it is normally given.
fn downbeat_experiment() {
    use rbl_analysis::downbeat::{beat_profiles, position_scores};
    let dir = cache_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "gold")).collect())
        .unwrap_or_default();
    paths.sort();
    let (mut n, mut ok) = (0usize, 0usize);
    for path in &paths {
        let Some(track) = read_track(path) else { continue };
        // Rekordbox's grid from its first downbeat, so position 0 is the
        // truth, on half beats.
        let Some(first_down) = track.grid.iter().position(|b| b.beat_number == 1) else { continue };
        let beats: Vec<f64> = track.grid[first_down..].iter().map(|b| f64::from(b.time_ms) / 1000.0).collect();
        let mut halves: Vec<f64> = Vec::with_capacity(beats.len() * 2);
        for pair in beats.windows(2) {
            halves.push(pair[0]);
            halves.push((pair[0] + pair[1]) / 2.0);
        }
        let profiles = beat_profiles(&track.samples, track.sample_rate, &halves);
        let scores = position_scores(&profiles, 8, &[8, 16, 32, 64]);
        let best = scores.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map_or(0, |(i, _)| i);
        n += 1;
        if best == 0 {
            ok += 1;
        } else {
            let fmt = scores.iter().map(|v| format!("{v:.3}")).collect::<Vec<_>>().join(" ");
            println!("{:<50} picked {best}: {fmt}", truncate(&track.title, 50));
        }
    }
    println!("downbeat right on rekordbox's grid: {ok} / {n}");
}

/// Measures the key rules against rekordbox's key names: the profile match
/// alone, then each rule set, with what every rule fired on, fixed and
/// broke; and a search over the `BassRoot` knobs.
fn key_experiment() {
    use rbl_analysis::key::{gather_evidence, judge, BassSource, KeyEvidence, KeyOptions, Rule, DEFAULT_RULES};
    let dir = cache_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "gold")).collect())
        .unwrap_or_default();
    paths.sort();
    let options = KeyOptions::default();
    let edges = [30.0, 45.0, 90.0];

    // Per track: rekordbox's key and the evidence for each edge length,
    // with the bass read against the grid our own analysis finds.
    struct Row { rb: String, title: String, evidence: Vec<KeyEvidence> }
    let started = Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let rows = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(path) = paths.get(i) else { break };
                let Some(track) = read_track(path) else { continue };
                let analysis = rbl_analysis::analyse(&track.samples, track.sample_rate);
                let grid = key_grid_of(&track, &analysis);
                let evidence: Vec<KeyEvidence> = edges.iter().filter_map(|&secs| {
                    let rules = [Rule::BassRoot { margin: 1.0, source: BassSource::Edges { secs } }];
                    gather_evidence(&track.samples, track.sample_rate, options, &rules, &grid)
                }).collect();
                if evidence.len() == edges.len() {
                    rows.lock().unwrap().push(Row { rb: track.key.clone(), title: track.title.clone(), evidence });
                }
            });
        }
    });
    let rows = rows.into_inner().unwrap();
    let n = rows.len();
    println!("evidence for {n} tracks in {:.1}s", started.elapsed().as_secs_f64());

    // A rule set's score, and what each rule did.
    let score = |rules: &[Rule], edge_index: usize, verbose: bool| -> usize {
        let mut exact = 0usize;
        let mut per_rule: Vec<(usize, usize, usize)> = vec![(0, 0, 0); rules.len()]; // fired, fixed, broke
        for row in &rows {
            let Some(report) = judge(&row.evidence[edge_index], options, rules) else { continue };
            if report.key.name == row.rb { exact += 1; }
            let name_of = |v: rbl_analysis::key::Verdict| if v.minor { MINORS[v.tonic] } else { MAJORS[v.tonic] };
            for applied in &report.applied {
                let index = rules.iter().position(|r| *r == applied.rule).unwrap_or(0);
                per_rule[index].0 += 1;
                let was_right = name_of(applied.before) == row.rb;
                let is_right = name_of(applied.after) == row.rb;
                if !was_right && is_right { per_rule[index].1 += 1; }
                if was_right && !is_right { per_rule[index].2 += 1; }
                if verbose && was_right != is_right {
                    println!("    {:<44} rb {:<4} {:?}: {} -> {}  {}", truncate(&row.title, 44), row.rb, applied.rule, name_of(applied.before), name_of(applied.after), if is_right { "fixed" } else { "BROKE" });
                }
            }
        }
        if verbose {
            for (rule, (fired, fixed, broke)) in rules.iter().zip(&per_rule) {
                println!("  {rule:?}: fired {fired}, fixed {fixed}, broke {broke}");
            }
        }
        exact
    };

    println!("profile match alone: {} / {n}", score(&[], 1, false));
    println!("PreferMinor 0.3 alone: {} / {n}", score(&[Rule::PreferMinor { bias: 0.3 }], 1, false));
    println!("shipped rules ({DEFAULT_RULES:?}): {} / {n}", score(DEFAULT_RULES, 1, true));

    println!("BassRoot search (after PreferMinor 0.3):");
    let mut results: Vec<(usize, String)> = Vec::new();
    for (ei, secs) in edges.iter().enumerate() {
        let sources = [
            BassSource::Edges { secs: *secs }, BassSource::Downbeats, BassSource::EdgesAndDownbeats { secs: *secs },
            BassSource::OffBeats, BassSource::SecondEighth, BassSource::PhraseStart { bars: 4 }, BassSource::Whole,
        ];
        for source in sources {
            if ei > 0 && !matches!(source, BassSource::Edges { .. } | BassSource::EdgesAndDownbeats { .. }) {
                continue;
            }
            for margin in [0.01, 0.02, 0.05, 0.1, 0.15, 0.2, 0.3, 1.0] {
                let rules = [Rule::PreferMinor { bias: 0.3 }, Rule::BassRoot { margin, source }];
                results.push((score(&rules, ei, false), format!("margin {margin:.2} {source:?}")));
            }
        }
    }
    for source in [BassSource::SecondEighth, BassSource::PhraseStart { bars: 4 }, BassSource::OffBeats, BassSource::Whole, BassSource::Edges { secs: 45.0 }] {
        for weight in [0.05, 0.1, 0.15, 0.2, 0.3, 0.5] {
            let rules = [Rule::PreferMinor { bias: 0.3 }, Rule::BassVote { weight, source }];
            results.push((score(&rules, 1, false), format!("vote {weight:.2} {source:?}")));
            let rules = [Rule::BassVote { weight, source }, Rule::PreferMinor { bias: 0.3 }];
            results.push((score(&rules, 1, false), format!("vote {weight:.2} {source:?} before PreferMinor")));
        }
    }
    results.sort_by_key(|r| std::cmp::Reverse(r.0));
    for (exact, name) in results.iter().take(16) {
        println!("  {exact:>3} / {n}  {name}");
    }
    let second: Vec<&(usize, String)> = results.iter().filter(|(_, n)| n.contains("SecondEighth") || n.contains("PhraseStart")).take(8).collect();
    for (exact, name) in second { println!("  {exact:>3} / {n}  {name}"); }
    println!("failures of the shipped rules:");
    let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for row in &rows {
        let Some(report) = judge(&row.evidence[1], options, DEFAULT_RULES) else { continue };
        let ours = report.key.name;
        if ours == row.rb { continue; }
        let kind = if relative(&ours) == row.rb { "relative" } else if a_fifth_away(&ours, &row.rb) { "fifth" } else if parallel(&ours) == row.rb { "parallel" } else { "other" };
        *kinds.entry(kind).or_default() += 1;
        println!("  {:<46} rb {:<4} ours {:<4} {kind}", truncate(&row.title, 46), row.rb, ours);
    }
    println!("  {kinds:?}");
}

/// The grid a track's analysis gives the key rules.
fn key_grid_of(track: &Track, analysis: &rbl_analysis::Analysis) -> rbl_analysis::key::KeyGrid {
    let beat_secs: Vec<f64> = analysis.tempo.beats.iter().map(|b| f64::from(b.time_ms) / 1000.0).collect();
    let phase = rbl_analysis::downbeat::grid_phase(&track.samples, track.sample_rate, &beat_secs);
    rbl_analysis::key::KeyGrid {
        beats: analysis.tempo.beats.iter().map(|b| (f64::from(b.time_ms) / 1000.0, b.beat_number)).collect(),
        phrase_starts: phase.phrase_starts,
    }
}

const MAJORS: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const MINORS: [&str; 12] = ["Cm", "Dbm", "Dm", "Ebm", "Em", "Fm", "F#m", "Gm", "Abm", "Am", "Bbm", "Bm"];

/// The relative major of a minor key, or the relative minor of a major key.
fn relative(name: &str) -> String {
    if let Some(i) = MINORS.iter().position(|k| *k == name) {
        return MAJORS[(i + 3) % 12].to_owned();
    }
    if let Some(i) = MAJORS.iter().position(|k| *k == name) {
        return MINORS[(i + 9) % 12].to_owned();
    }
    String::new()
}

/// The same tonic in the other mode.
fn parallel(name: &str) -> String {
    if let Some(i) = MINORS.iter().position(|k| *k == name) {
        return MAJORS[i].to_owned();
    }
    if let Some(i) = MAJORS.iter().position(|k| *k == name) {
        return MINORS[i].to_owned();
    }
    String::new()
}

/// Same mode, tonic a fifth up or down.
fn a_fifth_away(a: &str, b: &str) -> bool {
    let index = |name: &str| -> Option<(usize, bool)> {
        MINORS.iter().position(|k| *k == name).map(|i| (i, true))
            .or_else(|| MAJORS.iter().position(|k| *k == name).map(|i| (i, false)))
    };
    match (index(a), index(b)) {
        (Some((ia, ma)), Some((ib, mb))) => ma == mb && ((ia + 7) % 12 == ib || (ib + 7) % 12 == ia),
        _ => false,
    }
}

/// Does the strongest pitch class of the bass name rekordbox's tonic? For
/// several bands and windows: the whole track, the first and last 45 s,
/// the first frame of each bar, and the frames between beats.
fn bassroot_experiment() {
    use rbl_analysis::key::{chroma_frames, fold_frames, KeyOptions, BINS};
    let dir = cache_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "gold")).collect())
        .unwrap_or_default();
    paths.sort();
    let bands: Vec<(&str, f64, f64, usize)> = vec![
        ("40-120 h1", 40.0, 120.0, 1), ("55-250 h1", 55.0, 250.0, 1), ("80-250 h1", 80.0, 250.0, 1),
        ("55-500 h2", 55.0, 500.0, 2), ("80-400 h2", 80.0, 400.0, 2), ("40-250 h1", 40.0, 250.0, 1),
    ];
    let windows = ["whole", "edges 45s", "downbeats", "off-beats", "edges+downbeats", "2nd eighth", "phrase 2 bars", "phrase 4 bars", "phrase 8 bars"];
    let tonic_of = |name: &str| -> Option<usize> {
        MINORS.iter().position(|k| *k == name).or_else(|| MAJORS.iter().position(|k| *k == name))
    };
    let counts = std::sync::Mutex::new(vec![vec![(0usize, 0usize); windows.len()]; bands.len()]); // (argmax hit, top-2 hit)
    let phrase_counts = std::sync::Mutex::new((0usize, 0usize)); // tracks, phrase starts
    let n = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(path) = paths.get(i) else { break };
                let Some(track) = read_track(path) else { continue };
                let Some(rb_tonic) = tonic_of(&track.key) else { continue };
                let analysis = rbl_analysis::analyse(&track.samples, track.sample_rate);
                let beats: Vec<f64> = analysis.tempo.beats.iter().map(|b| f64::from(b.time_ms) / 1000.0).collect();
                let downbeats: Vec<f64> = analysis.tempo.beats.iter().filter(|b| b.beat_number == 1).map(|b| f64::from(b.time_ms) / 1000.0).collect();
                let phrase_starts = key_grid_of(&track, &analysis).phrase_starts;
                let bar_secs = beats.windows(2).map(|w| w[1] - w[0]).next().unwrap_or(0.5) * 4.0;
                let hop_secs = 4096.0 / f64::from(track.sample_rate);
                n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                { let mut pc = phrase_counts.lock().unwrap(); pc.0 += 1; pc.1 += phrase_starts.len(); }
                for (bi, (_, lo, hi, h)) in bands.iter().enumerate() {
                    let options = KeyOptions { low_hz: *lo, high_hz: *hi, harmonics: *h, ..KeyOptions::default() };
                    let Some(frames) = chroma_frames(&track.samples, track.sample_rate, options) else { continue };
                    let total_secs = frames.len() as f64 * hop_secs;
                    let pick = |keep: &dyn Fn(usize) -> bool| -> Vec<[f64; BINS]> {
                        frames.iter().enumerate().filter(|(i, _)| keep(*i)).map(|(_, f)| *f).collect()
                    };
                    let frame_of = |t: f64| (t / hop_secs).floor().max(0.0) as usize;
                    let down: std::collections::HashSet<usize> = downbeats.iter().map(|&t| frame_of(t)).collect();
                    let on_beat: std::collections::HashSet<usize> = beats.iter().map(|&t| frame_of(t)).collect();
                    let centre = |i: usize| i as f64 * hop_secs + 8192.0 / 2.0 / f64::from(track.sample_rate);
                    let second_eighth = |i: usize| {
                        let t = centre(i);
                        let b = beats.partition_point(|&x| x <= t);
                        b > 0 && b < beats.len() && t >= (beats[b - 1] + beats[b]) / 2.0
                    };
                    let sets: Vec<Vec<[f64; BINS]>> = vec![
                        frames.clone(),
                        pick(&|i| { let t = i as f64 * hop_secs; t < 45.0 || t >= total_secs - 45.0 }),
                        pick(&|i| down.contains(&i)),
                        pick(&|i| !on_beat.contains(&i)),
                        pick(&|i| { let t = i as f64 * hop_secs; down.contains(&i) || t < 45.0 || t >= total_secs - 45.0 }),
                        pick(&second_eighth),
                        pick(&|i| second_eighth(i) && phrase_starts.iter().any(|&s| centre(i) >= s && centre(i) < s + 2.0 * bar_secs)),
                        pick(&|i| second_eighth(i) && phrase_starts.iter().any(|&s| centre(i) >= s && centre(i) < s + 4.0 * bar_secs)),
                        pick(&|i| second_eighth(i) && phrase_starts.iter().any(|&s| centre(i) >= s && centre(i) < s + 8.0 * bar_secs)),
                    ];
                    for (wi, set) in sets.iter().enumerate() {
                        let chroma = fold_frames(set, false, 0);
                        let mut order: Vec<usize> = (0..12).collect();
                        order.sort_by(|a, b| chroma[*b].partial_cmp(&chroma[*a]).unwrap());
                        let mut c = counts.lock().unwrap();
                        if order[0] == rb_tonic { c[bi][wi].0 += 1; }
                        if order[0] == rb_tonic || order[1] == rb_tonic { c[bi][wi].1 += 1; }
                    }
                }
            });
        }
    });
    let n = n.load(std::sync::atomic::Ordering::Relaxed);
    let counts = counts.into_inner().unwrap();
    let pc = phrase_counts.into_inner().unwrap();
    println!("phrase starts found: {:.1} per track", pc.1 as f64 / pc.0.max(1) as f64);
    println!("strongest bass class == rekordbox tonic (top-1 / top-2) of {n}:");
    print!("{:<12}", "band");
    for w in &windows { print!(" {w:>16}"); }
    println!();
    for (row, (name, ..)) in counts.iter().zip(&bands) {
        print!("{name:<12}");
        for (hit, top2) in row { print!(" {hit:>7} / {top2:<6}"); }
        println!();
    }
}
