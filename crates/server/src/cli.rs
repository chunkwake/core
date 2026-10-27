//! Команды первоначальной настройки и диагностики.

use crate::config::{self, Properties};
use crate::{MC_VERSION, serve};
use chunkwake_net::GameVersion;
use std::io::{self, BufRead, IsTerminal, Write};
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

fn usage() -> &'static str {
    "usage: chunkwake [--dir PATH] [run | setup | doctor | config show | config set KEY VALUE]"
}

pub async fn execute() -> Result<(), String> {
    let cli = Cli::parse(std::env::args().skip(1))?;
    match cli.command {
        Command::Run => serve(&cli.dir).await,
        Command::Setup => {
            if !io::stdin().is_terminal() {
                return Err("setup requires an interactive terminal; run it in a Windows/Linux terminal before starting the service".into());
            }
            let mut input = io::stdin().lock();
            let mut output = io::stdout().lock();
            setup(&cli.dir, &mut input, &mut output)
        }
        Command::Doctor => {
            let (props, version) = check(&cli.dir)?;
            println!(
                "Ready: Minecraft {} (protocol {}), {}:{}, jar {}",
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
            print_summary(&props, &mut io::stdout()).map_err(|e| e.to_string())
        }
        Command::Set(key, value) => set_config(&cli.dir, &key, &value),
        Command::Help => {
            println!("{}", usage());
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

fn setup<R: BufRead, W: Write>(dir: &Path, input: &mut R, output: &mut W) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!(
            "--dir {} does not exist; create it first",
            dir.display()
        ));
    }
    let path = config_path(dir);
    let mut text = if path.exists() {
        config::read(&path)?
    } else {
        config::DEFAULT_PROPERTIES.to_owned()
    };
    let current = Properties::parse(&text)?;
    writeln!(output, "Chunkwake setup: {}", path.display()).map_err(|e| e.to_string())?;
    writeln!(output, "Press Enter to keep each current value.").map_err(|e| e.to_string())?;
    for (key, value) in [
        ("server-ip", current.server_ip.as_str()),
        ("server-port", &current.server_port.to_string()),
        ("motd", current.motd.as_str()),
        ("max-players", &current.max_players.to_string()),
        ("vanilla-jar", current.vanilla_jar.as_str()),
    ] {
        let entered = prompt(input, output, key, value)?;
        text = config::set(&text, key, &entered)?;
    }
    let props = Properties::parse(&text)?;
    props.check_supported()?;
    if !is_loopback(&props.server_ip) {
        writeln!(output, "Warning: this address may expose the server. Offline mode does not verify player accounts.")
            .map_err(|e| e.to_string())?;
        if !confirm(input, output, "Use this address?", false)? {
            return Err("setup cancelled; no changes saved".into());
        }
    }
    writeln!(output, "\nConfiguration:").map_err(|e| e.to_string())?;
    print_summary(&props, output).map_err(|e| e.to_string())?;
    writeln!(output, "Jar resolves to: {}", props.jar_path(dir).display())
        .map_err(|e| e.to_string())?;
    if !confirm(input, output, "Save configuration?", false)? {
        return Err("setup cancelled; no changes saved".into());
    }
    config::save(&path, &text)?;
    writeln!(
        output,
        "Saved {}. Run `chunkwake doctor --dir {}` to check the jar.",
        path.display(),
        dir.display()
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn is_loopback(ip: &str) -> bool {
    ip.parse::<IpAddr>()
        .expect("validated server-ip")
        .is_loopback()
}

fn prompt<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    key: &str,
    current: &str,
) -> Result<String, String> {
    write!(output, "{key} [{current}]: ").map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    if input.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
        return Err("input ended; setup cancelled without saving".into());
    }
    let value = line.trim_end_matches(['\r', '\n']);
    Ok(if value.is_empty() {
        current.to_owned()
    } else {
        value.to_owned()
    })
}

fn confirm<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    question: &str,
    default: bool,
) -> Result<bool, String> {
    let hint = if default { "Y/n" } else { "y/N" };
    let answer = prompt(input, output, question, hint)?;
    if answer == hint {
        Ok(default)
    } else {
        match answer.to_ascii_lowercase().as_str() {
            "y" | "yes" => Ok(true),
            "n" | "no" => Ok(false),
            _ => Err(format!(
                "{question}: answer y or n; setup cancelled without saving"
            )),
        }
    }
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
    use std::io::Cursor;
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
    fn setup_then_repeat_preserves_custom_lines() {
        let dir = temp_dir();
        let mut output = Vec::new();
        setup(&dir, &mut Cursor::new("\n\nHello\n\n\ny\n"), &mut output).unwrap();
        let path = config_path(&dir);
        let first = config::read(&path).unwrap();
        assert!(first.contains("motd=Hello"));
        config::save(&path, &format!("# custom\ncustom-key=keep\n{first}")).unwrap();
        setup(&dir, &mut Cursor::new("\n\n\n\n\ny\n"), &mut output).unwrap();
        let second = config::read(&path).unwrap();
        assert!(second.contains("# custom\ncustom-key=keep"));
        assert!(second.contains("motd=Hello"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn setup_eof_does_not_write() {
        let dir = temp_dir();
        assert!(setup(&dir, &mut Cursor::new(""), &mut Vec::new()).is_err());
        assert!(!config_path(&dir).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doctor_rejects_unsupported_auth_and_missing_jar() {
        let dir = temp_dir();
        config::save(&config_path(&dir), "online-mode=true").unwrap();
        assert!(check(&dir).unwrap_err().contains("online-mode"));
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
    fn setup_requires_confirmation_for_public_address() {
        let dir = temp_dir();
        let result = setup(
            &dir,
            &mut Cursor::new("0.0.0.0\n\n\n\n\nn\n"),
            &mut Vec::new(),
        );
        assert!(result.is_err());
        assert!(!config_path(&dir).exists());
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
