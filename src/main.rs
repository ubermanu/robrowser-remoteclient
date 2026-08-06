use std::{io, net::SocketAddr, path::PathBuf, sync::Arc};

use clap::Parser;

mod client;
mod grf;
mod server;

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
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();
    let client = Arc::new(client::Client::open(&args.client)?);
    server::serve(client, args.bind).await
}
