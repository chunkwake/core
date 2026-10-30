//! Доступ обработчиков команд к миру.
//!
//! Команды не лезут в приватные поля сами: изменяющее действие почти всегда
//! принадлежит сессии игрока (инвентарь, режим игры, эффекты), поэтому здесь
//! собраны тонкие обёртки над уже существующими механизмами — [`Control`],
//! `set_block`, `spawn_mob`. Так команда из консоли и команда из чата ведут
//! себя одинаково: обе идут через тот же канал.

use super::selector::Target;
use crate::live::Control;
use crate::play::PlayPlan;
use chunkwake_world::BlockEntity;
use chunkwake_world::nbt::{Compound, Tag};
use std::collections::{HashMap, HashSet};

/// `replace <filter>`: имя блока или `#тег` из каталога блоков.
fn block_filter_matches(name: &str, filter: &str, tags: &HashMap<String, HashSet<String>>) -> bool {
    BlockFilter::new(filter, tags).matches(name)
}

/// Разобранный фильтр `/fill … replace`: сравнение на каждый блок без `format!`.
enum BlockFilter<'a> {
    Tag(&'a HashSet<String>),
    Name(String),
}

impl<'a> BlockFilter<'a> {
    fn new(filter: &str, tags: &'a HashMap<String, HashSet<String>>) -> Self {
        if let Some(tag) = filter.strip_prefix('#') {
            let tag = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            // Нет тега — фильтр не совпадает ни с чем.
            return match tags.get(&tag) {
                Some(set) => Self::Tag(set),
                None => Self::Name(String::new()),
            };
        }
        let bare = filter.split('[').next().unwrap_or(filter);
        Self::Name(if bare.contains(':') {
            bare.to_owned()
        } else {
            format!("minecraft:{bare}")
        })
    }

    fn matches(&self, name: &str) -> bool {
        match self {
            Self::Tag(set) => set.contains(name),
            Self::Name(want) => !want.is_empty() && name == want,
        }
    }
}

fn is_air_name(name: &str) -> bool {
    matches!(
        name,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    )
}

fn boxes_overlap(a0: [i32; 3], a1: [i32; 3], b0: [i32; 3], b1: [i32; 3]) -> bool {
    a0[0] <= b1[0]
        && a1[0] >= b0[0]
        && a0[1] <= b1[1]
        && a1[1] >= b0[1]
        && a0[2] <= b1[2]
        && a1[2] >= b0[2]
}

fn sequence_seed(world: i64, name: &str, salt: i64, include_world: bool, include_id: bool) -> u64 {
    let mut hash = salt as u64;
    if include_world {
        hash ^= world as u64;
    }
    if include_id {
        for byte in name.as_bytes() {
            hash = hash
                .wrapping_mul(0x100000001b3)
                .wrapping_add(u64::from(*byte));
        }
    }
    if hash == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        hash
    }
}

fn pick_int(state: &mut u64, lo: i32, hi: i32) -> i32 {
    let mut x = *state;
    if x == 0 {
        x = 0x9E37_79B9_7F4A_7C15;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    let span = u64::try_from(i64::from(hi) - i64::from(lo) + 1).unwrap_or(1);
    lo.saturating_add((x % span) as i32)
}
use std::sync::atomic::Ordering;
use uuid::Uuid;

impl PlayPlan {
    /// Координаты для телепорта командой (/tp).
    pub(crate) fn teleport_to(&self, uuid: Uuid, pos: [f64; 3], yaw: f32, pitch: f32) -> bool {
        self.teleport_player(uuid, pos, yaw, pitch)
    }

    /// `/locate biome`: ближайшая точка или `None` (плоский мир, нет такого биома).
    pub(crate) fn locate_biome(&self, name: &str, origin: [i32; 3]) -> Option<[i32; 3]> {
        self.locate_biome.as_ref().and_then(|f| f(name, origin))
    }

    /// `/locate structure`: ближайший старт или `None` (плоский мир, такой структуры нет).
    pub(crate) fn locate_structure(&self, name: &str, origin: [i32; 3]) -> Option<[i32; 3]> {
        self.locate_structure.as_ref().and_then(|f| f(name, origin))
    }

    /// Все цели мира для селекторов: игроки, мобы, выпавшие предметы, стрелы.
    pub(crate) fn command_targets(&self) -> Vec<Target> {
        // Точная ёмкость: иначе вектор растёт и копирует цели два-три раза.
        let players = self.hub.players.lock().unwrap().len();
        let mobs = self.mobs.lock().unwrap().len();
        let dropped = self.dropped.lock().unwrap().len();
        let mut out: Vec<Target> = Vec::with_capacity(players + mobs + dropped);
        out.extend(self.hub.players.lock().unwrap().values().map(|p| Target {
            uuid: p.entry.uuid,
            name: p.entry.name.clone(),
            entity_id: p.entity_id,
            pos: p.pos,
            yaw: p.yaw,
            pitch: p.pitch,
            game_mode: Some(0),
            tags: Vec::new(),
        }));
        // Режим игры берём из зеркала: в записи таб-листа его нет.
        {
            let live = self.live.lock().unwrap();
            for target in &mut out {
                if let Some(l) = live.get(&target.uuid) {
                    target.game_mode = Some(l.game_mode);
                }
            }
        }
        // Мобы и предметы: только позиция и вид — для `@e`, `kill`, `tp`.
        {
            let mobs = self.mobs.lock().unwrap();
            for (entity_id, entry) in mobs.iter() {
                out.push(entity_ref(
                    *entity_id,
                    entry.mob.pos,
                    entry.mob.species.name(),
                ));
            }
        }
        {
            let dropped = self.dropped.lock().unwrap();
            for (entity_id, item) in dropped.iter() {
                out.push(entity_ref(*entity_id, item.pos, "item"));
            }
        }
        let tags = self.entity_tags.lock().unwrap();
        for target in &mut out {
            if let Some(held) = tags.get(&target.uuid) {
                target.tags.clone_from(held);
            }
        }
        out
    }

    /// Одна цель по UUID без сборки всего снимка мира.
    ///
    /// Полный снимок перебирает игроков, мобов и предметы: для `@s` и
    /// «цель по UUID» это лишняя работа на каждую команду.
    pub(crate) fn command_target(&self, uuid: Uuid) -> Option<Target> {
        if let Some(player) = self.hub.players.lock().unwrap().get(&uuid) {
            let game_mode = self
                .live
                .lock()
                .unwrap()
                .get(&uuid)
                .map_or(Some(0), |live| Some(live.game_mode));
            let tags = self
                .entity_tags
                .lock()
                .unwrap()
                .get(&uuid)
                .cloned()
                .unwrap_or_default();
            return Some(Target {
                uuid,
                name: player.entry.name.clone(),
                entity_id: player.entity_id,
                pos: player.pos,
                yaw: player.yaw,
                pitch: player.pitch,
                game_mode,
                tags,
            });
        }
        let mob = {
            let mobs = self.mobs.lock().unwrap();
            mobs.iter()
                .find(|(entity_id, _)| entity_uuid(**entity_id) == uuid)
                .map(|(entity_id, entry)| (*entity_id, entry.mob.pos, entry.mob.species.name()))
        };
        let (entity_id, pos, name) = mob?;
        let mut target = entity_ref(entity_id, pos, name);
        if let Some(held) = self.entity_tags.lock().unwrap().get(&uuid) {
            target.tags.clone_from(held);
        }
        Some(target)
    }

    /// UUID игрока по имени без сборки снимка мира.
    pub(crate) fn player_uuid_by_name(&self, name: &str) -> Option<Uuid> {
        self.hub
            .players
            .lock()
            .unwrap()
            .values()
            .find(|p| p.entry.name.eq_ignore_ascii_case(name))
            .map(|p| p.entry.uuid)
    }

    /// Игроки онлайн для `/list`.
    pub(crate) fn online_players(&self) -> Vec<Target> {
        self.command_targets()
            .into_iter()
            .filter(|t| t.game_mode.is_some())
            .collect()
    }

    /// Предел игроков из `max-players`.
    pub(crate) fn max_players(&self) -> usize {
        self.login.max_players.max(0) as usize
    }

    /// Зерно мира для `/seed`.
    pub(crate) fn seed(&self) -> i64 {
        self.seed
    }

    pub(crate) fn day_time(&self) -> i64 {
        self.day_time.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn game_time(&self) -> i64 {
        self.game_time.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// `/time set`: ставит время суток и рассылает всем.
    pub(crate) fn set_day_time(&self, ticks: i64) {
        self.day_time.store(
            ticks.rem_euclid(24_000),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.broadcast_world_time();
    }

    /// `/time add`: сдвигает время суток, возвращает новое.
    pub(crate) fn set_day_time_relative(&self, delta: i64) -> i64 {
        let now = self
            .day_time
            .try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |d| Some((d + delta).rem_euclid(24_000)),
            )
            .map(|old| (old + delta).rem_euclid(24_000))
            .unwrap_or(0);
        self.broadcast_world_time();
        now
    }

    /// Убирает и раскладывает предметы (`/give`), вместе с компонентами.
    pub(crate) fn give_item(
        &self,
        uuid: Uuid,
        item: i32,
        count: i32,
        extra: u32,
        damage: i32,
    ) -> bool {
        if !self.live.lock().unwrap().contains_key(&uuid) {
            return false;
        }
        self.push_control(
            uuid,
            Control::Give {
                item,
                count,
                extra,
                damage,
            },
        );
        true
    }

    /// Очищает инвентарь; возвращает, сколько предметов было (по зеркалу).
    pub(crate) fn clear_inventory(&self, uuid: Uuid) -> i32 {
        let count = self.live.lock().unwrap().get(&uuid).map_or(0, |l| {
            l.slots
                .iter()
                .flatten()
                .map(|s| s.count as i32)
                .sum::<i32>()
        });
        self.push_control(uuid, Control::Clear);
        count
    }

    /// Снимает до `max` штук предметов из списка id.
    ///
    /// `extra == 0` и `damage == None` — любые компоненты. Иначе снимаются
    /// стопки, у которых заданные компоненты совпали.
    pub(crate) fn clear_items(
        &self,
        uuid: Uuid,
        items: &[i32],
        extra: u32,
        damage: Option<i32>,
        max: i32,
    ) -> i32 {
        let removed = self.live.lock().unwrap().get(&uuid).map_or(0, |l| {
            crate::survival::query_count(&l.slots, Some(items), extra, damage, max)
        });
        self.push_control(
            uuid,
            Control::ClearItem {
                items: items.to_vec(),
                extra,
                damage,
                max,
            },
        );
        removed
    }

    /// `/kill`: игрока убивает сессия, сущность убираем из мира.
    pub(crate) fn kill_target(&self, target: &Target) -> bool {
        if target.game_mode.is_some() {
            return self.push_control(target.uuid, Control::Health(0.0));
        }
        self.remove_entity(target.entity_id)
    }

    /// Накладывает эффект (`/effect give`). `particles` — `hideParticles` наоборот.
    pub(crate) fn give_effect(
        &self,
        uuid: Uuid,
        id: i32,
        amplifier: u8,
        duration: i32,
        particles: bool,
    ) -> bool {
        self.push_control(
            uuid,
            Control::Effect {
                effect: crate::survival::Effect {
                    id,
                    amplifier,
                    duration,
                    ambient: false,
                    particles,
                    show_icon: true,
                    age: 0,
                },
                force: false,
                command: true,
            },
        )
    }

    /// Снимает эффекты (`/effect clear`).
    pub(crate) fn clear_effects(&self, uuid: Uuid, id: Option<i32>, command: bool) -> bool {
        self.push_control(uuid, Control::ClearEffect { id, command })
    }

    /// Опыт игрока по зеркалу: `(уровень, очки)`.
    pub(crate) fn xp_of(&self, uuid: Uuid) -> Option<(i32, i32)> {
        self.live
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|l| (l.xp.level, l.xp.points()))
    }

    /// Причина отказа во входе: бан игрока, бан IP или белый список.
    /// Оператор проходит белый список. `None` — пускать.
    pub fn admission_denied(
        &self,
        name: &str,
        uuid: Uuid,
        addr: std::net::SocketAddr,
    ) -> Option<String> {
        use super::state::AccessList;
        let ip = addr.ip().to_string();
        if self.vars.is_allowed(AccessList::BannedIps, &ip) {
            return Some("Your IP address is banned from this server!".into());
        }
        if self.vars.is_allowed(AccessList::BannedPlayers, name) {
            return Some("You are banned from this server.".into());
        }
        if self.whitelist_enabled()
            && self.op_level(uuid) == 0
            && !self.vars.is_allowed(AccessList::Whitelist, name)
        {
            return Some("You are not white-listed on this server!".into());
        }
        None
    }

    /// Точка возрождения игрока (`/spawnpoint`); `None` — сброс на спавн мира.
    ///
    /// Пишется и в память сессии, и в сохранение, если игрок уже есть на диске.
    pub(crate) fn set_player_spawn(&self, uuid: Uuid, pos: Option<[i32; 3]>) {
        self.spawn_points.lock().unwrap().insert(uuid, pos);
        if let Some(save) = self.player_saves.lock().unwrap().get_mut(&uuid) {
            save.spawn = pos;
        }
    }

    /// Режим игры по умолчанию для новых игроков (`/defaultgamemode`).
    pub(crate) fn set_default_game_mode(&self, mode: u8) {
        self.default_game_mode
            .store(mode.min(3), std::sync::atomic::Ordering::Relaxed);
    }

    /// Текущий режим игры по умолчанию (для новых игроков).
    #[allow(dead_code)]
    pub(crate) fn default_game_mode(&self) -> u8 {
        self.default_game_mode
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Сколько столбцов знает мир: изменённые и прочитанные с диска.
    pub(crate) fn loaded_columns(&self) -> usize {
        self.edited.read().unwrap().len().max(self.regions_loaded())
    }

    /// Просьба остановить сервер (`/stop`).
    pub(crate) fn request_shutdown(&self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Запрошена ли остановка.
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Загрузка процесса, снятая сэмплером сервера.
    pub fn set_proc_stats(&self, stats: chunkwake_metrics::ProcStats) {
        *self.proc_stats.lock().unwrap() = stats;
    }

    /// Настройки сервера для команд: правила, мир, каталог `/reload`.
    ///
    /// `ServerVars` живут в файле мира, поэтому читаются здесь, а не в
    /// [`PlayPlan::build`]: build не знает ни каталога мира, ни
    /// `server.properties`. Вызывается до первого тика и до первого игрока.
    pub fn configure(&mut self, props: &super::PlanConfig<'_>, world_dir: &std::path::Path) {
        self.vars = super::state::ServerVars::load(world_dir);
        self.level_name = props.level_name.to_owned();
        self.motd = props.motd.to_owned();
        self.seed = props.seed;
        self.version_name = props.version_name.to_owned();
        self.version_protocol = props.version_protocol;
        self.reload_dir = Some(props.server_dir.to_owned());
        self.debug_dir = world_dir.to_owned();
        self.whitelist_enabled
            .store(props.whitelist, std::sync::atomic::Ordering::Relaxed);
        self.allow_flight
            .store(props.allow_flight, std::sync::atomic::Ordering::Relaxed);
        self.vars.seed_pvp(props.pvp);
    }

    /// Учёт входящего пакета play-соединения.
    pub fn count_bytes_in(&self, bytes: u64) {
        self.net_stats.add_in(bytes);
    }

    /// Учёт исходящей пачки play-соединения.
    pub fn count_bytes_out(&self, bytes: u64, packets: u64) {
        self.net_stats.add_out(bytes, packets);
    }

    /// Пинг игрока по RTT `keep_alive`.
    pub fn set_ping(&self, uuid: Uuid, millis: f32) {
        self.pings.lock().unwrap().insert(uuid, millis);
    }

    /// Забыть пинга при выходе игрока.
    pub fn forget_ping(&self, uuid: Uuid) {
        self.pings.lock().unwrap().remove(&uuid);
    }

    /// Замер тика, снятый снаружи (лаборатория, тесты).
    pub fn record_tick_sample(&self, work_ms: f64, gap_ms: f64) {
        self.ticks.lock().unwrap().record(work_ms, gap_ms);
    }

    /// `/tick rate`.
    pub(crate) fn set_tick_rate(&self, rate: i32) {
        self.ticks.lock().unwrap().set_rate(f64::from(rate));
    }

    /// `/tick freeze` и `unfreeze`.
    pub(crate) fn set_tick_frozen(&self, frozen: bool) {
        self.ticks.lock().unwrap().set_frozen(frozen);
    }

    /// `/tick step`.
    pub(crate) fn request_steps(&self, ticks: u64) {
        self.ticks.lock().unwrap().request_steps(ticks);
    }

    /// `/tick sprint`.
    pub(crate) fn start_sprint(&self, ticks: u64) {
        self.ticks.lock().unwrap().start_sprint(ticks);
    }

    /// `/tick sprint stop`: сколько успело пройти.
    pub(crate) fn stop_sprint(&self) -> Option<u64> {
        self.ticks.lock().unwrap().stop_sprint()
    }

    /// `/gamerule`: чтение всех имён и значений.
    pub(crate) fn gamerules(&self) -> Vec<(String, super::state::RuleValue)> {
        self.vars
            .rule_names()
            .into_iter()
            .filter_map(|n| self.vars.rule(&n).map(|v| (n, v)))
            .collect()
    }

    /// Состояние блока по спецификации (`stone`, `oak_stairs[facing=east]`).
    pub(crate) fn parse_block_state(&self, spec: &str) -> Option<u32> {
        let (name, properties) = match spec.split_once('[') {
            Some((name, tail)) => (name, Some(tail.strip_suffix(']')?)),
            None => (spec, None),
        };
        let name = super::args::resource_name(name);
        let mut state = self.survival.placement.default_state(&name)?;
        for pair in properties.into_iter().flat_map(|p| p.split(',')) {
            let (key, value) = pair.split_once('=')?;
            state = self
                .survival
                .placement
                .with_prop(state, key.trim(), value.trim())?;
        }
        Some(state)
    }

    /// Внутри ли мира позиция (по высоте и границам).
    pub(crate) fn in_world(&self, [x, y, z]: [i32; 3]) -> bool {
        let height = self.template.height() as i32;
        y >= self.min_y && y < self.min_y + height && x.abs() < 30_000_000 && z.abs() < 30_000_000
    }

    /// Ставит блок и будит редстоун; `None` — вне мира, `Some(false)` — не изменился.
    pub(crate) fn place_block(&self, pos: [i32; 3], state: u32) -> Option<bool> {
        let old = self.set_block(pos, state)?;
        self.redstone_update(&crate::redstone::around(pos));
        Some(old != state)
    }

    /// Имя блока в позиции (`/execute if block`).
    pub fn block_name_at(&self, pos: [i32; 3]) -> Option<String> {
        self.block(pos)
            .map(|state| self.state_name(state).to_owned())
    }

    /// Состояние блока. `None` — вне мира или чанк шума ещё не в памяти.
    pub(crate) fn block_state_at(&self, pos: [i32; 3]) -> Option<u32> {
        self.block(pos)
    }

    /// Имя биома клетки 4×4×4. Плоский мир читает шаблон. Чанка шума нет — `None`.
    pub(crate) fn biome_name_at(&self, pos: [i32; 3]) -> Option<String> {
        let ry = pos[1] - self.min_y;
        if ry < 0 || ry as usize >= self.template().height() {
            return None;
        }
        let id = with_column(self, pos[0], pos[2], |column| {
            let y = ry as usize;
            let section = column.sections.get(y / 16)?;
            Some(section.biome_at(
                (pos[0] & 15) as usize / 4,
                (y % 16) / 4,
                (pos[2] & 15) as usize / 4,
            ))
        })??;
        self.survival.biome_names.get(id as usize).cloned()
    }

    /// Чанк уже материализован, симулируется или стоит в `/forceload`.
    pub(crate) fn chunk_resident(&self, x: i32, z: i32) -> bool {
        let key = (x >> 4, z >> 4);
        self.edited.read().unwrap().contains_key(&key)
            || self.forced.lock().unwrap().contains(&key)
            || self.simulated.lock().unwrap().contains(&key)
    }

    /// Мировая Y карты высот на X/Z.
    pub(crate) fn height_over(&self, x: f64, z: f64, kind: &str) -> ColumnHeight {
        let bare = kind.strip_prefix("minecraft:").unwrap_or(kind);
        let ocean = bare == "ocean_floor";
        let mask = match bare {
            "world_surface" => 1u8,
            "motion_blocking" => 2,
            "motion_blocking_no_leaves" => 4,
            "ocean_floor" => 2,
            _ => return ColumnHeight::Unknown,
        };
        let (ix, iz) = (x.floor() as i32, z.floor() as i32);
        let Some(relative) = with_column(self, ix, iz, |column| {
            column_top(column, (ix & 15) as usize, (iz & 15) as usize, |state| {
                let bits = chunkwake_world::height_bits(state);
                bits & mask != 0 && (!ocean || !chunkwake_world::is_fluid_state(state))
            })
        }) else {
            return ColumnHeight::Missing;
        };
        ColumnHeight::Y(f64::from(self.min_y) + f64::from(relative))
    }

    /// Стопки окна игрока. Нет игрока — пусто.
    pub(crate) fn player_windows(
        &self,
        uuid: Uuid,
        windows: &[usize],
    ) -> Vec<chunkwake_protocol::inventory::Slot> {
        let live = self.live.lock().unwrap();
        let Some(live) = live.get(&uuid) else {
            return Vec::new();
        };
        windows
            .iter()
            .filter_map(|&window| live.slots.get(window).copied())
            .collect()
    }

    /// Окно выбранной ячейки хотбара.
    pub(crate) fn held_window(&self, uuid: Uuid) -> Option<usize> {
        let live = self.live.lock().unwrap();
        let live = live.get(&uuid)?;
        Some(36 + live.selected.min(8))
    }

    /// Сколько живых мобов в мире (`/top` и тесты).
    pub fn mob_count(&self) -> usize {
        self.mobs.lock().unwrap().len()
    }

    /// Точка появления мира для тестов и команд.
    pub fn spawn_point(&self) -> [f64; 3] {
        self.vars.world_spawn().unwrap_or(self.spawn)
    }

    /// Правила сервера: чтение из тестов и `/top`.
    pub fn server_vars(&self) -> &super::state::ServerVars {
        &self.vars
    }

    /// Название текущей сложности (`/difficulty` и тесты).
    pub fn difficulty_name(&self) -> &'static str {
        match self.difficulty() {
            crate::survival::Difficulty::Peaceful => "peaceful",
            crate::survival::Difficulty::Easy => "easy",
            crate::survival::Difficulty::Normal => "normal",
            crate::survival::Difficulty::Hard => "hard",
        }
    }

    /// Спавн моба командой (`/summon`).
    pub(crate) fn summon_mob(&self, species: crate::mobs::Species, pos: [f64; 3]) -> Option<i32> {
        self.spawn_mob(species, pos, 0.0, false, Some("COMMAND"))
    }

    /// Убирает сущность из мира (`/kill` для моба или предмета).
    pub(crate) fn remove_entity(&self, entity_id: i32) -> bool {
        let removed = self.mobs.lock().unwrap().remove(&entity_id).is_some()
            | self.dropped.lock().unwrap().remove(&entity_id).is_some()
            | self.arrows.lock().unwrap().remove(&entity_id).is_some();
        if removed {
            let packet =
                chunkwake_protocol::play::remove_entities(self.ids.remove_entities, &[entity_id]);
            self.hub.send_entity(
                None,
                packet,
                Some(crate::play::EntityEvent::Gone(entity_id)),
            );
        }
        removed
    }

    /// `ops.json`: выдать или снять права (`/op`, `/deop`).
    ///
    /// Встроенные команды идут через [`PlayPlan::op_command`]: там же
    /// обновление дерева у клиента. Метод оставлен для плагинов.
    #[allow(dead_code)]
    pub(crate) fn set_op_level(&self, uuid: Uuid, grant: bool) -> bool {
        let before = self.op_level(uuid);
        self.set_op(uuid, grant);
        self.op_level(uuid) != before
    }

    /// Сколько чанков держит `/forceload`.
    pub(crate) fn forced_chunks(&self) -> usize {
        self.forced.lock().unwrap().len()
    }

    /// Ставит или снимает `forceload` на чанк под точкой; возвращает число чанков.
    pub(crate) fn force_load_here(&self, pos: [f64; 3], add: bool) -> usize {
        let chunk = (pos[0].floor() as i32 >> 4, pos[2].floor() as i32 >> 4);
        let mut forced = self.forced.lock().unwrap();
        if add {
            forced.insert(chunk);
        } else {
            forced.remove(&chunk);
        }
        forced.len()
    }

    /// Сложность мира (`/difficulty`).
    pub(crate) fn set_difficulty(&self, level: u8) {
        self.survival
            .difficulty
            .store(level.min(3), Ordering::Relaxed);
        self.hub.send(
            None,
            chunkwake_protocol::play::change_difficulty(self.ids.change_difficulty, level.min(3)),
        );
    }

    /// Версия игры для `/version`.
    pub(crate) fn version(&self) -> (String, i32) {
        (self.version_name.clone(), self.version_protocol)
    }

    /// Команды плагинов для дерева и подсказок.
    pub(crate) fn plugin_commands(&self) -> Vec<String> {
        self.plugin_commands.lock().unwrap().clone()
    }

    /// ID пакета `commands`.
    pub(crate) fn commands_id(&self) -> i32 {
        self.ids.commands
    }

    /// Таблица парсеров аргументов.
    pub(crate) fn parsers(&self) -> &chunkwake_protocol::text::CommandParsers {
        &self.parsers
    }

    /// Игроки по имени или селектору (`/op`, `/ban`).
    pub(crate) fn command_targets_named(&self, token: &str) -> Option<Vec<Target>> {
        if token.starts_with('@') {
            let selector = super::selector::Selector::parse(token).ok()?;
            return selector
                .apply(None, &self.command_targets(), |t| t.tags.clone())
                .ok();
        }
        let players: Vec<Target> = self
            .command_targets()
            .into_iter()
            .filter(|t| t.game_mode.is_some() && t.name.eq_ignore_ascii_case(token))
            .collect();
        (!players.is_empty()).then_some(players)
    }

    /// Кик игрока (`/kick`, `/ban`).
    pub(crate) fn kick(&self, uuid: Uuid, reason: &str) {
        self.push_control(uuid, Control::Kick(reason.to_owned()));
    }

    /// Пересылка на другой сервер (`/transfer`).
    pub(crate) fn transfer(&self, uuid: Uuid, host: &str, port: u16) {
        self.push_control(uuid, Control::Transfer(host.to_owned(), port));
    }

    /// Просьба сохранить мир (`/save-all`).
    pub(crate) fn request_save(&self) {
        self.save_requested
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Была ли просьба сохранить мир; сбрасывает флаг.
    pub fn take_save_request(&self) -> bool {
        self.save_requested
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    /// Запоминает, сколько длился автосейв.
    pub fn set_autosave_ms(&self, millis: u32) {
        self.autosave_ms
            .store(millis, std::sync::atomic::Ordering::Relaxed);
    }

    /// Белый список включён или выключен (`/whitelist on|off`).
    pub(crate) fn set_whitelist_enabled(&self, on: bool) {
        self.whitelist_enabled
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Включён ли белый список.
    pub fn whitelist_enabled(&self) -> bool {
        self.whitelist_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Перечитать `ops.json` и правила (`/reload`).
    pub(crate) fn reload_configuration(&self) {
        let Some(dir) = self.reload_dir.as_ref() else {
            return;
        };
        match crate::ops::OpList::load(dir.join("ops.json")) {
            Ok(list) => self.set_ops(list),
            Err(e) => tracing::error!("reload ops.json: {e}"),
        }
    }

    /// Перечитать белый список: он хранится вместе с правилами.
    pub(crate) fn reload_whitelist(&self) {
        // Списки живут в `gamerules.json` и читаются при старте: `/reload`
        // перечитывает файл целиком.
    }

    /// NBT блок-сущности вместе с `id` и координатами. `None` — блок её не хранит.
    pub(crate) fn block_nbt(&self, pos: [i32; 3]) -> Option<Compound> {
        let be = self
            .block_entity(pos)
            .or_else(|| self.ensure_container(pos))?;
        let mut nbt = be.nbt.clone();
        nbt.insert("id".into(), Tag::String(be.id));
        nbt.insert("x".into(), Tag::Int(pos[0]));
        nbt.insert("y".into(), Tag::Int(pos[1]));
        nbt.insert("z".into(), Tag::Int(pos[2]));
        Some(nbt)
    }

    /// Пишет NBT блок-сущности. Координаты и `id` в сохранённый compound не входят.
    pub(crate) fn store_block_nbt(&self, pos: [i32; 3], mut nbt: Compound) -> bool {
        let id = nbt
            .get("id")
            .and_then(Tag::as_str)
            .map(str::to_owned)
            .or_else(|| self.block_entity(pos).map(|be| be.id))
            .or_else(|| {
                self.block_name_at(pos)
                    .as_deref()
                    .and_then(super::super::container_entity)
                    .map(str::to_owned)
            });
        let Some(id) = id else {
            return false;
        };
        if self.block_entity(pos).is_none()
            && self
                .block_name_at(pos)
                .as_deref()
                .and_then(super::super::container_entity)
                .is_none()
        {
            return false;
        }
        for key in ["id", "x", "y", "z"] {
            nbt.remove(key);
        }
        self.set_block_entity(pos, Some(BlockEntity { id, nbt }));
        true
    }

    /// Пустая блок-сущность контейнера, если блок её ещё не имеет.
    fn ensure_container(&self, pos: [i32; 3]) -> Option<BlockEntity> {
        let name = self.block_name_at(pos)?;
        let id = super::super::container_entity(&name)?;
        let be = BlockEntity {
            id: id.into(),
            nbt: Compound::new(),
        };
        self.set_block_entity(pos, Some(be.clone()));
        Some(be)
    }

    /// Снимок сущности: живые поля и ранее записанный `/data` хвост.
    pub(crate) fn entity_nbt(&self, target: &Target) -> Compound {
        let mut base = self.entity_base(target);
        if let Some(extra) = self.entity_extra.lock().unwrap().get(&target.uuid) {
            for (key, value) in extra {
                if !is_live_key(key) {
                    base.insert(key.clone(), value.clone());
                }
            }
        }
        let tags = self.entity_tags.lock().unwrap().get(&target.uuid).cloned();
        match tags {
            Some(tags) if !tags.is_empty() => {
                base.insert(
                    "Tags".into(),
                    Tag::List(tags.into_iter().map(Tag::String).collect()),
                );
            }
            _ => {
                base.remove("Tags");
            }
        }
        base
    }

    /// Записывает снимок: живые поля применяются к игроку или мобу, остальное хранится.
    pub(crate) fn store_entity_nbt(&self, target: &Target, mut nbt: Compound) -> bool {
        self.apply_entity_fields(target, &nbt);
        let tags = match nbt.remove("Tags") {
            Some(Tag::List(items)) => items
                .into_iter()
                .filter_map(|tag| match tag {
                    Tag::String(name) => Some(name),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        self.entity_tags.lock().unwrap().insert(target.uuid, tags);
        let extra = nbt
            .into_iter()
            .filter(|(key, _)| !is_live_key(key))
            .collect();
        self.entity_extra.lock().unwrap().insert(target.uuid, extra);
        true
    }

    /// Добавляет метку. `false` — она уже есть или меток больше 1024.
    pub(crate) fn add_tag(&self, uuid: Uuid, name: &str) -> bool {
        if name.is_empty() || name.len() > 1024 {
            return false;
        }
        let mut tags = self.entity_tags.lock().unwrap();
        let entry = tags.entry(uuid).or_default();
        if entry.iter().any(|tag| tag == name) || entry.len() >= 1024 {
            return false;
        }
        entry.push(name.to_owned());
        true
    }

    /// Снимает метку. `false` — такой не было.
    pub(crate) fn remove_tag(&self, uuid: Uuid, name: &str) -> bool {
        let mut tags = self.entity_tags.lock().unwrap();
        let Some(entry) = tags.get_mut(&uuid) else {
            return false;
        };
        let before = entry.len();
        entry.retain(|tag| tag != name);
        before != entry.len()
    }

    /// Игрок или моб. Предмет, снаряд и опыт — нет.
    pub(crate) fn is_living(&self, target: &Target) -> bool {
        target.game_mode.is_some() || self.mobs.lock().unwrap().contains_key(&target.entity_id)
    }

    /// Взмах руки (`/swing`).
    pub(crate) fn swing(&self, entity_id: i32, hand: i32, kind: i32, duration: i32) {
        self.broadcast_packet(chunkwake_protocol::play::swing_animation(
            self.ids.swing_animation,
            entity_id,
            hand,
            kind,
            duration,
        ));
    }

    /// Камера одного игрока (`/spectate`).
    pub(crate) fn set_camera(&self, uuid: Uuid, entity_id: i32) {
        self.send_to(
            uuid,
            chunkwake_protocol::play::set_camera(self.ids.set_camera, entity_id),
        );
    }

    /// Рассылает пассажиров транспорта (`/ride`).
    pub(crate) fn send_passengers(&self, vehicle: i32, passengers: &[i32]) {
        self.broadcast_packet(chunkwake_protocol::play::set_passengers(
            self.ids.set_passengers,
            vehicle,
            passengers,
        ));
    }

    /// Снимает всадника и возвращает id транспорта, если он был.
    pub(crate) fn unlink_rider(&self, rider: Uuid) -> Option<Uuid> {
        let mut links = self.links.lock().unwrap();
        let vehicle = links.get(&rider).and_then(|link| link.vehicle)?;
        if let Some(link) = links.get_mut(&rider) {
            link.vehicle = None;
        }
        if let Some(link) = links.get_mut(&vehicle) {
            link.passengers.retain(|id| *id != rider);
        }
        Some(vehicle)
    }

    /// Транспорт сущности, если `/ride` или `execute on` его уже записали.
    pub(crate) fn vehicle_of(&self, rider: Uuid) -> Option<Uuid> {
        self.links
            .lock()
            .unwrap()
            .get(&rider)
            .and_then(|link| link.vehicle)
    }

    /// Пассажиры транспорта.
    pub(crate) fn passengers_of(&self, vehicle: Uuid) -> Vec<Uuid> {
        self.links
            .lock()
            .unwrap()
            .get(&vehicle)
            .map(|link| link.passengers.clone())
            .unwrap_or_default()
    }

    /// Открывает trigger-счёт и рассылает ноль.
    pub(crate) fn enable_trigger(&self, holder: &str, objective: &str) -> Result<bool, bool> {
        let opened = self
            .scoreboard
            .lock()
            .unwrap()
            .enable_trigger(holder, objective)?;
        if opened {
            self.hub.send(
                None,
                chunkwake_protocol::play::set_score(self.ids.set_score, holder, objective, 0, None),
            );
        }
        Ok(opened)
    }

    /// `/trigger`: закрывает счёт и рассылает новое значение.
    pub(crate) fn pull_trigger(
        &self,
        holder: &str,
        objective: &str,
        value: i32,
        add: bool,
    ) -> Result<i32, bool> {
        let next = self
            .scoreboard
            .lock()
            .unwrap()
            .pull_trigger(holder, objective, value, add)?;
        self.hub.send(
            None,
            chunkwake_protocol::play::set_score(self.ids.set_score, holder, objective, next, None),
        );
        Ok(next)
    }

    /// Всадник `rider` сидит на `vehicle` (`execute on vehicle|passengers`).
    pub fn link_rider(&self, rider: Uuid, vehicle: Uuid) {
        let mut links = self.links.lock().unwrap();
        links.entry(rider).or_default().vehicle = Some(vehicle);
        let passengers = &mut links.entry(vehicle).or_default().passengers;
        if !passengers.contains(&rider) {
            passengers.push(rider);
        }
    }

    fn entity_base(&self, target: &Target) -> Compound {
        let mut root = Compound::new();
        if target.game_mode.is_some() {
            let pose = self.live.lock().unwrap().get(&target.uuid).map(|live| {
                (
                    live.pos,
                    live.yaw,
                    live.pitch,
                    live.health,
                    Some(live.slots),
                )
            });
            let (pos, yaw, pitch, health, slots) = pose.unwrap_or_else(|| {
                let (pos, yaw, pitch) =
                    self.player_at(target.uuid)
                        .unwrap_or((target.pos, target.yaw, target.pitch));
                (pos, yaw, pitch, 20.0, None)
            });
            insert_pose(&mut root, pos, yaw, pitch, health);
            if let Some(slots) = slots {
                root.insert(
                    "Inventory".into(),
                    Tag::List(inventory_list(&slots, |id| self.item_name(id).to_owned())),
                );
            }
            return root;
        }
        let mobs = self.mobs.lock().unwrap();
        if let Some(entry) = mobs.get(&target.entity_id) {
            root.insert("id".into(), Tag::String(target.name.clone()));
            insert_pose(
                &mut root,
                entry.mob.pos,
                entry.mob.yaw,
                entry.mob.pitch,
                entry.mob.health,
            );
            if let Some(name) = &entry.mob.custom_name {
                root.insert("CustomName".into(), Tag::String(name.clone()));
            }
        } else {
            root.insert("id".into(), Tag::String(target.name.clone()));
            insert_pose(&mut root, target.pos, target.yaw, target.pitch, 0.0);
        }
        root
    }

    fn apply_entity_fields(&self, target: &Target, nbt: &Compound) {
        if target.game_mode.is_some() {
            if let Some(health) = nbt.get("Health").and_then(super::snbt::number) {
                self.push_control(target.uuid, Control::Health(health as f32));
            }
            if let Some(pos) = list3(nbt.get("Pos")) {
                let (yaw, pitch) = list2(nbt.get("Rotation")).unwrap_or_else(|| {
                    self.live
                        .lock()
                        .unwrap()
                        .get(&target.uuid)
                        .map(|live| (live.yaw, live.pitch))
                        .unwrap_or((target.yaw, target.pitch))
                });
                if let Some(player) = self.hub.players.lock().unwrap().get_mut(&target.uuid) {
                    player.pos = pos;
                    player.yaw = yaw;
                    player.pitch = pitch;
                }
                self.teleport_to(target.uuid, pos, yaw, pitch);
            }
            if let Some(items) = nbt.get("Inventory").and_then(Tag::as_list) {
                for window in player_windows() {
                    self.push_control(target.uuid, Control::Slot(window, None));
                }
                for tag in items {
                    let Some(map) = tag.as_compound() else {
                        continue;
                    };
                    let slot = map.get("Slot").and_then(Tag::as_i64).unwrap_or(0) as i32;
                    let Some(window) = player_slot_to_window(slot) else {
                        continue;
                    };
                    let stack = crate::stack_nbt::from_nbt(map, |name| self.item_id(name));
                    self.push_control(target.uuid, Control::Slot(window, stack));
                }
            }
            return;
        }
        let mut mobs = self.mobs.lock().unwrap();
        let Some(entry) = mobs.get_mut(&target.entity_id) else {
            return;
        };
        if let Some(health) = nbt.get("Health").and_then(super::snbt::number) {
            entry.mob.health = health as f32;
        }
        if let Some(pos) = list3(nbt.get("Pos")) {
            entry.mob.pos = pos;
            if let Some((yaw, pitch)) = list2(nbt.get("Rotation")) {
                entry.mob.yaw = yaw;
                entry.mob.pitch = pitch;
            }
        }
        if let Some(name) = nbt.get("CustomName") {
            entry.mob.custom_name = Some(crate::stack_nbt::tag_to_json(name).to_string());
        }
    }

    /// Значение счёта для `/execute if score`.
    pub fn score(&self, holder: &str, objective: &str) -> Option<i32> {
        self.scoreboard.lock().unwrap().get(holder, objective)
    }

    /// Есть ли цель счёта (`/execute store`, `/scoreboard modify`).
    pub fn has_objective(&self, name: &str) -> bool {
        self.scoreboard
            .lock()
            .unwrap()
            .objectives
            .contains_key(name)
    }

    /// Подпись и способ рисования цели; пакет уходит всем.
    pub(crate) fn modify_objective(
        &self,
        name: &str,
        display: Option<serde_json::Value>,
        render: Option<i32>,
    ) -> bool {
        let updated = {
            let mut board = self.scoreboard.lock().unwrap();
            if !board.modify(name, display, render) {
                return false;
            }
            board.objectives.get(name).map(|objective| {
                (
                    objective.display_name.clone(),
                    objective.render_type,
                    objective.number_format.clone(),
                )
            })
        };
        let Some((display, render, format)) = updated else {
            return false;
        };
        self.send_objective(name, 2, &display, render, format.as_ref());
        true
    }

    /// Формат числа цели. `None` — формат сброшен.
    pub(crate) fn set_objective_format(
        &self,
        name: &str,
        format: Option<chunkwake_protocol::play::NumberFormat>,
    ) -> bool {
        let updated = {
            let mut board = self.scoreboard.lock().unwrap();
            if !board.set_format(name, format) {
                return false;
            }
            board.objectives.get(name).map(|objective| {
                (
                    objective.display_name.clone(),
                    objective.render_type,
                    objective.number_format.clone(),
                )
            })
        };
        let Some((display, render, format)) = updated else {
            return false;
        };
        self.send_objective(name, 2, &display, render, format.as_ref());
        true
    }

    /// Формат одного счёта. В пакет значения не пишется.
    pub(crate) fn set_score_format(
        &self,
        holder: &str,
        objective: &str,
        format: Option<chunkwake_protocol::play::NumberFormat>,
    ) -> bool {
        self.scoreboard
            .lock()
            .unwrap()
            .set_score_format(holder, objective, format)
    }

    /// Формат цели для тестов.
    pub fn objective_format(&self, name: &str) -> Option<chunkwake_protocol::play::NumberFormat> {
        self.scoreboard
            .lock()
            .unwrap()
            .objectives
            .get(name)
            .and_then(|objective| objective.number_format.clone())
    }

    /// Формат отдельного счёта для тестов.
    pub fn score_format(
        &self,
        holder: &str,
        objective: &str,
    ) -> Option<chunkwake_protocol::play::NumberFormat> {
        self.scoreboard
            .lock()
            .unwrap()
            .score_formats
            .get(&(holder.to_owned(), objective.to_owned()))
            .cloned()
    }

    fn send_objective(
        &self,
        name: &str,
        mode: u8,
        display: &serde_json::Value,
        render: i32,
        format: Option<&chunkwake_protocol::play::NumberFormat>,
    ) {
        self.hub.send(
            None,
            chunkwake_protocol::play::set_objective(
                self.ids.set_objective,
                name,
                mode,
                display,
                render,
                format,
                false,
            ),
        );
    }

    /// Заливает область по правилам `/fill`; возвращает число изменённых блоков.
    ///
    /// `filter` — имя блока или `#тег` для `replace <filter>`.
    pub(crate) fn fill_region(
        &self,
        from: [i32; 3],
        to: [i32; 3],
        state: u32,
        mode: super::world::FillMode,
        filter: Option<&str>,
    ) -> i64 {
        use super::world::FillMode;
        let (x1, x2) = (from[0].min(to[0]), from[0].max(to[0]));
        let (y1, y2) = (from[1].min(to[1]), from[1].max(to[1]));
        let (z1, z2) = (from[2].min(to[2]), from[2].max(to[2]));
        // Фильтр разбирается один раз: раньше на каждый блок приходился
        // `format!` для имени без пространства имён.
        let filter = filter.map(|filter| BlockFilter::new(filter, &self.survival.block_tags));
        let air = self
            .survival
            .block_defaults
            .get("minecraft:air")
            .copied()
            .unwrap_or(state);
        let mut origins = HashSet::new();
        let mut changed = 0;
        for y in y1..=y2 {
            for z in z1..=z2 {
                for x in x1..=x2 {
                    let position = [x, y, z];
                    let current = self.block(position);
                    if let Some(filter) = &filter {
                        let Some(state_id) = current else { continue };
                        if !filter.matches(self.state_name(state_id)) {
                            continue;
                        }
                    }
                    let edge = x == x1 || x == x2 || y == y1 || y == y2 || z == z1 || z == z2;
                    let place = match mode {
                        FillMode::Replace | FillMode::Destroy | FillMode::Strict => true,
                        // `keep` не трогает непустые блоки, `hollow` — только рамку,
                        // `outline` — рамку и внутренний воздух.
                        FillMode::Keep => {
                            current.is_none_or(|s| self.state_name(s).ends_with(":air"))
                        }
                        FillMode::Hollow => edge,
                        FillMode::Outline => true,
                    };
                    if !place {
                        continue;
                    }
                    if mode == FillMode::Destroy
                        && current.is_some_and(|s| !self.state_name(s).ends_with(":air"))
                    {
                        self.break_drops(position);
                    }
                    let target = match mode {
                        FillMode::Outline if !edge => air,
                        _ => state,
                    };
                    if self
                        .set_block(position, target)
                        .is_some_and(|old| old != target)
                    {
                        changed += 1;
                        origins.extend(crate::redstone::around(position));
                    }
                }
            }
        }
        if !origins.is_empty() {
            self.redstone_update(&origins.into_iter().collect::<Vec<_>>());
        }
        changed
    }

    /// Копирует область `/clone`. `Ok` — сколько блоков сменилось.
    pub(crate) fn clone_blocks(
        &self,
        begin: [i32; 3],
        end: [i32; 3],
        dest: [i32; 3],
        mask: super::world::CloneMask<'_>,
        mode: super::world::CloneMode,
        strict: bool,
    ) -> Result<i64, super::world::CloneError> {
        use super::world::{CloneError, CloneMask, CloneMode};
        let source_min = [
            begin[0].min(end[0]),
            begin[1].min(end[1]),
            begin[2].min(end[2]),
        ];
        let source_max = [
            begin[0].max(end[0]),
            begin[1].max(end[1]),
            begin[2].max(end[2]),
        ];
        let span = [
            source_max[0] - source_min[0],
            source_max[1] - source_min[1],
            source_max[2] - source_min[2],
        ];
        let dest_max = [dest[0] + span[0], dest[1] + span[1], dest[2] + span[2]];
        if !self.in_world(source_min)
            || !self.in_world(source_max)
            || !self.in_world(dest)
            || !self.in_world(dest_max)
        {
            return Err(CloneError::OutOfBounds);
        }
        if mode == CloneMode::Normal && boxes_overlap(source_min, source_max, dest, dest_max) {
            return Err(CloneError::Overlap);
        }
        let volume = (i64::from(span[0]) + 1) * (i64::from(span[1]) + 1) * (i64::from(span[2]) + 1);
        let limit = self.vars.number("max_block_modifications").max(0);
        if volume > limit {
            return Err(CloneError::TooBig { limit, volume });
        }
        if !self.box_resident(source_min, source_max) || !self.box_resident(dest, dest_max) {
            return Err(CloneError::Unloaded);
        }
        let mut samples = Vec::new();
        for z in source_min[2]..=source_max[2] {
            for y in source_min[1]..=source_max[1] {
                for x in source_min[0]..=source_max[0] {
                    let from = [x, y, z];
                    let Some(state) = self.block(from) else {
                        return Err(CloneError::Unloaded);
                    };
                    let name = self.state_name(state);
                    let keep = match mask {
                        CloneMask::Replace => true,
                        CloneMask::Masked => !is_air_name(name),
                        CloneMask::Filtered(filter) => {
                            block_filter_matches(name, filter, &self.survival.block_tags)
                        }
                    };
                    if !keep {
                        continue;
                    }
                    let at = [
                        dest[0] + (x - source_min[0]),
                        dest[1] + (y - source_min[1]),
                        dest[2] + (z - source_min[2]),
                    ];
                    samples.push((from, at, state, self.block_entity(from)));
                }
            }
        }
        let air = self
            .survival
            .block_defaults
            .get("minecraft:air")
            .copied()
            .unwrap_or(0);
        if mode == CloneMode::Move {
            let dest_cells: HashSet<[i32; 3]> = samples.iter().map(|(_, at, _, _)| *at).collect();
            for z in source_min[2]..=source_max[2] {
                for y in source_min[1]..=source_max[1] {
                    for x in source_min[0]..=source_max[0] {
                        let from = [x, y, z];
                        if dest_cells.contains(&from) {
                            continue;
                        }
                        self.set_block(from, air);
                        self.set_block_entity(from, None);
                    }
                }
            }
        }
        let mut changed = 0i64;
        let mut origins = HashSet::new();
        for (_, at, state, be) in &samples {
            if self.set_block(*at, *state).is_some_and(|old| old != *state) {
                changed += 1;
                origins.insert(*at);
            }
            self.set_block_entity(*at, be.clone());
        }
        if !strict && !origins.is_empty() {
            self.redstone_update(&origins.into_iter().collect::<Vec<_>>());
        }
        if changed == 0 {
            Err(CloneError::Failed)
        } else {
            Ok(changed)
        }
    }

    /// Клетки биома 4×4×4 внутри ящика. `Ok(0)` — ни одна не изменилась.
    pub(crate) fn fill_biomes(
        &self,
        from: [i32; 3],
        to: [i32; 3],
        biome: u32,
        filter: Option<&std::collections::HashSet<u32>>,
    ) -> Result<i32, &'static str> {
        let min = [from[0].min(to[0]), from[1].min(to[1]), from[2].min(to[2])];
        let max = [from[0].max(to[0]), from[1].max(to[1]), from[2].max(to[2])];
        if !self.box_resident(min, max) {
            return Err("argument.pos.unloaded");
        }
        let mut cells = std::collections::HashSet::new();
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    let ry = y - self.min_y;
                    if ry < 0 {
                        continue;
                    }
                    cells.insert((
                        (x >> 4, z >> 4),
                        (x & 15) as usize,
                        ry as usize,
                        (z & 15) as usize,
                    ));
                }
            }
        }
        let mut changed = std::collections::HashSet::new();
        let mut count = 0i32;
        {
            let mut world = self.edited.write().unwrap();
            for (chunk, lx, y, lz) in cells {
                let Some((column, rev)) = world.get_mut(&chunk) else {
                    return Err("argument.pos.unloaded");
                };
                if y >= column.height() {
                    continue;
                }
                let section = y / 16;
                let index = (lx / 4) + 4 * (lz / 4) + 16 * ((y % 16) / 4);
                let old = column
                    .sections
                    .get(section)
                    .map(|section| section.biome.get(index))
                    .unwrap_or(biome);
                if filter.is_some_and(|ids| !ids.contains(&old)) || old == biome {
                    continue;
                }
                column.set_biome(lx, y, lz, biome);
                *rev += 1;
                count += 1;
                changed.insert(chunk);
            }
        }
        if count == 0 {
            return Ok(0);
        }
        let packets = {
            let world = self.edited.read().unwrap();
            let mut dirty = self.dirty.lock().unwrap();
            let mut frames = self.chunk_frames.lock().unwrap();
            let mut parts = Vec::new();
            for chunk in changed {
                dirty.insert(chunk);
                frames.remove(&chunk);
                if let Some((column, _)) = world.get(&chunk) {
                    parts.push((chunk.0, chunk.1, column.encode_biomes()));
                }
            }
            parts
        };
        let view: Vec<(i32, i32, &[u8])> = packets
            .iter()
            .map(|(x, z, data)| (*x, *z, data.as_slice()))
            .collect();
        self.broadcast_packet(chunkwake_protocol::play::chunks_biomes(
            self.ids.chunks_biomes,
            &view,
        ));
        Ok(count)
    }

    fn box_resident(&self, min: [i32; 3], max: [i32; 3]) -> bool {
        let (x1, x2) = (min[0] >> 4, max[0] >> 4);
        let (z1, z2) = (min[2] >> 4, max[2] >> 4);
        for x in x1..=x2 {
            for z in z1..=z2 {
                if !self.chunk_resident(x << 4, z << 4) {
                    return false;
                }
            }
        }
        true
    }

    /// Это измерение сервера. Других миров нет.
    pub(crate) fn is_current_dimension(&self, name: &str) -> bool {
        let name = super::args::resource_name(name);
        name == self.login.spawn.dimension
    }

    /// Текущая граница и пакеты её изменения.
    pub(crate) fn border_now(&self) -> i64 {
        self.game_time.load(Ordering::Relaxed)
    }

    pub(crate) fn broadcast_border_size(&self, size: f64) {
        self.broadcast_packet(chunkwake_protocol::play::set_border_size(
            self.ids.set_border_size,
            size,
        ));
    }

    pub(crate) fn broadcast_border_lerp(&self, from: f64, to: f64, ticks: i64) {
        self.broadcast_packet(chunkwake_protocol::play::set_border_lerp_size(
            self.ids.set_border_lerp_size,
            from,
            to,
            ticks.saturating_mul(50),
        ));
    }

    pub(crate) fn broadcast_border_center(&self, x: f64, z: f64) {
        self.broadcast_packet(chunkwake_protocol::play::set_border_center(
            self.ids.set_border_center,
            x,
            z,
        ));
    }

    pub(crate) fn broadcast_border_warning(&self, blocks: Option<i32>, time: Option<i32>) {
        if let Some(blocks) = blocks {
            self.broadcast_packet(chunkwake_protocol::play::set_border_warning_distance(
                self.ids.set_border_warning_distance,
                blocks,
            ));
        }
        if let Some(time) = time {
            self.broadcast_packet(chunkwake_protocol::play::set_border_warning_delay(
                self.ids.set_border_warning_delay,
                time,
            ));
        }
    }

    /// Число из диапазона `/random`. Именованная последовательность запоминается.
    pub(crate) fn random_int(&self, sequence: Option<&str>, lo: i32, hi: i32) -> i32 {
        let mut state = match sequence {
            Some(name) => {
                let mut map = self.random_seq.lock().unwrap();
                *map.entry(name.to_owned())
                    .or_insert_with(|| sequence_seed(self.seed, name, 0, true, true))
            }
            None => rand::random::<u64>() | 1,
        };
        let value = pick_int(&mut state, lo, hi);
        if let Some(name) = sequence {
            self.random_seq
                .lock()
                .unwrap()
                .insert(name.to_owned(), state);
        }
        value
    }

    /// Сбрасывает одну последовательность или все.
    pub(crate) fn reset_random(
        &self,
        name: Option<&str>,
        salt: i64,
        include_world: bool,
        include_id: bool,
    ) {
        let mut map = self.random_seq.lock().unwrap();
        match name {
            None => map.clear(),
            Some(name) => {
                map.insert(
                    name.to_owned(),
                    sequence_seed(self.seed, name, salt, include_world, include_id),
                );
            }
        }
    }

    /// Отправляет готовый пакет всем (`/say`).
    pub(crate) fn broadcast_packet(&self, packet: Vec<u8>) {
        self.hub.send(None, packet);
    }

    /// ID пакета `system_chat`.
    pub(crate) fn chat_packet_id(&self) -> i32 {
        self.ids.system_chat
    }

    /// Рассылает время мира (`/time set`).
    pub(crate) fn broadcast_world_time(&self) {
        self.broadcast_time();
    }

    /// Рассылает точку возрождения мира (`/setworldspawn`).
    ///
    /// Карта Xaero и точка возрождения на клиенте берут её из
    /// `set_default_spawn_position`, а не из `login`: без рассылки команда
    /// меняла только сервер.
    pub(crate) fn broadcast_world_spawn(&self) {
        let here = self.spawn_point();
        let pos = [
            here[0].floor() as i32,
            here[1].floor() as i32,
            here[2].floor() as i32,
        ];
        self.broadcast_packet(chunkwake_protocol::play::set_default_spawn_position(
            self.ids.set_default_spawn_position,
            &self.login.spawn.dimension,
            pos,
            0.0,
            0.0,
        ));
    }

    /// Выбрасывает предмет в мир (`/loot spawn`): тот же путь, что у дропа блока.
    pub(crate) fn drop_in_world(
        &self,
        from: [f64; 3],
        stack: chunkwake_protocol::inventory::Stack,
    ) {
        self.drop_item(from, [0.0, 0.2, 0.0], stack, std::time::Duration::ZERO);
    }

    /// Погода: клиенту уходит `game_event` (1 — дождь, 2 — гроза, 7/8 — конец).
    pub(crate) fn broadcast_weather(&self, weather: super::state::Weather) {
        let event = match (weather.rain, weather.thunder) {
            (false, _) => 7,
            (true, false) => 1,
            (true, true) => 2,
        };
        self.hub.send(
            None,
            chunkwake_protocol::play::game_event(self.ids.game_event, event, 0.0),
        );
    }

    /// Названный звук одному игроку (`/playsound`); `false` — звука нет в реестре.
    ///
    /// `source` — категория: `master` = 0, `music` = 1, ….
    pub(crate) fn play_named_sound_from(
        &self,
        name: &str,
        uuid: Uuid,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
        source: i32,
    ) -> bool {
        if !self.survival.sounds.contains_key(name) {
            return false;
        }
        self.send_to(
            uuid,
            chunkwake_protocol::play::sound(
                self.ids.sound,
                self.survival.sounds[name],
                source,
                pos,
                volume,
                pitch,
                rand::random(),
            ),
        );
        true
    }

    /// Останавливает звуки у игрока (`/stopsound`).
    ///
    /// `name == None` — все звуки категории. `source == None` — категория не задана.
    pub(crate) fn stop_sound(&self, uuid: Uuid, name: Option<&str>, source: Option<i32>) {
        let id = name.and_then(|n| self.survival.sounds.get(n).copied());
        self.send_to(
            uuid,
            chunkwake_protocol::play::stop_sound(self.ids.stop_sound, id, source),
        );
    }

    /// Заголовок или подзаголовок (`/title`).
    pub(crate) fn send_title(&self, uuid: Uuid, part: &str, component: serde_json::Value) {
        let id = match part {
            "title" => self.ids.set_title_text,
            "subtitle" => self.ids.set_subtitle_text,
            _ => self.ids.set_action_bar_text,
        };
        self.send_to(
            uuid,
            chunkwake_protocol::play::plain_component_packet(id, &component),
        );
    }

    /// Тайминги заголовка (`/title … times`).
    pub(crate) fn send_title_animation(&self, uuid: Uuid, fade_in: i32, stay: i32, fade_out: i32) {
        self.send_to(
            uuid,
            chunkwake_protocol::play::titles_animation(
                self.ids.set_titles_animation,
                fade_in,
                stay,
                fade_out,
            ),
        );
    }

    /// Сброс заголовка (`/title … clear|reset`).
    pub(crate) fn send_clear_titles(&self, uuid: Uuid, reset: bool) {
        self.send_to(
            uuid,
            chunkwake_protocol::play::clear_titles(self.ids.clear_titles, reset),
        );
    }

    /// Цели счёта по токену: селектор, `*` или имя держателя.
    pub(crate) fn score_holders(&self, token: &str) -> Vec<String> {
        if token == "*" {
            return self.objective_holders();
        }
        if token.starts_with('@') {
            return self
                .command_targets_named(token)
                .unwrap_or_default()
                .into_iter()
                .map(|t| t.name)
                .collect();
        }
        vec![token.to_owned()]
    }

    /// Все держатели счёта во всех целях.
    pub(crate) fn objective_holders(&self) -> Vec<String> {
        let board = self.scoreboard.lock().unwrap();
        let mut out: Vec<String> = board
            .scores
            .values()
            .flat_map(|m| m.keys().cloned())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Цели счёта по имени.
    pub(crate) fn objective_names(&self) -> Vec<String> {
        self.scoreboard
            .lock()
            .unwrap()
            .objectives
            .keys()
            .cloned()
            .collect()
    }

    /// Добавляет цель счёта; пакет уходит всем.
    pub(crate) fn add_objective(&self, objective: super::scoreboard::Objective) -> bool {
        let name = objective.name.clone();
        let display = objective.display_name.clone();
        let render = objective.render_type;
        if !self.scoreboard.lock().unwrap().add_objective(objective) {
            return false;
        }
        self.hub.send(
            None,
            chunkwake_protocol::play::set_objective(
                self.ids.set_objective,
                &name,
                0,
                &display,
                render,
                None,
                false,
            ),
        );
        true
    }

    /// Убирает цель счёта; пакет уходит всем.
    pub(crate) fn remove_objective(&self, name: &str) -> bool {
        if !self.scoreboard.lock().unwrap().remove_objective(name) {
            return false;
        }
        self.hub.send(
            None,
            chunkwake_protocol::play::set_objective(
                self.ids.set_objective,
                name,
                1,
                &serde_json::Value::Null,
                0,
                None,
                false,
            ),
        );
        true
    }

    /// Ставит или снимает счёт; пакет уходит всем.
    pub(crate) fn set_score(&self, holder: &str, objective: &str, value: Option<i32>) -> bool {
        if !self
            .scoreboard
            .lock()
            .unwrap()
            .set(holder, objective, value)
        {
            return false;
        }
        let packet = match value {
            Some(value) => chunkwake_protocol::play::set_score(
                self.ids.set_score,
                holder,
                objective,
                value,
                None,
            ),
            None => {
                chunkwake_protocol::play::reset_score(self.ids.reset_score, holder, Some(objective))
            }
        };
        self.hub.send(None, packet);
        true
    }

    /// Счёт держателя по всем целям (`/scoreboard players list`).
    pub(crate) fn score_list(&self, holder: &str) -> Vec<(String, i32)> {
        let board = self.scoreboard.lock().unwrap();
        let mut out: Vec<(String, i32)> = board
            .scores
            .iter()
            .filter_map(|(objective, m)| m.get(holder).map(|v| (objective.clone(), *v)))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Показывает цель в слоте (`/scoreboard objectives setdisplay`).
    pub(crate) fn set_objective_display(&self, slot: &str, objective: &str) -> bool {
        if !self
            .scoreboard
            .lock()
            .unwrap()
            .objectives
            .contains_key(objective)
        {
            return false;
        }
        self.scoreboard
            .lock()
            .unwrap()
            .display
            .insert(slot.to_owned(), objective.to_owned());
        self.hub.send(
            None,
            chunkwake_protocol::play::set_display_objective(
                self.ids.set_display_objective,
                slot,
                objective,
            ),
        );
        true
    }
}

/// Сущность мира как цель селектора.
fn entity_ref(entity_id: i32, pos: [f64; 3], name: &str) -> Target {
    Target {
        // У сущностей мира нет UUID: берём предсказуемый, чтобы сравнение
        // работало; отличать их достаточно по `entity_id`.
        uuid: entity_uuid(entity_id),
        name: args_name(name),
        entity_id,
        pos,
        yaw: 0.0,
        pitch: 0.0,
        game_mode: None,
        tags: Vec::new(),
    }
}

/// Имя вида для `type=`: `minecraft:cow`.
fn args_name(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn is_live_key(key: &str) -> bool {
    matches!(key, "Pos" | "Rotation" | "Health" | "Inventory" | "id")
}

/// UUID моба выводится из `entity_id` — тот же вывод, что в `entity_ref`.
fn entity_uuid(entity_id: i32) -> Uuid {
    Uuid::from_u128(u128::from(entity_id as u32).rotate_left(32))
}

fn insert_pose(root: &mut Compound, pos: [f64; 3], yaw: f32, pitch: f32, health: f32) {
    root.insert(
        "Pos".into(),
        Tag::List(pos.into_iter().map(Tag::Double).collect()),
    );
    root.insert(
        "Rotation".into(),
        Tag::List(vec![Tag::Float(yaw), Tag::Float(pitch)]),
    );
    root.insert("Health".into(), Tag::Float(health));
}

fn list3(tag: Option<&Tag>) -> Option<[f64; 3]> {
    let list = tag?.as_list()?;
    let nums: Vec<f64> = list.iter().filter_map(super::snbt::number).collect();
    (nums.len() >= 3).then_some([nums[0], nums[1], nums[2]])
}

fn list2(tag: Option<&Tag>) -> Option<(f32, f32)> {
    let list = tag?.as_list()?;
    let nums: Vec<f64> = list.iter().filter_map(super::snbt::number).collect();
    (nums.len() >= 2).then(|| (nums[0] as f32, nums[1] as f32))
}

fn inventory_list(
    slots: &[chunkwake_protocol::inventory::Slot],
    name: impl Fn(i32) -> String,
) -> Vec<Tag> {
    let mut out = Vec::new();
    for (window, stack) in slots.iter().enumerate() {
        let Some(stack) = *stack else { continue };
        let Some(slot) = window_to_player_slot(window) else {
            continue;
        };
        let item = name(stack.item);
        let Some(mut map) = crate::stack_nbt::named_nbt(stack, &item, &|id| {
            let n = name(id);
            (n != "minecraft:air").then_some(n)
        }) else {
            continue;
        };
        map.insert("Slot".into(), Tag::Byte(slot));
        out.push(Tag::Compound(map));
    }
    out
}

/// Высота столбца для `positioned over`.
pub(crate) enum ColumnHeight {
    /// Клетка над верхним подходящим блоком.
    Y(f64),
    /// Такого имени карты нет.
    Unknown,
    /// Столбца нет в памяти.
    Missing,
}

/// Столбец из памяти. Плоский мир без записи берёт шаблон и чанк не создаёт.
fn with_column<T>(
    plan: &PlayPlan,
    x: i32,
    z: i32,
    f: impl FnOnce(&chunkwake_world::ChunkColumn) -> T,
) -> Option<T> {
    let key = (x >> 4, z >> 4);
    let world = plan.edited.read().unwrap();
    if let Some(entry) = world.get(&key) {
        return Some(f(&entry.0));
    }
    drop(world);
    if plan.terrain.is_none() {
        return Some(f(plan.template()));
    }
    None
}

/// Y относительно низа столбца: клетка над верхним блоком, который принял фильтр.
fn column_top(
    column: &chunkwake_world::ChunkColumn,
    x: usize,
    z: usize,
    accept: impl Fn(u32) -> bool,
) -> u32 {
    for si in (0..column.sections.len()).rev() {
        let section = &column.sections[si];
        for local_y in (0..16).rev() {
            let state = section.get((local_y * 16 + z) * 16 + x);
            if accept(state) {
                return (si * 16 + local_y + 1) as u32;
            }
        }
    }
    0
}

fn window_to_player_slot(window: usize) -> Option<i8> {
    match window {
        36..=44 => Some((window - 36) as i8),
        9..=35 => Some(window as i8),
        5 => Some(100),
        6 => Some(101),
        7 => Some(102),
        8 => Some(103),
        45 => Some(-106),
        _ => None,
    }
}

fn player_slot_to_window(slot: i32) -> Option<usize> {
    match slot {
        0..=8 => Some(36 + slot as usize),
        9..=35 => Some(slot as usize),
        100 => Some(5),
        101 => Some(6),
        102 => Some(7),
        103 => Some(8),
        -106 => Some(45),
        _ => None,
    }
}

fn player_windows() -> impl Iterator<Item = usize> {
    (9..36).chain(36..45).chain(5..9).chain(std::iter::once(45))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_names_are_namespaced() {
        assert_eq!(args_name("cow"), "minecraft:cow");
        assert_eq!(args_name("minecraft:cow"), "minecraft:cow");
    }

    #[test]
    fn entity_uuids_are_unique_by_id() {
        let a = entity_ref(1, [0.0; 3], "cow");
        let b = entity_ref(2, [0.0; 3], "cow");
        assert_ne!(a.uuid, b.uuid);
        assert_eq!(entity_ref(1, [1.0; 3], "cow").uuid, a.uuid);
        assert_eq!(entity_uuid(1), a.uuid);
    }

    #[test]
    fn block_filter_parses_once_and_matches() {
        let tags = HashMap::from([(
            "minecraft:dirt".to_owned(),
            HashSet::from(["minecraft:stone".to_owned()]),
        )]);
        let by_name = BlockFilter::new("stone", &tags);
        assert!(by_name.matches("minecraft:stone"));
        assert!(!by_name.matches("minecraft:dirt"));
        let full = BlockFilter::new("minecraft:stone", &tags);
        assert!(full.matches("minecraft:stone"));
        // Свойства в фильтре отбрасываются, как у ванили.
        let props = BlockFilter::new("minecraft:stone[axis=y]", &tags);
        assert!(props.matches("minecraft:stone"));
        let tag = BlockFilter::new("#dirt", &tags);
        assert!(tag.matches("minecraft:stone"));
        assert!(!tag.matches("minecraft:gravel"));
        // Неизвестный тег не совпадает ни с чем.
        let missing = BlockFilter::new("#nope", &tags);
        assert!(!missing.matches("minecraft:stone"));
    }
}
