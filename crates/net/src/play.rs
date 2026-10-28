//! Play-сессия M2: плоский мир, стриминг чанков, чат, таб-лист и видимость
//! других игроков. Мир статичен.

use crate::survival::{self, Cause, Difficulty, Inventory, Vitals};
use crate::{Connection, LoggedIn, NetError};
use chunkwake_data::DataError;
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Reader;
use chunkwake_protocol::frame::FrameCodec;
use chunkwake_protocol::inventory::{self as inv, Slot};
use chunkwake_protocol::play::{self, Login, Move, PlayerEntry, game_event};
use chunkwake_protocol::text;
use chunkwake_world::ChunkColumn;
use chunkwake_world::light::{Engine, LightAccess, LightTable};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// Сколько ждать ответа плагинов на отменяемое событие; дольше — не отменено.
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Хуки плагинов (реализует JVM-мост). Вызовы не блокируют: события уходят
/// в поток плагинов, ответы на отменяемые приходят через `oneshot`.
pub trait PluginHooks: Send + Sync {
    /// Вход игрока; сообщение о входе рассылают плагины.
    fn join(&self, uuid: Uuid, name: &str);
    fn quit(&self, uuid: Uuid);
    /// Ломка блока; `None` — слушателей нет, ждать нечего. `true` — отменено.
    fn block_break(
        &self,
        uuid: Uuid,
        pos: [i32; 3],
        block: &str,
    ) -> Option<oneshot::Receiver<bool>>;
    /// Команда без `/`; `true` — команда известна плагинам.
    fn command(&self, uuid: Uuid, line: &str) -> oneshot::Receiver<bool>;
    /// Тик сервера (20/с).
    fn tick(&self);
}

#[derive(Debug, Clone, Copy)]
struct Ids {
    login: i32,
    game_event: i32,
    set_chunk_cache_center: i32,
    level_chunk_with_light: i32,
    chunk_batch_start: i32,
    chunk_batch_finished: i32,
    player_position: i32,
    keep_alive: i32,
    forget_level_chunk: i32,
    player_info_update: i32,
    player_info_remove: i32,
    system_chat: i32,
    sb_chat: i32,
    sb_keep_alive: i32,
    sb_move_pos: i32,
    sb_move_pos_rot: i32,
    sb_move_rot: i32,
    add_entity: i32,
    teleport_entity: i32,
    rotate_head: i32,
    remove_entities: i32,
    move_entity_pos_rot: i32,
    block_update: i32,
    section_blocks_update: i32,
    block_changed_ack: i32,
    sb_player_action: i32,
    sb_use_item_on: i32,
    sb_set_creative_mode_slot: i32,
    sb_set_carried_item: i32,
    container_set_content: i32,
    container_set_slot: i32,
    set_health: i32,
    respawn: i32,
    damage_event: i32,
    entity_event: i32,
    sb_container_click: i32,
    sb_client_command: i32,
    sb_player_command: i32,
    sb_use_item: i32,
    commands: i32,
    sb_chat_command: i32,
    sb_chat_command_signed: i32,
}

pub struct PlayPlan {
    ids: Ids,
    login: Login,
    /// Тело чанка (одинаковое для всех координат плоского мира).
    chunk_body: Vec<u8>,
    /// Чанк генератора (плоский мир); изменённые чанки — в `edited`.
    template: ChunkColumn,
    /// Изменённые столбцы и номер их правки (для сверки с кэшем кадров).
    edited: Mutex<HashMap<(i32, i32), (ChunkColumn, u64)>>,
    /// Номер правки, записанный на диск, по столбцам.
    saved: Mutex<HashMap<(i32, i32), u64>>,
    /// Изменения блоков текущего тика: секция → (локальный индекс, состояние).
    pending_blocks: Mutex<PendingBlocks>,
    light: Arc<LightTable>,
    /// Ленивая загрузка с диска: источник и уже прочитанные регионы.
    source: std::sync::OnceLock<Arc<dyn ChunkSource>>,
    regions: Mutex<HashSet<(i32, i32)>>,
    region_load: Mutex<()>,
    min_y: i32,
    /// ID предмета → состояние блока, который он ставит.
    item_blocks: Vec<Option<u32>>,
    spawn: [f64; 3],
    /// ID типа сущности `minecraft:player` в реестре `entity_type`.
    player_type: i32,
    next_entity_id: AtomicI32,
    hub: Hub,
    /// Кадры `level_chunk_with_light`, уже сжатые: один раз на чанк для всех игроков.
    chunk_frames: Mutex<HashMap<(i32, i32), Frame>>,
    /// Состояние блока → имя (`minecraft:stone`), для событий плагинов.
    block_names: Arc<Vec<String>>,
    /// ID `brigadier:string` в `command_argument_type`.
    string_parser: i32,
    survival: SurvivalData,
    hooks: std::sync::OnceLock<Arc<dyn PluginHooks>>,
    /// Команды плагинов для дерева `commands` у клиента.
    plugin_commands: Mutex<Vec<String>>,
}

/// Готовый к записи кадр, общий для многих соединений.
type Frame = Arc<Vec<u8>>;

/// Предел кэша кадров чанков; при переполнении кэш сбрасывается целиком.
const CHUNK_CACHE_LIMIT: usize = 65_536;

/// Столбец с диска и его координаты.
pub type LoadedChunk = ((i32, i32), ChunkColumn);
/// Изменения блоков по секциям: (локальный индекс, состояние).
type PendingBlocks = HashMap<[i32; 3], Vec<(u16, u32)>>;

/// Мир для пересчёта света: читает правленые столбцы или шаблон,
/// при записи копирует шаблон в правленые и запоминает затронутые.
struct WorldLight<'a> {
    world: &'a mut HashMap<(i32, i32), (ChunkColumn, u64)>,
    template: &'a ChunkColumn,
    touched: HashSet<(i32, i32)>,
}

impl LightAccess for WorldLight<'_> {
    fn column(&mut self, cx: i32, cz: i32) -> Option<&ChunkColumn> {
        Some(self.world.get(&(cx, cz)).map_or(self.template, |e| &e.0))
    }
    fn column_mut(&mut self, cx: i32, cz: i32) -> Option<&mut ChunkColumn> {
        self.touched.insert((cx, cz));
        let template = self.template;
        Some(
            &mut self
                .world
                .entry((cx, cz))
                .or_insert_with(|| (template.clone(), 0))
                .0,
        )
    }
}

/// Источник сохранённых чанков: регион 32×32 читается целиком при первом обращении.
pub trait ChunkSource: Send + Sync {
    fn load_region(&self, rx: i32, rz: i32) -> Result<Vec<LoadedChunk>, String>;
}

/// Кадр для всех игроков; `skip` — кому не слать (обычно автору события).
#[derive(Clone)]
struct Broadcast {
    skip: Option<Uuid>,
    /// Только этому игроку (сообщения плагинов).
    only: Option<Uuid>,
    packet: Frame,
    /// Событие сущности: получатель сам решает по дальности, слать ли `packet`,
    /// заспавнить сущность или убрать её.
    entity: Option<EntityEvent>,
    /// Движения игроков за тик: получатель берёт только видимые.
    moves: Option<Arc<Vec<MoveItem>>>,
    /// Изменения блоков за тик по секциям: получатель берёт чанки в своём радиусе.
    blocks: Option<Arc<Vec<BlockItem>>>,
}

/// Изменения одной секции за тик, готовый кадр.
struct BlockItem {
    chunk: (i32, i32),
    frame: Vec<u8>,
}

/// Движение одной сущности в пачке тика: кадр(ы) дельты и состояние для спавна.
struct MoveItem {
    e: EntityRef,
    frame: Vec<u8>,
}

/// Состояние `minecraft:air`.
const AIR: u32 = 0;

/// Период рассылки движений (тик ванилы).
const TICK: Duration = Duration::from_millis(50);

#[derive(Clone, Copy)]
enum EntityEvent {
    /// Сущность появилась или сдвинулась; `packet` — дельта для тех, кто её уже видит.
    At(EntityRef),
    /// Сущность исчезла; `packet` слать только тем, кто её видел.
    Gone(i32),
}

/// Минимум для спавна сущности игрока у нового зрителя.
#[derive(Clone, Copy)]
struct EntityRef {
    entity_id: i32,
    uuid: Uuid,
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
}

/// Игрок онлайн: запись таб-листа и его сущность в мире.
#[derive(Clone)]
struct Online {
    entry: PlayerEntry,
    entity_id: i32,
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
    /// Позиция, от которой считаются дельты (последняя разосланная, квантованная).
    base: [i64; 3],
    /// Счётчик движений: каждое `SYNC_EVERY`-е — абсолютный телепорт против дрейфа.
    moves: u32,
    /// С прошлого тика были движение / поворот.
    dirty: bool,
    rotated: bool,
    on_ground: bool,
}

impl Online {
    fn at(&self) -> EntityRef {
        EntityRef {
            entity_id: self.entity_id,
            uuid: self.entry.uuid,
            pos: self.pos,
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }
}

/// Как часто вместо дельты слать абсолютную позицию (у ванилы — раз в 60 тиков).
const SYNC_EVERY: u32 = 60;

/// Видна ли сущность из чанка `center` при дальности `r` чанков.
fn near(center: (i32, i32), pos: [f64; 3], r: i32) -> bool {
    let (x, z) = ((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
    (x - center.0).abs() <= r && (z - center.1).abs() <= r
}

/// Общее состояние сессий: игроки онлайн и широковещательный канал.
struct Hub {
    /// Кодек кадров play (порог сжатия сервера): рассылка кодируется один раз.
    codec: FrameCodec,
    players: Mutex<HashMap<Uuid, Online>>,
    tx: broadcast::Sender<Broadcast>,
}

impl Hub {
    fn frame(&self, packet: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(packet.len() + 5);
        self.codec.encode(packet, &mut out);
        // Сжатый кадр намного меньше исходной ёмкости; не держим лишнее (кэш чанков).
        if out.capacity() > out.len() * 2 {
            out.shrink_to_fit();
        }
        out
    }

    fn send(&self, skip: Option<Uuid>, packet: Vec<u8>) {
        self.send_entity(skip, packet, None);
    }

    fn send_entity(&self, skip: Option<Uuid>, packet: Vec<u8>, entity: Option<EntityEvent>) {
        // Ошибка = нет подписчиков; это нормально.
        let _ = self.tx.send(Broadcast {
            skip,
            only: None,
            packet: Arc::new(if packet.is_empty() {
                packet
            } else {
                self.frame(&packet)
            }),
            entity,
            moves: None,
            blocks: None,
        });
    }
}

impl std::fmt::Debug for PlayPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayPlan")
            .field("chunk_body", &self.chunk_body.len())
            .finish_non_exhaustive()
    }
}

/// Мир и данные, нужные сессиям.
pub struct WorldSpec {
    /// Чанк генератора: копируется при первой правке столбца.
    pub template: ChunkColumn,
    pub min_y: i32,
    /// ID предмета → состояние блока, который он ставит.
    pub item_blocks: Vec<Option<u32>>,
    pub spawn: [f64; 3],
    /// ID `minecraft:player` в реестре `entity_type`.
    pub player_type: i32,
    /// Свойства света состояний блоков (шаблон уже освещён).
    pub light: Arc<LightTable>,
    /// Состояние блока → имя блока.
    pub block_names: Arc<Vec<String>>,
    /// ID `brigadier:string` в `command_argument_type`.
    pub string_parser: i32,
    pub survival: SurvivalData,
}

/// Данные для выживания.
/// Команды сервера (обрабатываются до плагинов).
const BUILTIN_COMMANDS: &[&str] = &["give"];

pub struct SurvivalData {
    /// Твёрдость по состоянию блока (-1 — неразрушимый, 0 — ломается сразу).
    pub hardness: Vec<f32>,
    /// Предмет, выпадающий из состояния блока.
    pub drops: Vec<Option<i32>>,
    /// Свойства предметов по ID: размер стопки, еда.
    pub items: Vec<chunkwake_data::world::ItemInfo>,
    /// ID `minecraft:fall` и `minecraft:out_of_world` в реестре `damage_type`.
    pub damage_fall: i32,
    pub damage_void: i32,
    pub damage_starve: i32,
    pub difficulty: Difficulty,
    /// ID предмета по имени (`minecraft:bread`) для `/give`.
    pub item_ids: HashMap<String, i32>,
}

impl PlayPlan {
    pub fn build(
        packets: &PacketTable,
        login: Login,
        world: WorldSpec,
        compression_threshold: i32,
    ) -> Result<Self, DataError> {
        let WorldSpec {
            template,
            min_y,
            item_blocks,
            spawn,
            player_type,
            light,
            block_names,
            string_parser,
            survival,
        } = world;
        let cb = |name| packets.id(State::Play, Direction::Clientbound, name);
        let sb = |name| packets.id(State::Play, Direction::Serverbound, name);
        Ok(Self {
            ids: Ids {
                login: cb("login")?,
                game_event: cb("game_event")?,
                set_chunk_cache_center: cb("set_chunk_cache_center")?,
                level_chunk_with_light: cb("level_chunk_with_light")?,
                chunk_batch_start: cb("chunk_batch_start")?,
                chunk_batch_finished: cb("chunk_batch_finished")?,
                player_position: cb("player_position")?,
                keep_alive: cb("keep_alive")?,
                forget_level_chunk: cb("forget_level_chunk")?,
                player_info_update: cb("player_info_update")?,
                player_info_remove: cb("player_info_remove")?,
                system_chat: cb("system_chat")?,
                sb_chat: sb("chat")?,
                sb_keep_alive: sb("keep_alive")?,
                sb_move_pos: sb("move_player_pos")?,
                sb_move_pos_rot: sb("move_player_pos_rot")?,
                sb_move_rot: sb("move_player_rot")?,
                add_entity: cb("add_entity")?,
                teleport_entity: cb("teleport_entity")?,
                rotate_head: cb("rotate_head")?,
                remove_entities: cb("remove_entities")?,
                move_entity_pos_rot: cb("move_entity_pos_rot")?,
                block_update: cb("block_update")?,
                section_blocks_update: cb("section_blocks_update")?,
                block_changed_ack: cb("block_changed_ack")?,
                sb_player_action: sb("player_action")?,
                sb_use_item_on: sb("use_item_on")?,
                sb_set_creative_mode_slot: sb("set_creative_mode_slot")?,
                sb_set_carried_item: sb("set_carried_item")?,
                container_set_content: cb("container_set_content")?,
                container_set_slot: cb("container_set_slot")?,
                set_health: cb("set_health")?,
                respawn: cb("respawn")?,
                damage_event: cb("damage_event")?,
                entity_event: cb("entity_event")?,
                sb_container_click: sb("container_click")?,
                sb_client_command: sb("client_command")?,
                sb_player_command: sb("player_command")?,
                sb_use_item: sb("use_item")?,
                commands: cb("commands")?,
                sb_chat_command: sb("chat_command")?,
                sb_chat_command_signed: sb("chat_command_signed")?,
            },
            login,
            chunk_body: template.encode_body(),
            template,
            edited: Mutex::new(HashMap::new()),
            saved: Mutex::new(HashMap::new()),
            pending_blocks: Mutex::new(HashMap::new()),
            light,
            source: std::sync::OnceLock::new(),
            regions: Mutex::new(HashSet::new()),
            region_load: Mutex::new(()),
            min_y,
            item_blocks,
            spawn,
            player_type,
            next_entity_id: AtomicI32::new(1),
            chunk_frames: Mutex::new(HashMap::new()),
            block_names,
            string_parser,
            survival,
            hooks: std::sync::OnceLock::new(),
            plugin_commands: Mutex::new(Vec::new()),
            hub: Hub {
                codec: FrameCodec {
                    threshold: (compression_threshold >= 0)
                        .then_some(compression_threshold as usize),
                },
                players: Mutex::new(HashMap::new()),
                tx: broadcast::channel(1024).0,
            },
        })
    }

    /// Сжатый кадр чанка из кэша (кодируется при первом запросе).
    fn chunk_frame(&self, x: i32, z: i32) -> Frame {
        if let Some(f) = self.chunk_frames.lock().unwrap().get(&(x, z)) {
            return f.clone();
        }
        self.ensure_region(x, z);
        // Под блокировкой — только дешёвый клон (секции за Arc);
        // кодирование и сжатие — вне её.
        let snapshot = self.edited.lock().unwrap().get(&(x, z)).cloned();
        let edited = snapshot.map(|(c, rev)| (c.encode_body(), rev));
        let (body, rev) = match &edited {
            Some((body, rev)) => (body.as_slice(), *rev),
            None => (self.chunk_body.as_slice(), 0),
        };
        let packet = play::level_chunk_with_light(self.ids.level_chunk_with_light, x, z, body);
        let frame = Arc::new(self.hub.frame(&packet));
        // В кэш — только если чанк не изменили, пока мы его сжимали.
        let world = self.edited.lock().unwrap();
        if world.get(&(x, z)).map_or(0, |e| e.1) == rev {
            let mut cache = self.chunk_frames.lock().unwrap();
            if cache.len() >= CHUNK_CACHE_LIMIT {
                cache.clear();
            }
            cache.insert((x, z), frame.clone());
        }
        frame
    }

    /// Ставит блок (мировые координаты) и рассылает `block_update`.
    /// `None` — позиция вне мира по высоте.
    fn set_block(&self, [x, y, z]: [i32; 3], state: u32) -> Option<u32> {
        let ry = y - self.min_y;
        if ry < 0 || ry as usize >= self.template.height() {
            return None;
        }
        let key = (x >> 4, z >> 4);
        // Свет доходит до соседних чанков: их регионы тоже должны быть прочитаны.
        for dx in -1..=1 {
            for dz in -1..=1 {
                self.ensure_region(key.0 + dx, key.1 + dz);
            }
        }
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        let old = {
            let mut world = self.edited.lock().unwrap();
            let (column, rev) = world
                .entry(key)
                .or_insert_with(|| (self.template.clone(), 0));
            let old = column.set_block(lx, ry as usize, lz, state);
            if old == state {
                return Some(old);
            }
            *rev += 1;
            let mut access = WorldLight {
                world: &mut world,
                template: &self.template,
                touched: HashSet::new(),
            };
            Engine::new(&mut access, &self.light, self.template.height()).block_changed([x, ry, z]);
            let mut touched = access.touched;
            touched.insert(key);
            // Под той же блокировкой: chunk_frame не вставит устаревший кадр.
            let mut frames = self.chunk_frames.lock().unwrap();
            for pos in touched {
                frames.remove(&pos);
                if pos != key
                    && let Some(e) = world.get_mut(&pos)
                {
                    e.1 += 1;
                }
            }
            old
        };
        if old != state {
            let local = ((x & 15) << 8 | (z & 15) << 4 | (y & 15)) as u16;
            self.pending_blocks
                .lock()
                .unwrap()
                .entry([x >> 4, y >> 4, z >> 4])
                .or_default()
                .push((local, state));
        }
        Some(old)
    }

    /// Подключает диск: дальше регионы читаются при первом обращении к их чанкам.
    /// Подключает плагины (один раз, до входа игроков).
    pub fn set_hooks(&self, hooks: Arc<dyn PluginHooks>) {
        let _ = self.hooks.set(hooks);
    }

    fn hooks(&self) -> Option<&Arc<dyn PluginHooks>> {
        self.hooks.get()
    }

    /// Системное сообщение (JSON-компонент) одному игроку.
    pub fn send_json(&self, to: Uuid, json: &str) {
        let packet = self
            .hub
            .frame(&text::system_chat_json(self.ids.system_chat, json, false));
        let _ = self.hub.tx.send(Broadcast {
            skip: None,
            only: Some(to),
            packet: Arc::new(packet),
            entity: None,
            moves: None,
            blocks: None,
        });
    }

    /// Системное сообщение (JSON-компонент) всем.
    pub fn broadcast_json(&self, json: &str) {
        self.hub.send(
            None,
            text::system_chat_json(self.ids.system_chat, json, false),
        );
    }

    /// Команды плагинов: запоминаем и обновляем дерево у всех онлайн.
    pub fn set_commands(&self, names: Vec<String>) {
        *self.plugin_commands.lock().unwrap() = names;
        self.hub.send(None, self.commands_packet());
    }

    /// Дерево команд для клиента: встроенные и команды плагинов.
    fn commands_packet(&self) -> Vec<u8> {
        let mut names: Vec<String> = BUILTIN_COMMANDS.iter().map(|&n| n.to_owned()).collect();
        names.extend(self.plugin_commands.lock().unwrap().iter().cloned());
        text::commands(self.ids.commands, &names, self.string_parser)
    }

    pub fn set_source(&self, source: Arc<dyn ChunkSource>) {
        let _ = self.source.set(source);
    }

    /// Гарантирует, что регион чанка `(x, z)` прочитан с диска.
    fn ensure_region(&self, x: i32, z: i32) {
        let Some(source) = self.source.get() else {
            return;
        };
        let key = (x >> 5, z >> 5);
        if self.regions.lock().unwrap().contains(&key) {
            return;
        }
        // Один загрузчик: два игрока не читают один регион дважды.
        let _guard = self.region_load.lock().unwrap();
        if self.regions.lock().unwrap().contains(&key) {
            return;
        }
        match source.load_region(key.0, key.1) {
            Ok(columns) => {
                let mut world = self.edited.lock().unwrap();
                let mut cache = self.chunk_frames.lock().unwrap();
                for (pos, c) in columns {
                    // Правка, сделанная до загрузки, не затирается диском.
                    if let std::collections::hash_map::Entry::Vacant(e) = world.entry(pos) {
                        e.insert((c, 0));
                        cache.remove(&pos);
                    }
                }
            }
            Err(e) => tracing::error!("region {key:?}: {e}"),
        }
        self.regions.lock().unwrap().insert(key);
    }

    /// Чанк генератора — основа для загрузки сохранённых столбцов.
    pub fn template(&self) -> &ChunkColumn {
        &self.template
    }

    /// Изменённые с последнего сохранения столбцы: копии и номера правок.
    /// После успешной записи передайте их в [`Self::mark_saved`].
    pub fn unsaved(&self) -> Vec<((i32, i32), ChunkColumn, u64)> {
        let world = self.edited.lock().unwrap();
        let saved = self.saved.lock().unwrap();
        world
            .iter()
            .filter(|(pos, (_, rev))| saved.get(pos) != Some(rev) && *rev != 0)
            .map(|(pos, (c, rev))| (*pos, c.clone(), *rev))
            .collect()
    }

    pub fn mark_saved(&self, chunks: impl IntoIterator<Item = ((i32, i32), u64)>) {
        self.saved.lock().unwrap().extend(chunks);
    }

    fn block(&self, [x, y, z]: [i32; 3]) -> Option<u32> {
        let ry = y - self.min_y;
        if ry < 0 || ry as usize >= self.template.height() {
            return None;
        }
        self.ensure_region(x >> 4, z >> 4);
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        let world = self.edited.lock().unwrap();
        let column = world
            .get(&(x >> 4, z >> 4))
            .map_or(&self.template, |e| &e.0);
        Some(column.block(lx, ry as usize, lz))
    }

    /// Центр, пачка чанков и её конец — одним буфером и одной записью в сокет.
    async fn send_chunks(
        &self,
        conn: &mut Connection,
        center: (i32, i32),
        coords: &[(i32, i32)],
        forget: &[(i32, i32)],
    ) -> Result<(), NetError> {
        let ids = self.ids;
        let hub = &self.hub;
        let frames: Vec<Frame> = coords
            .iter()
            .map(|&(x, z)| self.chunk_frame(x, z))
            .collect();
        let mut out = Vec::with_capacity(frames.iter().map(|f| f.len()).sum::<usize>() + 64);
        for &(x, z) in forget {
            out.extend(hub.frame(&play::forget_level_chunk(ids.forget_level_chunk, x, z)));
        }
        out.extend(hub.frame(&play::set_chunk_cache_center(
            ids.set_chunk_cache_center,
            center.0,
            center.1,
        )));
        out.extend(hub.frame(&play::chunk_batch_start(ids.chunk_batch_start)));
        for f in &frames {
            out.extend_from_slice(f);
        }
        out.extend(hub.frame(&play::chunk_batch_finished(
            ids.chunk_batch_finished,
            coords.len() as i32,
        )));
        conn.send_frames(&out).await
    }

    /// Игрок перешёл в другой чанк: выгружаем ушедшие из радиуса, шлём новые.
    async fn move_center(
        &self,
        conn: &mut Connection,
        old: (i32, i32),
        new: (i32, i32),
        r: i32,
    ) -> Result<(), NetError> {
        let forget = chunks_around(old, r, Some((new, r)));
        self.send_chunks(conn, new, &chunks_around(new, r, Some((old, r))), &forget)
            .await
    }

    /// Сессия игрока: вход в таб-лист и мир, игра, выход с оповещением остальных.
    pub async fn run(&self, player: &LoggedIn, conn: &mut Connection) -> Result<(), NetError> {
        let ids = self.ids;
        let me = Online {
            entry: PlayerEntry {
                uuid: player.uuid,
                name: player.name.clone(),
                properties: player.properties.clone(),
                game_mode: self.login.spawn.game_mode as i32,
                latency_ms: 0,
            },
            entity_id: self.next_entity_id.fetch_add(1, Ordering::Relaxed),
            pos: self.spawn,
            yaw: 0.0,
            pitch: 0.0,
            base: play::quantize(self.spawn),
            moves: 0,
            dirty: false,
            rotated: false,
            on_ground: true,
        };
        let uuid = me.entry.uuid;
        // Подписка до вставки в список: ни одно событие после снимка не теряется.
        let rx = self.hub.tx.subscribe();
        let others: Vec<EntityRef> = {
            let mut players = self.hub.players.lock().unwrap();
            let others = players.values().map(Online::at).collect();
            players.insert(uuid, me.clone());
            others
        };
        self.hub.send(
            Some(uuid),
            play::player_info_add(ids.player_info_update, std::slice::from_ref(&me.entry)),
        );
        // Спавн каждый получатель строит сам, если новичок в его зоне видимости.
        self.hub
            .send_entity(Some(uuid), Vec::new(), Some(EntityEvent::At(me.at())));
        match self.hooks() {
            Some(h) => h.join(uuid, &me.entry.name),
            None => self.hub.send(
                None,
                play::system_chat(
                    ids.system_chat,
                    &format!("{} joined the game", me.entry.name),
                    false,
                ),
            ),
        }

        let result = self.session(conn, rx, &me, &others).await;

        self.hub.players.lock().unwrap().remove(&uuid);
        self.hub.send_entity(
            None,
            play::remove_entities(ids.remove_entities, &[me.entity_id]),
            Some(EntityEvent::Gone(me.entity_id)),
        );
        self.hub.send(
            None,
            play::player_info_remove(ids.player_info_remove, &[uuid]),
        );
        match self.hooks() {
            Some(h) => h.quit(uuid),
            None => self.hub.send(
                None,
                play::system_chat(
                    ids.system_chat,
                    &format!("{} left the game", me.entry.name),
                    false,
                ),
            ),
        }
        result
    }

    fn spawn_frame(&self, e: &EntityRef) -> Vec<u8> {
        self.hub.frame(&play::add_entity(
            self.ids.add_entity,
            e.entity_id,
            e.uuid,
            self.player_type,
            e.pos,
            e.yaw,
            e.pitch,
            0,
        ))
    }

    /// Новая позиция/поворот игрока: только обновить состояние. Рассылка — в тике.
    fn moved(&self, uuid: Uuid, m: Move) {
        let mut players = self.hub.players.lock().unwrap();
        let Some(p) = players.get_mut(&uuid) else {
            return;
        };
        if let Some(pos) = m.pos {
            p.pos = pos;
        }
        if let Some((yaw, pitch)) = m.rot {
            (p.yaw, p.pitch) = (yaw, pitch);
            p.rotated = true;
        }
        p.on_ground = m.on_ground;
        p.dirty = true;
    }

    /// Запускает тик рассылки движений: раз в 50 мс одна пачка на всех.
    pub fn spawn_tick(self: &Arc<Self>) {
        let plan = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(TICK);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let Some(plan) = plan.upgrade() else { return };
                plan.flush_blocks();
                plan.flush_moves();
                if let Some(h) = plan.hooks() {
                    h.tick();
                }
            }
        });
    }

    /// Изменения блоков за тик: по кадру на секцию (`block_update` для одного
    /// блока, иначе `section_blocks_update`), одной рассылкой.
    fn flush_blocks(&self) {
        let pending = std::mem::take(&mut *self.pending_blocks.lock().unwrap());
        if pending.is_empty() {
            return;
        }
        let ids = self.ids;
        let batch: Vec<BlockItem> = pending
            .into_iter()
            .map(|([sx, sy, sz], blocks)| {
                let packet = match blocks.as_slice() {
                    [(local, state)] => {
                        let l = *local as i32;
                        let pos = [
                            sx * 16 + (l >> 8),
                            sy * 16 + (l & 15),
                            sz * 16 + (l >> 4 & 15),
                        ];
                        play::block_update(ids.block_update, pos, *state as i32)
                    }
                    _ => play::section_blocks_update(
                        ids.section_blocks_update,
                        [sx, sy, sz],
                        &blocks,
                    ),
                };
                BlockItem {
                    chunk: (sx, sz),
                    frame: self.hub.frame(&packet),
                }
            })
            .collect();
        let _ = self.hub.tx.send(Broadcast {
            skip: None,
            only: None,
            packet: Arc::new(Vec::new()),
            entity: None,
            moves: None,
            blocks: Some(Arc::new(batch)),
        });
    }

    /// Собирает движения за тик в одну рассылку. Без движений ничего не шлёт.
    fn flush_moves(&self) {
        let ids = self.ids;
        let mut batch = Vec::new();
        {
            let mut players = self.hub.players.lock().unwrap();
            for p in players.values_mut().filter(|p| p.dirty) {
                p.dirty = false;
                let q = play::quantize(p.pos);
                p.moves = p.moves.wrapping_add(1);
                let delta = play::delta(p.base, q).filter(|_| p.moves % SYNC_EVERY != 0);
                p.base = q;
                let e = p.at();
                let packet = match delta {
                    Some(d) => play::move_entity_pos_rot(
                        ids.move_entity_pos_rot,
                        e.entity_id,
                        d,
                        e.yaw,
                        e.pitch,
                        p.on_ground,
                    ),
                    None => play::teleport_entity(
                        ids.teleport_entity,
                        e.entity_id,
                        e.pos,
                        e.yaw,
                        e.pitch,
                        p.on_ground,
                    ),
                };
                let mut frame = self.hub.frame(&packet);
                if std::mem::take(&mut p.rotated) {
                    frame.extend(self.hub.frame(&play::rotate_head(
                        ids.rotate_head,
                        e.entity_id,
                        e.yaw,
                    )));
                }
                batch.push(MoveItem { e, frame });
            }
        }
        if batch.is_empty() {
            return;
        }
        let _ = self.hub.tx.send(Broadcast {
            skip: None,
            only: None,
            packet: Arc::new(Vec::new()),
            entity: None,
            moves: Some(Arc::new(batch)),
            blocks: None,
        });
    }

    /// Пересчёт видимости всех игроков после смены своего чанка.
    fn refresh_visible(
        &self,
        me: Uuid,
        center: (i32, i32),
        r: i32,
        visible: &mut HashSet<i32>,
        out: &mut Vec<u8>,
    ) {
        let all: Vec<EntityRef> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.entry.uuid != me)
            .map(Online::at)
            .collect();
        let mut gone = Vec::new();
        for e in &all {
            match (near(center, e.pos, r), visible.contains(&e.entity_id)) {
                (true, false) => {
                    out.extend(self.spawn_frame(e));
                    visible.insert(e.entity_id);
                }
                (false, true) => {
                    gone.push(e.entity_id);
                    visible.remove(&e.entity_id);
                }
                _ => {}
            }
        }
        if !gone.is_empty() {
            out.extend(
                self.hub
                    .frame(&play::remove_entities(self.ids.remove_entities, &gone)),
            );
        }
    }

    async fn session(
        &self,
        conn: &mut Connection,
        mut rx: broadcast::Receiver<Broadcast>,
        me: &Online,
        others: &[EntityRef],
    ) -> Result<(), NetError> {
        let ids = self.ids;
        let uuid = me.entry.uuid;
        let mut login = self.login.clone();
        login.entity_id = me.entity_id;
        conn.send(&play::login(ids.login, &login)).await?;
        conn.send(&self.commands_packet()).await?;
        conn.send(&play::game_event(
            ids.game_event,
            game_event::LEVEL_CHUNKS_LOAD_START,
            0.0,
        ))
        .await?;

        let (cx, cz) = (
            (self.spawn[0].floor() as i32) >> 4,
            (self.spawn[2].floor() as i32) >> 4,
        );
        let r_view = login.view_distance;
        self.send_chunks(conn, (cx, cz), &chunks_around((cx, cz), r_view, None), &[])
            .await?;
        let mut center = (cx, cz);
        conn.send(&play::player_position(
            ids.player_position,
            1,
            self.spawn,
            0.0,
            0.0,
        ))
        .await?;
        // Таб-лист — все игроки; сущности — только в зоне видимости.
        let tab: Vec<PlayerEntry> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .map(|p| p.entry.clone())
            .collect();
        conn.send(&play::player_info_add(ids.player_info_update, &tab))
            .await?;
        let mut visible: HashSet<i32> = HashSet::new();
        let mut out = Vec::new();
        for e in others.iter().filter(|e| near(center, e.pos, r_view)) {
            out.extend(self.spawn_frame(e));
            visible.insert(e.entity_id);
        }
        if !out.is_empty() {
            conn.send_frames(&out).await?;
        }

        // Keep-alive: шлём раз в 15 с, ответ ждём не дольше 30 с.
        let mut ticker = tokio::time::interval(KEEP_ALIVE_INTERVAL);
        ticker.tick().await;
        // Выживание: инвентарь, здоровье. Креатив (1) и наблюдатель (3) урона не получают.
        let survival = matches!(login.spawn.game_mode, 0 | 2);
        let mut inv = Inventory::default();
        let mut vitals = Vitals::default();
        vitals.reset_fall(self.spawn);
        let mut last_void: Option<Instant> = None;
        let mut teleport_id = 1;
        // Поедание: слот хотбара, еда и момент окончания.
        let mut eating: Option<(usize, chunkwake_data::world::Food, Instant)> = None;
        let mut food_ticker = tokio::time::interval(TICK);
        food_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        self.send_inventory(conn, &mut inv).await?;
        self.send_health(conn, &vitals).await?;
        let mut pending: Option<(i64, Instant)> = None;
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Как в ваниле: новый keep-alive только после ответа на предыдущий.
                    match pending {
                        Some((_, sent)) if sent.elapsed() > KEEP_ALIVE_TIMEOUT => {
                            return Err(NetError::Timeout);
                        }
                        Some(_) => {}
                        None => {
                            let payload = rand::random::<i64>();
                            conn.send(&play::keep_alive(ids.keep_alive, payload)).await?;
                            pending = Some((payload, Instant::now()));
                        }
                    }
                }
                _ = food_ticker.tick(), if survival => {
                    if let Some((slot, food, done)) = eating
                        && Instant::now() >= done
                    {
                        eating = None;
                        // Предмет мог смениться (клики): едим, только если еда на месте.
                        let same = inv.slots[slot].and_then(|s| self.survival.items.get(s.item as usize))
                            .and_then(|i| i.food) == Some(food);
                        if same && slot == inv.held_slot() && inv.consume_held() {
                            vitals.eat(food.nutrition, food.saturation);
                            self.send_slots(conn, &mut inv, &[slot]).await?;
                            self.send_health(conn, &vitals).await?;
                            conn.send(&inv::entity_event(ids.entity_event, me.entity_id, inv::ENTITY_EVENT_USE_ITEM_COMPLETE)).await?;
                        }
                    }
                    let t = vitals.tick(self.survival.difficulty);
                    if let Some(d) = t.starve {
                        self.hurt(conn, me, &mut vitals, d, Cause::Starve).await?;
                    } else if t.changed {
                        self.send_health(conn, &vitals).await?;
                    }
                }
                event = rx.recv() => match event {
                    Ok(first) => {
                        // Забираем всё, что уже накопилось, и пишем одним вызовом.
                        out.clear();
                        let mut next = Some(first);
                        while let Some(b) = next {
                            if b.skip != Some(uuid) && b.only.is_none_or(|o| o == uuid) {
                                self.deliver(uuid, &b, center, r_view, &mut visible, &mut out);
                            }
                            next = rx.try_recv().ok();
                        }
                        if !out.is_empty() {
                            conn.send_frames(&out).await?;
                        }
                    }
                    // Отстали — пропущенное не догоняем (позиции выровняет периодический телепорт).
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                },
                packet = conn.read_packet() => {
                    let packet = packet?;
                    let mut r = Reader::new(&packet);
                    let id = r.var_int()?;
                    let (has_pos, has_rot) = if id == ids.sb_move_pos {
                        (true, false)
                    } else if id == ids.sb_move_pos_rot {
                        (true, true)
                    } else if id == ids.sb_move_rot {
                        (false, true)
                    } else {
                        (false, false)
                    };
                    if has_pos || has_rot {
                        let m = play::read_move(&mut r, has_pos, has_rot)?;
                        if vitals.dead {
                            continue;
                        }
                        let y = m.pos.map(|p| p[1]);
                        if let Some(d) = vitals.on_move(m.pos, m.on_ground, survival) {
                            self.hurt(conn, me, &mut vitals, d, Cause::Fall).await?;
                        }
                        let below_void = y.is_some_and(|y| y < self.min_y as f64 - survival::VOID_DEPTH);
                        if survival && below_void
                            && last_void.is_none_or(|t| t.elapsed() >= TICK * survival::VOID_INTERVAL_TICKS as u32)
                        {
                            last_void = Some(Instant::now());
                            self.hurt(conn, me, &mut vitals, survival::VOID_DAMAGE, Cause::Void).await?;
                        }
                        if let Some([x, _, z]) = m.pos {
                            let now = ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4);
                            if now != center {
                                self.move_center(conn, center, now, r_view).await?;
                                center = now;
                                out.clear();
                                self.refresh_visible(uuid, center, r_view, &mut visible, &mut out);
                                if !out.is_empty() {
                                    conn.send_frames(&out).await?;
                                }
                            }
                        }
                        self.moved(uuid, m);
                        continue;
                    }
                    if id == ids.sb_set_carried_item {
                        inv.selected = (r.u16()? as usize).min(8);
                        eating = None;
                        continue;
                    }
                    if id == ids.sb_set_creative_mode_slot {
                        let (slot, stack) = inv::read_creative_slot(&mut r)?;
                        if !survival {
                            inv.set(slot, stack);
                        }
                        continue;
                    }
                    if id == ids.sb_container_click {
                        let c = inv::read_container_click(&mut r)?;
                        if c.window == inv::PLAYER_WINDOW {
                            inv.apply_click(&c);
                            // Выброс (Q) не поддержан: предметов-сущностей ещё нет — откатываем.
                            if c.mode == 4 {
                                self.send_inventory(conn, &mut inv).await?;
                            }
                        }
                        continue;
                    }
                    if id == ids.sb_use_item {
                        let hand = r.var_int()?;
                        let sequence = r.var_int()?;
                        let food = inv.held()
                            .filter(|_| hand == 0 && survival && !vitals.dead)
                            .and_then(|s| self.survival.items.get(s.item as usize))
                            .and_then(|i| i.food)
                            .filter(|f| f.can_always_eat || vitals.food < survival::MAX_FOOD);
                        eating = food.map(|f| (inv.held_slot(), f, Instant::now() + Duration::from_secs_f32(f.seconds)));
                        conn.send(&play::block_changed_ack(ids.block_changed_ack, sequence)).await?;
                        continue;
                    }
                    if id == ids.sb_player_command {
                        let _entity = r.var_int()?;
                        match r.var_int()? {
                            play::START_SPRINTING => vitals.sprinting = true,
                            play::STOP_SPRINTING => vitals.sprinting = false,
                            _ => {}
                        }
                        continue;
                    }
                    if id == ids.sb_client_command {
                        if r.var_int()? == inv::PERFORM_RESPAWN && vitals.dead {
                            teleport_id += 1;
                            center = self.respawn(conn, me, &login, &mut vitals, &mut inv, center, teleport_id).await?;
                            out.clear();
                            self.refresh_visible(uuid, center, r_view, &mut visible, &mut out);
                            if !out.is_empty() {
                                conn.send_frames(&out).await?;
                            }
                        }
                        continue;
                    }
                    if id == ids.sb_player_action {
                        let a = play::read_player_action(&mut r)?;
                        if vitals.dead {
                            continue;
                        }
                        match a.action {
                            play::START_DESTROY_BLOCK | play::ABORT_DESTROY_BLOCK | play::STOP_DESTROY_BLOCK => {
                                let hardness = self.hardness(a.pos);
                                // Креатив и мгновенные блоки — сразу при ударе; остальное — по
                                // окончании ломки (время пока не проверяем).
                                let now = match a.action {
                                    play::START_DESTROY_BLOCK => !survival || hardness == 0.0,
                                    play::STOP_DESTROY_BLOCK => survival && hardness >= 0.0,
                                    _ => false,
                                };
                                if now {
                                    self.break_block(conn, uuid, a.pos, survival, &mut inv).await?;
                                    if survival {
                                        vitals.exhaust(survival::EXHAUSTION_BREAK);
                                    }
                                }
                                conn.send(&play::block_changed_ack(ids.block_changed_ack, a.sequence)).await?;
                            }
                            // Выбросить предмет нельзя (нет предметов-сущностей): вернуть слот.
                            play::DROP_ALL_ITEMS | play::DROP_ITEM => {
                                let held = inv.held_slot();
                                self.send_slots(conn, &mut inv, &[held]).await?;
                            }
                            play::RELEASE_USE_ITEM => eating = None,
                            play::SWAP_ITEM_WITH_OFFHAND => {
                                let held = inv.held_slot();
                                inv.slots.swap(held, survival::OFFHAND);
                                self.send_slots(conn, &mut inv, &[held, survival::OFFHAND]).await?;
                            }
                            _ => {}
                        }
                        continue;
                    }
                    if id == ids.sb_use_item_on {
                        let u = play::read_use_item_on(&mut r)?;
                        let state = inv
                            .held()
                            .filter(|_| u.hand == 0 && !vitals.dead)
                            .and_then(|s| self.item_blocks.get(s.item as usize).copied().flatten());
                        if let (Some(state), Some(d)) = (state, play::direction_offset(u.face)) {
                            let target = [u.pos[0] + d[0], u.pos[1] + d[1], u.pos[2] + d[2]];
                            if self.block(target) == Some(AIR) {
                                self.set_block(target, state);
                                if survival && inv.consume_held() {
                                    let held = inv.held_slot();
                                    self.send_slots(conn, &mut inv, &[held]).await?;
                                }
                            }
                        }
                        conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence)).await?;
                        continue;
                    }
                    if id == ids.sb_chat_command || id == ids.sb_chat_command_signed {
                        let line = r.string(32767)?.to_owned();
                        tracing::info!("{} issued server command: /{line}", me.entry.name);
                        if let Some(args) = line.strip_prefix("give ") {
                            let msg = self.give(conn, &mut inv, args).await?;
                            conn.send(&text::system_chat_json(ids.system_chat, &msg.to_string(), false)).await?;
                            continue;
                        }
                        let known = match self.hooks() {
                            Some(h) => {
                                let rx = h.command(uuid, &line);
                                tokio::time::timeout(Duration::from_secs(5), rx)
                                    .await
                                    .is_ok_and(|r| r.unwrap_or(false))
                            }
                            None => false,
                        };
                        if !known {
                            let msg = serde_json::json!({"text": format!("Unknown command: /{line}"), "color": "red"});
                            conn.send(&text::system_chat_json(ids.system_chat, &msg.to_string(), false)).await?;
                        }
                        continue;
                    }
                    if id == ids.sb_chat {
                        let text = play::read_chat_message(&mut r)?;
                        let line = format!("<{}> {text}", me.entry.name);
                        tracing::info!("{line}");
                        self.hub.send(None, play::system_chat(ids.system_chat, &line, false));
                        continue;
                    }
                    if id == ids.sb_keep_alive && pending.is_some_and(|(p, _)| p == r.i64().unwrap_or(0)) {
                        pending = None;
                    }
                    // Остальное (настройки, подтверждение телепорта) — с M3.
                }
            }
        }
    }

    fn hardness(&self, pos: [i32; 3]) -> f32 {
        let state = self.block(pos).unwrap_or(AIR);
        self.survival
            .hardness
            .get(state as usize)
            .copied()
            .unwrap_or(0.0)
    }

    /// Ломка блока игроком: плагины, затем мир; в выживании — дроп в инвентарь.
    async fn break_block(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        pos: [i32; 3],
        survival: bool,
        inv: &mut Inventory,
    ) -> Result<(), NetError> {
        if self.plugins_cancel_break(uuid, pos).await {
            // Отменено: вернуть блок клиенту.
            let old = self.block(pos).unwrap_or(AIR);
            return conn
                .send(&play::block_update(self.ids.block_update, pos, old as i32))
                .await;
        }
        let Some(old) = self.set_block(pos, AIR) else {
            return Ok(());
        };
        // Предметов-сущностей пока нет: дроп сразу в инвентарь.
        if survival && let Some(item) = self.survival.drops.get(old as usize).copied().flatten() {
            let max = self
                .survival
                .items
                .get(item as usize)
                .map_or(64, |i| i.max_stack);
            let changed = inv.add(item, 1, max as i32);
            self.send_slots(conn, inv, &changed).await?;
        }
        Ok(())
    }

    async fn send_inventory(
        &self,
        conn: &mut Connection,
        inv: &mut Inventory,
    ) -> Result<(), NetError> {
        let state = inv.next_state();
        conn.send(&inv::container_set_content(
            self.ids.container_set_content,
            inv::PLAYER_WINDOW,
            state,
            &inv.slots,
            inv.carried,
        ))
        .await
    }

    /// Встроенная `/give <предмет> [количество]`: выдаёт предмет себе.
    async fn give(
        &self,
        conn: &mut Connection,
        inv: &mut Inventory,
        args: &str,
    ) -> Result<serde_json::Value, NetError> {
        let mut it = args.split_whitespace();
        let name = it.next().unwrap_or("");
        let name = if name.contains(':') {
            name.to_owned()
        } else {
            format!("minecraft:{name}")
        };
        let count = it.next().map_or(Ok(1), str::parse::<i32>);
        let (Some(&item), Ok(count @ 1..=6400)) = (self.survival.item_ids.get(&name), count) else {
            return Ok(serde_json::json!({"text": "Usage: /give <item> [1-6400]", "color": "red"}));
        };
        let max = self
            .survival
            .items
            .get(item as usize)
            .map_or(64, |i| i.max_stack);
        let changed = inv.add(item, count, max as i32);
        self.send_slots(conn, inv, &changed).await?;
        Ok(serde_json::json!({"text": format!("Gave {count} {name}")}))
    }

    async fn send_slots(
        &self,
        conn: &mut Connection,
        inv: &mut Inventory,
        slots: &[usize],
    ) -> Result<(), NetError> {
        let mut out = Vec::new();
        for &s in slots {
            let state = inv.next_state();
            let stack: Slot = inv.slots[s];
            out.extend(self.hub.frame(&inv::container_set_slot(
                self.ids.container_set_slot,
                inv::PLAYER_WINDOW,
                state,
                s as i16,
                stack,
            )));
        }
        if out.is_empty() {
            return Ok(());
        }
        conn.send_frames(&out).await
    }

    async fn send_health(&self, conn: &mut Connection, v: &Vitals) -> Result<(), NetError> {
        conn.send(&inv::set_health(
            self.ids.set_health,
            v.health,
            v.food,
            v.saturation,
        ))
        .await
    }

    /// Урон игроку: здоровье, анимация у всех, при смерти — сообщение и падение.
    async fn hurt(
        &self,
        conn: &mut Connection,
        me: &Online,
        v: &mut Vitals,
        amount: f32,
        cause: Cause,
    ) -> Result<(), NetError> {
        let died = v.damage(amount);
        self.send_health(conn, v).await?;
        let damage_type = match cause {
            Cause::Fall => self.survival.damage_fall,
            Cause::Void => self.survival.damage_void,
            Cause::Starve => self.survival.damage_starve,
        };
        self.hub.send(
            None,
            inv::damage_event(self.ids.damage_event, me.entity_id, damage_type),
        );
        if died {
            self.hub.send(
                Some(me.entry.uuid),
                inv::entity_event(self.ids.entity_event, me.entity_id, inv::ENTITY_EVENT_DEATH),
            );
            let msg = serde_json::json!({"translate": cause.death_key(), "with": [me.entry.name]});
            tracing::info!("{} died ({})", me.entry.name, cause.death_key());
            self.broadcast_json(&msg.to_string());
        }
        Ok(())
    }

    /// Респаун после смерти: новый игрок у клиента в точке спавна, полное
    /// здоровье; инвентарь сохраняется. Возвращает новый центр чанков.
    #[allow(clippy::too_many_arguments)]
    async fn respawn(
        &self,
        conn: &mut Connection,
        me: &Online,
        login: &Login,
        v: &mut Vitals,
        inv: &mut Inventory,
        center: (i32, i32),
        teleport_id: i32,
    ) -> Result<(i32, i32), NetError> {
        v.respawn();
        v.reset_fall(self.spawn);
        conn.send(&inv::respawn(self.ids.respawn, &login.spawn, 0))
            .await?;
        let spawn_chunk = (
            (self.spawn[0].floor() as i32) >> 4,
            (self.spawn[2].floor() as i32) >> 4,
        );
        if spawn_chunk != center {
            self.move_center(conn, center, spawn_chunk, login.view_distance)
                .await?;
        }
        conn.send(&play::game_event(
            self.ids.game_event,
            game_event::LEVEL_CHUNKS_LOAD_START,
            0.0,
        ))
        .await?;
        conn.send(&play::player_position(
            self.ids.player_position,
            teleport_id,
            self.spawn,
            0.0,
            0.0,
        ))
        .await?;
        self.send_health(conn, v).await?;
        self.send_inventory(conn, inv).await?;
        // Остальные: убрать тело и показать игрока заново в точке спавна.
        self.hub.send_entity(
            Some(me.entry.uuid),
            play::remove_entities(self.ids.remove_entities, &[me.entity_id]),
            Some(EntityEvent::Gone(me.entity_id)),
        );
        self.moved(
            me.entry.uuid,
            Move {
                pos: Some(self.spawn),
                rot: Some((0.0, 0.0)),
                on_ground: true,
            },
        );
        tracing::info!("{} respawned", me.entry.name);
        Ok(spawn_chunk)
    }

    /// Спрашивает плагины, отменить ли ломку блока. Без слушателей — сразу «нет».
    async fn plugins_cancel_break(&self, uuid: Uuid, pos: [i32; 3]) -> bool {
        let Some(h) = self.hooks() else { return false };
        let state = self.block(pos).unwrap_or(AIR);
        let name = self
            .block_names
            .get(state as usize)
            .map_or("minecraft:air", String::as_str);
        let Some(rx) = h.block_break(uuid, pos, name) else {
            return false;
        };
        tokio::time::timeout(PLUGIN_TIMEOUT, rx)
            .await
            .is_ok_and(|r| r.unwrap_or(false))
    }

    /// Кладёт в `out` то, что этот получатель должен увидеть из рассылки `b`.
    fn deliver(
        &self,
        me: Uuid,
        b: &Broadcast,
        center: (i32, i32),
        r: i32,
        visible: &mut HashSet<i32>,
        out: &mut Vec<u8>,
    ) {
        if let Some(batch) = &b.blocks {
            for item in batch.iter() {
                let (x, z) = item.chunk;
                if (x - center.0).abs() <= r && (z - center.1).abs() <= r {
                    out.extend_from_slice(&item.frame);
                }
            }
            return;
        }
        if let Some(batch) = &b.moves {
            for m in batch.iter().filter(|m| m.e.uuid != me) {
                self.track(&m.e, &m.frame, center, r, visible, out);
            }
            return;
        }
        match b.entity {
            None => out.extend_from_slice(&b.packet),
            Some(EntityEvent::Gone(id)) => {
                if visible.remove(&id) {
                    out.extend_from_slice(&b.packet);
                }
            }
            Some(EntityEvent::At(e)) => self.track(&e, &b.packet, center, r, visible, out),
        }
    }

    /// Видимость одной сущности у получателя: дельта, спавн, удаление или ничего.
    fn track(
        &self,
        e: &EntityRef,
        frame: &[u8],
        center: (i32, i32),
        r: i32,
        visible: &mut HashSet<i32>,
        out: &mut Vec<u8>,
    ) {
        match (near(center, e.pos, r), visible.contains(&e.entity_id)) {
            (true, true) => out.extend_from_slice(frame),
            (true, false) => {
                out.extend(self.spawn_frame(e));
                visible.insert(e.entity_id);
            }
            (false, true) => {
                out.extend(self.hub.frame(&play::remove_entities(
                    self.ids.remove_entities,
                    &[e.entity_id],
                )));
                visible.remove(&e.entity_id);
            }
            (false, false) => {}
        }
    }
}

/// Чанки квадрата радиуса `r` вокруг `c`, ближние первыми, кроме попадающих в `skip`.
fn chunks_around(c: (i32, i32), r: i32, skip: Option<((i32, i32), i32)>) -> Vec<(i32, i32)> {
    let inside = |(x, z): (i32, i32), (s, sr): ((i32, i32), i32)| {
        (x - s.0).abs() <= sr && (z - s.1).abs() <= sr
    };
    let mut coords: Vec<(i32, i32)> = (-r..=r)
        .flat_map(|dx| (-r..=r).map(move |dz| (c.0 + dx, c.1 + dz)))
        .filter(|&p| !skip.is_some_and(|s| inside(p, s)))
        .collect();
    coords.sort_by_key(|&(x, z)| (x - c.0).pow(2) + (z - c.1).pow(2));
    coords
}

#[cfg(test)]
mod tests {
    use super::{chunks_around, near};

    #[test]
    fn visibility_range() {
        assert!(near((0, 0), [15.9, 0.0, -16.0], 1));
        assert!(!near((0, 0), [32.0, 0.0, 0.0], 1));
        assert!(near((2, 0), [32.0, 0.0, 0.0], 0));
    }

    #[test]
    fn chunk_window_shift() {
        assert_eq!(chunks_around((0, 0), 2, None).len(), 25);
        assert_eq!(chunks_around((0, 0), 2, None)[0], (0, 0));
        // Шаг на один чанк по x: одна новая колонка из 5 чанков.
        let new = chunks_around((1, 0), 2, Some(((0, 0), 2)));
        assert_eq!(new.len(), 5);
        assert!(new.iter().all(|&(x, _)| x == 3));
        let gone = chunks_around((0, 0), 2, Some(((1, 0), 2)));
        assert!(gone.iter().all(|&(x, _)| x == -2));
    }
}
