//! Listening for devices on a Pro DJ Link network.
//!
//! Listen-only. Every player and mixer announces itself on UDP 50000 every
//! 1.5 seconds, so a socket bound there sees the whole network without saying
//! anything. **Nothing is transmitted**: announcing ourselves as a source is
//! the part that needs the database server's menus, which are not built, and a
//! device that announces and then cannot answer is worse than one that stays
//! quiet.
//!
//! The port is shared with rekordbox, which holds it whenever it is running,
//! so binding is expected to fail sometimes and says so rather than retrying.

use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rbl_prolink::{DeviceTable, KeepAlive, PORT_ANNOUNCE};
use serde::Serialize;

/// How long a read blocks before the thread checks whether it should stop.
const POLL: Duration = Duration::from_millis(250);

/// Peers are reported no more often than this, however chatty the network is:
/// six devices announcing at 1.5 s each is four events a second otherwise.
const REPORT_EVERY: Duration = Duration::from_millis(500);

/// A device on the network, as the interface shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerDto {
    pub name: String,
    pub device_number: u8,
    pub kind: String,
    pub address: String,
    pub last_seen_ms: u64,
}

/// Whether the network can be listened to, and who is on it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatusDto {
    pub listening: bool,
    /// Why not, when it is not.
    pub problem: Option<String>,
    pub peers: Vec<PeerDto>,
}

/// A running listener.
#[derive(Debug)]
pub struct Listener {
    stop: Arc<AtomicBool>,
}

impl Listener {
    /// Binds the announce port and reports peers until stopped.
    ///
    /// Binds to all interfaces: a link network is usually a second adapter,
    /// and guessing which would mean guessing the user's setup.
    pub fn start<F>(mut report: F) -> std::io::Result<Self>
    where
        F: FnMut(Vec<PeerDto>) + Send + 'static,
    {
        let socket = UdpSocket::bind(("0.0.0.0", PORT_ANNOUNCE))?;
        socket.set_read_timeout(Some(POLL))?;
        // Players announce to the broadcast address, so the socket has to
        // accept broadcast traffic to see them at all.
        socket.set_broadcast(true)?;

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut table = DeviceTable::new();
            let started = Instant::now();
            let mut last_report = Instant::now();
            let mut buffer = [0_u8; 512];

            while !thread_stop.load(Ordering::Relaxed) {
                let now_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                if let Ok((len, _from)) = socket.recv_from(&mut buffer) {
                    if let Ok(keep_alive) =
                        KeepAlive::decode(buffer.get(..len).unwrap_or(&[]))
                    {
                        table.observe(&keep_alive, now_ms);
                    }
                }
                // Expiring on every pass, not only on a packet: a device that
                // goes quiet has to disappear from the list.
                table.expire(now_ms);

                if last_report.elapsed() >= REPORT_EVERY {
                    last_report = Instant::now();
                    report(
                        table
                            .peers()
                            .iter()
                            .map(|peer| PeerDto {
                                name: peer.name.clone(),
                                device_number: peer.device_number,
                                kind: format!("{:?}", peer.device_type),
                                address: peer.ip.to_string(),
                                last_seen_ms: now_ms.saturating_sub(peer.last_seen_ms),
                            })
                            .collect(),
                    );
                }
            }
        });

        Ok(Self { stop })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Why the announce port could not be bound, in words worth showing.
pub fn explain(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::AddrInUse => format!(
            "Something already has UDP {PORT_ANNOUNCE} — usually rekordbox itself. \
             Quit it to listen."
        ),
        std::io::ErrorKind::PermissionDenied => {
            format!("Not allowed to bind UDP {PORT_ANNOUNCE}.")
        }
        _ => format!("Could not listen on UDP {PORT_ANNOUNCE}: {error}"),
    }
}
