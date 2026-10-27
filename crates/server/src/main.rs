//! Точка входа сервера Chunkwake.

mod cli;
mod config;
mod setup_tui;

use chunkwake_net::Server;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use tokio::net::TcpListener;

pub const MC_VERSION: &str = "26.3";

#[tokio::main]
async fn main() {
    // Локальные переменные окружения (например, CHUNKWAKE_JAVA) из .env в текущем
    // каталоге или выше. Переменные, уже заданные в окружении, не перезаписываются.
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        eprintln!("warning: .env: {error}");
    }
    if let Err(error) = cli::execute().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn serve(dir: &Path) -> Result<(), String> {
    let (props, version) = cli::check(dir)?;
    tracing_subscriber::fmt().with_target(false).init();
    tracing::info!("Minecraft {} (protocol {})", version.name, version.protocol);

    // Данные ванилы генерируются из server.jar один раз и кэшируются по версии.
    let java = std::env::var_os("CHUNKWAKE_JAVA").unwrap_or_else(|| "java".into());
    tracing::info!("preparing vanilla data (first run takes a few seconds)");
    let data_dir = chunkwake_data::generator::ensure_generated(
        Path::new(&java),
        &props.jar_path(dir),
        &version.name,
        &dir.join("generated"),
    )
    .map_err(|e| e.to_string())?;
    let packets =
        chunkwake_data::packets::PacketTable::load(&data_dir.join("reports/packets.json"))
            .map_err(|e| e.to_string())?;
    packets.verify_hardcoded().map_err(|e| e.to_string())?;
    tracing::info!("loaded {} packet ids", packets.len());

    let registries =
        chunkwake_data::registries::RegistryData::load(&data_dir).map_err(|e| e.to_string())?;
    let plan = chunkwake_net::configuration::ConfigurationPlan::build(
        &packets,
        &registries,
        &version.name,
    )
    .map_err(|e| e.to_string())?;
    tracing::info!(
        "prepared {} synchronized registries and tags for {} registries ({} KiB)",
        registries.synchronized.len(),
        registries.tags.len(),
        plan.sync_bytes() / 1024
    );

    let ip: IpAddr = props.server_ip.parse().expect("validated server-ip");
    let bind = SocketAddr::new(ip, props.server_port);
    let listener = TcpListener::bind(&bind).await.map_err(|e| {
        format!("bind {bind}: {e}; check server-ip/server-port or whether the port is already used")
    })?;
    tracing::info!("listening on {bind}");

    let mut net = props.net_config(version);
    net.configuration = Some(std::sync::Arc::new(plan));
    let server = Server::new(net);
    server
        .serve(listener, |player, _conn| {
            // Play появится в M2; пока соединение закрывается.
            tracing::info!("{} reached play state", player.name);
        })
        .await
        .map_err(|e| e.to_string())
}
