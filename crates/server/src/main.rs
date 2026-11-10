//! Точка входа сервера Chunkwake.

mod backup;
mod cli;
mod config;
mod players;
mod setup_tui;
mod storage;
mod world_setup;

use chunkwake_net::Server;
use chunkwake_server::MC_VERSION;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use tokio::net::TcpListener;

fn main() {
    // Локальные переменные окружения (например, CHUNKWAKE_JAVA) из .env в текущем
    // каталоге или выше. Переменные, уже заданные в окружении, не перезаписываются.
    //
    // Случай, который стоил отладки: `.env` в корне репозитория задаёт
    // CHUNKWAKE_JAVA, и запуск сервера из подкаталога молча подхватывал его —
    // `doctor` проверял не ту Java, что стоит в `java-path`. Поэтому сначала
    // ищем `.env` в каталоге сервера (он передаётся `--dir`), и только потом
    // поднимаемся выше.
    if let Err(error) = dotenv_in_server_dir() {
        eprintln!("warning: .env: {error}");
    }
    let dir = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[0] == "--dir")
        .map_or_else(
            || std::path::PathBuf::from("."),
            |pair| std::path::PathBuf::from(&pair[1]),
        );
    let (requested, view_distance) = config::Properties::load(&dir.join("server.properties"))
        .map_or((config::MemoryProfile::Auto, 8), |p| {
            (p.memory_profile, p.view_distance)
        });
    let budget = config::MemoryBudget::resolve(requested, view_distance);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(budget.tokio_workers)
        .max_blocking_threads(budget.blocking_threads)
        .enable_all()
        .build()
        .expect("build Tokio runtime");
    if let Err(error) = runtime.block_on(cli::execute()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

/// `.env` из каталога сервера, затем обычный поиск вверх по дереву.
///
/// Отсутствие файла — обычный случай, не повод для предупреждения.
fn dotenv_in_server_dir() -> Result<(), dotenvy::Error> {
    let dir = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[0] == "--dir")
        .map(|pair| std::path::PathBuf::from(&pair[1]))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let local = dir.join(".env");
    let result = if local.is_file() {
        dotenvy::from_path(&local)
    } else {
        dotenvy::dotenv().map(|_| ())
    };
    match result {
        Ok(()) => Ok(()),
        // Нет файла — не ошибка: переменные берутся из окружения.
        Err(error) if error.not_found() => Ok(()),
        Err(error) => Err(error),
    }
}

/// Логи: консоль и `logs/latest.log` рядом с `server.properties`.
///
/// Служба запускает сервер без перенаправления вывода, поэтому лог нужен
/// файлом — иначе от запуска не остаётся ничего. Файл пишется неблокирующе
/// (отдельный поток `tracing-appender`): диск не должен тормозить тик.
///
/// Не удалось создать каталог или файл — работаем только с консолью. Это
/// важнее, чем лог: read-only установка не должна из-за него не стартовать.
fn init_logging(dir: &Path, level: tracing::level_filters::LevelFilter) -> Result<(), String> {
    let logs = dir.join("logs");
    let fallback = || {
        tracing_subscriber::fmt()
            .with_timer(LogTime)
            .with_target(false)
            .with_max_level(level)
            .init();
    };
    if let Err(error) = std::fs::create_dir_all(&logs) {
        fallback();
        tracing::warn!("logs: {}: {error}; logging to console only", logs.display());
        return Ok(());
    }
    // Суточные файлы (`chunkwake.2026-11-01.log`) не дают логу расти без
    // предела; `latest.log` — то, что читают в инструкциях и в службе.
    // Симлинк не используем: в Windows он требует особых прав.
    let daily = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("chunkwake")
        .filename_suffix("log")
        .max_log_files(14)
        .build(&logs);
    let daily = match daily {
        Ok(daily) => daily,
        Err(error) => {
            fallback();
            tracing::warn!("logs: {error}; logging to console only");
            return Ok(());
        }
    };
    let latest = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join("latest.log"))
    {
        Ok(file) => file,
        Err(error) => {
            fallback();
            tracing::warn!("logs/latest.log: {error}; logging to console only");
            return Ok(());
        }
    };
    let (file, guard) = tracing_appender::non_blocking(daily);
    // Страж живёт до конца процесса: на его сбросе буфер дописывается на диск.
    std::mem::forget(guard);
    let latest = std::sync::Arc::new(std::sync::Mutex::new(latest));
    tracing_subscriber::fmt()
        .with_timer(LogTime)
        .with_target(false)
        .with_max_level(level)
        .with_writer(move || Tee {
            file: file.clone(),
            latest: latest.clone(),
            console: std::io::stdout(),
        })
        .init();
    Ok(())
}

/// Пишет строку в консоль, суточный файл и `latest.log`: смотреть лог в
/// терминале по-прежнему нужно, а службе нужен файл с постоянным именем.
struct Tee {
    file: tracing_appender::non_blocking::NonBlocking,
    latest: std::sync::Arc<std::sync::Mutex<std::fs::File>>,
    console: std::io::Stdout,
}

impl std::io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Ошибки файлов глотаем: без лога сервер должен продолжать работать.
        let _ = std::io::Write::write_all(&mut self.file, buf);
        if let Ok(mut latest) = self.latest.lock() {
            let _ = latest.write_all(buf);
        }
        std::io::Write::write_all(&mut self.console, buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::Write::flush(&mut self.file);
        if let Ok(mut latest) = self.latest.lock() {
            let _ = latest.flush();
        }
        std::io::Write::flush(&mut self.console)
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
    let budget = config::MemoryBudget::resolve(props.memory_profile, props.view_distance);
    chunkwake_bridge::set_jvm_max_heap(budget.jvm_xmx);
    init_logging(dir, props.log_level)?;
    tracing::info!(
        "memory profile: {:?}, gen workers {}, Tokio workers {}, blocking {}, proto limit {}, base columns {}",
        budget.profile,
        budget.gen_workers,
        budget.tokio_workers,
        budget.blocking_threads,
        budget.proto_limit,
        budget.base_column_limit
    );
    tracing::info!("Minecraft {} (protocol {})", version.name, version.protocol);
    let boot_started = std::time::Instant::now();

    // Данные ванилы встраиваются в бинарь при сборке и распаковываются один
    // раз; дальше ни jar, ни Java не нужны. Если в этой сборке встроенных
    // данных нет, остаётся прежний путь: кэш, а без него генератор из jar.
    let cache_root = props.generated_path(dir);
    let data_dir = vanilla_data(&props, &version, dir, &cache_root)?;
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
            .map_err(|e| e.to_string())?
            .with_data_dir(&data_dir);
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
    // Измерения, объявленные клиенту, сразу получают свои каталоги регионов.
    for (dimension, template, min_y) in play.dimension_layouts() {
        storage.enable_dimension_with(dimension.key(), template, min_y);
    }
    // Регионы читаются с диска при первом обращении к их чанкам.
    play.set_source(storage.clone());
    play.set_player_source(storage.clone());
    play.set_maps_dir(dir.join(&props.level_name).join("data/minecraft/maps"));
    // Генерация чанков — на своих потоках, чтобы тик не ждал счёта.
    play.set_self_weak();
    play.set_gen_pool(chunkwake_net::chunkgen::GenPool::new(budget.gen_workers));
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
    // Старые миры писались без `Heightmaps`, и BlueMap не рисовал их чанки.
    // Дописываем раздел до включения плагинов, иначе карта снова пустая.
    // Ошибку не считаем смертельной: мир без heightmaps хуже, чем не
    // запустившийся сервер, но и падать из-за чтения регионов незачем.
    match storage.backfill_heightmaps() {
        Ok(0) => {}
        Ok(n) => {
            tracing::info!("heightmaps: добавлены в {n} чанков (для BlueMap и редакторов карт)")
        }
        Err(error) => tracing::warn!("heightmaps: {error}; внешние карты могут не видеть рельеф"),
    }
    let autosave_task = storage.spawn_autosave(play.clone());
    let unload_task = storage.spawn_unloader(play.clone());
    if std::env::var_os("CHUNKWAKE_MEMSTATS").is_some() {
        #[cfg(feature = "alloc-count")]
        chunkwake_server::alloc_count::init_trace();
        let plan = play.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(10));
                tracing::info!("memstats world: {}", plan.mem_stats());
                #[cfg(feature = "alloc-count")]
                tracing::info!(
                    "memstats heap: {}{}",
                    chunkwake_server::alloc_count::report(),
                    chunkwake_server::alloc_count::trace_report()
                );
                if std::env::var_os("CHUNKWAKE_MEMSTATS_MI").is_some() {
                    // SAFETY: функции mimalloc без аргументов-указателей кроме null.
                    unsafe {
                        libmimalloc_sys::mi_collect(true);
                        libmimalloc_sys::mi_stats_print_out(None, std::ptr::null_mut());
                    }
                }
            }
        });
    }
    let bridge = start_plugins(dir, &props, &play).await;

    let mut net = props.net_config(version);
    net.configuration = Some(std::sync::Arc::new(plan));
    let gate = play.clone();
    net.admit = Some(chunkwake_net::Admit(std::sync::Arc::new(
        move |name, uuid, addr, properties, full| {
            gate.admission_denied(name, uuid, addr, properties, full)
        },
    )));
    let server = Server::new(net);
    if let Some(translator) = bridge.as_ref().and_then(|b| b.translator()) {
        server.set_translator(translator);
        tracing::info!("ViaVersion: clients of other versions are translated");
    }
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
    // Ctrl+C, SIGTERM или `stop`: завершить соединения, плагины и сохранить мир.
    let terminate = terminate_signal();
    tokio::pin!(terminate);
    loop {
        tokio::select! {
            result = &mut serving => {
                serve_error = result.err().map(|e| e.to_string());
                serving_done = true;
                break;
            },
            _ = tokio::signal::ctrl_c() => break,
            _ = &mut terminate => break,
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
    unload_task.abort();
    let _ = tick_task.await;
    let _ = autosave_task.await;
    let _ = unload_task.await;
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

/// Каталог ванильных данных: встроенный архив, готовый кэш или генератор из jar.
///
/// Порядок такой, что обычный запуск релизной сборки не трогает ни диск
/// (после первой распаковки), ни Java: архив внутри бинаря распаковывается в
/// `generated/<версия>` и дальше читается как обычный кэш. Сборка без
/// встроенных данных (dev без `generated/`, CI) идёт прежним путём через jar.
fn vanilla_data(
    props: &config::Properties,
    version: &chunkwake_net::GameVersion,
    dir: &Path,
    cache_root: &Path,
) -> Result<std::path::PathBuf, String> {
    if chunkwake_data::bundle::present() {
        let started = std::time::Instant::now();
        let ready_before = chunkwake_data::bundle::extracted(&cache_root.join(&version.name));
        if !ready_before {
            tracing::info!(
                "unpacking vanilla data from the binary into {} (first start only)",
                cache_root.join(&version.name).display()
            );
        }
        let data =
            chunkwake_data::bundle::ensure(cache_root, &version.name).map_err(|e| e.to_string())?;
        if ready_before {
            tracing::debug!(
                "vanilla data: embedded bundle ready in {} ms ({})",
                started.elapsed().as_millis(),
                data.display()
            );
        } else {
            tracing::info!(
                "vanilla data ready in {} ({} ms)",
                data.display(),
                started.elapsed().as_millis()
            );
        }
        return Ok(data);
    }
    if let Some(data) = chunkwake_data::cache::cached(cache_root, &version.name)
        .filter(|d| config::read_cached_version(d).is_some())
    {
        // Кэш мог остаться от сборки с бандлом: шаблоны и язык уже внутри.
        return Ok(data);
    }
    let java = std::env::var_os("CHUNKWAKE_JAVA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| props.java_command(dir));
    let Some(jar) = props.jar_path(dir) else {
        return Err(format!(
            "no vanilla data: this build has none embedded and vanilla-jar is empty; \
             put server.jar in {} or run `chunkwake config set vanilla-jar server.jar`",
            dir.display()
        ));
    };
    tracing::info!(
        "extracting vanilla data from server.jar with {} (one-time, usually 10-30s)",
        java.display()
    );
    let data = chunkwake_data::cache::ensure_generated_with(
        &java,
        &jar,
        &version.name,
        cache_root,
        &|msg| tracing::info!("{msg}"),
    )
    .map_err(|e| e.to_string())?;
    config::cache_version(&jar, &data)?;
    tracing::info!(
        "vanilla data cached in {}; server.jar is no longer needed",
        data.display()
    );
    Ok(data)
}

/// Запускает сэмплер процесса и HTTP-эндпоинт метрик.
///
/// Сэмплер опрашивает `sysinfo` раз в секунду: загрузку CPU он считает как
/// разницу между опросами, поэтому делать это из команды `/top` нельзя.
/// Снимок и история в памяти общие для `/metrics`, `/snapshot`, `/api/events`
/// и `chunkwake top`; каждая публикация будит подписчиков SSE, поэтому
/// дашборд в браузере не опрашивает сервер сам.
fn start_metrics(
    props: &config::Properties,
    play: &std::sync::Arc<chunkwake_net::play::PlayPlan>,
) -> Option<chunkwake_metrics::http::Shared> {
    if !props.metrics_enabled {
        tracing::info!("metrics disabled (metrics-enabled=false)");
        return None;
    }
    let live: chunkwake_metrics::http::Shared =
        std::sync::Arc::new(std::sync::RwLock::new(chunkwake_metrics::Live::new()));
    // Профиль по фазам приходит из самого тика, а не из сэмплера: только тик
    // знает, сколько заняла каждая подсистема, и только он попадает в секунду
    // переменное число раз.
    {
        let live_profile = live.clone();
        play.set_profile_sink(std::sync::Arc::new(
            move |at: u64, marks: &[f32; chunkwake_metrics::PHASE_COUNT]| {
                if let Ok(mut slot) = live_profile.write() {
                    slot.publish_profile(at, marks);
                }
            },
        ));
    }
    let play_sampler = play.clone();
    let live_sampler = live.clone();
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
            if let Ok(mut slot) = live_sampler.write() {
                slot.publish(fresh);
            }
        }
    });
    let ip: std::net::IpAddr = props
        .metrics_ip
        .parse()
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let port = props.metrics_port;
    let live_server = live.clone();
    let access = if props.metrics_token.is_empty() {
        chunkwake_metrics::http::Access::local()
    } else {
        chunkwake_metrics::http::Access::with_token(props.metrics_token.clone())
    };
    tokio::spawn(async move {
        match chunkwake_metrics::http::spawn(ip, port, live_server, access).await {
            // Ссылка на дашборд — первое, что нужно, когда открываешь сервер.
            Ok(bound) if bound == port => {
                tracing::info!("dashboard on http://{ip}:{bound}/ (metrics on /metrics)")
            }
            // Порт был занят: говорим явно, иначе `chunkwake top` без флагов
            // будет искать сервер на «правильном» порту и не найдёт.
            Ok(bound) => tracing::warn!(
                "dashboard on http://{ip}:{bound}/ (port {port} is busy; \
                 `chunkwake top --port {bound}` or update metrics-port)"
            ),
            Err(error) => tracing::error!("metrics disabled: {error}"),
        }
    });
    Some(live)
}

fn log_player_exit(name: &str, result: Result<(), chunkwake_net::NetError>) {
    match result {
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
    let Some(api) = chunkwake_bridge::find_api_dir(&props.api_path(dir)) else {
        tracing::warn!(
            "plugin API not found (build it with `java/gradlew -p java dist`); plugins disabled"
        );
        return None;
    };
    let java = std::env::var_os("CHUNKWAKE_JAVA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| props.java_command(dir));
    let plan = play.clone();
    let world_dir = std::path::absolute(dir.join(&props.level_name))
        .unwrap_or_else(|_| dir.join(&props.level_name));
    // Jar игры для classpath JVM: плагины под Paper зовут `net.minecraft.*`,
    // а эти классы есть только внутри ванильного jar. Берём `vanilla-jar`,
    // если задан, иначе стандартный `server.jar` рядом с `server.properties`.
    let game_jar = props
        .jar_path(dir)
        .filter(|p| p.is_file())
        .or_else(|| Some(dir.join("server.jar")).filter(|p| p.is_file()))
        // Абсолютный путь: `Bridge` работает в потоке JVM, где рабочий каталог
        // может отличаться, а относительный `.\server.jar` оттуда не откроется.
        .and_then(|p| std::path::absolute(&p).ok().or(Some(p)));
    tracing::debug!("plugins: game jar = {game_jar:?}");
    // JVM поднимается и плагины включаются синхронно, до первого тика: всё это
    // время сервер не отвечает. Без сообщения это неотличимо от зависания,
    // поэтому пишем и «начали», и «заняло столько».
    tracing::info!(
        "plugins: starting the JVM ({}) and enabling plugins...",
        java.display()
    );
    let waiting = std::time::Instant::now();
    let started = tokio::task::spawn_blocking(move || {
        chunkwake_bridge::Bridge::start(
            &java,
            &api,
            &plugins,
            &world_dir,
            game_jar.as_deref(),
            &plan,
        )
    })
    .await;
    match started {
        Ok(Ok((bridge, n))) => {
            play.set_hooks(bridge.clone());
            tracing::info!(
                "plugins: enabled {n} plugin(s) in {} ms",
                waiting.elapsed().as_millis()
            );
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

/// Ждёт SIGTERM: так сервер останавливают systemd, Docker и `kill`.
/// Без перехвата процесс умирал сразу, не сохранив мир и игроков.
/// На Windows такого сигнала нет: там остановка идёт через Ctrl+C и `stop`.
async fn terminate_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "SIGTERM handler is unavailable");
                std::future::pending::<()>().await;
            }
        }
    }
    #[cfg(not(unix))]
    std::future::pending::<()>().await;
}
