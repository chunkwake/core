//! Точка входа сервера Chunkwake.

mod backup;
mod cli;
mod config;
mod players;
mod setup_tui;
mod storage;
mod world_setup;

use chunkwake_server::MC_VERSION;
use chunkwake_net::Server;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use tokio::net::TcpListener;

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

/// Местное время до секунды: `2026-10-28 11:04:32`, без `T`, `Z` и долей секунды.
struct LogTime;

impl tracing_subscriber::fmt::time::FormatTime for LogTime {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

async fn serve(dir: &Path) -> Result<(), String> {
    let _server_lock = backup::lock(dir)?;
    let (props, version) = cli::check(dir)?;
    tracing_subscriber::fmt()
        .with_timer(LogTime)
        .with_target(false)
        .with_max_level(props.log_level)
        .init();
    tracing::info!("Minecraft {} (protocol {})", version.name, version.protocol);
    let boot_started = std::time::Instant::now();

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
    let boot = {
        let started = std::time::Instant::now();
        let boot = chunkwake_data::boot::load_or_build(&data_dir).map_err(|e| e.to_string())?;
        tracing::info!(
            "data: {} packet ids, {} registries (boot image in {} ms)",
            boot.packets.len(),
            boot.registries.tags.len(),
            started.elapsed().as_millis()
        );
        boot
    };
    let packets = &boot.packets;
    packets.verify_hardcoded().map_err(|e| e.to_string())?;
    let registries = &boot.registries;
    let plan =
        chunkwake_net::configuration::ConfigurationPlan::build(packets, registries, &version.name)
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

    let started = std::time::Instant::now();
    let (play, storage) =
        world_setup::build_play_plan(&data_dir, boot, &props, &dir.join(&props.level_name))?;
    let (play, storage) = (std::sync::Arc::new(play), std::sync::Arc::new(storage));
    // Регионы читаются с диска при первом обращении к их чанкам.
    play.set_source(storage.clone());
    play.set_player_source(storage.clone());
    // Генерация чанков — на своих потоках, чтобы тик не ждал счёта.
    play.set_self_weak();
    play.set_gen_pool(chunkwake_net::chunkgen::GenPool::new(
        chunkwake_net::chunkgen::workers(),
    ));
    play.set_ops(chunkwake_net::ops::OpList::load(dir.join("ops.json"))?);
    play.set_spawn_protection(props.spawn_protection as i32);
    tracing::info!(
        "world: {} (world in {} ms, up in {} ms)",
        props.level_name,
        started.elapsed().as_millis(),
        boot_started.elapsed().as_millis()
    );
    // Видно, какие дистанции реально применяются: симуляция обрезается по прорисовке.
    tracing::info!(
        "chunks: view distance {}, simulation distance {}",
        props.view_distance,
        props.simulation_distance.min(props.view_distance)
    );
    let tick_task = play.spawn_tick();
    // Метрики: сэмплер процесса раз в секунду, снимок — для `/top` и HTTP.
    let _metrics = start_metrics(&props, &play);
    let autosave_task = storage.spawn_autosave(play.clone());
    let bridge = start_plugins(dir, &props, &play).await;

    let mut net = props.net_config(version);
    net.configuration = Some(std::sync::Arc::new(plan));
    let server = Server::new(net);
    let world = play.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let (session_tx, mut session_rx) = tokio::sync::mpsc::unbounded_channel();
    let serving = server.serve_until(listener, shutdown_rx, move |player, conn| {
        let _ = session_tx.send((player, conn));
    });
    let mut sessions = tokio::task::JoinSet::new();
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
    let mut serve_error = None;
    let mut serving_done = false;
    let mut session_errors = Vec::new();
    // Ctrl+C или `stop`: завершить соединения, плагины и сохранить мир.
    loop {
        tokio::select! {
            result = &mut serving => {
                serve_error = result.err().map(|e| e.to_string());
                serving_done = true;
                break;
            },
            _ = tokio::signal::ctrl_c() => break,
            Some((player, mut conn)) = session_rx.recv() => {
                let play = play.clone();
                sessions.spawn(async move {
                    let result = play.run(&player, &mut conn).await;
                    log_player_exit(&player.name, result);
                });
            }
            Some(result) = sessions.join_next(), if !sessions.is_empty() => {
                if let Err(error) = result {
                    tracing::error!(%error, "player task failed");
                    session_errors.push(format!("player task failed: {error}"));
                }
            }
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
    let _ = shutdown_tx.send(true);
    if !serving_done {
        serve_error = serving.await.err().map(|e| e.to_string());
    }
    while let Ok((player, mut conn)) = session_rx.try_recv() {
        let play = play.clone();
        sessions.spawn(async move {
            let result = play.run(&player, &mut conn).await;
            log_player_exit(&player.name, result);
        });
    }
    play.disconnect_all();
    let session_timeout = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(result) = sessions.join_next().await {
            if let Err(error) = result {
                tracing::error!(%error, "player task failed");
                session_errors.push(format!("player task failed: {error}"));
            }
        }
    })
    .await
    .is_err();
    if session_timeout {
        tracing::error!("player shutdown timed out after 10 seconds");
        sessions.abort_all();
        while sessions.join_next().await.is_some() {}
    }
    tick_task.abort();
    autosave_task.abort();
    let _ = tick_task.await;
    let _ = autosave_task.await;
    let mut errors = Vec::new();
    if let Some(error) = serve_error {
        errors.push(format!("network: {error}"));
    }
    errors.extend(session_errors);
    if session_timeout {
        errors.push("player shutdown timed out".into());
    }
    if let Some(bridge) = bridge {
        match bridge.shutdown() {
            Ok(reply) => match tokio::task::spawn_blocking(move || {
                reply.recv_timeout(std::time::Duration::from_secs(10))
            })
            .await
            {
                Ok(Ok(true)) => {}
                Ok(Ok(false)) => errors.push("plugin shutdown failed".into()),
                Ok(Err(error)) => errors.push(format!("plugin shutdown: {error}")),
                Err(error) => errors.push(format!("plugin shutdown task: {error}")),
            },
            Err(error) => errors.push(format!("plugin shutdown: {error}")),
        }
    }
    tracing::info!("stopping: saving world");
    match tokio::task::spawn_blocking(move || storage.save(&world)).await {
        Ok(Ok(n)) => tracing::info!("saved {n} chunks; pending player snapshots flushed"),
        Ok(Err(error)) => errors.push(error),
        Err(error) => errors.push(format!("final save task: {error}")),
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// Запускает сэмплер процесса и HTTP-эндпоинт метрик.
///
/// Сэмплер опрашивает `sysinfo` раз в секунду: загрузку CPU он считает как
/// разницу между опросами, поэтому делать это из команды `/top` нельзя.
/// Снимок в памяти общий для `/metrics`, `/snapshot` и `chunkwake top`.
fn start_metrics(
    props: &config::Properties,
    play: &std::sync::Arc<chunkwake_net::play::PlayPlan>,
) -> Option<chunkwake_metrics::http::Shared> {
    if !props.metrics_enabled {
        tracing::info!("metrics disabled (metrics-enabled=false)");
        return None;
    }
    let snapshot: chunkwake_metrics::http::Shared =
        std::sync::Arc::new(std::sync::RwLock::new(chunkwake_metrics::Snapshot::default()));
    let play_sampler = play.clone();
    let snapshot_sampler = snapshot.clone();
    tokio::spawn(async move {
        let mut sampler = chunkwake_metrics::ProcessSampler::new();
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let proc_stats = sampler.sample();
            play_sampler.set_proc_stats(proc_stats.clone());
            let mut fresh = play_sampler.load_snapshot();
            fresh.proc = proc_stats;
            if let Ok(mut slot) = snapshot_sampler.write() {
                *slot = fresh;
            }
        }
    });
    let ip: std::net::IpAddr = props
        .metrics_ip
        .parse()
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let port = props.metrics_port;
    let snapshot_server = snapshot.clone();
    tokio::spawn(async move {
        match chunkwake_metrics::http::spawn(ip, port, snapshot_server).await {
            Ok(bound) if bound == port => tracing::info!("metrics on http://{ip}:{bound}/metrics"),
            // Порт был занят: говорим явно, иначе `chunkwake top` без флагов
            // будет искать сервер на «правильном» порту и не найдёт.
            Ok(bound) => tracing::warn!(
                "metrics on http://{ip}:{bound}/metrics (port {port} is busy; \
                 `chunkwake top --port {bound}` or update metrics-port)"
            ),
            Err(error) => tracing::error!("metrics disabled: {error}"),
        }
    });
    Some(snapshot)
}

fn log_player_exit(name: &str, result: Result<(), chunkwake_net::NetError>) {    match result {
        Ok(()) => tracing::info!(%name, "player left"),
        Err(chunkwake_net::NetError::Io(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
            ) =>
        {
            tracing::info!(%name, "player disconnected");
        }
        Err(error) => tracing::warn!(%name, ?error, "player session ended with an error"),
    }
}

/// Paper-плагины из `plugins/`: без API (`java/dist`, `api/`) или Java сервер
/// работает без них.
async fn start_plugins(
    dir: &Path,
    props: &config::Properties,
    play: &std::sync::Arc<chunkwake_net::play::PlayPlan>,
) -> Option<std::sync::Arc<chunkwake_bridge::Bridge>> {
    let plugins = dir.join("plugins");
    let has_plugins = match std::fs::read_dir(&plugins) {
        Ok(entries) => entries.filter_map(Result::ok).any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!("cannot inspect {}: {error}", plugins.display());
            return None;
        }
    };
    if !has_plugins {
        return None;
    }
    let Some(api) = chunkwake_bridge::find_api_dir(dir) else {
        tracing::warn!(
            "plugin API not found (build it with `java/gradlew -p java dist`); plugins disabled"
        );
        return None;
    };
    let java = std::env::var_os("CHUNKWAKE_JAVA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| props.java_command(dir));
    let plan = play.clone();
    let started = tokio::task::spawn_blocking(move || {
        chunkwake_bridge::Bridge::start(&java, &api, &plugins, &plan)
    })
    .await;
    match started {
        Ok(Ok((bridge, n))) => {
            play.set_hooks(bridge.clone());
            tracing::info!("enabled {n} plugin(s)");
            Some(bridge)
        }
        Ok(Err(e)) => {
            tracing::error!("plugins disabled: {e}");
            None
        }
        Err(e) => {
            tracing::error!("plugins disabled: {e}");
            None
        }
    }
}
