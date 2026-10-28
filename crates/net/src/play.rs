//! Play-сессия M2: плоский мир, стриминг чанков, чат, таб-лист и видимость
//! других игроков. Мир статичен.

use crate::click::{self, Kind};
use crate::items::{self, Dropped};
use crate::live::{self, Control, Event, Live};
use crate::ops::{self, OpList};
use crate::survival::{self, Cause, Difficulty, Inventory, PlayerSave, PlayerState, Vitals};
use crate::{Connection, LoggedIn, NetError};
use chunkwake_data::DataError;
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Reader;
use chunkwake_protocol::frame::FrameCodec;
use chunkwake_protocol::inventory::{self as inv, Slot, Stack};
use chunkwake_protocol::play::{self, Login, Move, PlayerEntry, game_event};
use chunkwake_protocol::text;
use chunkwake_world::nbt::{Compound, Tag};
use chunkwake_world::{BlockEntity, ChunkColumn};
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
    /// Есть ли у плагинов слушатели события.
    fn listens(&self, kind: Event) -> bool;
    /// Событие с ответом (формат — в [`Event`]); `None` — слушателей нет.
    fn event(&self, kind: Event, uuid: Uuid, arg: String) -> Option<oneshot::Receiver<String>>;
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
    player_abilities: i32,
    player_info_remove: i32,
    system_chat: i32,
    sb_chat: i32,
    sb_command_suggestion: i32,
    command_suggestions: i32,
    sb_keep_alive: i32,
    sb_move_pos: i32,
    sb_move_pos_rot: i32,
    sb_move_rot: i32,
    add_entity: i32,
    teleport_entity: i32,
    rotate_head: i32,
    remove_entities: i32,
    set_entity_data: i32,
    take_item_entity: i32,
    set_entity_motion: i32,
    set_held_slot: i32,
    move_entity_pos_rot: i32,
    block_update: i32,
    block_destruction: i32,
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
    sb_container_close: i32,
    sb_player_input: i32,
    open_screen: i32,
    container_close: i32,
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
    parsers: text::CommandParsers,
    survival: SurvivalData,
    hooks: std::sync::OnceLock<Arc<dyn PluginHooks>>,
    /// Команды плагинов для дерева `commands` у клиента.
    plugin_commands: Mutex<Vec<String>>,
    /// Предметы на земле по ID сущности.
    dropped: Mutex<HashMap<i32, Dropped>>,
    players_source: std::sync::OnceLock<Arc<dyn PlayerSource>>,
    /// Несохранённые снимки игроков.
    player_saves: Mutex<HashMap<Uuid, PlayerSave>>,
    ops: std::sync::OnceLock<OpList>,
    /// Имя предмета по ID (`minecraft:bread`).
    item_names: Vec<String>,
    /// Зеркало онлайн-игроков для плагинов и каналы управления их сессиями.
    live: Mutex<HashMap<Uuid, Live>>,
    controls: Mutex<HashMap<Uuid, tokio::sync::mpsc::UnboundedSender<Control>>>,
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

/// Сохранённые игроки (`playerdata`): чтение при входе, запись при выходе.
pub trait PlayerSource: Send + Sync {
    fn load_player(&self, uuid: Uuid) -> Option<PlayerSave>;
    fn save_player(&self, uuid: Uuid, save: &PlayerSave);
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

/// Снимок игрока для сохранения — раз в 5 с.
const SNAPSHOT_TICKS: u32 = 100;

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
    pub parsers: text::CommandParsers,
    pub survival: SurvivalData,
}

/// Данные для выживания.
/// Уровень прав встроенной команды.
fn builtin_level(name: &str) -> i32 {
    match name {
        "op" | "deop" => ops::LEVEL_ADMINS,
        _ => ops::LEVEL_GAMEMASTERS,
    }
}

fn permission_event(id: i32, entity: i32, level: i32) -> Vec<u8> {
    inv::entity_event(
        id,
        entity,
        inv::ENTITY_EVENT_PERMISSION_LEVEL_0 + level.clamp(0, 4) as u8,
    )
}

/// Открытое окно с блоком мира (сундук, верстак): его слоты без инвентаря игрока.
struct Window {
    id: i32,
    kind: Kind,
    pos: [i32; 3],
    /// Вторая половина двойного сундука.
    second: Option<[i32; 3]>,
    slots: Vec<Slot>,
}

impl Window {
    fn positions(&self) -> Vec<[i32; 3]> {
        std::iter::once(self.pos).chain(self.second).collect()
    }
}

/// Свойства сундука: ловушка, `facing` (N0 E1 S2 W3), `type`, `waterlogged`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChestState {
    pub trapped: bool,
    pub facing: u8,
    pub kind: u8,
    pub waterlogged: bool,
}

pub const CHEST_SINGLE: u8 = 0;
pub const CHEST_LEFT: u8 = 1;
pub const CHEST_RIGHT: u8 = 2;

impl ChestState {
    /// `ChestBlock.getConnectedDirection`.
    fn connected(self) -> u8 {
        if self.kind == CHEST_LEFT { (self.facing + 1) % 4 } else { (self.facing + 3) % 4 }
    }
}

/// Сосед по горизонтальному направлению N0 E1 S2 W3.
fn offset([x, y, z]: [i32; 3], dir: u8) -> [i32; 3] {
    match dir {
        0 => [x, y, z - 1],
        1 => [x + 1, y, z],
        2 => [x, y, z + 1],
        _ => [x - 1, y, z],
    }
}

/// Бит шифта в `ServerboundPlayerInputPacket`.
const INPUT_SHIFT: u8 = 0x20;

/// Блок-сущность контейнера по имени блока.
fn container_entity(block: &str) -> Option<&'static str> {
    match block {
        "minecraft:chest" => Some("minecraft:chest"),
        "minecraft:trapped_chest" => Some("minecraft:trapped_chest"),
        "minecraft:barrel" => Some("minecraft:barrel"),
        _ => None,
    }
}

/// `Direction` по номеру (как `BlockFace` Bukkit).
const FACES: [&str; 6] = ["DOWN", "UP", "NORTH", "SOUTH", "WEST", "EAST"];

/// Высота глаз стоящего игрока (`Player.getStandingEyeHeight`).
const EYE_HEIGHT: f64 = 1.62;

/// Режимы игры по номерам (`GameType`).
const GAME_MODES: [&str; 4] = ["survival", "creative", "adventure", "spectator"];
/// Предел `/give` за раз.
const MAX_GIVE: i32 = 6400;

pub struct SurvivalData {
    /// Твёрдость по состоянию блока (-1 — неразрушимый, 0 — ломается сразу).
    pub hardness: Vec<f32>,
    /// Рецепты верстака и сетки 2×2.
    pub recipes: chunkwake_data::recipes::Recipes,
    /// Типы меню (`menu`): сундук 9×3, двойной 9×6 и верстак.
    pub menu_chest: i32,
    pub menu_double_chest: i32,
    /// Состояния сундуков (`ChestBlock`): свойства по состоянию и обратно.
    pub chests: HashMap<u32, ChestState>,
    pub chest_states: HashMap<ChestState, u32>,
    pub menu_crafting: i32,
    /// Нужен ли правильный инструмент для дропа, по состоянию блока.
    pub requires_tool: Vec<bool>,
    /// Компонент `minecraft:tool` по ID предмета.
    pub tools: Vec<Option<chunkwake_data::tools::Tool>>,
    /// Имя блока по состоянию (для правил инструментов).
    pub block_names: Vec<String>,
    /// Состояние блока по умолчанию по имени (`Block.setType` у плагинов).
    pub block_defaults: HashMap<String, u32>,
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
    /// ID `minecraft:item` в реестре `entity_type`.
    pub item_entity_type: i32,
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
            parsers,
            survival,
        } = world;
        let cb = |name| packets.id(State::Play, Direction::Clientbound, name);
        let sb = |name| packets.id(State::Play, Direction::Serverbound, name);
        let mut item_names = vec![
            String::new();
            survival
                .item_ids
                .values()
                .max()
                .map_or(0, |&m| m as usize + 1)
        ];
        for (name, &id) in &survival.item_ids {
            item_names[id as usize] = name.clone();
        }
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
                player_abilities: cb("player_abilities")?,
                player_info_remove: cb("player_info_remove")?,
                system_chat: cb("system_chat")?,
                sb_chat: sb("chat")?,
                sb_command_suggestion: sb("command_suggestion")?,
                command_suggestions: cb("command_suggestions")?,
                sb_keep_alive: sb("keep_alive")?,
                sb_move_pos: sb("move_player_pos")?,
                sb_move_pos_rot: sb("move_player_pos_rot")?,
                sb_move_rot: sb("move_player_rot")?,
                add_entity: cb("add_entity")?,
                teleport_entity: cb("teleport_entity")?,
                rotate_head: cb("rotate_head")?,
                remove_entities: cb("remove_entities")?,
                set_entity_data: cb("set_entity_data")?,
                take_item_entity: cb("take_item_entity")?,
                set_entity_motion: cb("set_entity_motion")?,
                set_held_slot: cb("set_held_slot")?,
                move_entity_pos_rot: cb("move_entity_pos_rot")?,
                block_update: cb("block_update")?,
                block_destruction: cb("block_destruction")?,
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
                sb_container_close: sb("container_close")?,
                sb_player_input: sb("player_input")?,
                open_screen: cb("open_screen")?,
                container_close: cb("container_close")?,
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
            parsers,
            survival,
            hooks: std::sync::OnceLock::new(),
            plugin_commands: Mutex::new(Vec::new()),
            dropped: Mutex::new(HashMap::new()),
            players_source: std::sync::OnceLock::new(),
            player_saves: Mutex::new(HashMap::new()),
            ops: std::sync::OnceLock::new(),
            item_names,
            live: Mutex::new(HashMap::new()),
            controls: Mutex::new(HashMap::new()),
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
        let online: Vec<Uuid> = self.hub.players.lock().unwrap().keys().copied().collect();
        for uuid in online {
            self.send_to(uuid, self.commands_packet(self.op_level(uuid)));
        }
    }

    pub fn set_ops(&self, ops: OpList) {
        let _ = self.ops.set(ops);
    }

    /// Уровень прав игрока (0 — не оператор).
    pub fn op_level(&self, uuid: Uuid) -> i32 {
        self.ops.get().map_or(0, |o| o.level(uuid))
    }

    /// Пакет только одному игроку.
    fn send_to(&self, to: Uuid, packet: Vec<u8>) {
        let _ = self.hub.tx.send(Broadcast {
            skip: None,
            only: Some(to),
            packet: Arc::new(self.hub.frame(&packet)),
            entity: None,
            moves: None,
            blocks: None,
        });
    }

    /// Права игрока сменились: клиенту — уровень и дерево команд по нему.
    fn permissions_changed(&self, uuid: Uuid) {
        let level = self.op_level(uuid);
        let entity = self
            .hub
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| p.entity_id);
        if let Some(entity) = entity {
            self.send_to(uuid, permission_event(self.ids.entity_event, entity, level));
            self.send_to(uuid, self.commands_packet(level));
        }
    }

    /// `Player.setOp` из плагина: онлайн-игрок по UUID.
    pub fn set_op(&self, uuid: Uuid, op: bool) {
        let Some(list) = self.ops.get() else { return };
        let name = self
            .hub
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| p.entry.name.clone());
        let changed = match (op, name) {
            (true, Some(name)) => list.op(uuid, &name),
            (false, _) => list.deop(uuid),
            _ => false,
        };
        if changed {
            self.permissions_changed(uuid);
        }
    }

    /// Предметы для плагинов: строки `minecraft:stone 64`.
    pub fn item_list(&self) -> String {
        let mut out = String::new();
        for (id, name) in self
            .item_names
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.is_empty())
        {
            let max = self.survival.items.get(id).map_or(64, |i| i.max_stack);
            out += &format!(
                "{name} {max}
"
            );
        }
        out
    }

    fn item_name(&self, id: i32) -> &str {
        self.item_names
            .get(id as usize)
            .filter(|n| !n.is_empty())
            .map_or("minecraft:air", String::as_str)
    }

    fn item_id(&self, name: &str) -> Option<i32> {
        self.survival.item_ids.get(name).copied()
    }

    /// Снимок игрока для плагинов (формат — [`Live::render`]); `None` — не онлайн.
    pub fn player_state(&self, uuid: Uuid) -> Option<String> {
        let live = self.live.lock().unwrap().get(&uuid)?.clone();
        Some(live.render(|id| Some(self.item_name(id).to_owned())))
    }

    /// Команда плагина сессии игрока (формат — [`Control::parse`]).
    pub fn player_control(&self, uuid: Uuid, line: &str) -> bool {
        let Some(c) = Control::parse(line, |n| self.item_id(n)) else {
            return false;
        };
        if let Some(l) = self.live.lock().unwrap().get_mut(&uuid) {
            c.apply(l);
        }
        self.controls
            .lock()
            .unwrap()
            .get(&uuid)
            .is_some_and(|tx| tx.send(c).is_ok())
    }

    /// Запрос плагина к миру: `get x y z` → имя блока, `set x y z имя` → `1`/`0`.
    pub fn world_request(&self, line: &str) -> String {
        let mut it = line.split_whitespace();
        let cmd = it.next();
        let mut n = || it.next().and_then(|s| s.parse::<i32>().ok());
        let (Some(x), Some(y), Some(z)) = (n(), n(), n()) else {
            return String::new();
        };
        match cmd {
            Some("get") => self
                .block([x, y, z])
                .map_or(String::new(), |s| self.state_name(s).to_owned()),
            Some("set") => {
                let state = it.next().and_then(|name| {
                    let name = if name.contains(':') { name.to_owned() } else { format!("minecraft:{name}") };
                    self.survival.block_defaults.get(&name).copied()
                });
                match state {
                    Some(s) if self.set_block([x, y, z], s).is_some() => "1".into(),
                    _ => "0".into(),
                }
            }
            _ => String::new(),
        }
    }

    fn publish_live(&self, uuid: Uuid, st: &PlayerState, game_mode: u8) {
        let (pos, yaw, pitch) = self.player_at(uuid).unwrap_or(([0.0; 3], 0.0, 0.0));
        let v = &st.vitals;
        let live = Live {
            health: v.health,
            food: v.food,
            saturation: v.saturation,
            exhaustion: v.exhaustion,
            game_mode,
            selected: st.inv.selected,
            slots: st.inv.slots,
            pos,
            yaw,
            pitch,
        };
        self.live.lock().unwrap().insert(uuid, live);
    }

    /// Событие плагинам с ожиданием ответа; `None` — слушателей нет или таймаут.
    /// Аргумент строится, только если слушатели есть.
    async fn plugin_event(
        &self,
        kind: Event,
        uuid: Uuid,
        arg: impl FnOnce() -> String,
    ) -> Option<String> {
        let h = self.hooks().filter(|h| h.listens(kind))?;
        let rx = h.event(kind, uuid, arg())?;
        tokio::time::timeout(PLUGIN_TIMEOUT, rx).await.ok()?.ok()
    }

    /// Отменено ли событие (`c` в ответе).
    async fn cancelled(&self, kind: Event, uuid: Uuid, arg: impl FnOnce() -> String) -> bool {
        self.plugin_event(kind, uuid, arg)
            .await
            .is_some_and(|r| r.trim() == "c")
    }

    /// Онлайн-игрок по имени (без учёта регистра).
    fn online_by_name(&self, name: &str) -> Option<(Uuid, String)> {
        self.hub
            .players
            .lock()
            .unwrap()
            .values()
            .find(|p| p.entry.name.eq_ignore_ascii_case(name))
            .map(|p| (p.entry.uuid, p.entry.name.clone()))
    }

    /// `/op` и `/deop` (`OpCommand`, `DeOpCommand`): сообщение как у ванилы.
    fn op_command(&self, name: &str, grant: bool) -> serde_json::Value {
        let Some(list) = self.ops.get() else {
            return serde_json::json!({"text": "Operators are not available", "color": "red"});
        };
        let target = self
            .online_by_name(name)
            .or_else(|| list.find_name(name).map(|e| (e.uuid, e.name)));
        let Some((uuid, name)) = target else {
            return serde_json::json!({"translate": "argument.player.unknown", "color": "red"});
        };
        let changed = if grant {
            list.op(uuid, &name)
        } else {
            list.deop(uuid)
        };
        if !changed {
            let key = if grant {
                "commands.op.failed"
            } else {
                "commands.deop.failed"
            };
            return serde_json::json!({"translate": key, "color": "red"});
        }
        tracing::info!("{} {name}", if grant { "opped" } else { "de-opped" });
        self.permissions_changed(uuid);
        let key = if grant {
            "commands.op.success"
        } else {
            "commands.deop.success"
        };
        serde_json::json!({"translate": key, "with": [name]})
    }

    /// Команда из консоли сервера: встроенные (`op`, `deop`, `list`), иначе —
    /// плагинам от имени консоли. Возвращает текст ответа для лога.
    pub async fn console_command(&self, line: &str) -> String {
        let mut parts = line.split_whitespace();
        let plain = |v: serde_json::Value| text::plain_text(&v);
        match (parts.next(), parts.next()) {
            (Some("op"), Some(name)) => plain(self.op_command(name, true)),
            (Some("deop"), Some(name)) => plain(self.op_command(name, false)),
            (Some("list"), None) => {
                let names: Vec<String> = self
                    .hub
                    .players
                    .lock()
                    .unwrap()
                    .values()
                    .map(|p| p.entry.name.clone())
                    .collect();
                format!(
                    "There are {} players online: {}",
                    names.len(),
                    names.join(", ")
                )
            }
            _ => {
                let known = match self.hooks() {
                    Some(h) => {
                        tokio::time::timeout(Duration::from_secs(5), h.command(Uuid::nil(), line))
                            .await
                            .is_ok_and(|r| r.unwrap_or(false))
                    }
                    None => false,
                };
                if known {
                    String::new()
                } else {
                    format!("Unknown command: {line}")
                }
            }
        }
    }

    /// Дерево команд для клиента по уровню прав: встроенные и команды плагинов.
    fn commands_packet(&self, level: i32) -> Vec<u8> {
        use text::{Node, Parser};
        let mut roots = vec![
            // give <предмет> [количество]
            Node::literal(
                "give",
                false,
                vec![Node::argument(
                    "item",
                    true,
                    Parser::ItemStack,
                    vec![Node::argument(
                        "count",
                        true,
                        Parser::Integer(1, MAX_GIVE),
                        vec![],
                    )],
                )],
            ),
            // gamemode <режим>
            Node::literal(
                "gamemode",
                false,
                GAME_MODES
                    .iter()
                    .map(|&m| Node::literal(m, true, vec![]))
                    .collect(),
            ),
            Node::literal(
                "op",
                false,
                vec![Node::argument(
                    "targets",
                    true,
                    Parser::GreedyString,
                    vec![],
                )],
            ),
            Node::literal(
                "deop",
                false,
                vec![Node::argument(
                    "targets",
                    true,
                    Parser::GreedyString,
                    vec![],
                )],
            ),
        ];
        // Встроенные — только с нужным уровнем, как ванила прячет их из дерева.
        roots.retain(|n| level >= builtin_level(n.name()));
        roots.extend(self.plugin_commands.lock().unwrap().iter().map(|n| {
            Node::literal(
                n,
                true,
                vec![Node::argument("args", true, Parser::GreedyStringAsk, vec![])],
            )
        }));
        text::commands_tree(self.ids.commands, &roots, self.parsers)
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

    pub fn set_player_source(&self, source: Arc<dyn PlayerSource>) {
        let _ = self.players_source.set(source);
    }

    /// Снимки игроков с прошлого сохранения (забирает их).
    pub fn take_player_saves(&self) -> Vec<(Uuid, PlayerSave)> {
        self.player_saves.lock().unwrap().drain().collect()
    }

    /// Запомнить состояние игрока для следующего сохранения.
    fn snapshot(&self, uuid: Uuid, st: &PlayerState) {
        let Some((pos, yaw, pitch)) = self.player_at(uuid) else {
            return;
        };
        self.player_saves
            .lock()
            .unwrap()
            .insert(uuid, st.save(pos, yaw, pitch));
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
        // Сохранённый игрок: позиция, инвентарь, здоровье. Погибший — заново на спавне.
        // Свежий снимок в памяти (вышел и зашёл до автосохранения) важнее файла.
        let pending = self.player_saves.lock().unwrap().get(&player.uuid).cloned();
        let save = pending
            .or_else(|| {
                self.players_source
                    .get()
                    .and_then(|s| s.load_player(player.uuid))
            })
            .filter(|s| s.health > 0.0 && s.pos.iter().all(|c| c.is_finite()));
        let (pos, yaw, pitch) = save
            .as_ref()
            .map_or((self.spawn, 0.0, 0.0), |s| (s.pos, s.yaw, s.pitch));
        let mut state = save.as_ref().map(PlayerState::restore).unwrap_or_default();
        let game_mode = *state.game_mode.get_or_insert(self.login.spawn.game_mode);
        let me = Online {
            entry: PlayerEntry {
                uuid: player.uuid,
                name: player.name.clone(),
                properties: player.properties.clone(),
                game_mode: game_mode as i32,
                latency_ms: 0,
            },
            entity_id: self.next_entity_id.fetch_add(1, Ordering::Relaxed),
            pos,
            yaw,
            pitch,
            base: play::quantize(pos),
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
        self.publish_live(uuid, &state, game_mode);
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

        let (control_tx, controls) = tokio::sync::mpsc::unbounded_channel();
        self.controls.lock().unwrap().insert(uuid, control_tx);
        let result = self
            .session(conn, rx, controls, &me, &others, &mut state)
            .await;
        self.controls.lock().unwrap().remove(&uuid);
        self.live.lock().unwrap().remove(&uuid);
        // Вышел — сразу на диск (не ждём автосохранения).
        self.snapshot(uuid, &state);
        let quit = self.player_saves.lock().unwrap().remove(&uuid);
        if let (Some(save), Some(source)) = (quit, self.players_source.get()) {
            let source = source.clone();
            let _ = tokio::task::spawn_blocking(move || source.save_player(uuid, &save)).await;
        }

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

    /// Бросает предмет: клиентам — спавн со скоростью, дальше физику ведёт
    /// `tick_items`.
    fn drop_item(&self, from: [f64; 3], vel: [f64; 3], stack: Stack, delay: Duration) {
        let entity_id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        self.dropped
            .lock()
            .unwrap()
            .insert(entity_id, Dropped::new(from, vel, stack, delay));
        self.hub.send(
            None,
            play::add_entity_moving(
                self.ids.add_entity,
                entity_id,
                Uuid::from_u128(rand::random()),
                self.survival.item_entity_type,
                from,
                vel,
                0.0,
                0.0,
                0,
            ),
        );
        self.hub.send(
            None,
            inv::set_entity_item(self.ids.set_entity_data, entity_id, Some(stack)),
        );
    }

    /// Игрок бросает предмет вперёд (`Player.drop`).
    fn throw(&self, feet: [f64; 3], yaw: f32, pitch: f32, stack: Stack) {
        self.drop_item(
            items::throw_origin(feet),
            items::throw_velocity(yaw, pitch, rand::random()),
            stack,
            items::THROW_DELAY,
        );
    }

    /// Кадры спавна всех предметов на земле (для входящего игрока).
    fn dropped_frames(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (&id, d) in self.dropped.lock().unwrap().iter() {
            out.extend(self.hub.frame(&play::add_entity_moving(
                self.ids.add_entity,
                id,
                Uuid::from_u128(id as u128),
                self.survival.item_entity_type,
                d.pos,
                d.vel,
                0.0,
                0.0,
                0,
            )));
            out.extend(self.hub.frame(&inv::set_entity_item(
                self.ids.set_entity_data,
                id,
                Some(d.stack),
            )));
        }
        out
    }

    /// Подбор предметов у ног игрока в инвентарь. Возвращает изменённые слоты.
    /// Предметы, которые игрок подберёт в этот тик (для событий плагинов).
    fn pickup_candidates(&self, feet: [f64; 3]) -> Vec<(i32, Stack)> {
        let now = Instant::now();
        self.dropped
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, d)| now >= d.pickup_at && items::in_reach(feet, d.pos))
            .map(|(&id, d)| (id, d.stack))
            .collect()
    }

    fn pickup(
        &self,
        collector: i32,
        feet: [f64; 3],
        inv: &mut Inventory,
        vetoed: &HashSet<i32>,
    ) -> Vec<usize> {
        let mut changed = Vec::new();
        let mut dropped = self.dropped.lock().unwrap();
        if dropped.is_empty() {
            return changed;
        }
        let now = Instant::now();
        let mut taken = Vec::new();
        for (&id, d) in dropped.iter_mut() {
            if now < d.pickup_at || !items::in_reach(feet, d.pos) || vetoed.contains(&id) {
                continue;
            }
            let max = self
                .survival
                .items
                .get(d.stack.item as usize)
                .map_or(64, |i| i.max_stack);
            let (slots, left) = inv.add(d.stack.item, d.stack.count, max as i32);
            let got = d.stack.count - left;
            if got == 0 {
                continue;
            }
            changed.extend(slots);
            self.hub.send(
                None,
                inv::take_item_entity(self.ids.take_item_entity, id, collector, got),
            );
            if left == 0 {
                taken.push(id);
            } else {
                d.stack.count = left;
                self.hub.send(
                    None,
                    inv::set_entity_item(self.ids.set_entity_data, id, Some(d.stack)),
                );
            }
        }
        for id in &taken {
            dropped.remove(id);
        }
        if !taken.is_empty() {
            self.hub.send(
                None,
                play::remove_entities(self.ids.remove_entities, &taken),
            );
        }
        changed
    }

    /// Тик предметов (`ItemEntity.tick`): физика, слияние стопок, исчезновение.
    /// Клиентам — позиция и скорость при толчках и раз в секунду в полёте.
    fn tick_items(&self) {
        let mut dropped = self.dropped.lock().unwrap();
        if dropped.is_empty() {
            return;
        }
        let now = Instant::now();
        let min_y = self.min_y;
        let solid = |p: [i32; 3]| p[1] >= min_y && self.block(p).is_none_or(|b| b != AIR);
        let mut syncs = Vec::new();
        let mut gone = Vec::new();
        // Кого проверять на слияние: летящих раз в 2 тика, лежащих раз в 40 (как ванила).
        let mut merging = Vec::new();
        for (&id, d) in dropped.iter_mut() {
            if d.resting() {
                d.age += 1;
                if d.age.is_multiple_of(40) {
                    merging.push(id);
                }
                // Раз в 4 тика: не убрали ли опору.
                let below = [d.pos[0], d.pos[1] - 0.01, d.pos[2]].map(|c| c.floor() as i32);
                if !d.age.is_multiple_of(4) || solid(below) {
                    if now >= d.despawn_at {
                        gone.push(id);
                    }
                    continue;
                }
                d.on_ground = false;
            }
            let hit = items::step(d, solid);
            if hit || d.age.is_multiple_of(items::SYNC_TICKS) {
                syncs.push((id, d.pos, d.vel, d.on_ground));
            }
            if d.pos[1] < min_y as f64 - items::VOID_DEPTH || now >= d.despawn_at {
                gone.push(id);
            } else if d.age.is_multiple_of(2) {
                merging.push(id);
            }
        }
        for id in &gone {
            dropped.remove(id);
        }
        // Слияние: меньшая стопка уходит в большую, большая остаётся на месте.
        let mut updated = Vec::new();
        for a in merging {
            let Some(&da) = dropped.get(&a) else { continue };
            let max = self
                .survival
                .items
                .get(da.stack.item as usize)
                .map_or(64, |i| i.max_stack) as i32;
            let other = dropped
                .iter()
                .find(|&(&b, db)| b != a && items::can_merge(&da, db, max))
                .map(|(&b, &db)| (b, db));
            let Some((b, db)) = other else { continue };
            let (keep, absorbed) = if da.stack.count >= db.stack.count {
                (a, b)
            } else {
                (b, a)
            };
            let add = dropped[&absorbed];
            let d = dropped.get_mut(&keep).unwrap();
            d.stack.count += add.stack.count;
            d.pickup_at = d.pickup_at.max(add.pickup_at);
            d.despawn_at = d.despawn_at.max(add.despawn_at);
            updated.push((keep, d.stack));
            dropped.remove(&absorbed);
            gone.push(absorbed);
        }
        drop(dropped);
        for (id, pos, vel, on_ground) in syncs {
            if gone.contains(&id) {
                continue;
            }
            self.hub.send(
                None,
                play::teleport_entity(self.ids.teleport_entity, id, pos, 0.0, 0.0, on_ground),
            );
            self.hub.send(
                None,
                play::set_entity_motion(self.ids.set_entity_motion, id, vel),
            );
        }
        for (id, stack) in updated {
            if !gone.contains(&id) {
                self.hub.send(
                    None,
                    inv::set_entity_item(self.ids.set_entity_data, id, Some(stack)),
                );
            }
        }
        if !gone.is_empty() {
            self.hub
                .send(None, play::remove_entities(self.ids.remove_entities, &gone));
        }
    }

    /// Позиция ног и поворот игрока.
    fn player_at(&self, uuid: Uuid) -> Option<([f64; 3], f32, f32)> {
        self.hub
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| (p.pos, p.yaw, p.pitch))
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
                plan.tick_items();
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
        mut controls: tokio::sync::mpsc::UnboundedReceiver<Control>,
        me: &Online,
        others: &[EntityRef],
        state: &mut PlayerState,
    ) -> Result<(), NetError> {
        let ids = self.ids;
        let uuid = me.entry.uuid;
        let mut login = self.login.clone();
        login.entity_id = me.entity_id;
        login.spawn.game_mode = state.game_mode.unwrap_or(login.spawn.game_mode);
        conn.send(&play::login(ids.login, &login)).await?;
        conn.send(&play::player_abilities(
            ids.player_abilities,
            login.spawn.game_mode,
        ))
        .await?;
        let level = self.op_level(uuid);
        conn.send(&self.commands_packet(level)).await?;
        conn.send(&permission_event(ids.entity_event, me.entity_id, level))
            .await?;
        conn.send(&play::game_event(
            ids.game_event,
            game_event::LEVEL_CHUNKS_LOAD_START,
            0.0,
        ))
        .await?;

        let (cx, cz) = (
            (me.pos[0].floor() as i32) >> 4,
            (me.pos[2].floor() as i32) >> 4,
        );
        let r_view = login.view_distance;
        self.send_chunks(conn, (cx, cz), &chunks_around((cx, cz), r_view, None), &[])
            .await?;
        let mut center = (cx, cz);
        conn.send(&play::player_position(
            ids.player_position,
            1,
            me.pos,
            me.yaw,
            me.pitch,
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
        out.extend(self.dropped_frames());
        if !out.is_empty() {
            conn.send_frames(&out).await?;
        }

        // Keep-alive: шлём раз в 15 с, ответ ждём не дольше 30 с.
        let mut ticker = tokio::time::interval(KEEP_ALIVE_INTERVAL);
        ticker.tick().await;
        // Выживание: инвентарь, здоровье. Креатив (1) и наблюдатель (3) урона не получают.
        let mut survival = matches!(login.spawn.game_mode, 0 | 2);
        state.vitals.reset_fall(me.pos);
        let mut saved_ticks: u32 = 0;
        let mut last_void: Option<Instant> = None;
        let mut teleport_id = 1;
        // Поедание: слот хотбара, еда и момент окончания.
        let mut eating: Option<(usize, chunkwake_data::world::Food, Instant)> = None;
        // Открытое окно (сундук, верстак) и счётчик номеров окон.
        let mut window: Option<Window> = None;
        let mut next_window = 0;
        // Шифт из `player_input` (`isSecondaryUseActive`).
        let mut sneaking = false;
        let mut food_ticker = tokio::time::interval(TICK);
        food_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        self.send_inventory(conn, &mut state.inv).await?;
        // Выбранная ячейка из сохранения: иначе клиент держит 0-ю, а сервер другую.
        conn.send(&inv::set_held_slot(
            ids.set_held_slot,
            state.inv.selected as i32,
        ))
        .await?;
        self.send_health(conn, &state.vitals).await?;
        let mut pending: Option<(i64, Instant)> = None;
        // Ломка (`ServerPlayerGameMode`): тики сессии, текущий блок и отложенная
        // ломка (клиент закончил раньше, чем позволяет прогресс).
        let mut game_ticks: u64 = 0;
        let mut digging: Option<([i32; 3], u64)> = None;
        let mut delayed: Option<([i32; 3], u64)> = None;
        // Трещины, которые видят другие игроки (`destroyBlockProgress`).
        let mut crack: Option<([i32; 3], i8)> = None;
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
                Some(c) = controls.recv() => {
                    match c {
                        Control::Health(h) => {
                            if !state.vitals.dead {
                                state.vitals.health = h.min(survival::MAX_HEALTH);
                                self.send_health(conn, &state.vitals).await?;
                                if h <= 0.0 && state.vitals.damage(0.0) {
                                    self.die(conn, me, &mut state.inv, Cause::Generic).await?;
                                }
                            }
                        }
                        Control::Food(f) => {
                            state.vitals.food = f;
                            self.send_health(conn, &state.vitals).await?;
                        }
                        Control::Saturation(s) => {
                            state.vitals.saturation = s;
                            self.send_health(conn, &state.vitals).await?;
                        }
                        Control::Exhaustion(e) => state.vitals.exhaustion = e,
                        Control::GameMode(mode) => {
                            login.spawn.game_mode = mode;
                            state.game_mode = Some(mode);
                            survival = matches!(mode, 0 | 2);
                            if let Some((feet, _, _)) = self.player_at(uuid) {
                                state.vitals.reset_fall(feet);
                            }
                            self.set_game_mode(conn, uuid, mode, false).await?;
                            self.send_health(conn, &state.vitals).await?;
                        }
                        Control::Slot(w, s) => {
                            state.inv.slots[w] = s;
                            self.send_slots(conn, &mut state.inv, &[w]).await?;
                        }
                        Control::Selected(s) => {
                            state.inv.selected = s;
                            eating = None;
                            conn.send(&inv::set_held_slot(ids.set_held_slot, s as i32)).await?;
                        }
                        Control::ContainerChanged(pos) => {
                            if let Some(w) = window.as_mut().filter(|w| w.positions().contains(&pos)) {
                                w.slots = self.window_slots(w);
                                self.send_window(conn, &mut state.inv, window.as_ref(), window.as_ref().map_or(0, |w| w.id)).await?;
                            }
                        }
                        Control::Teleport(pos, yaw, pitch) => {
                            teleport_id += 1;
                            center = self.teleport(conn, uuid, pos, yaw, pitch, teleport_id, center, r_view).await?;
                            state.vitals.reset_fall(pos);
                            out.clear();
                            self.refresh_visible(uuid, center, r_view, &mut visible, &mut out);
                            if !out.is_empty() {
                                conn.send_frames(&out).await?;
                            }
                        }
                    }
                    self.publish_live(uuid, state, login.spawn.game_mode);
                }
                _ = food_ticker.tick() => {
                    game_ticks += 1;
                    let now = digging.or(delayed).and_then(|(pos, start)| {
                        let block = self.block(pos).unwrap_or(AIR);
                        let p = self.destroy_progress(uuid, block, state.inv.held(), &state.vitals);
                        (block != AIR).then(|| (pos, ((p * (game_ticks - start) as f32) * 10.0).min(9.0) as i8))
                    });
                    if now != crack {
                        let packet = match (now, crack) {
                            (Some((pos, stage)), _) => play::block_destruction(ids.block_destruction, me.entity_id, pos, stage),
                            (None, Some((pos, _))) => play::block_destruction(ids.block_destruction, me.entity_id, pos, -1),
                            (None, None) => unreachable!(),
                        };
                        self.hub.send(Some(uuid), packet);
                        crack = now;
                    }
                    if let Some((pos, start)) = delayed {
                        let block = self.block(pos).unwrap_or(AIR);
                        if block == AIR {
                            delayed = None;
                        } else {
                            let p = self.destroy_progress(uuid, block, state.inv.held(), &state.vitals);
                            if p * (game_ticks - start + 1) as f32 >= 1.0 {
                                delayed = None;
                                self.mine(conn, uuid, pos, block, state).await?;
                            }
                        }
                    }
                    if self.hooks().is_some() {
                        self.publish_live(uuid, state, login.spawn.game_mode);
                    }
                    saved_ticks += 1;
                    if saved_ticks >= SNAPSHOT_TICKS {
                        saved_ticks = 0;
                        self.snapshot(uuid, state);
                    }
                    if !state.vitals.dead && login.spawn.game_mode != 3
                        && let Some((feet, _, _)) = self.player_at(uuid)
                    {
                        // Отменённые плагинами подборы (`EntityPickupItemEvent`).
                        let mut vetoed = HashSet::new();
                        if self.hooks().is_some_and(|h| h.listens(Event::Pickup)) {
                            for (id, s) in self.pickup_candidates(feet) {
                                let name = self.item_name(s.item);
                                if self.cancelled(Event::Pickup, uuid, || format!("{name} {} 0", s.count)).await {
                                    vetoed.insert(id);
                                }
                            }
                        }
                        let changed = self.pickup(me.entity_id, feet, &mut state.inv, &vetoed);
                        if !changed.is_empty() {
                            self.send_slots(conn, &mut state.inv, &changed).await?;
                        }
                    }
                    if !survival {
                        continue;
                    }
                    if let Some((slot, food, done)) = eating
                        && Instant::now() >= done
                    {
                        eating = None;
                        // Предмет мог смениться (клики): едим, только если еда на месте.
                        let same = state.inv.slots[slot].and_then(|s| self.survival.items.get(s.item as usize))
                            .and_then(|i| i.food) == Some(food);
                        if same && slot == state.inv.held_slot() && state.inv.consume_held() {
                            state.vitals.eat(food.nutrition, food.saturation);
                            self.send_slots(conn, &mut state.inv, &[slot]).await?;
                            self.send_health(conn, &state.vitals).await?;
                            conn.send(&inv::entity_event(ids.entity_event, me.entity_id, inv::ENTITY_EVENT_USE_ITEM_COMPLETE)).await?;
                        }
                    }
                    let t = state.vitals.tick(self.survival.difficulty);
                    if let Some(d) = t.starve {
                        self.hurt(conn, me, &mut state.vitals, &mut state.inv, d, Cause::Starve).await?;
                    } else if t.changed {
                        self.send_health(conn, &state.vitals).await?;
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
                        if state.vitals.dead {
                            continue;
                        }
                        // `PlayerMoveEvent` (как Paper: сдвиг > 1/16 блока или поворот > 10°).
                        if let Some((from, fyaw, fpitch)) = self.player_at(uuid)
                            && self.hooks().is_some_and(|h| h.listens(Event::Move))
                        {
                            let to = m.pos.unwrap_or(from);
                            let (tyaw, tpitch) = m.rot.unwrap_or((fyaw, fpitch));
                            let d2: f64 = (0..3).map(|i| (to[i] - from[i]).powi(2)).sum();
                            if d2 > 1.0 / 256.0 || (tyaw - fyaw).abs() > 10.0 || (tpitch - fpitch).abs() > 10.0 {
                                let reply = self.plugin_event(Event::Move, uuid, || {
                                    format!("{} {} {} {fyaw} {fpitch} {} {} {} {tyaw} {tpitch}",
                                        from[0], from[1], from[2], to[0], to[1], to[2])
                                }).await.unwrap_or_default();
                                let target = match reply.trim() {
                                    "c" => Some((from, fyaw, fpitch)),
                                    r => r.strip_prefix("t ").and_then(|t| {
                                        let v: Vec<f64> = t.split(' ').filter_map(|s| s.parse().ok()).collect();
                                        (v.len() == 5 && v.iter().all(|x| x.is_finite()))
                                            .then(|| ([v[0], v[1], v[2]], v[3] as f32, v[4] as f32))
                                    }),
                                };
                                if let Some((pos, yaw, pitch)) = target {
                                    teleport_id += 1;
                                    center = self.teleport(conn, uuid, pos, yaw, pitch, teleport_id, center, r_view).await?;
                                    state.vitals.reset_fall(pos);
                                    continue;
                                }
                            }
                        }
                        let y = m.pos.map(|p| p[1]);
                        if let Some(d) = state.vitals.on_move(m.pos, m.on_ground, survival) {
                            self.hurt(conn, me, &mut state.vitals, &mut state.inv, d, Cause::Fall).await?;
                        }
                        let below_void = y.is_some_and(|y| y < self.min_y as f64 - survival::VOID_DEPTH);
                        if survival && below_void
                            && last_void.is_none_or(|t| t.elapsed() >= TICK * survival::VOID_INTERVAL_TICKS as u32)
                        {
                            last_void = Some(Instant::now());
                            self.hurt(conn, me, &mut state.vitals, &mut state.inv, survival::VOID_DAMAGE, Cause::Void).await?;
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
                        state.inv.selected = (r.u16()? as usize).min(8);
                        eating = None;
                        continue;
                    }
                    if id == ids.sb_set_creative_mode_slot {
                        let (slot, stack) = inv::read_creative_slot(&mut r)?;
                        if !survival {
                            // Слот < 0 — выброс из креатив-меню (`handleSetCreativeModeSlot`).
                            if slot < 0 {
                                if let (Some(s), Some((feet, yaw, pitch))) = (stack, self.player_at(uuid)) {
                                    self.throw(feet, yaw, pitch, s);
                                }
                            } else {
                                state.inv.set(slot, stack);
                            }
                        }
                        continue;
                    }
                    if id == ids.sb_container_click {
                        let c = inv::read_container_click(&mut r)?;
                        // Сундук сломали, пока окно открыто: закрываем без клика.
                        if let Some(w) = window.as_ref().filter(|w| w.id == c.window && !self.window_valid(w)) {
                            conn.send(&inv::container_close(ids.container_close, w.id)).await?;
                            self.close_window(&mut state.inv, &mut window, uuid);
                            self.send_inventory(conn, &mut state.inv).await?;
                            continue;
                        }
                        {
                            let creative = login.spawn.game_mode == 1;
                            let (thrown, resync) = self.click(&mut state.inv, &mut window, &c, creative, uuid);
                            if resync {
                                self.send_window(conn, &mut state.inv, window.as_ref(), c.window).await?;
                            }
                            let mut kept = false;
                            if let Some((feet, yaw, pitch)) = self.player_at(uuid) {
                                for s in thrown {
                                    let name = self.item_name(s.item);
                                    if self.cancelled(Event::Drop, uuid, || format!("{name} {}", s.count)).await {
                                        if let Some(left) = state.inv.add_stack(s, self.max_stack(s.item)) {
                                            self.throw(feet, yaw, pitch, left);
                                        }
                                        kept = true;
                                    } else {
                                        self.throw(feet, yaw, pitch, s);
                                    }
                                }
                            }
                            if kept {
                                self.send_window(conn, &mut state.inv, window.as_ref(), c.window).await?;
                            }
                        }
                        continue;
                    }
                    if id == ids.sb_player_input {
                        sneaking = r.u8()? & INPUT_SHIFT != 0;
                        continue;
                    }
                    if id == ids.sb_container_close {
                        let _id = r.var_int()?;
                        self.close_window(&mut state.inv, &mut window, uuid);
                        self.send_inventory(conn, &mut state.inv).await?;
                        continue;
                    }
                    if id == ids.sb_use_item {
                        let hand = r.var_int()?;
                        let sequence = r.var_int()?;
                        let hand_name = if hand == 0 { "HAND" } else { "OFF_HAND" };
                        if !state.vitals.dead
                            && self.cancelled(Event::Interact, uuid, || format!("RIGHT_CLICK_AIR ~ ~ ~ SELF {hand_name}")).await
                        {
                            eating = None;
                            conn.send(&play::block_changed_ack(ids.block_changed_ack, sequence)).await?;
                            continue;
                        }
                        let food = state.inv.held()
                            .filter(|_| hand == 0 && survival && !state.vitals.dead)
                            .and_then(|s| self.survival.items.get(s.item as usize))
                            .and_then(|i| i.food)
                            .filter(|f| f.can_always_eat || state.vitals.food < survival::MAX_FOOD);
                        eating = food.map(|f| (state.inv.held_slot(), f, Instant::now() + Duration::from_secs_f32(f.seconds)));
                        conn.send(&play::block_changed_ack(ids.block_changed_ack, sequence)).await?;
                        continue;
                    }
                    if id == ids.sb_player_command {
                        let _entity = r.var_int()?;
                        match r.var_int()? {
                            play::START_SPRINTING => state.vitals.sprinting = true,
                            play::STOP_SPRINTING => state.vitals.sprinting = false,
                            _ => {}
                        }
                        continue;
                    }
                    if id == ids.sb_client_command {
                        if r.var_int()? == inv::PERFORM_RESPAWN && state.vitals.dead {
                            teleport_id += 1;
                            center = self.respawn(conn, me, &login, &mut state.vitals, &mut state.inv, center, teleport_id).await?;
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
                        if state.vitals.dead {
                            continue;
                        }
                        match a.action {
                            play::START_DESTROY_BLOCK | play::ABORT_DESTROY_BLOCK | play::STOP_DESTROY_BLOCK => {
                                let block = self.block(a.pos).unwrap_or(AIR);
                                let held = state.inv.held();
                                if a.action == play::START_DESTROY_BLOCK {
                                    let face = FACES.get(a.face as usize).copied().unwrap_or("UP");
                                    if self.cancelled(Event::Interact, uuid, || {
                                        format!("LEFT_CLICK_BLOCK {} {} {} {face} HAND", a.pos[0], a.pos[1], a.pos[2])
                                    }).await {
                                        conn.send(&play::block_update(ids.block_update, a.pos, block as i32)).await?;
                                        conn.send(&play::block_changed_ack(ids.block_changed_ack, a.sequence)).await?;
                                        continue;
                                    }
                                }
                                // `handleBlockBreakAction`: креатив ломает сразу, приключение
                                // и наблюдатель не ломают, выживание — по прогрессу.
                                let mode = login.spawn.game_mode;
                                let mut resync = false;
                                match a.action {
                                    play::START_DESTROY_BLOCK if mode == 1 => {
                                        self.break_block(conn, uuid, a.pos, false).await?;
                                    }
                                    _ if mode >= 2 => resync = a.action != play::ABORT_DESTROY_BLOCK,
                                    play::START_DESTROY_BLOCK => {
                                        let p = self.destroy_progress(uuid, block, held, &state.vitals);
                                        if block != AIR && p >= 1.0 {
                                            self.mine(conn, uuid, a.pos, block, state).await?;
                                        } else {
                                            // Начатая ломка другого блока: клиенту — его настоящее состояние.
                                            if let Some((old, _)) = digging.filter(|(old, _)| *old != a.pos) {
                                                let s = self.block(old).unwrap_or(AIR);
                                                conn.send(&play::block_update(ids.block_update, old, s as i32)).await?;
                                            }
                                            digging = Some((a.pos, game_ticks));
                                        }
                                    }
                                    play::STOP_DESTROY_BLOCK => match digging {
                                        Some((pos, start)) if pos == a.pos && block != AIR => {
                                            let p = self.destroy_progress(uuid, block, held, &state.vitals);
                                            if p * (game_ticks - start + 1) as f32 >= 0.7 {
                                                digging = None;
                                                self.mine(conn, uuid, a.pos, block, state).await?;
                                            } else if delayed.is_none() {
                                                // Клиент быстрее сервера: доломаем, когда хватит прогресса.
                                                digging = None;
                                                delayed = Some((pos, start));
                                            }
                                        }
                                        Some(_) => resync = true,
                                        None => {}
                                    },
                                    _ => {
                                        if digging.is_some_and(|(pos, _)| pos != a.pos) {
                                            resync = true;
                                        }
                                        digging = None;
                                    }
                                }
                                if resync {
                                    let s = self.block(a.pos).unwrap_or(AIR);
                                    conn.send(&play::block_update(ids.block_update, a.pos, s as i32)).await?;
                                }
                                conn.send(&play::block_changed_ack(ids.block_changed_ack, a.sequence)).await?;
                            }
                            play::DROP_ALL_ITEMS | play::DROP_ITEM if !state.vitals.dead => {
                                let held = state.inv.held_slot();
                                let all = a.action == play::DROP_ALL_ITEMS;
                                if let (Some(s), Some((feet, yaw, pitch))) = (state.inv.slots[held], self.player_at(uuid)) {
                                    let n = if all { s.count } else { 1 };
                                    let name = self.item_name(s.item);
                                    if !self.cancelled(Event::Drop, uuid, || format!("{name} {n}")).await {
                                        state.inv.slots[held] = (s.count > n).then_some(Stack { count: s.count - n, ..s });
                                        self.throw(feet, yaw, pitch, Stack { count: n, ..s });
                                        eating = None;
                                    }
                                }
                                self.send_slots(conn, &mut state.inv, &[held]).await?;
                            }
                            play::RELEASE_USE_ITEM => eating = None,
                            play::SWAP_ITEM_WITH_OFFHAND => {
                                let held = state.inv.held_slot();
                                state.inv.slots.swap(held, survival::OFFHAND);
                                self.send_slots(conn, &mut state.inv, &[held, survival::OFFHAND]).await?;
                            }
                            _ => {}
                        }
                        continue;
                    }
                    if id == ids.sb_use_item_on {
                        let u = play::read_use_item_on(&mut r)?;
                        let face = FACES.get(u.face as usize).copied().unwrap_or("UP");
                        let hand = if u.hand == 0 { "HAND" } else { "OFF_HAND" };
                        if !state.vitals.dead && self.cancelled(Event::Interact, uuid, || {
                            format!("RIGHT_CLICK_BLOCK {} {} {} {face} {hand}", u.pos[0], u.pos[1], u.pos[2])
                        }).await {
                            let held = state.inv.held_slot();
                            self.send_slots(conn, &mut state.inv, &[held]).await?;
                            if let Some(d) = play::direction_offset(u.face) {
                                let t = [u.pos[0] + d[0], u.pos[1] + d[1], u.pos[2] + d[2]];
                                let s = self.block(t).unwrap_or(AIR);
                                conn.send(&play::block_update(ids.block_update, t, s as i32)).await?;
                            }
                            conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence)).await?;
                            continue;
                        }
                        let clicked = self.state_name(self.block(u.pos).unwrap_or(AIR)).to_owned();
                        let kind = match clicked.as_str() {
                            "minecraft:crafting_table" => Some(Kind::Crafting),
                            "minecraft:chest" | "minecraft:trapped_chest" | "minecraft:barrel" => Some(Kind::Chest(3)),
                            _ => None,
                        };
                        // `useItemOn`: шифт с предметом в руке — не открывать, а ставить.
                        let bypass = sneaking && (state.inv.held().is_some() || state.inv.slots[survival::OFFHAND].is_some());
                        if let Some(kind) = kind.filter(|_| !state.vitals.dead && login.spawn.game_mode != 3 && !bypass) {
                            self.close_window(&mut state.inv, &mut window, uuid);
                            next_window = next_window % 100 + 1;
                            // Двойной сундук: первая половина — та, что `type=right` (`CompoundContainer`).
                            let pair = self.chest_partner(u.pos);
                            let (kind, pos, second) = match (kind, pair) {
                                (Kind::Chest(_), Some((partner, clicked_right))) => {
                                    if clicked_right { (Kind::Chest(6), u.pos, Some(partner)) } else { (Kind::Chest(6), partner, Some(u.pos)) }
                                }
                                _ => (kind, u.pos, None),
                            };
                            let (menu, title) = match kind {
                                Kind::Crafting => (self.survival.menu_crafting, "container.crafting"),
                                Kind::Chest(6) => (self.survival.menu_double_chest, "container.chestDouble"),
                                _ if clicked == "minecraft:barrel" => (self.survival.menu_chest, "container.barrel"),
                                _ => (self.survival.menu_chest, "container.chest"),
                            };
                            let mut w = Window { id: next_window, kind, pos, second, slots: Vec::new() };
                            w.slots = self.window_slots(&w);
                            conn.send(&inv::open_screen(ids.open_screen, w.id, menu, &serde_json::json!({"translate": title}))).await?;
                            self.send_window(conn, &mut state.inv, Some(&w), w.id).await?;
                            window = Some(w);
                            conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence)).await?;
                            continue;
                        }
                        let place = state.inv
                            .held()
                            .filter(|_| u.hand == 0 && !state.vitals.dead)
                            .and_then(|s| self.item_blocks.get(s.item as usize).copied().flatten());
                        if let (Some(mut place), Some(d)) = (place, play::direction_offset(u.face)) {
                            let target = [u.pos[0] + d[0], u.pos[1] + d[1], u.pos[2] + d[2]];
                            if let Some(c) = self.survival.chests.get(&place).copied() {
                                let yaw = self.player_at(uuid).map_or(0.0, |p| p.1);
                                place = self.chest_placement(target, c, yaw, u.face, sneaking);
                            }
                            if self.block(target) == Some(AIR) {
                                // `BlockPlaceEvent`: отмена — клиенту прежний блок и предмет.
                                let name = self.state_name(place).to_owned();
                                if self.cancelled(Event::Place, uuid, || format!(
                                    "{} {} {} {name} {} {} {} {hand}",
                                    target[0], target[1], target[2], u.pos[0], u.pos[1], u.pos[2],
                                )).await {
                                    conn.send(&play::block_update(ids.block_update, target, AIR as i32)).await?;
                                    let held = state.inv.held_slot();
                                    self.send_slots(conn, &mut state.inv, &[held]).await?;
                                    conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence)).await?;
                                    continue;
                                }
                                self.set_block(target, place);
                                self.link_chest(target, place);
                                if let Some(id) = container_entity(&name) {
                                    self.set_block_entity(target, Some(BlockEntity { id: id.into(), nbt: Default::default() }));
                                }
                                if survival && state.inv.consume_held() {
                                    let held = state.inv.held_slot();
                                    self.send_slots(conn, &mut state.inv, &[held]).await?;
                                }
                            }
                        }
                        conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence)).await?;
                        continue;
                    }
                    if id == ids.sb_chat_command || id == ids.sb_chat_command_signed {
                        let mut line = r.string(32767)?.to_owned();
                        tracing::info!("{} issued server command: /{line}", me.entry.name);
                        // `PlayerCommandPreprocessEvent`: отмена или подмена строки.
                        if let Some(reply) = self.plugin_event(Event::CommandPreprocess, uuid, || line.clone()).await {
                            let reply = reply.trim();
                            if reply == "c" {
                                continue;
                            }
                            if let Some(m) = reply.strip_prefix("m ") {
                                line = m.trim().trim_start_matches('/').to_owned();
                            }
                        }
                        // Встроенные без прав — как неизвестные (ванила их не показывает).
                        let builtin = line.split(' ').next().unwrap_or("");
                        let allowed = self.op_level(uuid) >= builtin_level(builtin);
                        let is_builtin = matches!(builtin, "give" | "gamemode" | "op" | "deop");
                        if is_builtin && allowed && let Some(name) = line.strip_prefix("op ").or(line.strip_prefix("deop ")) {
                            let msg = self.op_command(name.trim(), builtin == "op");
                            conn.send(&text::system_chat_json(ids.system_chat, &msg.to_string(), false)).await?;
                            continue;
                        }
                        if is_builtin && !allowed {
                            let msg = serde_json::json!({"translate": "command.unknown.command", "color": "red"});
                            conn.send(&text::system_chat_json(ids.system_chat, &msg.to_string(), false)).await?;
                            continue;
                        }
                        if let Some(arg) = line.strip_prefix("gamemode ") {
                            let Some(mode) = GAME_MODES.iter().position(|&m| m == arg.trim()) else {
                                let msg = serde_json::json!({"text": "Usage: /gamemode <survival|creative|adventure|spectator>", "color": "red"});
                                conn.send(&text::system_chat_json(ids.system_chat, &msg.to_string(), false)).await?;
                                continue;
                            };
                            let mode = mode as u8;
                            if self.cancelled(Event::GameMode, uuid, || format!("{mode} COMMAND")).await {
                                continue;
                            }
                            login.spawn.game_mode = mode;
                            state.game_mode = Some(mode);
                            survival = matches!(mode, 0 | 2);
                            if let Some((feet, _, _)) = self.player_at(uuid) {
                                state.vitals.reset_fall(feet);
                            }
                            self.set_game_mode(conn, uuid, mode, true).await?;
                            self.send_health(conn, &state.vitals).await?;
                            continue;
                        }
                        if let Some(args) = line.strip_prefix("give ") {
                            let msg = self.give(conn, &mut state.inv, args).await?;
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
                    if id == ids.sb_command_suggestion {
                        let tx = r.var_int()?;
                        let typed = r.string(32500)?.to_owned();
                        let line = typed.strip_prefix('/').unwrap_or(&typed).to_owned();
                        let start = typed.rfind(' ').map_or(0, |i| i + 1);
                        let reply = self.plugin_event(Event::TabComplete, uuid, || line).await.unwrap_or_default();
                        let matches: Vec<&str> = reply.lines().filter(|l| !l.is_empty()).collect();
                        conn.send(&text::command_suggestions(
                            ids.command_suggestions,
                            tx,
                            typed[..start].chars().count() as i32,
                            typed[start..].chars().count() as i32,
                            &matches,
                        )).await?;
                        continue;
                    }
                    if id == ids.sb_chat {
                        let text = play::read_chat_message(&mut r)?.to_owned();
                        // Плагины: `AsyncPlayerChatEvent`/`AsyncChatEvent` — отмена или своё сообщение.
                        let reply = self.plugin_event(Event::Chat, uuid, || text.clone()).await;
                        match reply.as_deref().map(str::trim) {
                            Some("c") => {}
                            Some(json) if !json.is_empty() => {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                                    tracing::info!("{}", text::plain_text(&v));
                                }
                                self.broadcast_json(json);
                            }
                            _ => {
                                let line = format!("<{}> {text}", me.entry.name);
                                tracing::info!("{line}");
                                self.hub.send(None, play::system_chat(ids.system_chat, &line, false));
                            }
                        }
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

    fn tool(&self, held: Slot) -> Option<&chunkwake_data::tools::Tool> {
        held.and_then(|s| self.survival.tools.get(s.item as usize)?.as_ref())
    }

    fn state_name(&self, state: u32) -> &str {
        self.survival
            .block_names
            .get(state as usize)
            .map_or("minecraft:air", String::as_str)
    }

    /// Прогресс ломки за тик предметом в руке: вода у глаз и опора под ногами.
    fn destroy_progress(&self, uuid: Uuid, state: u32, held: Slot, v: &Vitals) -> f32 {
        let hardness = self
            .survival
            .hardness
            .get(state as usize)
            .copied()
            .unwrap_or(0.0);
        let requires = self
            .survival
            .requires_tool
            .get(state as usize)
            .copied()
            .unwrap_or(false);
        let in_water = self.player_at(uuid).is_some_and(|(feet, _, _)| {
            let eye = [
                feet[0].floor() as i32,
                (feet[1] + EYE_HEIGHT).floor() as i32,
                feet[2].floor() as i32,
            ];
            let name = self.state_name(self.block(eye).unwrap_or(AIR));
            name == "minecraft:water" || name == "minecraft:bubble_column"
        });
        survival::destroy_progress(
            hardness,
            requires,
            self.tool(held),
            self.state_name(state),
            in_water,
            v.on_ground(),
        )
    }

    /// Выпадет ли что-то из блока, сломанного предметом `held`.
    fn drops_with(&self, state: u32, held: Slot) -> bool {
        let requires = self
            .survival
            .requires_tool
            .get(state as usize)
            .copied()
            .unwrap_or(false);
        survival::correct_tool(requires, self.tool(held), self.state_name(state))
    }

    /// Ломка блока игроком: плагины, затем мир; `drops` — выпадает ли предмет.
    /// Ломка в выживании: дроп по инструменту, голод и износ инструмента.
    async fn mine(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        pos: [i32; 3],
        block: u32,
        state: &mut PlayerState,
    ) -> Result<(), NetError> {
        let held = state.inv.held();
        let drops = self.drops_with(block, held);
        if !self.break_block(conn, uuid, pos, drops).await? {
            return Ok(());
        }
        state.vitals.exhaust(survival::EXHAUSTION_BREAK);
        // `ItemStack.mineBlock`: инструмент изнашивается, если блок ломается не мгновенно.
        let hardness = self.survival.hardness.get(block as usize).copied().unwrap_or(0.0);
        let slot = state.inv.held_slot();
        if hardness != 0.0
            && let Some(mut s) = held
            && let Some(tool) = self.tool(held)
        {
            let max = self.survival.items.get(s.item as usize).map_or(0, |i| i.max_damage);
            if max > 0 {
                s.damage += tool.damage_per_block;
                state.inv.slots[slot] = (s.damage < max).then_some(s);
                self.send_slots(conn, &mut state.inv, &[slot]).await?;
            }
        }
        Ok(())
    }

    /// Ломка блока игроком: плагины, затем мир; `drops` — выпадает ли предмет.
    /// `false` — плагин отменил.
    async fn break_block(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        pos: [i32; 3],
        drops: bool,
    ) -> Result<bool, NetError> {
        if self.plugins_cancel_break(uuid, pos).await {
            // Отменено: вернуть блок клиенту.
            let old = self.block(pos).unwrap_or(AIR);
            conn.send(&play::block_update(self.ids.block_update, pos, old as i32))
                .await?;
            return Ok(false);
        }
        let Some(old) = self.set_block(pos, AIR) else {
            return Ok(false);
        };
        self.unlink_chest(pos, old);
        // Содержимое сундука — на землю.
        if self.block_entity(pos).is_some() {
            let center = [pos[0] as f64 + 0.5, pos[1] as f64 + 0.5, pos[2] as f64 + 0.5];
            for s in self.container_items(pos).into_iter().flatten() {
                let vel = items::pop_velocity(rand::random(), rand::random());
                self.drop_item(center, vel, s, items::BLOCK_DELAY);
            }
            self.set_block_entity(pos, None);
        }
        if drops && let Some(item) = self.survival.drops.get(old as usize).copied().flatten() {
            let center = [
                pos[0] as f64 + 0.5,
                pos[1] as f64 + 0.25,
                pos[2] as f64 + 0.5,
            ];
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(center, vel, Stack { item, count: 1 , damage: 0}, items::BLOCK_DELAY);
        }
        Ok(true)
    }

    fn max_stack(&self, item: i32) -> i32 {
        self.survival.items.get(item as usize).map_or(64, |i| i.max_stack as i32)
    }

    /// Результат крафта для сетки `size × size`.
    fn craft_result(&self, grid: &[Slot], size: usize) -> Slot {
        let ids: Vec<Option<i32>> = grid.iter().map(|s| s.map(|s| s.item)).collect();
        let (item, count) = self.survival.recipes.find(&ids, size)?;
        Some(Stack { item, count, damage: 0 })
    }

    /// Клик в окне: итог считает сервер (`AbstractContainerMenu.doClick`).
    /// Возвращает выброшенное и нужна ли клиенту полная пересылка окна.
    fn click(
        &self,
        inv: &mut Inventory,
        window: &mut Option<Window>,
        c: &inv::ContainerClick,
        creative: bool,
        me: Uuid,
    ) -> (Vec<Stack>, bool) {
        let (kind, mut view) = if c.window == inv::PLAYER_WINDOW {
            (Kind::Player, inv.slots.to_vec())
        } else if let Some(w) = window.as_mut().filter(|w| w.id == c.window) {
            if matches!(w.kind, Kind::Chest(_)) {
                w.slots = self.window_slots(w);
            }
            (w.kind, w.slots.iter().chain(&inv.slots[9..45]).copied().collect())
        } else {
            return (Vec::new(), false);
        };
        // Что клиент ожидает увидеть после своего клика.
        let mut expected = view.clone();
        for &(s, stack) in &c.changed {
            if let Some(e) = expected.get_mut(s as usize) {
                *e = stack;
            }
        }
        let size = match kind {
            Kind::Player => 2,
            Kind::Crafting => 3,
            Kind::Chest(_) => 0,
        };
        let fill = |slots: &mut [Slot]| {
            if size > 0 {
                slots[0] = self.craft_result(&slots[1..=size * size], size);
            }
        };
        // `ResultSlot.onTake`: остаток (ведро, бутылка) — в опустевшую ячейку, иначе в инвентарь.
        let mut leftovers = Vec::new();
        let mut craft = |slots: &mut [Slot]| {
            for s in &mut slots[1..=size * size] {
                if let Some(x) = s {
                    let rest = self.craft_remainder(x.item);
                    x.count -= 1;
                    if x.count <= 0 {
                        *s = rest;
                    } else {
                        leftovers.extend(rest);
                    }
                }
            }
            fill(slots);
        };
        let max = |item: i32| self.max_stack(item);
        let equip = |item: i32| self.survival.items.get(item as usize).and_then(|i| i.equip);
        let rules = click::Rules {
            max_stack: &max,
            equip: &equip,
            creative,
        };
        let thrown = click::Menu {
            kind,
            slots: &mut view,
            carried: &mut inv.carried,
            drag: &mut inv.drag,
            rules: &rules,
            craft: &mut craft,
        }
        .click(c.slot, c.button, c.mode);
        fill(&mut view);
        let same = |a: &Slot, b: &Slot| a.map(|s| (s.item, s.count)) == b.map(|s| (s.item, s.count));
        let resync = c.state != inv.state
            || view.iter().zip(&expected).any(|(a, b)| !same(a, b))
            || !same(&inv.carried, &c.carried);
        let mut thrown = thrown;
        match window.as_mut().filter(|_| kind != Kind::Player) {
            Some(w) => {
                let n = w.slots.len();
                if matches!(kind, Kind::Chest(_)) && w.slots[..] != view[..n] {
                    w.slots.copy_from_slice(&view[..n]);
                    self.save_window(w, me);
                }
                w.slots.copy_from_slice(&view[..n]);
                inv.slots[9..45].copy_from_slice(&view[n..]);
            }
            None => inv.slots.copy_from_slice(&view),
        }
        for s in leftovers {
            thrown.extend(inv.add_stack(s, self.max_stack(s.item)));
        }
        (thrown, resync)
    }

    /// Окно всё ещё у своего блока (сундук не сломали).
    fn window_valid(&self, w: &Window) -> bool {
        match w.kind {
            Kind::Chest(_) => w.positions().iter().all(|&p| self.block_entity(p).is_some()),
            _ => self.block(w.pos).is_some_and(|s| self.state_name(s) == "minecraft:crafting_table"),
        }
    }

    /// Закрытие окна: сетка крафта и курсор возвращаются в инвентарь (не влезло — на землю).
    fn close_window(&self, inv: &mut Inventory, window: &mut Option<Window>, uuid: Uuid) {
        let mut back: Vec<Stack> = inv.slots[1..5].iter_mut().filter_map(Option::take).collect();
        inv.slots[0] = None;
        if let Some(mut w) = window.take()
            && w.kind == Kind::Crafting
        {
            back.extend(w.slots[1..10].iter_mut().filter_map(Option::take));
        }
        back.extend(inv.carried.take());
        inv.drag = Default::default();
        for s in back {
            if let Some(left) = inv.add_stack(s, self.max_stack(s.item))
                && let Some((feet, yaw, pitch)) = self.player_at(uuid)
            {
                self.throw(feet, yaw, pitch, left);
            }
        }
    }

    /// Окно целиком клиенту (свои слоты, затем инвентарь игрока).
    async fn send_window(
        &self,
        conn: &mut Connection,
        inv: &mut Inventory,
        window: Option<&Window>,
        id: i32,
    ) -> Result<(), NetError> {
        let Some(w) = window.filter(|w| w.id == id && id != inv::PLAYER_WINDOW) else {
            return self.send_inventory(conn, inv).await;
        };
        let view: Vec<Slot> = w.slots.iter().chain(&inv.slots[9..45]).copied().collect();
        let state = inv.next_state();
        conn.send(&inv::container_set_content(
            self.ids.container_set_content,
            w.id,
            state,
            &view,
            inv.carried,
        ))
        .await
    }

    /// Слоты окна из блок-сущностей (двойной сундук — две половины подряд).
    fn window_slots(&self, w: &Window) -> Vec<Slot> {
        match w.kind {
            Kind::Chest(_) => w.positions().iter().flat_map(|&p| self.container_items(p)).collect(),
            _ => w.slots.clone(),
        }
    }

    /// Сохраняет сундук окна и сообщает остальным, у кого он открыт.
    fn save_window(&self, w: &Window, me: Uuid) {
        for (i, &pos) in w.positions().iter().enumerate() {
            self.save_container(pos, &w.slots[i * 27..(i + 1) * 27]);
        }
        let controls = self.controls.lock().unwrap();
        for (_, tx) in controls.iter().filter(|(id, _)| **id != me) {
            for &pos in &w.positions() {
                let _ = tx.send(Control::ContainerChanged(pos));
            }
        }
    }

    /// Остаток предмета после крафта (`Item.craftRemainder` в `Items`).
    fn craft_remainder(&self, item: i32) -> Slot {
        let rest = match self.item_name(item) {
            "minecraft:water_bucket" | "minecraft:lava_bucket" | "minecraft:milk_bucket" => "minecraft:bucket",
            "minecraft:honey_bottle" | "minecraft:dragon_breath" => "minecraft:glass_bottle",
            _ => return None,
        };
        Some(Stack { item: self.item_id(rest)?, count: 1, damage: 0 })
    }

    fn chest_at(&self, pos: [i32; 3]) -> Option<ChestState> {
        self.survival.chests.get(&self.block(pos)?).copied()
    }

    /// Вторая половина двойного сундука и `true`, если `pos` — правая (первая) половина.
    fn chest_partner(&self, pos: [i32; 3]) -> Option<([i32; 3], bool)> {
        let c = self.chest_at(pos).filter(|c| c.kind != CHEST_SINGLE)?;
        let other = offset(pos, c.connected());
        let o = self.chest_at(other)?;
        (o.trapped == c.trapped && o.facing == c.facing && o.kind == 3 - c.kind).then_some((other, c.kind == CHEST_RIGHT))
    }

    /// `ChestBlock.getStateForPlacement`: смотрит на игрока, цепляется к соседу.
    fn chest_placement(&self, pos: [i32; 3], c: ChestState, yaw: f32, face: i32, sneaking: bool) -> u32 {
        // `Direction.fromYRot`: юг, запад, север, восток → N0 E1 S2 W3.
        let look = [2u8, 3, 0, 1][((yaw / 90.0 + 0.5).floor() as i32 & 3) as usize];
        let mut facing = (look + 2) % 4;
        let mut kind = CHEST_SINGLE;
        let partner = |dir: u8| {
            self.chest_at(offset(pos, dir))
                .filter(|o| o.trapped == c.trapped && o.kind == CHEST_SINGLE)
                .map(|o| o.facing)
        };
        let clicked = match face {
            2 => Some(0u8),
            5 => Some(1),
            3 => Some(2),
            4 => Some(3),
            _ => None,
        };
        if let Some(cf) = clicked.filter(|_| sneaking)
            && let Some(d) = partner((cf + 2) % 4)
            && d % 2 != cf % 2
        {
            facing = d;
            kind = if (d + 3) % 4 == (cf + 2) % 4 { CHEST_RIGHT } else { CHEST_LEFT };
        }
        if kind == CHEST_SINGLE && !sneaking {
            if partner((facing + 1) % 4) == Some(facing) {
                kind = CHEST_LEFT;
            } else if partner((facing + 3) % 4) == Some(facing) {
                kind = CHEST_RIGHT;
            }
        }
        let want = ChestState { facing, kind, ..c };
        self.survival.chest_states.get(&want).copied().unwrap_or_else(|| {
            self.survival.chest_states.get(&ChestState { kind: CHEST_SINGLE, ..want }).copied().unwrap_or(0)
        })
    }

    /// Поставленный двойной сундук: сосед становится второй половиной (`updateShape`).
    fn link_chest(&self, pos: [i32; 3], state: u32) {
        let Some(c) = self.survival.chests.get(&state).copied().filter(|c| c.kind != CHEST_SINGLE) else {
            return;
        };
        let other = offset(pos, c.connected());
        if let Some(o) = self.chest_at(other)
            && let Some(&s) = self.survival.chest_states.get(&ChestState { kind: 3 - c.kind, ..o })
        {
            self.set_block(other, s);
        }
    }

    /// Сломанная половина: оставшаяся становится одиночной.
    fn unlink_chest(&self, pos: [i32; 3], old: u32) {
        let Some(c) = self.survival.chests.get(&old).copied().filter(|c| c.kind != CHEST_SINGLE) else {
            return;
        };
        let other = offset(pos, c.connected());
        if let Some(o) = self.chest_at(other).filter(|o| o.kind == 3 - c.kind && o.facing == c.facing)
            && let Some(&s) = self.survival.chest_states.get(&ChestState { kind: CHEST_SINGLE, ..o })
        {
            self.set_block(other, s);
        }
    }

    fn block_entity(&self, pos: [i32; 3]) -> Option<BlockEntity> {
        let key = (pos[0] >> 4, pos[2] >> 4);
        self.ensure_region(key.0, key.1);
        let world = self.edited.lock().unwrap();
        world
            .get(&key)
            .map_or(&self.template, |e| &e.0)
            .block_entities
            .get(&pos)
            .cloned()
    }

    /// Ставит или убирает блок-сущность; чанк сохранится и перекодируется.
    fn set_block_entity(&self, pos: [i32; 3], be: Option<BlockEntity>) {
        let key = (pos[0] >> 4, pos[2] >> 4);
        self.ensure_region(key.0, key.1);
        let mut world = self.edited.lock().unwrap();
        let (column, rev) = world
            .entry(key)
            .or_insert_with(|| (self.template.clone(), 0));
        match be {
            Some(be) => {
                column.block_entities.insert(pos, be);
            }
            None => {
                if column.block_entities.remove(&pos).is_none() {
                    return;
                }
            }
        }
        *rev += 1;
        self.chunk_frames.lock().unwrap().remove(&key);
    }

    /// 27 слотов сундука из `Items` его блок-сущности.
    fn container_items(&self, pos: [i32; 3]) -> Vec<Slot> {
        let mut slots = vec![None; 27];
        let Some(be) = self.block_entity(pos) else {
            return slots;
        };
        for t in be.nbt.get("Items").and_then(Tag::as_list).unwrap_or(&[]) {
            let Some(c) = t.as_compound() else { continue };
            let slot = c.get("Slot").and_then(Tag::as_i64).unwrap_or(-1);
            let item = c.get("id").and_then(Tag::as_str).and_then(|n| self.item_id(n));
            let count = c.get("count").and_then(Tag::as_i64).unwrap_or(1) as i32;
            let damage = match c.get("components") {
                Some(Tag::Compound(m)) => m.get("minecraft:damage").and_then(Tag::as_i64).unwrap_or(0) as i32,
                _ => 0,
            };
            if let (Some(item), Some(dst)) = (item, slots.get_mut(slot.max(0) as usize).filter(|_| slot >= 0)) {
                *dst = (count > 0).then_some(Stack { item, count, damage });
            }
        }
        slots
    }

    /// Сохраняет слоты сундука в его блок-сущность (`Items` как в ванилле).
    fn save_container(&self, pos: [i32; 3], slots: &[Slot]) {
        let id = self
            .block(pos)
            .and_then(|s| container_entity(self.state_name(s)))
            .unwrap_or("minecraft:chest");
        let items: Vec<Tag> = slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let s = (*s)?;
                let mut c = Compound::from([
                    ("Slot".into(), Tag::Byte(i as i8)),
                    ("id".into(), Tag::String(self.item_name(s.item).to_owned())),
                    ("count".into(), Tag::Int(s.count)),
                ]);
                if s.damage > 0 {
                    let comps = Compound::from([("minecraft:damage".into(), Tag::Int(s.damage))]);
                    c.insert("components".into(), Tag::Compound(comps));
                }
                Some(Tag::Compound(c))
            })
            .collect();
        let mut nbt = self.block_entity(pos).map(|b| b.nbt).unwrap_or_default();
        nbt.insert("Items".into(), Tag::List(items));
        self.set_block_entity(pos, Some(BlockEntity { id: id.into(), nbt }));
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
    /// Смена режима (`ServerPlayer.setGameMode`): событие клиенту, способности,
    /// таб-лист всем, сообщение как у ванилы.
    async fn set_game_mode(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        mode: u8,
        announce: bool,
    ) -> Result<(), NetError> {
        let ids = self.ids;
        conn.send(&play::game_event(
            ids.game_event,
            game_event::CHANGE_GAME_MODE,
            mode as f32,
        ))
        .await?;
        conn.send(&play::player_abilities(ids.player_abilities, mode))
            .await?;
        if let Some(p) = self.hub.players.lock().unwrap().get_mut(&uuid) {
            p.entry.game_mode = mode as i32;
        }
        self.hub.send(
            None,
            play::player_info_game_mode(ids.player_info_update, uuid, mode as i32),
        );
        if !announce {
            return Ok(());
        }
        let name = GAME_MODES[mode as usize];
        let msg = serde_json::json!({
            "translate": "commands.gamemode.success.self",
            "with": [{"translate": format!("gameMode.{name}")}],
        });
        conn.send(&text::system_chat_json(
            ids.system_chat,
            &msg.to_string(),
            false,
        ))
        .await
    }

    async fn give(
        &self,
        conn: &mut Connection,
        inv: &mut Inventory,
        args: &str,
    ) -> Result<serde_json::Value, NetError> {
        let mut it = args.split_whitespace();
        let name = it.next().unwrap_or("");
        // `item_stack` может прийти с компонентами: `bread[...]` — берём только имя.
        let name = name.split(['[', '{']).next().unwrap_or("");
        let name = if name.contains(':') {
            name.to_owned()
        } else {
            format!("minecraft:{name}")
        };
        let count = it.next().map_or(Ok(1), str::parse::<i32>);
        let (Some(&item), Ok(count @ 1..=MAX_GIVE)) = (self.survival.item_ids.get(&name), count)
        else {
            return Ok(
                serde_json::json!({"text": format!("Usage: /give <item> [1-{MAX_GIVE}]"), "color": "red"}),
            );
        };
        let max = self
            .survival
            .items
            .get(item as usize)
            .map_or(64, |i| i.max_stack);
        let (changed, _) = inv.add(item, count, max as i32);
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
        inv: &mut Inventory,
        amount: f32,
        cause: Cause,
    ) -> Result<(), NetError> {
        let mut amount = amount;
        if let Some(reply) = self
            .plugin_event(Event::Damage, me.entry.uuid, || {
                format!("{} {amount}", cause.bukkit())
            })
            .await
        {
            match reply.trim() {
                "c" => return Ok(()),
                r => {
                    if let Some(a) = r.strip_prefix("d ").and_then(|a| a.parse::<f32>().ok()) {
                        amount = a.max(0.0);
                    }
                }
            }
        }
        let died = v.damage(amount);
        self.send_health(conn, v).await?;
        let damage_type = match cause {
            Cause::Fall => Some(self.survival.damage_fall),
            Cause::Void => Some(self.survival.damage_void),
            Cause::Starve => Some(self.survival.damage_starve),
            Cause::Generic => None,
        };
        if let Some(t) = damage_type {
            self.hub.send(
                None,
                inv::damage_event(self.ids.damage_event, me.entity_id, t),
            );
        }
        if died {
            self.die(conn, me, inv, cause).await?;
        }
        Ok(())
    }

    /// Смерть (`ServerPlayer.die`): `PlayerDeathEvent`, выпадение инвентаря, сообщение.
    async fn die(
        &self,
        conn: &mut Connection,
        me: &Online,
        inv: &mut Inventory,
        cause: Cause,
    ) -> Result<(), NetError> {
        {
            self.hub.send(
                Some(me.entry.uuid),
                inv::entity_event(self.ids.entity_event, me.entity_id, inv::ENTITY_EVENT_DEATH),
            );
            let mut drops: Vec<Stack> = inv
                .slots
                .iter()
                .chain(std::iter::once(&inv.carried))
                .flatten()
                .copied()
                .collect();
            let msg = serde_json::json!({"translate": cause.death_key(), "with": [me.entry.name]});
            let mut message = Some(msg.to_string());
            let mut keep = false;
            let reply = self
                .plugin_event(Event::Death, me.entry.uuid, || {
                    let mut arg = format!("{}\n{msg}", cause.damage_type());
                    for s in &drops {
                        arg += &format!("\n{} {}", self.item_name(s.item), s.count);
                    }
                    arg
                })
                .await;
            if let Some(reply) = reply {
                let mut lines = reply.lines();
                keep = lines.next() == Some("1");
                message = lines.next().filter(|m| !m.is_empty()).map(str::to_owned);
                drops = live::parse_stacks(lines, |n| self.item_id(n));
            }
            // Без keepInventory инвентарь очищается; выпадает список из события.
            if !keep {
                inv.slots.fill(None);
                inv.carried = None;
                self.send_inventory(conn, inv).await?;
            }
            if let Some((feet, _, _)) = self.player_at(me.entry.uuid) {
                for s in drops {
                    let vel = items::scatter_velocity(rand::random(), rand::random());
                    self.drop_item(items::throw_origin(feet), vel, s, items::THROW_DELAY);
                }
            }
            tracing::info!("{} died ({})", me.entry.name, cause.death_key());
            if let Some(m) = message {
                self.broadcast_json(&m);
            }
        }
        Ok(())
    }

    /// Телепорт игрока сервером: позиция клиенту, всем остальным и сдвиг чанков.
    /// Возвращает новый центр.
    #[allow(clippy::too_many_arguments)]
    async fn teleport(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        pos: [f64; 3],
        yaw: f32,
        pitch: f32,
        teleport_id: i32,
        center: (i32, i32),
        r_view: i32,
    ) -> Result<(i32, i32), NetError> {
        let now = ((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
        if now != center {
            self.move_center(conn, center, now, r_view).await?;
        }
        conn.send(&play::player_position(self.ids.player_position, teleport_id, pos, yaw, pitch))
            .await?;
        self.moved(
            uuid,
            Move {
                pos: Some(pos),
                rot: Some((yaw, pitch)),
                on_ground: false,
            },
        );
        Ok(now)
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
