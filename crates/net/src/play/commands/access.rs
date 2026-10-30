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
use std::sync::atomic::Ordering;
use std::collections::HashSet;
use uuid::Uuid;

impl PlayPlan {
    /// Координаты для телепорта командой (/tp).
    pub(crate) fn teleport_to(&self, uuid: Uuid, pos: [f64; 3], yaw: f32, pitch: f32) -> bool {
        self.teleport_player(uuid, pos, yaw, pitch)
    }

    /// Все цели мира для селекторов: игроки, мобы, выпавшие предметы, стрелы.
    pub(crate) fn command_targets(&self) -> Vec<Target> {
        let mut out: Vec<Target> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .map(|p| Target {
                uuid: p.entry.uuid,
                name: p.entry.name.clone(),
                entity_id: p.entity_id,
                pos: p.pos,
                yaw: p.yaw,
                pitch: p.pitch,
                game_mode: Some(0),
                tags: Vec::new(),
            })
            .collect();
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
        out
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

    /// Убирает и раскладывает предметы (`/give`).
    pub(crate) fn give_item(&self, uuid: Uuid, item: i32, count: i32) -> bool {
        self.push_control(uuid, Control::Give(item, count))
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

    /// `/kill`: игрока убивает сессия, сущность убираем из мира.
    pub(crate) fn kill_target(&self, target: &Target) -> bool {
        if target.game_mode.is_some() {
            return self.push_control(target.uuid, Control::Health(0.0));
        }
        self.remove_entity(target.entity_id)
    }

    /// Накладывает эффект (`/effect give`).
    pub(crate) fn give_effect(&self, uuid: Uuid, id: i32, amplifier: u8, duration: i32) -> bool {
        self.push_control(
            uuid,
            Control::Effect {
                effect: crate::survival::Effect {
                    id,
                    amplifier,
                    duration,
                    ambient: false,
                    particles: true,
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

    /// Точка возрождения игрока (`/spawnpoint`); `None` — сброс на спавн мира.
    pub(crate) fn set_player_spawn(&self, uuid: Uuid, pos: Option<[i32; 3]>) {
        self.spawn_points.lock().unwrap().insert(uuid, pos);
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
        self.block(pos).map(|state| self.state_name(state).to_owned())
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
            let packet = chunkwake_protocol::play::remove_entities(
                self.ids.remove_entities,
                &[entity_id],
            );
            self.hub
                .send_entity(None, packet, Some(crate::play::EntityEvent::Gone(entity_id)));
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

    /// Снимок блок-сущности для `/data get block`; `None` — её нет.
    pub(crate) fn block_entity_snapshot(&self, pos: [i32; 3]) -> Option<serde_json::Value> {
        let name = self.block_name_at(pos)?;
        // Контейнеры: координаты и вид, содержимое пока не отдаём.
        name.contains("chest").then(|| {
            serde_json::json!({
                "id": name,
                "x": pos[0],
                "y": pos[1],
                "z": pos[2],
            })
        })
    }

    /// Снимок сущности для `/data get entity`.
    pub(crate) fn entity_snapshot(&self, target: &Target) -> Option<serde_json::Value> {
        if target.game_mode.is_some() {
            let (pos, yaw, pitch) = self.player_at(target.uuid)?;
            let health = self
                .live
                .lock()
                .unwrap()
                .get(&target.uuid)
                .map(|l| l.health);
            return Some(serde_json::json!({
                "Pos": pos,
                "Rotation": [yaw, pitch],
                "Health": health,
            }));
        }
        let mobs = self.mobs.lock().unwrap();
        let entry = mobs.get(&target.entity_id)?;
        Some(serde_json::json!({
            "id": target.name,
            "Pos": entry.mob.pos,
            "Health": entry.mob.health,
        }))
    }

    /// Значение счёта для `/execute if score`.
    pub fn score(&self, holder: &str, objective: &str) -> Option<i32> {
        self.scoreboard.lock().unwrap().get(holder, objective)
    }

    /// Заливает область по правилам `/fill`; возвращает число изменённых блоков.
    pub(crate) fn fill_region(
        &self,
        from: [i32; 3],
        to: [i32; 3],
        state: u32,
        mode: super::world::FillMode,
    ) -> i64 {
        use super::world::FillMode;
        let (x1, x2) = (from[0].min(to[0]), from[0].max(to[0]));
        let (y1, y2) = (from[1].min(to[1]), from[1].max(to[1]));
        let (z1, z2) = (from[2].min(to[2]), from[2].max(to[2]));
        let mut origins = HashSet::new();
        let mut changed = 0;
        for y in y1..=y2 {
            for z in z1..=z2 {
                for x in x1..=x2 {
                    let edge = x == x1 || x == x2 || y == y1 || y == y2 || z == z1 || z == z2;
                    let place = match mode {
                        FillMode::Replace | FillMode::Destroy => true,
                        // `keep` не трогает непустые блоки, `hollow` — только рамку,
                        // `outline` — рамку и внутренний воздух.
                        FillMode::Keep => self
                            .block([x, y, z])
                            .is_none_or(|s| self.state_name(s).ends_with(":air")),
                        FillMode::Hollow => edge,
                        FillMode::Outline => true,
                    };
                    if !place {
                        continue;
                    }
                    let target = match mode {
                        FillMode::Outline if !edge => self
                            .survival
                            .block_defaults
                            .get("minecraft:air")
                            .copied()
                            .unwrap_or(state),
                        _ => state,
                    };
                    let position = [x, y, z];
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
    pub(crate) fn play_named_sound(
        &self,
        name: &str,
        uuid: Uuid,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
    ) -> bool {
        if !self.survival.sounds.contains_key(name) {
            return false;
        }
        self.send_to(
            uuid,
            chunkwake_protocol::play::sound(
                self.ids.sound,
                self.survival.sounds[name],
                0,
                pos,
                volume,
                pitch,
                rand::random(),
            ),
        );
        true
    }

    /// Останавливает звуки у игрока (`/stopsound`); `None` — все.
    pub(crate) fn stop_sound(&self, uuid: Uuid, name: Option<&str>) {
        // `stop_sound` принимает звук или `null` (все) и категорию `master`.
        let id = name.and_then(|n| self.survival.sounds.get(n).copied());
        self.send_to(
            uuid,
            chunkwake_protocol::play::stop_sound(self.ids.stop_sound, id, 0),
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
                false,
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
            chunkwake_protocol::play::set_objective(self.ids.set_objective, name, 1, &serde_json::Value::Null, 0, false, false),
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
            None => chunkwake_protocol::play::reset_score(
                self.ids.reset_score,
                holder,
                Some(objective),
            ),
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
        uuid: Uuid::from_u128(u128::from(entity_id as u32).rotate_left(32)),
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
    }
}
