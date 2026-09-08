//! Binding the two TCP ports a player connects to.
//!
//! A player first asks the port-query service which port the database server
//! is on, then opens a second connection there and speaks the message
//! protocol. Both are blocking threads: a session is one player, and a link
//! network has at most a handful.

use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::{Message, PORT_QUERY_REQUEST};

/// How long a session may sit idle before we close it.
const IDLE: Duration = Duration::from_secs(30);
/// How long a single read may block. Short enough to notice a shutdown.
const READ_TIMEOUT: Duration = Duration::from_millis(200);
/// A session's reassembly buffer never legitimately grows past this.
const MAX_PENDING: usize = 64 * 1024;

/// Answers messages. Implemented over the library index; kept as a trait so
/// the codec and the socket layer can be tested without one.
pub trait Handler: Send + Sync {
    /// Returns the messages to send back, which may be none.
    fn handle(&self, message: &Message) -> Vec<Message>;
}

/// Serves one already-accepted session until the peer closes it or `stop` is set.
pub fn serve_session(
    handler: &Arc<dyn Handler>,
    stream: &mut TcpStream,
    stop: &Arc<AtomicBool>,
) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_nodelay(true)?;

    let mut pending: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];
    let mut idle_since = std::time::Instant::now();

    while !stop.load(Ordering::Relaxed) {
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(()), // the peer closed
            Ok(len) => {
                idle_since = std::time::Instant::now();
                pending.extend_from_slice(chunk.get(..len).unwrap_or(&[]));
            }
            Err(error) if is_timeout(&error) => {
                if idle_since.elapsed() > IDLE {
                    return Ok(());
                }
                continue;
            }
            Err(error) => return Err(error),
        }

        // A buffer that keeps growing without yielding a message means the
        // peer is not speaking this protocol; drop it rather than grow forever.
        if pending.len() > MAX_PENDING {
            return Ok(());
        }

        let (messages, used) = Message::decode_all(&pending);
        pending.drain(..used);
        for message in messages {
            for reply in handler.handle(&message) {
                stream.write_all(&reply.encode())?;
            }
        }
    }
    Ok(())
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

/// The port-query service and the database server, both listening.
#[derive(Debug)]
pub struct Bound {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
    query: SocketAddr,
    database: SocketAddr,
}

impl Bound {
    /// Binds both listeners and starts accepting.
    ///
    /// Ports are arguments so a test can use ephemeral loopback ports: 12523
    /// belongs to rekordbox whenever it is running.
    pub fn start(
        handler: Arc<dyn Handler>,
        address: IpAddr,
        query_port: u16,
        database_port: u16,
    ) -> io::Result<Self> {
        let query_listener = TcpListener::bind(SocketAddr::new(address, query_port))?;
        let database_listener = TcpListener::bind(SocketAddr::new(address, database_port))?;
        let (query, database) = (query_listener.local_addr()?, database_listener.local_addr()?);

        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::with_capacity(2);

        // The port-query service: read the fixed request, answer with the port.
        {
            let stop = Arc::clone(&stop);
            let port = database.port();
            threads.push(std::thread::spawn(move || {
                accept_loop(&query_listener, &stop, move |stream| {
                    answer_port_query(stream, port)
                });
            }));
        }
        {
            let stop = Arc::clone(&stop);
            let inner = Arc::clone(&stop);
            threads.push(std::thread::spawn(move || {
                accept_loop(&database_listener, &stop, move |stream| {
                    serve_session(&handler, stream, &inner)
                });
            }));
        }

        Ok(Self { stop, threads, query, database })
    }

    pub const fn query_address(&self) -> SocketAddr {
        self.query
    }

    pub const fn database_address(&self) -> SocketAddr {
        self.database
    }

    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Unblock the accept loops by connecting to each once.
        for address in [self.query, self.database] {
            drop(TcpStream::connect_timeout(&address, Duration::from_millis(200)));
        }
        for thread in self.threads.drain(..) {
            drop(thread.join());
        }
    }
}

impl Drop for Bound {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn accept_loop<F>(listener: &TcpListener, stop: &Arc<AtomicBool>, session: F)
where
    F: Fn(&mut TcpStream) -> io::Result<()> + Send + Clone + 'static,
{
    while !stop.load(Ordering::Relaxed) {
        let Ok((mut stream, peer)) = listener.accept() else {
            continue;
        };
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let session = session.clone();
        // One thread per player. A link network has at most a handful, and a
        // slow session must not stall the others.
        drop(std::thread::spawn(move || {
            if let Err(error) = session(&mut stream) {
                tracing::debug!(%peer, %error, "session ended");
            }
        }));
    }
}

/// Reads the fixed port-query request and answers with a two-byte port.
fn answer_port_query(stream: &mut TcpStream, port: u16) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut request = vec![0_u8; PORT_QUERY_REQUEST.len()];
    stream.read_exact(&mut request)?;
    if request != PORT_QUERY_REQUEST {
        // Not the request we know; say nothing rather than guess.
        return Ok(());
    }
    stream.write_all(&port.to_be_bytes())
}
