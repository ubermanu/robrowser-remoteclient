use clap::Parser;
use std::{io, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::signal::unix::{SignalKind, signal};

mod client;
mod des;
mod grf;
mod server;
mod state;

/// Serve assets over HTTP for a roBrowser client
#[derive(Parser)]
struct Args {
    /// Path to the Ragnarok client directory
    client: PathBuf,

    /// Address to listen on
    #[arg(
        long,
        env = "ROBROWSER_REMOTECLIENT_BIND",
        default_value = "0.0.0.0:8080"
    )]
    bind: SocketAddr,

    /// Send permissive CORS headers
    #[arg(long, env = "ROBROWSER_REMOTECLIENT_CORS")]
    cors: bool,

    /// Answer file name searches, used by the GRF and map viewers
    #[arg(long, env = "ROBROWSER_REMOTECLIENT_SEARCH")]
    search: bool,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();
    let shared = Arc::new(state::Shared::open(&args.client)?);

    reindex_on_hangup(Arc::clone(&shared))?;

    server::serve(shared, args.bind, args.cors, args.search).await
}

/// Rebuild the file index on `SIGHUP`, so files added to the client directory
/// can be picked up without dropping the connections a restart would.
/// `systemctl reload` spells this as `ExecReload=/bin/kill -HUP $MAINPID`.
fn reindex_on_hangup(shared: Arc<state::Shared>) -> io::Result<()> {
    let mut hangups = signal(SignalKind::hangup())?;

    tokio::spawn(async move {
        while hangups.recv().await.is_some() {
            // Opening the archives and walking the loose directories both
            // block for as long as the client directory is large.
            let shared = Arc::clone(&shared);
            match tokio::task::spawn_blocking(move || shared.reload()).await {
                Ok(Ok(())) => println!("reindexed"),
                Ok(Err(error)) => eprintln!("reindex failed: {error}"),
                Err(error) => eprintln!("reindex panicked: {error}"),
            }
        }
    });

    Ok(())
}
