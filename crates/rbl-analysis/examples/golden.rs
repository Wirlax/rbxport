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

/// Searches the key front end and matcher against rekordbox's key names.
///
/// Each front end costs one FFT pass over every track; the matcher variants
/// on top of it are free, so the front ends are few and the matchers many.
fn key_experiment() {
    use rbl_analysis::key::{best_key, chroma_frames, fold_frames, KeyOptions, Profile};
    let dir = cache_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "gold")).collect())
        .unwrap_or_default();
    paths.sort();
    let base = KeyOptions::default();
    let fronts: Vec<(&str, KeyOptions)> = vec![
        ("h4/.6 <1k", KeyOptions { harmonics: 4, harmonic_decay: 0.6, high_hz: 1000.0, ..base }),
        ("h4/.6 <1.5k", KeyOptions { harmonics: 4, harmonic_decay: 0.6, high_hz: 1500.0, ..base }),
        ("h4/.6 <2k", KeyOptions { harmonics: 4, harmonic_decay: 0.6, high_hz: 2000.0, ..base }),
        ("h4/.6 <3k", KeyOptions { harmonics: 4, harmonic_decay: 0.6, high_hz: 3000.0, ..base }),
        ("h3/.6 <2k", KeyOptions { harmonics: 3, harmonic_decay: 0.6, high_hz: 2000.0, ..base }),
        ("h5/.6 <2k", KeyOptions { harmonics: 5, harmonic_decay: 0.6, high_hz: 2000.0, ..base }),
        ("h4/.5 <2k", KeyOptions { harmonics: 4, harmonic_decay: 0.5, high_hz: 2000.0, ..base }),
        ("h4/.7 <2k", KeyOptions { harmonics: 4, harmonic_decay: 0.7, high_hz: 2000.0, ..base }),
        ("h4/.6 <2k sqrt", KeyOptions { harmonics: 4, harmonic_decay: 0.6, high_hz: 2000.0, power: 0.5, ..base }),
        ("h4/.6 <2k 80+", KeyOptions { harmonics: 4, harmonic_decay: 0.6, high_hz: 2000.0, low_hz: 80.0, ..base }),
    ];
    let bass = KeyOptions { high_hz: 250.0, harmonics: 1, power: 1.0, ..base };

    // Per track: rekordbox's key, then one folded chroma per front end (both
    // normalisations) and the bass chroma.
    // Indexed [front][norm][tuning].
    struct Row { rb: String, title: String, chroma: Vec<[[[f64; 12]; 2]; 2]>, bass: [[[f64; 12]; 2]; 2] }
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
                let fold_all = |frames: &[[f64; rbl_analysis::key::BINS]]| {
                    let offset = rbl_analysis::key::tuning_offset(frames);
                    [[fold_frames(frames, false, 0), fold_frames(frames, false, offset)],
                     [fold_frames(frames, true, 0), fold_frames(frames, true, offset)]]
                };
                let chroma: Vec<[[[f64; 12]; 2]; 2]> = fronts.iter().map(|(_, o)| {
                    let frames = chroma_frames(&track.samples, track.sample_rate, *o).unwrap_or_default();
                    fold_all(&frames)
                }).collect();
                let bass_frames = chroma_frames(&track.samples, track.sample_rate, bass).unwrap_or_default();
                let bass = fold_all(&bass_frames);
                rows.lock().unwrap().push(Row { rb: track.key.clone(), title: track.title.clone(), chroma, bass });
            });
        }
    });
    let rows = rows.into_inner().unwrap();
    println!("chroma for {} tracks in {:.1}s", rows.len(), started.elapsed().as_secs_f64());

    let profiles: Vec<(&str, Profile)> = vec![
        ("edma", Profile::EDMA), ("temperley", Profile::TEMPERLEY), ("krumhansl", Profile::KRUMHANSL),
        ("shaath", Profile::SHAATH), ("diatonic", Profile::DIATONIC),
    ];
    let mut results: Vec<(usize, usize, String)> = Vec::new();
    let biases = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5];
    let bass_weights = [0.0, 0.5, 1.0];
    let name_of = |fname: &str, norm: bool, tuning: bool, pname: &str, minor_bias: f64, bass_weight: f64| {
        format!("{fname:<14} norm {} tune {} {pname:<9} bias {minor_bias:.2} bass {bass_weight:.1}", u8::from(norm), u8::from(tuning))
    };
    for (fi, (fname, fopt)) in fronts.iter().enumerate() {
        for norm in [false, true] {
            for tuning in [false, true] {
                for (pname, profile) in &profiles {
                    for minor_bias in biases {
                        for bass_weight in bass_weights {
                            let options = KeyOptions { profile: *profile, minor_bias, frame_norm: norm, bass_weight, tuning, ..*fopt };
                            let (mut exact, mut compatible) = (0usize, 0usize);
                            for row in &rows {
                                let mut chroma = row.chroma[fi][usize::from(norm)][usize::from(tuning)];
                                if bass_weight > 0.0 {
                                    let b = row.bass[usize::from(norm)][usize::from(tuning)];
                                    let scale = bass_weight * chroma.iter().sum::<f64>() / b.iter().sum::<f64>().max(1e-12);
                                    for (slot, v) in chroma.iter_mut().zip(b.iter()) { *slot += v * scale; }
                                }
                                let ours = best_key(&chroma, options).map(|k| k.name).unwrap_or_default();
                                if ours == row.rb { exact += 1; }
                                if ours == row.rb || relative(&ours) == row.rb || a_fifth_away(&ours, &row.rb) { compatible += 1; }
                            }
                            results.push((exact, compatible, name_of(fname, norm, tuning, pname, minor_bias, bass_weight)));
                        }
                    }
                }
            }
        }
    }
    results.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let n = rows.len();
    println!("top variants (exact / compatible of {n}):");
    for (exact, compatible, name) in results.iter().take(25) {
        println!("  {exact:>3} / {compatible:>3}  {name}");
    }
    // Two refinements on the best front end: the mode from the third above
    // the tonic rather than a flat bias, and profiles learned from the
    // playlist itself with each track held out of its own profile.
    if let Some((_, _, name)) = results.first() {
        println!("refinements on the best front end ({name}):");
    }
    let best = results.first().map(|r| r.2.clone()).unwrap_or_default();
    let mut best_front: Option<(usize, KeyOptions)> = None;
    'find: for (fi, (fname, fopt)) in fronts.iter().enumerate() {
        for norm in [false, true] {
            for tuning in [false, true] {
                for (pname, profile) in &profiles {
                    for minor_bias in biases {
                        for bass_weight in bass_weights {
                            if name_of(fname, norm, tuning, pname, minor_bias, bass_weight) == best {
                                best_front = Some((fi, KeyOptions { profile: *profile, minor_bias, frame_norm: norm, bass_weight, tuning, ..*fopt }));
                                break 'find;
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some((fi, options)) = best_front {
        let chroma_of = |row: &Row| row.chroma[fi][usize::from(options.frame_norm)][usize::from(options.tuning)];
        let tonic_of = |name: &str| -> Option<(usize, bool)> {
            MINORS.iter().position(|k| *k == name).map(|i| (i, true)).or_else(|| MAJORS.iter().position(|k| *k == name).map(|i| (i, false)))
        };
        // Mode from the thirds: major when the major third beats the minor
        // third by a factor.
        for factor in [0.8, 1.0, 1.2, 1.5, 2.0] {
            let mut exact = 0usize;
            for row in &rows {
                let chroma = chroma_of(row);
                let Some(k) = best_key(&chroma, options) else { continue };
                let t = usize::from(k.tonic);
                let major = chroma[(t + 4) % 12] > chroma[(t + 3) % 12] * factor;
                let name = if major { MAJORS[t] } else { MINORS[t] };
                if name == row.rb { exact += 1; }
            }
            println!("  mode by thirds, factor {factor:.1}: {exact} / {n}");
        }
        // Tonic from the biased search, mode from the two profiles at that
        // tonic with a smaller bias of its own.
        for mode_bias in [0.0, 0.05, 0.1, 0.15, 0.2, 0.25] {
            let mut exact = 0usize;
            for row in &rows {
                let chroma = chroma_of(row);
                let Some(k) = best_key(&chroma, options) else { continue };
                let t = usize::from(k.tonic);
                let corr = |profile: &[f64; 12]| {
                    let rotated: Vec<f64> = (0..12).map(|i| profile[(i + 12 - t) % 12]).collect();
                    let mc = chroma.iter().sum::<f64>() / 12.0; let mp = rotated.iter().sum::<f64>() / 12.0;
                    let (mut num, mut dc, mut dp) = (0.0, 0.0, 0.0);
                    for i in 0..12 { let a = chroma[i] - mc; let b = rotated[i] - mp; num += a * b; dc += a * a; dp += b * b; }
                    num / (dc.sqrt() * dp.sqrt()).max(1e-12)
                };
                let minor = corr(&options.profile.minor) + mode_bias > corr(&options.profile.major);
                let name = if minor { MINORS[t] } else { MAJORS[t] };
                if name == row.rb { exact += 1; }
            }
            println!("  tonic biased, mode with bias {mode_bias:.2}: {exact} / {n}");
        }
        // Learned profiles, leave-one-out.
        let labelled: Vec<(usize, bool, [f64; 12])> = rows.iter().filter_map(|row| {
            let (t, minor) = tonic_of(&row.rb)?;
            let c = chroma_of(row);
            let sum: f64 = c.iter().sum();
            let mut rotated = [0.0_f64; 12];
            for i in 0..12 { rotated[i] = c[(i + t) % 12] / sum.max(1e-12); }
            Some((t, minor, rotated))
        }).collect();
        for minor_bias in [0.0, 0.1, 0.2, 0.3] {
            let mut exact = 0usize;
            let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            let mut lines = Vec::new();
            for (i, row) in rows.iter().enumerate() {
                let mut major = [0.0_f64; 12]; let mut minor = [0.0_f64; 12];
                let (mut nmaj, mut nmin) = (0.0_f64, 0.0_f64);
                for (j, (_, is_minor, rotated)) in labelled.iter().enumerate() {
                    if j == i { continue; }
                    let target = if *is_minor { &mut minor } else { &mut major };
                    for k in 0..12 { target[k] += rotated[k]; }
                    if *is_minor { nmin += 1.0 } else { nmaj += 1.0 }
                }
                for k in 0..12 { major[k] /= nmaj.max(1.0); minor[k] /= nmin.max(1.0); }
                let learned = KeyOptions { profile: Profile { major, minor }, minor_bias, ..options };
                let ours = best_key(&chroma_of(row), learned).map(|k| k.name).unwrap_or_default();
                if ours == row.rb { exact += 1; } else {
                    let kind = if relative(&ours) == row.rb { "relative" } else if a_fifth_away(&ours, &row.rb) { "fifth" } else if parallel(&ours) == row.rb { "parallel" } else { "other" };
                    *kinds.entry(kind).or_default() += 1;
                    lines.push(format!("    {:<46} rb {:<4} ours {:<4} {kind}", truncate(&row.title, 46), row.rb, ours));
                }
            }
            println!("  learned profiles (leave-one-out), bias {minor_bias:.1}: {exact} / {n}  {kinds:?}");
            if minor_bias == 0.1 { for l in lines { println!("{l}"); } }
        }
        // Margins: where rekordbox's key ranks among our candidates.
        let mut ranks: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
        for row in &rows {
            let chroma = chroma_of(row);
            let mut scored: Vec<(f64, String)> = Vec::new();
            for tonic in 0..12 { for minor in [false, true] {
                let o = KeyOptions { ..options };
                let profile = if minor { o.profile.minor } else { o.profile.major };
                let rotated: Vec<f64> = (0..12).map(|i| profile[(i + 12 - tonic) % 12]).collect();
                let mc = chroma.iter().sum::<f64>() / 12.0; let mp = rotated.iter().sum::<f64>() / 12.0;
                let (mut num, mut dc, mut dp) = (0.0, 0.0, 0.0);
                for i in 0..12 { let a = chroma[i] - mc; let b = rotated[i] - mp; num += a * b; dc += a * a; dp += b * b; }
                let score = num / (dc.sqrt() * dp.sqrt()).max(1e-12) + if minor { o.minor_bias } else { 0.0 };
                scored.push((score, if minor { MINORS[tonic].to_owned() } else { MAJORS[tonic].to_owned() }));
            }}
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            let rank = scored.iter().position(|(_, k)| *k == row.rb).unwrap_or(24);
            *ranks.entry(rank).or_default() += 1;
        }
        println!("  rank of rekordbox's key among our 24 (0 = ours): {ranks:?}");
    }
    // The failures of the best variant, classified.
    if let Some((_, _, name)) = results.first() {
        println!("failures of the best ({name}):");
    }
    let best = results.first().map(|r| r.2.clone()).unwrap_or_default();
    // Re-derive the best options from its name is clumsy; instead rerun the
    // search for the best tuple by index.
    let mut best_opts: Option<(usize, KeyOptions)> = None;
    'outer: for (fi, (fname, fopt)) in fronts.iter().enumerate() {
        for norm in [false, true] {
            for tuning in [false, true] {
                for (pname, profile) in &profiles {
                    for minor_bias in biases {
                        for bass_weight in bass_weights {
                            if name_of(fname, norm, tuning, pname, minor_bias, bass_weight) == best {
                                best_opts = Some((fi, KeyOptions { profile: *profile, minor_bias, frame_norm: norm, bass_weight, tuning, ..*fopt }));
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some((fi, options)) = best_opts {
        let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for row in &rows {
            let mut chroma = row.chroma[fi][usize::from(options.frame_norm)][usize::from(options.tuning)];
            if options.bass_weight > 0.0 {
                let b = row.bass[usize::from(options.frame_norm)][usize::from(options.tuning)];
                let scale = options.bass_weight * chroma.iter().sum::<f64>() / b.iter().sum::<f64>().max(1e-12);
                for (slot, v) in chroma.iter_mut().zip(b.iter()) { *slot += v * scale; }
            }
            let ours = best_key(&chroma, options).map(|k| k.name).unwrap_or_default();
            if ours == row.rb { continue; }
            let kind = if relative(&ours) == row.rb { "relative" } else if a_fifth_away(&ours, &row.rb) { "fifth" } else if parallel(&ours) == row.rb { "parallel" } else { "other" };
            *kinds.entry(kind).or_default() += 1;
            println!("  {:<46} rb {:<4} ours {:<4} {kind}", truncate(&row.title, 46), row.rb, ours);
        }
        println!("  {kinds:?}");
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
