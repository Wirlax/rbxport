//! READ-ONLY: serves the installed library over the link, as the app does
//! with LINK on, and prints the players until Ctrl-C.
//! `cargo run --release -p rbl-link --example serve [interface]`
//!
//! rekordbox must not be running: it holds the ports. Verify from another
//! terminal with `tcpdump -i <interface> udp port 50000` (a keep-alive every
//! 2 s), or with a player on the network.
#![allow(clippy::pedantic, clippy::print_stdout, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// perf-ok: a read-only tool run by hand, not shipped code; printing is its point.

use std::sync::Arc;
use std::time::Duration;

fn main() {
    let wanted = std::env::args().nth(1);
    let interfaces = rbl_link::interfaces();
    let interface = match &wanted {
        Some(name) => interfaces.iter().find(|i| &i.name == name).cloned(),
        None => interfaces.first().cloned(),
    }
    .unwrap_or_else(|| {
        // perf-ok: a tool run by hand.
        eprintln!("interfaces: {:?}", interfaces.iter().map(|i| format!("{} {}", i.name, i.address)).collect::<Vec<_>>());
        panic!("no such interface");
    });

    // perf-ok: a read-only tool run by hand.
    let db = rbl_db::Library::open_installed_read_only().expect("open");
    let (library, _) = rbl_index::load(&db).expect("index"); // perf-ok: a tool run by hand.
    let share_root = db.location().share_root.clone();
    // perf-ok: printing is the point of a tool.
    println!("{} tracks; {} playlists; share {}", library.len(), library.playlists().len(), share_root.display());

    let source = Arc::new(rbl_link::StaticSource { library: Arc::new(library), share_root });
    let started = std::time::Instant::now();
    // perf-ok: a tool run by hand; failing to bind is its answer.
    let link = rbl_link::LinkExport::start(source, interface, rbl_link::Ports::REKORDBOX).expect("start");
    let snapshot = link.snapshot();
    println!(
        "serving as rekordbox on {} ({}) in {:?}; database port {}, query {}, portmap {}",
        snapshot.interface.name,
        snapshot.interface.address,
        started.elapsed(),
        snapshot.database_port,
        link.query_address(),
        link.portmap_address()
    );
    loop {
        std::thread::sleep(Duration::from_secs(2));
        let players = link.snapshot().players;
        if players.is_empty() {
            println!("no players");
        }
        for p in players {
            println!(
                "{:?} {} #{} at {}: loaded {:?} playing={} master={} bpm={}",
                p.kind, p.name, p.number, p.address, p.loaded, p.playing, p.master, p.bpm_x100
            );
        }
    }
}
