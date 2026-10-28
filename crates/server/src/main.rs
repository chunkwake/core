//! Точка входа сервера Chunkwake.

mod cli;
mod config;
mod players;
mod setup_tui;
mod storage;
mod world_setup;

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
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(props.log_level)
        .init();
    tracing::info!("Minecraft {} (protocol {})", version.name, version.protocol);

    // Данные ванилы генерируются из server.jar один раз; дальше jar и Java не нужны.
    let cache_root = props.generated_path(dir);
    let data_dir = match chunkwake_data::generator::cached(&cache_root, &version.name) {
        Some(d) if config::read_cached_version(&d).is_some() => d,
        _ => {
            let java = std::env::var_os("CHUNKWAKE_JAVA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| props.java_command(dir));
            tracing::info!(
                "extracting vanilla data from server.jar with {} (one-time, usually 10-30s)",
                java.display()
            );
            let jar = props.jar_path(dir);
            let d = chunkwake_data::generator::ensure_generated_with(
                &java,
                &jar,
                &version.name,
                &cache_root,
                &|msg| tracing::info!("{msg}"),
            )
            .map_err(|e| e.to_string())?;
            config::cache_version(&jar, &d)?;
            tracing::info!(
                "vanilla data cached in {}; server.jar is no longer needed",
                d.display()
            );
            d
        }
    };
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

    tracing::info!("loading world data (blocks, items, loot tables)");
    let started = std::time::Instant::now();
    let (play, storage) = world_setup::build_play_plan(
        &data_dir,
        &packets,
        &registries,
        &props,
        &dir.join(&props.level_name),
    )?;
    let (play, storage) = (std::sync::Arc::new(play), std::sync::Arc::new(storage));
    // Регионы читаются с диска при первом обращении к их чанкам.
    play.set_source(storage.clone());
    play.set_player_source(storage.clone());
    play.set_ops(chunkwake_net::ops::OpList::load(dir.join("ops.json"))?);
    tracing::info!(
        "world: {} (data loaded in {:.1}s)",
        props.level_name,
        started.elapsed().as_secs_f64()
    );
    play.spawn_tick();
    storage.spawn_autosave(play.clone());
    start_plugins(dir, &props, &play).await;

    let mut net = props.net_config(version);
    net.configuration = Some(std::sync::Arc::new(plan));
    let server = Server::new(net);
    let world = play.clone();
    let serving = server.serve(listener, move |player, mut conn| {
        let play = play.clone();
        tokio::spawn(async move {
            let result = play.run(&player, &mut conn).await;
            tracing::info!(name = %player.name, ?result, "player left");
        });
    });
    // Консоль: строки stdin — команды сервера (`op`, `deop`, `list`, `stop`, плагины).
    let (console_tx, mut console) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::Builder::new()
        .name("console".into())
        .spawn(move || {
            for line in std::io::stdin().lines().map_while(Result::ok) {
                let line = line.trim().trim_start_matches('/').to_owned();
                if !line.is_empty() && console_tx.send(line).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    tokio::pin!(serving);
    // Ctrl+C или `stop`: сохранить мир и выйти.
    loop {
        tokio::select! {
            result = &mut serving => return result.map_err(|e| e.to_string()),
            _ = tokio::signal::ctrl_c() => break,
            Some(line) = console.recv() => {
                if line == "stop" {
                    break;
                }
                let reply = world.console_command(&line).await;
                if !reply.is_empty() {
                    tracing::info!("{reply}");
                }
            }
        }
    }
    tracing::info!("stopping: saving world");
    let n = tokio::task::spawn_blocking(move || storage.save(&world))
        .await
        .map_err(|e| e.to_string())??;
    tracing::info!("saved {n} chunks and players");
    Ok(())
}

/// Paper-плагины из `plugins/`: без API (`java/dist`, `api/`) или Java сервер
/// работает без них.
async fn start_plugins(
    dir: &Path,
    props: &config::Properties,
    play: &std::sync::Arc<chunkwake_net::play::PlayPlan>,
) {
    let Some(api) = chunkwake_bridge::find_api_dir(dir) else {
        tracing::warn!(
            "plugin API not found (build it with `java/gradlew -p java dist`); plugins disabled"
        );
        return;
    };
    let java = std::env::var_os("CHUNKWAKE_JAVA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| props.java_command(dir));
    let plugins = dir.join("plugins");
    let plan = play.clone();
    let started = tokio::task::spawn_blocking(move || {
        chunkwake_bridge::Bridge::start(&java, &api, &plugins, &plan)
    })
    .await;
    match started {
        Ok(Ok((bridge, n))) => {
            play.set_hooks(bridge);
            tracing::info!("enabled {n} plugin(s)");
        }
        Ok(Err(e)) => tracing::error!("plugins disabled: {e}"),
        Err(e) => tracing::error!("plugins disabled: {e}"),
    }
}
