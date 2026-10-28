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
            _ => return Err(format!("unknown command; {}", usage())),
        };
        Ok(Self { dir, command })
    }
}

fn help() -> &'static str {
    "Chunkwake  ·  Minecraft 26.3 server

USAGE
  chunkwake [--dir PATH] [COMMAND]

COMMANDS
  setup                   Configure the server interactively
  doctor                  Check settings, jar, Java, and port
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

EXAMPLES
  chunkwake setup --dir ./server
  chunkwake config set server-port 25570 --dir ./server
  chunkwake doctor --dir ./server"
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
        Command::Help => {
            println!("{}", help());
            Ok(())
        }
    }
}

/// Checks the draft settings without writing them or starting the server.
pub fn diagnose(props: &Properties, dir: &Path) -> Vec<Diagnostic> {
    let mut issues = Vec::new();
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
