//! Точка входа сервера Chunkwake.

mod cli;
mod config;

use chunkwake_net::Server;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use tokio::net::TcpListener;

pub const MC_VERSION: &str = "26.3";

#[tokio::main]
async fn main() {
    if let Err(error) = cli::execute().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn serve(dir: &Path) -> Result<(), String> {
    let (props, version) = cli::check(dir)?;
    tracing_subscriber::fmt().with_target(false).init();
    tracing::info!("Minecraft {} (protocol {})", version.name, version.protocol);

    let ip: IpAddr = props.server_ip.parse().expect("validated server-ip");
    let bind = SocketAddr::new(ip, props.server_port);
    let listener = TcpListener::bind(&bind).await.map_err(|e| {
        format!("bind {bind}: {e}; check server-ip/server-port or whether the port is already used")
    })?;
    tracing::info!("listening on {bind}");

    let server = Server::new(props.net_config(version));
    server
        .serve(listener, |player, _conn| {
            // Configuration/play появятся на следующих этапах; пока соединение закрывается.
            tracing::info!("{} reached configuration state", player.name);
        })
        .await
        .map_err(|e| e.to_string())
}
