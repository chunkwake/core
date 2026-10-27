//! Чтение, проверка и точечное редактирование `server.properties`.

use chunkwake_net::{GameVersion, NetConfig};
use std::collections::HashMap;
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

pub const DEFAULT_PROPERTIES: &str = "\
# Chunkwake server properties
server-ip=127.0.0.1
server-port=25565
motd=A Chunkwake Server
max-players=20
online-mode=false
network-compression-threshold=256
vanilla-jar=server.jar
";

pub const KEYS: &[&str] = &[
    "server-ip",
    "server-port",
    "motd",
    "max-players",
    "online-mode",
    "network-compression-threshold",
    "vanilla-jar",
];

#[derive(Debug)]
pub struct Properties {
    pub server_ip: String,
    pub server_port: u16,
    pub motd: String,
    pub max_players: usize,
    pub online_mode: bool,
    pub compression_threshold: i32,
    pub vanilla_jar: String,
}

impl Properties {
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::parse(&read(path)?)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut map = parse_kv(DEFAULT_PROPERTIES);
        map.extend(parse_kv(text));
        let value = |key: &str| map[key];
        let ip = value("server-ip");
        let server_ip = if ip.is_empty() { "0.0.0.0" } else { ip };
        server_ip.parse::<IpAddr>().map_err(|_| {
            format!("server-ip={ip:?}: use an IPv4 or IPv6 address, such as 127.0.0.1")
        })?;
        let port = value("server-port")
            .parse::<u16>()
            .map_err(|_| "server-port: enter a number from 1 to 65535".to_owned())?;
        if port == 0 {
            return Err("server-port: enter a number from 1 to 65535".into());
        }
        let max_players = value("max-players")
            .parse::<usize>()
            .map_err(|_| "max-players: enter a positive integer".to_owned())?;
        if max_players == 0 {
            return Err("max-players: enter a positive integer".into());
        }
        let online_mode = value("online-mode")
            .parse::<bool>()
            .map_err(|_| "online-mode: use true or false".to_owned())?;
        let compression_threshold = value("network-compression-threshold")
            .parse::<i32>()
            .map_err(|_| {
                "network-compression-threshold: enter -1 or a non-negative integer".to_owned()
            })?;
        if compression_threshold < -1 {
            return Err("network-compression-threshold: enter -1 or a non-negative integer".into());
        }
        let vanilla_jar = value("vanilla-jar");
        if vanilla_jar.is_empty() {
            return Err("vanilla-jar: enter a path to the Minecraft server.jar".into());
        }
        Ok(Self {
            server_ip: server_ip.into(),
            server_port: port,
            motd: value("motd").into(),
            max_players,
            online_mode,
            compression_threshold,
            vanilla_jar: vanilla_jar.into(),
        })
    }

    pub fn check_supported(&self) -> Result<(), String> {
        if self.online_mode {
            return Err("online-mode=true: authentication is not implemented yet; set online-mode=false with `chunkwake config set online-mode false`".into());
        }
        Ok(())
    }

    pub fn jar_path(&self, dir: &Path) -> PathBuf {
        let path = Path::new(&self.vanilla_jar);
        if path.is_absolute() {
            path.to_owned()
        } else {
            dir.join(path)
        }
    }

    pub fn net_config(&self, version: GameVersion) -> NetConfig {
        NetConfig {
            version,
            motd: self.motd.clone(),
            max_players: self.max_players,
            compression_threshold: self.compression_threshold,
            online_mode: self.online_mode,
        }
    }
}

pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "{} is missing; run `chunkwake setup --dir {}` first",
                path.display(),
                path.parent().unwrap_or(Path::new(".")).display()
            )
        } else {
            format!("read {}: {e}", path.display())
        }
    })
}

pub fn set(text: &str, key: &str, value: &str) -> Result<String, String> {
    if !KEYS.contains(&key) {
        return Err(format!(
            "unknown setting {key:?}; supported: {}",
            KEYS.join(", ")
        ));
    }
    if value.contains(['\r', '\n']) {
        return Err(format!("{key}: value must be one line"));
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let position = lines.iter().rposition(|line| {
        let line = line.trim();
        !line.starts_with('#')
            && line
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == key)
    });
    match position {
        Some(index) => {
            let prefix = lines[index]
                .split_once('=')
                .expect("matched key has equals")
                .0;
            lines[index] = format!("{prefix}={value}");
        }
        None => lines.push(format!("{key}={value}")),
    }
    let mut result = lines.join(newline);
    result.push_str(newline);
    Ok(result)
}

pub fn save(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn parse_kv(text: &str) -> HashMap<&str, &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect()
}

/// Читает `version.json` из ванильного jar.
pub fn read_vanilla_version(jar: &Path) -> Result<GameVersion, String> {
    let file = std::fs::File::open(jar).map_err(|e| {
        format!("vanilla-jar={}: {e}; place the Minecraft 26.3 server.jar there or run `chunkwake config set vanilla-jar PATH`", jar.display())
    })?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| format!("vanilla-jar={}: invalid jar: {e}", jar.display()))?;
    let mut text = String::new();
    zip.by_name("version.json")
        .map_err(|e| format!("vanilla-jar={}: missing version.json: {e}", jar.display()))?
        .read_to_string(&mut text)
        .map_err(|e| format!("vanilla-jar={}: read version.json: {e}", jar.display()))?;
    parse_version_json(&text).map_err(|e| format!("vanilla-jar={}: {e}", jar.display()))
}

fn parse_version_json(text: &str) -> Result<GameVersion, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("version.json: {e}"))?;
    let name = v["name"]
        .as_str()
        .ok_or("version.json: no name")?
        .to_owned();
    let protocol = v["protocol_version"]
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .ok_or("version.json: invalid protocol_version")?;
    Ok(GameVersion { name, protocol })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_overrides() {
        let p = Properties::parse("server-port=25570\nmotd = Hi\n# c\nonline-mode=true").unwrap();
        assert_eq!(p.server_port, 25570);
        assert_eq!(p.motd, "Hi");
        assert!(p.online_mode);
        assert_eq!(p.compression_threshold, 256);
        assert_eq!(p.server_ip, "127.0.0.1");
    }

    #[test]
    fn invalid_values_are_rejected() {
        for text in [
            "server-port=0",
            "server-port=65536",
            "max-players=-1",
            "max-players=0",
            "online-mode=yes",
            "network-compression-threshold=-2",
            "network-compression-threshold=2147483648",
            "server-ip=bad host",
        ] {
            assert!(Properties::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn last_duplicate_wins_and_edit_preserves_other_lines() {
        let input = "# note\r\nserver-port=1\r\ncustom=keep\r\nserver-port = 2\r\n";
        assert_eq!(Properties::parse(input).unwrap().server_port, 2);
        let edited = set(input, "server-port", "25565").unwrap();
        assert_eq!(
            edited,
            "# note\r\nserver-port=1\r\ncustom=keep\r\nserver-port =25565\r\n"
        );
        assert_eq!(Properties::parse(&edited).unwrap().server_port, 25565);
    }

    #[test]
    fn version_json() {
        let v =
            parse_version_json(r#"{"id":"26.3","name":"26.3","protocol_version":777}"#).unwrap();
        assert_eq!((v.name.as_str(), v.protocol), ("26.3", 777));
    }
}
