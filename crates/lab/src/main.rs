//! Isolated end-to-end and comparative runs against the same vanilla jar.

use chunkwake_bots::{Client, Event, Ids, status};
use chunkwake_data::packets::PacketTable;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, timeout};

mod tui;
mod progress;

const VERSION: &str = "26.3";
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const STEP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Smoke,
    Compare,
    Bench,
}

struct Args {
    mode: Mode,
    chunkwake: PathBuf,
    jar: PathBuf,
    java: PathBuf,
    work_dir: PathBuf,
    generated_cache: Option<PathBuf>,
    accept_eula: bool,
    keep_worlds: bool,
    bots: usize,
    warmup: Duration,
    duration: Duration,
    repetitions: usize,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut it = std::env::args().skip(1);
        let mode = match it.next().as_deref() {
            Some("smoke") => Mode::Smoke,
            Some("compare") => Mode::Compare,
            Some("bench") => Mode::Bench,
            Some("--help" | "-h" | "help") => {
                println!("{}", usage());
                std::process::exit(0);
            }
            None => return tui::run(),
            _ => return Err(usage().into()),
        };
        let mut chunkwake = None;
        let mut jar = None;
        let mut java = default_java();
        let mut work_dir = PathBuf::from("target/chunkwake-lab");
        let mut generated_cache = None;
        let (mut accept_eula, mut keep_worlds) = (false, false);
        let (mut bots, mut warmup, mut duration, mut repetitions) = (20, 10, 30, 3);
        while let Some(flag) = it.next() {
            match flag.as_str() {
                "--chunkwake" => {
                    chunkwake = Some(PathBuf::from(
                        it.next().ok_or("--chunkwake requires a path")?,
                    ))
                }
                "--jar" => jar = Some(PathBuf::from(it.next().ok_or("--jar requires a path")?)),
                "--java" => java = PathBuf::from(it.next().ok_or("--java requires a path")?),
                "--work-dir" => {
                    work_dir = PathBuf::from(it.next().ok_or("--work-dir requires a path")?)
                }
                "--generated-cache" => {
                    generated_cache = Some(PathBuf::from(
                        it.next().ok_or("--generated-cache requires a path")?,
                    ))
                }
                "--accept-vanilla-eula" => accept_eula = true,
                "--keep-worlds" => keep_worlds = true,
                "--bots" => {
                    bots = it
                        .next()
                        .ok_or("--bots requires a number")?
                        .parse()
                        .map_err(|_| "--bots requires a positive number")?
                }
                "--warmup" => {
                    warmup = it
                        .next()
                        .ok_or("--warmup requires seconds")?
                        .parse()
                        .map_err(|_| "--warmup requires seconds")?
                }
                "--duration" => {
                    duration = it
                        .next()
                        .ok_or("--duration requires seconds")?
                        .parse()
                        .map_err(|_| "--duration requires seconds")?
                }
                "--repetitions" => {
                    repetitions = it
                        .next()
                        .ok_or("--repetitions requires a number")?
                        .parse()
                        .map_err(|_| "--repetitions requires a positive number")?
                }
                _ => return Err(format!("unknown option {flag}\n{}", usage())),
            }
        }
        if bots == 0 || duration == 0 || repetitions == 0 || bots > 1000 || repetitions > 20 {
            return Err(
                "--bots, --duration and --repetitions must be positive and within safe limits"
                    .into(),
            );
        }
        let chunkwake = chunkwake.ok_or("--chunkwake is required")?;
        let jar = jar.unwrap_or_default();
        Ok(Self {
            mode,
            chunkwake,
            jar,
            java,
            work_dir,
            generated_cache,
            accept_eula,
            keep_worlds,
            bots,
            warmup: Duration::from_secs(warmup),
            duration: Duration::from_secs(duration),
            repetitions,
        })
    }
}

fn usage() -> &'static str {
    "chunkwake-lab [smoke|compare|bench] --chunkwake PATH [--jar PATH] [--java PATH] [--work-dir PATH]\n  [--generated-cache PATH] [--accept-vanilla-eula] [--keep-worlds]\n  [--bots N] [--warmup SEC] [--duration SEC] [--repetitions N]\n  Run with no arguments to open the TUI. Missing --jar downloads vanilla into the lab cache."
}

#[derive(Serialize, Clone)]
struct Case {
    id: String,
    value: Value,
    error: Option<String>,
}

#[derive(Serialize)]
struct Run {
    server: String,
    cases: Vec<Case>,
    startup_ms: u128,
    log: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    benchmark: Option<Benchmark>,
}

#[derive(Serialize, Clone)]
struct Benchmark {
    joined: usize,
    failed: usize,
    errors: Vec<String>,
    join_p50_ms: f64,
    join_p95_ms: f64,
    join_p99_ms: f64,
    packets_in: u64,
    bytes_in: u64,
    bytes_out: u64,
    cpu_seconds: Option<f64>,
    rss_peak_bytes: Option<u64>,
}

#[derive(Serialize)]
struct Difference {
    id: String,
    chunkwake: Value,
    vanilla: Value,
    expected: bool,
    reason: Option<String>,
}

#[derive(Serialize)]
struct Report {
    mode: String,
    version: String,
    jar: String,
    chunkwake: String,
    java: String,
    os: String,
    bots: usize,
    warmup_secs: u64,
    duration_secs: u64,
    repetitions: usize,
    runs: Vec<Run>,
    differences: Vec<Difference>,
    errors: Vec<String>,
}

#[tokio::main]
async fn main() {
    if let Err(error) = execute().await {
        if error == "cancelled" {
            return;
        }
        eprintln!("chunkwake-lab: {error}");
        std::process::exit(2);
    }
}

async fn execute() -> Result<(), String> {
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err(format!(".env: {error}"));
    }
    let mut args = Args::parse()?;
    let progress = progress::Progress::new(std::env::args_os().len() == 1)?;
    progress.update(None, "Preparing test workspace");
    if args.java.components().count() > 1 {
        args.java = ordinary_path(
            fs::canonicalize(&args.java)
                .map_err(|e| format!("--java {}: {e}", args.java.display()))?,
        );
    }
    let chunkwake = ordinary_path(
        fs::canonicalize(&args.chunkwake)
            .map_err(|e| format!("--chunkwake {}: {e}", args.chunkwake.display()))?,
    );
    fs::create_dir_all(&args.work_dir).map_err(|e| e.to_string())?;
    let consent = args.work_dir.join("vanilla-eula-accepted");
    if args.accept_eula {
        fs::write(&consent, b"accepted via --accept-vanilla-eula\n").map_err(|e| e.to_string())?;
    }
    if (args.mode != Mode::Smoke || args.jar.as_os_str().is_empty() || !args.jar.exists())
        && !consent.exists()
    {
        return Err("vanilla EULA not accepted; read the Minecraft EULA, then rerun with --accept-vanilla-eula".into());
    }
    let requested_jar = if args.jar.as_os_str().is_empty() {
        args.work_dir.join(format!("cache/{VERSION}/server.jar"))
    } else {
        args.jar.clone()
    };
    if !requested_jar.exists() {
        progress.update(None, "Downloading vanilla server.jar");
        download_jar(&requested_jar, VERSION).await?;
    } else {
        progress.update(None, "Using cached vanilla server.jar");
    }
    let jar = ordinary_path(
        fs::canonicalize(&requested_jar)
            .map_err(|e| format!("--jar {}: {e}", requested_jar.display()))?,
    );
    let generated_cache = args
        .generated_cache
        .clone()
        .unwrap_or_else(|| args.work_dir.join("cache/generated"));
    fs::create_dir_all(&generated_cache).map_err(|e| e.to_string())?;
    let generated_cache =
        ordinary_path(fs::canonicalize(&generated_cache).map_err(|e| e.to_string())?);
    let (version, protocol) = jar_version(&jar)?;
    if version != VERSION {
        return Err(format!(
            "--jar is Minecraft {version}; Chunkwake requires {VERSION}"
        ));
    }
    let needs_java = args.mode != Mode::Smoke
        || !generated_cache
            .join(VERSION)
            .join("reports/packets.json")
            .exists();
    if needs_java {
        progress.update(None, "Checking Java and Minecraft version");
        check_java(&args.java, &jar, &version)?;
    }
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let root = ordinary_path(fs::canonicalize(&args.work_dir).map_err(|e| e.to_string())?)
        .join(format!("{id}-{}", std::process::id()));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let count = if args.mode == Mode::Bench {
        args.repetitions
    } else {
        1
    };
    let mut report = Report {
        mode: format!("{:?}", args.mode).to_lowercase(),
        version,
        jar: jar.display().to_string(),
        chunkwake: chunkwake.display().to_string(),
        java: args.java.display().to_string(),
        os: std::env::consts::OS.into(),
        bots: args.bots,
        warmup_secs: args.warmup.as_secs(),
        duration_secs: args.duration.as_secs(),
        repetitions: count,
        runs: Vec::new(),
        differences: Vec::new(),
        errors: Vec::new(),
    };
    let expected: BTreeMap<String, String> =
        serde_json::from_str(include_str!("../expected-differences.json"))
            .map_err(|e| e.to_string())?;
    'runs: for iteration in 0..count {
        let order = if args.mode == Mode::Smoke {
            vec!["chunkwake"]
        } else if iteration % 2 == 0 {
            vec!["chunkwake", "vanilla"]
        } else {
            vec!["vanilla", "chunkwake"]
        };
        for server in order {
            progress.run(server, iteration + 1, count);
            let dir = root.join(format!("{iteration}-{server}"));
            let port = free_port()?;
            write_config(
                &dir,
                port,
                &jar,
                &generated_cache,
                server == "vanilla",
                args.bots,
            )?;
            let mut child = ManagedChild::start(server, &dir, &chunkwake, &jar, &args.java)?;
            progress.update(Some(server), "Starting server and waiting for readiness");
            let startup = child.ready(port, protocol, &generated_cache).await;
            let startup_failed = startup.is_err();
            let log_path = root.join(format!("{iteration}-{server}.log"));
            let mut run = Run {
                server: format!("{server}-{iteration}"),
                cases: Vec::new(),
                startup_ms: startup.as_ref().map(|d| d.as_millis()).unwrap_or(0),
                log: log_path.file_name().unwrap().to_string_lossy().into_owned(),
                benchmark: None,
            };
            match startup {
                Ok(_) => {
                    progress.update(Some(server), "Ready; running client workload");
                    let table_path = generated_cache.join(VERSION).join("reports/packets.json");
                    // First Chunkwake start generates the shared protocol table.
                    match PacketTable::load(&table_path)
                        .map_err(|e| e.to_string())
                        .and_then(|table| Ids::load(&table))
                    {
                        Ok(ids) => {
                            let addr = format!("127.0.0.1:{port}");
                            if args.mode == Mode::Bench {
                                match benchmark(&addr, ids, protocol, &args, &child, &progress, server).await {
                                    Ok(b) => run.benchmark = Some(b),
                                    Err(e) => report.errors.push(format!("{server}: {e}")),
                                }
                            } else {
                                progress.update(Some(server), "Checking status, login, movement and keep-alive");
                                run.cases = scenario(&addr, ids, protocol).await;
                            }
                        }
                        Err(e) => report.errors.push(format!("{server}: protocol table: {e}")),
                    }
                }
                Err(e) => report.errors.push(format!("{server}: startup: {e}")),
            }
            progress.update(Some(server), "Stopping server and saving world");
            if let Err(e) = child.stop().await {
                report.errors.push(format!("{server}: stop: {e}"));
            }
            let _ = fs::copy(dir.join("server.log"), &log_path);
            if args.mode != Mode::Bench && !run.cases.is_empty() {
                progress.update(Some(server), "Restarting to check saved player state");
                // A new process validates that player state survives the shutdown.
                let port = free_port()?;
                write_config(
                    &dir,
                    port,
                    &jar,
                    &generated_cache,
                    server == "vanilla",
                    args.bots,
                )?;
                let mut restarted =
                    ManagedChild::start(server, &dir, &chunkwake, &jar, &args.java)?;
                if let Err(e) = restarted.ready(port, protocol, &generated_cache).await {
                    run.cases.push(case_error("restart", e));
                } else {
                    let table = generated_cache.join(VERSION).join("reports/packets.json");
                    if let Ok(ids) = PacketTable::load(&table)
                        .map_err(|e| e.to_string())
                        .and_then(|t| Ids::load(&t))
                    {
                        let target = run
                            .cases
                            .iter()
                            .find(|c| c.id == "move_target")
                            .and_then(|c| serde_json::from_value::<[f64; 3]>(c.value.clone()).ok());
                        run.cases.push(
                            persistence(&format!("127.0.0.1:{port}"), ids, protocol, target).await,
                        );
                    }
                }
                if let Err(e) = restarted.stop().await {
                    report.errors.push(format!("{server}: restart stop: {e}"));
                }
                let _ = fs::copy(
                    dir.join("server.log"),
                    root.join(format!("{iteration}-{server}-restart.log")),
                );
            }
            progress.update(Some(server), if startup_failed { "Startup failed" } else { "Run complete" });
            report.runs.push(run);
            if startup_failed {
                break 'runs;
            }
        }
    }
    if args.mode == Mode::Compare {
        progress.update(None, "Comparing scenario results");
        report.differences = compare(&report.runs, &expected);
    }
    progress.update(None, "Writing report and cleaning up");
    write_report(&root, &report)?;
    if !args.keep_worlds {
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
    drop(progress);
    println!("Report: {}", root.join("report.md").display());
    let failed = !report.errors.is_empty()
        || report.runs.iter().any(|r| {
            r.cases.iter().any(|c| c.error.is_some())
                || r.benchmark
                    .as_ref()
                    .is_some_and(|b| b.failed > 0 || b.joined == 0)
        })
        || report.differences.iter().any(|d| !d.expected);
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

fn default_java() -> PathBuf {
    std::env::var_os("CHUNKWAKE_JAVA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("java"))
}

fn ordinary_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

fn jar_version(path: &Path) -> Result<(String, i32), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut text = String::new();
    zip.by_name("version.json")
        .map_err(|e| e.to_string())?
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok((
        value["name"]
            .as_str()
            .ok_or("jar version.json: missing name")?
            .into(),
        value["protocol_version"]
            .as_i64()
            .ok_or("jar version.json: missing protocol_version")? as i32,
    ))
}

fn jar_main_class_major(path: &Path) -> Result<u16, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut class = zip
        .by_name("net/minecraft/bundler/Main.class")
        .map_err(|e| format!("server.jar has no bundler main class: {e}"))?;
    let mut header = [0u8; 8];
    class.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..4] != b"\xca\xfe\xba\xbe" {
        return Err("invalid Java class header in server.jar".into());
    }
    Ok(u16::from_be_bytes([header[6], header[7]]))
}

fn java_feature(output: &str) -> Option<u16> {
    let version = output.split('"').nth(1)?;
    let version = version.strip_prefix("1.").unwrap_or(version);
    version
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

fn check_java(java: &Path, jar: &Path, game_version: &str) -> Result<(), String> {
    let major = jar_main_class_major(jar)?;
    let required = major.saturating_sub(44);
    let output = Command::new(java).arg("-version").output().map_err(|e| {
        format!(
            "Java {}: {e}; choose a Java installation in the TUI or pass --java PATH",
            java.display()
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "Java {} -version failed; choose a working Java installation with --java PATH",
            java.display()
        ));
    }
    let version_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let installed = java_feature(&version_text).ok_or_else(|| {
        format!(
            "could not read version of Java {}; output: {}",
            java.display(),
            version_text.trim()
        )
    })?;
    if installed < required {
        return Err(format!(
            "Java {installed} ({}) is too old for Minecraft {game_version}: server.jar requires Java {required} or newer (class version {major}). Choose another Java executable in the TUI, or pass --java PATH.",
            java.display()
        ));
    }
    Ok(())
}

async fn download_jar(path: &Path, version: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let manifest: Value = client
        .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await
        .map_err(|e| format!("Minecraft version manifest: {e}"))?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let url = manifest["versions"]
        .as_array()
        .and_then(|versions| versions.iter().find(|v| v["id"] == version))
        .and_then(|v| v["url"].as_str())
        .ok_or_else(|| format!("Minecraft {version} is not in the official version manifest"))?;
    official_url(url)?;
    let details: Value = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let server = &details["downloads"]["server"];
    let url = server["url"]
        .as_str()
        .ok_or("official manifest has no server jar URL")?;
    let sha1 = server["sha1"]
        .as_str()
        .ok_or("official manifest has no server jar hash")?;
    let size = server["size"]
        .as_u64()
        .ok_or("official manifest has no server jar size")?;
    official_url(url)?;
    println!(
        "Downloading Minecraft {version} server.jar from Mojang ({:.1} MiB)...",
        size as f64 / 1_048_576.0
    );
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let parent = path.parent().ok_or("invalid jar path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = path.with_extension("jar.part");
    let result = async {
        let mut file = File::create(&temp).map_err(|e| e.to_string())?;
        let mut digest = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            digest.update(&chunk);
            received += chunk.len() as u64;
            if received > size {
                return Err("server jar exceeds manifest size".to_owned());
            }
        }
        file.sync_all().map_err(|e| e.to_string())?;
        if received != size {
            return Err(format!("server jar truncated: {received} of {size} bytes"));
        }
        let actual = digest
            .finish()
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if !actual.eq_ignore_ascii_case(sha1) {
            return Err(format!(
                "server jar SHA-1 mismatch: expected {sha1}, got {actual}"
            ));
        }
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn official_url(raw: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some(
                "piston-meta.mojang.com"
                    | "piston-data.mojang.com"
                    | "launchermeta.mojang.com"
                    | "launcher.mojang.com"
            )
        )
    {
        return Err(format!(
            "unexpected download host in official manifest: {raw}"
        ));
    }
    Ok(())
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(listener.local_addr().map_err(|e| e.to_string())?.port())
}

fn write_config(
    dir: &Path,
    port: u16,
    jar: &Path,
    generated: &Path,
    vanilla: bool,
    bots: usize,
) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let base = format!(
        "server-ip=127.0.0.1\nserver-port={port}\nonline-mode=false\nwhite-list=false\nenforce-whitelist=false\nenable-status=true\nmotd=Chunkwake Lab\nmax-players={}\nnetwork-compression-threshold=256\nlevel-name=world\nlevel-seed=12345\nlevel-type=minecraft:flat\nview-distance=2\nsimulation-distance=2\nspawn-protection=0\n",
        bots.max(200) + 10
    );
    let text = if vanilla {
        base
    } else {
        format!(
            "{base}vanilla-jar={}\njava-path=java\ngenerated-dir={}\n",
            jar.display(),
            generated.display()
        )
    };
    fs::write(dir.join("server.properties"), text).map_err(|e| e.to_string())?;
    if vanilla {
        fs::write(dir.join("eula.txt"), b"eula=true\n").map_err(|e| e.to_string())?;
    }
    Ok(())
}

struct ManagedChild {
    child: Child,
    dir: PathBuf,
    server: String,
    stopped: bool,
}

impl ManagedChild {
    fn start(
        server: &str,
        dir: &Path,
        chunkwake: &Path,
        jar: &Path,
        java: &Path,
    ) -> Result<Self, String> {
        let log = File::create(dir.join("server.log")).map_err(|e| e.to_string())?;
        let error_log = log.try_clone().map_err(|e| e.to_string())?;
        let mut command = if server == "vanilla" {
            let mut c = Command::new(java);
            c.arg("-Xms512M")
                .arg("-Xmx1G")
                .arg("-jar")
                .arg(jar)
                .arg("nogui");
            c
        } else {
            let mut c = Command::new(chunkwake);
            c.arg("--dir").arg(dir);
            c.env("CHUNKWAKE_JAVA", java);
            c
        };
        command
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(error_log));
        let temp = dir.join("tmp");
        fs::create_dir_all(&temp).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            command.env("TEMP", &temp).env("TMP", &temp);
        }
        #[cfg(unix)]
        {
            command.env("TMPDIR", &temp);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command
            .spawn()
            .map_err(|e| format!("start {server}: {e}"))?;
        Ok(Self {
            child,
            dir: dir.to_owned(),
            server: server.into(),
            stopped: false,
        })
    }

    async fn ready(
        &mut self,
        port: u16,
        protocol: i32,
        generated_cache: &Path,
    ) -> Result<Duration, String> {
        let started = Instant::now();
        let addr = format!("127.0.0.1:{port}");
        while started.elapsed() < READY_TIMEOUT {
            if let Some(exit) = self.child.try_wait().map_err(|e| e.to_string())? {
                let detail = log_cause(&self.dir.join("server.log"))
                    .map(|cause| format!(": {cause}"))
                    .unwrap_or_default();
                return Err(format!(
                    "exited before ready ({exit}){detail}; see linked log"
                ));
            }
            let generated = generated_cache.join(VERSION).join("reports/packets.json");
            if timeout(Duration::from_secs(2), status(&addr, protocol))
                .await
                .is_ok_and(|r| r.is_ok())
                && (self.server == "vanilla" || generated.exists())
            {
                return Ok(started.elapsed());
            }
            sleep(Duration::from_millis(250)).await;
        }
        Err(format!(
            "readiness timed out; see {}",
            self.dir.join("server.log").display()
        ))
    }

    async fn stop(&mut self) -> Result<(), String> {
        if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
            self.stopped = true;
            return Ok(());
        }
        if let Some(stdin) = &mut self.child.stdin {
            stdin.write_all(b"stop\n").map_err(|e| e.to_string())?;
            stdin.flush().map_err(|e| e.to_string())?;
        }
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(30) {
            if let Some(exit) = self.child.try_wait().map_err(|e| e.to_string())? {
                self.stopped = true;
                return if exit.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "exit {exit}; see {}",
                        self.dir.join("server.log").display()
                    ))
                };
            }
            sleep(Duration::from_millis(100)).await;
        }
        self.force_kill();
        Err("shutdown timed out after 30 seconds".into())
    }

    fn force_kill(&mut self) {
        if self.stopped {
            return;
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/T", "/F", "/PID", &self.child.id().to_string()])
                .output();
        }
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{}", self.child.id())])
                .output();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stopped = true;
    }
}

fn log_cause(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .rev()
        .find(|line| {
            line.contains("Caused by:") || line.starts_with("error:") || line.contains("/ERROR]")
        })
        .map(str::trim)
        .map(str::to_owned)
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.force_kill();
    }
}

fn case(id: &str, value: Value) -> Case {
    Case {
        id: id.into(),
        value,
        error: None,
    }
}
fn case_error(id: &str, error: String) -> Case {
    Case {
        id: id.into(),
        value: Value::Null,
        error: Some(error),
    }
}

async fn scenario(addr: &str, ids: Ids, protocol: i32) -> Vec<Case> {
    let mut cases = Vec::new();
    match timeout(STEP_TIMEOUT, status(addr, protocol)).await {
        Ok(Ok(response)) => {
            cases.push(case(
                "status_protocol",
                json!(response["version"]["protocol"]),
            ));
            cases.push(case("status_motd", json!(response["description"])));
            cases.push(case(
                "status_max_players",
                json!(response["players"]["max"]),
            ));
            if response["version"]["protocol"] != protocol {
                cases.push(case_error("status", "wrong protocol".into()));
            }
        }
        Ok(Err(e)) => cases.push(case_error("status", e)),
        Err(_) => cases.push(case_error("status", "timed out".into())),
    }
    match timeout(
        STEP_TIMEOUT,
        Client::connect(addr, "LabPlayer", ids, protocol, VERSION),
    )
    .await
    {
        Ok(Ok(mut client)) => {
            cases.push(case("offline_login", json!(true)));
            let initial = timeout(STEP_TIMEOUT, async {
                loop {
                    if let Event::Position(pos) = client.next_event().await? {
                        return Ok::<_, String>(pos);
                    }
                }
            })
            .await;
            match initial {
                Ok(Ok(pos)) => {
                    cases.push(case("play_position", json!(true)));
                    let target = [pos[0] + 1.0, pos[1], pos[2] + 1.0];
                    match client.move_to(target).await {
                        Ok(()) => cases.push(case("move_target", json!(target))),
                        Err(e) => cases.push(case_error("movement", e)),
                    }
                    let keep_alive = timeout(Duration::from_secs(25), async {
                        loop {
                            if client.next_event().await? == Event::KeepAlive {
                                return Ok::<_, String>(());
                            }
                        }
                    })
                    .await;
                    match keep_alive {
                        Ok(Ok(())) => cases.push(case("keep_alive", json!(true))),
                        Ok(Err(e)) => cases.push(case_error("keep_alive", e)),
                        Err(_) => cases.push(case_error(
                            "keep_alive",
                            "no keep-alive within 25 seconds".into(),
                        )),
                    }
                }
                Ok(Err(e)) => cases.push(case_error("play_position", e)),
                Err(_) => cases.push(case_error("play_position", "timed out".into())),
            }
            drop(client);
            sleep(Duration::from_millis(500)).await;
        }
        Ok(Err(e)) => cases.push(case_error("offline_login", e)),
        Err(_) => cases.push(case_error("offline_login", "timed out".into())),
    }
    cases
}

async fn persistence(addr: &str, ids: Ids, protocol: i32, target: Option<[f64; 3]>) -> Case {
    let Some(target) = target else {
        return case_error("position_persisted", "movement was not completed".into());
    };
    let result = timeout(STEP_TIMEOUT, async {
        let mut client = Client::connect(addr, "LabPlayer", ids, protocol, VERSION).await?;
        loop {
            if let Event::Position(pos) = client.next_event().await? {
                let close = pos.iter().zip(target).all(|(a, b)| (a - b).abs() < 0.75);
                return Ok::<_, String>(close);
            }
        }
    })
    .await;
    match result {
        Ok(Ok(true)) => case("position_persisted", json!(true)),
        Ok(Ok(false)) => case_error(
            "position_persisted",
            "rejoined at a different position".into(),
        ),
        Ok(Err(e)) => case_error("position_persisted", e),
        Err(_) => case_error("position_persisted", "timed out".into()),
    }
}

fn compare(runs: &[Run], expected: &BTreeMap<String, String>) -> Vec<Difference> {
    if runs.len() < 2 {
        return Vec::new();
    }
    let ours = &runs[0].cases;
    let vanilla = &runs[1].cases;
    ours.iter()
        .filter_map(|a| {
            let b = vanilla.iter().find(|b| b.id == a.id)?;
            if a.id == "move_target" || a.value == b.value {
                return None;
            }
            let reason = expected.get(&a.id).cloned();
            Some(Difference {
                id: a.id.clone(),
                chunkwake: a.value.clone(),
                vanilla: b.value.clone(),
                expected: reason.is_some(),
                reason,
            })
        })
        .collect()
}

fn write_report(root: &Path, report: &Report) -> Result<(), String> {
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut md = format!(
        "# Chunkwake Lab · {} · Minecraft {}\n\nHost: {} · Java: {} · Bots: {} · Warmup: {} s · Duration: {} s · Repetitions: {}\n\n",
        report.mode,
        report.version,
        report.os,
        report.java,
        report.bots,
        report.warmup_secs,
        report.duration_secs,
        report.repetitions
    );
    for run in &report.runs {
        md += &format!(
            "## {}\n\nStartup: {} ms · [log]({})\n\n",
            run.server, run.startup_ms, run.log
        );
        for c in &run.cases {
            md += &format!(
                "- {}: {}{}\n",
                c.id,
                c.value,
                c.error
                    .as_ref()
                    .map(|e| format!(" — ERROR: {e}"))
                    .unwrap_or_default()
            );
        }
        if let Some(b) = &run.benchmark {
            md += &format!(
                "\nJoined: {} · Failed: {} · Join p50/p95/p99: {:.1}/{:.1}/{:.1} ms · RX payload {} bytes · TX payload {} bytes · CPU {:?} s · peak RSS {:?} bytes\n",
                b.joined,
                b.failed,
                b.join_p50_ms,
                b.join_p95_ms,
                b.join_p99_ms,
                b.bytes_in,
                b.bytes_out,
                b.cpu_seconds,
                b.rss_peak_bytes
            );
            for error in &b.errors {
                md += &format!("- Bot error: {error}\n");
            }
        }
        md += "\n";
    }
    for d in &report.differences {
        md += &format!(
            "- Difference {}: Chunkwake {} / vanilla {}{}\n",
            d.id,
            d.chunkwake,
            d.vanilla,
            d.reason
                .as_ref()
                .map(|r| format!(" · known: {r}"))
                .unwrap_or_default()
        );
    }
    for e in &report.errors {
        md += &format!("- Error: {e}\n");
    }
    fs::write(root.join("report.md"), md).map_err(|e| e.to_string())
}

#[derive(Default)]
struct BotResult {
    join_ms: f64,
    packets: u64,
    bytes_in: u64,
    bytes_out: u64,
}

async fn benchmark(
    addr: &str,
    ids: Ids,
    protocol: i32,
    args: &Args,
    child: &ManagedChild,
    progress: &progress::Progress,
    server: &str,
) -> Result<Benchmark, String> {
    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..args.bots {
        let addr = addr.to_owned();
        let warmup = args.warmup;
        let duration = args.duration;
        tasks.spawn(async move {
            let mut client = timeout(STEP_TIMEOUT, Client::connect(&addr, &format!("lab{i}"), ids, protocol, VERSION)).await.map_err(|_| "login timeout".to_owned())??;
            let join_ms = client.join_time.as_secs_f64() * 1000.0;
            let origin = timeout(STEP_TIMEOUT, async {
                loop { if let Event::Position(pos) = client.next_event().await? { return Ok::<_, String>(pos); } }
            }).await.map_err(|_| "play position timeout".to_owned())??;
            let started = Instant::now();
            // Vanilla 26.3 rejects more than one position packet in a server tick.
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            tick.tick().await;
            let mut angle = i as f64;
            let mut base = None;
            while started.elapsed() < warmup + duration {
                if started.elapsed() >= warmup && base.is_none() { base = Some((client.packets_in, client.bytes_in, client.bytes_out)); }
                tokio::select! {
                    _ = tick.tick() => {
                        angle += 0.05;
                        client.move_to([origin[0] + 2.0 * angle.cos(), origin[1], origin[2] + 2.0 * angle.sin()]).await?;
                    }
                    event = client.next_event() => { event?; }
                }
            }
            let (p, rx, tx) = base.unwrap_or((0, 0, 0));
            Ok::<BotResult, String>(BotResult { join_ms, packets: client.packets_in - p, bytes_in: client.bytes_in - rx, bytes_out: client.bytes_out - tx })
        });
        sleep(Duration::from_millis(5)).await;
    }
    sleep(args.warmup).await;
    let before = process_stats(child.child.id());
    let mut peak = before.map(|(_, rss)| rss);
    let mut results = Vec::new();
    let mut failed = 0;
    let mut errors = Vec::new();
    let deadline = Instant::now() + args.warmup + args.duration + STEP_TIMEOUT;
    let measurement_started = Instant::now();
    while !tasks.is_empty() && Instant::now() < deadline {
        tokio::select! {
            result = tasks.join_next() => {
                match result {
                    Some(Ok(Ok(data))) => results.push(data),
                    Some(Ok(Err(error))) => { failed += 1; if errors.len() < 5 { errors.push(error); } }
                    Some(Err(error)) => { failed += 1; if errors.len() < 5 { errors.push(error.to_string()); } }
                    None => break,
                }
            }
            _ = sleep(Duration::from_secs(1)) => {
                if let Some((_, rss)) = process_stats(child.child.id()) { peak = Some(peak.unwrap_or(0).max(rss)); }
                let elapsed = measurement_started.elapsed();
                let phase = if elapsed < args.warmup { "Warmup" } else { "Measuring" };
                let phase_elapsed = if elapsed < args.warmup { elapsed } else { elapsed.saturating_sub(args.warmup) };
                let phase_total = if elapsed < args.warmup { args.warmup } else { args.duration };
                progress.update(Some(server), &format!("{phase} {} / {} s · {} / {} bots finished · {} failed", phase_elapsed.as_secs().min(phase_total.as_secs()), phase_total.as_secs(), results.len() + failed, args.bots, failed));
            }
        }
    }
    failed += tasks.len();
    tasks.abort_all();
    let after = process_stats(child.child.id());
    let mut times = results.iter().map(|r| r.join_ms).collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    let percentile = |p: f64| -> f64 {
        if times.is_empty() {
            0.0
        } else {
            times[((times.len() - 1) as f64 * p).round() as usize]
        }
    };
    Ok(Benchmark {
        joined: results.len(),
        failed,
        errors,
        join_p50_ms: percentile(0.50),
        join_p95_ms: percentile(0.95),
        join_p99_ms: percentile(0.99),
        packets_in: results.iter().map(|r| r.packets).sum(),
        bytes_in: results.iter().map(|r| r.bytes_in).sum(),
        bytes_out: results.iter().map(|r| r.bytes_out).sum(),
        cpu_seconds: before.zip(after).map(|(a, b)| (b.0 - a.0).max(0.0)),
        rss_peak_bytes: peak,
    })
}

#[cfg(windows)]
fn process_stats(pid: u32) -> Option<(f64, u64)> {
    let script = format!(
        "$p=Get-Process -Id {pid} -ErrorAction Stop; @{{cpu=$p.CPU;rss=$p.WorkingSet64}} | ConvertTo-Json -Compress"
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    Some((value["cpu"].as_f64()?, value["rss"].as_u64()?))
}

#[cfg(unix)]
fn process_stats(pid: u32) -> Option<(f64, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let ticks: u64 = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    let hz: f64 = String::from_utf8(Command::new("getconf").arg("CLK_TCK").output().ok()?.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rss_kb: u64 = status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some((ticks as f64 / hz, rss_kb * 1024))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn official_manifest_hosts_only() {
        assert!(official_url("https://piston-meta.mojang.com/v1/version.json").is_ok());
        assert!(official_url("https://piston-data.mojang.com/v1/objects/server.jar").is_ok());
        assert!(official_url("http://piston-data.mojang.com/server.jar").is_err());
        assert!(official_url("https://piston-data.mojang.com.evil.test/server.jar").is_err());
    }

    #[test]
    fn reads_jar_version() {
        let path = std::env::temp_dir().join(format!(
            "lab-jar-{}-{}.jar",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("version.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"{"name":"26.3","protocol_version":777}"#)
            .unwrap();
        zip.start_file(
            "net/minecraft/bundler/Main.class",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"\xca\xfe\xba\xbe\x00\x00\x00\x45").unwrap();
        zip.finish().unwrap();
        assert_eq!(jar_version(&path).unwrap(), ("26.3".into(), 777));
        assert_eq!(jar_main_class_major(&path).unwrap(), 69);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn parses_java_version_output() {
        assert_eq!(
            java_feature("openjdk version \"21.0.11\" 2026-04-21"),
            Some(21)
        );
        assert_eq!(java_feature("openjdk version \"25.0.1\""), Some(25));
        assert_eq!(java_feature("java version \"1.8.0_451\""), Some(8));
        assert_eq!(java_feature("garbage"), None);
    }

    #[test]
    fn known_difference_is_visible_but_expected() {
        let runs = vec![
            Run {
                server: "chunkwake".into(),
                cases: vec![case("status", json!("a"))],
                startup_ms: 0,
                log: String::new(),
                benchmark: None,
            },
            Run {
                server: "vanilla".into(),
                cases: vec![case("status", json!("b"))],
                startup_ms: 0,
                log: String::new(),
                benchmark: None,
            },
        ];
        let unknown = compare(&runs, &BTreeMap::new());
        assert_eq!(unknown.len(), 1);
        assert!(!unknown[0].expected);
        let known = compare(
            &runs,
            &BTreeMap::from([("status".into(), "tracked gap".into())]),
        );
        assert!(known[0].expected);
        assert_eq!(known[0].reason.as_deref(), Some("tracked gap"));
    }
}
