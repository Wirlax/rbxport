//! This fork's own: `tempo-bridge`, a read-only HTTP bridge that lets the
//! Tempo app see Ronan's rekordbox playlists and hear their tracks. It
//! listens on 127.0.0.1 only and answers to a shared key. It reads the
//! library through `rbl-db` in read-only mode and never writes it.

mod api;
mod audio;
mod snapshot;
mod store;

use std::sync::Arc;

const DEFAULT_PORT: u16 = 4107;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("tempo-bridge: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("TEMPO_BRIDGE_KEY").unwrap_or_default();
    if key.trim().is_empty() {
        return Err("TEMPO_BRIDGE_KEY is not set: refusing to start without a key".into());
    }
    let port = match std::env::var("TEMPO_BRIDGE_PORT") {
        Ok(text) if !text.trim().is_empty() => text.trim().parse::<u16>().map_err(|_| format!("TEMPO_BRIDGE_PORT {text:?} is not a port"))?,
        _ => DEFAULT_PORT,
    };

    let location = rbl_db::detect()?;
    let store = tokio::task::spawn_blocking(move || store::Store::open(location)).await??;
    let app = api::router(Arc::new(api::AppState { store, key }));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    eprintln!("tempo-bridge: listening on 127.0.0.1:{port}");
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

/// Ends on Ctrl-C or on the TERM signal launchd sends.
async fn shutdown() {
    use tokio::signal::unix::{signal, SignalKind};
    let term = async {
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = term => {}
    }
}
