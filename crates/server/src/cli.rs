//! Команды первоначальной настройки и диагностики.

use crate::config::{self, Properties};
use crate::{MC_VERSION, serve, setup_tui};
use chunkwake_net::GameVersion;
use std::io::{self, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run,
    Setup,
    Doctor,
    Show,
    Set(String, String),
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
  doctor                  Check settings and the vanilla server jar
  config show             Show the current settings
  config set KEY VALUE    Change one setting
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
        Command::Setup => setup_tui::run(&cli.dir),
        Command::Doctor => {
            let (props, version) = check(&cli.dir)?;
            println!(
                "✓  Ready to start · Minecraft {} (protocol {})\n   Listen address  {}:{}\n   Vanilla jar     {}",
                version.name,
                version.protocol,
                props.server_ip,
                props.server_port,
                props.jar_path(&cli.dir).display()
            );
            if !is_loopback(&props.server_ip) {
                println!(
                    "Warning: offline-mode does not verify player accounts; limit access to trusted users."
                );
            }
            Ok(())
        }
        Command::Show => {
            let props = Properties::load(&config_path(&cli.dir))?;
            println!("Chunkwake  ·  Current settings\n");
            print_summary(&props, &mut io::stdout()).map_err(|e| e.to_string())
        }
        Command::Set(key, value) => set_config(&cli.dir, &key, &value),
        Command::Help => {
            println!("{}", help());
            Ok(())
        }
    }
}

fn config_path(dir: &Path) -> PathBuf {
    dir.join("server.properties")
}

pub fn check(dir: &Path) -> Result<(Properties, GameVersion), String> {
    let props = Properties::load(&config_path(dir))?;
    props.check_supported()?;
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

fn print_summary<W: Write>(props: &Properties, output: &mut W) -> io::Result<()> {
    writeln!(output, "server-ip={}", props.server_ip)?;
    writeln!(output, "server-port={}", props.server_port)?;
    writeln!(output, "motd={}", props.motd)?;
    writeln!(output, "max-players={}", props.max_players)?;
    writeln!(output, "online-mode={}", props.online_mode)?;
    writeln!(
        output,
        "network-compression-threshold={}",
        props.compression_threshold
    )?;
    writeln!(output, "vanilla-jar={}", props.vanilla_jar)
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
}
