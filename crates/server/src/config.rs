//! Чтение, проверка и точечное редактирование `server.properties`.

use chunkwake_net::{GameVersion, NetConfig};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

pub const DEFAULT_PROPERTIES: &str = "\
# Chunkwake server properties
server-ip=127.0.0.1
server-port=25565
enable-status=true
motd=A Chunkwake Server
max-players=20
online-mode=false
network-compression-threshold=256
vanilla-jar=
java-path=java
generated-dir=generated
log-level=info
# World and gameplay settings are stored for future versions of Chunkwake.
level-name=world
level-seed=
level-type=minecraft:flat
generator-settings=
view-distance=8
simulation-distance=6
gamemode=survival
difficulty=easy
hardcore=false
pvp=true
white-list=false
allow-flight=false
spawn-protection=16
metrics-enabled=true
metrics-bind=127.0.0.1
metrics-port=25585
# Пусто — дашборд доступен только с этой машины (metrics-bind на loopback).
# Задайте пароль, чтобы открыть его из сети:
# chunkwake config set metrics-token <длинная случайная строка>
metrics-token=
";

pub const KEYS: &[&str] = &[
    "server-ip",
    "server-port",
    "enable-status",
    "motd",
    "max-players",
    "online-mode",
    "network-compression-threshold",
    "vanilla-jar",
    "java-path",
    "generated-dir",
    "log-level",
    "level-name",
    "level-seed",
    "level-type",
    "generator-settings",
    "view-distance",
    "simulation-distance",
    "gamemode",
    "difficulty",
    "hardcore",
    "pvp",
    "white-list",
    "allow-flight",
    "spawn-protection",
    "metrics-enabled",
    "metrics-bind",
    "metrics-port",
    "metrics-token",
];

#[derive(Debug)]
pub struct Properties {
    pub server_ip: String,
    pub server_port: u16,
    pub enable_status: bool,
    pub motd: String,
    pub max_players: usize,
    pub online_mode: bool,
    pub compression_threshold: i32,
    pub vanilla_jar: Option<String>,
    pub java_path: String,
    pub generated_dir: String,
    /// Каталог мира (`level-name`), относительно каталога сервера.
    pub level_name: String,
    /// Сырые `level-seed`, `level-type`, `generator-settings`: разбираются с данными игры.
    pub level_seed: String,
    pub level_type: String,
    pub generator_settings: String,
    /// Радиус прорисовки: сколько чанков уходит клиенту.
    pub view_distance: u8,
    /// Радиус симуляции: где тикают блоки и сущности. Не больше прорисовки.
    pub simulation_distance: u8,
    /// 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub game_mode: u8,
    pub difficulty: chunkwake_net::survival::Difficulty,
    /// Радиус `spawn-protection`. 0 — защита выключена.
    pub spawn_protection: u32,
    /// `hardcore`: сердечко хардкора в пакете входа.
    pub hardcore: bool,
    /// Стартовое правило `pvp`, если мир его ещё не сохранил.
    pub pvp: bool,
    /// `allow-flight`: выживание и приключение тоже могут летать.
    pub allow_flight: bool,
    /// `white-list`: пускать только имена из белого списка.
    pub white_list: bool,
    /// HTTP с `/metrics` и `/snapshot` (`chunkwake top`).
    pub metrics_enabled: bool,
    pub metrics_ip: String,
    pub metrics_port: u16,
    /// Токен доступа к дашборду. Пустой — доступ только с текущей машины.
    ///
    /// Нужен, когда `metrics-bind` смотрит не только на loopback: иначе
    /// дашборд со всей нагрузкой открыт любому в сети.
    pub metrics_token: String,
    pub log_level: tracing_subscriber::filter::LevelFilter,
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
        let enable_status = value("enable-status")
            .parse::<bool>()
            .map_err(|_| "enable-status: use true or false".to_owned())?;
        let compression_threshold = value("network-compression-threshold")
            .parse::<i32>()
            .map_err(|_| {
                "network-compression-threshold: enter -1 or a non-negative integer".to_owned()
            })?;
        if compression_threshold < -1 {
            return Err("network-compression-threshold: enter -1 or a non-negative integer".into());
        }
        let vanilla_jar = value("vanilla-jar");
        let vanilla_jar = if vanilla_jar.is_empty() {
            // Данные обычно встроены в бинарь; jar нужен только чтобы их
            // пересобрать, поэтому пустое значение — допустимый выбор.
            None
        } else {
            Some(vanilla_jar.to_owned())
        };
        let java_path = value("java-path");
        if java_path.is_empty() {
            return Err("java-path: enter a Java executable or path".into());
        }
        let generated_dir = value("generated-dir");
        if generated_dir.is_empty() {
            return Err("generated-dir: enter a directory".into());
        }
        let log_level = match value("log-level") {
            "error" => tracing_subscriber::filter::LevelFilter::ERROR,
            "warn" => tracing_subscriber::filter::LevelFilter::WARN,
            "info" => tracing_subscriber::filter::LevelFilter::INFO,
            "debug" => tracing_subscriber::filter::LevelFilter::DEBUG,
            "trace" => tracing_subscriber::filter::LevelFilter::TRACE,
            _ => return Err("log-level: choose error, warn, info, debug, or trace".into()),
        };
        if value("level-name").is_empty() {
            return Err("level-name: enter a world name".into());
        }
        let distance = |key: &str| -> Result<u8, String> {
            let text = value(key);
            let n = text
                .parse::<u8>()
                .map_err(|_| format!("{key}: enter a number from 2 to 32"))?;
            if !(2..=32).contains(&n) {
                return Err(format!("{key}: enter a number from 2 to 32"));
            }
            Ok(n)
        };
        let view_distance = distance("view-distance")?;
        let simulation_distance = distance("simulation-distance")?;
        if !["survival", "creative", "adventure", "spectator"].contains(&value("gamemode")) {
            return Err("gamemode: choose survival, creative, adventure, or spectator".into());
        }
        if !["peaceful", "easy", "normal", "hard"].contains(&value("difficulty")) {
            return Err("difficulty: choose peaceful, easy, normal, or hard".into());
        }
        let flag = |key: &str| -> Result<bool, String> {
            value(key)
                .parse::<bool>()
                .map_err(|_| format!("{key}: use true or false"))
        };
        let hardcore = flag("hardcore")?;
        let pvp = flag("pvp")?;
        let white_list = flag("white-list")?;
        let allow_flight = flag("allow-flight")?;
        let spawn_protection = value("spawn-protection")
            .parse::<u32>()
            .map_err(|_| "spawn-protection: enter a non-negative integer".to_owned())?;
        let metrics_enabled = value("metrics-enabled")
            .parse::<bool>()
            .map_err(|_| "metrics-enabled: use true or false".to_owned())?;
        let metrics_ip = value("metrics-bind").to_owned();
        metrics_ip.parse::<IpAddr>().map_err(|_| {
            "metrics-bind: use an IPv4 or IPv6 address, such as 127.0.0.1".to_owned()
        })?;
        let metrics_port = value("metrics-port")
            .parse::<u16>()
            .map_err(|_| "metrics-port: enter a number from 1 to 65535".to_owned())?;
        if metrics_port == 0 {
            return Err("metrics-port: enter a number from 1 to 65535".into());
        }
        // Токен — одна строка без пробелов по краям: его передают в ссылке
        // и заголовке, поэтому пробел в середине сделал бы вход неочевидным.
        let metrics_token = value("metrics-token").trim().to_owned();
        if metrics_token.contains(char::is_whitespace) {
            return Err("metrics-token: no spaces; use a long random string".into());
        }
        if !metrics_token.is_empty() && metrics_token.len() < 16 {
            return Err("metrics-token: at least 16 characters, otherwise it is guessable".into());
        }
        Ok(Self {
            server_ip: server_ip.into(),
            server_port: port,
            enable_status,
            motd: value("motd").into(),
            max_players,
            online_mode,
            compression_threshold,
            vanilla_jar,
            java_path: java_path.into(),
            generated_dir: generated_dir.into(),
            level_name: value("level-name").into(),
            level_seed: value("level-seed").into(),
            level_type: value("level-type").into(),
            generator_settings: value("generator-settings").into(),
            view_distance,
            simulation_distance,
            game_mode: ["survival", "creative", "adventure", "spectator"]
                .iter()
                .position(|m| *m == value("gamemode"))
                .unwrap_or(0) as u8,
            difficulty: {
                use chunkwake_net::survival::Difficulty::*;
                match value("difficulty") {
                    "peaceful" => Peaceful,
                    "normal" => Normal,
                    "hard" => Hard,
                    _ => Easy,
                }
            },
            log_level,
            spawn_protection,
            hardcore,
            pvp,
            allow_flight,
            white_list,
            metrics_enabled,
            metrics_ip,
            metrics_port,
            metrics_token,
        })
    }

    pub fn check_supported(&self) -> Result<(), String> {
        // Настройки будущих этапов проверяются при разборе и хранятся в файле,
        // но не передаются текущему игровому движку.
        Ok(())
    }

    /// Путь к `server.jar`, если он задан. Данные обычно встроены в бинарь,
    /// поэтому jar не обязателен: он нужен только чтобы собрать кэш заново.
    pub fn jar_path(&self, dir: &Path) -> Option<PathBuf> {
        let text = self.vanilla_jar.as_deref()?;
        let path = Path::new(text);
        Some(if path.is_absolute() {
            path.to_owned()
        } else {
            dir.join(path)
        })
    }

    pub fn java_command(&self, dir: &Path) -> PathBuf {
        let path = Path::new(&self.java_path);
        if path.is_absolute() || path.components().count() == 1 {
            path.to_owned()
        } else {
            dir.join(path)
        }
    }

    pub fn generated_path(&self, dir: &Path) -> PathBuf {
        let path = Path::new(&self.generated_dir);
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
            enable_status: self.enable_status,
            session_server: chunkwake_net::auth::MOJANG_SESSION_SERVER.into(),
            configuration: None,
            admit: None,
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

pub fn effective_value(text: &str, key: &str) -> String {
    let mut map = parse_kv(DEFAULT_PROPERTIES);
    map.extend(parse_kv(text));
    map[key].to_owned()
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
/// Файл версии внутри кэша данных: с ним server.jar при запуске не нужен.
pub const CACHED_VERSION: &str = "version.json";

/// Версия из готового кэша (`generated/<версия>/version.json`).
pub fn read_cached_version(data_dir: &Path) -> Option<GameVersion> {
    let text = std::fs::read_to_string(data_dir.join(CACHED_VERSION)).ok()?;
    parse_version_json(&text).ok()
}

/// Копирует `version.json` из jar в кэш.
pub fn cache_version(jar: &Path, data_dir: &Path) -> Result<(), String> {
    let text =
        chunkwake_data::cache::version_json(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
    std::fs::write(data_dir.join(CACHED_VERSION), text)
        .map_err(|e| format!("write {}: {e}", data_dir.display()))
}

/// Читает `version.json` из ванильного jar (в том числе из вложенного jar
/// версии внутри бандлера): так `doctor` и `check` сверяют версию файла.
pub fn read_vanilla_version(jar: &Path) -> Result<GameVersion, String> {
    if !jar.is_file() {
        return Err(format!(
            "vanilla-jar={}: not found; the embedded data does not need it, \
             the jar is only needed to rebuild the data cache",
            jar.display()
        ));
    }
    let text = chunkwake_data::cache::version_json(jar)
        .map_err(|e| format!("vanilla-jar={}: {e}", jar.display()))?;
    parse_version_json(&text).map_err(|e| format!("vanilla-jar={}: {e}", jar.display()))
}

/// `version.json` → версия игры. Тем же разбором пользуются кэш, jar и
/// встроенный архив.
pub fn version_from_json(text: &str) -> Result<GameVersion, String> {
    parse_version_json(text)
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
        assert!(p.enable_status);
        assert_eq!(p.java_path, "java");
        assert_eq!(effective_value("", "gamemode"), "survival");
        // Прорисовка и симуляция: по умолчанию 8 и 6, как в этом плане.
        assert_eq!(p.view_distance, 8);
        assert_eq!(p.simulation_distance, 6);
        assert!(!p.hardcore);
        assert!(p.pvp);
        assert!(!p.allow_flight);
        assert_eq!(effective_value("", "view-distance"), "8");
        assert_eq!(effective_value("", "simulation-distance"), "6");
    }

    #[test]
    fn distances_come_from_the_file() {
        let p = Properties::parse("view-distance=12\nsimulation-distance=4").unwrap();
        assert_eq!(p.view_distance, 12);
        assert_eq!(p.simulation_distance, 4);
        assert_eq!(
            effective_value("view-distance=5", "view-distance"),
            "5",
            "файл важнее умолчания"
        );
    }

    #[test]
    fn simulation_is_clamped_by_view_distance() {
        // Сервер отдаёт клиенту прорисовку не меньше симуляции; за пределами
        // прорисовки чанков нет, поэтому симуляция обрезается.
        let p = Properties::parse("view-distance=4\nsimulation-distance=16").unwrap();
        assert_eq!(p.simulation_distance.min(p.view_distance), 4);
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
            "enable-status=yes",
            "view-distance=1",
            "simulation-distance=33",
            "gamemode=unknown",
            "difficulty=unknown",
            "hardcore=yes",
            "spawn-protection=-1",
            "log-level=verbose",
        ] {
            assert!(Properties::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn future_settings_preserve_comments_and_unknown_keys() {
        let original = "# existing\ncustom-setting=kept\ngamemode=survival\n";
        let updated = set(original, "gamemode", "creative").unwrap();
        assert_eq!(
            updated,
            "# existing\ncustom-setting=kept\ngamemode=creative\n"
        );
        Properties::parse(&updated).unwrap();
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
