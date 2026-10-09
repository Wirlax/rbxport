//! This fork's own: `rbxport-mcp`, an MCP server over stdio that builds
//! mini-set playlists in Ronan's rekordbox library from Claude Desktop or
//! Claude Code. The tools are in `server.rs`; how a write reaches the
//! library, and when the app is started and quit, in `app.rs`.

mod app;
mod library;
mod server;

use std::sync::Arc;
use std::time::Duration;

use rmcp::ServiceExt;

/// How long a launch of ours may sit unused after the last write before it
/// is quit.
const IDLE: Duration = Duration::from_secs(10 * 60);

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        // Stdout is the protocol's: what went wrong goes to the client's log.
        eprintln!("rbxport-mcp: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app = Arc::new(app::App::new());
    let watcher = {
        let app = Arc::clone(&app);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            loop {
                tick.tick().await;
                app.quit_if_idle(IDLE).await;
            }
        })
    };
    let service = server::Server::new(Arc::clone(&app), None).serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    watcher.abort();
    app.quit_if_ours().await;
    Ok(())
}
