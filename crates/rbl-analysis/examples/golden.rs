//! Milestone 3 gate: compare our analysis against rekordbox's own stamps.
//!
//! READ-ONLY. Reads the installed library, decodes real audio files and
//! compares detected BPM and key against what rekordbox recorded.
//!
//! `cargo run --release -p rbl-analysis --example golden -- [count]`
#![allow(clippy::pedantic, clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::time::Instant;

/// Only the first minutes are needed: tempo and key are global properties and
/// decoding a whole DJ set per track would dominate the run.
const ANALYSE_SECS: f64 = 90.0;

fn main() {
    let want: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(200);

    let db = match rbl_db::Library::open_installed_read_only() {
        Ok(db) => db,
        Err(e) => { println!("cannot open library: {e}"); return; }
    };
    let (library, _stats) = rbl_index::load(&db).expect("index");
    println!("library: {} tracks", library.len());

    // Candidates: analysed by rekordbox, with a BPM and a readable file.
    let mut candidates: Vec<u32> = Vec::new();
    for row in 0..library.len() as u32 {
        let i = row as usize;
        if library.bpm_x100[i] == 0 || library.analysed[i] == 0 {
            continue;
        }
        let path = library.folder_path.get(i);
        if path.is_empty() || !Path::new(path).exists() {
            continue;
        }
        candidates.push(row);
        if candidates.len() >= want * 3 {
            break;
        }
    }
    println!("candidates with a BPM and a present file: {}", candidates.len());

    let mut bpm_errors: Vec<f64> = Vec::new();
    let mut octave_errors = 0usize;
    let mut key_matches = 0usize;
    let mut key_compared = 0usize;
    let mut analysed = 0usize;
    let mut failed = 0usize;
    let started = Instant::now();

    for &row in candidates.iter() {
        if analysed >= want {
            break;
        }
        let i = row as usize;
        let path = Path::new(library.folder_path.get(i));
        let expected_bpm = f64::from(library.bpm_x100[i]) / 100.0;
        let expected_key = library.key_name(row).to_owned();

        let audio = match rbl_audio::decode_mono(path, Some(ANALYSE_SECS)) {
            Ok(a) => a,
            Err(_) => { failed += 1; continue; }
        };
        if audio.duration_secs() < 20.0 {
            continue;
        }

        let result = rbl_analysis::analyse(&audio.samples, audio.sample_rate);
        analysed += 1;

        let folded = rbl_analysis::tempo::nearest_octave(result.tempo.bpm, expected_bpm);
        let direct_error = (result.tempo.bpm - expected_bpm).abs();
        let folded_error = (folded - expected_bpm).abs();
        if folded_error < direct_error - 0.01 {
            octave_errors += 1;
        }
        bpm_errors.push(folded_error);

        if !expected_key.is_empty() {
            if let Some(key) = &result.key {
                key_compared += 1;
                // Count the relative major/minor as agreement: they share every
                // note, and rekordbox itself reports either for such tracks.
                if key.name == expected_key || relative(&key.name) == expected_key {
                    key_matches += 1;
                }
            }
        }

        if analysed <= 10 {
            println!(
                "  {:<44} rb {:>6.2} | ours {:>6.2} (folded {:>6.2})  key rb {:<4} ours {:<4}",
                truncate(library.title.get(i), 44),
                expected_bpm,
                result.tempo.bpm,
                folded,
                if expected_key.is_empty() { "-" } else { &expected_key },
                result.key.as_ref().map_or("-", |k| k.name.as_str()),
            );
        }
    }

    bpm_errors.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = bpm_errors.len();
    let pct = |p: f64| bpm_errors.get(((n as f64 * p) as usize).min(n.saturating_sub(1))).copied().unwrap_or(0.0);
    let within = |t: f64| bpm_errors.iter().filter(|&&e| e <= t).count();

    println!("\n== BPM vs rekordbox ({n} tracks, {:.1}s) ==", started.elapsed().as_secs_f64());
    if n > 0 {
        println!("  median error   {:.3} BPM", pct(0.5));
        println!("  p90 error      {:.3} BPM", pct(0.9));
        println!("  worst          {:.3} BPM", pct(1.0));
        println!("  within 0.05    {} / {n}  ({:.0}%)", within(0.05), within(0.05) as f64 / n as f64 * 100.0);
        println!("  within 0.5     {} / {n}  ({:.0}%)", within(0.5), within(0.5) as f64 / n as f64 * 100.0);
        println!("  within 2.0     {} / {n}  ({:.0}%)", within(2.0), within(2.0) as f64 / n as f64 * 100.0);
        println!("  needed octave folding: {octave_errors} / {n}");
    }
    if key_compared > 0 {
        println!("== Key vs rekordbox ==");
        println!("  agreement {key_matches} / {key_compared}  ({:.0}%)",
                 key_matches as f64 / key_compared as f64 * 100.0);
    }
    if failed > 0 {
        println!("  ({failed} files could not be decoded)");
    }
}

/// The relative major of a minor key, or the relative minor of a major key.
fn relative(name: &str) -> String {
    const MAJOR: [&str; 12] = ["C","Db","D","Eb","E","F","F#","G","Ab","A","Bb","B"];
    const MINOR: [&str; 12] = ["Cm","Dbm","Dm","Ebm","Em","Fm","F#m","Gm","Abm","Am","Bbm","Bm"];
    if let Some(i) = MINOR.iter().position(|k| *k == name) {
        return MAJOR[(i + 3) % 12].to_owned();
    }
    if let Some(i) = MAJOR.iter().position(|k| *k == name) {
        return MINOR[(i + 9) % 12].to_owned();
    }
    String::new()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}
