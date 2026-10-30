//! Переменные сервера, которые меняются командами: геймрулы, белый список,
//! баны, погода, точка возрождения мира, `idle-timeout`.
//!
//! Всё, что не относится к миру, а относится к серверу, живёт здесь и
//! сохраняется в `world/gamerules.json` рядом с миром, чтобы правило не
//! терялось между запусками (как `level.dat` ванилы).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Значение геймрула: ванила хранит числа, флаги и строки.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

impl RuleValue {
    /// Как его показать в чате.
    pub fn render(&self) -> String {
        match self {
            RuleValue::Bool(v) => v.to_string(),
            RuleValue::Int(v) => v.to_string(),
            RuleValue::Text(v) => v.clone(),
        }
    }

    /// Разбор введённого значения по типу умолчания.
    pub fn parse_like(&self, input: &str) -> Option<Self> {
        match self {
            RuleValue::Bool(_) => input.parse::<bool>().ok().map(RuleValue::Bool),
            RuleValue::Int(_) => input.parse::<i64>().ok().map(RuleValue::Int),
            RuleValue::Text(_) => Some(RuleValue::Text(input.to_owned())),
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            RuleValue::Bool(v) => *v,
            RuleValue::Int(v) => *v != 0,
            RuleValue::Text(v) => v == "true",
        }
    }

    pub fn as_int(&self) -> i64 {
        match self {
            RuleValue::Bool(v) => i64::from(*v),
            RuleValue::Int(v) => *v,
            RuleValue::Text(_) => 0,
        }
    }
}

/// Ванильные умолчания 26.3.
///
/// Имена и типы взяты из `reports/commands.json` этой же версии: в 26.x
/// правила переименованы в snake_case (`keep_inventory`, `advance_time`) и
/// у каждого объявлен парсер — `brigadier:bool` или `brigadier:integer`.
/// Список полный: `/gamerule` без аргументов должен показывать ровно то же,
/// что ванила, даже если тик пока читает не все правила.
pub fn defaults() -> BTreeMap<String, RuleValue> {
    let mut m = BTreeMap::new();
    let flag = |m: &mut BTreeMap<String, RuleValue>, name: &str, value: bool| {
        m.insert(name.to_owned(), RuleValue::Bool(value));
    };
    let number = |m: &mut BTreeMap<String, RuleValue>, name: &str, value: i64| {
        m.insert(name.to_owned(), RuleValue::Int(value));
    };
    let flags: &[(&str, bool)] = &[
        ("advance_time", true),
        ("advance_weather", true),
        ("allow_entering_nether_using_portals", true),
        ("block_drops", true),
        ("block_explosion_drop_decay", true),
        ("command_block_output", true),
        ("command_blocks_work", true),
        ("drowning_damage", true),
        ("elytra_movement_check", true),
        ("ender_pearls_vanish_on_death", true),
        ("entity_drops", true),
        ("fall_damage", true),
        ("fire_damage", true),
        ("forgive_dead_players", true),
        ("freeze_damage", true),
        ("global_sound_events", true),
        ("immediate_respawn", false),
        ("keep_inventory", false),
        ("lava_source_conversion", false),
        ("limited_crafting", false),
        ("locator_bar", true),
        ("log_admin_commands", true),
        ("mob_drops", true),
        ("mob_explosion_drop_decay", true),
        ("mob_griefing", true),
        ("natural_health_regeneration", true),
        ("player_movement_check", true),
        ("projectiles_can_break_blocks", false),
        ("pvp", true),
        ("raids", true),
        ("reduced_debug_info", false),
        ("send_command_feedback", true),
        ("show_advancement_messages", true),
        ("show_death_messages", true),
        ("spawn_mobs", true),
        ("spawn_monsters", true),
        ("spawn_patrols", true),
        ("spawn_phantoms", true),
        ("spawn_wandering_traders", true),
        ("spawn_wardens", true),
        ("spawner_blocks_work", true),
        ("spectators_generate_chunks", true),
        ("spread_vines", true),
        ("tnt_explodes", true),
        ("tnt_explosion_drop_decay", true),
        ("universal_anger", false),
        ("water_source_conversion", true),
    ];
    let numbers: &[(&str, i64)] = &[
        ("fire_spread_radius_around_player", 0),
        ("max_block_modifications", 32_768),
        ("max_command_forks", 65_536),
        ("max_command_sequence_length", 65_536),
        ("max_entity_cramming", 24),
        ("max_minecart_speed", 8),
        ("max_snow_accumulation_height", 1),
        ("players_nether_portal_creative_delay", 0),
        ("players_nether_portal_default_delay", 80),
        ("players_sleeping_percentage", 100),
        ("random_tick_speed", 3),
        ("respawn_radius", 10),
    ];
    for (name, value) in flags {
        flag(&mut m, name, *value);
    }
    for (name, value) in numbers {
        number(&mut m, name, *value);
    }
    m
}

/// Геймрулы и остальные переменные сервера.
#[derive(Debug)]
pub struct ServerVars {
    rules: Mutex<BTreeMap<String, RuleValue>>,
    /// Точка возрождения мира; `None` — спавн из настроек мира.
    world_spawn: Mutex<Option<[f64; 3]>>,
    /// `advance_weather` не трогает явно выставленную погоду, пока она не сменится.
    weather: Mutex<Weather>,
    /// `setidletimeout`: минуты; 0 — выключено.
    idle_timeout: Mutex<i64>,
    /// Белый список (`white-list` + `/whitelist`).
    whitelist: Mutex<AccessLists>,
    /// `save-off` — автосейв приостановлен.
    saving: Mutex<bool>,
    /// Открыт ли сервер в LAN (`/publish`).
    published: Mutex<Option<u16>>,
    /// Граница мира (`/worldborder`).
    border: Mutex<WorldBorder>,
    path: Option<std::path::PathBuf>,
}

/// Погода: ясная, дождь, гроза, и сколько тиков осталось.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weather {
    pub rain: bool,
    pub thunder: bool,
    /// Тики до смены; 0 — бесконечно, пока не сменят командой.
    pub ticks: i64,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            rain: false,
            thunder: false,
            ticks: 0,
        }
    }
}

/// Белый список и баны.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccessLists {
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub banned_players: Vec<String>,
    #[serde(default)]
    pub banned_ips: Vec<String>,
}

/// Диаметр ванильной границы: `WorldBorder.MAX_SIZE`.
pub const BORDER_MAX_SIZE: f64 = 5.9999968e7;
/// Край мира, за который центр и кромка границы не выходят.
pub const BORDER_MAX_CENTER: f64 = 2.9999984e7;

/// Граница мира. Поля анимации живут только в памяти: на диск пишется итог.
#[derive(Clone, Debug)]
pub struct WorldBorder {
    pub center_x: f64,
    pub center_z: f64,
    /// Диаметр в начале сдвига.
    pub from: f64,
    /// Диаметр в конце сдвига. Без анимации равен `from`.
    pub to: f64,
    /// Игровое время начала сдвига. `lerp_ticks == 0` — граница стоит.
    pub lerp_start: i64,
    pub lerp_ticks: i64,
    pub damage: f32,
    pub buffer: f32,
    pub warning_blocks: i32,
    pub warning_time: i32,
}

impl Default for WorldBorder {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_z: 0.0,
            from: BORDER_MAX_SIZE,
            to: BORDER_MAX_SIZE,
            lerp_start: 0,
            lerp_ticks: 0,
            damage: 0.2,
            buffer: 5.0,
            warning_blocks: 5,
            warning_time: 15,
        }
    }
}

impl WorldBorder {
    /// Диаметр в момент `now`.
    pub fn size_at(&self, now: i64) -> f64 {
        if self.lerp_ticks <= 0 {
            return self.to;
        }
        let elapsed = (now - self.lerp_start).clamp(0, self.lerp_ticks);
        let t = elapsed as f64 / self.lerp_ticks as f64;
        self.from + (self.to - self.from) * t
    }

    /// Снимок для `initialize_border`: старый и новый диаметр и остаток анимации.
    pub fn login(&self, now: i64) -> chunkwake_protocol::play::BorderState {
        let left = if self.lerp_ticks <= 0 {
            0
        } else {
            (self.lerp_ticks - (now - self.lerp_start)).clamp(0, self.lerp_ticks)
        };
        let settled = left == 0;
        chunkwake_protocol::play::BorderState {
            center_x: self.center_x,
            center_z: self.center_z,
            old_size: if settled { self.to } else { self.from },
            new_size: self.to,
            lerp_millis: left.saturating_mul(50),
            warning_blocks: self.warning_blocks,
            warning_time: self.warning_time,
        }
    }
}

/// Что получилось у `/worldborder set|add`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BorderResize {
    Immediate(f64),
    Grow { size: f64, ticks: i64 },
    Shrink { size: f64, ticks: i64 },
    NoChange,
    TooSmall,
    TooBig,
    TooFar,
}

/// Граница, которую пишем в `gamerules.json`. Анимация к этому моменту уже снята.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedBorder {
    #[serde(default)]
    center_x: f64,
    #[serde(default)]
    center_z: f64,
    #[serde(default = "default_border_size")]
    size: f64,
    #[serde(default = "default_border_damage")]
    damage: f32,
    #[serde(default = "default_border_buffer")]
    buffer: f32,
    #[serde(default = "default_warning_blocks")]
    warning_blocks: i32,
    #[serde(default = "default_warning_time")]
    warning_time: i32,
}

impl Default for SavedBorder {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_z: 0.0,
            size: default_border_size(),
            damage: default_border_damage(),
            buffer: default_border_buffer(),
            warning_blocks: default_warning_blocks(),
            warning_time: default_warning_time(),
        }
    }
}

fn default_border_size() -> f64 {
    BORDER_MAX_SIZE
}
fn default_border_damage() -> f32 {
    0.2
}
fn default_border_buffer() -> f32 {
    5.0
}
fn default_warning_blocks() -> i32 {
    5
}
fn default_warning_time() -> i32 {
    15
}

impl From<SavedBorder> for WorldBorder {
    fn from(saved: SavedBorder) -> Self {
        Self {
            center_x: saved.center_x,
            center_z: saved.center_z,
            from: saved.size,
            to: saved.size,
            lerp_start: 0,
            lerp_ticks: 0,
            damage: saved.damage,
            buffer: saved.buffer,
            warning_blocks: saved.warning_blocks,
            warning_time: saved.warning_time,
        }
    }
}

fn border_fits(x: f64, z: f64, diameter: f64) -> bool {
    let half = diameter / 2.0;
    let edge =
        |center: f64| center - half >= -BORDER_MAX_CENTER && center + half <= BORDER_MAX_CENTER;
    edge(x) && edge(z)
}

/// Сохраняемые переменные (без путей и без мьютексов).
#[derive(Serialize, Deserialize)]
struct Saved {
    #[serde(default = "defaults")]
    rules: BTreeMap<String, RuleValue>,
    #[serde(default)]
    world_spawn: Option<[f64; 3]>,
    #[serde(default)]
    access: AccessLists,
    #[serde(default)]
    border: SavedBorder,
}

impl ServerVars {
    /// Без файла: тесты и сервер в памяти.
    pub fn ephemeral() -> Self {
        Self {
            rules: Mutex::new(defaults()),
            world_spawn: Mutex::new(None),
            weather: Mutex::new(Weather::default()),
            idle_timeout: Mutex::new(0),
            whitelist: Mutex::new(AccessLists::default()),
            saving: Mutex::new(true),
            published: Mutex::new(None),
            border: Mutex::new(WorldBorder::default()),
            path: None,
        }
    }

    /// `pvp` из `server.properties`, если файл мира этот ключ ещё не хранит.
    ///
    /// Уже записанное `/gamerule pvp` остаётся. В память пишется сразу, на диск —
    /// вместе со следующим сохранением правил.
    pub fn seed_pvp(&self, enabled: bool) {
        if self.saved_rule_present("pvp") {
            return;
        }
        let mut rules = self.rules.lock().unwrap();
        if let Some(slot) = rules.get_mut("pvp") {
            *slot = RuleValue::Bool(enabled);
        }
    }

    fn saved_rule_present(&self, name: &str) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return false;
        };
        value
            .get("rules")
            .and_then(|rules| rules.get(name))
            .is_some()
    }

    /// Читает `gamerules.json` из каталога мира; нет файла — умолчания.
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join("gamerules.json");
        let saved = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Saved>(&text).ok());
        let mut rules = defaults();
        let (spawn, access, border) = match saved {
            Some(saved) => {
                // Незнакомые правила из файла сохраняем: их могла записать
                // другая версия сервера, и терять их при сохранении нельзя.
                rules.extend(saved.rules);
                (saved.world_spawn, saved.access, saved.border.into())
            }
            None => (None, AccessLists::default(), WorldBorder::default()),
        };
        Self {
            rules: Mutex::new(rules),
            world_spawn: Mutex::new(spawn),
            weather: Mutex::new(Weather::default()),
            idle_timeout: Mutex::new(0),
            whitelist: Mutex::new(access),
            saving: Mutex::new(true),
            published: Mutex::new(None),
            border: Mutex::new(border),
            path: Some(path),
        }
    }

    /// Все правила по алфавиту: `/gamerule` без аргументов и дерево.
    pub fn rule_names(&self) -> Vec<String> {
        self.rules.lock().unwrap().keys().cloned().collect()
    }

    pub fn rule(&self, name: &str) -> Option<RuleValue> {
        self.rules.lock().unwrap().get(name).cloned()
    }

    /// Установка правила: `None` — такого правила нет.
    pub fn set_rule(&self, name: &str, value: RuleValue) -> Option<RuleValue> {
        let mut rules = self.rules.lock().unwrap();
        let slot = rules.get_mut(name)?;
        let old = slot.clone();
        *slot = value;
        drop(rules);
        self.save();
        Some(old)
    }

    /// Удобное чтение флага из тика: без аллокаций.
    pub fn flag(&self, name: &str) -> bool {
        self.rules
            .lock()
            .unwrap()
            .get(name)
            .is_some_and(RuleValue::as_bool)
    }

    pub fn number(&self, name: &str) -> i64 {
        self.rules
            .lock()
            .unwrap()
            .get(name)
            .map_or(0, RuleValue::as_int)
    }

    pub fn weather(&self) -> Weather {
        *self.weather.lock().unwrap()
    }

    pub fn set_weather(&self, weather: Weather) {
        *self.weather.lock().unwrap() = weather;
    }

    /// Тик погоды: `advance_weather` гасит дождь, когда вышло время.
    pub fn tick_weather(&self) {
        if !self.flag("advance_weather") {
            return;
        }
        let mut weather = self.weather.lock().unwrap();
        if weather.ticks > 0 {
            weather.ticks -= 1;
            if weather.ticks == 0 {
                weather.rain = false;
                weather.thunder = false;
            }
        }
    }

    pub fn world_spawn(&self) -> Option<[f64; 3]> {
        *self.world_spawn.lock().unwrap()
    }

    pub fn set_world_spawn(&self, pos: [f64; 3]) {
        *self.world_spawn.lock().unwrap() = Some(pos);
        self.save();
    }

    pub fn idle_timeout(&self) -> i64 {
        *self.idle_timeout.lock().unwrap()
    }

    pub fn set_idle_timeout(&self, minutes: i64) {
        *self.idle_timeout.lock().unwrap() = minutes;
    }

    pub fn access(&self) -> AccessLists {
        self.whitelist.lock().unwrap().clone()
    }

    /// Изменение списков: `Some(true)` — добавить, `Some(false)` — убрать.
    pub fn update_access(&self, list: AccessList, name: &str, add: Option<bool>) -> bool {
        let mut access = self.whitelist.lock().unwrap();
        let names = match list {
            AccessList::Whitelist => &mut access.whitelist,
            AccessList::BannedPlayers => &mut access.banned_players,
            AccessList::BannedIps => &mut access.banned_ips,
        };
        let present = names.iter().any(|n| n.eq_ignore_ascii_case(name));
        let changed = match add {
            Some(true) if !present => {
                names.push(name.to_owned());
                true
            }
            Some(false) if present => {
                names.retain(|n| !n.eq_ignore_ascii_case(name));
                true
            }
            Some(true) | Some(false) => false,
            None => false,
        };
        drop(access);
        if changed {
            self.save();
        }
        changed
    }

    pub fn is_allowed(&self, list: AccessList, name: &str) -> bool {
        let access = self.whitelist.lock().unwrap();
        let names = match list {
            AccessList::Whitelist => &access.whitelist,
            AccessList::BannedPlayers => &access.banned_players,
            AccessList::BannedIps => &access.banned_ips,
        };
        names.iter().any(|n| n.eq_ignore_ascii_case(name))
    }

    pub fn saving(&self) -> bool {
        *self.saving.lock().unwrap()
    }

    pub fn set_saving(&self, on: bool) {
        *self.saving.lock().unwrap() = on;
    }

    pub fn published(&self) -> Option<u16> {
        *self.published.lock().unwrap()
    }

    pub fn set_published(&self, port: Option<u16>) {
        *self.published.lock().unwrap() = port;
    }

    pub fn border(&self) -> WorldBorder {
        self.border.lock().unwrap().clone()
    }

    pub fn border_at(&self, now: i64) -> WorldBorder {
        let mut border = self.border.lock().unwrap().clone();
        if border.lerp_ticks > 0 && now - border.lerp_start >= border.lerp_ticks {
            border.from = border.to;
            border.lerp_ticks = 0;
        }
        border
    }

    /// `/worldborder set|add`. `adding` прибавляет расстояние к текущему диаметру.
    pub fn resize_border(&self, distance: f64, ticks: i64, now: i64, adding: bool) -> BorderResize {
        let mut border = self.border.lock().unwrap();
        let current = border.size_at(now);
        let next = if adding { current + distance } else { distance };
        if next < 1.0 {
            return BorderResize::TooSmall;
        }
        if next > BORDER_MAX_SIZE {
            return BorderResize::TooBig;
        }
        if !border_fits(border.center_x, border.center_z, next) {
            return BorderResize::TooFar;
        }
        if ticks <= 0 && (next - current).abs() < 1e-6 {
            return BorderResize::NoChange;
        }
        if ticks <= 0 {
            border.from = next;
            border.to = next;
            border.lerp_ticks = 0;
            drop(border);
            self.save();
            return BorderResize::Immediate(next);
        }
        border.from = current;
        border.to = next;
        border.lerp_start = now;
        border.lerp_ticks = ticks;
        drop(border);
        self.save();
        if next > current {
            BorderResize::Grow { size: next, ticks }
        } else {
            BorderResize::Shrink { size: next, ticks }
        }
    }

    /// `/worldborder center`. `None` — центр или кромка выходят за мир.
    pub fn set_border_center(&self, x: f64, z: f64, now: i64) -> bool {
        if x.abs() > BORDER_MAX_CENTER || z.abs() > BORDER_MAX_CENTER {
            return false;
        }
        let mut border = self.border.lock().unwrap();
        let size = border.size_at(now).max(border.to);
        if !border_fits(x, z, size) {
            return false;
        }
        if (border.center_x - x).abs() < 1e-4 && (border.center_z - z).abs() < 1e-4 {
            return false;
        }
        border.center_x = x;
        border.center_z = z;
        drop(border);
        self.save();
        true
    }

    /// Урон и буфер границы. `false` — значение уже такое.
    pub fn set_border_damage(&self, per_block: Option<f32>, buffer: Option<f32>) -> bool {
        let mut border = self.border.lock().unwrap();
        let mut changed = false;
        if let Some(value) = per_block
            && (border.damage - value).abs() > 1e-6
        {
            border.damage = value;
            changed = true;
        }
        if let Some(value) = buffer
            && (border.buffer - value).abs() > 1e-6
        {
            border.buffer = value;
            changed = true;
        }
        drop(border);
        if changed {
            self.save();
        }
        changed
    }

    /// Предупреждение границы. `false` — значение уже такое.
    pub fn set_border_warning(&self, blocks: Option<i32>, time: Option<i32>) -> bool {
        let mut border = self.border.lock().unwrap();
        let mut changed = false;
        if let Some(value) = blocks
            && border.warning_blocks != value
        {
            border.warning_blocks = value;
            changed = true;
        }
        if let Some(value) = time
            && border.warning_time != value
        {
            border.warning_time = value;
            changed = true;
        }
        drop(border);
        if changed {
            self.save();
        }
        changed
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let border = self.border.lock().unwrap();
        let saved_border = SavedBorder {
            center_x: border.center_x,
            center_z: border.center_z,
            size: border.to,
            damage: border.damage,
            buffer: border.buffer,
            warning_blocks: border.warning_blocks,
            warning_time: border.warning_time,
        };
        drop(border);
        let saved = Saved {
            rules: self.rules.lock().unwrap().clone(),
            world_spawn: *self.world_spawn.lock().unwrap(),
            access: self.whitelist.lock().unwrap().clone(),
            border: saved_border,
        };
        match serde_json::to_string_pretty(&saved) {
            Ok(text) => {
                if let Err(e) = std::fs::write(path, text) {
                    tracing::error!("write {}: {e}", path.display());
                }
            }
            Err(e) => tracing::error!("gamerules: {e}"),
        }
    }
}

/// Какой список смотреть в `white-list`/`ban`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessList {
    Whitelist,
    BannedPlayers,
    BannedIps,
}

/// Узлы правил для дерева `/gamerule`: имя и тип значения.
///
/// Тип берётся из умолчания (`Bool` → `brigadier:bool`, `Int` →
/// `brigadier:integer`), поэтому дерево не может разойтись с набором правил.
pub fn rule_specs() -> Vec<(String, chunkwake_protocol::text::Parser)> {
    defaults()
        .into_iter()
        .map(|(name, value)| {
            let parser = match value {
                RuleValue::Bool(_) => chunkwake_protocol::text::Parser::Bool,
                RuleValue::Int(_) => {
                    chunkwake_protocol::text::Parser::Integer(i64::MIN as i32, i32::MAX)
                }
                RuleValue::Text(_) => chunkwake_protocol::text::Parser::String(
                    chunkwake_protocol::text::StringMode::Word,
                ),
            };
            (name, parser)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_specs_match_defaults() {
        let specs = rule_specs();
        assert_eq!(specs.len(), defaults().len());
        let by_name: std::collections::HashMap<_, _> = specs.iter().cloned().collect();
        assert!(matches!(
            by_name["keep_inventory"],
            chunkwake_protocol::text::Parser::Bool
        ));
        assert!(matches!(
            by_name["random_tick_speed"],
            chunkwake_protocol::text::Parser::Integer(..)
        ));
    }

    #[test]
    fn defaults_match_game_version() {
        let vars = ServerVars::ephemeral();
        assert!(vars.flag("advance_time"));
        assert!(!vars.flag("keep_inventory"));
        assert_eq!(vars.number("random_tick_speed"), 3);
        assert_eq!(vars.number("max_entity_cramming"), 24);
        assert_eq!(vars.number("respawn_radius"), 10);
        assert!(!vars.rule("doDaylightCycle").is_some(), "старых имён нет");
        let names = vars.rule_names();
        // Имена и число правил сверены с `reports/commands.json` 26.3.
        assert_eq!(names.len(), 59, "{names:?}");
        assert!(names.windows(2).all(|w| w[0] < w[1]), "по алфавиту");
    }

    #[test]
    fn set_rule_keeps_type() {
        let vars = ServerVars::ephemeral();
        let old = vars
            .set_rule("keep_inventory", RuleValue::Bool(true))
            .unwrap();
        assert_eq!(old, RuleValue::Bool(false));
        assert!(vars.flag("keep_inventory"));
        assert!(
            vars.set_rule("nope", RuleValue::Bool(true)).is_none(),
            "незнакомое правило не создаётся"
        );
        assert_eq!(RuleValue::Int(3).parse_like("7"), Some(RuleValue::Int(7)));
        assert_eq!(
            RuleValue::Bool(false).parse_like("yes"),
            None,
            "флаг принимает только true/false"
        );
    }

    #[test]
    fn access_lists_add_and_remove() {
        let vars = ServerVars::ephemeral();
        assert!(vars.update_access(AccessList::Whitelist, "Alice", Some(true)));
        assert!(!vars.update_access(AccessList::Whitelist, "Alice", Some(true)));
        assert!(vars.is_allowed(AccessList::Whitelist, "alice"));
        assert!(vars.update_access(AccessList::Whitelist, "Alice", Some(false)));
        assert!(!vars.is_allowed(AccessList::Whitelist, "Alice"));
    }

    #[test]
    fn weather_clears_after_countdown() {
        let vars = ServerVars::ephemeral();
        vars.set_weather(Weather {
            rain: true,
            thunder: false,
            ticks: 2,
        });
        vars.tick_weather();
        assert!(vars.weather().rain);
        vars.tick_weather();
        assert!(!vars.weather().rain);
    }

    #[test]
    fn round_trip_through_file() {
        let dir = std::env::temp_dir().join(format!("chunkwake-vars-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        {
            let vars = ServerVars::load(&dir);
            vars.set_rule("keep_inventory", RuleValue::Bool(true));
            vars.set_world_spawn([1.0, 2.0, 3.0]);
            vars.update_access(AccessList::BannedPlayers, "Griefer", Some(true));
        }
        let vars = ServerVars::load(&dir);
        assert!(vars.flag("keep_inventory"));
        assert_eq!(vars.world_spawn(), Some([1.0, 2.0, 3.0]));
        assert!(vars.is_allowed(AccessList::BannedPlayers, "Griefer"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pvp_property_seeds_only_when_the_file_has_no_key() {
        let dir = std::env::temp_dir().join(format!(
            "chunkwake-pvp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let vars = ServerVars::load(&dir);
        vars.seed_pvp(false);
        assert!(!vars.flag("pvp"));
        vars.set_rule("keep_inventory", RuleValue::Bool(true));
        let vars = ServerVars::load(&dir);
        vars.seed_pvp(true);
        assert!(
            !vars.flag("pvp"),
            "сохранённое правило важнее server.properties"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
