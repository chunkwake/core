//! Команды первоначальной настройки и диагностики.

use crate::backup;
use crate::config::{self, Properties};
use crate::{MC_VERSION, serve, setup_tui};
use chunkwake_net::GameVersion;
use std::io::{self, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub key: &'static str,
    pub message: String,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run,
    Setup,
    Doctor,
    Show,
    Set(String, String),
    BackupCreate,
    BackupList,
    BackupVerify(String),
    BackupRestore(String),
    BackupResume,
    BackupRollback,
    Top {
        host: Option<String>,
        port: Option<u16>,
        watch: Option<u64>,
        json: bool,
        color: bool,
    },
    Help,
}

#[derive(Debug)]
struct Cli {
    dir: PathBuf,
    command: Command,
}

impl Cli {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let mut dir = PathBuf::from(".");
        let mut command = Vec::new();
        while let Some(arg) = args.next() {
            if arg == "--dir" {
                dir = PathBuf::from(args.next().ok_or("--dir requires a path")?);
            } else {
                command.push(arg);
            }
        }
        let command = match command.as_slice() {
            [] => Command::Run,
            [s] if s == "run" => Command::Run,
            [s] if s == "setup" => Command::Setup,
            [s] if s == "doctor" => Command::Doctor,
            [s] if s == "--help" || s == "-h" || s == "help" => Command::Help,
            [a, b] if a == "config" && b == "show" => Command::Show,
            [a, b, key, value] if a == "config" && b == "set" => {
                Command::Set(key.clone(), value.clone())
            }
            [a, b] if a == "backup" && b == "create" => Command::BackupCreate,
            [a, b] if a == "backup" && b == "list" => Command::BackupList,
            [a, b, id] if a == "backup" && b == "verify" => Command::BackupVerify(id.clone()),
            [a, b, id] if a == "backup" && b == "restore" => Command::BackupRestore(id.clone()),
            [a, b] if a == "backup" && b == "resume" => Command::BackupResume,
            [a, b] if a == "backup" && b == "rollback" => Command::BackupRollback,
            [s, rest @ ..] if s == "top" => parse_top(rest)?,
            _ => return Err(format!("unknown command; {}", usage())),
        };
        Ok(Self { dir, command })
    }
}

/// `chunkwake top [--host H] [--port P] [--watch SEC] [--json] [--no-color]`.
fn parse_top(args: &[String]) -> Result<Command, String> {
    let mut host = None;
    let mut port = None;
    let mut watch = None;
    let mut json = false;
    let mut color = true;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--host" => host = Some(it.next().ok_or("--host requires a value")?.clone()),
            "--port" => {
                let value = it.next().ok_or("--port requires a value")?;
                port = Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| format!("--port {value}: expected 1..65535"))?,
                );
            }
            "--watch" => {
                let value = it.next().ok_or("--watch requires seconds")?;
                watch = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("--watch {value}: expected seconds"))?,
                );
            }
            "--json" => json = true,
            "--no-color" => color = false,
            other => return Err(format!("unknown option {other}; {}", usage())),
        }
    }
    Ok(Command::Top {
        host,
        port,
        watch,
        json,
        color,
    })
}

fn help() -> &'static str {
    "Chunkwake  ·  Minecraft 26.3 server

USAGE
  chunkwake [--dir PATH] [COMMAND]

COMMANDS
  setup                   Configure the server interactively
  doctor                  Check settings, jar, Java, and port
  top                     Show live load once (CPU, TPS, MSPT, memory)
  config show             Show the current settings
  config set KEY VALUE    Change one setting
  backup create           Create an offline backup
  backup list             List completed backups
  backup verify ID        Check backup checksums
  backup restore ID       Restore a backup (requires confirmation)
  backup resume           Finish an interrupted restore
  backup rollback         Undo an interrupted restore
  run                     Start the server (default)

OPTIONS
  --dir PATH              Server directory (default: current directory)
  -h, --help              Show this help

TOP OPTIONS
  --host HOST             Metrics address (default: metrics-bind, then 127.0.0.1)
  --port PORT             Metrics port (default: metrics-port, then 25585)
  --watch SEC             Redraw every SEC seconds until Ctrl+C
  --json                  Print the raw snapshot instead of a table
  --no-color              Plain text without ANSI colours

EXAMPLES
  chunkwake setup --dir ./server
  chunkwake config set server-port 25570 --dir ./server
  chunkwake doctor --dir ./server
  chunkwake top --dir ./server --watch 2"
}

fn usage() -> &'static str {
    "run `chunkwake --help` for commands and examples"
}

pub async fn execute() -> Result<(), String> {
    let cli = Cli::parse(std::env::args().skip(1))?;
    match cli.command {
        Command::Run => serve(&cli.dir).await,
        Command::Setup => {
            std::fs::create_dir_all(&cli.dir).map_err(|e| format!("{}: {e}", cli.dir.display()))?;
            let _lock = backup::lock(&cli.dir)?;
            setup_tui::run(&cli.dir)
        }
        Command::Doctor => {
            if let Some(error) = backup::incomplete(&cli.dir) {
                return Err(error);
            }
            let props = Properties::load(&config_path(&cli.dir)).map_err(|error| {
                if let Some(id) = backup::latest_valid(&cli.dir) {
                    format!("{error}\nVerified backup available: {id}; run `chunkwake backup restore {id} --dir {}`", cli.dir.display())
                } else { error }
            })?;
            let mut issues = diagnose(&props, &cli.dir);
            for message in backup::scan_data(&cli.dir)? {
                issues.push(Diagnostic {
                    key: "world",
                    message,
                });
            }
            if !issues.is_empty() {
                let mut message = issues
                    .iter()
                    .map(|issue| issue.message.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                if issues.iter().any(|i| i.key == "world")
                    && let Some(id) = backup::latest_valid(&cli.dir)
                {
                    message += &format!(
                        "\nVerified backup available: {id}; restore with `chunkwake backup restore {id} --dir {}` after stopping the server",
                        cli.dir.display()
                    );
                }
                return Err(message);
            }
            let (props, version) = check(&cli.dir)?;
            println!(
                "✓  Ready to start · Minecraft {} (protocol {})\n   Listen address  {}:{}\n   Vanilla jar     {}",
                version.name,
                version.protocol,
                props.server_ip,
                props.server_port,
                props.jar_path(&cli.dir).display()
            );
            if !is_loopback(&props.server_ip) && !props.online_mode {
                println!(
                    "Warning: offline-mode does not verify player accounts; limit access to trusted users."
                );
            }
            Ok(())
        }
        Command::Show => {
            let text = config::read(&config_path(&cli.dir))?;
            Properties::parse(&text)?;
            println!("Chunkwake  ·  Current settings\n");
            print_summary(&text, &mut io::stdout()).map_err(|e| e.to_string())
        }
        Command::Set(key, value) => {
            let _lock = backup::lock(&cli.dir)?;
            set_config(&cli.dir, &key, &value)
        }
        Command::BackupCreate => {
            let _lock = backup::lock(&cli.dir)?;
            let id = backup::create(&cli.dir)?;
            println!(
                "Backup {id} created in {}",
                cli.dir.join("backups").display()
            );
            Ok(())
        }
        Command::BackupList => {
            let _lock = backup::lock(&cli.dir)?;
            for id in backup::list(&cli.dir)? {
                println!("{id}");
            }
            Ok(())
        }
        Command::BackupVerify(id) => {
            let _lock = backup::lock(&cli.dir)?;
            let count = backup::verify(&cli.dir, &id)?;
            println!("Backup {id} verified: {count} files");
            Ok(())
        }
        Command::BackupRestore(id) => {
            let _lock = backup::lock(&cli.dir)?;
            let safety = backup::restore(&cli.dir, &id)?;
            println!("Restored {id}. Previous state saved as backup {safety}.");
            Ok(())
        }
        Command::BackupResume | Command::BackupRollback => {
            let _lock = backup::lock(&cli.dir)?;
            let rollback = cli.command == Command::BackupRollback;
            backup::continue_restore(&cli.dir, rollback)?;
            println!(
                "Restore {}.",
                if rollback { "rolled back" } else { "completed" }
            );
            Ok(())
        }
        Command::Top {
            host,
            port,
            watch,
            json,
            color,
        } => top(&cli.dir, host, port, watch, json, color).await,
        Command::Help => {
            println!("{}", help());
            Ok(())
        }
    }
}

/// `chunkwake top`: снимок нагрузки с работающего сервера.
///
/// По умолчанию — один снимок в терминал; `--watch` перерисовывает его на
/// месте (без интерактивного ввода), `--json` отдаёт машинный формат.
async fn top(
    dir: &Path,
    host: Option<String>,
    port: Option<u16>,
    watch: Option<u64>,
    json: bool,
    color: bool,
) -> Result<(), String> {
    // Адрес берём из настроек сервера: тот же, что в `metrics-bind`.
    let props = Properties::load(&config_path(dir)).ok();
    let host = host
        .or_else(|| props.as_ref().map(|p| p.metrics_ip.clone()))
        .unwrap_or_else(|| "127.0.0.1".into());
    let port = port
        .or_else(|| props.as_ref().map(|p| p.metrics_port))
        .unwrap_or(25585);
    let once = || async {
        chunkwake_metrics::http::fetch(&host, port)
            .await
            .map_err(|e| format!("{e}\nIs the server running with metrics-enabled=true?"))
    };
    let Some(period) = watch else {
        let snapshot = once().await?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?
            );
        } else {
            print!(
                "{}",
                chunkwake_metrics::render::ansi::render(
                    &snapshot,
                    chunkwake_metrics::render::Section::All,
                    color,
                )
            );
        }
        return Ok(());
    };
    // Перерисовка на месте: `ESC[H` + `ESC[2J`, без альтернативного экрана.
    let period = std::time::Duration::from_secs(period.max(1));
    loop {
        match once().await {
            Ok(snapshot) => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string(&snapshot).map_err(|e| e.to_string())?
                    );
                } else if color {
                    print!(
                        "\x1b[H\x1b[2J{}",
                        chunkwake_metrics::render::ansi::render(
                            &snapshot,
                            chunkwake_metrics::render::Section::All,
                            true,
                        )
                    );
                } else {
                    println!(
                        "{}",
                        chunkwake_metrics::render::ansi::render(
                            &snapshot,
                            chunkwake_metrics::render::Section::All,
                            false,
                        )
                    );
                }
            }
            Err(error) => eprintln!("{error}"),
        }
        tokio::time::sleep(period).await;
    }
}

/// Checks the draft settings without writing them or starting the server.
pub fn diagnose(props: &Properties, dir: &Path) -> Vec<Diagnostic> {    let mut issues = Vec::new();
    let cache_ready = chunkwake_data::generator::cached(&props.generated_path(dir), MC_VERSION)
        .and_then(|data| config::read_cached_version(&data))
        .is_some_and(|version| version.name == MC_VERSION);
    if !cache_ready {
        let jar = props.jar_path(dir);
        match config::read_vanilla_version(&jar) {
            Ok(version) if version.name == MC_VERSION => {
                let (setting, java) = match std::env::var_os("CHUNKWAKE_JAVA") {
                    Some(value) => ("CHUNKWAKE_JAVA", PathBuf::from(value)),
                    None => ("java-path", props.java_command(dir)),
                };
                match std::process::Command::new(&java).arg("-version").output() {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => issues.push(Diagnostic {
                        key: "java-path",
                        message: format!(
                            "{setting}={}: Java exited with {}; choose a working Java executable",
                            java.display(),
                            output.status
                        ),
                    }),
                    Err(error) => issues.push(Diagnostic {
                        key: "java-path",
                        message: format!(
                            "{setting}={}: {error}; install Java or change {setting}",
                            java.display()
                        ),
                    }),
                }
            }
            Ok(version) => issues.push(Diagnostic {
                key: "vanilla-jar",
                message: format!(
                    "vanilla-jar={}: Minecraft {} found; Chunkwake needs {MC_VERSION}",
                    jar.display(),
                    version.name
                ),
            }),
            Err(error) => issues.push(Diagnostic {
                key: "vanilla-jar",
                message: error,
            }),
        }
    }
    let address = format!("{}:{}", props.server_ip, props.server_port);
    let bind = (props.server_ip.as_str(), props.server_port);
    if let Err(error) = std::net::TcpListener::bind(bind) {
        issues.push(Diagnostic {
            key: "server-port",
            message: format!(
                "server-ip/server-port={address}: {error}; choose an available address and port"
            ),
        });
    }
    issues
}

fn config_path(dir: &Path) -> PathBuf {
    dir.join("server.properties")
}

pub fn check(dir: &Path) -> Result<(Properties, GameVersion), String> {
    if let Some(error) = backup::incomplete(dir) {
        return Err(error);
    }
    let props = Properties::load(&config_path(dir))?;
    props.check_supported()?;
    // Данные уже извлечены: jar больше не нужен.
    if let Some(version) = chunkwake_data::generator::cached(&props.generated_path(dir), MC_VERSION)
        .and_then(|d| config::read_cached_version(&d))
        .filter(|v| v.name == MC_VERSION)
    {
        return Ok((props, version));
    }
    let path = props.jar_path(dir);
    let version = config::read_vanilla_version(&path)?;
    if version.name != MC_VERSION {
        return Err(format!(
            "vanilla-jar={}: Minecraft {} found, but Chunkwake supports {}; replace the jar or change vanilla-jar",
            path.display(),
            version.name,
            MC_VERSION
        ));
    }
    Ok((props, version))
}

fn set_config(dir: &Path, key: &str, value: &str) -> Result<(), String> {
    let path = config_path(dir);
    let original = config::read(&path)?;
    let changed = config::set(&original, key, value)?;
    Properties::parse(&changed)?;
    config::save(&path, &changed)?;
    println!("{key} updated in {}", path.display());
    Ok(())
}

fn is_loopback(ip: &str) -> bool {
    ip.parse::<IpAddr>()
        .expect("validated server-ip")
        .is_loopback()
}

fn print_summary<W: Write>(text: &str, output: &mut W) -> io::Result<()> {
    for key in config::KEYS {
        if *key == "level-name" {
            writeln!(output, "\n# Saved for future versions; no effect yet")?;
        }
        writeln!(output, "{key}={}", config::effective_value(text, key))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zip::write::SimpleFileOptions;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "chunkwake-cli-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn top_parses_options() {
        let cli = Cli::parse([
            "top".into(),
            "--port".into(),
            "25590".into(),
            "--watch".into(),
            "2".into(),
            "--no-color".into(),
        ])
        .unwrap();
        match cli.command {
            Command::Top {
                port,
                watch,
                color,
                json,
                host,
            } => {
                assert_eq!(port, Some(25590));
                assert_eq!(watch, Some(2));
                assert!(!color);
                assert!(!json);
                assert!(host.is_none());
            }
            other => panic!("не top: {other:?}"),
        }
        assert!(Cli::parse(["top".into(), "--port".into(), "x".into()]).is_err());
        assert!(Cli::parse(["top".into(), "--bogus".into()]).is_err());
    }

    #[test]
    fn arguments_and_missing_setup() {
        assert_eq!(
            Cli::parse(["doctor".into(), "--dir".into(), "world".into()])
                .unwrap()
                .command,
            Command::Doctor
        );
        let dir = temp_dir();
        assert!(check(&dir).unwrap_err().contains("setup"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doctor_rejects_missing_jar() {
        let dir = temp_dir();
        config::save(
            &config_path(&dir),
            "online-mode=false\nvanilla-jar=server.jar",
        )
        .unwrap();
        assert!(check(&dir).unwrap_err().contains("vanilla-jar"));
        let props = Properties::load(&config_path(&dir)).unwrap();
        assert_eq!(props.jar_path(&dir), dir.join("server.jar"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doctor_rejects_wrong_jar_version() {
        let dir = temp_dir();
        config::save(&config_path(&dir), "vanilla-jar=server.jar").unwrap();
        let file = std::fs::File::create(dir.join("server.jar")).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("version.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"{"name":"26.4","protocol_version":999}"#)
            .unwrap();
        zip.finish().unwrap();
        let error = check(&dir).unwrap_err();
        assert!(error.contains("26.4"));
        assert!(error.contains("vanilla-jar"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_set_rejects_invalid_value_without_touching_file() {
        let dir = temp_dir();
        let path = config_path(&dir);
        config::save(&path, "# keep\nserver-port=25565\nunknown=yes\n").unwrap();
        assert!(set_config(&dir, "server-port", "99999").is_err());
        assert_eq!(
            config::read(&path).unwrap(),
            "# keep\nserver-port=25565\nunknown=yes\n"
        );
        assert!(set_config(&dir, "unknown", "no").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doctor_reports_busy_port_and_missing_java() {
        let dir = temp_dir();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let file = std::fs::File::create(dir.join("server.jar")).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("version.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"{"name":"26.3","protocol_version":777}"#)
            .unwrap();
        zip.finish().unwrap();
        let props = Properties::parse(&format!(
            "server-port={port}\njava-path=missing-chunkwake-java-command"
        ))
        .unwrap();
        let issues = diagnose(&props, &dir);
        assert!(issues.iter().any(|issue| issue.key == "server-port"));
        assert!(issues.iter().any(|issue| issue.key == "java-path"));
        drop(listener);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
