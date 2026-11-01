//! Play-сессия M2: плоский мир, стриминг чанков, чат, таб-лист и видимость
//! других игроков. Мир статичен.

use crate::click::{self, Kind};
use crate::furnace::{self, Furnace};
use crate::items::{self, Dropped};
use crate::live::{self, Control, Event, Live};
use crate::ops::OpList;
use crate::survival::{self, Cause, Difficulty, Inventory, PlayerSave, PlayerState, Vitals};
use crate::{Connection, LoggedIn, NetError};
use chunkwake_data::DataError;
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Reader;
use chunkwake_protocol::frame::FrameCodec;
use chunkwake_protocol::inventory::{self as inv, Slot, Stack};
use chunkwake_protocol::play::{self, Login, Move, PlayerEntry, game_event};
use chunkwake_protocol::text;
use chunkwake_world::light::{Engine, LightAccess, LightTable};
use chunkwake_world::nbt::{Compound, Tag};
use chunkwake_world::{BlockEntity, ChunkColumn};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU8, Ordering};
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
    set_default_spawn_position: i32,
    initialize_border: i32,
    set_border_center: i32,
    set_border_lerp_size: i32,
    set_border_size: i32,
    set_border_warning_delay: i32,
    set_border_warning_distance: i32,
    set_camera: i32,
    swing_animation: i32,
    set_passengers: i32,
    boss_event: i32,
    post_effects: i32,
    chunks_biomes: i32,
    waypoint: i32,
    clear_dialog: i32,
    level_particles: i32,
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
    block_entity_data: i32,
    block_destruction: i32,
    section_blocks_update: i32,
    block_changed_ack: i32,
    sb_player_action: i32,
    sb_use_item_on: i32,
    sb_set_creative_mode_slot: i32,
    sb_pick_item_from_block: i32,
    sb_set_carried_item: i32,
    container_set_content: i32,
    container_set_slot: i32,
    container_set_data: i32,
    set_experience: i32,
    sb_container_button_click: i32,
    set_health: i32,
    respawn: i32,
    damage_event: i32,
    entity_event: i32,
    sb_container_click: i32,
    sb_attack: i32,
    sb_interact: i32,
    hurt_animation: i32,
    sb_container_close: i32,
    sb_player_input: i32,
    sb_player_abilities: i32,
    open_screen: i32,
    container_close: i32,
    sb_client_command: i32,
    sb_player_command: i32,
    sb_use_item: i32,
    commands: i32,
    block_event: i32,
    sound: i32,
    level_event: i32,
    set_time: i32,
    change_difficulty: i32,
    update_mob_effect: i32,
    update_attributes: i32,
    remove_mob_effect: i32,
    sb_chat_command: i32,
    sb_chat_command_signed: i32,
    /// Заголовки, звук-стоп и счёт: нужны `/title`, `/stopsound`, `/scoreboard`.
    set_title_text: i32,
    set_subtitle_text: i32,
    set_action_bar_text: i32,
    set_titles_animation: i32,
    clear_titles: i32,
    stop_sound: i32,
    set_objective: i32,
    set_score: i32,
    reset_score: i32,
    set_display_objective: i32,
    transfer: i32,
}

pub struct PlayPlan {
    ids: Ids,
    login: Login,
    /// Тело чанка (одинаковое для всех координат плоского мира).
    chunk_body: Vec<u8>,
    /// Чанк генератора (плоский мир); изменённые чанки — в `edited`.
    template: ChunkColumn,
    /// Плоский `the_void`: камень и булыжник стартовой платформы.
    void_platform: Option<(u32, u32)>,
    /// Нормальный мир: столбцы, которых нет на диске. Плоский мир — `None`.
    terrain: Option<TerrainFn>,
    /// `/locate biome`: ближайшая точка биома от блока. `None` — плоский мир.
    locate_biome: Option<LocateFn>,
    /// `/locate structure`: ближайший старт поставленной структуры. `None` — плоский мир.
    locate_structure: Option<LocateFn>,
    /// Тег точки интереса (`minecraft:village`) → типы.
    poi_tags: HashMap<String, HashSet<String>>,
    /// Id и `#теги` `worldgen/structure` для tab-complete. Клиенту этот реестр не шлётся.
    structure_names: Vec<String>,
    /// Точки интереса загруженных чанков: чанк → позиции и тип.
    pois: Mutex<FastMap<(i32, i32), Vec<Poi>>>,
    /// Изменённые столбцы и номер их правки (для сверки с кэшем кадров).
    edited: std::sync::RwLock<Columns>,
    /// Номер правки, записанный на диск, по столбцам.
    saved: Mutex<FastMap<(i32, i32), u64>>,
    /// Столбцы, чья правка выросла после последней записи (скан автосейва — по ним).
    dirty: Mutex<FastSet<(i32, i32)>>,
    /// Изменения блоков текущего тика: секция → (локальный индекс, состояние).
    pending_blocks: Mutex<PendingBlocks>,
    /// Правка мира целиком (установка с формой и сигналом, тик редстоуна):
    /// рассылка не видит промежуточных состояний, иначе клиент мигает.
    world_edit: Mutex<()>,
    /// Пролив света при генерации чанка: `spread_chunk` читает соседей и
    /// пишет в них, поэтому два одновременных пролива дают тёмный шов.
    light_lock: Mutex<()>,
    light: Arc<LightTable>,
    /// Ленивая загрузка с диска: источник и уже прочитанные регионы.
    source: std::sync::OnceLock<Arc<dyn ChunkSource>>,
    regions: Mutex<FastSet<(i32, i32)>>,
    /// Замки загрузки по шардам: разные регионы читаются параллельно.
    /// Один общий замок сериализовал всю загрузку — на 32 это четыре региона
    /// по 1024 чанка NBT строго по очереди.
    region_load: Vec<Mutex<()>>,
    /// Пул генерации: тяжёлый счёт чанков идёт вне задачи сессии.
    gen_pool: std::sync::OnceLock<Arc<crate::chunkgen::GenPool>>,
    /// Слабая ссылка на себя: задачам пула нужен `'static` доступ к плану.
    self_weak: std::sync::OnceLock<std::sync::Weak<Self>>,
    min_y: i32,
    /// ID предмета → состояние блока, который он ставит.
    item_blocks: Vec<Option<u32>>,
    spawn: [f64; 3],
    /// Радиус `spawn-protection`. 0 — выключено. Не-оператор не ломает и не ставит внутри.
    spawn_protection: std::sync::atomic::AtomicI32,
    /// ID типа сущности `minecraft:player` в реестре `entity_type`.
    player_type: i32,
    next_entity_id: AtomicI32,
    hub: Hub,
    /// Кадры `level_chunk_with_light`, уже сжатые: один раз на чанк для всех игроков.
    chunk_frames: Mutex<FastMap<(i32, i32), Frame>>,
    /// Кадры появления сущностей: один пакет на многих зрителей.
    spawn_frames: Mutex<SpawnFrameCache>,
    /// Кадры удаления сущности: тоже один пакет на всех, кто её видел.
    remove_frames: Mutex<FastMap<i32, Frame>>,
    /// Состояние блока → имя (`minecraft:stone`), для событий плагинов.
    block_names: Arc<Vec<String>>,
    /// ID `brigadier:string` в `command_argument_type`.
    parsers: text::CommandParsers,
    survival: SurvivalData,
    hooks: std::sync::OnceLock<Arc<dyn PluginHooks>>,
    /// Команды плагинов для дерева `commands` у клиента.
    plugin_commands: Mutex<Vec<String>>,
    /// Предметы на земле по ID сущности.
    dropped: Mutex<FastMap<i32, Dropped>>,
    /// Индекс предметов по чанкам: `(cx, cz)` → ID предметов в нём.
    ///
    /// Подбор игроком и всасывание воронкой раньше сканировали **все**
    /// предметы мира: O(игроков × предметов) и O(воронок × предметов) за тик.
    /// С индексом ищутся только чанки в радиусе действия.
    dropped_by_chunk: Mutex<FastMap<(i32, i32), Vec<i32>>>,
    /// Сферы опыта.
    orbs: Mutex<FastMap<i32, crate::orbs::Orb>>,
    /// Работающие и открытые печи.
    furnaces: Mutex<FastMap<[i32; 3], FurnaceEntry>>,
    /// Запас буферов тика предметов (синхронизации, исчезнувшие, слияние):
    /// тик берёт их и возвращает, а не аллоцирует заново.
    items_syncs: Mutex<Vec<ItemSync>>,
    items_gone: Mutex<Vec<i32>>,
    items_merging: Mutex<Vec<i32>>,
    /// Отложенные тики редстоуна (`scheduleTick`): позиция и тик сервера.
    redstone_ticks: Mutex<crate::ticks::TickQueue>,
    /// Летящие и воткнутые стрелы по id сущности.
    arrows: Mutex<FastMap<i32, crate::arrows::Arrow>>,
    /// Инвентари плагинов (`Bukkit.createInventory`) по номеру.
    plugin_invs: Mutex<FastMap<i32, PluginInv>>,
    next_plugin_inv: std::sync::atomic::AtomicI32,
    /// Отложенные тики жидкостей (`FlowingFluid`): место и тик.
    fluid_ticks: Mutex<crate::ticks::TickQueue>,
    /// Переключения факелов за последние тики — для перегорания.
    torch_toggles: Mutex<FastMap<[i32; 3], Vec<u64>>>,
    /// До какого тика перегоревший факел нельзя зажечь снова (`BURNOUT_COOLDOWN`).
    torch_cool: Mutex<FastMap<[i32; 3], u64>>,
    /// Игроки на нажимных плитах: позиция плиты и сколько их.
    plates: Mutex<FastMap<[i32; 3], usize>>,
    /// Воронки мира и их пауза в тиках.
    hopper_ticks: Mutex<crate::ticks::TickQueue>,
    /// Блоки, поставленные плагинами с физикой: соседей обновит тик.
    deferred_updates: Mutex<Vec<[i32; 3]>>,
    /// Блоки в движении от поршней: тик сервера и что поставить на место.
    piston_moves: Mutex<Vec<(u64, Moves)>>,
    /// Выходы компараторов (`OutputSignal`), загружаются из блок-сущности.
    comparators: Mutex<FastMap<[i32; 3], i32>>,
    /// Тики сервера (для редстоуна).
    server_ticks: std::sync::atomic::AtomicU64,
    /// Печи из загруженных с диска чанков — подхватит тик.
    pending_furnaces: Mutex<Vec<[i32; 3]>>,
    /// Костры с едой: слоты и прогресс.
    campfires: Mutex<FastMap<[i32; 3], crate::campfire::Campfire>>,
    /// Костры из загруженных чанков — подхватит тик.
    pending_campfires: Mutex<Vec<[i32; 3]>>,
    players_source: std::sync::OnceLock<Arc<dyn PlayerSource>>,
    /// Несохранённые снимки игроков.
    player_saves: Mutex<FastMap<Uuid, PlayerSave>>,
    ops: std::sync::OnceLock<OpList>,
    /// Имя предмета по ID (`minecraft:bread`).
    item_names: Vec<String>,
    /// Зеркало онлайн-игроков для плагинов и каналы управления их сессиями.
    live: Mutex<FastMap<Uuid, Live>>,
    /// Старый снимок игрока: ревизии больше нет — отпечаток считается по полям.
    state_cache: Mutex<FastMap<Uuid, (u64, Arc<str>)>>,
    /// Живые мобы по ID сущности.
    mobs: Mutex<FastMap<i32, mobs::MobEntry>>,
    /// Чанки, уже заселённые мобами при генерации (или прочитанные из `entities/`).
    populated: Mutex<FastSet<(i32, i32)>>,
    /// Прочитанные с диска мобы: тик заберёт их в `mobs` (регион может читаться из самого тика).
    mobs_loaded: Mutex<Vec<mobs::MobEntry>>,
    /// Чанки `entities/`, где по последней записи были мобы.
    entity_written: Mutex<FastSet<(i32, i32)>>,
    /// Новые заселённые чанки, ещё не записанные.
    entity_new: Mutex<FastSet<(i32, i32)>>,
    controls: Mutex<FastMap<Uuid, tokio::sync::mpsc::UnboundedSender<Control>>>,
    shutting_down: AtomicBool,
    /// Кто смотрит сундук или бочку: крышка открыта, пока список не пуст.
    open_blocks: Mutex<FastMap<[i32; 3], Vec<Uuid>>>,
    /// Чанки в радиусе симуляции вокруг игроков: только они тикают.
    ///
    /// За `Arc`, потому что набор читают десять подсистем за тик. Раньше
    /// каждая делала `.lock().unwrap().clone()` — при 10 игроках и радиусе 6
    /// это ~1.7 тыс. хэш-вставок × 10 каждые 50 мс. Теперь это одна атомарная
    /// прибавка на подсистему, а сам набор пересобирается только когда
    /// кто-то переехал в другой чанк.
    simulated: Mutex<Arc<FastSet<(i32, i32)>>>,
    /// Состав, для которого посчитан `simulated`: радиус и чанки игроков.
    /// Пока он тот же, набор не пересобирается.
    simulated_key: Mutex<Option<SimulatedKey>>,
    /// Время суток (0..24000). Тик двигает его и шлёт клиенту, клиент сам не считает.
    day_time: AtomicI64,
    /// Сколько серверных тиков прошло с запуска (`/time query gametime`).
    game_time: AtomicI64,
    /// Замеры тика: окна TPS/MSPT, перцентили и управление `/tick`.
    pub(crate) ticks: Mutex<chunkwake_metrics::TickStats>,
    /// Переменные сервера: геймрулы, погода, белый список, баны.
    pub(crate) vars: crate::play::commands::state::ServerVars,
    /// Счётчики сети для снимка нагрузки.
    pub(crate) net_stats: crate::play::commands::net::NetStats,
    /// Пинг игроков по RTT `keep_alive`: uuid → мс.
    pub(crate) pings: Mutex<FastMap<Uuid, f32>>,
    /// Загрузка процесса из сэмплера сервера: `/top` её не опрашивает сам,
    /// потому что `sysinfo` считает проценты между двумя опросами.
    pub(crate) proc_stats: Mutex<chunkwake_metrics::ProcStats>,
    /// Есть ли работа в этом тике: игроки, сущности, отложенные тики.
    ///
    /// Флаг ставится теми, кто добавляет работу (вход игрока, спавн, дроп,
    /// расписание), и пересчитывается в конце каждого рабочего тика. Раньше
    /// `idle()` на каждом тике пробовал десяток мьютексов — 250 нс простоя на
    /// пустом сервере, то есть в сто раз больше самого тика.
    work_pending: AtomicBool,
    /// Профиль включён (`/debug start` или внешний приёмник): без него тик
    /// не берёт замки профиля и не зовёт часы системы.
    profiling: AtomicBool,
    /// Пересчёт `/debug`: тики в файл.
    debug_profile: Mutex<Option<DebugProfile>>,
    /// Куда отдавать замеры по фазам тика: ставит сервер, чтобы профиль
    /// попадал в тот же `Live`, что и остальные метрики. Без приёмника
    /// замеры просто не собираются — лаборатории и тесты его не ставят.
    profile_sink: Mutex<Option<ProfileSink>>,
    /// Куда писать профиль: каталог мира.
    pub(crate) debug_dir: std::path::PathBuf,
    /// Просьба остановить сервер (`/stop`, сигнал консоли).
    shutdown: AtomicBool,
    /// Зерно мира для `/seed`.
    pub(crate) seed: i64,
    /// Название сервера для снимка нагрузки (`motd`).
    pub(crate) motd: String,
    /// Сколько занял последний автосейв, мс.
    pub(crate) autosave_ms: std::sync::atomic::AtomicU32,
    /// С какого момента считаем аптайм.
    pub(crate) started: Instant,
    /// Порог сжатия: попадает в снимок и `/perf`.
    pub(crate) compression_threshold: i32,
    /// Точки возрождения игроков (`/spawnpoint`); `None` — спавн мира.
    pub(crate) spawn_points: Mutex<HashMap<Uuid, Option<[i32; 3]>>>,
    /// Режим игры по умолчанию (`/defaultgamemode`).
    pub(crate) default_game_mode: std::sync::atomic::AtomicU8,
    /// Просьба сохранить мир (`/save-all`).
    pub(crate) save_requested: AtomicBool,
    /// Белый список включён (`/whitelist on`).
    pub(crate) whitelist_enabled: AtomicBool,
    /// `allow-flight`: выживание и приключение тоже могут летать.
    pub(crate) allow_flight: AtomicBool,
    /// Каталог сервера для `/reload` (`ops.json`).
    pub(crate) reload_dir: Option<std::path::PathBuf>,
    /// Таблица счёта `scoreboard`.
    pub(crate) scoreboard: Mutex<crate::play::commands::scoreboard::Scoreboard>,
    /// Команды `/team`.
    pub(crate) teams: Mutex<crate::play::commands::team::Board>,
    /// Хвост NBT сущности, который не является живым полем (`CustomName` и т. п.).
    pub(crate) entity_extra: Mutex<HashMap<Uuid, Compound>>,
    /// Метки `/tag`. Ключ — UUID цели, у моба он выводится из id сущности.
    pub(crate) entity_tags: Mutex<HashMap<Uuid, Vec<String>>>,
    /// Последовательности `/random`: имя → состояние генератора.
    pub(crate) random_seq: Mutex<HashMap<String, u64>>,
    /// Полоски `/bossbar`.
    pub(crate) boss_bars: Mutex<HashMap<String, crate::play::commands::extra::BossBar>>,
    /// Секундомеры `/stopwatch` (реальное время, не тики).
    pub(crate) stopwatches: Mutex<HashMap<String, crate::play::commands::extra::Watch>>,
    /// Экранные эффекты `/posteffect` по игроку.
    pub(crate) post_effects: Mutex<HashMap<Uuid, Vec<String>>>,
    /// База и модификаторы `/attribute` по UUID сущности.
    pub(crate) entity_attributes:
        Mutex<HashMap<Uuid, HashMap<String, crate::play::commands::attribute::AttrState>>>,
    /// Цвет и стиль `/waypoint` по UUID игрока.
    pub(crate) waypoints: Mutex<HashMap<Uuid, crate::play::commands::waypoint::WaypointIcon>>,
    /// Протокольные id `particle_type` для `/particle`.
    particle_types: Mutex<HashMap<String, i32>>,
    /// `en_us.json` из ванильного jar: фразы команд в консоли.
    command_lang: Mutex<HashMap<String, String>>,
    /// Связи `execute on`: vehicle, passengers и остальные, если они уже есть.
    pub(crate) links: Mutex<HashMap<Uuid, crate::play::commands::execute::Links>>,
    /// Имя мира (`level-name`) для сообщений.
    pub(crate) level_name: String,
    /// Версия игры для `/version` и снимка.
    pub(crate) version_name: String,
    pub(crate) version_protocol: i32,
    /// Чанки, которые тикают всегда (`/forceload`).
    pub(crate) forced: Mutex<FastSet<(i32, i32)>>,
    /// Регенерация, отравление, иссушение, голод — id из реестра, один раз при старте.
    effect_kinds: survival::EffectKinds,
    /// `wet_sponge`, `bucket`, `water_bucket` для тика печи.
    sponge: Option<(i32, i32, i32)>,
}

/// Генератор столбца по координатам чанка: `None` — чанк уже отдан.
pub type TerrainFn = Arc<dyn Fn(i32, i32) -> Option<ChunkColumn> + Send + Sync>;
/// `/locate`: ближайшая точка от блока.
pub type LocateFn = Arc<dyn Fn(&str, [i32; 3]) -> Option<[i32; 3]> + Send + Sync>;
/// Кадр появления по квантованной позиции: id, позиция, поворот, элитра.
type SpawnFrameCache = FastMap<(i32, [i64; 3], u32, u32, bool), Frame>;
/// Состав, по которому посчитан набор симуляции: радиус, чанки игроков, форс.
type SimulatedKey = (i32, Vec<(i32, i32)>, Vec<(i32, i32)>);
/// Сжатый кадр, общий для многих соединений.
type Frame = Arc<Vec<u8>>;

/// Приёмник профиля по фазам: секунда и замеры.
///
/// Функция, а не ссылка на `Live`: `net` не должен знать про `chunkwake-metrics`
/// ничего сверх типов замеров, а тесты и лаборатория подключают свой приёмник
/// или обходятся без него.
pub type ProfileSink = Arc<dyn Fn(u64, &[f32; chunkwake_metrics::PHASE_COUNT]) + Send + Sync>;

/// `/debug start`: куда писать профиль и сколько тиков уже записано.
pub struct DebugProfile {
    path: std::path::PathBuf,
    file: std::io::BufWriter<std::fs::File>,
    ticks: u32,
}

impl DebugProfile {
    /// Создаёт файл рядом с миром: `debug/<время>.txt`.
    pub fn create(dir: &std::path::Path) -> Option<Self> {
        let dir = dir.join("debug");
        std::fs::create_dir_all(&dir).ok()?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let path = dir.join(format!("profile-{stamp}.txt"));
        let file = std::fs::File::create(&path).ok()?;
        Some(Self {
            path,
            file: std::io::BufWriter::new(file),
            ticks: 0,
        })
    }

    fn write(&mut self, tick: u64, work_ms: f64) {
        use std::io::Write;
        if self.ticks < 20 * 60 * 5 && writeln!(self.file, "{tick}\t{work_ms:.3}").is_ok() {
            self.ticks += 1;
        }
    }
}

/// Поток генерации уступает обычному тику: ядра заняты, но не в ущерб ему.
#[cfg(windows)]
pub(crate) fn background_priority() {
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadPriority(handle: *mut std::ffi::c_void, priority: i32) -> i32;
    }
    /// `THREAD_PRIORITY_BELOW_NORMAL`.
    const BELOW_NORMAL: i32 = -1;
    // SAFETY: псевдо-дескриптор текущего потока (-2), меняется только его приоритет.
    unsafe {
        SetThreadPriority(GetCurrentThread(), BELOW_NORMAL);
    }
}

#[cfg(not(windows))]
pub(crate) fn background_priority() {}

/// Предел кэша кадров чанков. Кадр — уже сжатый чанк, около 4.7 КиБ, так что
/// видимая область одного игрока при радиусе 32 — это 4 225 чанков, и кэш
/// нужен, чтобы при повторном заходе не кодировать и не сжимать заново, а не
/// чтобы держать весь пройденный мир. 16 384 кадра — это ~75 МБ и запас на
/// нескольких игроков.
const CHUNK_CACHE_LIMIT: usize = 16_384;

/// Предел кэша кадров появления. Один кадр — сотня байт; ключ меняется с
/// каждой сменой позиции, поэтому кэш короткоживущий: важен только всплеск,
/// когда один игрок становится виден многим сразу.
const SPAWN_CACHE_LIMIT: usize = 4096;

/// Замков загрузки регионов. Их немного, но каждый читается долго, поэтому
/// шардов заведомо больше, чем потоков генерации: потоки не ждут друг друга
/// на разных регионах.
const REGION_SHARDS: usize = 16;

/// Шард замка для региона `(rx, rz)`.
fn region_shard((rx, rz): (i32, i32)) -> usize {
    ((rx.wrapping_mul(73_856_093) ^ rz.wrapping_mul(19_349_663)) as u32 as usize) % REGION_SHARDS
}

/// Столбец с диска и его координаты.
pub type LoadedChunk = ((i32, i32), ChunkColumn);
/// Чанк `entities/*.mca`: координаты и NBT-корень.
pub type StoredEntities = ((i32, i32), Compound);
thread_local! {
    /// Последний загруженный регион, к которому обращался поток: (мир, регион).
    static LAST_REGION: std::cell::Cell<Option<(usize, (i32, i32))>> =
        const { std::cell::Cell::new(None) };
}

/// Хеш ключей чанков и блоков.
///
/// Координаты и ID сущностей приходят из мира, а не от злоумышленника, поэтому
/// SipHash (`std` по умолчанию) здесь платит за стойкость, которая не нужна:
/// его берут на каждый `block()`, на каждый шаг BFS света и на каждый поиск
/// моба. `FxHasher` — умножение на константу, в разы дешевле на коротких
/// целочисленных ключах, и уже применялся в `ChunkHasher`.
use rustc_hash::{FxHashMap, FxHashSet};

/// Быстрая карта: ключи — целые числа и их кортежи.
type FastMap<K, V> = FxHashMap<K, V>;
/// Быстрое множество: те же ключи.
type FastSet<K> = FxHashSet<K>;

/// Правленые и загруженные столбцы: столбец и номер правки.
type Columns = FastMap<(i32, i32), (ChunkColumn, u64)>;

/// Изменения блоков по секциям: (локальный индекс, состояние).
type PendingBlocks = FastMap<[i32; 3], Vec<(u16, u32)>>;

/// Синхронизация предмета за тик: ID, позиция, скорость, стоит ли на земле.
type ItemSync = (i32, [f64; 3], [f64; 3], bool);

/// Точка интереса: мировая позиция и тип.
///
/// Тип — `&'static str`: у ванили он общий для всех точек одного вида, а скан
/// нового чанка иначе аллоцировал бы строку на каждую найденную точку.
type Poi = ([i32; 3], &'static str);

/// Платформа `the_void` поверх шаблона. `None` — чанк платформу не пересекает.
fn stamp_void(
    template: &ChunkColumn,
    cx: i32,
    cz: i32,
    min_y: i32,
    stone: u32,
    cobble: u32,
    light: &LightTable,
) -> Option<ChunkColumn> {
    if !chunkwake_worldgen::void_platform_chunk(cx, cz) {
        return None;
    }
    let mut column = template.clone();
    let y = (chunkwake_worldgen::void_platform_y(min_y) - min_y) as usize;
    let x0 = cx * 16;
    let z0 = cz * 16;
    for lz in 0..16 {
        for lx in 0..16 {
            let x = x0 + lx;
            let z = z0 + lz;
            let Some(kind) = chunkwake_worldgen::void_platform_block(x, z) else {
                continue;
            };
            let state = match kind {
                chunkwake_worldgen::VoidPlatformBlock::Cobblestone => cobble,
                chunkwake_worldgen::VoidPlatformBlock::Stone => stone,
            };
            column.set_block(lx as usize, y, lz as usize, state);
        }
    }
    chunkwake_world::light::relight_column(&mut column, light);
    Some(column)
}

/// Мир только для чтения: загруженные столбцы, шаблон и правила плоского мира.
///
/// Держится на время фазы тика (см. [`PlayPlan::with_world`]): один
/// `RwLock::read` на тысячи чтений блока вместо замка на каждое.
pub struct WorldView<'a> {
    columns: &'a Columns,
    template: &'a ChunkColumn,
    min_y: i32,
    /// Плоский мир: отсутствующий столбец — это шаблон, а не «не загружено».
    flat: bool,
    void_platform: Option<(u32, u32)>,
}

impl WorldView<'_> {
    /// Небесный свет в клетке (`getBrightness(SKY)`): 0, если чанк не
    /// загружен или у мира нет неба.
    pub fn sky_light_at(&self, [x, y, z]: [i32; 3]) -> Option<u8> {
        let ry = y - self.min_y;
        if ry < 0 || ry as usize >= self.template.height() {
            return None;
        }
        let Some((column, _)) = self.columns.get(&(x >> 4, z >> 4)) else {
            return Some(0);
        };
        if !column.has_skylight {
            return Some(0);
        }
        let section = ry as usize / 16;
        let layer = column.sky_light.get(section)?;
        let i = (ry as usize % 16) * 256 + (z & 15) as usize * 16 + (x & 15) as usize;
        Some(layer.get(i))
    }

    /// Блочный свет в клетке (`getLightFromBlocks`).
    pub fn block_light_at(&self, [x, y, z]: [i32; 3]) -> Option<u8> {
        let ry = y - self.min_y;
        if ry < 0 || ry as usize >= self.template.height() {
            return None;
        }
        let Some((column, _)) = self.columns.get(&(x >> 4, z >> 4)) else {
            return Some(0);
        };
        let layer = column.block_light.get(ry as usize / 16)?;
        let i = (ry as usize % 16) * 256 + (z & 15) as usize * 16 + (x & 15) as usize;
        Some(layer.get(i))
    }

    /// Состояние блока; `y` — мировая координата. `None` — вне мира по
    /// высоте или чанк ещё не загружен (у шума).
    pub fn block(&self, [x, y, z]: [i32; 3]) -> Option<u32> {
        let ry = y - self.min_y;
        if ry < 0 || ry as usize >= self.template.height() {
            return None;
        }
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        if let Some((column, _)) = self.columns.get(&(x >> 4, z >> 4)) {
            return Some(column.block(lx, ry as usize, lz));
        }
        if !self.flat {
            return None;
        }
        Some(PlayPlan::flat_in(
            self.void_platform,
            self.min_y,
            self.template,
            x,
            y,
            z,
            lx,
            ry as usize,
            lz,
        ))
    }

    /// Столбец по координатам чанка, если он загружен.
    pub fn column(&self, cx: i32, cz: i32) -> Option<&ChunkColumn> {
        self.columns.get(&(cx, cz)).map(|e| &e.0)
    }
}

/// Мир для пересчёта света. Плоский мир подставляет шаблон, нормальный —
/// отсутствующий чанк просто не виден, чтобы не засветить рельеф травой.
struct WorldLight<'a> {
    world: &'a mut Columns,
    fallback: Option<&'a ChunkColumn>,
    touched: FastSet<(i32, i32)>,
    /// Последний записанный столбец: BFS пишет в один подряд, без лишних вставок.
    last: Option<(i32, i32)>,
    /// Платформа void: столбец, если чанк не равен плоскому шаблону.
    special: Option<&'a dyn Fn(i32, i32) -> Option<ChunkColumn>>,
}

impl WorldLight<'_> {
    /// Платформа void, если её ещё нет в мире: свет должен видеть камень, не воздух.
    fn ensure_special(&mut self, cx: i32, cz: i32) {
        if self.world.contains_key(&(cx, cz)) {
            return;
        }
        let Some(make) = self.special else {
            return;
        };
        let Some(column) = make(cx, cz) else {
            return;
        };
        self.touched.insert((cx, cz));
        self.last = Some((cx, cz));
        self.world.insert((cx, cz), (column, 0));
    }
}

impl LightAccess for WorldLight<'_> {
    fn column(&mut self, cx: i32, cz: i32) -> Option<&ChunkColumn> {
        self.ensure_special(cx, cz);
        if let Some(e) = self.world.get(&(cx, cz)) {
            return Some(&e.0);
        }
        self.fallback
    }
    fn column_mut(&mut self, cx: i32, cz: i32) -> Option<&mut ChunkColumn> {
        self.ensure_special(cx, cz);
        if !self.world.contains_key(&(cx, cz)) {
            let template = self.fallback?;
            self.touched.insert((cx, cz));
            self.last = Some((cx, cz));
            return Some(
                &mut self
                    .world
                    .entry((cx, cz))
                    .or_insert_with(|| (template.clone(), 0))
                    .0,
            );
        }
        if self.last != Some((cx, cz)) {
            self.touched.insert((cx, cz));
            self.last = Some((cx, cz));
        }
        Some(&mut self.world.get_mut(&(cx, cz)).unwrap().0)
    }
}

/// Записывает свет и сбрасывает кадры. Номер правки растёт у каждого
/// затронутого столбца, включая свой: сжатие кадра идёт без замка.
/// `computed == None` — BFS уже правил `world`.
fn write_light(
    world: &mut Columns,
    frames: &mut FastMap<(i32, i32), Frame>,
    key: (i32, i32),
    computed: Option<HashMap<(i32, i32), ChunkColumn>>,
    touched: FastSet<(i32, i32)>,
) {
    let mut touched = touched;
    touched.insert(key);
    for pos in touched {
        frames.remove(&pos);
        if let Some(map) = computed.as_ref() {
            let Some(column) = map.get(&pos).cloned() else {
                continue;
            };
            if let Some(e) = world.get_mut(&pos) {
                e.0 = column;
                e.1 += 1;
            } else {
                world.insert(pos, (column, 1));
            }
        } else if let Some(e) = world.get_mut(&pos) {
            e.1 += 1;
        }
    }
}

/// Сохранённые игроки (`playerdata`): чтение при входе, запись при выходе.
pub trait PlayerSource: Send + Sync {
    fn load_player(&self, uuid: Uuid) -> Option<PlayerSave>;
    fn schedule_save(&self);
}

/// Источник сохранённых чанков: регион 32×32 читается целиком при первом обращении.
pub trait ChunkSource: Send + Sync {
    fn load_region(&self, rx: i32, rz: i32) -> Result<Vec<LoadedChunk>, String>;
    /// Корни NBT чанков `entities/*.mca` региона.
    fn load_entities(&self, _rx: i32, _rz: i32) -> Result<Vec<StoredEntities>, String> {
        Ok(Vec::new())
    }
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
/// Контейнер для воронок: позиции частей, слоты подряд, печь ли, имя блока.
struct Cont {
    parts: Vec<[i32; 3]>,
    slots: Vec<Slot>,
    furnace: bool,
    name: String,
}

/// Блоки на конечных местах после движения поршня.
type Moves = Vec<([i32; 3], u32, Option<BlockEntity>)>;
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
    gliding: bool,
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
    /// Полёт на элитрах: другим игрокам уходит в метаданных при спавне.
    gliding: bool,
    /// Живой и не наблюдатель: к нему тянутся сферы опыта.
    collects: bool,
}

impl Online {
    fn at(&self) -> EntityRef {
        EntityRef {
            entity_id: self.entity_id,
            uuid: self.entry.uuid,
            pos: self.pos,
            yaw: self.yaw,
            pitch: self.pitch,
            gliding: self.gliding,
        }
    }
}

/// Снимок игрока для сохранения — раз в 5 с.
const SNAPSHOT_TICKS: u32 = 100;

/// Как часто вместо дельты слать абсолютную позицию (у ванилы — раз в 60 тиков).
const SYNC_EVERY: u32 = 60;

/// Глаза стоящего игрока: ноги + 1.62.
fn eye_of(feet: [f64; 3]) -> [f64; 3] {
    [feet[0], feet[1] + 1.62, feet[2]]
}

/// `spawn-protection`: чебышёв от блока спавна строго меньше радиуса.
fn in_spawn(spawn: [f64; 3], radius: i32, pos: [i32; 3]) -> bool {
    if radius <= 0 {
        return false;
    }
    let dx = (pos[0] - spawn[0].floor() as i32).abs();
    let dz = (pos[2] - spawn[2].floor() as i32).abs();
    dx.max(dz) < radius
}

/// Квадрат расстояния от глаз до ближайшей точки блока не больше `reach`².
fn block_reach(eye: [f64; 3], block: [i32; 3], reach: f64) -> bool {
    let mut d2 = 0.0;
    for i in 0..3 {
        let b = block[i] as f64;
        let c = eye[i].clamp(b, b + 1.0);
        let d = eye[i] - c;
        d2 += d * d;
    }
    d2 <= reach * reach
}

/// Полный куб пересекает хитбокс игрока 0.6×1.8.
fn player_hits_cube(feet: [f64; 3], block: [i32; 3]) -> bool {
    let min = [feet[0] - 0.3, feet[1], feet[2] - 0.3];
    let max = [feet[0] + 0.3, feet[1] + 1.8, feet[2] + 0.3];
    (0..3).all(|i| {
        let b = block[i] as f64;
        min[i] < b + 1.0 && max[i] > b
    })
}

/// Доля урона от падения, которая остаётся на блоке приземления.
fn fall_kept(name: &str) -> f32 {
    if name == "minecraft:slime_block" || name.ends_with("_bed") {
        0.0
    } else if name == "minecraft:hay_block" {
        0.2
    } else {
        1.0
    }
}

/// Видна ли сущность из чанка `center` при дальности `r` чанков.
fn near(center: (i32, i32), pos: [f64; 3], r: i32) -> bool {
    let (x, z) = ((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
    (x - center.0).abs() <= r && (z - center.1).abs() <= r
}

/// Чанк, в котором лежит позиция.
fn chunk_of(pos: [f64; 3]) -> (i32, i32) {
    ((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4)
}

/// Квадрат чанков радиуса `r` вокруг `center` — как `chunks_around`, но без порядка.
fn chunk_square(center: (i32, i32), r: i32, out: &mut FastSet<(i32, i32)>) {
    for x in (center.0 - r)..=(center.0 + r) {
        for z in (center.1 - r)..=(center.1 + r) {
            out.insert((x, z));
        }
    }
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
        // Ёмкость сразу под сжатый размер: `shrink_to_fit` после сжатия —
        // это realloc + memcpy на каждую рассылку, а рассылок десятки за тик.
        //
        // Запас — доля от размера пакета: обычный zlib даёт около половины,
        // но плотные данные (свет, шум) сжимаются хуже, и на чанке запас
        // `len/2` приводил к реаллокации ровно на самом дорогом пакете.
        let mut out = Vec::with_capacity(packet.len() * 3 / 4 + 64);
        self.codec.encode(packet, &mut out);
        out
    }

    fn send(&self, skip: Option<Uuid>, packet: Vec<u8>) {
        self.send_entity(skip, packet, None);
    }

    /// Уже сжатый кадр всем: соседний чанк после пролива света.
    fn send_ready(&self, frame: Frame) {
        let _ = self.tx.send(Broadcast {
            skip: None,
            only: None,
            packet: frame,
            entity: None,
            moves: None,
            blocks: None,
        });
    }

    /// Много пакетов всем — одной рассылкой: кадры подряд, одно пробуждение
    /// задачи игрока и одна запись в сокет вместо пакета на событие.
    fn send_many(&self, packets: impl IntoIterator<Item = Vec<u8>>) {
        // Один проход без сбора пакетов в промежуточный `Vec`: на стаде это
        // тысяча лишних перемещений. Ёмкость оценивается по `size_hint`,
        // а если подсказки нет — по первому пакету.
        let mut it = packets.into_iter();
        let (low, high) = it.size_hint();
        let hint = high.unwrap_or(low);
        let mut out = Vec::with_capacity(hint.saturating_mul(24).max(256));
        let mut any = false;
        for p in &mut it {
            self.codec.encode(&p, &mut out);
            any = true;
        }
        if !any {
            return;
        }
        let _ = self.tx.send(Broadcast {
            skip: None,
            only: None,
            packet: Arc::new(out),
            entity: None,
            moves: None,
            blocks: None,
        });
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

/// Теги точек интереса из каталога данных. Нет каталога — пустая карта.
pub fn load_poi_tags(data_dir: &std::path::Path) -> HashMap<String, HashSet<String>> {
    poi::load_tags(data_dir)
}

/// Теги `data/minecraft/tags/<folder>`, вместе с вложенными папками и `#тегами`.
pub fn load_registry_tags(
    data_dir: &std::path::Path,
    folder: &str,
) -> HashMap<String, HashSet<String>> {
    let dir = data_dir.join("data/minecraft/tags").join(folder);
    let mut raw: HashMap<String, Vec<String>> = HashMap::new();
    collect_tag_files(&dir, "", &mut raw);
    let mut out = HashMap::new();
    for name in raw.keys() {
        let mut set = HashSet::new();
        let mut seen = HashSet::new();
        expand_tag(&raw, name, &mut set, &mut seen);
        out.insert(name.clone(), set);
    }
    out
}

fn collect_tag_files(dir: &std::path::Path, prefix: &str, raw: &mut HashMap<String, Vec<String>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name_os = entry.file_name();
            let Some(name) = name_os.to_str() else {
                continue;
            };
            let next = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            collect_tag_files(&path, &next, raw);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let id = if prefix.is_empty() {
            format!("minecraft:{stem}")
        } else {
            format!("minecraft:{prefix}/{stem}")
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let values = value["values"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                entry
                    .as_str()
                    .or_else(|| entry.get("id").and_then(|id| id.as_str()))
                    .map(str::to_owned)
            })
            .collect();
        raw.insert(id, values);
    }
}

fn expand_tag(
    raw: &HashMap<String, Vec<String>>,
    name: &str,
    set: &mut HashSet<String>,
    seen: &mut HashSet<String>,
) {
    if !seen.insert(name.to_owned()) {
        return;
    }
    let Some(values) = raw.get(name) else {
        return;
    };
    for value in values {
        if let Some(tag) = value.strip_prefix('#') {
            let tag = if tag.contains(':') {
                tag.to_owned()
            } else {
                format!("minecraft:{tag}")
            };
            expand_tag(raw, &tag, set, seen);
        } else if value.contains(':') {
            set.insert(value.clone());
        } else if !value.is_empty() {
            set.insert(format!("minecraft:{value}"));
        }
    }
}

/// Имена `worldgen/structure` и их `#теги` для подсказок `/locate structure`.
pub fn structure_names(data_dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    collect_json_ids(
        &data_dir.join("data/minecraft/worldgen/structure"),
        "",
        false,
        &mut out,
    );
    collect_json_ids(
        &data_dir.join("data/minecraft/tags/worldgen/structure"),
        "",
        true,
        &mut out,
    );
    out.sort();
    out.dedup();
    out
}

fn collect_json_ids(dir: &std::path::Path, prefix: &str, tag: bool, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let next = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            collect_json_ids(&path, &next, tag, out);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let id = if prefix.is_empty() {
            stem.to_owned()
        } else {
            format!("{prefix}/{stem}")
        };
        if tag {
            out.push(format!("#minecraft:{id}"));
        } else {
            out.push(format!("minecraft:{id}"));
        }
    }
}

/// Мир и данные, нужные сессиям.
pub struct WorldSpec {
    /// Чанк генератора: копируется при первой правке столбца.
    pub template: ChunkColumn,
    /// Нормальный мир: сгенерировать недостающий чанк. Плоский — `None`.
    ///
    /// `None` от замыкания — чанк уже отдан раньше: генератор его не
    /// повторяет, и вставлять в мир нечего.
    pub terrain: Option<TerrainFn>,
    /// `/locate biome`. `None` — плоский мир или климат не загружен.
    pub locate_biome: Option<LocateFn>,
    /// `/locate structure`. `None` — плоский мир или генератор без структур.
    pub locate_structure: Option<LocateFn>,
    /// Теги `point_of_interest_type` из датапака.
    pub poi_tags: HashMap<String, HashSet<String>>,
    /// Id и `#теги` структур для `/locate structure`.
    pub structure_names: Vec<String>,
    /// Плоский `the_void`: камень и булыжник платформы. `None` — шаблон как есть.
    pub void_platform: Option<(u32, u32)>,
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

/// Общие флаги сущности: присед, бег, элитры.
fn player_flags(sneaking: bool, sprinting: bool, gliding: bool) -> u8 {
    u8::from(sneaking) << 1 | u8::from(sprinting) << 3 | u8::from(gliding) << 7
}

fn permission_event(id: i32, entity: i32, level: i32) -> Vec<u8> {
    inv::entity_event(
        id,
        entity,
        inv::ENTITY_EVENT_PERMISSION_LEVEL_0 + level.clamp(0, 4) as u8,
    )
}

/// Открытое окно с блоком мира (сундук, верстак): его слоты без инвентаря игрока.
/// Инвентарь плагина: слоты и тип меню клиента.
struct PluginInv {
    slots: Vec<Slot>,
    menu: i32,
}

/// Окно плагина хранится как сундук с условной позицией вне мира.
fn plugin_pos(id: i32) -> [i32; 3] {
    [i32::MIN, i32::MIN, id]
}

fn plugin_of(pos: [i32; 3]) -> Option<i32> {
    (pos[0] == i32::MIN && pos[1] == i32::MIN).then_some(pos[2])
}

struct Window {
    id: i32,
    kind: Kind,
    pos: [i32; 3],
    /// Вторая половина двойного сундука.
    second: Option<[i32; 3]>,
    slots: Vec<Slot>,
    /// Последние отправленные свойства окна (печь).
    data: [i32; 4],
    /// Последние отправленные свойства стола зачарования.
    enchant: [i32; 10],
    /// Варианты стола (после `PrepareItemEnchantEvent`) и для чего они: предмет, полки, зерно.
    offer: Option<((Slot, i32, i32), crate::enchanting::Offer)>,
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
        if self.kind == CHEST_LEFT {
            (self.facing + 1) % 4
        } else {
            (self.facing + 3) % 4
        }
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
/// Бит `flying` в `ServerboundPlayerAbilitiesPacket`.
const ABILITY_FLYING: u8 = 0x02;

/// Блок-сущность контейнера по имени блока.
/// `SUN_ANGLE` из таймлайна `OVERWORLD_DAY`, в градусах.
///
/// Ключевые кадры — `6000 → 360°` и `6000 → 0°` на периоде 24000: в полдень
/// угол 0, к следующему полудню 360. Easing —
/// `symmetricCubicBezier(0.362, 0.241)`, то есть `cubicBezier(0.362, 0.241,
/// 0.638, 0.759)`.
fn sun_angle(day: f32) -> f32 {
    // Позиция внутри периода, начиная с полудня.
    let t = (day - 6000.0).rem_euclid(24000.0) / 24000.0;
    360.0 * cubic_bezier(0.362, 0.241, 0.241, 0.241, t)
}

/// `EasingType.CubicBezier.apply`: 4 итерации Ньютона, затем бисекция.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    // `curveFromControls`: a = 3v1 - 3v2 + 1, b = -6v1 + 3v2, c = 3v1.
    let (ax, bx, cx) = (3.0 * x1 - 3.0 * x2 + 1.0, -6.0 * x1 + 3.0 * x2, 3.0 * x1);
    let (ay, by, cy) = (3.0 * y1 - 3.0 * y2 + 1.0, -6.0 * y1 + 3.0 * y2, 3.0 * y1);
    let sample = |a: f32, b: f32, c: f32, t: f32| ((a * t + b) * t + c) * t;
    let gradient = |a: f32, b: f32, c: f32, t: f32| (3.0 * a * t + 2.0 * b) * t + c;

    let mut t = x;
    for _ in 0..4 {
        let error = sample(ax, bx, cx, t) - x;
        if error.abs() < 1.0e-5 {
            return sample(ay, by, cy, t);
        }
        let g = gradient(ax, bx, cx, t);
        if g < 1.0e-5 {
            break;
        }
        t -= (error / g).clamp(-0.25, 0.25);
    }
    // `solveTBisect`: половиним отрезок, пока ошибка не станет меньше 1e-5.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    while lo < hi {
        let error = sample(ax, bx, cx, t) - x;
        if error.abs() < 1.0e-5 {
            break;
        }
        if error < 0.0 {
            lo = t;
        } else {
            hi = t;
        }
        t = (hi + lo) / 2.0;
    }
    sample(ay, by, cy, t)
}

/// Ось, вдоль которой летел снаряд: 0 — X, 1 — Y, 2 — Z. По ней считается
/// сторона попадания в мишень.
fn dominant_axis(vel: [f64; 3]) -> usize {
    let (x, y, z) = (vel[0].abs(), vel[1].abs(), vel[2].abs());
    if y > x && y > z {
        1
    } else if z > x {
        2
    } else {
        0
    }
}

fn container_entity(block: &str) -> Option<&'static str> {
    match block {
        "minecraft:chest" => Some("minecraft:chest"),
        "minecraft:trapped_chest" => Some("minecraft:trapped_chest"),
        "minecraft:barrel" => Some("minecraft:barrel"),
        "minecraft:hopper" => Some("minecraft:hopper"),
        "minecraft:dispenser" => Some("minecraft:dispenser"),
        "minecraft:dropper" => Some("minecraft:dropper"),
        "minecraft:furnace" => Some("minecraft:furnace"),
        "minecraft:blast_furnace" => Some("minecraft:blast_furnace"),
        "minecraft:smoker" => Some("minecraft:smoker"),
        _ => None,
    }
}

/// Слотов в контейнере: воронка 5, раздатчик и выбрасыватель 9, сундук и бочка 27.
fn container_size(block: &str) -> usize {
    match block {
        "minecraft:hopper" => 5,
        "minecraft:dispenser" | "minecraft:dropper" => 9,
        _ => 27,
    }
}

/// Индекс печи в `FURNACES` по имени блока.
fn furnace_kind(block: &str) -> Option<u8> {
    FURNACES.iter().position(|f| *f == block).map(|i| i as u8)
}

use chunkwake_data::world::FURNACES;

/// Ванильные команды: дерево, права, селекторы и обработчики.
pub mod commands;
mod mobs;
mod poi;
mod stream;

pub use poi::group_of as poi_group;

/// Заселение мира для бенчмарков и тестов: мобы и предметы без сокетов.
#[cfg(feature = "bench")]
pub mod bench {
    use super::*;

    /// Ставит `count` коров вокруг точки `center` и добавляет `items` предметов.
    /// Возвращает число действительно созданных мобов.
    pub fn populate(plan: &PlayPlan, center: [f64; 3], count: usize, items: usize) -> usize {
        let mut made = 0;
        for i in 0..count {
            let a = i as f64 * 0.7;
            let r = 8.0 + (i % 7) as f64;
            let pos = [center[0] + r * a.cos(), center[1], center[2] + r * a.sin()];
            if plan
                .spawn_mob(crate::mobs::Species::Cow, pos, 0.0, false, Some("BENCH"))
                .is_some()
            {
                made += 1;
            }
        }
        for i in 0..items {
            let a = i as f64 * 0.9;
            let r = 4.0 + (i % 5) as f64;
            let pos = [center[0] + r * a.cos(), center[1], center[2] + r * a.sin()];
            plan.drop_item(
                pos,
                [0.0, 0.0, 0.0],
                Stack::new(1, 1),
                std::time::Duration::ZERO,
            );
        }
        made
    }

    /// Ставит `side × side` блоков с именем `block` вокруг `origin` (мировые
    /// координаты). Меряет путь записи блока целиком: `set_block`, свет,
    /// индексы, очередь рассылки.
    pub fn place(plan: &PlayPlan, origin: [i32; 3], side: i32, block: &str) -> usize {
        let Some(state) = plan.survival.block_defaults.get(block).copied() else {
            return 0;
        };
        let mut placed = 0;
        for dx in 0..side {
            for dz in 0..side {
                let pos = [origin[0] + dx, origin[1], origin[2] + dz];
                if plan.set_block(pos, state).is_some() {
                    placed += 1;
                }
            }
        }
        placed
    }

    /// Состояние блока по умолчанию (`minecraft:stone` → ID состояния).
    pub fn state(plan: &PlayPlan, block: &str) -> Option<u32> {
        plan.survival.block_defaults.get(block).copied()
    }

    /// Записывает один блок: полный путь `set_block` (свет, индексы, рассылка).
    pub fn write(plan: &PlayPlan, pos: [i32; 3], state: u32) -> Option<u32> {
        plan.set_block(pos, state)
    }

    /// Сумма состояний `count` блоков подряд от `origin`: цена одного чтения
    /// блока из мира (замок, поиск столбца, палитра секции).
    pub fn read(plan: &PlayPlan, origin: [i32; 3], count: i32) -> u32 {
        let mut sum = 0u32;
        for dx in 0..count {
            let pos = [origin[0] + dx, origin[1], origin[2]];
            sum = sum.wrapping_add(plan.block(pos).unwrap_or(0));
        }
        sum
    }
}

/// Рецепт костра, скопированный на время события.
struct CampOffer {
    id: String,
    experience: f32,
    time: i32,
    result_item: i32,
    result_count: i32,
}

/// Печь в памяти: состояние и кто смотрит в её окно.
struct FurnaceEntry {
    f: Furnace,
    viewers: Vec<Uuid>,
}

/// `Direction` по номеру (как `BlockFace` Bukkit).
const FACES: [&str; 6] = ["DOWN", "UP", "NORTH", "SOUTH", "WEST", "EAST"];

/// Высота глаз стоящего игрока (`Player.getStandingEyeHeight`).
const EYE_HEIGHT: f64 = 1.62;

/// Что сделал `use_item` с луком или арбалетом.
enum RangedUse {
    Ignore,
    /// Начато натяжение: слот оружия.
    Draw(usize),
    /// Выстрел уже произведён (заряженный арбалет).
    Done,
}

struct OwnedShot {
    uuid: Uuid,
    eye: [f64; 3],
    yaw: f32,
    pitch: f32,
    yaw_offset: f64,
    speed: f64,
    base: f64,
    crit: bool,
    punch: i32,
    pierce: i32,
    rocket: bool,
    /// Стрела не подбирается (Бесконечность, креатив, `setConsumeItem(false)`).
    creative_only: bool,
}

fn charged_extra(extra: u32, charged: Vec<Stack>) -> u32 {
    let mut e = (*chunkwake_protocol::components::get(extra)).clone();
    e.charged = charged;
    e.raw = None;
    chunkwake_protocol::components::intern(e)
}

/// Режимы игры по номерам (`GameType`).
const GAME_MODES: [&str; 4] = ["survival", "creative", "adventure", "spectator"];
/// Предел `/give` за раз.
const MAX_GIVE: i32 = 6400;

pub struct SurvivalData {
    /// Стол зачарования: чары, предметы, полки.
    pub enchanting: chunkwake_data::enchanting::Enchanting,
    pub menu_enchantment: i32,
    pub menu_grindstone: i32,
    /// Чары для плагинов по порядку сетевых ID (строки запроса `enchantments`).
    pub enchantments: Vec<String>,
    /// Твёрдость по состоянию блока (-1 — неразрушимый, 0 — ломается сразу).
    pub hardness: Vec<f32>,
    /// Рецепты верстака и сетки 2×2.
    pub recipes: chunkwake_data::recipes::Recipes,
    /// Типы меню (`menu`): сундук 9×3, двойной 9×6 и верстак.
    pub menu_chest: i32,
    pub menu_double_chest: i32,
    /// Меню воронки и раздатчика/выбрасывателя (`generic_3x3`).
    pub menu_hopper: i32,
    pub menu_3x3: i32,
    /// `generic_9x1` … `generic_9x6` для окон плагинов.
    pub menu_rows: [i32; 6],
    /// Состояния сундуков (`ChestBlock`): свойства по состоянию и обратно.
    pub chests: HashMap<u32, ChestState>,
    pub chest_states: HashMap<ChestState, u32>,
    pub menu_crafting: i32,
    /// Меню печей по порядку `FURNACES`.
    pub menu_furnace: [i32; 3],
    /// Состояние печи → то же с переключённым `lit`.
    pub furnace_lit: HashMap<u32, u32>,
    /// Эффекты по имени (`minecraft:haste`) в реестре `mob_effect`.
    pub effects: HashMap<String, i32>,
    /// Сетевые ID атрибутов (`attribute`).
    pub attributes: HashMap<String, i32>,
    /// Сетевые ID звуков (`sound_event`).
    pub sounds: HashMap<String, i32>,
    /// ID `minecraft:movement_speed` в реестре `attribute`.
    pub attribute_speed: i32,
    /// ID `minecraft:jump_strength` и `minecraft:gravity`.
    pub attribute_jump: i32,
    pub attribute_gravity: i32,
    /// Ориентация ставимых блоков.
    pub placement: crate::placement::Placement,
    /// Инструмент нотного блока по состоянию (`reports/instruments.csv`).
    pub instruments: chunkwake_data::world::Instruments,
    /// Нужен ли правильный инструмент для дропа, по состоянию блока.
    pub requires_tool: Vec<bool>,
    /// Компонент `minecraft:tool` по ID предмета.
    pub tools: Vec<Option<chunkwake_data::tools::Tool>>,
    /// Имя блока по состоянию (для правил инструментов).
    pub block_names: Arc<Vec<String>>,
    /// Состояние блока по умолчанию по имени (`Block.setType` у плагинов).
    pub block_defaults: HashMap<String, u32>,
    /// ID типа блока в реестре `block` (для `block_event`, не состояние).
    pub block_types: HashMap<String, i32>,
    /// Предмет, выпадающий из состояния блока.
    pub loot: Arc<chunkwake_data::world::LootSource>,
    /// Свойства предметов по ID: размер стопки, еда.
    pub items: Vec<chunkwake_data::world::ItemInfo>,
    /// ID `minecraft:fall` и `minecraft:out_of_world` в реестре `damage_type`.
    pub damage_fall: i32,
    pub damage_void: i32,
    pub damage_starve: i32,
    pub damage_magic: i32,
    pub damage_wither: i32,
    pub damage_arrow: i32,
    pub arrow_entity_type: i32,
    pub firework_entity_type: i32,
    pub snowball_entity_type: i32,
    pub egg_entity_type: i32,
    /// Текущая сложность: `/difficulty` меняет её, пока сервер запущен.
    pub difficulty: AtomicU8,
    /// ID предмета по имени (`minecraft:bread`) для `/give`.
    pub item_ids: HashMap<String, i32>,
    /// Имя зелья по сетевому ID реестра `potion` (`minecraft:swiftness`).
    pub potions: Vec<String>,
    /// Сетевой ID компонента `minecraft:potion_contents`.
    pub potion_component: i32,
    /// ID `minecraft:item` в реестре `entity_type`.
    pub item_entity_type: i32,
    pub orb_entity_type: i32,
    /// ID корова, свинья, овца, курица (`mobs::Species`) в реестре `entity_type`.
    pub mob_entity_types: [i32; 4],
    /// Категория `creature` биомов по сетевому ID биома.
    pub creature_spawns: Vec<CreatureSpawns>,
    /// Коллизия и трение блоков; `None` — старый кэш без `shapes.csv`.
    pub physics: Option<Arc<chunkwake_data::world::BlockPhysics>>,
    /// Тег предмета (`minecraft:dirt`) → id предметов.
    pub item_tags: HashMap<String, HashSet<String>>,
    /// Тег блока → id блоков. `/fill … replace #тег`.
    pub block_tags: HashMap<String, HashSet<String>>,
    /// Имя биома по сетевому ID (`worldgen/biome`).
    pub biome_names: Vec<String>,
    /// Тег биома (`minecraft:is_forest`) → имена биомов.
    pub biome_tags: HashMap<String, HashSet<String>>,
    /// Каталог данных: `/place template` читает из него `.nbt`.
    pub data_dir: std::path::PathBuf,
    /// Состояния блоков (`reports/blocks.json`): палитра шаблонов.
    pub block_states: Arc<Vec<Option<(String, chunkwake_data::world::Properties)>>>,
}

/// `natural_mob_spawns.creature` и `creature_world_gen_spawn_probability` биома.
#[derive(Debug, Clone, Default)]
pub struct CreatureSpawns {
    pub chance: f32,
    /// Вид, вес, размер группы от и до.
    pub entries: Vec<(String, u32, u32, u32)>,
}

/// Красный ответ и ключи провала: их пишем в консоль отдельно от успеха.
fn command_error(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.get("color").and_then(|color| color.as_str()) == Some("red") {
        return true;
    }
    let key = object
        .get("translate")
        .and_then(|key| key.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    key.contains("failed")
        || key.contains("unknown")
        || key.contains("notfound")
        || key.contains("not_found")
        || key.contains("invalid")
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
            terrain,
            locate_biome,
            locate_structure,
            poi_tags,
            structure_names,
            void_platform,
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
        let effect_kinds = survival::EffectKinds {
            regeneration: survival.effects.get("minecraft:regeneration").copied(),
            poison: survival.effects.get("minecraft:poison").copied(),
            wither: survival.effects.get("minecraft:wither").copied(),
            hunger: survival.effects.get("minecraft:hunger").copied(),
        };
        let sponge = survival
            .item_ids
            .get("minecraft:wet_sponge")
            .copied()
            .zip(survival.item_ids.get("minecraft:bucket").copied())
            .zip(survival.item_ids.get("minecraft:water_bucket").copied())
            .map(|((s, b), w)| (s, b, w));
        // Режим игры по умолчанию до `/defaultgamemode`.
        let login_game_mode = login.spawn.game_mode;
        Ok(Self {
            ids: Ids {
                login: cb("login")?,
                game_event: cb("game_event")?,
                set_chunk_cache_center: cb("set_chunk_cache_center")?,
                set_default_spawn_position: cb("set_default_spawn_position")?,
                initialize_border: cb("initialize_border")?,
                set_border_center: cb("set_border_center")?,
                set_border_lerp_size: cb("set_border_lerp_size")?,
                set_border_size: cb("set_border_size")?,
                set_border_warning_delay: cb("set_border_warning_delay")?,
                set_border_warning_distance: cb("set_border_warning_distance")?,
                set_camera: cb("set_camera")?,
                swing_animation: cb("swing_animation")?,
                set_passengers: cb("set_passengers")?,
                boss_event: cb("boss_event")?,
                post_effects: cb("post_effects")?,
                chunks_biomes: cb("chunks_biomes")?,
                waypoint: cb("waypoint")?,
                clear_dialog: cb("clear_dialog")?,
                level_particles: cb("level_particles")?,
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
                block_entity_data: cb("block_entity_data")?,
                block_destruction: cb("block_destruction")?,
                section_blocks_update: cb("section_blocks_update")?,
                block_changed_ack: cb("block_changed_ack")?,
                sb_player_action: sb("player_action")?,
                sb_use_item_on: sb("use_item_on")?,
                sb_set_creative_mode_slot: sb("set_creative_mode_slot")?,
                sb_pick_item_from_block: sb("pick_item_from_block")?,
                sb_set_carried_item: sb("set_carried_item")?,
                container_set_content: cb("container_set_content")?,
                container_set_slot: cb("container_set_slot")?,
                container_set_data: cb("container_set_data")?,
                set_experience: cb("set_experience")?,
                sb_container_button_click: sb("container_button_click")?,
                set_health: cb("set_health")?,
                respawn: cb("respawn")?,
                damage_event: cb("damage_event")?,
                entity_event: cb("entity_event")?,
                sb_container_click: sb("container_click")?,
                sb_attack: sb("attack")?,
                sb_interact: sb("interact")?,
                hurt_animation: cb("hurt_animation")?,
                sb_container_close: sb("container_close")?,
                sb_player_input: sb("player_input")?,
                sb_player_abilities: sb("player_abilities")?,
                open_screen: cb("open_screen")?,
                container_close: cb("container_close")?,
                sb_client_command: sb("client_command")?,
                sb_player_command: sb("player_command")?,
                sb_use_item: sb("use_item")?,
                commands: cb("commands")?,
                block_event: cb("block_event")?,
                sound: cb("sound")?,
                level_event: cb("level_event")?,
                set_time: cb("set_time")?,
                change_difficulty: cb("change_difficulty")?,
                update_mob_effect: cb("update_mob_effect")?,
                update_attributes: cb("update_attributes")?,
                remove_mob_effect: cb("remove_mob_effect")?,
                sb_chat_command: sb("chat_command")?,
                sb_chat_command_signed: sb("chat_command_signed")?,
                set_title_text: cb("set_title_text")?,
                set_subtitle_text: cb("set_subtitle_text")?,
                set_action_bar_text: cb("set_action_bar_text")?,
                set_titles_animation: cb("set_titles_animation")?,
                clear_titles: cb("clear_titles")?,
                stop_sound: cb("stop_sound")?,
                set_objective: cb("set_objective")?,
                set_score: cb("set_score")?,
                reset_score: cb("reset_score")?,
                set_display_objective: cb("set_display_objective")?,
                transfer: cb("transfer")?,
            },
            login,
            chunk_body: template.encode_body(),
            template,
            void_platform,
            terrain,
            locate_biome,
            locate_structure,
            poi_tags,
            structure_names,
            pois: Mutex::new(FastMap::default()),
            edited: std::sync::RwLock::new(Columns::default()),
            saved: Mutex::new(FastMap::default()),
            dirty: Mutex::default(),
            pending_blocks: Mutex::new(FastMap::default()),
            world_edit: Mutex::new(()),
            light_lock: Mutex::new(()),
            light,
            source: std::sync::OnceLock::new(),
            regions: Mutex::new(FastSet::default()),
            region_load: (0..REGION_SHARDS).map(|_| Mutex::new(())).collect(),
            gen_pool: std::sync::OnceLock::new(),
            self_weak: std::sync::OnceLock::new(),
            min_y,
            item_blocks,
            spawn,
            spawn_protection: std::sync::atomic::AtomicI32::new(0),
            player_type,
            next_entity_id: AtomicI32::new(1),
            chunk_frames: Mutex::new(FastMap::default()),
            spawn_frames: Mutex::new(FastMap::default()),
            remove_frames: Mutex::new(FastMap::default()),
            block_names,
            parsers,
            survival,
            hooks: std::sync::OnceLock::new(),
            plugin_commands: Mutex::new(Vec::new()),
            furnaces: Mutex::new(FastMap::default()),
            items_syncs: Mutex::new(Vec::new()),
            items_gone: Mutex::new(Vec::new()),
            items_merging: Mutex::new(Vec::new()),
            redstone_ticks: Mutex::default(),
            arrows: Mutex::new(FastMap::default()),
            plugin_invs: Mutex::new(FastMap::default()),
            next_plugin_inv: std::sync::atomic::AtomicI32::new(1),
            fluid_ticks: Mutex::default(),
            torch_toggles: Mutex::new(FastMap::default()),
            torch_cool: Mutex::new(FastMap::default()),
            plates: Mutex::new(FastMap::default()),
            comparators: Mutex::new(FastMap::default()),
            piston_moves: Mutex::new(Vec::new()),
            deferred_updates: Mutex::new(Vec::new()),
            hopper_ticks: Mutex::new(crate::ticks::TickQueue::default()),
            server_ticks: std::sync::atomic::AtomicU64::new(0),
            pending_furnaces: Mutex::new(Vec::new()),
            campfires: Mutex::new(FastMap::default()),
            pending_campfires: Mutex::new(Vec::new()),
            dropped: Mutex::new(FastMap::default()),
            dropped_by_chunk: Mutex::new(FastMap::default()),
            orbs: Mutex::new(FastMap::default()),
            players_source: std::sync::OnceLock::new(),
            player_saves: Mutex::new(FastMap::default()),
            ops: std::sync::OnceLock::new(),
            item_names,
            live: Mutex::new(FastMap::default()),
            state_cache: Mutex::new(FastMap::default()),
            mobs: Mutex::new(FastMap::default()),
            populated: Mutex::new(FastSet::default()),
            mobs_loaded: Mutex::new(Vec::new()),
            entity_written: Mutex::new(FastSet::default()),
            entity_new: Mutex::new(FastSet::default()),
            controls: Mutex::new(FastMap::default()),
            shutting_down: AtomicBool::new(false),
            open_blocks: Mutex::new(FastMap::default()),
            simulated: Mutex::new(Arc::new(FastSet::default())),
            simulated_key: Mutex::new(None),
            day_time: AtomicI64::new(0),
            game_time: AtomicI64::new(0),
            ticks: Mutex::new(chunkwake_metrics::TickStats::new()),
            vars: crate::play::commands::state::ServerVars::ephemeral(),
            net_stats: crate::play::commands::net::NetStats::default(),
            pings: Mutex::new(FastMap::default()),
            proc_stats: Mutex::new(chunkwake_metrics::ProcStats::default()),
            debug_profile: Mutex::new(None),
            work_pending: AtomicBool::new(false),
            profiling: AtomicBool::new(false),
            profile_sink: Mutex::new(None),
            debug_dir: std::path::PathBuf::from("debug"),
            shutdown: AtomicBool::new(false),
            seed: 0,
            motd: String::from("A Chunkwake Server"),
            autosave_ms: std::sync::atomic::AtomicU32::new(0),
            started: Instant::now(),
            compression_threshold,
            spawn_points: Mutex::new(HashMap::new()),
            default_game_mode: std::sync::atomic::AtomicU8::new(login_game_mode),
            save_requested: AtomicBool::new(false),
            whitelist_enabled: AtomicBool::new(false),
            allow_flight: AtomicBool::new(false),
            reload_dir: None,
            scoreboard: Mutex::new(crate::play::commands::scoreboard::Scoreboard::default()),
            teams: Mutex::new(crate::play::commands::team::Board::default()),
            entity_extra: Mutex::new(HashMap::new()),
            entity_tags: Mutex::new(HashMap::new()),
            random_seq: Mutex::new(HashMap::new()),
            boss_bars: Mutex::new(HashMap::new()),
            stopwatches: Mutex::new(HashMap::new()),
            post_effects: Mutex::new(HashMap::new()),
            entity_attributes: Mutex::new(HashMap::new()),
            waypoints: Mutex::new(HashMap::new()),
            particle_types: Mutex::new(HashMap::new()),
            command_lang: Mutex::new(HashMap::new()),
            links: Mutex::new(HashMap::new()),
            level_name: String::new(),
            version_name: String::new(),
            version_protocol: 0,
            forced: Mutex::new(FastSet::default()),
            effect_kinds,
            sponge,
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
        // Рельеф уже вставлен волной. Мобы — после него, не на плоском шаблоне.
        self.ensure_region(x, z);
        self.populate_chunk(x, z);
        // Платформа void не в общем шаблоне: после диска, чтобы сохранённый чанк победил.
        self.materialize_void(x, z);
        // Под блокировкой — только дешёвый клон (секции за Arc);
        // кодирование и сжатие — вне её.
        let snapshot = self.edited.read().unwrap().get(&(x, z)).cloned();
        let edited = snapshot.map(|(c, rev)| (c.encode_body(), rev));
        let (body, rev) = match &edited {
            Some((body, rev)) => (body.as_slice(), *rev),
            None => (self.chunk_body.as_slice(), 0),
        };
        let packet = play::level_chunk_with_light(self.ids.level_chunk_with_light, x, z, body);
        let frame = Arc::new(self.hub.frame(&packet));
        // В кэш — только если чанк не изменили, пока мы его сжимали.
        let world = self.edited.read().unwrap();
        if world.get(&(x, z)).map_or(0, |e| e.1) == rev {
            let mut cache = self.chunk_frames.lock().unwrap();
            if cache.len() >= CHUNK_CACHE_LIMIT {
                // Восьмая часть в порядке хеша (по сути случайная): без обрыва,
                // когда весь кэш разом пересжимается для всех игроков.
                let old: Vec<_> = cache.keys().take(CHUNK_CACHE_LIMIT / 8).copied().collect();
                for k in old {
                    cache.remove(&k);
                }
            }
            cache.insert((x, z), frame.clone());
        }
        frame
    }

    /// Столбец платформы void, если этот чанк её пересекает.
    fn void_column(&self, cx: i32, cz: i32) -> Option<ChunkColumn> {
        let (stone, cobble) = self.void_platform?;
        stamp_void(
            &self.template,
            cx,
            cz,
            self.min_y,
            stone,
            cobble,
            &self.light,
        )
    }

    /// После чтения региона: платформа, если диск её не принёс.
    fn materialize_void(&self, cx: i32, cz: i32) {
        if self.void_platform.is_none() || !chunkwake_worldgen::void_platform_chunk(cx, cz) {
            return;
        }
        if self.edited.read().unwrap().contains_key(&(cx, cz)) {
            return;
        }
        let Some(column) = self.void_column(cx, cz) else {
            return;
        };
        let list = self.collect_pois(cx, cz, &column);
        self.edited
            .write()
            .unwrap()
            .entry((cx, cz))
            .or_insert((column, 0));
        self.store_pois(cx, cz, list);
    }

    /// Блок плоского мира, которого ещё нет в `edited`: шаблон или платформа.
    /// Живёт только в [`WorldView`]: прямой путь через `self` больше не нужен
    /// и брал бы замок мира.
    #[allow(clippy::too_many_arguments)]
    fn flat_in(
        void_platform: Option<(u32, u32)>,
        min_y: i32,
        template: &ChunkColumn,
        x: i32,
        y: i32,
        z: i32,
        lx: usize,
        ry: usize,
        lz: usize,
    ) -> u32 {
        if let Some((stone, cobble)) = void_platform
            && y == chunkwake_worldgen::void_platform_y(min_y)
            && let Some(kind) = chunkwake_worldgen::void_platform_block(x, z)
        {
            return match kind {
                chunkwake_worldgen::VoidPlatformBlock::Cobblestone => cobble,
                chunkwake_worldgen::VoidPlatformBlock::Stone => stone,
            };
        }
        template.block(lx, ry, lz)
    }

    /// Точки интереса столбца: обход один раз при установке чанка.
    ///
    /// Имя и тип точки берутся только у найденного блока: у остальных
    /// проверка — один `&` по таблице флагов состояния. Однородная секция
    /// (воздух, камень) пропускается целиком: в обычном столбце это почти
    /// весь объём, а типовых точек в нём нет.
    fn collect_pois(&self, cx: i32, cz: i32, column: &ChunkColumn) -> Vec<Poi> {
        let mut found = Vec::new();
        for (si, section) in column.sections.iter().enumerate() {
            if section.is_uniform() {
                continue;
            }
            let mut i = 0usize;
            section.for_each(|state| {
                let index = i;
                i += 1;
                if chunkwake_world::poi_kind_bits(state) == 0 {
                    return;
                }
                let Some(kind) = poi::of_block(self.state_name(state)) else {
                    return;
                };
                let x = index & 15;
                let y = si * 16 + (index >> 8);
                let z = (index >> 4) & 15;
                found.push((
                    [
                        cx * 16 + x as i32,
                        self.min_y + y as i32,
                        cz * 16 + z as i32,
                    ],
                    kind,
                ));
            });
        }
        found
    }

    /// Записывает точки интереса столбца в индекс.
    fn store_pois(&self, cx: i32, cz: i32, list: Vec<Poi>) {
        let mut map = self.pois.lock().unwrap();
        if list.is_empty() {
            map.remove(&(cx, cz));
        } else {
            map.insert((cx, cz), list);
        }
    }

    /// Одна клетка после `/setblock`: чанк уже проиндексирован целиком.
    fn note_poi(&self, pos: [i32; 3], state: u32) {
        let key = (pos[0] >> 4, pos[2] >> 4);
        let mut map = self.pois.lock().unwrap();
        let list = map.entry(key).or_default();
        list.retain(|(at, _)| *at != pos);
        if let Some(kind) = poi::of_block(self.state_name(state)) {
            list.push((pos, kind));
        }
        if list.is_empty() {
            map.remove(&key);
        }
    }

    /// Ближайшая точка интереса среди уже загруженных чанков. Радиус — 256 чанков.
    pub(crate) fn locate_poi(&self, name: &str, origin: [i32; 3]) -> Option<[i32; 3]> {
        let types = poi::query(&self.poi_tags, name)?;
        let (cx, cz) = (origin[0] >> 4, origin[2] >> 4);
        let map = self.pois.lock().unwrap();
        let mut best: Option<([i32; 3], i64)> = None;
        for ((x, z), list) in map.iter() {
            if (x - cx).abs() > 256 || (z - cz).abs() > 256 {
                continue;
            }
            for (pos, kind) in list {
                if !types.contains(*kind) {
                    continue;
                }
                let dist = (pos[0] as i64 - origin[0] as i64).pow(2)
                    + (pos[1] as i64 - origin[1] as i64).pow(2)
                    + (pos[2] as i64 - origin[2] as i64).pow(2);
                if best.is_none_or(|(_, d)| dist < d) {
                    best = Some((*pos, dist));
                }
            }
        }
        best.map(|(pos, _)| pos)
    }

    /// Ставит блок (мировые координаты) и рассылает `block_update`.
    /// `None` — позиция вне мира по высоте или чанк шума ещё не сгенерирован.
    fn set_block(&self, [x, y, z]: [i32; 3], state: u32) -> Option<u32> {
        let ry = y - self.min_y;
        if ry < 0 || ry as usize >= self.template.height() {
            return None;
        }
        let key = (x >> 4, z >> 4);
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        // Чанк шума появится при отправке игроку, а не из тика.
        if self.terrain.is_some() && !self.edited.read().unwrap().contains_key(&key) {
            return None;
        }
        self.materialize_void(key.0, key.1);
        let template = &self.template;
        let (old, inserted) = {
            let mut world = self.edited.write().unwrap();
            let inserted = !world.contains_key(&key);
            let (column, rev) = world.entry(key).or_insert_with(|| (template.clone(), 0));
            let old = column.set_block(lx, ry as usize, lz, state);
            if old == state {
                return Some(old);
            }
            *rev += 1;
            self.dirty.lock().unwrap().insert(key);
            (old, inserted)
        };
        if inserted {
            if let Some((column, _)) = self.edited.read().unwrap().get(&key) {
                let list = self.collect_pois(key.0, key.1, column);
                self.store_pois(key.0, key.1, list);
            }
        } else {
            self.note_poi([x, y, z], state);
        }
        if self.light.same_light(old, state) {
            // Кадр чанка кэшируется без сверки ревизии: иначе клиент получит старый блок.
            self.chunk_frames.lock().unwrap().remove(&key);
        } else {
            // Свет доходит до соседних чанков, но читает только загруженное:
            // это вызывается и из тика, диск тут ждать нельзя.
            // BFS света одного блока короче копии девяти столбцов: прямо под замком.
            let platform = self.void_platform;
            let min_y = self.min_y;
            let template = &self.template;
            let light = &self.light;
            let make = |cx: i32, cz: i32| {
                let (stone, cobble) = platform?;
                stamp_void(template, cx, cz, min_y, stone, cobble, light)
            };
            let mut world = self.edited.write().unwrap();
            // Под той же блокировкой: chunk_frame не вставит устаревший кадр.
            let mut frames = self.chunk_frames.lock().unwrap();
            let mut access = WorldLight {
                world: &mut world,
                fallback: self.terrain.is_none().then_some(template),
                touched: FastSet::default(),
                last: None,
                special: platform.is_some().then_some(&make as _),
            };
            Engine::new(&mut access, &self.light, self.template.height()).block_changed([x, ry, z]);
            let touched = std::mem::take(&mut access.touched);
            drop(access);
            self.dirty.lock().unwrap().extend(touched.iter().copied());
            write_light(&mut world, &mut frames, key, None, touched);
        }
        let local = ((x & 15) << 8 | (z & 15) << 4 | (y & 15)) as u16;
        self.pending_blocks
            .lock()
            .unwrap()
            .entry([x >> 4, y >> 4, z >> 4])
            .or_default()
            .push((local, state));
        self.observe([x, y, z]);
        self.wake_fluids([x, y, z]);
        Some(old)
    }

    /// Наблюдатели, смотрящие на изменённый `changed`, дают импульс через 2 тика.
    fn observe(&self, changed: [i32; 3]) {
        for d in 0..6 {
            let n = crate::redstone::step(changed, d);
            let Some(s) = self.block(n) else { continue };
            if !chunkwake_world::has_flag(s, chunkwake_world::flags::OBSERVER) {
                continue;
            }
            let pl = &self.survival.placement;
            if pl.prop(s, "powered") != Some("true")
                && crate::redstone::watching(d, pl.prop(s, "facing").unwrap_or(""))
            {
                self.schedule(n, 2);
            }
        }
    }

    /// Выход компаратора: из памяти или блок-сущности.
    fn comparator_output(&self, p: [i32; 3]) -> i32 {
        if let Some(&v) = self.comparators.lock().unwrap().get(&p) {
            return v;
        }
        let v = self
            .block_entity(p)
            .and_then(|be| be.nbt.get("OutputSignal").and_then(Tag::as_i64))
            .unwrap_or(0) as i32;
        self.comparators.lock().unwrap().insert(p, v);
        v
    }

    /// Заполненность контейнера в `p` для компаратора.
    fn analog_at(&self, p: [i32; 3]) -> Option<i32> {
        let s = self.block(p)?;
        let name = self.state_name(s);
        let full = |slots: &[Slot]| {
            slots
                .iter()
                .flatten()
                .map(|st| (st.count, self.max_stack(st.item)))
                .collect::<Vec<_>>()
        };
        if furnace_kind(name).is_some() {
            let slots = self.with_furnace(p, |e| e.f.slots)?;
            return Some(crate::redstone::fullness(full(&slots), 3));
        }
        container_entity(name)?;
        let mut slots = self.container_items(p);
        if let Some((other, _)) = self.chest_partner(p) {
            slots.extend(self.container_items(other));
        }
        Some(crate::redstone::fullness(full(&slots), slots.len()))
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

    pub fn set_spawn_protection(&self, radius: i32) {
        self.spawn_protection
            .store(radius.max(0), Ordering::Relaxed);
    }

    /// Id частиц из реестра `particle_type` (`/particle`).
    pub fn set_particle_types(&self, types: HashMap<String, i32>) {
        *self.particle_types.lock().unwrap() = types;
    }

    /// Строки `en_us.json`: консоль подставляет их вместо ключей перевода.
    pub fn set_command_lang(&self, lang: HashMap<String, String>) {
        *self.command_lang.lock().unwrap() = lang;
    }

    /// Фраза команды для лога. Клиенту уходит исходный компонент.
    pub fn command_text(&self, value: &serde_json::Value) -> String {
        let lang = self.command_lang.lock().unwrap();
        chunkwake_protocol::text::console_line(value, &lang)
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

    /// Имя предмета для сообщений команды (`/item`).
    pub(crate) fn item_name_of(&self, id: i32) -> &str {
        self.item_name(id)
    }

    /// ID предмета по полному имени (`/loot`).
    pub(crate) fn item_id_named(&self, name: &str) -> Option<i32> {
        self.item_id(name)
    }

    fn item_id(&self, name: &str) -> Option<i32> {
        self.survival.item_ids.get(name).copied()
    }

    /// Игрок в таб-листе и зеркале без сокета: команды видят его как онлайн.
    pub fn present_player(&self, uuid: Uuid, name: &str) {
        let pos = self.spawn;
        let entity_id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let me = Online {
            entry: PlayerEntry {
                uuid,
                name: name.to_owned(),
                properties: Vec::new(),
                game_mode: 0,
                latency_ms: 0,
            },
            entity_id,
            pos,
            yaw: 0.0,
            pitch: 0.0,
            base: play::quantize(pos),
            moves: 0,
            dirty: false,
            rotated: false,
            on_ground: true,
            gliding: false,
            collects: true,
        };
        self.hub.players.lock().unwrap().insert(uuid, me);
        self.mark_work();
        self.live.lock().unwrap().insert(
            uuid,
            Live {
                health: 20.0,
                food: 20,
                saturation: 5.0,
                exhaustion: 0.0,
                game_mode: 0,
                selected: 0,
                slots: [None; survival::SLOTS],
                pos,
                yaw: 0.0,
                pitch: 0.0,
                effects: Vec::new(),
                xp: Default::default(),
                sneaking: false,
            },
        );
    }

    /// Снимок игрока для плагинов (формат — [`Live::render`]); `None` — не онлайн.
    ///
    /// Текст кэшируется по [`Live::fingerprint`]: плагин, читающий несколько
    /// полей подряд, получает то же состояние, что и раньше, но без
    /// повторного рендера 41 строки инвентаря. Сверка идёт по самим полям
    /// зеркала, поэтому устаревший текст невозможен.
    pub fn player_state(&self, uuid: Uuid) -> Option<String> {
        self.player_state_shared(uuid)
            .map(|(_, text)| text.to_string())
    }

    /// Текст снимка как общий буфер: без копии для передачи в JVM.
    ///
    /// Возвращает отпечаток состояния и текст. Java сверяет отпечаток дешёвым
    /// вызовом и разбирает строку только когда он изменился.
    pub fn player_state_shared(&self, uuid: Uuid) -> Option<(u64, Arc<str>)> {
        let live = self.live.lock().unwrap().get(&uuid)?.clone();
        let key = live.fingerprint();
        if let Some((at, text)) = self.state_cache.lock().unwrap().get(&uuid)
            && *at == key
        {
            return Some((key, text.clone()));
        }
        let text: Arc<str> = Arc::from(
            live.render(|id| Some(self.item_name(id)), |id| self.effect_name(id))
                .as_str(),
        );
        let mut cache = self.state_cache.lock().unwrap();
        if cache.len() > 1024 {
            cache.clear();
        }
        cache.insert(uuid, (key, text.clone()));
        Some((key, text))
    }

    /// Отпечаток снимка игрока: `None` — игрока нет. Дешёвый вызов для Java.
    pub fn state_version(&self, uuid: Uuid) -> Option<u64> {
        let live = self.live.lock().unwrap();
        Some(live.get(&uuid)?.fingerprint())
    }

    /// Команда плагина сессии игрока (формат — [`Control::parse`]).
    pub fn player_control(&self, uuid: Uuid, line: &str) -> bool {
        let Some(c) = Control::parse(
            line,
            |n| self.item_id(n),
            |n| self.survival.effects.get(n).copied(),
        ) else {
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

    fn effect_name(&self, id: i32) -> Option<&str> {
        self.survival
            .effects
            .iter()
            .find(|(_, v)| **v == id)
            .map(|(k, _)| k.as_str())
    }

    /// Запрос плагина к миру: `get x y z` → имя блока, `set x y z имя` → `1`/`0`,
    /// `effects` → эффекты по порядку ID (строки), `isblock имя` → `1`/`0`,
    /// `blocks с` → имена блоков по порядку начиная с номера `с` (кусок до 7 КБ).
    pub fn world_request(&self, line: &str) -> String {
        if let Some(rest) = line.strip_prefix("inv ") {
            return self.plugin_inv_request(rest);
        }
        if let Some(rest) = line.strip_prefix("mob ") {
            return self.mob_request(rest);
        }
        if line == "tps" {
            return self.tick_meter_line();
        }
        if line == "seed" {
            return self.seed.to_string();
        }
        // `dropitem x y z разброс предмет`: `World.dropItem(Naturally)`.
        if let Some(rest) = line.strip_prefix("dropitem ") {
            // Без промежуточного `Vec`: плагины, ставящие предметы пачками,
            // зовут это на каждый предмет.
            let mut it = rest.splitn(5, ' ');
            let num = |v: Option<&str>| v.and_then(|v| v.parse::<f64>().ok());
            let (x, y, z) = (num(it.next()), num(it.next()), num(it.next()));
            let natural = it.next() == Some("1");
            let (Some(x), Some(y), Some(z)) = (x, y, z) else {
                return "0".into();
            };
            let Some(stack) = it
                .next()
                .and_then(|l| crate::stack_nbt::from_line(l, |n| self.item_id(n)))
            else {
                return "0".into();
            };
            let (at, vel) = if natural {
                // `dropItemNaturally`: сдвиг до ±0.25 и скорость выпавшего из блока.
                let r: [f64; 3] = rand::random();
                (
                    [
                        x + r[0] * 0.5 - 0.25,
                        y + r[1] * 0.5 - 0.25,
                        z + r[2] * 0.5 - 0.25,
                    ],
                    items::pop_velocity(rand::random(), rand::random()),
                )
            } else {
                ([x, y, z], [0.0; 3])
            };
            self.drop_item(at, vel, stack, items::BLOCK_DELAY);
            return "1".into();
        }
        if let Some(name) = line.strip_prefix("defaultdata ") {
            return self
                .survival
                .block_defaults
                .get(name)
                .map_or(String::new(), |&s| self.state_string(s));
        }
        if let Some(name) = line.strip_prefix("isblock ") {
            return if self.survival.block_defaults.contains_key(name) {
                "1"
            } else {
                "0"
            }
            .into();
        }
        if let Some(from) = line.strip_prefix("blocks ") {
            let from: usize = from.trim().parse().unwrap_or(0);
            let mut names: Vec<&String> = self.survival.block_defaults.keys().collect();
            names.sort();
            let mut out = String::new();
            for n in names.into_iter().skip(from) {
                if out.len() + n.len() + 1 > 7000 {
                    break;
                }
                out.push_str(n);
                out.push('\n');
            }
            return out;
        }
        // `enchantinfo имя`: предметы `supported_items`, затем `exclusive_set` (через пробел).
        if let Some(name) = line.strip_prefix("enchantinfo ") {
            let Some(d) = self
                .survival
                .enchanting
                .defs
                .iter()
                .find(|d| d.name == name.trim())
            else {
                return String::new();
            };
            let join = |set: &HashSet<String>| {
                let mut v: Vec<&str> = set.iter().map(String::as_str).collect();
                v.sort_unstable();
                v.join(" ")
            };
            return format!("{}\n{}", join(&d.supported), join(&d.exclusive));
        }
        if line == "enchantments" {
            return self.survival.enchantments.join(
                "
",
            );
        }
        if line == "effects" {
            let mut all: Vec<(&i32, &String)> =
                self.survival.effects.iter().map(|(k, v)| (v, k)).collect();
            all.sort();
            return all
                .into_iter()
                .map(|(_, k)| k.as_str())
                .collect::<Vec<_>>()
                .join("\n");
        }
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
            Some("light") => {
                // `skylight blocklight`: плагинам нужны оба уровня (BlueMap
                // рисует освещение по ним). Нет чанка — как у ванили: 15 неба, 0 блока.
                let (sky, block) = self.with_world(|w| {
                    (
                        w.sky_light_at([x, y, z]).unwrap_or(15),
                        w.block_light_at([x, y, z]).unwrap_or(0),
                    )
                });
                format!("{sky} {block}")
            }
            Some("data") => self
                .block([x, y, z])
                .map_or(String::new(), |s| self.state_string(s)),
            Some("set") => {
                let state = it.next().and_then(|spec| self.parse_state(spec));
                let physics = it.next() != Some("0");
                match state {
                    Some(s) if self.set_block([x, y, z], s).is_some() => {
                        // Поток плагинов: физику — в тик, иначе события редстоуна ждали бы сами себя.
                        if physics {
                            self.deferred_updates.lock().unwrap().push([x, y, z]);
                            self.mark_work();
                        }
                        "1".into()
                    }
                    _ => "0".into(),
                }
            }
            _ => String::new(),
        }
    }

    /// Присед меняется от пакета ввода, вне тика: снимок правится точечно.
    fn set_sneaking(&self, uuid: Uuid, sneaking: bool) {
        if let Some(live) = self.live.lock().unwrap().get_mut(&uuid)
            && live.sneaking != sneaking
        {
            live.sneaking = sneaking;
            self.state_cache.lock().unwrap().remove(&uuid);
            self.mark_work();
        }
    }

    fn publish_live(&self, uuid: Uuid, st: &PlayerState, game_mode: u8) {
        let (pos, yaw, pitch) = self.player_at(uuid).unwrap_or(([0.0; 3], 0.0, 0.0));
        let v = &st.vitals;
        let mut live = Live {
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
            effects: v.effects.clone(),
            xp: st.xp,
            sneaking: false,
        };
        {
            // Присед ставит `player_input`, а не этот снимок: переносим текущее
            // значение, иначе каждое обновление инвентаря его сбрасывало бы.
            let mut live_map = self.live.lock().unwrap();
            if let Some(old) = live_map.get(&uuid) {
                live.sneaking = old.sneaking;
            }
            live_map.insert(uuid, live);
        }
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

    /// Событие без игрока из синхронного кода (тик): поток ждёт ответа
    /// плагинов, как главный поток Paper. Без слушателей — сразу `None`.
    fn sync_event(&self, kind: Event, arg: impl FnOnce() -> String) -> Option<String> {
        let h = self.hooks().filter(|h| h.listens(kind))?;
        let rx = h.event(kind, Uuid::nil(), arg())?;
        let rt = tokio::runtime::Handle::try_current().ok()?;
        tokio::task::block_in_place(|| {
            rt.block_on(async { tokio::time::timeout(PLUGIN_TIMEOUT, rx).await.ok()?.ok() })
        })
    }

    /// `BlockRedstoneEvent`: новая сила (плагин может сменить).
    fn redstone_event(&self, pos: [i32; 3], old: i32, new: i32) -> i32 {
        self.sync_event(Event::Redstone, || {
            format!("{} {} {} {old} {new}", pos[0], pos[1], pos[2])
        })
        .and_then(|r| r.trim().parse::<i32>().ok())
        .map_or(new, |v| v.clamp(0, 15))
    }

    /// `BlockRedstoneEvent` пачкой — один вызов плагинов на всю сеть проводов.
    fn redstone_events(&self, changes: &[([i32; 3], i32, i32)]) -> Vec<i32> {
        let reply = self.sync_event(Event::Redstone, || {
            // Одна строка на изменение, без промежуточного `Vec` строк и
            // `join`: на большой сети это тысячи форматирований.
            let mut arg = String::with_capacity(changes.len() * 24);
            for (i, (p, old, new)) in changes.iter().enumerate() {
                use std::fmt::Write;
                if i > 0 {
                    arg.push(';');
                }
                let _ = write!(arg, "{} {} {} {old} {new}", p[0], p[1], p[2]);
            }
            arg
        });
        let mut parts = reply.as_deref().map(|r| r.split(';'));
        changes
            .iter()
            .map(|&(_, _, new)| {
                parts
                    .as_mut()
                    .and_then(|i| i.next())
                    .and_then(|v| v.trim().parse::<i32>().ok())
                    .map_or(new, |v| v.clamp(0, 15))
            })
            .collect()
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
    ///
    /// Права обновляются у клиента (`permissions_changed`): дерево команд и
    /// значок оператора приходят сразу, без перезахода.
    pub(crate) fn op_command(&self, name: &str, grant: bool) -> serde_json::Value {
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

    /// Команда из консоли сервера: встроенные через диспетчер, иначе —
    /// плагинам от имени консоли. Возвращает текст ответа для лога.
    pub async fn console_command(&self, line: &str) -> String {
        let source = crate::play::commands::dispatch::Source::console();
        if let Some(outcome) = crate::play::commands::execute(self, &source, line) {
            return match outcome {
                crate::play::commands::dispatch::Outcome::Feedback(msg) => self.command_text(&msg),
                crate::play::commands::dispatch::Outcome::Broadcast(msg) => {
                    self.broadcast_json(&msg.to_string());
                    self.command_text(&msg)
                }
                crate::play::commands::dispatch::Outcome::Silent => String::new(),
            };
        }
        let known = match self.hooks() {
            Some(h) => tokio::time::timeout(Duration::from_secs(5), h.command(Uuid::nil(), line))
                .await
                .is_ok_and(|r| r.unwrap_or(false)),
            None => false,
        };
        if known {
            String::new()
        } else {
            format!("Unknown command: {line}")
        }
    }

    /// Выполнение команды игрока: `None` — команда не наша (её пробуют плагины).
    pub fn run_command(
        &self,
        source: &crate::play::commands::dispatch::Source,
        line: &str,
    ) -> Option<crate::play::commands::dispatch::Outcome> {
        crate::play::commands::execute(self, source, line)
    }

    /// Команда из цепочки `/execute`: `~ ~ ~` считается от `pos`.
    pub fn run_command_at(
        &self,
        source: &crate::play::commands::dispatch::Source,
        line: &str,
        pos: [f64; 3],
        yaw: f32,
        pitch: f32,
    ) -> Option<crate::play::commands::dispatch::Outcome> {
        crate::play::commands::execute_with(self, source, line, Some((pos, yaw, pitch)))
    }

    fn difficulty(&self) -> Difficulty {
        Difficulty::from_id(self.survival.difficulty.load(Ordering::Relaxed))
    }

    /// Команда в канал сессии игрока; `false` — игрока нет или канал закрыт.
    fn push_control(&self, uuid: Uuid, c: Control) -> bool {
        // Зеркало обновляем сразу: плагин и следующая команда видят результат.
        if let Some(live) = self.live.lock().unwrap().get_mut(&uuid) {
            c.apply(live);
        }
        self.controls
            .lock()
            .unwrap()
            .get(&uuid)
            .is_some_and(|tx| tx.send(c).is_ok())
    }

    /// Телепорт игрока (`Player.teleport`).
    fn teleport_player(&self, uuid: Uuid, pos: [f64; 3], yaw: f32, pitch: f32) -> bool {
        self.push_control(uuid, Control::Teleport(pos, yaw, pitch))
    }

    /// Сколько столбцов мира прочитано с диска.
    fn regions_loaded(&self) -> usize {
        self.regions.lock().unwrap().len() * 1024
    }

    /// Плюс тик игрового времени. Часы клиента идут сами (скорость 1);
    /// сверка — раз в секунду, как `MinecraftServer.tickChildren`.
    fn advance_time(&self) {
        let now = self.game_time.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self
            .day_time
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |d| {
                Some((d + 1).rem_euclid(24_000))
            });
        if now % 20 == 0 {
            self.broadcast_time();
        }
    }

    fn broadcast_time(&self) {
        self.hub.send(
            None,
            play::set_time(
                self.ids.set_time,
                self.game_time.load(Ordering::Relaxed),
                self.day_time.load(Ordering::Relaxed),
                true,
            ),
        );
    }

    /// Дерево команд для клиента по уровню прав: встроенные и команды плагинов.
    fn commands_packet(&self, level: i32) -> Vec<u8> {
        crate::play::commands::commands_packet(self, level)
    }

    pub fn set_source(&self, source: Arc<dyn ChunkSource>) {
        let _ = self.source.set(source);
    }

    /// Пул генерации создаётся один раз и переживает сессии игроков.
    /// Пока пула нет (тесты без мира), генерация идёт прямо в сессии.
    pub fn set_gen_pool(&self, pool: Arc<crate::chunkgen::GenPool>) {
        let _ = self.gen_pool.set(pool);
    }

    /// Слабая ссылка на себя: задачам пула нужен `'static` доступ к плану.
    pub fn set_self_weak(self: &Arc<Self>) {
        let _ = self.self_weak.set(Arc::downgrade(self));
    }

    /// Гарантирует, что регион чанка `(x, z)` прочитан с диска.
    ///
    /// Вызывается только с потоков генерации (задачи пула) и загрузки чанков,
    /// но не из тика и не из задачи сессии игрока: разбор региона — это до
    /// 1024 чанков NBT, на тике он превращался в замирание мира.
    /// Загрузка синхронна намеренно: `generate_chunk` должен видеть уже
    /// прочитанный столбец, иначе сгенерирует поверх сохранённого.
    fn ensure_region(&self, x: i32, z: i32) {
        let Some(source) = self.source.get() else {
            return;
        };
        let key = (x >> 5, z >> 5);
        // Регионы только добавляются: последний известный в потоке — без замка.
        let me = (self as *const Self as usize, key);
        if LAST_REGION.with(|c| c.get() == Some(me)) {
            return;
        }
        if self.regions.lock().unwrap().contains(&key) {
            LAST_REGION.with(|c| c.set(Some(me)));
            return;
        }
        // Замок своего шарда: разные регионы читаются параллельно, а один и
        // тот же — один раз. Второй проверки под замком достаточно: пока
        // первый загрузчик держал шард, второй ждал и увидит готовый регион.
        let _guard = self.region_load[region_shard(key)].lock().unwrap();
        if self.regions.lock().unwrap().contains(&key) {
            LAST_REGION.with(|c| c.set(Some(me)));
            return;
        }
        match source.load_region(key.0, key.1) {
            Ok(columns) => self.install_region(columns),
            Err(e) => tracing::error!("region {key:?}: {e}"),
        }
        match source.load_entities(key.0, key.1) {
            Ok(chunks) => self.load_mobs(chunks),
            Err(e) => tracing::error!("entities {key:?}: {e}"),
        }
        self.regions.lock().unwrap().insert(key);
    }

    /// Вставляет прочитанный регион: только те столбцы, которых ещё нет,
    /// поэтому правка, сделанная до загрузки, не затирается диском.
    fn install_region(&self, columns: Vec<LoadedChunk>) {
        let mut world = self.edited.write().unwrap();
        let mut cache = self.chunk_frames.lock().unwrap();
        let mut found = Vec::new();
        let mut hoppers = Vec::new();
        let mut fresh = Vec::new();
        // Сохранённые чанки уже сгенерированы: мобов при генерации им не положено.
        self.populated
            .lock()
            .unwrap()
            .extend(columns.iter().map(|(pos, _)| *pos));
        for (pos, c) in columns {
            hoppers.extend(
                c.block_entities
                    .iter()
                    .filter(|(_, be)| be.id == "minecraft:hopper")
                    .map(|(p, _)| *p),
            );
            found.extend(
                c.block_entities
                    .iter()
                    .filter(|(_, be)| FURNACES.contains(&be.id.as_str()))
                    .map(|(p, _)| *p),
            );
            self.pending_campfires.lock().unwrap().extend(
                c.block_entities
                    .iter()
                    .filter(|(_, be)| be.id == "minecraft:campfire")
                    .map(|(p, _)| *p),
            );
            if let std::collections::hash_map::Entry::Vacant(e) = world.entry(pos) {
                fresh.push(pos);
                e.insert((c, 0));
                cache.remove(&pos);
            }
        }
        drop(world);
        drop(cache);
        // Скан точек интереса — уже под замком на запись это значило бы
        // останавливать тик на 98 тыс. блоков каждого региона с диска.
        // Здесь достаточно чтения: столбцы уже вставлены, и читатели друг
        // другу не мешают.
        if !fresh.is_empty() {
            let mut lists = Vec::with_capacity(fresh.len());
            {
                let world = self.edited.read().unwrap();
                for pos in fresh {
                    if let Some((column, _)) = world.get(&pos) {
                        lists.push((pos, self.collect_pois(pos.0, pos.1, column)));
                    }
                }
            }
            for (pos, list) in lists {
                self.store_pois(pos.0, pos.1, list);
            }
        }
        self.pending_furnaces.lock().unwrap().extend(found);
        // Чанк с диска: воронки встают в очередь на ближайший тик.
        {
            let now = self.server_ticks.load(std::sync::atomic::Ordering::Relaxed);
            let mut queue = self.hopper_ticks.lock().unwrap();
            for p in hoppers {
                queue.schedule(p, now, true);
            }
        }
    }

    /// Блоки чанка, если его ещё нет. `true` — столбец только что вставлен.
    /// Диск не затирается. Свет ставит [`Self::spread_commit`], не этот вызов.
    fn generate_chunk(&self, x: i32, z: i32) -> bool {
        self.ensure_region(x, z);
        let Some(generate) = &self.terrain else {
            return false;
        };
        if self.edited.read().unwrap().contains_key(&(x, z)) {
            return false;
        }
        let Some(mut column) = generate(x, z) else {
            // Чанк уже отдан: вызывающий возьмёт готовый столбец из мира.
            return false;
        };
        // Заготовка `from_blocks` — небо 15 везде. До пролива это не свет.
        let n = column.sections.len();
        column.sky_light = vec![chunkwake_world::light::LightLayer::Uniform(0); n];
        column.block_light = vec![chunkwake_world::light::LightLayer::Uniform(0); n];
        // Скан точек интереса — до замка: он только читает наш столбец.
        let list = self.collect_pois(x, z, &column);
        let mut world = self.edited.write().unwrap();
        if let std::collections::hash_map::Entry::Vacant(e) = world.entry((x, z)) {
            e.insert((column, 0));
            drop(world);
            self.chunk_frames.lock().unwrap().remove(&(x, z));
            self.store_pois(x, z, list);
            true
        } else {
            false
        }
    }

    /// Пролив света одного нового чанка в соседей и обратно. Соседи, чей кадр
    /// уже уходил клиенту, возвращаются — их надо переслать.
    ///
    /// Для пачки новых чанков это неверный путь: первый чанк видит у соседа
    /// нулевой свет, а сосед потом не пересчитывает первого — на общей грани
    /// остаётся тёмный шов. Пачку считает `light_ring` одним
    /// `light::spread_wave`; здесь — одиночный случай (догрузка, тесты).
    #[cfg(test)]
    #[allow(dead_code)]
    fn spread_commit(&self, x: i32, z: i32) -> Vec<(i32, i32)> {
        if self.terrain.is_none() {
            return Vec::new();
        }
        // Сосед с диска должен быть в памяти, иначе грань останется тёмной.
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            self.ensure_region(x + dx, z + dz);
        }
        let mut columns = HashMap::new();
        {
            let world = self.edited.read().unwrap();
            let Some(col) = world.get(&(x, z)) else {
                return Vec::new();
            };
            columns.insert((x, z), col.0.clone());
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let n = (x + dx, z + dz);
                if let Some((col, _)) = world.get(&n) {
                    columns.insert(n, col.clone());
                }
            }
        }
        let touched = chunkwake_world::light::spread_chunk(&mut columns, (x, z), &self.light);
        self.commit_light(&mut columns, &touched)
    }

    /// Чанк генератора — основа для загрузки сохранённых столбцов.
    pub fn template(&self) -> &ChunkColumn {
        &self.template
    }

    /// Изменённые с последнего сохранения столбцы: копии и номера правок.
    /// После успешной записи передайте их в [`Self::mark_saved`].
    pub fn unsaved(&self) -> Vec<((i32, i32), ChunkColumn, u64)> {
        let world = self.edited.read().unwrap();
        let saved = self.saved.lock().unwrap();
        let dirty = self.dirty.lock().unwrap();
        // Только правленые после записи, а не все загруженные с диска.
        dirty
            .iter()
            .filter_map(|pos| Some((pos, world.get(pos)?)))
            .filter(|(pos, (_, rev))| saved.get(*pos) != Some(rev) && *rev != 0)
            .map(|(pos, (c, rev))| (*pos, c.clone(), *rev))
            .collect()
    }

    pub fn set_player_source(&self, source: Arc<dyn PlayerSource>) {
        let _ = self.players_source.set(source);
    }

    /// Снимки остаются в памяти до подтверждения успешной записи.
    pub fn pending_player_saves(&self) -> Vec<(Uuid, PlayerSave)> {
        self.player_saves
            .lock()
            .unwrap()
            .iter()
            .map(|(id, save)| (*id, save.clone()))
            .collect()
    }

    pub fn mark_player_saved(&self, uuid: Uuid, written: &PlayerSave) {
        let mut pending = self.player_saves.lock().unwrap();
        acknowledge_player_save(&mut pending, uuid, written);
    }

    pub fn disconnect_all(&self) {
        self.shutting_down.store(true, Ordering::Release);
        for tx in self.controls.lock().unwrap().values() {
            let _ = tx.send(Control::Shutdown);
        }
    }

    /// Запомнить состояние игрока для следующего сохранения.
    fn snapshot(&self, uuid: Uuid, st: &PlayerState) {
        let Some((pos, yaw, pitch)) = self.player_at(uuid) else {
            return;
        };
        let mut save = st.save(pos, yaw, pitch);
        // `/spawnpoint` живёт отдельно от инвентаря: снимок не должен его стирать.
        match self.spawn_points.lock().unwrap().get(&uuid).copied() {
            Some(spawn) => save.spawn = spawn,
            None => {
                if let Some(prev) = self.player_saves.lock().unwrap().get(&uuid) {
                    save.spawn = prev.spawn;
                }
            }
        }
        self.player_saves.lock().unwrap().insert(uuid, save);
    }

    pub fn mark_saved(&self, chunks: impl IntoIterator<Item = ((i32, i32), u64)>) {
        let world = self.edited.read().unwrap();
        let mut saved = self.saved.lock().unwrap();
        let mut dirty = self.dirty.lock().unwrap();
        for (pos, rev) in chunks {
            // Записана последняя правка — столбец чист; иначе ждёт следующего сейва.
            if world.get(&pos).is_some_and(|e| e.1 == rev) {
                dirty.remove(&pos);
            }
            saved.insert(pos, rev);
        }
    }

    /// Одно чтение блока: берёт замок мира (в тике лучше [`Self::with_world`],
    /// где замок один на всю фазу).
    #[inline]
    fn block(&self, p: [i32; 3]) -> Option<u32> {
        let [x, y, z] = p;
        let ry = y - self.min_y;
        if ry < 0 || ry as usize >= self.template.height() {
            return None;
        }
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        let columns = self.edited.read().unwrap();
        if let Some((column, _)) = columns.get(&(x >> 4, z >> 4)) {
            return Some(column.block(lx, ry as usize, lz));
        }
        if self.terrain.is_some() {
            return None;
        }
        drop(columns);
        Some(Self::flat_in(
            self.void_platform,
            self.min_y,
            &self.template,
            x,
            y,
            z,
            lx,
            ry as usize,
            lz,
        ))
    }

    /// Мир только для чтения на время фазы: один `RwLock::read` вместо замка
    /// в каждом `block()`.
    ///
    /// Тик мобов читает блоки тысячами (коробки коллизий — по 2–5 на моба в
    /// каждом из десятков блоков), и замок на каждое чтение стоил дороже
    /// самого поиска в карте. Здесь набор столбцов берётся один раз, а внутри
    /// это уже обычные заимствования.
    ///
    /// Порядок замков не меняется: вызывающий сначала берёт свой замок
    /// (мобы, предметы, стрелы), потом мир — как было с `block()`.
    fn with_world<R>(&self, f: impl FnOnce(&WorldView<'_>) -> R) -> R {
        let columns = self.edited.read().unwrap();
        let view = WorldView {
            columns: &columns,
            template: &self.template,
            min_y: self.min_y,
            flat: self.terrain.is_none(),
            void_platform: self.void_platform,
        };
        f(&view)
    }

    /// Пересобирает набор чанков, которые тикают: квадрат радиуса
    /// `simulation-distance` вокруг каждого игрока. Зовётся раз в тик, но
    /// набор пересобирается только когда состав изменился: сравнение идёт по
    /// позициям игроков (чанк + радиус), а не по всему набору.
    fn refresh_simulated(&self) {
        let r = self.login.simulation_distance;
        // Ключ состава: чанки игроков и `/forceload`. Дешёвая проверка «ничего не изменилось».
        let mut key: Vec<(i32, i32)> = {
            let players = self.hub.players.lock().unwrap();
            players.values().map(|p| chunk_of(p.pos)).collect()
        };
        key.sort_unstable();
        let mut forced: Vec<(i32, i32)> = self.forced.lock().unwrap().iter().copied().collect();
        forced.sort_unstable();
        let mut last = self.simulated_key.lock().unwrap();
        if last
            .as_ref()
            .is_some_and(|(lr, lk, lf)| *lr == r && lk == &key && lf == &forced)
        {
            return;
        }
        let mut set = FastSet::default();
        {
            let players = self.hub.players.lock().unwrap();
            for p in players.values() {
                chunk_square(chunk_of(p.pos), r, &mut set);
            }
        }
        set.extend(forced.iter().copied());
        *last = Some((r, key, forced));
        drop(last);
        *self.simulated.lock().unwrap() = Arc::new(set);
    }

    /// Набор симулируемых чанков для чтения: атомарная прибавка вместо клона.
    fn simulated_set(&self) -> Arc<FastSet<(i32, i32)>> {
        self.simulated.lock().unwrap().clone()
    }

    /// Сколько чанков в наборе — для тестов и `/tps`.
    pub fn simulated_chunks(&self) -> usize {
        self.simulated.lock().unwrap().len()
    }

    /// Задать набор напрямую: тесты и загрузка без игроков.
    ///
    /// Ставит флаг работы: иначе тик решил бы, что делать нечего, и не
    /// запустил бы подсистемы (лаборатория и тесты не имеют игроков).
    pub fn set_simulated(&self, chunks: impl IntoIterator<Item = (i32, i32)>) {
        let set: FastSet<(i32, i32)> = chunks.into_iter().collect();
        *self.simulated.lock().unwrap() = Arc::new(set);
        // Прямая установка обходит `refresh_simulated`, поэтому ключ состава
        // сбрасывается: иначе следующий тик решил бы, что набор уже верный.
        *self.simulated_key.lock().unwrap() = None;
        self.mark_work();
    }

    /// `f` по пачке на `workers` потоках, результат в исходном порядке.
    #[cfg(test)]
    fn map_limited<T, R>(items: &[T], workers: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R>
    where
        T: Sync,
        R: Send,
    {
        let workers = workers.max(1).min(items.len().max(1));
        if workers <= 1 || items.len() <= 1 {
            return items.iter().map(f).collect();
        }
        let mut out = Vec::with_capacity(items.len());
        std::thread::scope(|scope| {
            let mut joins = Vec::with_capacity(workers);
            for batch in items.chunks(items.len().div_ceil(workers)) {
                joins.push(scope.spawn(|| {
                    background_priority();
                    batch.iter().map(&f).collect::<Vec<_>>()
                }));
            }
            for join in joins {
                out.extend(join.join().expect("chunk wave"));
            }
        });
        out
    }

    async fn send_batch(&self, conn: &mut Connection, frames: &[Frame]) -> Result<(), NetError> {
        // Заголовки пачки кодируются один раз на пачку, а не через `hub.frame`
        // на каждый вызов: при включённом сжатии это два лишних zlib-потока.
        // Ёмкость считается сразу — кадры собираются без роста буфера.
        let start = self
            .hub
            .frame(&play::chunk_batch_start(self.ids.chunk_batch_start));
        let end = self.hub.frame(&play::chunk_batch_finished(
            self.ids.chunk_batch_finished,
            frames.len() as i32,
        ));
        let mut out = Vec::with_capacity(
            start.len() + end.len() + frames.iter().map(|f| f.len()).sum::<usize>(),
        );
        out.extend_from_slice(&start);
        for f in frames {
            out.extend_from_slice(f);
        }
        out.extend_from_slice(&end);
        conn.send_owned_frames(out).await
    }

    /// Светит кольцо и пересылает тех, чей кадр уже уходил клиенту.
    ///
    /// Столбцы на время света **переносятся** из `edited`, а не копируются:
    /// пачка из 96 чанков — это 96 × ~400 КиБ `states` плюс слои света, и
    /// раньше каждую пачку дважды прогоняли через `memcpy`. Перенос делает
    /// то же самое без копий; на время работы столбцов в `edited` нет, а
    /// `light_lock` не даёт другому проливу увидеть их отсутствие.
    fn light_ring(&self, ring: &[(i32, i32)]) {
        let resent = {
            let _light = self.light_lock.lock().unwrap();
            let mut columns = HashMap::new();
            {
                let mut world = self.edited.write().unwrap();
                for &pos in ring {
                    if let Some(entry) = world.remove(&pos) {
                        columns.insert(pos, entry.0);
                    }
                }
                // Загруженные соседи тоже участвуют: свет должен прийти из них.
                for &(x, z) in ring {
                    for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        let n = (x + dx, z + dz);
                        if columns.contains_key(&n) {
                            continue;
                        }
                        if let Some(entry) = world.remove(&n) {
                            columns.insert(n, entry.0);
                        }
                    }
                }
            }
            let origins: HashSet<(i32, i32)> = ring.iter().copied().collect();
            let touched = chunkwake_world::light::spread_wave(&mut columns, &origins, &self.light);
            self.commit_light(&mut columns, &touched)
        };
        for (nx, nz) in resent {
            self.hub.send_ready(self.chunk_frame(nx, nz));
        }
    }

    /// Записывает посчитанный свет в мир, поднимая ревизии затронутых
    /// столбцов и сбрасывая их кадры. Возвращает соседей, чей кадр уже уходил
    /// клиенту и должен быть переслан.
    ///
    /// Столбцы возвращаются из области света целиком (`columns` — их новые
    /// владельцы), поэтому копий слоёв света больше нет.
    fn commit_light(
        &self,
        columns: &mut HashMap<(i32, i32), ChunkColumn>,
        touched: &[(i32, i32)],
    ) -> Vec<(i32, i32)> {
        let mut world = self.edited.write().unwrap();
        let mut frames = self.chunk_frames.lock().unwrap();
        let mut dirty = self.dirty.lock().unwrap();
        let mut resent = Vec::new();
        for &pos in touched {
            let Some(updated) = columns.remove(&pos) else {
                continue;
            };
            // Ревизия прежнего столбца: свет — не правка, но кадр устарел.
            let revision = world.get(&pos).map_or(0, |e| e.1);
            let had = frames.remove(&pos).is_some();
            // Свой чанк остаётся с ревизией 0: он не правлен, только посчитан.
            let revision = if revision != 0 || had {
                dirty.insert(pos);
                if had {
                    resent.push(pos);
                }
                revision + 1
            } else {
                0
            };
            world.insert(pos, (updated, revision));
        }
        // Незатронутые столбцы области тоже надо вернуть: их сняли на время
        // пролива, и потерять их значило бы потерять чанк из мира.
        for (pos, column) in columns.drain() {
            let revision = world.get(&pos).map_or(0, |e| e.1);
            world.insert(pos, (column, revision));
        }
        resent
    }

    /// Чанк целиком: генерация, свет, кадр. Пара — соседи для пересылки.
    ///
    /// Одиночный путь: вызывается, когда чанк нужен вне волны (догрузка после
    /// правки, тесты). Пачку чанков ведёт поток (`play/stream.rs`) —
    /// там свет считается одним `light::spread_wave` на всю пачку.
    #[cfg(test)]
    #[allow(dead_code)]
    fn build_chunk(&self, x: i32, z: i32) -> (Frame, Vec<(i32, i32)>) {
        let resent = if self.generate_chunk(x, z) {
            let _light = self.light_lock.lock().unwrap();
            self.spread_commit(x, z)
        } else {
            Vec::new()
        };
        (self.chunk_frame(x, z), resent)
    }

    /// Игрок перешёл в другой чанк: выгружаем ушедшие из радиуса, шлём новые.
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
        if let Some(spawn) = save.as_ref().and_then(|s| s.spawn) {
            self.spawn_points
                .lock()
                .unwrap()
                .insert(player.uuid, Some(spawn));
        }
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
            gliding: false,
            collects: true,
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
        if self.shutting_down.load(Ordering::Acquire) {
            let _ = self
                .controls
                .lock()
                .unwrap()
                .get(&uuid)
                .unwrap()
                .send(Control::Shutdown);
        }
        let result = self
            .session(conn, rx, controls, &me, &others, &mut state)
            .await;
        // Хвост буфера (например, причина кика) — до закрытия.
        let _ = conn.flush().await;
        self.release_viewer(uuid);
        self.controls.lock().unwrap().remove(&uuid);
        self.live.lock().unwrap().remove(&uuid);
        // Немедленно запрашиваем автосохранение, оставляя снимок до подтверждения записи.
        self.snapshot(uuid, &state);
        if let Some(source) = self.players_source.get() {
            source.schedule_save();
        }

        self.hub.players.lock().unwrap().remove(&uuid);
        self.forget_ping(uuid);
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
        // Один и тот же игрок появляется у многих наблюдателей сразу, а
        // пакет зависит только от его позиции и флагов. Кэш по квантованной
        // позиции убирает повторные сборку и сжатие на каждого зрителя.
        let key = (
            e.entity_id,
            play::quantize(e.pos),
            e.yaw.to_bits(),
            e.pitch.to_bits(),
            e.gliding,
        );
        if let Some(hit) = self.spawn_frames.lock().unwrap().get(&key) {
            return hit.as_ref().clone();
        }
        let mut out = self.hub.frame(&play::add_entity(
            self.ids.add_entity,
            e.entity_id,
            e.uuid,
            self.player_type,
            e.pos,
            e.yaw,
            e.pitch,
            0,
        ));
        if e.gliding {
            out.extend(self.hub.frame(&inv::set_entity_flags(
                self.ids.set_entity_data,
                e.entity_id,
                player_flags(false, false, true),
            )));
        }
        let mut cache = self.spawn_frames.lock().unwrap();
        if cache.len() >= SPAWN_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, Arc::new(out.clone()));
        out
    }

    /// Готовый кадр «удалить сущность» для одного получателя.
    ///
    /// Пакет зависит только от id, поэтому собирать и сжимать его на каждого
    /// зрителя незачем: при уходе группы сущностей из виду это десятки
    /// одинаковых zlib-потоков за один тик. Кэш маленький и живёт один тик —
    /// id сущностей переиспользуются, поэтому записи сбрасываются по размеру.
    fn remove_frame(&self, entity_id: i32) -> Vec<u8> {
        {
            let cache = self.remove_frames.lock().unwrap();
            if let Some(hit) = cache.get(&entity_id) {
                return hit.as_ref().clone();
            }
        }
        let frame = self.hub.frame(&play::remove_entities(
            self.ids.remove_entities,
            &[entity_id],
        ));
        let mut cache = self.remove_frames.lock().unwrap();
        if cache.len() >= SPAWN_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(entity_id, Arc::new(frame.clone()));
        frame
    }

    /// Бросает предмет: клиентам — спавн со скоростью, дальше физику ведёт
    /// `tick_items`.
    fn drop_item(&self, from: [f64; 3], vel: [f64; 3], stack: Stack, delay: Duration) {
        let entity_id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let item = Dropped::new(from, vel, stack, delay);
        self.index_drop(entity_id, from);
        self.dropped.lock().unwrap().insert(entity_id, item);
        self.mark_work();
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

    /// `ExperienceOrb.award`: сферы на `amount` очков в точке `pos`.
    fn spawn_orbs(&self, pos: [f64; 3], amount: i32) {
        for value in crate::orbs::award(amount) {
            // `tryMergeToExisting`: та же величина рядом — в неё (+1 к счёту).
            {
                let roll = rand::Rng::gen_range(&mut rand::thread_rng(), 0..40);
                let mut orbs = self.orbs.lock().unwrap();
                if let Some(o) = orbs.iter_mut().find_map(|(&id, o)| {
                    (crate::orbs::touching(pos, o.pos, 0.0)
                        && crate::orbs::can_absorb_spawn(id, roll, value, o))
                    .then_some(o)
                }) {
                    o.count += 1;
                    o.age = 0;
                    continue;
                }
            }
            let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
            let vel = crate::orbs::spawn_velocity(rand::random());
            let orb = crate::orbs::Orb {
                pos,
                vel,
                on_ground: false,
                value,
                count: 1,
                age: 0,
            };
            self.orbs.lock().unwrap().insert(id, orb);
            self.mark_work();
            // Два пакета сферы — одной рассылкой.
            self.hub.send_many(self.orb_packets(id, &orb));
        }
    }

    fn orb_packets(&self, id: i32, o: &crate::orbs::Orb) -> [Vec<u8>; 2] {
        [
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                Uuid::from_u128(id as u128 | 1 << 64),
                self.survival.orb_entity_type,
                o.pos,
                o.vel,
                0.0,
                0.0,
                0,
            ),
            inv::set_orb_value(self.ids.set_entity_data, id, o.value),
        ]
    }

    fn orb_frames(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (&id, o) in self.orbs.lock().unwrap().iter() {
            for p in self.orb_packets(id, o) {
                out.extend(self.hub.frame(&p));
            }
        }
        out
    }

    /// Тик сфер: тяга к ближайшему игроку в 8 блоках, физика, исчезновение.
    fn tick_orbs(&self) {
        let mut orbs = self.orbs.lock().unwrap();
        if orbs.is_empty() {
            return;
        }
        // Наблюдатели и мёртвые сферы не притягивают (`EntitySelector.NO_SPECTATORS`).
        let players: Vec<[f64; 3]> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.collects)
            .map(|p| p.pos)
            .collect();
        let min_y = self.min_y;
        let simulated = self.simulated_set();
        let solid = |p: [i32; 3]| p[1] >= min_y && self.block(p).is_none_or(|b| b != AIR);
        let mut syncs = Vec::new();
        let mut gone = Vec::new();
        for (&id, o) in orbs.iter_mut() {
            // Сфера вне симуляции висит на месте, пока чанк не начнут тикать.
            if !simulated.is_empty() && !simulated.contains(&chunk_of(o.pos)) {
                continue;
            }
            let near = players
                .iter()
                .map(|p| {
                    (
                        p,
                        (p[0] - o.pos[0]).powi(2)
                            + (p[1] - o.pos[1]).powi(2)
                            + (p[2] - o.pos[2]).powi(2),
                    )
                })
                .filter(|(_, d)| *d < crate::orbs::FOLLOW_RANGE.powi(2))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(p, _)| *p);
            if crate::orbs::step(o, near, solid) {
                syncs.push((id, o.pos, o.vel, o.on_ground));
            }
            if o.age >= crate::orbs::LIFETIME_TICKS || o.pos[1] < min_y as f64 - items::VOID_DEPTH {
                gone.push(id);
            }
        }
        for id in &gone {
            orbs.remove(id);
        }
        // `scanForMerges` раз в 20 тиков: соседние сферы той же величины сливаются.
        let ids: Vec<i32> = orbs
            .iter()
            .filter(|(_, o)| o.age % 20 == 1)
            .map(|(&id, _)| id)
            .collect();
        // Сетка по блокам: касание — не дальше 1.0 по осям, хватает соседних клеток.
        let cell = |p: [f64; 3]| p.map(|c| c.floor() as i32);
        let mut grid: FastMap<[i32; 3], Vec<i32>> = FastMap::default();
        if !ids.is_empty() {
            for (&id, o) in orbs.iter() {
                grid.entry(cell(o.pos)).or_default().push(id);
            }
        }
        for a in ids {
            let Some(&oa) = orbs.get(&a) else { continue };
            let c = cell(oa.pos);
            let others: Vec<i32> = (-1..=1)
                .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| [dx, dy, dz])))
                .filter_map(|o| grid.get(&[c[0] + o[0], c[1] + o[1], c[2] + o[2]]))
                .flatten()
                .copied()
                .filter(|&b| {
                    b != a
                        && orbs.get(&b).is_some_and(|ob| {
                            ob.value == oa.value && crate::orbs::touching(oa.pos, ob.pos, 0.5)
                        })
                })
                .collect();
            for b in others {
                if let Some(ob) = orbs.remove(&b) {
                    let o = orbs.get_mut(&a).unwrap();
                    o.count += ob.count;
                    o.age = o.age.min(ob.age);
                    gone.push(b);
                }
            }
        }
        drop(orbs);
        // Синхронизация всех сущностей тика — одной рассылкой.
        let gone_set: HashSet<i32> = gone.iter().copied().collect();
        self.hub.send_many(
            syncs
                .drain(..)
                .filter(|s| !gone_set.contains(&s.0))
                .flat_map(|(id, pos, vel, on_ground)| {
                    [
                        play::teleport_entity(
                            self.ids.teleport_entity,
                            id,
                            pos,
                            0.0,
                            0.0,
                            on_ground,
                        ),
                        play::set_entity_motion(self.ids.set_entity_motion, id, vel),
                    ]
                }),
        );
        if !gone.is_empty() {
            self.hub
                .send(None, play::remove_entities(self.ids.remove_entities, &gone));
        }
    }

    /// Сфера у ног игрока (`ExperienceOrb.playerTouch`): ID, величина, позиция.
    fn orb_near(&self, feet: [f64; 3]) -> Option<(i32, i32, [f64; 3])> {
        self.orbs
            .lock()
            .unwrap()
            .iter()
            .find(|(_, o)| items::in_reach(feet, o.pos))
            .map(|(&id, o)| (id, o.value, o.pos))
    }

    /// Забрать одну сферу из `id` (слитая отдаёт по одной, исчезает на последней).
    fn take_orb(&self, id: i32, collector: i32) -> bool {
        let mut orbs = self.orbs.lock().unwrap();
        let Some(o) = orbs.get_mut(&id) else {
            return false;
        };
        o.count -= 1;
        let last = o.count == 0;
        if last {
            orbs.remove(&id);
        }
        drop(orbs);
        // Клиент на подбор сферы сам играет её звук.
        self.hub.send(
            None,
            inv::take_item_entity(self.ids.take_item_entity, id, collector, 1),
        );
        if last {
            self.hub
                .send(None, play::remove_entities(self.ids.remove_entities, &[id]));
        }
        true
    }

    /// Подбор сферы: `PlayerPickupExperienceEvent` (отмена — сфера остаётся),
    /// Починка с `PlayerItemMendEvent`, остаток через `PlayerExpChangeEvent`.
    /// `true` — сфера забрана.
    async fn absorb_orb(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        collector: i32,
        state: &mut PlayerState,
        (id, value, pos): (i32, i32, [f64; 3]),
    ) -> Result<bool, NetError> {
        if self
            .cancelled(Event::PickupXp, uuid, || {
                format!("{value} {} {} {}", pos[0], pos[1], pos[2])
            })
            .await
        {
            return Ok(false);
        }
        if !self.take_orb(id, collector) {
            return Ok(false);
        }
        let slots = [state.inv.held_slot(), survival::OFFHAND, 5, 6, 7, 8];
        let names = ["HAND", "OFF_HAND", "HEAD", "CHEST", "LEGS", "FEET"];
        let mut left = value;
        let mut changed = Vec::new();
        // `repairPlayerItems`: случайный повреждённый предмет с Починкой, пока есть опыт.
        while left > 0 {
            let open: Vec<usize> = (0..slots.len())
                .filter(|&k| {
                    state.inv.slots[slots[k]].is_some_and(|s| {
                        s.damage > 0
                            && chunkwake_protocol::components::stack_level(
                                s.extra,
                                "minecraft:mending",
                            ) > 0
                    })
                })
                .collect();
            if open.is_empty() {
                break;
            }
            let k = open[rand::Rng::gen_range(&mut rand::thread_rng(), 0..open.len())];
            let Some(mut s) = state.inv.slots[slots[k]] else {
                break;
            };
            let mut repair = crate::orbs::mend_repair(left, s.damage);
            let reply = self
                .plugin_event(Event::Mend, uuid, || {
                    format!(
                        "{} {repair} {left} {}",
                        names[k],
                        crate::stack_nbt::to_line(s, self.item_name(s.item))
                    )
                })
                .await;
            match reply.as_deref().map(str::trim) {
                Some("c") => break,
                Some(r) if !r.is_empty() => {
                    repair = r.parse::<i32>().unwrap_or(repair).clamp(0, s.damage)
                }
                _ => {}
            }
            if repair <= 0 {
                break;
            }
            s.damage -= repair;
            state.inv.slots[slots[k]] = Some(s);
            changed.push(slots[k]);
            left = crate::orbs::mend_left(left, repair);
        }
        if !changed.is_empty() {
            changed.dedup();
            self.send_slots(conn, &mut state.inv, &changed).await?;
        }
        if left > 0
            && let Some(r) = self
                .plugin_event(Event::ExpChange, uuid, || left.to_string())
                .await
            && let Ok(n) = r.trim().parse::<i32>()
        {
            left = n;
        }
        if left != 0 {
            let old = state.xp.level;
            state.xp.add_points(left);
            self.send_xp(conn, &state.xp).await?;
            self.xp_sounds(uuid, old, &state.xp);
        }
        Ok(true)
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
    /// Кладёт предмет в индекс по его чанку. Зовётся при появлении и при
    /// переносе между чанками (предметы летают).
    fn index_drop(&self, id: i32, pos: [f64; 3]) {
        let chunk = (pos[0].floor() as i32 >> 4, pos[2].floor() as i32 >> 4);
        self.dropped_by_chunk
            .lock()
            .unwrap()
            .entry(chunk)
            .or_default()
            .push(id);
    }

    /// Пересобирает индекс «чанк → предметы» по текущему набору.
    fn rebuild_drop_index(&self) {
        let dropped = self.dropped.lock().unwrap();
        let mut cells: Vec<(i32, (i32, i32))> = Vec::with_capacity(dropped.len());
        for (&id, d) in dropped.iter() {
            let chunk = (d.pos[0].floor() as i32 >> 4, d.pos[2].floor() as i32 >> 4);
            cells.push((id, chunk));
        }
        // Замок индекса — только после `dropped`: `drops_near` берёт их в том
        // же порядке, иначе сессия игрока и тик могли бы встать намертво.
        drop(dropped);
        let mut index = self.dropped_by_chunk.lock().unwrap();
        // Карта и векторы переиспользуются: новый `HashMap` на каждый тик —
        // это аллокация на чанк с предметами, то есть на весь бой.
        for ids in index.values_mut() {
            ids.clear();
        }
        for (id, chunk) in cells {
            index.entry(chunk).or_default().push(id);
        }
        index.retain(|_, ids| !ids.is_empty());
    }

    /// Предметы в чанке и его соседях в пределах `r` чанков.
    fn drops_near(&self, pos: [f64; 3], r: i32) -> Vec<(i32, Dropped)> {
        let (cx, cz) = (pos[0].floor() as i32 >> 4, pos[2].floor() as i32 >> 4);
        let index = self.dropped_by_chunk.lock().unwrap();
        let dropped = self.dropped.lock().unwrap();
        let mut out = Vec::new();
        for dx in -r..=r {
            for dz in -r..=r {
                let Some(ids) = index.get(&(cx + dx, cz + dz)) else {
                    continue;
                };
                for &id in ids {
                    if let Some(d) = dropped.get(&id) {
                        out.push((id, *d));
                    }
                }
            }
        }
        out
    }

    /// Предметы, которые этот игрок может подобрать: в радиусе и не на паузе.
    fn pickup_candidates(&self, feet: [f64; 3]) -> Vec<(i32, Stack)> {
        let now = Instant::now();
        // Радиус подбора — 1 блок, то есть не больше двух чанков по стороне.
        self.drops_near(feet, 1)
            .into_iter()
            .filter(|(_, d)| now >= d.pickup_at && items::in_reach(feet, d.pos))
            .map(|(id, d)| (id, d.stack))
            .collect()
    }

    /// Воткнутые стрелы, до которых дотягивается игрок.
    fn arrow_candidates(&self, feet: [f64; 3]) -> Vec<(i32, [f64; 3])> {
        let arrows = self.arrows.lock().unwrap();
        arrows
            .iter()
            .filter(|(_, a)| a.stuck && !a.creative_only && items::in_reach(feet, a.pos))
            .map(|(&id, a)| (id, a.pos))
            .collect()
    }

    /// Воткнутые стрелы у ног (`AbstractArrow.playerTouch`): в инвентарь, если есть место.
    fn pickup_arrows(
        &self,
        collector: i32,
        feet: [f64; 3],
        inv: &mut Inventory,
        vetoed: &HashSet<i32>,
    ) -> Vec<usize> {
        let mut changed = Vec::new();
        let near: Vec<i32> = self
            .arrow_candidates(feet)
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| !vetoed.contains(id))
            .collect();
        let Some(item) = self.item_id("minecraft:arrow").filter(|_| !near.is_empty()) else {
            return changed;
        };
        let mut taken = Vec::new();
        for id in near {
            let mut trial = inv.clone();
            let (slots, left) = trial.add(item, 1, 64);
            if left > 0 || self.arrows.lock().unwrap().remove(&id).is_none() {
                continue;
            }
            *inv = trial;
            changed.extend(slots);
            self.hub.send(
                None,
                inv::take_item_entity(self.ids.take_item_entity, id, collector, 1),
            );
            taken.push(id);
        }
        if !taken.is_empty() {
            self.hub.send(
                None,
                play::remove_entities(self.ids.remove_entities, &taken),
            );
        }
        changed
    }

    fn pickup(
        &self,
        collector: i32,
        feet: [f64; 3],
        inv: &mut Inventory,
        vetoed: &HashSet<i32>,
    ) -> Vec<usize> {
        let mut changed = self.pickup_arrows(collector, feet, inv, vetoed);
        let now = Instant::now();
        let mut taken = Vec::new();
        // Только предметы рядом: подбор идёт в радиусе блока.
        let near: Vec<i32> = self
            .drops_near(feet, 1)
            .into_iter()
            .filter(|(_, d)| {
                now >= d.pickup_at && items::in_reach(feet, d.pos) && !vetoed.contains(&0)
            })
            .map(|(id, _)| id)
            .collect();
        if near.is_empty() {
            return changed;
        }
        let mut dropped = self.dropped.lock().unwrap();
        for id in near {
            let Some(d) = dropped.get_mut(&id) else {
                continue;
            };
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
        // Предметы вне радиуса симуляции стоят: как чанки, которые не тикают.
        let simulated = self.simulated_set();
        let asleep = |d: &crate::items::Dropped| {
            !simulated.is_empty() && !simulated.contains(&chunk_of(d.pos))
        };
        // Один замок мира на фазу: предметы проверяют опору тысячами раз.
        let solid =
            |w: &WorldView<'_>, p: [i32; 3]| p[1] >= min_y && w.block(p).is_none_or(|b| b != AIR);
        // Буферы тика: не новые векторы на каждый вызов, а взятые из запаса
        // мира — на бой с сотнями дропа это сотни аллокаций за тик.
        let mut syncs = std::mem::take(&mut *self.items_syncs.lock().unwrap());
        let mut gone = std::mem::take(&mut *self.items_gone.lock().unwrap());
        // Кого проверять на слияние: летящих раз в 2 тика, лежащих раз в 40 (как ванила).
        let mut merging = std::mem::take(&mut *self.items_merging.lock().unwrap());
        syncs.clear();
        gone.clear();
        merging.clear();
        self.with_world(|w| {
            for (&id, d) in dropped.iter_mut() {
                if asleep(d) {
                    continue;
                }
                if d.resting() {
                    d.age += 1;
                    if d.age.is_multiple_of(40) {
                        merging.push(id);
                    }
                    // Раз в 4 тика: не убрали ли опору.
                    let below = [d.pos[0], d.pos[1] - 0.01, d.pos[2]].map(|c| c.floor() as i32);
                    if !d.age.is_multiple_of(4) || solid(w, below) {
                        if now >= d.despawn_at {
                            gone.push(id);
                        }
                        continue;
                    }
                    d.on_ground = false;
                }
                let hit = items::step(d, |p| solid(w, p));
                if hit || d.age.is_multiple_of(items::SYNC_TICKS) {
                    syncs.push((id, d.pos, d.vel, d.on_ground));
                }
                if d.pos[1] < min_y as f64 - items::VOID_DEPTH || now >= d.despawn_at {
                    gone.push(id);
                } else if d.age.is_multiple_of(2) {
                    merging.push(id);
                }
            }
        });
        for id in &gone {
            dropped.remove(id);
        }
        // Слияние: меньшая стопка уходит в большую, большая остаётся на месте.
        let mut updated = Vec::new();
        // Сетка по блокам: слияние в радиусе < 0.75, хватает соседних клеток (O(n), не O(n²)).
        let cell = |p: [f64; 3]| p.map(|c| c.floor() as i32);
        let mut grid: FastMap<[i32; 3], Vec<i32>> = FastMap::default();
        if !merging.is_empty() {
            for (&id, d) in dropped.iter() {
                grid.entry(cell(d.pos)).or_default().push(id);
            }
        }
        for &a in &merging {
            let Some(&da) = dropped.get(&a) else { continue };
            let max = self
                .survival
                .items
                .get(da.stack.item as usize)
                .map_or(64, |i| i.max_stack) as i32;
            let c = cell(da.pos);
            let other = (-1..=1)
                .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| [dx, dy, dz])))
                .filter_map(|o| grid.get(&[c[0] + o[0], c[1] + o[1], c[2] + o[2]]))
                .flatten()
                .filter(|&&b| b != a)
                .find_map(|&b| {
                    let db = *dropped.get(&b)?;
                    items::can_merge(&da, &db, max).then_some((b, db))
                });
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
        // Индекс по чанкам перестраивается раз в тик: предметы летают между
        // чанками и исчезают, а точечно поддерживать его в пяти местах —
        // лишний риск. Перестройка O(предметов) и без новых аллокаций: карта
        // и её векторы переиспользуются.
        self.rebuild_drop_index();
        // Синхронизация всех сущностей тика — одной рассылкой.
        let gone_set: FastSet<i32> = gone.iter().copied().collect();
        self.hub.send_many(
            syncs
                .drain(..)
                .filter(|s| !gone_set.contains(&s.0))
                .flat_map(|(id, pos, vel, on_ground)| {
                    [
                        play::teleport_entity(
                            self.ids.teleport_entity,
                            id,
                            pos,
                            0.0,
                            0.0,
                            on_ground,
                        ),
                        play::set_entity_motion(self.ids.set_entity_motion, id, vel),
                    ]
                }),
        );
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
        // Буферы возвращаются в запас: следующий тик возьмёт их без аллокаций.
        return_tick_buffer(&self.items_syncs, &mut syncs);
        return_tick_buffer(&self.items_gone, &mut gone);
        return_tick_buffer(&self.items_merging, &mut merging);
    }

    /// Тик стрел: полёт, втыкание, попадание в игроков, подбор воткнутых, исчезновение.
    fn tick_arrows(&self) {
        use crate::arrows as ar;
        if self.arrows.lock().unwrap().is_empty() {
            return;
        }
        let players: Vec<(Uuid, [f64; 3], bool, i32)> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .map(|p| (p.entry.uuid, p.pos, p.collects, p.entity_id))
            .collect();
        let alive: Vec<(Uuid, [f64; 3])> =
            players.iter().filter(|p| p.2).map(|p| (p.0, p.1)).collect();
        let alive_ids: Vec<(Uuid, i32)> =
            players.iter().filter(|p| p.2).map(|p| (p.0, p.3)).collect();
        // Подшаги стрел (~0.1 блока) много раз проверяют одни клетки: ответ — на тик.
        // Мир берётся один раз на фазу: `with_world` ниже держит один замок.
        let solid_seen: std::cell::RefCell<FastMap<[i32; 3], bool>> = Default::default();
        let water_seen: std::cell::RefCell<FastMap<[i32; 3], bool>> = Default::default();
        let solid = |w: &WorldView<'_>, c: [i32; 3]| {
            if let Some(&v) = solid_seen.borrow().get(&c) {
                return v;
            }
            let v = match w.block(c) {
                None => c[1] < self.min_y,
                Some(s) => chunkwake_world::has_flag(s, chunkwake_world::flags::SOLID),
            };
            solid_seen.borrow_mut().insert(c, v);
            v
        };
        let water = |w: &WorldView<'_>, c: [i32; 3]| {
            if let Some(&v) = water_seen.borrow().get(&c) {
                return v;
            }
            let v = w
                .block(c)
                .and_then(|s| crate::fluids::state_fluid(&self.survival.placement, s))
                .is_some_and(|f| f.kind == crate::fluids::Kind::Water);
            water_seen.borrow_mut().insert(c, v);
            v
        };
        let mut gone = Vec::new();
        let mut stuck = Vec::new();
        let mut broken = Vec::new();
        let mut hurt = Vec::new();
        let simulated = self.simulated_set();
        {
            let mut arrows = self.arrows.lock().unwrap();
            self.with_world(|w| {
                for (&id, a) in arrows.iter_mut() {
                    // Стрела в нетикающем чанке висит в воздухе, как и в ванили.
                    if !simulated.is_empty() && !simulated.contains(&chunk_of(a.pos)) {
                        continue;
                    }
                    let punch = a.punch;
                    for hit in ar::step(a, &alive, |c| solid(w, c), |c| water(w, c)) {
                        match hit {
                            ar::Hit::Pop => {
                                broken.push((id, true, a.pos));
                                gone.push(id);
                            }
                            ar::Hit::Block if a.thrown => {
                                broken.push((id, false, a.pos));
                                gone.push(id);
                            }
                            ar::Hit::Block => {
                                self.target_hit(a.pos, [a.vel[0], a.vel[1], a.vel[2]]);
                                stuck.push((id, a.pos));
                            }
                            ar::Hit::Player(k, d, vel) => {
                                if a.thrown {
                                    broken.push((id, false, a.pos));
                                    gone.push(id);
                                } else {
                                    hurt.push((alive_ids[k], d, punch, vel));
                                    if a.stopped {
                                        gone.push(id);
                                    }
                                }
                            }
                        }
                    }
                    if a.stuck {
                        if a.age >= ar::DESPAWN || a.pos[1] < self.min_y as f64 - 64.0 {
                            gone.push(id);
                        }
                    } else if a.pos[1] < self.min_y as f64 - 64.0 {
                        gone.push(id);
                    }
                }
                gone.sort_unstable();
                gone.dedup();
                for id in &gone {
                    arrows.remove(id);
                }
            });
        }
        for (id, rocket, pos) in broken {
            if rocket {
                self.play_sound(
                    None,
                    "minecraft:entity.firework_rocket.blast",
                    crate::sounds::NEUTRAL,
                    pos,
                    1.0,
                    1.0,
                );
            }
            self.hub.send(
                None,
                inv::entity_event(self.ids.entity_event, id, crate::arrows::EVENT_BREAK),
            );
        }
        for (id, pos) in stuck {
            self.hub.send(
                None,
                play::teleport_entity(self.ids.teleport_entity, id, pos, 0.0, 0.0, false),
            );
            self.hub.send(
                None,
                play::set_entity_motion(self.ids.set_entity_motion, id, [0.0; 3]),
            );
            self.block_sound(
                None,
                "minecraft:entity.arrow.hit",
                pos.map(|c| c.floor() as i32),
                1.0,
                1.0,
            );
        }
        for ((uuid, entity), d, punch, vel) in hurt {
            self.push_control(uuid, Control::Hurt(d, Cause::Arrow));
            if punch > 0 {
                let h = (vel[0] * vel[0] + vel[2] * vel[2]).sqrt();
                if h > 1e-6 {
                    let s = punch as f64 * 0.6;
                    self.hub.send(
                        None,
                        play::set_entity_motion(
                            self.ids.set_entity_motion,
                            entity,
                            [vel[0] / h * s, 0.1, vel[2] / h * s],
                        ),
                    );
                }
            }
        }
        if !gone.is_empty() {
            self.hub
                .send(None, play::remove_entities(self.ids.remove_entities, &gone));
        }
    }

    /// Выпустить снаряд (стрела, снежок, яйцо): сущность у клиентов, полёт — в `tick_arrows`.
    fn shoot_arrow(&self, pos: [f64; 3], vel: [f64; 3], entity_type: i32, thrown: bool) {
        self.launch(entity_type, crate::arrows::Arrow::flight(pos, vel, thrown));
    }

    fn hand_index(inv: &survival::Inventory, hand: i32) -> Option<usize> {
        match hand {
            0 => Some(inv.held_slot()),
            1 => Some(survival::OFFHAND),
            _ => None,
        }
    }

    /// Стрела или (для арбалета) ракета: вторая рука, основная, хотбар, остальной инвентарь.
    fn projectile_slot(&self, inv: &survival::Inventory, crossbow: bool) -> Option<usize> {
        let want = |s: Stack| {
            let n = self.item_name(s.item);
            n == "minecraft:arrow" || (crossbow && n == "minecraft:firework_rocket")
        };
        let held = inv.held_slot();
        let mut order = vec![survival::OFFHAND, held];
        order.extend((36..45).filter(|i| *i != held));
        order.extend(9..36);
        order
            .into_iter()
            .find(|&i| inv.slots.get(i).and_then(|s| *s).is_some_and(want))
    }

    fn wear_one(&self, stack: &mut Stack) -> bool {
        let max = self
            .survival
            .items
            .get(stack.item as usize)
            .map_or(0, |i| i.max_damage);
        if max <= 0 {
            return false;
        }
        let unbreaking =
            chunkwake_protocol::components::stack_level(stack.extra, "minecraft:unbreaking");
        if unbreaking != 0 && !rand::random::<u32>().is_multiple_of(unbreaking as u32 + 1) {
            return false;
        }
        stack.damage += 1;
        stack.damage >= max
    }

    fn break_in_hand(&self, entity: i32, offhand: bool) {
        let event = if offhand {
            inv::ENTITY_EVENT_BREAK_OFFHAND
        } else {
            inv::ENTITY_EVENT_BREAK_MAINHAND
        };
        self.hub.send(
            None,
            inv::entity_event(self.ids.entity_event, entity, event),
        );
    }

    fn launch(&self, entity_type: i32, arrow: crate::arrows::Arrow) {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let (pos, vel) = (arrow.pos, arrow.vel);
        self.arrows.lock().unwrap().insert(id, arrow);
        self.mark_work();
        let (yaw, pitch) = crate::arrows::rotation(vel);
        self.hub.send(
            None,
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                Uuid::from_u128(rand::random()),
                entity_type,
                pos,
                vel,
                pitch,
                yaw,
                0,
            ),
        );
    }

    fn shoot_owned(&self, shot: OwnedShot) {
        let dir = crate::arrows::aim(shot.yaw as f64, shot.pitch as f64, shot.yaw_offset);
        let spread = if shot.rocket {
            0.0
        } else {
            crate::arrows::BOW_SPREAD
        };
        let vel = crate::arrows::shoot(dir, shot.speed, spread, rand::random());
        let mut arrow = crate::arrows::Arrow::flight(shot.eye, vel, false);
        arrow.owner = Some(shot.uuid);
        arrow.left_owner = false;
        arrow.base_damage = shot.base;
        arrow.crit = shot.crit;
        arrow.punch = shot.punch;
        arrow.pierce = shot.pierce;
        arrow.firework = shot.rocket;
        arrow.creative_only = shot.creative_only;
        let ty = if shot.rocket {
            self.survival.firework_entity_type
        } else {
            self.survival.arrow_entity_type
        };
        self.launch(ty, arrow);
    }

    /// Правый клик по луку или арбалету. Заряженный арбалет стреляет сразу.
    async fn use_ranged(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        inv: &mut survival::Inventory,
        hand: i32,
        mode: u8,
    ) -> Result<RangedUse, NetError> {
        if mode == 3 {
            return Ok(RangedUse::Ignore);
        }
        let Some(slot) = Self::hand_index(inv, hand) else {
            return Ok(RangedUse::Ignore);
        };
        let Some(stack) = inv.slots[slot] else {
            return Ok(RangedUse::Ignore);
        };
        match self.item_name(stack.item) {
            "minecraft:crossbow"
                if !chunkwake_protocol::components::get(stack.extra)
                    .charged
                    .is_empty() =>
            {
                self.fire_crossbow(conn, uuid, entity, inv, slot, mode)
                    .await?;
                Ok(RangedUse::Done)
            }
            "minecraft:crossbow" | "minecraft:bow" => Ok(RangedUse::Draw(slot)),
            _ => Ok(RangedUse::Ignore),
        }
    }

    /// Отпускание: лук стреляет, арбалет вкладывает снаряд, если зарядка дошла.
    #[allow(clippy::too_many_arguments)]
    async fn release_draw(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        inv: &mut survival::Inventory,
        slot: usize,
        start: u64,
        now: u64,
        mode: u8,
    ) -> Result<(), NetError> {
        let Some(stack) = inv.slots[slot] else {
            return Ok(());
        };
        let ticks = now.saturating_sub(start) as u32;
        match self.item_name(stack.item) {
            "minecraft:bow" => {
                self.release_bow(conn, uuid, entity, inv, slot, ticks, mode)
                    .await
            }
            "minecraft:crossbow" => {
                let quick = chunkwake_protocol::components::stack_level(
                    stack.extra,
                    "minecraft:quick_charge",
                );
                if ticks < crate::arrows::crossbow_charge(quick) {
                    return Ok(());
                }
                self.load_crossbow(conn, uuid, inv, slot, mode).await
            }
            _ => Ok(()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn release_bow(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        inv: &mut survival::Inventory,
        slot: usize,
        ticks: u32,
        mode: u8,
    ) -> Result<(), NetError> {
        let power = crate::arrows::bow_power(ticks);
        if power < 0.1 {
            return Ok(());
        }
        let Some(stack) = inv.slots[slot] else {
            return Ok(());
        };
        let creative = mode == 1;
        let infinity =
            chunkwake_protocol::components::stack_level(stack.extra, "minecraft:infinity") > 0;
        let ammo = self.projectile_slot(inv, false);
        if ammo.is_none() && !creative {
            return Ok(());
        }
        let plain = ammo.is_none_or(|i| {
            inv.slots[i].is_some_and(|s| self.item_name(s.item) == "minecraft:arrow")
        });
        let ammo_name = ammo
            .and_then(|i| inv.slots[i])
            .map_or("minecraft:arrow", |s| self.item_name(s.item));
        let Some(consume) = self
            .shoot_bow_event(
                uuid,
                stack.item,
                ammo_name,
                power,
                slot,
                !creative && !(infinity && plain),
            )
            .await
        else {
            return self.send_slots(conn, inv, &[slot]).await;
        };
        let mut changed = vec![slot];
        if let Some(i) = ammo.filter(|_| consume) {
            inv.consume_at(i);
            changed.push(i);
        }
        let Some((feet, yaw, pitch)) = self.player_at(uuid) else {
            return Ok(());
        };
        let level = chunkwake_protocol::components::stack_level(stack.extra, "minecraft:power");
        let base = if level <= 0 {
            crate::arrows::BASE_DAMAGE
        } else {
            crate::arrows::BASE_DAMAGE + 0.5 + 0.5 * level as f64
        };
        let punch = chunkwake_protocol::components::stack_level(stack.extra, "minecraft:punch");
        self.shoot_owned(OwnedShot {
            uuid,
            eye: eye_of(feet),
            yaw,
            pitch,
            yaw_offset: 0.0,
            speed: crate::arrows::BOW_SPEED * power,
            base,
            crit: power >= 1.0,
            punch,
            pierce: 0,
            rocket: false,
            creative_only: !consume,
        });
        if !creative && let Some(mut s) = inv.slots[slot] {
            let broke = self.wear_one(&mut s);
            inv.slots[slot] = (!broke).then_some(s);
            if broke {
                self.break_in_hand(entity, slot == survival::OFFHAND);
            }
        }
        let pitch_s = 1.0 / (rand::random::<f32>() * 0.4 + 1.2) + power as f32 * 0.5;
        self.player_sound(uuid, "minecraft:entity.arrow.shoot", 1.0, pitch_s);
        changed.sort_unstable();
        changed.dedup();
        self.send_slots(conn, inv, &changed).await
    }

    async fn load_crossbow(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        inv: &mut survival::Inventory,
        slot: usize,
        mode: u8,
    ) -> Result<(), NetError> {
        let Some(stack) = inv.slots[slot] else {
            return Ok(());
        };
        let creative = mode == 1;
        let ammo = self.projectile_slot(inv, true);
        let projectile = if let Some(i) = ammo {
            let Some(s) = inv.slots[i] else {
                return Ok(());
            };
            if !creative {
                inv.consume_at(i);
            }
            Stack { count: 1, ..s }
        } else if creative && let Some(id) = self.item_id("minecraft:arrow") {
            Stack::new(id, 1)
        } else {
            return Ok(());
        };
        let mut next = stack;
        next.extra = charged_extra(stack.extra, vec![projectile]);
        inv.slots[slot] = Some(next);
        let mut changed = vec![slot];
        if let Some(i) = ammo.filter(|_| !creative) {
            changed.push(i);
        }
        self.player_sound(uuid, "minecraft:item.crossbow.loading_end", 1.0, 1.0);
        self.send_slots(conn, inv, &changed).await
    }

    async fn fire_crossbow(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        inv: &mut survival::Inventory,
        slot: usize,
        mode: u8,
    ) -> Result<(), NetError> {
        let Some(stack) = inv.slots[slot] else {
            return Ok(());
        };
        let loaded = chunkwake_protocol::components::get(stack.extra)
            .charged
            .clone();
        if loaded.is_empty() {
            return Ok(());
        }
        let Some((feet, yaw, pitch)) = self.player_at(uuid) else {
            return Ok(());
        };
        // Снаряд арбалета уже потрачен при зарядке: `setConsumeItem` тут ни на что не влияет.
        if self
            .shoot_bow_event(
                uuid,
                stack.item,
                self.item_name(loaded[0].item),
                1.0,
                slot,
                true,
            )
            .await
            .is_none()
        {
            return self.send_slots(conn, inv, &[slot]).await;
        }
        let multi =
            chunkwake_protocol::components::stack_level(stack.extra, "minecraft:multishot") > 0;
        let pierce = chunkwake_protocol::components::stack_level(stack.extra, "minecraft:piercing");
        let offsets: &[f64] = if multi {
            &crate::arrows::MULTISHOT_YAW
        } else {
            &[0.0]
        };
        let eye = eye_of(feet);
        for ammo in &loaded {
            let name = self.item_name(ammo.item);
            let rocket = name == "minecraft:firework_rocket";
            let speed = if rocket {
                crate::arrows::CROSSBOW_ROCKET
            } else {
                crate::arrows::CROSSBOW_ARROW
            };
            for &off in offsets {
                self.shoot_owned(OwnedShot {
                    uuid,
                    eye,
                    yaw,
                    pitch,
                    yaw_offset: off,
                    speed,
                    base: crate::arrows::BASE_DAMAGE,
                    crit: false,
                    punch: 0,
                    pierce: if rocket { 0 } else { pierce },
                    rocket,
                    // Мультивыстрел: боковые стрелы не подбираются (`Projectile.pickup`).
                    creative_only: off != 0.0 || mode == 1,
                });
            }
        }
        let mut next = stack;
        next.extra = charged_extra(stack.extra, Vec::new());
        let creative = mode == 1;
        if !creative && self.wear_one(&mut next) {
            inv.slots[slot] = None;
            self.break_in_hand(entity, slot == survival::OFFHAND);
        } else {
            inv.slots[slot] = Some(next);
        }
        self.player_sound(uuid, "minecraft:item.crossbow.shoot", 1.0, 1.0);
        self.send_slots(conn, inv, &[slot]).await
    }

    /// `EntityShootBowEvent` перед выстрелом: `None` — плагин отменил,
    /// иначе тратить ли снаряд (`setConsumeItem`).
    async fn shoot_bow_event(
        &self,
        uuid: Uuid,
        weapon: i32,
        ammo: &str,
        force: f64,
        slot: usize,
        consume: bool,
    ) -> Option<bool> {
        let hand = if slot == survival::OFFHAND {
            "OFF_HAND"
        } else {
            "HAND"
        };
        let weapon = self.item_name(weapon);
        let c = u8::from(consume);
        match self
            .plugin_event(Event::ShootBow, uuid, || {
                format!("{weapon} {ammo} {force} {hand} {c}")
            })
            .await
            .as_deref()
            .map(str::trim)
        {
            Some("c") => None,
            Some("1") => Some(true),
            Some("0") => Some(false),
            _ => Some(consume),
        }
    }

    /// Инвентари плагинов: `new <размер> chest|hopper|dispenser` → номер,
    /// `get <id>` → строки слотов (`-` — пусто), `set <id> <слот> <стопка|->`.
    fn plugin_inv_request(&self, line: &str) -> String {
        let mut it = line.splitn(4, ' ');
        let cmd = it.next().unwrap_or("");
        let num = |s: Option<&str>| s.and_then(|v| v.trim().parse::<i32>().ok());
        match cmd {
            "new" => {
                let Some(size) = num(it.next()).filter(|&n| n > 0 && n <= 54) else {
                    return String::new();
                };
                let menu = match it.next().map(str::trim) {
                    Some("hopper") if size == 5 => self.survival.menu_hopper,
                    Some("dispenser") if size == 9 => self.survival.menu_3x3,
                    _ if size % 9 == 0 => self.survival.menu_rows[size as usize / 9 - 1],
                    _ => return String::new(),
                };
                let id = self.next_plugin_inv.fetch_add(1, Ordering::Relaxed);
                self.plugin_invs.lock().unwrap().insert(
                    id,
                    PluginInv {
                        slots: vec![None; size as usize],
                        menu,
                    },
                );
                id.to_string()
            }
            "get" => {
                let Some(id) = num(it.next()) else {
                    return String::new();
                };
                let invs = self.plugin_invs.lock().unwrap();
                let Some(p) = invs.get(&id) else {
                    return String::new();
                };
                p.slots
                    .iter()
                    .map(|s| match s {
                        Some(s) => crate::stack_nbt::to_line(*s, self.item_name(s.item)),
                        None => "-".to_owned(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            "set" => {
                let (Some(id), Some(slot), Some(rest)) =
                    (num(it.next()), num(it.next()), it.next())
                else {
                    return "0".into();
                };
                let stack = match rest.trim() {
                    "-" => None,
                    l => match crate::stack_nbt::from_line(l, |n| self.item_id(n)) {
                        Some(s) => Some(s),
                        None => return "0".into(),
                    },
                };
                {
                    let mut invs = self.plugin_invs.lock().unwrap();
                    let Some(cell) = invs
                        .get_mut(&id)
                        .and_then(|p| p.slots.get_mut(slot as usize))
                    else {
                        return "0".into();
                    };
                    *cell = stack;
                }
                let pos = plugin_pos(id);
                for tx in self.controls.lock().unwrap().values() {
                    let _ = tx.send(Control::ContainerChanged(pos));
                }
                "1".into()
            }
            _ => String::new(),
        }
    }

    /// Строка стопки для плагинов (`-` — пусто).
    fn slot_line(&self, s: Slot) -> String {
        s.map_or_else(
            || "-".to_owned(),
            |s| crate::stack_nbt::to_line(s, self.item_name(s.item)),
        )
    }

    /// `InventoryClickEvent` до применения клика; `true` — плагин отменил.
    async fn click_cancelled(
        &self,
        uuid: Uuid,
        inv: &Inventory,
        window: Option<&Window>,
        c: &inv::ContainerClick,
    ) -> bool {
        if c.mode == 5
            || !self
                .hooks()
                .is_some_and(|h| h.listens(Event::InventoryClick))
        {
            return false;
        }
        let (ty, pid, top, current) = if c.window == inv::PLAYER_WINDOW {
            let cur = usize::try_from(c.slot)
                .ok()
                .and_then(|i| inv.slots.get(i).copied().flatten());
            ("CRAFTING", -1, 5, cur)
        } else if let Some(w) = window.filter(|w| w.id == c.window) {
            let n = w.slots.len();
            let cur = usize::try_from(c.slot).ok().and_then(|i| {
                if i < n {
                    w.slots[i]
                } else {
                    inv.slots.get(9 + i - n).copied().flatten()
                }
            });
            let ty = match w.kind {
                Kind::Chest(5) => "HOPPER",
                Kind::Chest(9) if plugin_of(w.pos).is_none() => "DISPENSER",
                Kind::Chest(_) => "CHEST",
                Kind::Crafting => "WORKBENCH",
                Kind::Furnace => "FURNACE",
                Kind::Enchanting => "ENCHANTING",
                Kind::Grindstone => "GRINDSTONE",
                Kind::Player => "CRAFTING",
            };
            (ty, plugin_of(w.pos).unwrap_or(-1), n, cur)
        } else {
            return false;
        };
        let (cur, cursor) = (self.slot_line(current), self.slot_line(inv.carried));
        self.cancelled(Event::InventoryClick, uuid, || {
            format!(
                "{ty} {pid} {top} {} {} {}\n{cur}\n{cursor}",
                c.slot, c.button, c.mode
            )
        })
        .await
    }

    /// Закрыть окно; для окна плагина — `InventoryCloseEvent`.
    async fn close_with_event(&self, inv: &mut Inventory, window: &mut Option<Window>, uuid: Uuid) {
        if let Some(pid) = window.as_ref().and_then(|w| plugin_of(w.pos)) {
            self.plugin_event(Event::InventoryClose, uuid, || pid.to_string())
                .await;
        }
        self.close_window(inv, window, uuid);
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

    /// Один тик мира. Отдельный метод, а не тело `spawn_tick`: его зовут и
    /// бенчмарки, и тесты, и по нему же видно, где стоит ранний выход простоя.
    ///
    /// Профиль по фазам снимается, когда он включён (`/debug start` или
    /// внешний приёмник). Каждый `lap` — это чтение часов, и на простое он
    /// дороже самого тика, поэтому в выключенном состоянии фаз не меряем.
    pub fn tick_once(&self) {
        use chunkwake_metrics::profile::*;
        let started = Instant::now();
        if !self.profiling.load(Ordering::Relaxed) {
            if self.idle() {
                self.advance_time();
                return;
            }
            self.run_phases(None);
            self.recompute_work();
            self.record_tick(started, None);
            return;
        }
        let mut watch = Stopwatch::start();
        // Простой: никого нет и все очереди пусты — тик сводится к времени.
        if self.idle() {
            self.advance_time();
            // Вся работа такого тика — часы мира: подсистемы не запускались,
            // и приписывать время им нельзя.
            watch.lap(PH_TIME);
            self.record_tick(started, Some(&watch));
            return;
        }
        self.run_phases(Some(&mut watch));
        // Работа кончилась? Следующий тик снова станет простым: пересчёт
        // здесь, потому что только здесь видны все очереди после фаз.
        self.recompute_work();
        watch.lap(PH_TIME);
        self.record_tick(started, Some(&watch));
    }

    /// Все подсистемы за тик; с профилем — с отметкой времени после каждой.
    fn run_phases(&self, mut watch: Option<&mut chunkwake_metrics::profile::Stopwatch>) {
        use chunkwake_metrics::profile::*;
        let lap = |phase: usize, watch: &mut Option<&mut chunkwake_metrics::profile::Stopwatch>| {
            if let Some(w) = watch.as_deref_mut() {
                w.lap(phase);
            }
        };
        self.refresh_simulated();
        lap(PH_REFRESH, &mut watch);
        self.tick_items();
        lap(PH_ITEMS, &mut watch);
        self.tick_arrows();
        lap(PH_ARROWS, &mut watch);
        self.tick_mobs();
        lap(PH_MOBS, &mut watch);
        self.tick_animals();
        lap(PH_ANIMALS, &mut watch);
        self.tick_orbs();
        lap(PH_ORBS, &mut watch);
        self.tick_furnaces();
        lap(PH_FURNACES, &mut watch);
        self.tick_campfires();
        lap(PH_CAMPFIRES, &mut watch);
        {
            let _edit = self.world_edit.lock().unwrap();
            self.tick_redstone(&mut watch);
        }
        self.flush_blocks();
        lap(PH_BLOCKS, &mut watch);
        self.flush_moves();
        lap(PH_MOVES, &mut watch);
        if let Some(h) = self.hooks() {
            h.tick();
        }
        lap(PH_PLUGINS, &mut watch);
        self.advance_time();
    }

    /// Замер тика: MSPT, интервал, профиль `/debug` и профиль по фазам.
    ///
    /// Интервал считается между **началами** тиков: `started` — это момент
    /// старта текущего тика, поэтому `last_interval_ms` вызывается первым,
    /// до добавления замера.
    fn record_tick(&self, started: Instant, watch: Option<&chunkwake_metrics::profile::Stopwatch>) {
        let now = Instant::now();
        let work = now.duration_since(started).as_secs_f64() * 1000.0;
        let mut ticks = self.ticks.lock().unwrap();
        let gap = ticks.last_interval_ms(started);
        ticks.record(work, gap);
        let tick = ticks.total();
        drop(ticks);
        let Some(watch) = watch else { return };
        // Профиль включают командой `/debug start`: без него замок не берётся
        // вовсе (на пустом сервере это половина цены тика).
        if let Some(profile) = self.debug_profile.lock().unwrap().as_mut() {
            profile.write(tick, work);
        }
        // Секунда профиля — по тем же часам, что и снимок метрик.
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Some(sink) = self.profile_sink.lock().unwrap().as_ref() {
            sink(at, &watch.marks());
        }
    }

    /// Ничего не может случиться в этом тике: нет игроков, сущностей и
    /// отложенной работы. `advance_time` всё равно идёт — часы мира должны
    /// тикать, даже когда в мире никого нет.
    ///
    /// Проверка — одно атомарное чтение: флаг ставят все, кто добавляет работу,
    /// а в конце каждого рабочего тика он пересчитывается
    /// ([`Self::recompute_work`]). Пробовать здесь десяток мьютексов значило
    /// платить за простой больше, чем за сам тик.
    fn idle(&self) -> bool {
        !self.work_pending.load(Ordering::Relaxed) && self.hooks().is_none()
    }

    /// Отмечает, что появилась работа (сущность, игрок, отложенный тик).
    fn mark_work(&self) {
        self.work_pending.store(true, Ordering::Relaxed);
    }

    /// Пересчитывает флаг по фактическому содержимому мира: зовётся в конце
    /// рабочего тика, когда все фазы уже отработали.
    fn recompute_work(&self) {
        let busy = !self.hub.players.lock().unwrap().is_empty()
            || !self.forced.lock().unwrap().is_empty()
            || !self.mobs.lock().unwrap().is_empty()
            || !self.mobs_loaded.lock().unwrap().is_empty()
            || !self.dropped.lock().unwrap().is_empty()
            || !self.arrows.lock().unwrap().is_empty()
            || !self.orbs.lock().unwrap().is_empty()
            || !self.furnaces.lock().unwrap().is_empty()
            || !self.pending_furnaces.lock().unwrap().is_empty()
            || !self.campfires.lock().unwrap().is_empty()
            || !self.pending_campfires.lock().unwrap().is_empty()
            || !self.hopper_ticks.lock().unwrap().is_empty()
            || !self.redstone_ticks.lock().unwrap().is_empty()
            || !self.fluid_ticks.lock().unwrap().is_empty()
            || !self.deferred_updates.lock().unwrap().is_empty()
            || !self.piston_moves.lock().unwrap().is_empty()
            || !self.plates.lock().unwrap().is_empty()
            || !self.torch_toggles.lock().unwrap().is_empty();
        self.work_pending.store(busy, Ordering::Relaxed);
    }

    /// Запускает тик рассылки движений: раз в 50 мс одна пачка на всех.
    ///
    /// Скорость, остановку и шаги задают `/tick rate`, `/tick freeze` и
    /// `/tick step`: интервал пересчитывается, когда меняется скорость, а
    /// замороженный сервер тикает только по шагам.
    pub fn spawn_tick(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let plan = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut interval = TICK;
            let mut tick = tokio::time::interval(interval);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut rate = 20.0;
            loop {
                tick.tick().await;
                let Some(plan) = plan.upgrade() else { return };
                // Скорость и заморозка живут в замерах: там же их видят команды.
                let (new_rate, frozen, step) = {
                    let mut ticks = plan.ticks.lock().unwrap();
                    let new_rate = ticks.rate();
                    let step = ticks.take_step();
                    (new_rate, ticks.frozen(), step)
                };
                if (new_rate - rate).abs() > f64::EPSILON {
                    rate = new_rate;
                    interval = Duration::from_secs_f64(1.0 / rate);
                    tick = tokio::time::interval(interval);
                    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                }
                if frozen && !step {
                    continue;
                }
                plan.tick_once();
            }
        })
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
            .map(|([sx, sy, sz], mut blocks)| {
                // Последняя запись позиции побеждает: промежуточные не рассылаются.
                // Битсет вместо `HashSet`: локальный индекс секции — 0..4096,
                // и хэшировать его на каждый блок дороже самой проверки.
                if blocks.len() > 1 {
                    let mut seen = [0u64; 64];
                    blocks.reverse();
                    blocks.retain(|(local, _)| {
                        let bit = &mut seen[*local as usize >> 6];
                        let mask = 1u64 << (*local as usize & 63);
                        let fresh = *bit & mask == 0;
                        *bit |= mask;
                        fresh
                    });
                    blocks.reverse();
                }
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
        // Под замком — только сборка пакетов: сжатие zlib ниже, вне его.
        // Раньше `hub.frame` (полный zlib) шёл под `players`, и каждая сессия,
        // зовущая `moved()`, ждала чужое сжатие.
        let mut raw = Vec::new();
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
                let head = std::mem::take(&mut p.rotated)
                    .then(|| play::rotate_head(ids.rotate_head, e.entity_id, e.yaw));
                raw.push((e, packet, head));
            }
        }
        for (e, packet, head) in raw {
            let mut frame = self.hub.frame(&packet);
            if let Some(p) = head {
                frame.extend(self.hub.frame(&p));
            }
            batch.push(MoveItem { e, frame });
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

    /// Точка спавна и стартовая граница мира. Xaero ждёт оба пакета:
    /// спавн задаёт мир карты, граница включает локальную отрисовку.
    async fn send_map_markers(&self, conn: &mut Connection) -> Result<(), NetError> {
        // `/setworldspawn` важнее спавна создания мира: карта и возрождение
        // должны идти за ним.
        let here = self.vars.world_spawn().unwrap_or(self.spawn);
        let pos = [
            here[0].floor() as i32,
            here[1].floor() as i32,
            here[2].floor() as i32,
        ];
        conn.send(&play::set_default_spawn_position(
            self.ids.set_default_spawn_position,
            &self.login.spawn.dimension,
            pos,
            0.0,
            0.0,
        ))
        .await?;
        let now = self.game_time.load(Ordering::Relaxed);
        let border = self.vars.border_at(now).login(now);
        conn.send(&play::initialize_border_state(
            self.ids.initialize_border,
            border,
        ))
        .await?;
        Ok(())
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
        // Спавн и граница — до чанков: Xaero по ним выбирает мир и включает
        // локальную карту. Без спавна тайлы так и не пишутся.
        self.send_map_markers(conn).await?;
        for packet in self.joined_packets(uuid) {
            conn.send(&packet).await?;
        }
        conn.send(&play::player_abilities(
            ids.player_abilities,
            login.spawn.game_mode,
            self.allow_flight.load(Ordering::Relaxed),
        ))
        .await?;
        conn.send(&play::set_time(
            ids.set_time,
            self.game_time.load(Ordering::Relaxed),
            self.day_time.load(Ordering::Relaxed),
            true,
        ))
        .await?;
        conn.send(&play::change_difficulty(
            ids.change_difficulty,
            self.difficulty().id(),
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
        let mut center = (cx, cz);
        let mut stream = stream::ChunkStream::new(center, r_view);
        self.stream_start(conn, &mut stream).await?;
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
        out.extend(self.orb_frames());
        out.extend(self.mob_frames());
        if !out.is_empty() {
            conn.send_frames(&out).await?;
        }

        // Keep-alive: шлём раз в 15 с, ответ ждём не дольше 30 с.
        let mut ticker = tokio::time::interval(KEEP_ALIVE_INTERVAL);
        ticker.tick().await;
        // `/setidletimeout`: минуты без игровых пакетов. Ноль — без таймаута.
        // Ответ keep-alive не считается действием.
        let mut last_action = Instant::now();
        // Выживание: инвентарь, здоровье. Креатив (1) и наблюдатель (3) урона не получают.
        let mut survival = matches!(login.spawn.game_mode, 0 | 2);
        state.vitals.reset_fall(me.pos);
        let mut saved_ticks: u32 = 0;
        // `takeXpDelay`: сферы — не чаще раза в 2 тика.
        let mut xp_delay: u32 = 0;
        let mut last_void: Option<Instant> = None;
        let mut teleport_id = 1;
        // Поедание: слот хотбара, еда и момент окончания.
        let mut eating: Option<(usize, chunkwake_data::world::Food, Instant)> = None;
        let mut drawing: Option<(usize, u64)> = None;
        // Питьё зелья: слот, предмет, компоненты и момент окончания.
        let mut drinking: Option<(usize, i32, u32, Instant)> = None;
        // Открытое окно (сундук, верстак) и счётчик номеров окон.
        let mut window: Option<Window> = None;
        let mut next_window = 0;
        // Шифт из `player_input` (`isSecondaryUseActive`).
        let mut sneaking = false;
        // Полёт на элитрах. Клиент включает его сам и гасит только по метаданным.
        let mut gliding = false;
        // Креатив/наблюдатель: клиент шлёт `player_abilities`, пока бит включён элитры нельзя.
        let mut flying = login.spawn.game_mode == 3;
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
        self.send_xp(conn, &state.xp).await?;
        let mut motion = 0u8;
        for e in &state.vitals.effects {
            conn.send(&inv::update_mob_effect(
                ids.update_mob_effect,
                me.entity_id,
                e.id,
                e.amplifier as i32,
                e.duration,
                e.flags(),
            ))
            .await?;
            motion |= self.motion_mask(e.id);
        }
        self.sync_motion(conn, me.entry.uuid, me.entity_id, &state.vitals, motion)
            .await?;
        let mut pending: Option<(i64, Instant)> = None;
        // Атрибуты от чар снаряжения, последние отправленные клиенту,
        // и шесть слотов (предмет, extra), по которым карта не пересчитывается.
        let mut ench_attrs = Default::default();
        let mut ench_gear = [(-1, 0); 6];
        // Ломка (`ServerPlayerGameMode`): тики сессии, текущий блок и отложенная
        // ломка (клиент закончил раньше, чем позволяет прогресс).
        let mut game_ticks: u64 = 0;
        let mut digging: Option<([i32; 3], u64)> = None;
        let mut delayed: Option<([i32; 3], u64)> = None;
        // Трещины, которые видят другие игроки (`destroyBlockProgress`).
        let mut crack: Option<([i32; 3], i8)> = None;
        loop {
            // Всё, что накопила прошлая итерация, — одной записью (вне `select!`:
            // прерванная запись порвала бы поток).
            let before = conn.written();
            conn.flush().await?;
            let written = conn.written() - before;
            if written > 0 {
                self.count_bytes_out(written, 1);
            }
            tokio::select! {
                Some(pos) = stream.ready.recv() => {
                    self.stream_ready(&mut stream, pos);
                }
                Some(frames) = stream.lit.recv() => {
                    self.stream_lit(conn, &mut stream, frames).await?;
                }
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
                    let idle = self.vars.idle_timeout();
                    if idle > 0 && last_action.elapsed() >= Duration::from_secs(idle as u64 * 60) {
                        return Err(NetError::Timeout);
                    }
                }
                Some(c) = controls.recv() => {
                    match c {
                        Control::Shutdown => return Ok(()),
                        Control::Kick(_) => return Ok(()),
                        Control::Transfer(host, port) => {
                            conn.send(&play::transfer(ids.transfer, &host, port)).await?;
                            conn.flush().await?;
                            return Ok(());
                        }
                        Control::Give {
                            item,
                            count,
                            extra,
                            damage,
                        } => {
                            let max = self.max_stack(item);
                            let (changed, _) =
                                state.inv.add_components(item, count, max, extra, damage);
                            self.send_slots(conn, &mut state.inv, &changed).await?;
                        }
                        Control::Hurt(d, cause) => {
                            if !state.vitals.dead && matches!(login.spawn.game_mode, 0 | 2) {
                                self.hurt(conn, me, &mut state.vitals, &mut state.inv, &mut state.xp, d, cause).await?;
                            }
                        }
                        Control::Health(h) => {
                            if !state.vitals.dead {
                                state.vitals.health = h.min(survival::MAX_HEALTH);
                                self.send_health(conn, &state.vitals).await?;
                                if h <= 0.0 && state.vitals.damage(0.0) {
                                    self.die(conn, me, &mut state.inv, &mut state.xp, Cause::Generic).await?;
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
                            flying = mode == 3;
                            self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding)
                                .await?;
                            self.send_health(conn, &state.vitals).await?;
                        }
                        Control::Slot(w, s) => {
                            state.inv.slots[w] = s;
                            self.send_slots(conn, &mut state.inv, &[w]).await?;
                            self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding).await?;
                        }
                        Control::OpenPlugin(pid, title) => {
                            let menu = self.plugin_invs.lock().unwrap().get(&pid).map(|p| (p.slots.len(), p.menu));
                            if let Some((size, menu)) = menu.filter(|_| !state.vitals.dead) {
                                if let Some(w) = window.as_ref() {
                                    conn.send(&inv::container_close(ids.container_close, w.id)).await?;
                                }
                                self.close_with_event(&mut state.inv, &mut window, uuid).await;
                                next_window = next_window % 100 + 1;
                                let pos = plugin_pos(pid);
                                let mut w = Window { id: next_window, kind: Kind::Chest(size), pos, second: None, slots: Vec::new(), data: [0; 4], enchant: [i32::MIN; 10], offer: None };
                                w.slots = self.window_slots(&w);
                                let title = serde_json::from_str(&title).unwrap_or_else(|_| serde_json::json!({"translate": "container.chest"}));
                                conn.send(&inv::open_screen(ids.open_screen, w.id, menu, &title)).await?;
                                self.send_window(conn, &mut state.inv, Some(&w), w.id).await?;
                                window = Some(w);
                                self.container_viewers(uuid, &[pos], true);
                            }
                        }
                        Control::CloseWindow => {
                            if let Some(w) = window.as_ref() {
                                conn.send(&inv::container_close(ids.container_close, w.id)).await?;
                                self.close_with_event(&mut state.inv, &mut window, uuid).await;
                                self.send_inventory(conn, &mut state.inv).await?;
                            }
                        }
                        Control::Clear => {
                            state.inv.slots = [None; survival::SLOTS];
                            state.inv.carried = None;
                            if let Some(w) = window.as_ref() {
                                self.send_window(conn, &mut state.inv, Some(w), w.id).await?;
                            } else {
                                self.send_inventory(conn, &mut state.inv).await?;
                            }
                            self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding).await?;
                        }
                        Control::ClearItem {
                            items,
                            extra,
                            damage,
                            max,
                        } => {
                            let _ = state.inv.take_selected(Some(&items), extra, damage, max);
                            if let Some(w) = window.as_ref() {
                                self.send_window(conn, &mut state.inv, Some(w), w.id).await?;
                            } else {
                                self.send_inventory(conn, &mut state.inv).await?;
                            }
                            self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding).await?;
                        }
                        Control::Selected(s) => {
                            state.inv.selected = s;
                            eating = None;
                            conn.send(&inv::set_held_slot(ids.set_held_slot, s as i32)).await?;
                        }
                        Control::Furnace(pos, data, items) => {
                            if let Some(w) = window.as_mut().filter(|w| w.pos == pos) {
                                for (i, (&v, old)) in data.iter().zip(w.data.iter_mut()).enumerate() {
                                    if v != *old {
                                        *old = v;
                                        conn.send(&inv::container_set_data(ids.container_set_data, w.id, i as i16, v as i16)).await?;
                                    }
                                }
                                if items {
                                    w.slots = self.window_slots(w);
                                    self.send_window(conn, &mut state.inv, window.as_ref(), window.as_ref().map_or(0, |w| w.id)).await?;
                                }
                            }
                        }
                        Control::ContainerChanged(pos) => {
                            if let Some(w) = window.as_mut().filter(|w| w.positions().contains(&pos)) {
                                w.slots = self.window_slots(w);
                                self.send_window(conn, &mut state.inv, window.as_ref(), window.as_ref().map_or(0, |w| w.id)).await?;
                            }
                        }
                        Control::Effect { effect: e, force, command } => {
                            self.grant_effect(conn, me, &mut state.vitals, &mut state.inv, &mut state.xp, survival, e, force, command).await?;
                        }
                        Control::ClearEffect { id, command } => {
                            self.revoke_effects(conn, me, &mut state.vitals, id, command).await?;
                        }
                        Control::Xp(change) => {
                            let old = state.xp.level;
                            if state.xp.apply(change) {
                                self.send_xp(conn, &state.xp).await?;
                                self.xp_sounds(me.entry.uuid, old, &state.xp);
                                if let Some(w) = window.as_mut().filter(|w| w.kind == Kind::Enchanting) {
                                    self.refresh_enchant(conn, w, state.xp.seed, uuid).await?;
                                }
                            }
                        }
                        Control::Teleport(pos, yaw, pitch) => {
                            teleport_id += 1;
                            center = self.teleport(conn, uuid, pos, yaw, pitch, teleport_id, &mut stream).await?;
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
                    self.sync_enchant_attrs(conn, uuid, me.entity_id, &state.inv, &mut ench_attrs, &mut ench_gear).await?;
                    let now = digging.or(delayed).and_then(|(pos, start)| {
                        let block = self.block(pos).unwrap_or(AIR);
                        let p = self.destroy_progress(uuid, block, state.inv.held(), state.inv.slots[5], &state.vitals);
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
                            let p = self.destroy_progress(uuid, block, state.inv.held(), state.inv.slots[5], &state.vitals);
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
                    let collects = !state.vitals.dead && login.spawn.game_mode != 3;
                    if let Some(p) = self.hub.players.lock().unwrap().get_mut(&uuid)
                        && p.collects != collects
                    {
                        p.collects = collects;
                    }
                    if collects
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
                        if self.hooks().is_some_and(|h| h.listens(Event::PickupArrow)) {
                            for (id, p) in self.arrow_candidates(feet) {
                                if self.cancelled(Event::PickupArrow, uuid, || format!("{} {} {}", p[0], p[1], p[2])).await {
                                    vetoed.insert(id);
                                }
                            }
                        }
                        let changed = self.pickup(me.entity_id, feet, &mut state.inv, &vetoed);
                        if !changed.is_empty() {
                            self.send_slots(conn, &mut state.inv, &changed).await?;
                        }
                        if xp_delay > 0 {
                            xp_delay -= 1;
                        } else if let Some(orb) = self.orb_near(feet)
                            && self.absorb_orb(conn, uuid, me.entity_id, state, orb).await?
                        {
                            xp_delay = crate::orbs::TAKE_DELAY;
                        }
                    }
                    if let Some((slot, item, extra, done)) = drinking
                        && Instant::now() >= done
                    {
                        drinking = None;
                        let in_hand = slot == state.inv.held_slot() || slot == survival::OFFHAND;
                        let same = state.inv.slots[slot]
                            .is_some_and(|s| s.item == item && s.extra == extra);
                        if in_hand && same && !state.vitals.dead && login.spawn.game_mode != 3 {
                            self.finish_drink(conn, me, state, slot, survival).await?;
                        }
                    }
                    if survival
                        && let Some((slot, food, done)) = eating
                        && Instant::now() >= done
                    {
                        eating = None;
                        // Предмет мог смениться (клики): едим, только если еда на месте.
                        let same = state.inv.slots[slot].and_then(|s| self.survival.items.get(s.item as usize))
                            .and_then(|i| i.food) == Some(food);
                        if same
                            && (slot == state.inv.held_slot() || slot == survival::OFFHAND)
                            && state.inv.consume_at(slot)
                        {
                            state.vitals.eat(food.nutrition, food.saturation);
                            self.send_slots(conn, &mut state.inv, &[slot]).await?;
                            self.send_health(conn, &state.vitals).await?;
                            conn.send(&inv::entity_event(ids.entity_event, me.entity_id, inv::ENTITY_EVENT_USE_ITEM_COMPLETE)).await?;
                            self.player_sound(uuid, "minecraft:entity.player.burp", 0.5, crate::sounds::pitch_09());
                        }
                    }
                    // Длительность тикает и в креативе. Урон от эффектов — только там, где игрок уязвим.
                    let pulsed = state.vitals.tick_effects(self.effect_kinds());
                    let mut motion = 0u8;
                    for e in pulsed.expired {
                        let name = self.effect_name(e.id).unwrap_or_default();
                        if self.potion_cancelled(uuid, "EXPIRATION", "REMOVED", name, None, Some(&e)).await {
                            state.vitals.effects.push(e);
                        } else {
                            conn.send(&inv::remove_mob_effect(ids.remove_mob_effect, me.entity_id, e.id)).await?;
                            motion |= self.motion_mask(e.id);
                        }
                    }
                    self.sync_motion(conn, uuid, me.entity_id, &state.vitals, motion).await?;
                    if !state.vitals.dead {
                        if pulsed.heal > 0.0 {
                            state.vitals.heal(pulsed.heal);
                            self.send_health(conn, &state.vitals).await?;
                        }
                        if pulsed.exhaustion > 0.0 {
                            state.vitals.exhaust(pulsed.exhaustion);
                        }
                        if survival && pulsed.poison && let Some(d) = survival::poison_hit(state.vitals.health) {
                            self.hurt(conn, me, &mut state.vitals, &mut state.inv, &mut state.xp, d, Cause::Poison).await?;
                        }
                        if survival && pulsed.wither && !state.vitals.dead {
                            self.hurt(conn, me, &mut state.vitals, &mut state.inv, &mut state.xp, 1.0, Cause::Wither).await?;
                        }
                    }
                    self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding).await?;
                    if !survival {
                        continue;
                    }
                    let t = state.vitals.tick(Difficulty::from_id(
                        self.survival.difficulty.load(Ordering::Relaxed),
                    ));
                    if let Some(d) = t.starve {
                        self.hurt(conn, me, &mut state.vitals, &mut state.inv, &mut state.xp, d, Cause::Starve).await?;
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
                    self.count_bytes_in(packet.len() as u64);
                    let mut r = Reader::new(&packet);
                    let id = r.var_int()?;
                    if id != ids.sb_keep_alive {
                        last_action = Instant::now();
                    }
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
                                    center = self.teleport(conn, uuid, pos, yaw, pitch, teleport_id, &mut stream).await?;
                                    state.vitals.reset_fall(pos);
                                    continue;
                                }
                            }
                        }
                        let y = m.pos.map(|p| p[1]);
                        let slow = self.effect_amp(&state.vitals, "minecraft:slow_falling").is_some();
                        let in_water = m.pos.is_some_and(|p| self.standing_in_water(p));
                        if let Some(d) = state.vitals.on_move(m.pos, m.on_ground, survival, slow) {
                            let scale = if in_water {
                                0.0
                            } else {
                                m.pos.map_or(1.0, |p| self.fall_scale(p))
                            };
                            if scale > 0.0 {
                                self.hurt(conn, me, &mut state.vitals, &mut state.inv, &mut state.xp, d * scale, Cause::Fall).await?;
                            }
                        }
                        if in_water {
                            state.vitals.clear_fall();
                        }
                        let below_void = y.is_some_and(|y| y < self.min_y as f64 - survival::VOID_DEPTH);
                        if survival && below_void
                            && last_void.is_none_or(|t| t.elapsed() >= TICK * survival::VOID_INTERVAL_TICKS as u32)
                        {
                            last_void = Some(Instant::now());
                            self.hurt(conn, me, &mut state.vitals, &mut state.inv, &mut state.xp, survival::VOID_DAMAGE, Cause::Void).await?;
                        }
                        // `LivingEntity.baseTick`: за границей мира бьёт
                        // `max(1, floor(-dist * damage))`, где `dist` — расстояние
                        // до кромки плюс `safeZone`.
                        if survival && let Some([x, _, z]) = m.pos {
                            let now = self.game_time.load(Ordering::Relaxed);
                            let border = self.vars.border_at(now);
                            let dist = border.distance_to(now, x, z) + border.buffer as f64;
                            let per_block = border.damage as f64;
                            if dist < 0.0 && per_block > 0.0 {
                                let amount = ((-dist) * per_block).floor().max(1.0) as f32;
                                self.hurt(conn, me, &mut state.vitals, &mut state.inv, &mut state.xp, amount, Cause::OutsideBorder).await?;
                            }
                        }
                        if let Some([x, _, z]) = m.pos {
                            let now = ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4);                            if now != center {
                                self.stream_move(conn, &mut stream, now).await?;
                                center = now;
                                out.clear();
                                self.refresh_visible(uuid, center, r_view, &mut visible, &mut out);
                                if !out.is_empty() {
                                    conn.send_frames(&out).await?;
                                }
                            }
                        }
                        let landed = m.on_ground;
                        self.moved(uuid, m);
                        if gliding && landed {
                            self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding).await?;
                        }
                        continue;
                    }
                    if id == ids.sb_set_carried_item {
                        state.inv.selected = (r.u16()? as usize).min(8);
                        eating = None;
                        continue;
                    }
                    if id == ids.sb_pick_item_from_block {
                        let pos = play::unpack_block_pos(r.i64()?);
                        let _include_data = r.bool()?;
                        if !state.vitals.dead && let Some(item) = self.clone_item(uuid, pos) {
                            let creative = login.spawn.game_mode == 1;
                            if let Some(changed) = state.inv.pick(item, creative) {
                                if !changed.is_empty() {
                                    self.send_slots(conn, &mut state.inv, &changed).await?;
                                }
                                conn.send(&inv::set_held_slot(ids.set_held_slot, state.inv.selected as i32)).await?;
                            }
                        }
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
                                self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding).await?;
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
                        if self.click_cancelled(uuid, &state.inv, window.as_ref(), &c).await {
                            self.send_window(conn, &mut state.inv, window.as_ref(), c.window).await?;
                            continue;
                        }
                        {
                            let creative = login.spawn.game_mode == 1;
                            let (thrown, resync) = self.click(&mut state.inv, &mut window, &c, creative, uuid);
                            if resync {
                                self.send_window(conn, &mut state.inv, window.as_ref(), c.window).await?;
                            }
                            if let Some(w) = window.as_mut().filter(|w| w.kind == Kind::Enchanting) {
                                self.refresh_enchant(conn, w, state.xp.seed, uuid).await?;
                            }
                            if let Some(w) = window.as_mut().filter(|w| w.kind == Kind::Grindstone) {
                                self.refresh_grindstone(conn, &mut state.inv, w, uuid).await?;
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
                        self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding).await?;
                        continue;
                    }
                    if id == ids.sb_attack {
                        let target = r.var_int()?;
                        if !state.vitals.dead {
                            let creative = login.spawn.game_mode == 1;
                            if let Some(slot) = self.attack_mob(uuid, target, &mut state.inv, creative, state.vitals.sprinting).await {
                                if state.inv.slots[slot].is_none() {
                                    self.break_in_hand(me.entity_id, false);
                                }
                                self.send_slots(conn, &mut state.inv, &[slot]).await?;
                            }
                        }
                        continue;
                    }
                    if id == ids.sb_interact {
                        let it = play::read_interact(&mut r)?;
                        if !state.vitals.dead {
                            let creative = login.spawn.game_mode == 1;
                            if let Some(slots) = self.interact_mob(uuid, it.entity, it.hand, &mut state.inv, creative).await {
                                self.send_slots(conn, &mut state.inv, &slots).await?;
                            }
                        }
                        continue;
                    }
                    if id == ids.sb_player_input {
                        sneaking = r.u8()? & INPUT_SHIFT != 0;
                        // Плагины видят присед сразу, без ожидания тика.
                        self.set_sneaking(uuid, sneaking);
                        continue;
                    }
                    if id == ids.sb_container_button_click {
                        let wid = r.var_int()?;
                        let button = r.var_int()?;
                        let creative = login.spawn.game_mode == 1;
                        let done = match window.as_mut().filter(|w| w.id == wid && w.kind == Kind::Enchanting) {
                            Some(w) => self.enchant(w, button, &mut state.xp, creative, uuid).await,
                            None => false,
                        };
                        if done {
                            if let Some(w) = window.as_ref() {
                                self.block_sound(None, "minecraft:block.enchantment_table.use", w.pos, 1.0, crate::sounds::pitch_09());
                            }
                            self.send_window(conn, &mut state.inv, window.as_ref(), wid).await?;
                            self.send_xp(conn, &state.xp).await?;
                        }
                        if let Some(w) = window.as_mut().filter(|w| w.kind == Kind::Enchanting) {
                            self.refresh_enchant(conn, w, state.xp.seed, uuid).await?;
                        }
                        self.publish_live(uuid, state, login.spawn.game_mode);
                        continue;
                    }
                    if id == ids.sb_container_close {
                        let _id = r.var_int()?;
                        self.close_with_event(&mut state.inv, &mut window, uuid).await;
                        self.send_inventory(conn, &mut state.inv).await?;
                        self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding).await?;
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
                            drinking = None;
                            conn.send(&play::block_changed_ack(ids.block_changed_ack, sequence)).await?;
                            continue;
                        }
                        if !state.vitals.dead && login.spawn.game_mode != 3
                            && self.use_bucket(conn, uuid, &mut state.inv, hand, survival).await?
                        {
                            conn.send(&play::block_changed_ack(ids.block_changed_ack, sequence)).await?;
                            continue;
                        }
                        match self.use_ranged(
                            conn, uuid, me.entity_id, &mut state.inv, hand, login.spawn.game_mode,
                        ).await? {
                            RangedUse::Draw(slot) => {
                                eating = None;
                                drinking = None;
                                drawing = Some((slot, game_ticks));
                                conn.send(&play::block_changed_ack(ids.block_changed_ack, sequence)).await?;
                                continue;
                            }
                            RangedUse::Done => {
                                eating = None;
                                drinking = None;
                                drawing = None;
                                conn.send(&play::block_changed_ack(ids.block_changed_ack, sequence)).await?;
                                continue;
                            }
                            RangedUse::Ignore => {}
                        }
                        let food_slot = Self::hand_index(&state.inv, hand)
                            .filter(|_| survival && !state.vitals.dead);
                        let food = food_slot
                            .and_then(|i| state.inv.slots[i])
                            .and_then(|s| self.survival.items.get(s.item as usize))
                            .and_then(|i| i.food)
                            .filter(|f| f.can_always_eat || state.vitals.food < survival::MAX_FOOD);
                        if let (Some(slot), Some(f)) = (food_slot, food) {
                            drinking = None;
                            eating = Some((
                                slot,
                                f,
                                Instant::now() + Duration::from_secs_f32(f.seconds),
                            ));
                        } else if let Some((slot, item, extra, seconds)) =
                            self.drink_use(&state.inv, hand, state.vitals.dead, login.spawn.game_mode)
                        {
                            eating = None;
                            drinking = Some((
                                slot,
                                item,
                                extra,
                                Instant::now() + Duration::from_secs_f32(seconds),
                            ));
                        } else {
                            eating = None;
                        }
                        conn.send(&play::block_changed_ack(ids.block_changed_ack, sequence)).await?;
                        continue;
                    }
                    if id == ids.sb_player_abilities {
                        let flags = r.u8()?;
                        let mayfly = self.allow_flight.load(Ordering::Relaxed)
                            || matches!(login.spawn.game_mode, 1 | 3);
                        flying = mayfly && flags & ABILITY_FLYING != 0;
                        if flying {
                            self.refresh_glide(conn, me, state, sneaking, flying, &mut gliding)
                                .await?;
                        }
                        continue;
                    }
                    if id == ids.sb_player_command {
                        let _entity = r.var_int()?;
                        match r.var_int()? {
                            play::START_SPRINTING => state.vitals.sprinting = true,
                            play::STOP_SPRINTING => state.vitals.sprinting = false,
                            play::START_FALL_FLYING => {
                                // `canGlide` ложен при креативном полёте: иначе клиент
                                // планирует (множитель 0.99) и остановка тянется, как в воде.
                                gliding = !flying
                                    && self.wearing_elytra(&state.inv)
                                    && !state.vitals.dead;
                                self.mark_gliding(uuid, gliding);
                                self.send_pose(
                                    conn,
                                    me,
                                    player_flags(sneaking, state.vitals.sprinting, gliding),
                                )
                                .await?;
                            }
                            _ => {}
                        }
                        // Клиент не пересчитывает Скорость/Замедление сам: бег меняет тот же пакет.
                        if state.vitals.effects.iter().any(|e| self.moves_speed(e.id)) {
                            self.send_speed(conn, uuid, me.entity_id, &state.vitals).await?;
                        }
                        continue;
                    }
                    if id == ids.sb_client_command {
                        if r.var_int()? == inv::PERFORM_RESPAWN && state.vitals.dead {
                            teleport_id += 1;
                            center = self.respawn(conn, me, &login, &mut state.vitals, &mut state.inv, &mut stream, teleport_id).await?;
                            // Опыт после смерти выставлен в `die`.
                            self.send_xp(conn, &state.xp).await?;
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
                                    // `NoteBlock.attack`: удар играет ноту, не меняя её.
                                    // Ванила делает это до ломки, поэтому и здесь — до
                                    // прогресса: по блоку в приключении он не ломается.
                                    if crate::redstone::is_note_block(self.state_name(block)) {
                                        self.play_note(a.pos);
                                    }
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
                                let reach = if mode == 1 { 5.0 } else { 4.5 };
                                let in_reach = self.player_at(uuid).is_some_and(|(p, _, _)| {
                                    block_reach(eye_of(p), a.pos, reach)
                                });
                                let mut resync = false;
                                let protected = self.spawn_blocked(uuid, a.pos);
                                if (!in_reach || protected) && a.action != play::ABORT_DESTROY_BLOCK {
                                    resync = true;
                                } else { match a.action {
                                    play::START_DESTROY_BLOCK if mode == 1 => {
                                        self.break_block(conn, uuid, a.pos, None).await?;
                                    }
                                    _ if mode >= 2 => resync = a.action != play::ABORT_DESTROY_BLOCK,
                                    play::START_DESTROY_BLOCK => {
                                        let p = self.destroy_progress(uuid, block, held, state.inv.slots[5], &state.vitals);
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
                                            let p = self.destroy_progress(uuid, block, held, state.inv.slots[5], &state.vitals);
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
                                } }
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
                                        drinking = None;
                                    }
                                }
                                self.send_slots(conn, &mut state.inv, &[held]).await?;
                            }
                            play::RELEASE_USE_ITEM => {
                                if let Some((slot, start)) = drawing.take() {
                                    self.release_draw(
                                        conn,
                                        uuid,
                                        me.entity_id,
                                        &mut state.inv,
                                        slot,
                                        start,
                                        game_ticks,
                                        login.spawn.game_mode,
                                    )
                                    .await?;
                                }
                                eating = None;
                                drinking = None;
                            }
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
                        // Еда на костёр — до обхода окна: присед с едой тоже кладёт её.
                        if !state.vitals.dead && login.spawn.game_mode != 3 {
                            let slot = if u.hand == 0 {
                                state.inv.held_slot()
                            } else {
                                survival::OFFHAND
                            };
                            if let Some(held) = state.inv.slots[slot]
                                && self
                                    .feed_campfire(uuid, u.pos, held, login.spawn.game_mode == 1)
                                    .await
                            {
                                if login.spawn.game_mode != 1 {
                                    state.inv.slots[slot] = (held.count > 1).then_some(Stack {
                                        count: held.count - 1,
                                        ..held
                                    });
                                }
                                self.send_slots(conn, &mut state.inv, &[slot]).await?;
                                conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence))
                                    .await?;
                                continue;
                            }
                        }
                        let clicked = self.state_name(self.block(u.pos).unwrap_or(AIR)).to_owned();
                        let kind = match clicked.as_str() {
                            "minecraft:crafting_table" => Some(Kind::Crafting),
                            "minecraft:enchanting_table" => Some(Kind::Enchanting),
                            "minecraft:grindstone" => Some(Kind::Grindstone),
                            "minecraft:chest" | "minecraft:trapped_chest" | "minecraft:barrel" => Some(Kind::Chest(27)),
                            "minecraft:hopper" => Some(Kind::Chest(5)),
                            "minecraft:dispenser" | "minecraft:dropper" => Some(Kind::Chest(9)),
                            n if furnace_kind(n).is_some() => Some(Kind::Furnace),
                            _ => None,
                        };
                        // `useItemOn`: шифт с предметом в руке — не открывать, а ставить.
                        let bypass = sneaking && (state.inv.held().is_some() || state.inv.slots[survival::OFFHAND].is_some());
                        if !state.vitals.dead && login.spawn.game_mode != 3 && !bypass && u.hand == 0 {
                            let yaw = self.player_at(uuid).map_or(0.0, |p| p.1);
                            if self.use_switch(conn, uuid, u.pos).await? || self.toggle_block(conn, uuid, u.pos, yaw).await? {
                                conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence)).await?;
                                continue;
                            }
                        }
                        if let Some(kind) = kind.filter(|_| !state.vitals.dead && login.spawn.game_mode != 3 && !bypass) {
                            self.close_window(&mut state.inv, &mut window, uuid);
                            next_window = next_window % 100 + 1;
                            // Двойной сундук: первая половина — та, что `type=right` (`CompoundContainer`).
                            let pair = self.chest_partner(u.pos);
                            let (kind, pos, second) = match (kind, pair) {
                                (Kind::Chest(_), Some((partner, clicked_right))) => {
                                    if clicked_right { (Kind::Chest(54), u.pos, Some(partner)) } else { (Kind::Chest(54), partner, Some(u.pos)) }
                                }
                                _ => (kind, u.pos, None),
                            };
                            let (menu, title) = match kind {
                                Kind::Crafting => (self.survival.menu_crafting, "container.crafting"),
                                Kind::Enchanting => (self.survival.menu_enchantment, "container.enchant"),
                                Kind::Grindstone => (self.survival.menu_grindstone, "container.grindstone"),
                                Kind::Furnace => {
                                    let k = furnace_kind(&clicked).unwrap_or(0) as usize;
                                    (self.survival.menu_furnace[k], ["container.furnace", "container.blast_furnace", "container.smoker"][k])
                                }
                                Kind::Chest(54) => (self.survival.menu_double_chest, "container.chestDouble"),
                                Kind::Chest(5) => (self.survival.menu_hopper, "container.hopper"),
                                Kind::Chest(9) => (self.survival.menu_3x3, if clicked == "minecraft:dropper" { "container.dropper" } else { "container.dispenser" }),
                                _ if clicked == "minecraft:barrel" => (self.survival.menu_chest, "container.barrel"),
                                _ => (self.survival.menu_chest, "container.chest"),
                            };
                            let mut w = Window { id: next_window, kind, pos, second, slots: Vec::new(), data: [0; 4], enchant: [i32::MIN; 10], offer: None };
                            w.slots = self.window_slots(&w);
                            conn.send(&inv::open_screen(ids.open_screen, w.id, menu, &serde_json::json!({"translate": title}))).await?;
                            self.send_window(conn, &mut state.inv, Some(&w), w.id).await?;
                            if kind == Kind::Enchanting {
                                self.refresh_enchant(conn, &mut w, state.xp.seed, uuid).await?;
                            }
                            if kind == Kind::Furnace {
                                let data = self.with_furnace(pos, |e| {
                                    e.viewers.push(uuid);
                                    e.f.data()
                                }).unwrap_or_default();
                                for (i, v) in data.into_iter().enumerate() {
                                    conn.send(&inv::container_set_data(ids.container_set_data, w.id, i as i16, v as i16)).await?;
                                }
                                w.data = data;
                            }
                            window = Some(w);
                            if matches!(kind, Kind::Chest(_)) {
                                let positions = window.as_ref().map(Window::positions).unwrap_or_default();
                                for (pos, n) in self.container_viewers(uuid, &positions, true) {
                                    self.show_container(pos, n, true);
                                }
                            }
                            conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence)).await?;
                            continue;
                        }
                        let hand_slot = Self::hand_index(&state.inv, u.hand);
                        if let Some(i) = hand_slot.filter(|_| !state.vitals.dead)
                            && let Some(s) = state.inv.slots[i]
                            && self.use_spawn_egg(self.item_name(s.item), u.pos, u.face, self.player_at(uuid).map_or(0.0, |p| p.1))
                        {
                            if login.spawn.game_mode != 1 {
                                state.inv.consume_at(i);
                                self.send_slots(conn, &mut state.inv, &[i]).await?;
                            }
                            conn.send(&play::block_changed_ack(ids.block_changed_ack, u.sequence)).await?;
                            continue;
                        }
                        let mut placed = false;
                        let place = hand_slot
                            .and_then(|i| state.inv.slots[i])
                            .filter(|_| !state.vitals.dead)
                            .and_then(|s| self.item_blocks.get(s.item as usize).copied().flatten());
                        if let (Some(mut place), Some(d)) = (place, play::direction_offset(u.face)) {
                            let mut target = [u.pos[0] + d[0], u.pos[1] + d[1], u.pos[2] + d[2]];
                            let (yaw, pitch) = self.player_at(uuid).map_or((0.0, 0.0), |p| (p.1, p.2));
                            let ctx = crate::placement::Ctx {
                                face: u.face.clamp(0, 5) as usize,
                                yaw,
                                pitch,
                                cursor_y: u.cursor[1],
                                cursor_x: (u.pos[0] - target[0]) as f32 + u.cursor[0],
                                cursor_z: (u.pos[2] - target[2]) as f32 + u.cursor[2],
                                near: [[0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]].map(|d| {
                                    let p = [target[0] + d[0], target[1], target[2] + d[2]];
                                    [self.block(p).unwrap_or(AIR), self.block([p[0], p[1] + 1, p[2]]).unwrap_or(AIR)]
                                }),
                            };
                            let pl = &self.survival.placement;
                            let clicked_state = self.block(u.pos).unwrap_or(AIR);
                            // Плита в плиту — двойная, на месте существующей.
                            let merged = pl.merge_slab(clicked_state, place, &ctx, true).map(|s| (u.pos, s)).or_else(|| {
                                pl.merge_slab(self.block(target).unwrap_or(AIR), place, &ctx, false).map(|s| (target, s))
                            });
                            let mut extra = Vec::new();
                            if let Some((pos, s)) = merged {
                                target = pos;
                                place = s;
                            } else if let Some(c) = self.survival.chests.get(&place).copied() {
                                place = self.chest_placement(target, c, yaw, u.face, sneaking);
                            } else {
                                let parts = pl.place(place, &ctx);
                                place = parts[0].1;
                                extra = parts[1..]
                                    .iter()
                                    .map(|(o, s)| ([target[0] + o[0], target[1] + o[1], target[2] + o[2]], *s))
                                    .collect();
                            }
                            // Жидкость заменяема; в источник воды заводняемый блок встаёт заводнённым.
                            let here = self.block(target).unwrap_or(AIR);
                            let liquid = crate::fluids::fluid_of(self.state_name(here), None).is_some();
                            if crate::fluids::state_fluid(pl, here).is_some_and(|f| f.source && f.kind == crate::fluids::Kind::Water)
                                && let Some(w) = pl.with_prop(place, "waterlogged", "true")
                            {
                                place = w;
                            }
                            let free = merged.is_some() || here == AIR || liquid;
                            let reach = if login.spawn.game_mode == 1 { 5.0 } else { 4.5 };
                            let feet = self.player_at(uuid).map(|p| p.0);
                            let in_reach = feet.is_some_and(|p| block_reach(eye_of(p), target, reach));
                            let full = (0..6).all(|f| pl.sturdy(place, f));
                            let inside = full && feet.is_some_and(|p| player_hits_cube(p, target));
                            let protected = self.spawn_blocked(uuid, target);
                            if free && in_reach && !inside && !protected && extra.iter().all(|(p, _)| self.block(*p) == Some(AIR)) {
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
                                placed = true;
                                // Провод ставится крестом (`getStateForPlacement`), дальше — форма и сила.
                                if crate::redstone::is_wire(&name) {
                                    for k in ["north", "south", "west", "east"] {
                                        if let Some(m) = self.survival.placement.with_prop(place, k, "side") {
                                            place = m;
                                        }
                                    }
                                }
                                // Нить тянется к соседним нитям и крюкам
                                // (`TripWireBlock.getStateForPlacement`).
                                if crate::redstone::is_tripwire(&name) {
                                    place = pl.tripwire_placement(place, &ctx);
                                }
                                // Нотный блок берёт инструмент у соседей
                                // (`NoteBlock.getStateForPlacement`).
                                if crate::redstone::is_note_block(&name) {
                                    place = self.note_instrument_at(target, place);
                                }
                                // Одна правка: тик не разошлёт промежуточные состояния.
                                let edit = self.world_edit.lock().unwrap();
                                self.set_block(target, place);
                                let sound = crate::sounds::place_sound(&name, |s| self.survival.sounds.contains_key(s));
                                self.block_sound(Some(uuid), &sound, target, 1.0, 0.8);
                                for &(p, s) in &extra {
                                    self.set_block(p, s);
                                }
                                // Дверь у включённого рычага сразу открыта (`getStateForPlacement`: `powered`).
                                self.refresh_openable(target, false);
                                self.redstone_update(&crate::redstone::around(target));
                                let mut touched = self.update_connections(target);
                                // Рельсы: форма нового и перестройка соседей (`RailState`).
                                let rails = self.survival.placement.rails(target, &|p| self.block(p).unwrap_or(AIR));
                                for (p, st) in rails {
                                    self.set_block(p, st);
                                    touched.push(p);
                                }
                                self.link_chest(target, place);
                                if let Some(id) = container_entity(&name) {
                                    self.set_block_entity(target, Some(BlockEntity { id: id.into(), nbt: Default::default() }));
                                }
                                // Крюк сразу считает свою цепь
                                // (`TripWireHookBlock.setPlacedBy`).
                                if crate::redstone::is_tripwire_hook(&name) {
                                    self.tripwire_hook_state(target, false, false, -1, None);
                                }
                                drop(edit);
                                if survival && let Some(i) = hand_slot && state.inv.consume_at(i) {
                                    self.send_slots(conn, &mut state.inv, &[i]).await?;
                                }
                                // Новое состояние — до подтверждения: иначе клиент откатит
                                // свой предсказанный блок к воздуху до рассылки тика (мигание).
                                touched.extend(std::iter::once(target).chain(extra.iter().map(|e| e.0)));
                                touched.sort();
                                touched.dedup();
                                for p in touched {
                                    let s = self.block(p).unwrap_or(AIR);
                                    conn.send(&play::block_update(ids.block_update, p, s as i32)).await?;
                                }
                            } else {
                                // Отказ: вернуть клиенту настоящие блоки, иначе предсказание
                                // останется призраком (ваниль шлёт кликнутый блок и цель).
                                let mut back = vec![u.pos, target];
                                back.extend(extra.iter().map(|e| e.0));
                                for p in back {
                                    if let Some(s) = self.block(p) {
                                        conn.send(&play::block_update(ids.block_update, p, s as i32)).await?;
                                    }
                                }
                            }
                        }
                        if !placed {
                            match self.use_ranged(
                                conn, uuid, me.entity_id, &mut state.inv, u.hand, login.spawn.game_mode,
                            ).await? {
                                RangedUse::Draw(slot) => {
                                    eating = None;
                                    drinking = None;
                                    drawing = Some((slot, game_ticks));
                                }
                                RangedUse::Done => {
                                    eating = None;
                                    drinking = None;
                                    drawing = None;
                                }
                                RangedUse::Ignore => {}
                            }
                        }
                        if !placed
                            && drawing.is_none()
                            && let Some((slot, item, extra, seconds)) = self.drink_use(
                                &state.inv,
                                u.hand,
                                state.vitals.dead,
                                login.spawn.game_mode,
                            )
                        {
                            eating = None;
                            drinking = Some((
                                slot,
                                item,
                                extra,
                                Instant::now() + Duration::from_secs_f32(seconds),
                            ));
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
                        // Встроенные команды: права, дерево и выполнение — в диспетчере.
                        let source = crate::play::commands::dispatch::Source::player(
                            uuid,
                            me.entry.name.as_str(),
                            self.op_level(uuid),
                        );
                        if let Some(outcome) = self.run_command(&source, &line) {
                            match outcome {
                                crate::play::commands::dispatch::Outcome::Feedback(msg) => {
                                    // Игроку без прав ванила отвечает «неизвестная команда».
                                    if !msg.is_null() {
                                        if command_error(&msg) {
                                            tracing::warn!(
                                                "{}: {}",
                                                me.entry.name,
                                                self.command_text(&msg)
                                            );
                                        }
                                        conn.send(&text::system_chat_json(
                                            ids.system_chat,
                                            &msg.to_string(),
                                            false,
                                        ))
                                        .await?;
                                    }
                                }
                                crate::play::commands::dispatch::Outcome::Broadcast(msg) => {
                                    self.broadcast_json(&msg.to_string());
                                }
                                crate::play::commands::dispatch::Outcome::Silent => {}
                            }
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
                            tracing::warn!("{}: Unknown command: /{line}", me.entry.name);
                            let msg = serde_json::json!({"text": format!("Unknown command: /{line}"), "color": "red"});
                            conn.send(&text::system_chat_json(ids.system_chat, &msg.to_string(), false)).await?;
                        }
                        continue;
                    }
                    if id == ids.sb_command_suggestion {
                        let tx = r.var_int()?;
                        let typed = r.string(32500)?.to_owned();
                        let line = typed.strip_prefix('/').unwrap_or(&typed).to_owned();
                        // Литералы клиент берёт из пакета `commands`; сервер
                        // добавляет то, что тот знать не может (имена плагинов
                        // и цели от `ask_server`), поэтому его ответ не должен
                        // затирать дерево пустым списком.
                        let mut matches = crate::play::commands::suggestions(
                            self,
                            &line,
                            self.op_level(uuid),
                        );
                        let reply = self
                            .plugin_event(Event::TabComplete, uuid, || line.clone())
                            .await
                            .unwrap_or_default();
                        matches.extend(
                            reply
                                .lines()
                                .filter(|l| !l.is_empty())
                                .map(str::to_owned),
                        );
                        matches.sort();
                        matches.dedup();
                        // Заменяемый участок — текущее слово после пробела.
                        let start = typed.rfind(' ').map_or(0, |i| i + 1);
                        let matches: Vec<&str> = matches.iter().map(String::as_str).collect();
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
                        // RTT — основа показателя пинга в `/top`.
                        if let Some((_, sent)) = pending {
                            self.set_ping(uuid, sent.elapsed().as_secs_f32() * 1000.0);
                        }
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

    fn spawn_blocked(&self, uuid: Uuid, pos: [i32; 3]) -> bool {
        let radius = self.spawn_protection.load(Ordering::Relaxed);
        radius > 0 && self.op_level(uuid) == 0 && in_spawn(self.spawn, radius, pos)
    }

    fn standing_in_water(&self, feet: [f64; 3]) -> bool {
        let p = [
            feet[0].floor() as i32,
            feet[1].floor() as i32,
            feet[2].floor() as i32,
        ];
        self.block(p).is_some_and(|s| {
            let n = self.state_name(s);
            n == "minecraft:water"
                || n == "minecraft:bubble_column"
                || self.survival.placement.prop(s, "waterlogged") == Some("true")
        })
    }

    fn fall_scale(&self, feet: [f64; 3]) -> f32 {
        let below = [
            feet[0].floor() as i32,
            (feet[1] - 0.2).floor() as i32,
            feet[2].floor() as i32,
        ];
        fall_kept(self.block(below).map_or("", |s| self.state_name(s)))
    }

    fn state_name(&self, state: u32) -> &str {
        self.survival
            .block_names
            .get(state as usize)
            .map_or("minecraft:air", String::as_str)
    }

    /// Предмет средней кнопки: блок в досягаемости, тот же предмет, что у блока.
    fn clone_item(&self, uuid: Uuid, pos: [i32; 3]) -> Option<i32> {
        let (feet, _, _) = self.player_at(uuid)?;
        let (dx, dy, dz) = (
            feet[0] - (pos[0] as f64 + 0.5),
            feet[1] + 1.62 - (pos[1] as f64 + 0.5),
            feet[2] - (pos[2] as f64 + 0.5),
        );
        if dx * dx + dy * dy + dz * dz > 64.0 {
            return None;
        }
        let name = match self.state_name(self.block(pos)?) {
            "minecraft:water" => "minecraft:water_bucket",
            "minecraft:lava" => "minecraft:lava_bucket",
            "minecraft:powder_snow" => "minecraft:powder_snow_bucket",
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" => return None,
            name => name,
        };
        self.item_id(name)
    }

    /// Прогресс ломки за тик предметом в руке: вода у глаз и опора под ногами.
    fn destroy_progress(
        &self,
        uuid: Uuid,
        state: u32,
        held: Slot,
        helmet: Slot,
        v: &Vitals,
    ) -> f32 {
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
        let level = |s: Slot, name: &str| {
            s.map_or(0, |s| {
                chunkwake_protocol::components::stack_level(s.extra, name)
            })
        };
        // Подводник на шлеме (окно игрока: ячейка 5) снимает штраф воды.
        let in_water = in_water && level(helmet, "minecraft:aqua_affinity") == 0;
        let amp = |name: &str| {
            self.survival
                .effects
                .get(name)
                .and_then(|&id| v.effect(id))
                .map(|e| e.amplifier)
        };
        let haste = amp("minecraft:haste").max(amp("minecraft:conduit_power"));
        let dig = survival::dig_multiplier(haste, amp("minecraft:mining_fatigue"));
        survival::destroy_progress(
            hardness,
            requires,
            self.tool(held),
            self.state_name(state),
            in_water,
            v.on_ground(),
            dig,
            level(held, "minecraft:efficiency"),
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
        let drops = self.drops_with(block, held).then_some(held);
        if !self.break_block(conn, uuid, pos, drops).await? {
            return Ok(());
        }
        state.vitals.exhaust(survival::EXHAUSTION_BREAK);
        // Опыт за руду (`DropExperienceBlock.spawnAfterBreak`) — сразу игроку, без сфер.
        let silk = held.map_or(0, |s| {
            chunkwake_protocol::components::stack_level(s.extra, "minecraft:silk_touch")
        });
        if drops.is_some()
            && silk == 0
            && let Some((lo, hi)) = crate::xp::ore_range(self.state_name(block))
        {
            let n = rand::Rng::gen_range(&mut rand::thread_rng(), lo..=hi);
            if n > 0 {
                self.spawn_orbs(
                    [
                        pos[0] as f64 + 0.5,
                        pos[1] as f64 + 0.5,
                        pos[2] as f64 + 0.5,
                    ],
                    n,
                );
            }
        }
        // `ItemStack.mineBlock`: инструмент изнашивается, если блок ломается не мгновенно.
        let hardness = self
            .survival
            .hardness
            .get(block as usize)
            .copied()
            .unwrap_or(0.0);
        let slot = state.inv.held_slot();
        if hardness != 0.0
            && let Some(mut s) = held
            && let Some(tool) = self.tool(held)
        {
            let max = self
                .survival
                .items
                .get(s.item as usize)
                .map_or(0, |i| i.max_damage);
            // Прочность: износ с вероятностью 1/(lvl+1) на каждую единицу.
            let unbreaking =
                chunkwake_protocol::components::stack_level(s.extra, "minecraft:unbreaking");
            let wear = (0..tool.damage_per_block)
                .filter(|_| {
                    unbreaking == 0 || rand::random::<u32>().is_multiple_of(unbreaking as u32 + 1)
                })
                .count() as i32;
            if max > 0 && wear > 0 {
                s.damage += wear;
                state.inv.slots[slot] = (s.damage < max).then_some(s);
                let entity = self
                    .hub
                    .players
                    .lock()
                    .unwrap()
                    .get(&uuid)
                    .map(|p| p.entity_id);
                if let Some(entity) = entity.filter(|_| s.damage >= max) {
                    // `broadcastBreakEvent(MAINHAND)`: звук и осколки у всех.
                    self.hub.send(
                        None,
                        inv::entity_event(
                            self.ids.entity_event,
                            entity,
                            inv::ENTITY_EVENT_BREAK_MAINHAND,
                        ),
                    );
                }
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
        drops: Option<Slot>,
    ) -> Result<bool, NetError> {
        if self.plugins_cancel_break(uuid, pos).await {
            // Отменено: вернуть блок клиенту.
            let old = self.block(pos).unwrap_or(AIR);
            conn.send(&play::block_update(self.ids.block_update, pos, old as i32))
                .await?;
            return Ok(false);
        }
        // Одна правка: тик не разошлёт промежуточные состояния соседей.
        let edit = self.world_edit.lock().unwrap();
        let Some(old) = self.set_block(pos, AIR) else {
            return Ok(false);
        };
        // Ножницы разоружают нить перед снятием (`TripWireBlock.playerWillDestroy`):
        // в мир пишется `disarmed = true`, и последующий пересчёт крюков
        // видит разоружённую нить — сигнал не поднимается. У нас блок уже
        // снят, поэтому крюкам передаётся именно такое состояние.
        let disarmed_by_shears = chunkwake_world::has_rare(old, chunkwake_world::rare::TRIPWIRE)
            && self.live.lock().unwrap().get(&uuid).is_some_and(|l| {
                self.item_name(l.slots[36 + l.selected].map_or(0, |s| s.item)) == "minecraft:shears"
            });
        // Сломан включённый рычаг или кнопка — сигнал пропадает.
        if self.survival.placement.prop(old, "powered") == Some("true") && {
            let n = self.state_name(old);
            crate::redstone::is_lever(n)
                || crate::redstone::is_button(n)
                || crate::redstone::is_tripwire(n)
        } {
            if disarmed_by_shears {
                // Разоружённая нить молчит: крюки считаются по `disarmed`.
                if let Some(gone) = self.survival.placement.with_prop(old, "disarmed", "true") {
                    self.update_tripwire_source(pos, gone);
                }
            } else {
                self.update_power(old, pos);
            }
        } else {
            self.redstone_update(&crate::redstone::around(pos));
        }
        // Нить снята без ножниц: крюки теряют `attached` и сигнал.
        if chunkwake_world::has_rare(old, chunkwake_world::rare::TRIPWIRE)
            && !disarmed_by_shears
            && let Some(on) = self.survival.placement.with_prop(old, "powered", "true")
        {
            self.update_tripwire_source(pos, on);
        }
        // Крюк: цепь рушится, а включённый ещё и гасит соседей (`onRemoved`).
        if chunkwake_world::has_rare(old, chunkwake_world::rare::TRIPWIRE_HOOK) {
            self.tripwire_hook_removed(old, pos);
        }
        // Частицы и звук ломки — остальным (ломающий проиграл их сам).
        self.hub.send(
            Some(uuid),
            play::level_event(
                self.ids.level_event,
                crate::sounds::DESTROY_BLOCK,
                pos,
                old as i32,
                false,
            ),
        );
        self.unlink_chest(pos, old);
        let mut touched = self.update_connections(pos);
        touched.push(pos);
        // Вторая половина двери, кровати, высокого растения.
        if let Some(o) = self.survival.placement.partner(old) {
            let p = [pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]];
            if self.block(p).is_some_and(|s| {
                self.survival.placement.partner(s).is_some()
                    && self.survival.placement.name(s) == self.survival.placement.name(old)
            }) {
                self.set_block(p, AIR);
                touched.push(p);
                touched.extend(self.update_connections(p));
            }
        }
        drop(edit);
        // До подтверждения ломки — иначе клиент откатит соседей (мигание).
        for p in touched {
            let s = self.block(p).unwrap_or(AIR);
            conn.send(&play::block_update(self.ids.block_update, p, s as i32))
                .await?;
        }
        // Печь: в блок-сущность её текущее содержимое.
        self.comparators.lock().unwrap().remove(&pos);
        self.piston_broken(pos, old);
        self.hopper_ticks.lock().unwrap().remove(pos);
        let removed = self.furnaces.lock().unwrap().remove(&pos);
        if let Some(c) = self.campfires.lock().unwrap().remove(&pos) {
            self.save_campfire(pos, &c, false);
        }
        if let Some(mut e) = removed {
            // Опыт за сготовленное — сферами на месте печи.
            let xp = e.f.take_xp(rand::random());
            if xp > 0 {
                self.spawn_orbs(
                    [
                        pos[0] as f64 + 0.5,
                        pos[1] as f64 + 0.5,
                        pos[2] as f64 + 0.5,
                    ],
                    xp,
                );
            }
            self.save_furnace(pos, &e.f);
        }
        // Содержимое сундука — на землю.
        if self.block_entity(pos).is_some() {
            let center = [
                pos[0] as f64 + 0.5,
                pos[1] as f64 + 0.5,
                pos[2] as f64 + 0.5,
            ];
            for s in self.container_items(pos).into_iter().flatten() {
                let vel = items::pop_velocity(rand::random(), rand::random());
                self.drop_item(center, vel, s, items::BLOCK_DELAY);
            }
            self.set_block_entity(pos, None);
        }
        if let Some(held) = drops {
            let center = [
                pos[0] as f64 + 0.5,
                pos[1] as f64 + 0.25,
                pos[2] as f64 + 0.5,
            ];
            for s in self.block_loot(old, held) {
                let vel = items::pop_velocity(rand::random(), rand::random());
                self.drop_item(center, vel, s, items::BLOCK_DELAY);
            }
        }
        Ok(true)
    }

    /// Ломает блок командой (`setblock destroy`, `fill destroy`): частицы и дроп.
    pub(crate) fn break_drops(&self, pos: [i32; 3]) {
        let Some(old) = self.block(pos) else { return };
        if self.state_name(old).ends_with(":air") {
            return;
        }
        self.hub.send(
            None,
            play::level_event(
                self.ids.level_event,
                crate::sounds::DESTROY_BLOCK,
                pos,
                old as i32,
                false,
            ),
        );
        let center = [
            pos[0] as f64 + 0.5,
            pos[1] as f64 + 0.5,
            pos[2] as f64 + 0.5,
        ];
        if self.block_entity(pos).is_some() {
            for stack in self.container_items(pos).into_iter().flatten() {
                let vel = items::pop_velocity(rand::random(), rand::random());
                self.drop_item(center, vel, stack, items::BLOCK_DELAY);
            }
            self.set_block_entity(pos, None);
        }
        let drop_at = [center[0], pos[1] as f64 + 0.25, center[2]];
        for stack in self.block_loot(old, None) {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(drop_at, vel, stack, items::BLOCK_DELAY);
        }
    }

    /// Дроп блока `state`, сломанного предметом `held`, по таблице лута.
    fn block_loot(&self, state: u32, held: Slot) -> Vec<Stack> {
        let Some(block) = self.survival.placement.name(state) else {
            return Vec::new();
        };
        let prop = |k: &str| self.survival.placement.prop(state, k).map(str::to_owned);
        let level = |n: &str| {
            held.map_or(0, |s| {
                chunkwake_protocol::components::stack_level(s.extra, n)
            })
        };
        let ctx = crate::loot::Ctx {
            prop: &prop,
            block,
            tool: held.map(|s| self.item_name(s.item)),
            level: &level,
            entity: None,
            killed_by_player: true,
            smelt: &|_| None,
        };
        crate::loot::roll(&self.survival.loot, &ctx, &mut rand::thread_rng())
            .into_iter()
            .filter_map(|(name, count)| {
                let item = self.item_id(&name)?;
                Some(Stack::new(item, count.min(self.max_stack(item).max(1))))
            })
            .collect()
    }

    fn max_stack(&self, item: i32) -> i32 {
        self.survival
            .items
            .get(item as usize)
            .map_or(64, |i| i.max_stack as i32)
    }

    /// Результат крафта для сетки `size × size`.
    fn craft_result(&self, grid: &[Slot], size: usize) -> Slot {
        let ids: Vec<Option<i32>> = grid.iter().map(|s| s.map(|s| s.item)).collect();
        let (item, count) = self.survival.recipes.find(&ids, size)?;
        Some(Stack {
            item,
            count,
            damage: 0,
            extra: 0,
        })
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
            if matches!(w.kind, Kind::Chest(_) | Kind::Furnace) {
                w.slots = self.window_slots(w);
            }
            (
                w.kind,
                w.slots.iter().chain(&inv.slots[9..45]).copied().collect(),
            )
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
            Kind::Chest(_) | Kind::Furnace | Kind::Enchanting | Kind::Grindstone => 0,
        };
        let fill = |slots: &mut [Slot]| {
            if kind == Kind::Grindstone {
                slots[2] = self.grind_result(slots[0], slots[1]);
            } else if size > 0 {
                slots[0] = self.craft_result(&slots[1..=size * size], size);
            }
        };
        // `ResultSlot.onTake`: остаток (ведро, бутылка) — в опустевшую ячейку, иначе в инвентарь.
        let mut leftovers = Vec::new();
        let mut grind_xp = 0;
        let mut grind_took = false;
        let mut craft = |slots: &mut [Slot]| {
            if kind == Kind::Grindstone {
                grind_xp += self.grind_experience(slots[0]) + self.grind_experience(slots[1]);
                grind_took = true;
                slots[0] = None;
                slots[1] = None;
                return;
            }
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
        // Печь окна: рецепты её вида.
        let fk = window
            .as_ref()
            .filter(|w| w.kind == Kind::Furnace)
            .and_then(|w| furnace_kind(self.state_name(self.block(w.pos).unwrap_or(AIR))))
            .unwrap_or(0);
        let smeltable = |item: i32| self.survival.recipes.cooking(fk, item).is_some();
        let fuel = |item: i32| {
            self.survival
                .items
                .get(item as usize)
                .is_some_and(|i| i.fuel.is_some())
        };
        let grindable = |s: &Stack| crate::grindstone::acceptable(s, self.item_max_damage(s.item));
        let rules = click::Rules {
            max_stack: &max,
            equip: &equip,
            smeltable: &smeltable,
            fuel: &fuel,
            bucket: self.item_id("minecraft:bucket"),
            lapis: self.item_id(crate::enchanting::LAPIS),
            grindable: &grindable,
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
        let same =
            |a: &Slot, b: &Slot| a.map(|s| (s.item, s.count)) == b.map(|s| (s.item, s.count));
        // Точило меняет чары и износ: сравнение смотрит только предмет и число.
        let resync = kind == Kind::Grindstone
            || c.state != inv.state
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
                if kind == Kind::Furnace && w.slots[..] != view[..n] {
                    self.set_furnace_slots(w.pos, [view[0], view[1], view[2]], me);
                }
                w.slots.copy_from_slice(&view[..n]);
                inv.slots[9..45].copy_from_slice(&view[n..]);
            }
            None => inv.slots.copy_from_slice(&view),
        }
        for s in leftovers {
            thrown.extend(inv.add_stack(s, self.max_stack(s.item)));
        }
        if grind_took && let Some(pos) = window.as_ref().map(|w| w.pos) {
            self.grind_taken(pos, grind_xp);
        }
        (thrown, resync)
    }

    fn item_max_damage(&self, item: i32) -> i32 {
        self.survival
            .items
            .get(item as usize)
            .map_or(0, |i| i.max_damage)
    }

    fn grind_result(&self, top: Slot, bottom: Slot) -> Slot {
        // Замыкания живут до конца вызова: `Ctx` держит на них ссылки.
        let max_damage = |item| self.item_max_damage(item);
        let max_stack = |item| self.max_stack(item);
        let curse = |id| {
            self.survival
                .enchanting
                .defs
                .iter()
                .find(|d| d.id == id)
                .is_some_and(|d| d.curse)
        };
        let min_cost = |id, level| {
            self.survival
                .enchanting
                .defs
                .iter()
                .find(|d| d.id == id)
                .map_or(0, |d| d.min_cost(level))
        };
        let ctx = crate::grindstone::Ctx {
            enchanted_book: self
                .item_id(crate::enchanting::ENCHANTED_BOOK)
                .unwrap_or(-1),
            book: self.item_id(crate::enchanting::BOOK).unwrap_or(-1),
            max_damage: &max_damage,
            max_stack: &max_stack,
            curse: &curse,
            min_cost: &min_cost,
        };
        crate::grindstone::compute(top, bottom, &ctx)
    }

    fn grind_experience(&self, stack: Slot) -> i32 {
        let max_damage = |item| self.item_max_damage(item);
        let max_stack = |item| self.max_stack(item);
        let curse = |id| {
            self.survival
                .enchanting
                .defs
                .iter()
                .find(|d| d.id == id)
                .is_some_and(|d| d.curse)
        };
        let min_cost = |id, level| {
            self.survival
                .enchanting
                .defs
                .iter()
                .find(|d| d.id == id)
                .map_or(0, |d| d.min_cost(level))
        };
        let ctx = crate::grindstone::Ctx {
            enchanted_book: self
                .item_id(crate::enchanting::ENCHANTED_BOOK)
                .unwrap_or(-1),
            book: self.item_id(crate::enchanting::BOOK).unwrap_or(-1),
            max_damage: &max_damage,
            max_stack: &max_stack,
            curse: &curse,
            min_cost: &min_cost,
        };
        crate::grindstone::experience_of(stack, &ctx)
    }

    /// Забор результата: сферы в центре блока и звук точила.
    fn grind_taken(&self, pos: [i32; 3], n: i32) {
        let roll = if n <= 0 {
            0
        } else {
            let half = (n + 1) / 2;
            rand::Rng::gen_range(&mut rand::thread_rng(), 0..half)
        };
        let amount = crate::grindstone::xp_amount(n, roll);
        if amount > 0 {
            self.spawn_orbs(
                [
                    pos[0] as f64 + 0.5,
                    pos[1] as f64 + 0.5,
                    pos[2] as f64 + 0.5,
                ],
                amount,
            );
        }
        self.hub.send(
            None,
            play::level_event(self.ids.level_event, 1042, pos, 0, false),
        );
    }

    /// Окно всё ещё у своего блока (сундук не сломали).
    fn window_valid(&self, w: &Window) -> bool {
        match w.kind {
            Kind::Chest(_) => w.positions().iter().all(|&p| match plugin_of(p) {
                Some(id) => self.plugin_invs.lock().unwrap().contains_key(&id),
                None => self.block_entity(p).is_some(),
            }),
            Kind::Furnace => self
                .block(w.pos)
                .is_some_and(|s| furnace_kind(self.state_name(s)).is_some()),
            Kind::Enchanting => self
                .block(w.pos)
                .is_some_and(|s| self.state_name(s) == "minecraft:enchanting_table"),
            Kind::Grindstone => self
                .block(w.pos)
                .is_some_and(|s| self.state_name(s) == "minecraft:grindstone"),
            _ => self
                .block(w.pos)
                .is_some_and(|s| self.state_name(s) == "minecraft:crafting_table"),
        }
    }

    fn wearing_elytra(&self, inv: &Inventory) -> bool {
        inv.slots[survival::CHEST].is_some_and(|s| self.item_name(s.item) == "minecraft:elytra")
    }

    fn mark_gliding(&self, uuid: Uuid, gliding: bool) {
        if let Some(p) = self.hub.players.lock().unwrap().get_mut(&uuid) {
            p.gliding = gliding;
        }
    }

    async fn send_pose(
        &self,
        conn: &mut Connection,
        me: &Online,
        flags: u8,
    ) -> Result<(), NetError> {
        let packet = inv::set_entity_flags(self.ids.set_entity_data, me.entity_id, flags);
        conn.send(&packet).await?;
        self.hub.send(Some(me.entry.uuid), packet);
        Ok(())
    }

    /// Гасит элитры, если их сняли, игрок на земле, мёртв или включил креативный полёт.
    async fn refresh_glide(
        &self,
        conn: &mut Connection,
        me: &Online,
        state: &PlayerState,
        sneaking: bool,
        flying: bool,
        gliding: &mut bool,
    ) -> Result<(), NetError> {
        if !*gliding {
            return Ok(());
        }
        let ground = self
            .hub
            .players
            .lock()
            .unwrap()
            .get(&me.entry.uuid)
            .is_some_and(|p| p.on_ground);
        if state.vitals.dead || ground || flying || !self.wearing_elytra(&state.inv) {
            *gliding = false;
            self.mark_gliding(me.entry.uuid, false);
            self.send_pose(
                conn,
                me,
                player_flags(sneaking, state.vitals.sprinting, false),
            )
            .await?;
        }
        Ok(())
    }

    fn container_viewers(
        &self,
        uuid: Uuid,
        positions: &[[i32; 3]],
        add: bool,
    ) -> Vec<([i32; 3], usize)> {
        let mut map = self.open_blocks.lock().unwrap();
        let mut out = Vec::new();
        for &pos in positions {
            let n = {
                let viewers = map.entry(pos).or_default();
                if add {
                    if !viewers.contains(&uuid) {
                        viewers.push(uuid);
                    }
                } else {
                    viewers.retain(|id| *id != uuid);
                }
                viewers.len()
            };
            if n == 0 {
                map.remove(&pos);
            }
            out.push((pos, n));
        }
        out
    }

    /// Крышка сундука — `block_event`, бочка — свойство `open`.
    fn show_container(&self, pos: [i32; 3], viewers: usize, opening: bool) {
        let Some(state) = self.block(pos) else { return };
        let name = self.state_name(state).to_owned();
        // Звук — на первом открытии и последнем закрытии (`ContainerOpenersCounter`).
        let kind = match name.as_str() {
            "minecraft:barrel" => Some("barrel"),
            "minecraft:chest" | "minecraft:trapped_chest" => Some("chest"),
            _ => None,
        };
        if let Some(kind) = kind
            && ((opening && viewers == 1) || (!opening && viewers == 0))
        {
            let sound = format!(
                "minecraft:block.{kind}.{}",
                if opening { "open" } else { "close" }
            );
            self.block_sound(None, &sound, pos, 0.5, crate::sounds::pitch_09());
        }
        if name == "minecraft:barrel" {
            let value = if viewers > 0 { "true" } else { "false" };
            if let Some(next) = self.survival.placement.with_prop(state, "open", value) {
                self.set_block(pos, next);
                self.hub.send(
                    None,
                    play::block_update(self.ids.block_update, pos, next as i32),
                );
            }
            return;
        }
        if matches!(name.as_str(), "minecraft:chest" | "minecraft:trapped_chest")
            && let Some(&block) = self.survival.block_types.get(name.as_str())
        {
            self.hub.send(
                None,
                play::block_event(self.ids.block_event, pos, 1, viewers.min(255) as u8, block),
            );
        }
    }

    fn release_viewer(&self, uuid: Uuid) {
        let positions: Vec<[i32; 3]> = self
            .open_blocks
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, viewers)| viewers.contains(&uuid))
            .map(|(pos, _)| *pos)
            .collect();
        if positions.is_empty() {
            return;
        }
        for (pos, n) in self.container_viewers(uuid, &positions, false) {
            self.show_container(pos, n, false);
        }
    }

    /// Закрытие окна: сетка крафта и курсор возвращаются в инвентарь (не влезло — на землю).
    fn close_window(&self, inv: &mut Inventory, window: &mut Option<Window>, uuid: Uuid) {
        let mut back: Vec<Stack> = inv.slots[1..5]
            .iter_mut()
            .filter_map(Option::take)
            .collect();
        inv.slots[0] = None;
        if let Some(mut w) = window.take() {
            match w.kind {
                Kind::Crafting => back.extend(w.slots[1..10].iter_mut().filter_map(Option::take)),
                Kind::Enchanting => back.extend(w.slots.iter_mut().filter_map(Option::take)),
                Kind::Grindstone => {
                    for slot in w.slots.iter_mut().take(2) {
                        back.extend(slot.take());
                    }
                }
                Kind::Furnace => {
                    if let Some(e) = self.furnaces.lock().unwrap().get_mut(&w.pos) {
                        e.viewers.retain(|v| *v != uuid);
                    }
                }
                Kind::Chest(_) => {
                    let positions = w.positions();
                    for (pos, n) in self.container_viewers(uuid, &positions, false) {
                        self.show_container(pos, n, false);
                    }
                }
                Kind::Player => {}
            }
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
            Kind::Chest(_) => w
                .positions()
                .iter()
                .flat_map(|&p| self.container_items(p))
                .collect(),
            Kind::Furnace => self
                .with_furnace(w.pos, |e| e.f.slots.to_vec())
                .unwrap_or_else(|| vec![None; 3]),
            Kind::Crafting if w.slots.is_empty() => vec![None; 10],
            Kind::Enchanting if w.slots.is_empty() => vec![None; 2],
            Kind::Grindstone if w.slots.is_empty() => vec![None; 3],
            _ => w.slots.clone(),
        }
    }

    /// Печь из памяти или из блок-сущности (`AbstractFurnaceBlockEntity.loadAdditional`).
    fn with_furnace<R>(&self, pos: [i32; 3], f: impl FnOnce(&mut FurnaceEntry) -> R) -> Option<R> {
        if !self.furnaces.lock().unwrap().contains_key(&pos) {
            let kind = furnace_kind(self.state_name(self.block(pos)?))?;
            let mut fur = Furnace::new(kind);
            if let Some(be) = self.block_entity(pos) {
                let items = self.container_items(pos);
                fur.slots = [items[0], items[1], items[2]];
                let int = |k: &str| be.nbt.get(k).and_then(Tag::as_i64).unwrap_or(0) as i32;
                fur.cook = int("cooking_time_spent");
                fur.cook_total = int("cooking_total_time");
                fur.lit_remaining = int("lit_time_remaining");
                fur.lit_total = int("lit_total_time");
                if let Some(Tag::Float(x)) = be.nbt.get("chunkwake:experience") {
                    fur.xp = *x;
                }
                if let Some(Tag::Float(s)) = be.nbt.get("speed_multiplier") {
                    fur.speed = *s;
                }
            }
            self.furnaces
                .lock()
                .unwrap()
                .entry(pos)
                .or_insert(FurnaceEntry {
                    f: fur,
                    viewers: Vec::new(),
                });
        }
        self.furnaces.lock().unwrap().get_mut(&pos).map(f)
    }

    /// Рецепты, топливо и остатки для печи вида `kind`.
    fn furnace_tick(&self, f: &mut Furnace) -> bool {
        let k = f.kind;
        let recipe = |item: i32| {
            self.survival
                .recipes
                .cooking(k, item)
                .map(|c| furnace::Cook {
                    result: c.result.0,
                    count: c.result.1,
                    time: c.time,
                    experience: c.experience,
                })
        };
        let fuel = |item: i32| {
            let f = self.survival.items.get(item as usize)?.fuel?;
            Some((f.burn[k as usize], f.speed[k as usize]))
        };
        let max = |item: i32| self.max_stack(item);
        let rem = |item: i32| self.craft_remainder(item);
        f.tick(&furnace::Env {
            recipe: &recipe,
            fuel: &fuel,
            max_stack: &max,
            remainder: &rem,
            sponge: self.sponge,
        })
    }

    /// Предметы печи из окна игрока `me`: сохранить и показать остальным.
    fn set_furnace_slots(&self, pos: [i32; 3], slots: [Slot; 3], me: Uuid) {
        let k = self.with_furnace(pos, |e| e.f.kind).unwrap_or(0);
        let recipe = |item: i32| {
            self.survival
                .recipes
                .cooking(k, item)
                .map(|c| furnace::Cook {
                    result: c.result.0,
                    count: c.result.1,
                    time: c.time,
                    experience: c.experience,
                })
        };
        let none = |_: i32| None;
        let max = |item: i32| self.max_stack(item);
        let rem = |_: i32| None;
        let env = furnace::Env {
            recipe: &recipe,
            fuel: &none,
            max_stack: &max,
            remainder: &rem,
            sponge: None,
        };
        let Some((fur, viewers, xp)) = self.with_furnace(pos, |e| {
            // Результат забрали — опыт за всё сготовленное (`FurnaceResultSlot`).
            let taken = e.f.slots[furnace::RESULT].map_or(0, |r| r.count)
                > slots[furnace::RESULT].map_or(0, |r| r.count);
            e.f.set_slots(slots, &env);
            let xp = if taken {
                e.f.take_xp(rand::random())
            } else {
                0
            };
            (e.f.clone(), e.viewers.clone(), xp)
        }) else {
            return;
        };
        if xp > 0
            && let Some((feet, _, _)) = self.player_at(me)
        {
            self.spawn_orbs(feet, xp);
        }
        self.redstone_update(&crate::redstone::around(pos));
        self.save_furnace(pos, &fur);
        let controls = self.controls.lock().unwrap();
        for v in viewers.iter().filter(|v| **v != me) {
            if let Some(tx) = controls.get(v) {
                let _ = tx.send(Control::Furnace(pos, fur.data(), true));
            }
        }
    }

    /// Записывает печь в блок-сущность (`saveAdditional`).
    fn save_furnace(&self, pos: [i32; 3], f: &Furnace) {
        let mut nbt = Compound::new();
        nbt.insert("Items".into(), Tag::List(self.items_nbt(&f.slots)));
        nbt.insert("cooking_time_spent".into(), Tag::Short(f.cook as i16));
        nbt.insert("cooking_total_time".into(), Tag::Short(f.cook_total as i16));
        nbt.insert(
            "lit_time_remaining".into(),
            Tag::Short(f.lit_remaining as i16),
        );
        nbt.insert("lit_total_time".into(), Tag::Short(f.lit_total as i16));
        nbt.insert("speed_multiplier".into(), Tag::Float(f.speed));
        if f.xp > 0.0 {
            nbt.insert("chunkwake:experience".into(), Tag::Float(f.xp));
        }
        let id = FURNACES[f.kind as usize];
        self.set_block_entity(pos, Some(BlockEntity { id: id.into(), nbt }));
    }

    /// Тик редстоуна: плиты под игроками и отложенные тики (кнопки, факелы,
    /// повторители, лампы, плиты).
    fn tick_redstone(&self, watch: &mut Option<&mut chunkwake_metrics::profile::Stopwatch>) {
        use chunkwake_metrics::profile::*;
        // Отметка фазы — только когда профиль включён; без него всё равно.
        macro_rules! lap {
            ($phase:expr) => {
                if let Some(w) = watch.as_deref_mut() {
                    w.lap($phase);
                }
            };
        }
        let now = self
            .server_ticks
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let simulated = self.simulated_set();
        let deferred = std::mem::take(&mut *self.deferred_updates.lock().unwrap());
        for p in deferred {
            // Правка плагина в нетикающем чанке подождёт своей симуляции.
            if !simulated.is_empty() && !simulated.contains(&(p[0] >> 4, p[2] >> 4)) {
                self.deferred_updates.lock().unwrap().push(p);
                self.mark_work();
                continue;
            }
            self.redstone_update(&crate::redstone::around(p));
        }
        lap!(PH_DEFERRED);
        self.scan_tripwires();
        lap!(PH_PLATES);
        self.scan_plates();
        lap!(PH_PLATES);
        self.tick_fluids(now);
        lap!(PH_FLUIDS);
        self.finish_moves(now);
        lap!(PH_PISTONS);
        self.tick_hoppers();
        lap!(PH_HOPPERS);
        let due: Vec<[i32; 3]> = {
            let mut list = self.redstone_ticks.lock().unwrap();
            if list.is_empty() {
                lap!(PH_SCHEDULED);
                return;
            }
            // Расписание вдалеке не теряется: тик подождёт, пока чанк в симуляции.
            list.take_due_in(now, |p| {
                simulated.is_empty() || simulated.contains(&(p[0] >> 4, p[2] >> 4))
            })
        };
        for pos in due {
            self.scheduled_tick(pos, now);
        }
        lap!(PH_SCHEDULED);
    }

    /// Отложить тик блока в `pos` через `delay` тиков, если ещё не отложен.
    fn schedule(&self, pos: [i32; 3], delay: u64) {
        self.enqueue_tick(pos, delay, false);
    }

    /// Смена входа диода: срок тика становится `delay` от текущего тика.
    fn reschedule(&self, pos: [i32; 3], delay: u64) {
        self.enqueue_tick(pos, delay, true);
    }

    fn enqueue_tick(&self, pos: [i32; 3], delay: u64, replace: bool) {
        let at = self.server_ticks.load(std::sync::atomic::Ordering::Relaxed) + delay;
        self.redstone_ticks
            .lock()
            .unwrap()
            .schedule(pos, at, replace);
        self.mark_work();
    }

    /// `TripWireBlock.entityInside`: нить под сущностью включается.
    ///
    /// Ванила зовёт `entityInside` при каждом движении сущности, то есть на
    /// каждом тике, и проверяет только ту клетку, где сущность оказалась.
    /// Здесь так же: клетки берутся из позиций игроков, мобов и предметов,
    /// и только те, где действительно лежит нить.
    fn scan_tripwires(&self) {
        // Быстрый выход: никто не двигается — нечего обходить.
        let players = self.hub.players.lock().unwrap();
        if players.values().all(|p| !p.collects)
            && self.mobs.lock().unwrap().values().all(|e| !e.mob.alive())
            && self.dropped.lock().unwrap().is_empty()
        {
            return;
        }
        let mut spots: Vec<[i32; 3]> = {
            let live = self.live.lock().unwrap();
            players
                .values()
                .filter(|p| p.collects)
                .filter(|p| live.get(&p.entry.uuid).is_some_and(|l| l.health > 0.0))
                .map(|p| {
                    [
                        p.pos[0].floor() as i32,
                        p.pos[1].floor() as i32,
                        p.pos[2].floor() as i32,
                    ]
                })
                .collect()
        };
        drop(players);
        spots.extend(
            self.mobs
                .lock()
                .unwrap()
                .values()
                .filter(|e| e.mob.alive())
                .map(|e| {
                    [
                        e.mob.pos[0].floor() as i32,
                        e.mob.pos[1].floor() as i32,
                        e.mob.pos[2].floor() as i32,
                    ]
                }),
        );
        spots.extend(self.dropped.lock().unwrap().values().map(|d| {
            [
                d.pos[0].floor() as i32,
                d.pos[1].floor() as i32,
                d.pos[2].floor() as i32,
            ]
        }));
        spots.sort();
        spots.dedup();
        for p in spots {
            // Нить занимает ту же клетку, что и ноги сущности.
            if self
                .block(p)
                .is_some_and(|s| chunkwake_world::has_rare(s, chunkwake_world::rare::TRIPWIRE))
            {
                let pl = &self.survival.placement;
                let state = self.block(p).unwrap_or(AIR);
                let attached = pl.prop(state, "attached") == Some("true");
                let powered = pl.prop(state, "powered") == Some("true");
                // Уже включённая нить ждёт своего тика: `entityInside` в
                // ванили выходит, если `powered` или тик уже запланирован.
                if !powered && self.tripwire_pressed(p, attached) {
                    self.tripwire_check(p);
                }
            }
        }
    }

    /// Игроки на плитах: нажать свободную (`checkPressed`), счёт для утяжелённых.
    fn scan_plates(&self) {
        // Некого проверять — плит касаться не нужно. Раньше здесь каждый тик
        // собирались пустые списки, брался замок и `plates` заменялся пустым
        // словарём, то есть простой сервер платил за полный обход.
        if self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .all(|p| !p.collects)
            && self.mobs.lock().unwrap().values().all(|e| !e.mob.alive())
            && self.dropped.lock().unwrap().is_empty()
        {
            if !self.plates.lock().unwrap().is_empty() {
                self.plates.lock().unwrap().clear();
            }
            return;
        }
        let feet: Vec<[i32; 3]> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.collects)
            .map(|p| {
                [
                    p.pos[0].floor() as i32,
                    (p.pos[1] + 0.01).floor() as i32,
                    p.pos[2].floor() as i32,
                ]
            })
            .collect();
        // Живые мобы жмут все плиты, как игроки.
        let mut feet = feet;
        feet.extend(
            self.mobs
                .lock()
                .unwrap()
                .values()
                .filter(|e| e.mob.alive())
                .map(|e| {
                    let p = e.mob.pos;
                    [
                        p[0].floor() as i32,
                        (p[1] + 0.01).floor() as i32,
                        p[2].floor() as i32,
                    ]
                }),
        );
        // Плиты ищутся флагом состояния: имя блока тут нужно было только для
        // «это плита ли», а имя — это поиск в таблице и срез строки.
        use chunkwake_world::extra as xf;
        let mut on: FastMap<[i32; 3], usize> = FastMap::default();
        for f in feet {
            if self
                .block(f)
                .is_some_and(|s| chunkwake_world::has_extra(s, xf::PLATE))
            {
                *on.entry(f).or_default() += 1;
            }
        }
        // Предметы жмут деревянные и утяжелённые плиты (каменные — только живые).
        // По клетке — число предметов: блок под стопкой читается один раз, а не на предмет.
        let mut items: FastMap<[i32; 3], usize> = FastMap::default();
        for d in self.dropped.lock().unwrap().values() {
            let f = [
                d.pos[0].floor() as i32,
                (d.pos[1] + 0.01).floor() as i32,
                d.pos[2].floor() as i32,
            ];
            *items.entry(f).or_default() += 1;
        }
        for (f, n) in items {
            if self.block(f).is_some_and(|s| {
                chunkwake_world::has_extra(s, xf::PLATE)
                    && !chunkwake_world::has_extra(s, xf::LIVING_PLATE)
            }) {
                *on.entry(f).or_default() += n;
            }
        }
        let fresh: Vec<[i32; 3]> = on.keys().copied().collect();
        *self.plates.lock().unwrap() = on;
        for p in fresh {
            let Some(state) = self.block(p) else { continue };
            let pl = &self.survival.placement;
            let pressed = pl.prop(state, "powered") == Some("true")
                || pl.prop(state, "power").is_some_and(|v| v != "0");
            // Утяжелённая плита пересчитывает силу каждый тик, пока на ней кто-то есть.
            if !pressed || chunkwake_world::has_extra(state, xf::WEIGHTED_PLATE) {
                self.update_plate(p);
            }
        }
    }

    /// Пересчитать плиту (`checkPressed`): сила, звук, соседи, повторная проверка.
    fn update_plate(&self, p: [i32; 3]) {
        let Some(state) = self.block(p) else { return };
        if !chunkwake_world::has_extra(state, chunkwake_world::extra::PLATE) {
            return;
        }
        // Имя нужно только дальше и только одно: строки на каждую проверку
        // плиты в тике не было смысла аллоцировать.
        let name = self.state_name(state);
        let weighted = chunkwake_world::has_extra(state, chunkwake_world::extra::WEIGHTED_PLATE);
        let pl = &self.survival.placement;
        let count = self.plates.lock().unwrap().get(&p).copied().unwrap_or(0);
        let mut power = crate::redstone::plate_power(name, count);
        let was = if weighted {
            pl.prop(state, "power")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        } else if pl.prop(state, "powered") == Some("true") {
            15
        } else {
            0
        };
        if was != power {
            power = self.redstone_event(p, was, power);
        }
        let (old, next) = if weighted {
            let old: i32 = pl
                .prop(state, "power")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            (old, pl.with_prop(state, "power", &power.to_string()))
        } else {
            let old = if pl.prop(state, "powered") == Some("true") {
                15
            } else {
                0
            };
            (
                old,
                pl.with_prop(state, "powered", if power > 0 { "true" } else { "false" }),
            )
        };
        if power > 0 {
            self.schedule(p, crate::redstone::plate_ticks(name));
        }
        if (old > 0) == (power > 0) && old == power {
            return;
        }
        if let Some(next) = next {
            self.set_block(p, next);
        }
        if (old > 0) != (power > 0) {
            self.block_sound(
                None,
                &crate::redstone::plate_sound(name, power > 0),
                p,
                1.0,
                1.0,
            );
        }
        self.redstone_update(&crate::redstone::around(p));
        self.redstone_update(&crate::redstone::around(crate::redstone::step(p, 0)));
    }

    /// Отложенный тик блока: кнопка отжимается, факел гаснет или загорается,
    /// повторитель переключается, лампа гаснет, плита проверяется.
    /// `TargetBlock.updateRedstoneOutput`: попадание выставляет `power` по
    /// расстоянию от центра блока и планирует сброс.
    ///
    /// Ось попадания — та, куда летела стрела; `dist` считается по двум
    /// другим осям, поэтому центр даёт 15, край — 1.
    fn target_hit(&self, pos: [f64; 3], vel: [f64; 3]) {
        let at = [
            pos[0].floor() as i32,
            pos[1].floor() as i32,
            pos[2].floor() as i32,
        ];
        let Some(state) = self.block(at) else { return };
        let name = self.state_name(state);
        if name != "minecraft:target" {
            return;
        }
        let axis = dominant_axis(vel);
        let frac = |v: f64| (v - v.floor() - 0.5).abs();
        let (dx, dy, dz) = (frac(pos[0]), frac(pos[1]), frac(pos[2]));
        // `TargetBlock.getRedstoneStrength`: по оси попадания берутся две
        // другие оси — Y → (X, Z), Z → (X, Y), X → (Y, Z).
        let dist = match axis {
            1 => dx.max(dz),
            2 => dx.max(dy),
            _ => dy.max(dz),
        };
        // `Mth.ceil(15 * Mth.clamp((0.5 - dist) / 0.5, 0, 1))`, минимум 1.
        let strength = ((0.5 - dist) / 0.5).clamp(0.0, 1.0);
        let power = (15.0 * strength).ceil().max(1.0) as i32;
        let pl = &self.survival.placement;
        let Some(next) = pl.with_prop(state, "power", &power.to_string()) else {
            return;
        };
        self.set_block(at, next);
        self.update_power(next, at);
        // `ACTIVATION_TICKS_ARROWS` 20 для стрел, 8 для прочих снарядов.
        self.schedule(at, 20);
    }

    /// `DaylightDetectorBlock.updateSignalStrength`: сила сигнала — небесный
    /// свет в клетке, у инвертированного датчика — дополнение до 15.
    ///
    /// Угол солнца берётся из таймлайна `OVERWORLD_DAY`: ключевые кадры
    /// `6000 → 0°` и `6000 → 360°` (к следующему полудню) с easing
    /// `symmetricCubicBezier(0.362, 0.241)`. После полудня (`π..2π`) угол
    /// тянется к `2π` с весом 0.2, и `power` умножается на косинус.
    fn update_daylight(&self, pos: [i32; 3]) {
        let Some(state) = self.block(pos) else { return };
        let pl = &self.survival.placement;
        if self.state_name(state) != "minecraft:daylight_detector" {
            return;
        }
        let inverted = pl.prop(state, "inverted") == Some("true");
        let sky = self.with_world(|w| w.sky_light_at(pos)).unwrap_or(0) as i32;
        let mut target = sky;
        if inverted {
            target = 15 - target;
        } else if target > 0 {
            let day = self.day_time.load(Ordering::Relaxed).rem_euclid(24000) as f32;
            let mut angle = sun_angle(day) * (std::f32::consts::PI / 180.0);
            let offset = if angle < std::f32::consts::PI {
                0.0
            } else {
                std::f32::consts::PI * 2.0
            };
            angle += (offset - angle) * 0.2;
            target = (target as f32 * angle.cos()).round() as i32;
        }
        let target = target.clamp(0, 15);
        let Some(next) = pl.with_prop(state, "power", &target.to_string()) else {
            return;
        };
        if next != state {
            self.set_block(pos, next);
            self.update_power(next, pos);
        }
    }

    fn scheduled_tick(&self, pos: [i32; 3], now: u64) {
        use crate::redstone as rs;
        let Some(state) = self.block(pos) else { return };
        let name = self.state_name(state).to_owned();
        let pl = &self.survival.placement;
        let at = |p: [i32; 3]| self.block(p).unwrap_or(AIR);
        let w = rs::World {
            pl,
            at: &at,
            output: &|p| self.comparator_output(p),
            analog: &|p| self.analog_at(p),
        };
        if rs::is_button(&name) {
            if let Some(off) = pl.with_prop(state, "powered", "false") {
                self.set_block(pos, off);
                let (sound, v, p) = rs::click_sound(&name, false);
                self.block_sound(None, &sound, pos, v, p);
                self.update_power(off, pos);
            }
        } else if rs::is_plate(&name) {
            self.update_plate(pos);
        } else if rs::is_tripwire(&name) {
            // `TripWireBlock.tick`: перепроверить, стоит ли кто-то на нити.
            if pl.prop(state, "powered") == Some("true") {
                self.tripwire_check(pos);
            }
        } else if rs::is_tripwire_hook(&name) {
            // `TripWireHookBlock.tick`: полный пересчёт цепи.
            self.tripwire_hook_state(pos, false, true, -1, None);
        } else if crate::pistons::is_piston(&name) {
            self.check_piston(pos);
        } else if name == "minecraft:dispenser" || name == "minecraft:dropper" {
            self.dispense(pos);
        } else if rs::is_observer(&name) {
            // Импульс: включиться на 2 тика, затем погаснуть.
            let on = pl.prop(state, "powered") != Some("true");
            if on {
                self.schedule(pos, 2);
            }
            if let Some(n) = pl.with_prop(state, "powered", if on { "true" } else { "false" }) {
                let back = w.observer_back(pos);
                self.set_block(pos, n);
                self.redstone_update(&rs::around(back));
            }
        } else if name == "minecraft:target" {
            // `TargetBlock.ACTIVATION_TICKS_*`: через свой срок `power`
            // возвращается в ноль.
            if let Some(off) = pl.with_prop(state, "power", "0") {
                self.set_block(pos, off);
                self.update_power(off, pos);
            }
        } else if name == "minecraft:daylight_detector" {
            // `DaylightDetectorBlock.updateSignalStrength`: цель — небесный
            // свет в клетке, дальше `power` от угла солнца.
            self.update_daylight(pos);
        } else if rs::is_comparator(&name) {
            let out = w.comparator_target(pos);
            let old = self.comparator_output(pos);
            let powered = if out > 0 { "true" } else { "false" };
            if out != old || pl.prop(state, "powered") != Some(powered) {
                self.comparators.lock().unwrap().insert(pos, out);
                let mut nbt = self.block_entity(pos).map(|b| b.nbt).unwrap_or_default();
                nbt.insert("OutputSignal".into(), Tag::Int(out));
                self.set_block_entity(
                    pos,
                    Some(BlockEntity {
                        id: "minecraft:comparator".into(),
                        nbt,
                    }),
                );
                if let Some(n) = pl.with_prop(state, "powered", powered) {
                    self.set_block(pos, n);
                }
                let front = w.repeater_front(pos);
                self.redstone_update(&rs::around(pos));
                self.redstone_update(&rs::around(front));
            }
        } else if rs::is_lamp(&name) {
            if pl.prop(state, "lit") == Some("true")
                && !w.powered(pos)
                && let Some(off) = pl.with_prop(state, "lit", "false")
            {
                self.set_block(pos, off);
            }
        } else if rs::is_torch(&name) {
            let lit = pl.prop(state, "lit") == Some("true");
            let off = w.torch_off(pos);
            // `isToggledTooFrequently`: 8 переключений за 60 тиков — перегорел.
            let mut toggles = self.torch_toggles.lock().unwrap();
            let list = toggles.entry(pos).or_default();
            list.retain(|&t| t + rs::BURNOUT_WINDOW > now);
            let mut cools = self.torch_cool.lock().unwrap();
            let next = if lit && off {
                list.push(now);
                if list.len() >= rs::BURNOUT_COUNT {
                    let pitch = 2.6 + (rand::random::<f32>() - rand::random::<f32>()) * 0.8;
                    self.block_sound(None, rs::BURNOUT, pos, 0.5, pitch);
                    cools.insert(pos, now + rs::BURNOUT_COOLDOWN);
                    self.schedule(pos, rs::BURNOUT_COOLDOWN);
                }
                Some("false")
            } else if !lit && !off && cools.get(&pos).is_some_and(|until| *until > now) {
                None
            } else if !lit && !off && list.len() < rs::BURNOUT_COUNT {
                cools.remove(&pos);
                Some("true")
            } else {
                None
            };
            drop(cools);
            if list.is_empty() {
                toggles.remove(&pos);
            }
            drop(toggles);
            if let Some(v) = next
                && let Some(n) = pl.with_prop(state, "lit", v)
            {
                self.set_block(pos, n);
                self.redstone_update(&rs::around(pos));
                self.redstone_update(&rs::around(rs::step(pos, 1)));
            }
        } else if rs::is_repeater(&name) {
            if w.repeater_locked(pos) {
                return;
            }
            let powered = pl.prop(state, "powered") == Some("true");
            let input = w.repeater_input(pos);
            // К концу задержки вход уже выключен — импульс не проходит.
            if powered == input {
                return;
            }
            let next = if input { "true" } else { "false" };
            if let Some(n) = pl.with_prop(state, "powered", next) {
                let front = w.repeater_front(pos);
                self.set_block(pos, n);
                self.redstone_update(&rs::around(pos));
                self.redstone_update(&rs::around(front));
            }
        }
    }

    /// Изменение около `origins`: опоры компонентов, форма и сила проводов,
    /// реакция дверей, ламп, факелов и повторителей (`neighborChanged`).
    fn redstone_update(&self, origins: &[[i32; 3]]) {
        use crate::redstone as rs;
        let pl = &self.survival.placement;
        let mut pending: Vec<[i32; 3]> = origins.to_vec();
        // Блоки волны: один лок мира на позицию, а не на каждый запрос сигнала.
        let cache: std::cell::RefCell<HashMap<[i32; 3], Option<u32>>> = Default::default();
        let get = |p: [i32; 3]| *cache.borrow_mut().entry(p).or_insert_with(|| self.block(p));
        let put = |p: [i32; 3], n: u32| {
            self.set_block(p, n);
            cache.borrow_mut().insert(p, Some(n));
        };
        // Провода, чья сеть уже решена в этом вызове: повторный `solve` дал бы то же.
        let mut solved: HashSet<[i32; 3]> = HashSet::new();
        for _ in 0..64 {
            if pending.is_empty() {
                break;
            }
            let at = |p: [i32; 3]| get(p).unwrap_or(AIR);
            let w = rs::World {
                pl,
                at: &at,
                output: &|p| self.comparator_output(p),
                analog: &|p| self.analog_at(p),
            };
            let mut next: Vec<[i32; 3]> = Vec::new();
            let mut wires = Vec::new();
            for &p in &pending {
                let s = at(p);
                if s == AIR {
                    continue;
                }
                // Опора в незагруженном чанке — не трогать.
                if w.supported(p) == Some(false) && (0..6).all(|d| get(rs::step(p, d)).is_some()) {
                    self.pop_block(p, s);
                    cache.borrow_mut().clear();
                    solved.clear();
                    next.extend_from_slice(&rs::around(p));
                    continue;
                }
                if !rs::is_wire(self.state_name(s)) {
                    continue;
                }
                let shape = w.wire_shape(p);
                let mut n = s;
                for (i, v) in shape.iter().enumerate() {
                    if let Some(m) = pl.with_prop(n, rs::DIRS[i + 2], v) {
                        n = m;
                    }
                }
                if n != s {
                    solved.clear();
                    put(p, n);
                    next.extend_from_slice(&rs::around(p));
                }
                if !solved.contains(&p) {
                    wires.push(p);
                }
            }
            // Изменённые провода сети — одним событием плагинам.
            let mut changes = Vec::new();
            for (p, power) in w.solve(&wires) {
                solved.insert(p);
                let old: i32 = pl
                    .prop(at(p), "power")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                if old != power {
                    changes.push((p, old, power));
                }
            }
            let powers = if changes.is_empty() {
                Vec::new()
            } else {
                self.redstone_events(&changes)
            };
            for (&(p, _, _), power) in changes.iter().zip(powers) {
                let s = at(p);
                if let Some(n) = pl.with_prop(s, "power", &power.to_string())
                    && n != s
                {
                    put(p, n);
                    next.extend_from_slice(&rs::around(p));
                }
            }
            for &p in &pending {
                let s = at(p);
                // Две самые частые проверки — по флагам состояния: имя
                // (срез строки из таблицы) нужно только остальным веткам.
                if rs::openable(self.state_name(s)) {
                    self.refresh_openable(p, true);
                    cache.borrow_mut().clear();
                    continue;
                }
                // Нить и крюк пересчитывают соединения и цепь по соседу.
                if chunkwake_world::has_rare(s, chunkwake_world::rare::TRIPWIRE) {
                    self.refresh_tripwire_shape(p);
                    continue;
                }
                // Нотный блок: `updateShape` по вертикальному соседу заново
                // выбирает инструмент (поставили камень под — звук сменился),
                // а `neighborChanged` играет ноту на фронте сигнала.
                if rs::is_note_block(self.state_name(s)) {
                    let tuned = self.note_instrument_at(p, s);
                    if tuned != s {
                        put(p, tuned);
                        next.extend_from_slice(&rs::around(p));
                    }
                    self.note_signal_update(p);
                    continue;
                }
                // У крюка в ванили нет `neighborChanged`: цепь считается
                // только по своему тику, который планирует нить. Немедленный
                // пересчёт здесь дал бы рекурсию через `notifyNeighbors`.
                if chunkwake_world::has_rare(s, chunkwake_world::rare::TRIPWIRE_HOOK) {
                    continue;
                }
                if rs::is_torch(self.state_name(s)) {
                    if (pl.prop(s, "lit") == Some("true")) == w.torch_off(p) {
                        self.schedule(p, 2);
                    }
                    continue;
                }
                let name = self.state_name(s);
                if rs::is_lamp(name) {
                    let lit = pl.prop(s, "lit") == Some("true");
                    let powered = w.powered(p);
                    if powered && !lit {
                        if self.redstone_event(p, 0, 15) == 15
                            && let Some(n) = pl.with_prop(s, "lit", "true")
                        {
                            put(p, n);
                        }
                    } else if !powered && lit {
                        self.schedule(p, 4);
                    }
                } else if rs::is_torch(name) {
                    if (pl.prop(s, "lit") == Some("true")) == w.torch_off(p) {
                        self.schedule(p, 2);
                    }
                } else if crate::pistons::is_piston(name) {
                    self.check_piston(p);
                    cache.borrow_mut().clear();
                    solved.clear();
                } else if name == "minecraft:hopper" {
                    // Сигнал выключает воронку (`HopperBlock.checkPoweredState`).
                    let enabled = if w.powered(p) { "false" } else { "true" };
                    if let Some(n) = pl.with_prop(s, "enabled", enabled)
                        && n != s
                    {
                        put(p, n);
                    }
                    let now = self.server_ticks.load(std::sync::atomic::Ordering::Relaxed);
                    self.hopper_ticks.lock().unwrap().schedule(p, now, true);
                    self.mark_work();
                } else if name == "minecraft:dispenser" || name == "minecraft:dropper" {
                    // Фронт сигнала (и квази-связность сверху) — выдать через 4 тика.
                    let powered = w.powered(p) || w.powered(rs::step(p, 1));
                    let triggered = pl.prop(s, "triggered") == Some("true");
                    if powered != triggered {
                        if powered {
                            self.schedule(p, crate::hoppers::DISPENSE_DELAY);
                        }
                        if let Some(n) =
                            pl.with_prop(s, "triggered", if powered { "true" } else { "false" })
                        {
                            put(p, n);
                        }
                    }
                } else if rs::is_comparator(name) {
                    if w.comparator_target(p) != self.comparator_output(p) {
                        self.reschedule(p, 2);
                    }
                } else if rs::is_repeater(name) {
                    let locked = w.repeater_locked(p);
                    if let Some(n) =
                        pl.with_prop(s, "locked", if locked { "true" } else { "false" })
                        && n != s
                    {
                        put(p, n);
                    }
                    if !locked && (pl.prop(s, "powered") == Some("true")) != w.repeater_input(p) {
                        self.reschedule(p, crate::redstone::repeater_delay(pl, s));
                    }
                }
            }
            next.sort();
            next.dedup();
            pending = next;
        }
    }

    /// Поршень в `p`: выдвинуться или втянуться по сигналу (`checkIfExtend`).
    fn check_piston(&self, p: [i32; 3]) {
        use crate::pistons as ps;
        let Some(s) = self.block(p) else { return };
        let pl = &self.survival.placement;
        let name = self.state_name(s).to_owned();
        let Some(f) = pl
            .prop(s, "facing")
            .and_then(|v| crate::redstone::DIRS.iter().position(|d| *d == v))
        else {
            return;
        };
        let extended = pl.prop(s, "extended") == Some("true");
        let at = |q: [i32; 3]| self.block(q).unwrap_or(AIR);
        let w = crate::redstone::World {
            pl,
            at: &at,
            output: &|q| self.comparator_output(q),
            analog: &|q| self.analog_at(q),
        };
        let hardness = |st: u32| {
            self.survival
                .hardness
                .get(st as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let c = ps::Ctx {
            pl,
            at: &at,
            hardness: &hardness,
            min_y: self.min_y,
            max_y: self.min_y + self.template.height() as i32 - 1,
        };
        let should = c.powered(&w, p, f);
        if should == extended {
            return;
        }
        let head = crate::redstone::step(p, f);
        let front = crate::redstone::step(head, f);
        // Втягивание во время выдвижения (`triggerEvent`): движение доезжает сразу,
        // а липкий не тянет блок, который ещё ехал (тип 2 — «выплёвывает»).
        let mut spit = false;
        if !should {
            let front_moving = self
                .piston_moves
                .lock()
                .unwrap()
                .iter()
                .any(|(_, m)| m.iter().any(|(q, _, _)| *q == front));
            if self.land_now(&[p, head, front]) {
                spit = front_moving;
            }
        }
        // Выдвижение, пока ещё едет, — проверить после остановки.
        if should
            && self
                .piston_moves
                .lock()
                .unwrap()
                .iter()
                .any(|(_, m)| m.iter().any(|(q, _, _)| *q == p || *q == head))
        {
            self.schedule(p, ps::MOVE_TICKS);
            return;
        }
        let sticky = name == "minecraft:sticky_piston";
        let ty = if sticky { "sticky" } else { "normal" };
        let block = self
            .survival
            .block_types
            .get(name.as_str())
            .copied()
            .unwrap_or(0);
        let mut finals = Vec::new();
        if should {
            let Some(plan) = ps::resolve(&c, p, f, true) else {
                return;
            };
            if self.piston_event(p, f, true, &plan.push) {
                return;
            }
            self.hub.send(
                None,
                play::block_event(self.ids.block_event, p, 0, f as u8, block),
            );
            self.piston_move(&plan, f, f, ty, &mut finals);
            self.claim(&[head]);
            if let Some(h) = self.moving_state(f, ty) {
                self.set_block(head, h);
            }
            if let Some(h) = self
                .survival
                .block_defaults
                .get("minecraft:piston_head")
                .and_then(|&d| pl.with_prop(d, "facing", crate::redstone::DIRS[f]))
                .and_then(|d| pl.with_prop(d, "type", ty))
                .and_then(|d| pl.with_prop(d, "short", "false"))
            {
                finals.push((head, h, None));
            }
            if let Some(e) = pl.with_prop(s, "extended", "true") {
                self.set_block(p, e);
            }
            let pitch = rand::random::<f32>() * 0.25 + 0.6;
            self.block_sound(None, "minecraft:block.piston.extend", p, 0.5, pitch);
        } else {
            let fs = at(front);
            let plan = if sticky && !spit && fs != AIR && c.reaction(fs) == ps::Reaction::Normal {
                // `moveBlocks` убирает голову до поиска: иначе она — неподвижный блок на пути.
                let clear = |q: [i32; 3]| if q == head { AIR } else { at(q) };
                let c2 = ps::Ctx { at: &clear, ..c };
                ps::resolve(&c2, p, f, false)
            } else {
                None
            };
            let moved = plan.as_ref().map_or(&[][..], |pl| &pl.push[..]);
            if self.piston_event(p, f, false, moved) {
                return;
            }
            self.hub.send(
                None,
                play::block_event(self.ids.block_event, p, 1, f as u8, block),
            );
            self.claim(&[p]);
            if let Some(m) = self.moving_state(f, ty) {
                self.set_block(p, m);
            }
            if let Some(e) = pl.with_prop(s, "extended", "false") {
                finals.push((p, e, None));
            }
            if at(head) != AIR && self.state_name(at(head)) == "minecraft:piston_head" {
                self.set_block(head, AIR);
            }
            if let Some(plan) = plan {
                self.piston_move(&plan, f ^ 1, f, ty, &mut finals);
            }
            let pitch = rand::random::<f32>() * 0.15 + 0.6;
            self.block_sound(None, "minecraft:block.piston.contract", p, 0.5, pitch);
        }
        let due = self.server_ticks.load(std::sync::atomic::Ordering::Relaxed) + ps::MOVE_TICKS;
        self.piston_moves.lock().unwrap().push((due, finals));
        self.mark_work();
        self.redstone_update(&crate::redstone::around(head));
    }

    /// Новое движение занимает места: прежние незаконченные движения туда уже не приедут.
    fn claim(&self, at: &[[i32; 3]]) {
        for (_, m) in self.piston_moves.lock().unwrap().iter_mut() {
            m.retain(|(q, _, _)| !at.contains(q));
        }
    }

    /// `moving_piston` с направлением и типом.
    fn moving_state(&self, f: usize, ty: &str) -> Option<u32> {
        let pl = &self.survival.placement;
        let d = *self
            .survival
            .block_defaults
            .get("minecraft:moving_piston")?;
        pl.with_prop(d, "facing", crate::redstone::DIRS[f])
            .and_then(|d| pl.with_prop(d, "type", ty))
    }

    /// `moveBlocks`: ломает хрупкие, освобождает старые места, ставит `moving_piston`
    /// на новые; настоящие блоки — в `finals` до конца движения.
    fn piston_move(
        &self,
        plan: &crate::pistons::Plan,
        dir: usize,
        facing: usize,
        ty: &str,
        finals: &mut Moves,
    ) {
        for &d in plan.destroy.iter().rev() {
            if let Some(ds) = self.block(d) {
                self.pop_block(d, ds);
            }
        }
        let states: Vec<u32> = plan
            .push
            .iter()
            .map(|&q| self.block(q).unwrap_or(AIR))
            .collect();
        let carried: Vec<Option<BlockEntity>> =
            plan.push.iter().map(|&q| self.block_entity(q)).collect();
        let dests: Vec<[i32; 3]> = plan
            .push
            .iter()
            .map(|&q| crate::redstone::step(q, dir))
            .collect();
        for &q in plan.push.iter().rev() {
            if !dests.contains(&q) {
                self.set_block(q, AIR);
                self.set_block_entity(q, None);
            }
        }
        self.claim(&dests);
        // Тип `moving_piston` — липкий или обычный, как у самого поршня.
        let moving = self.moving_state(facing, ty);
        for (i, &d) in dests.iter().enumerate() {
            if let Some(m) = moving {
                self.set_block(d, m);
            }
            finals.push((d, states[i], carried[i].clone()));
        }
    }

    /// Блоки, доехавшие до места: настоящие состояния вместо `moving_piston`.
    fn finish_moves(&self, now: u64) {
        let touched = self.land_moves(|m| m.0 <= now);
        self.redstone_update(&touched);
    }

    /// `PistonMovingBlockEntity.finalTick` для движений, задевающих `at`, сразу;
    /// соседи обновятся в следующем тике. `true` — что-то ещё ехало.
    fn land_now(&self, at: &[[i32; 3]]) -> bool {
        let touched = self.land_moves(|m| m.1.iter().any(|(q, _, _)| at.contains(q)));
        let any = !touched.is_empty();
        self.deferred_updates.lock().unwrap().extend(touched);
        any
    }

    /// Поставить настоящие блоки движений, выбранных `pick`; изменённые места вокруг.
    fn land_moves(&self, pick: impl Fn(&(u64, Moves)) -> bool) -> Vec<[i32; 3]> {
        let due: Vec<Moves> = {
            let mut moves = self.piston_moves.lock().unwrap();
            if moves.is_empty() {
                return Vec::new();
            }
            let (due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut *moves)
                .into_iter()
                .partition(|m| pick(m));
            *moves = keep;
            due.into_iter().map(|m| m.1).collect()
        };
        let mut touched = Vec::new();
        for (p, st, be) in due.into_iter().flatten() {
            if self
                .block(p)
                .is_some_and(|s| self.state_name(s) == "minecraft:moving_piston")
            {
                self.set_block(p, st);
                self.set_block_entity(p, be);
                touched.extend_from_slice(&crate::redstone::around(p));
                // `ObserverBlock.onPlace`: приехал включённым без своего тика — гаснет сразу.
                let pl = &self.survival.placement;
                // `updateFromNeighbourShapes` → `ObserverBlock.updateShape`: выключенный
                // наблюдатель, доехав, всегда даёт импульс — на этом держатся флай-машины.
                if chunkwake_world::has_flag(st, chunkwake_world::flags::OBSERVER)
                    && pl.prop(st, "powered") != Some("true")
                {
                    self.schedule(p, 2);
                }
                if chunkwake_world::has_flag(st, chunkwake_world::flags::OBSERVER)
                    && pl.prop(st, "powered") == Some("true")
                    && !self.redstone_ticks.lock().unwrap().contains(p)
                    && let Some(off) = pl.with_prop(st, "powered", "false")
                {
                    self.set_block(p, off);
                    let at = |q: [i32; 3]| self.block(q).unwrap_or(AIR);
                    let w = crate::redstone::World {
                        pl,
                        at: &at,
                        output: &|q| self.comparator_output(q),
                        analog: &|q| self.analog_at(q),
                    };
                    touched.extend_from_slice(&crate::redstone::around(w.observer_back(p)));
                }
            }
        }
        touched.sort();
        touched.dedup();
        touched
    }

    /// `BlockPistonExtendEvent` / `BlockPistonRetractEvent`; `true` — отменено.
    fn piston_event(&self, p: [i32; 3], f: usize, extend: bool, moved: &[[i32; 3]]) -> bool {
        self.sync_event(Event::Piston, || {
            let list: Vec<String> = moved
                .iter()
                .map(|q| format!("{},{},{}", q[0], q[1], q[2]))
                .collect();
            format!(
                "{} {} {} {} {} {}",
                p[0],
                p[1],
                p[2],
                crate::redstone::DIRS[f],
                u8::from(extend),
                list.join(";")
            )
        })
        .is_some_and(|r| r.trim() == "c")
    }

    /// Сломана голова поршня — ломается и основание; сломано выдвинутое основание — исчезает голова.
    fn piston_broken(&self, pos: [i32; 3], old: u32) {
        let pl = &self.survival.placement;
        let name = self.state_name(old);
        let Some(f) = pl
            .prop(old, "facing")
            .and_then(|v| crate::redstone::DIRS.iter().position(|d| *d == v))
        else {
            return;
        };
        if name == "minecraft:piston_head" {
            let base = crate::redstone::step(pos, f ^ 1);
            if let Some(b) = self.block(base)
                && crate::pistons::is_piston(self.state_name(b))
                && pl.prop(b, "extended") == Some("true")
            {
                self.pop_block(base, b);
            }
        } else if crate::pistons::is_piston(name) && pl.prop(old, "extended") == Some("true") {
            let head = crate::redstone::step(pos, f);
            if self
                .block(head)
                .is_some_and(|h| self.state_name(h) == "minecraft:piston_head")
            {
                self.set_block(head, AIR);
            }
        }
    }

    /// Содержимое контейнера в `p` для воронок и выбрасывателей.
    fn cont_get(&self, p: [i32; 3]) -> Option<Cont> {
        let name = self.state_name(self.block(p)?).to_owned();
        if furnace_kind(&name).is_some() {
            let slots = self.with_furnace(p, |e| e.f.slots)?;
            return Some(Cont {
                parts: vec![p],
                slots: slots.to_vec(),
                furnace: true,
                name,
            });
        }
        container_entity(&name)?;
        let mut parts = vec![p];
        // Двойной сундук: правая половина первой (`CompoundContainer`).
        if let Some((other, right)) = self.chest_partner(p) {
            if right {
                parts.push(other)
            } else {
                parts.insert(0, other)
            }
        }
        let slots = parts
            .iter()
            .flat_map(|&q| self.container_items(q))
            .collect();
        Some(Cont {
            parts,
            slots,
            furnace: false,
            name,
        })
    }

    /// Записать контейнер и показать изменения тем, у кого он открыт.
    fn cont_put(&self, c: &Cont) {
        if c.furnace {
            if let [a, b, r] = c.slots[..] {
                self.set_furnace_slots(c.parts[0], [a, b, r], Uuid::nil());
            }
            return;
        }
        let n = c.slots.len() / c.parts.len();
        for (i, &p) in c.parts.iter().enumerate() {
            self.save_container(p, &c.slots[i * n..(i + 1) * n]);
        }
        let controls = self.controls.lock().unwrap();
        for tx in controls.values() {
            for &p in &c.parts {
                let _ = tx.send(Control::ContainerChanged(p));
            }
        }
    }

    fn is_fuel(&self, item: i32) -> bool {
        self.survival
            .items
            .get(item as usize)
            .is_some_and(|i| i.fuel.is_some())
    }

    /// `InventoryMoveItemEvent`: `None` — отменено, иначе предмет (плагин может заменить).
    fn move_event(&self, from: &Cont, to: &Cont, source_first: bool, item: Stack) -> Option<Stack> {
        let reply = self.sync_event(Event::MoveItem, || {
            let (a, b) = (from.parts[0], to.parts[0]);
            format!(
                "{} {} {} {} {} {} {} {} {} {}",
                a[0],
                a[1],
                a[2],
                from.name,
                b[0],
                b[1],
                b[2],
                to.name,
                u8::from(source_first),
                crate::stack_nbt::to_line(item, self.item_name(item.item))
            )
        });
        match reply.as_deref().map(str::trim) {
            Some("c") => None,
            Some(r) if r.starts_with("i ") => {
                crate::stack_nbt::from_line(&r[2..], |n| self.item_id(n))
                    .map(|s| Stack { count: 1, ..s })
                    .or(Some(item))
            }
            _ => Some(item),
        }
    }

    /// Тик воронок: срабатывают только те, чей срок пришёл (`TickQueue`).
    ///
    /// Раньше здесь был полный проход по карте воронок ради декремента
    /// счётчика: O(всех воронок) каждый тик, хотя перенос идёт 1 раз в 8
    /// тиков. Карта не чистилась при выгрузке чанков, поэтому счётчик рос
    /// вместе с исследованным миром.
    fn tick_hoppers(&self) {
        let now = self.server_ticks.load(std::sync::atomic::Ordering::Relaxed);
        let ready: Vec<[i32; 3]> = {
            let mut queue = self.hopper_ticks.lock().unwrap();
            if queue.is_empty() {
                return;
            }
            let simulated = self.simulated_set();
            // Воронка вне симуляции не переносит: срок остаётся на месте.
            queue.take_due_in(now, |p| {
                simulated.is_empty() || simulated.contains(&(p[0] >> 4, p[2] >> 4))
            })
        };
        for p in ready {
            match self.hopper_step(p) {
                Some(moved) => {
                    let enabled = self.block(p).is_some_and(|s| {
                        self.survival.placement.prop(s, "enabled") != Some("false")
                    });
                    // Включённая воронка ждёт 8 тиков и после неудачной попытки.
                    if moved || enabled {
                        self.hopper_ticks.lock().unwrap().schedule(
                            p,
                            now + u64::from(crate::hoppers::COOLDOWN),
                            true,
                        );
                        self.mark_work();
                    }
                }
                None => {
                    // Воронки здесь больше нет: срок не возвращаем.
                }
            }
        }
    }

    /// `HopperBlockEntity.tryMoveItems`: отдать один предмет вперёд, взять один
    /// сверху (из контейнера или с земли). `None` — воронки здесь больше нет.
    fn hopper_step(&self, p: [i32; 3]) -> Option<bool> {
        use crate::hoppers as hp;
        let s = self.block(p)?;
        if !chunkwake_world::has_extra(s, chunkwake_world::extra::HOPPER) {
            return None;
        }
        let pl = &self.survival.placement;
        if pl.prop(s, "enabled") == Some("false") {
            return Some(false);
        }
        let mut me = self.cont_get(p)?;
        let max = |item: i32| self.max_stack(item);
        let mut moved = false;
        let f = pl
            .prop(s, "facing")
            .and_then(|v| crate::redstone::DIRS.iter().position(|d| *d == v))
            .unwrap_or(0);
        // Отдать: первый непустой слот, что влезает в цель.
        if me.slots.iter().any(Option::is_some)
            && let Some(mut t) = self.cont_get(crate::redstone::step(p, f))
        {
            for i in 0..me.slots.len() {
                let Some(st) = me.slots[i] else { continue };
                let allowed =
                    hp::insert_slots(t.furnace, t.slots.len(), f ^ 1, self.is_fuel(st.item));
                let mut trial = t.slots.clone();
                if !hp::insert_one(&mut trial, &allowed, st, max) {
                    continue;
                }
                let one = Stack { count: 1, ..st };
                let Some(item) = self.move_event(&me, &t, true, one) else {
                    break;
                };
                if item == one {
                    t.slots = trial;
                } else if !hp::insert_one(&mut t.slots, &allowed, item, max) {
                    break;
                }
                hp::take_one(&mut me.slots, i);
                if t.name == "minecraft:hopper" && t.slots.iter().flatten().count() == 1 {
                    let now = self.server_ticks.load(std::sync::atomic::Ordering::Relaxed);
                    self.hopper_ticks.lock().unwrap().schedule(
                        t.parts[0],
                        now + u64::from(hp::COOLDOWN),
                        true,
                    );
                    self.mark_work();
                }
                self.cont_put(&t);
                moved = true;
                break;
            }
        }
        // Взять сверху.
        let full = me
            .slots
            .iter()
            .all(|s| s.is_some_and(|s| s.count >= self.max_stack(s.item)));
        let above = crate::redstone::step(p, 1);
        if !full {
            if let Some(mut a) = self.cont_get(above) {
                let all: Vec<usize> = (0..me.slots.len()).collect();
                for i in hp::extract_slots(a.furnace, a.slots.len()) {
                    let Some(st) = a.slots[i] else { continue };
                    if a.furnace && i == 1 && self.item_name(st.item) != "minecraft:bucket" {
                        continue;
                    }
                    let mut trial = me.slots.clone();
                    if !hp::insert_one(&mut trial, &all, st, max) {
                        continue;
                    }
                    if self
                        .move_event(&a, &me, false, Stack { count: 1, ..st })
                        .is_none()
                    {
                        break;
                    }
                    me.slots = trial;
                    hp::take_one(&mut a.slots, i);
                    self.cont_put(&a);
                    moved = true;
                    break;
                }
            } else if !self
                .block(above)
                .is_some_and(|b| (0..6).all(|d| pl.sturdy(b, d)))
            {
                moved |= self.hopper_suck(p, &mut me);
            }
        }
        if moved {
            self.cont_put(&me);
        }
        Some(moved)
    }

    /// Воронка втягивает предметы с земли над собой (`addItem(container, ItemEntity)`).
    fn hopper_suck(&self, p: [i32; 3], me: &mut Cont) -> bool {
        // Область всасывания — чуть больше блока, то есть 1–2 чанка.
        let center = [p[0] as f64 + 0.5, p[1] as f64, p[2] as f64 + 0.5];
        let found: Vec<(i32, Stack)> = self
            .drops_near(center, 1)
            .into_iter()
            .filter(|(_, d)| crate::hoppers::in_suck_area(p, d.pos))
            .map(|(id, d)| (id, d.stack))
            .collect();
        let mut any = false;
        for (id, st) in found {
            if self
                .sync_event(Event::HopperPickup, || {
                    format!(
                        "{} {} {} {}",
                        p[0],
                        p[1],
                        p[2],
                        crate::stack_nbt::to_line(st, self.item_name(st.item))
                    )
                })
                .is_some_and(|r| r.trim() == "c")
            {
                continue;
            }
            let left = crate::hoppers::insert_all(&mut me.slots, st, |i| self.max_stack(i));
            if left == st.count {
                continue;
            }
            any = true;
            let mut dropped = self.dropped.lock().unwrap();
            if left == 0 {
                dropped.remove(&id);
                drop(dropped);
                self.hub
                    .send(None, play::remove_entities(self.ids.remove_entities, &[id]));
            } else if let Some(d) = dropped.get_mut(&id) {
                d.stack.count = left;
                let stack = d.stack;
                drop(dropped);
                self.hub.send(
                    None,
                    inv::set_entity_item(self.ids.set_entity_data, id, Some(stack)),
                );
            }
        }
        any
    }

    /// Выбрасыватель и раздатчик (`DispenserBlock.dispenseFrom`): случайный
    /// непустой слот; выбрасыватель кладёт в контейнер перед собой, иначе
    /// предмет вылетает с дымом и звуком.
    fn dispense(&self, pos: [i32; 3]) {
        use crate::hoppers as hp;
        let Some(s) = self.block(pos) else { return };
        let pl = &self.survival.placement;
        let Some(mut me) = self.cont_get(pos) else {
            return;
        };
        let full: Vec<usize> = (0..me.slots.len())
            .filter(|&i| me.slots[i].is_some())
            .collect();
        if full.is_empty() {
            self.hub.send(
                None,
                play::level_event(self.ids.level_event, hp::EVENT_FAIL, pos, 0, false),
            );
            return;
        }
        let i = full[rand::Rng::gen_range(&mut rand::thread_rng(), 0..full.len())];
        let Some(st) = me.slots[i] else { return };
        let one = Stack { count: 1, ..st };
        let f = pl
            .prop(s, "facing")
            .and_then(|v| crate::redstone::DIRS.iter().position(|d| *d == v))
            .unwrap_or(2);
        let front = crate::redstone::step(pos, f);
        if me.name == "minecraft:dropper"
            && let Some(mut t) = self.cont_get(front)
        {
            let allowed = hp::insert_slots(t.furnace, t.slots.len(), f ^ 1, self.is_fuel(st.item));
            let mut trial = t.slots.clone();
            if hp::insert_one(&mut trial, &allowed, one, |it| self.max_stack(it)) {
                if self.move_event(&me, &t, true, one).is_some() {
                    t.slots = trial;
                    hp::take_one(&mut me.slots, i);
                    self.cont_put(&t);
                    self.cont_put(&me);
                }
            } else {
                self.hub.send(
                    None,
                    play::level_event(self.ids.level_event, hp::EVENT_FAIL, pos, 0, false),
                );
            }
            return;
        }
        let d = crate::placement::step(f);
        let r: [f64; 4] = rand::random();
        let (at, vel) = hp::dispense_motion(pos, d, r);
        if self
            .sync_event(Event::Dispense, || {
                format!(
                    "{} {} {} {} {} {} {}",
                    pos[0],
                    pos[1],
                    pos[2],
                    vel[0],
                    vel[1],
                    vel[2],
                    crate::stack_nbt::to_line(one, self.item_name(one.item))
                )
            })
            .is_some_and(|r| r.trim() == "c")
        {
            return;
        }
        // Особые действия раздатчика (`DispenseItemBehavior`): вёдра, огниво.
        if me.name == "minecraft:dispenser" {
            match self.dispense_special(&mut me, i, st, front, f) {
                Some(true) => {
                    self.cont_put(&me);
                    // Снаряды звучат сами (`EVENT_LAUNCH`).
                    let projectile = matches!(
                        self.item_name(st.item),
                        "minecraft:arrow" | "minecraft:snowball" | "minecraft:egg"
                    );
                    if !projectile {
                        self.hub.send(
                            None,
                            play::level_event(
                                self.ids.level_event,
                                hp::EVENT_DISPENSE,
                                pos,
                                0,
                                false,
                            ),
                        );
                    }
                    return;
                }
                Some(false) => {
                    self.hub.send(
                        None,
                        play::level_event(self.ids.level_event, hp::EVENT_FAIL, pos, 0, false),
                    );
                    return;
                }
                None => {}
            }
        }
        hp::take_one(&mut me.slots, i);
        self.cont_put(&me);
        self.drop_item(at, vel, one, std::time::Duration::ZERO);
        self.hub.send(
            None,
            play::level_event(self.ids.level_event, hp::EVENT_DISPENSE, pos, 0, false),
        );
        self.hub.send(
            None,
            play::level_event(self.ids.level_event, hp::EVENT_SMOKE, pos, f as i32, false),
        );
    }

    /// Действие раздатчика для предмета в слоте `i`: `Some(true)` — сделано,
    /// `Some(false)` — осечка, `None` — обычный выброс.
    fn dispense_special(
        &self,
        me: &mut Cont,
        i: usize,
        st: Stack,
        front: [i32; 3],
        facing: usize,
    ) -> Option<bool> {
        use crate::fluids as fl;
        let name = self.item_name(st.item).to_owned();
        let front_state = self.block(front)?;
        let front_name = self.state_name(front_state).to_owned();
        if (front_name == "minecraft:campfire" || front_name == "minecraft:soul_campfire")
            && let Some(offer) = self.campfire_offer(st.item)
        {
            if self.campfire_room(front) {
                let time = self.campfire_start_sync(front, st, &offer);
                if self.campfire_place(front, st, time) {
                    crate::hoppers::take_one(&mut me.slots, i);
                    return Some(true);
                }
            }
            // Рецепт есть, слот нет — предмет вылетает, как обычный выброс.
            return None;
        }
        let empty = matches!(
            front_name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        );
        let open = empty || self.fluid_replaceable(front);
        match name.as_str() {
            // Полное ведро: вылить, в слоте остаётся пустое (`DispenseItemBehavior` для `BucketItem`).
            "minecraft:water_bucket" | "minecraft:lava_bucket" => {
                let kind = if name == "minecraft:water_bucket" {
                    fl::Kind::Water
                } else {
                    fl::Kind::Lava
                };
                let here = fl::state_fluid(&self.survival.placement, front_state);
                let soak = kind == fl::Kind::Water && self.waterloggable(front);
                if !open && here.is_none() && !soak {
                    return None;
                }
                if here.is_some_and(|f| f.source && f.kind == kind) {
                    return None;
                }
                self.pour(front, kind);
                let sound = if kind == fl::Kind::Water {
                    "minecraft:item.bucket.empty"
                } else {
                    "minecraft:item.bucket.empty_lava"
                };
                self.block_sound(None, sound, front, 1.0, 1.0);
                me.slots[i] = Some(Stack {
                    item: self.item_id("minecraft:bucket")?,
                    count: 1,
                    damage: 0,
                    extra: 0,
                });
                Some(true)
            }
            // Пустое ведро: зачерпнуть источник перед собой.
            "minecraft:bucket" => {
                let f =
                    fl::state_fluid(&self.survival.placement, front_state).filter(|f| f.source)?;
                let (filled, sound) = match f.kind {
                    fl::Kind::Water => ("minecraft:water_bucket", "minecraft:item.bucket.fill"),
                    fl::Kind::Lava => ("minecraft:lava_bucket", "minecraft:item.bucket.fill_lava"),
                };
                let item = self.item_id(filled)?;
                let full = Stack {
                    item,
                    count: 1,
                    damage: 0,
                    extra: 0,
                };
                if st.count == 1 {
                    me.slots[i] = Some(full);
                } else {
                    me.slots[i] = Some(Stack {
                        count: st.count - 1,
                        ..st
                    });
                    if crate::hoppers::insert_all(&mut me.slots, full, |_| 1) > 0 {
                        let c = front.map(|v| v as f64 + 0.5);
                        self.drop_item(c, [0.0, 0.1, 0.0], full, std::time::Duration::ZERO);
                    }
                }
                self.drain(front);
                self.block_sound(None, sound, front, 1.0, 1.0);
                Some(true)
            }
            // Стрела (`ProjectileDispenseBehavior`): из центра грани, сила 1.1, разброс 6.
            "minecraft:arrow" | "minecraft:snowball" | "minecraft:egg" => {
                let d = crate::placement::step(facing);
                let at = [
                    front[0] as f64 + 0.5 - 0.3 * d[0] as f64,
                    front[1] as f64 + 0.5 - 0.3 * d[1] as f64,
                    front[2] as f64 + 0.5 - 0.3 * d[2] as f64,
                ];
                let dir = [d[0] as f64, d[1] as f64 + 0.1, d[2] as f64];
                let vel = crate::arrows::shoot(
                    dir,
                    crate::arrows::DISPENSE_POWER,
                    crate::arrows::DISPENSE_SPREAD,
                    rand::random(),
                );
                let (ty, thrown) = match name.as_str() {
                    "minecraft:snowball" => (self.survival.snowball_entity_type, true),
                    "minecraft:egg" => (self.survival.egg_entity_type, true),
                    _ => (self.survival.arrow_entity_type, false),
                };
                self.shoot_arrow(at, vel, ty, thrown);
                // `ProjectileDispenseBehavior.playSound`: свой звук запуска вместо щелчка.
                let disp = crate::redstone::step(front, facing ^ 1);
                self.hub.send(
                    None,
                    play::level_event(
                        self.ids.level_event,
                        crate::arrows::EVENT_LAUNCH,
                        disp,
                        0,
                        false,
                    ),
                );
                self.hub.send(
                    None,
                    play::level_event(
                        self.ids.level_event,
                        crate::hoppers::EVENT_SMOKE,
                        disp,
                        facing as i32,
                        false,
                    ),
                );
                crate::hoppers::take_one(&mut me.slots, i);
                Some(true)
            }
            // Броня (`EquipmentDispenseItemBehavior`): на игрока перед раздатчиком в пустой слот.
            n if crate::arrows::armor_slot(n).is_some() => {
                let slot = crate::arrows::armor_slot(n)?;
                let target = {
                    let mut live = self.live.lock().unwrap();
                    let pos: Vec<(Uuid, [f64; 3])> = self
                        .hub
                        .players
                        .lock()
                        .unwrap()
                        .values()
                        .filter(|p| p.collects)
                        .map(|p| (p.entry.uuid, p.pos))
                        .collect();
                    let found = pos.into_iter().find(|(u, f)| {
                        crate::arrows::in_cell(*f, front)
                            && live.get(u).is_some_and(|l| l.slots[slot].is_none())
                    });
                    if let Some((u, _)) = found
                        && let Some(l) = live.get_mut(&u)
                    {
                        l.slots[slot] = Some(Stack { count: 1, ..st });
                    }
                    found.map(|f| f.0)
                };
                let uuid = target?;
                self.push_control(uuid, Control::Slot(slot, Some(Stack { count: 1, ..st })));
                crate::hoppers::take_one(&mut me.slots, i);
                Some(true)
            }
            // Огниво: огонь в пустоте перед раздатчиком, −1 прочности.
            "minecraft:flint_and_steel" => {
                if !empty {
                    return Some(false);
                }
                let fire = *self.survival.block_defaults.get("minecraft:fire")?;
                self.set_block(front, fire);
                self.block_sound(None, "minecraft:item.flintandsteel.use", front, 1.0, 1.0);
                let max = self
                    .survival
                    .items
                    .get(st.item as usize)
                    .map_or(64, |i| i.max_damage);
                me.slots[i] = (st.damage + 1 < max).then_some(Stack {
                    damage: st.damage + 1,
                    ..st
                });
                Some(true)
            }
            // Ножницы стригут овцу перед раздатчиком (−1 прочности); нет овцы — осечка.
            "minecraft:shears" => {
                if !self.dispense_shear(front) {
                    return Some(false);
                }
                let mut s = st;
                me.slots[i] = (!self.wear_one(&mut s)).then_some(s);
                Some(true)
            }
            // Яйцо призыва: моб перед раздатчиком, лицом по направлению выброса.
            n if n.ends_with("_spawn_egg") => {
                let yaw = [0.0, 0.0, 180.0, 0.0, 90.0, 270.0][facing.min(5)];
                if !self.dispense_egg(n, front, yaw) {
                    return None;
                }
                crate::hoppers::take_one(&mut me.slots, i);
                Some(true)
            }
            _ => None,
        }
    }

    /// Жидкости в `p` и вокруг: лава у воды застывает, остальные — тик через задержку.
    fn wake_fluids(&self, p: [i32; 3]) {
        use crate::fluids as fl;
        for d in 0..7 {
            let n = if d == 6 {
                p
            } else {
                crate::redstone::step(p, d)
            };
            let Some(s) = self.block(n) else { continue };
            let pl = &self.survival.placement;
            let Some(f) = fl::state_fluid(pl, s) else {
                continue;
            };
            if f.kind == fl::Kind::Lava {
                let at = |q: [i32; 3]| self.block(q).unwrap_or(AIR);
                let c = fl::Ctx { pl, at: &at };
                if let Some(b) = c.lava_meets(n) {
                    if let Some(&id) = self.survival.block_defaults.get(b) {
                        self.set_block(n, id);
                        self.fizz(n);
                    }
                    continue;
                }
            }
            let at = self.server_ticks.load(std::sync::atomic::Ordering::Relaxed) + f.kind.delay();
            self.fluid_ticks.lock().unwrap().schedule(n, at, false);
            self.mark_work();
        }
    }

    /// Ведро в руке (`BucketItem.use`): пустое черпает источник под взглядом,
    /// полное выливает перед блоком. `false` — в руке не ведро или некуда.
    async fn use_bucket(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        inv: &mut Inventory,
        hand: i32,
        survival: bool,
    ) -> Result<bool, NetError> {
        use crate::fluids as fl;
        let slot = if hand == 0 {
            inv.held_slot()
        } else {
            survival::OFFHAND
        };
        let Some(held) = inv.slots[slot] else {
            return Ok(false);
        };
        let bucket = self.item_name(held.item).to_owned();
        let pour = match bucket.as_str() {
            "minecraft:bucket" => None,
            "minecraft:water_bucket" => Some(fl::Kind::Water),
            "minecraft:lava_bucket" => Some(fl::Kind::Lava),
            _ => return Ok(false),
        };
        let Some((feet, yaw, pitch)) = self.player_at(uuid) else {
            return Ok(false);
        };
        let (y, p) = (yaw.to_radians() as f64, pitch.to_radians() as f64);
        let dir = [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()];
        let eye = [feet[0], feet[1] + EYE_HEIGHT, feet[2]];
        let pl = &self.survival.placement;
        let fluid = |q: [i32; 3]| fl::state_fluid(pl, self.block(q).unwrap_or(AIR));
        let solid = |q: [i32; 3]| {
            let n = self.state_name(self.block(q).unwrap_or(AIR));
            !matches!(
                n,
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | ""
            ) && fl::fluid_of(n, None).is_none()
        };
        // Пустое ведро видит источники, полное — только блоки.
        let hit = fl::trace(eye, dir, 4.5, |q| {
            solid(q) || (pour.is_none() && fluid(q).is_some_and(|f| f.source))
        });
        let Some((at, face)) = hit else {
            return Ok(false);
        };
        let hand_name = if hand == 0 { "HAND" } else { "OFF_HAND" };
        let face_name = FACES.get(face).copied().unwrap_or("UP");
        let (target, result, sound) = match pour {
            None => {
                let Some(f) = fluid(at).filter(|f| f.source) else {
                    return Ok(false);
                };
                let (filled, sound) = match f.kind {
                    fl::Kind::Water => ("minecraft:water_bucket", "minecraft:item.bucket.fill"),
                    fl::Kind::Lava => ("minecraft:lava_bucket", "minecraft:item.bucket.fill_lava"),
                };
                (at, filled, sound)
            }
            Some(kind) => {
                // Заводняемый блок (ступени, плиты, забор…) — вода внутрь;
                // заменяемый (трава) — льём в него, иначе перед ним.
                let target = if kind == fl::Kind::Water && self.waterloggable(at) {
                    at
                } else if solid(at) && !self.fluid_replaceable(at) {
                    crate::redstone::step(at, face)
                } else {
                    at
                };
                if solid(target)
                    && !self.fluid_replaceable(target)
                    && !(kind == fl::Kind::Water && self.waterloggable(target))
                {
                    return Ok(false);
                }
                let sound = if kind == fl::Kind::Water {
                    "minecraft:item.bucket.empty"
                } else {
                    "minecraft:item.bucket.empty_lava"
                };
                (target, "minecraft:bucket", sound)
            }
        };
        let action = if pour.is_some() { "EMPTY" } else { "FILL" };
        if self
            .cancelled(Event::Bucket, uuid, || {
                format!(
                    "{action} {} {} {} {face_name} {bucket} {hand_name}",
                    target[0], target[1], target[2]
                )
            })
            .await
        {
            // Вернуть клиенту то, что он уже предсказал.
            let s = self.block(target).unwrap_or(AIR);
            conn.send(&play::block_update(self.ids.block_update, target, s as i32))
                .await?;
            self.send_slots(conn, inv, &[slot]).await?;
            return Ok(true);
        }
        match pour {
            None => self.drain(target),
            Some(kind) => self.pour(target, kind),
        }
        self.block_sound(None, sound, target, 1.0, 1.0);
        if survival {
            let Some(item) = self.item_id(result) else {
                return Ok(true);
            };
            if held.count == 1 {
                inv.slots[slot] = Some(Stack {
                    item,
                    count: 1,
                    damage: 0,
                    extra: 0,
                });
                self.send_slots(conn, inv, &[slot]).await?;
            } else {
                inv.slots[slot] = Some(Stack {
                    count: held.count - 1,
                    ..held
                });
                let (mut changed, left) = inv.add(item, 1, 16);
                changed.push(slot);
                self.send_slots(conn, inv, &changed).await?;
                if left > 0 {
                    let one = Stack {
                        item,
                        count: 1,
                        damage: 0,
                        extra: 0,
                    };
                    self.drop_item(
                        eye,
                        [dir[0] * 0.3, 0.1, dir[2] * 0.3],
                        one,
                        items::THROW_DELAY,
                    );
                }
            }
        } else if pour.is_none()
            && !inv
                .slots
                .iter()
                .any(|s| s.is_some_and(|s| self.item_name(s.item) == result))
        {
            // Креатив: полное ведро добавляется, пустое остаётся.
            if let Some(item) = self.item_id(result) {
                let (changed, _) = inv.add(item, 1, 1);
                self.send_slots(conn, inv, &changed).await?;
            }
        }
        Ok(true)
    }

    /// Блок со свойством `waterlogged=false`: вода из ведра заходит внутрь.
    fn waterloggable(&self, p: [i32; 3]) -> bool {
        self.block(p)
            .is_some_and(|s| self.survival.placement.prop(s, "waterlogged") == Some("false"))
    }

    /// Вылить источник в `p`: заводнить блок, смыть растение или занять пустоту.
    fn pour(&self, p: [i32; 3], kind: crate::fluids::Kind) {
        let Some(s) = self.block(p) else { return };
        let pl = &self.survival.placement;
        if kind == crate::fluids::Kind::Water
            && let Some(w) = pl
                .prop(s, "waterlogged")
                .filter(|v| *v == "false")
                .and_then(|_| pl.with_prop(s, "waterlogged", "true"))
        {
            self.set_block(p, w);
            return;
        }
        let n = self.state_name(s);
        let empty = matches!(
            n,
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        );
        if !empty && crate::fluids::fluid_of(n, None).is_none() {
            self.pop_block(p, s);
        }
        self.set_block(
            p,
            self.fluid_state(Some(crate::fluids::Fluid::source(kind))),
        );
    }

    /// Зачерпнуть источник в `p`: снять заводнение или оставить воздух.
    fn drain(&self, p: [i32; 3]) {
        let Some(s) = self.block(p) else { return };
        let pl = &self.survival.placement;
        if pl.prop(s, "waterlogged") == Some("true") {
            if let Some(d) = pl.with_prop(s, "waterlogged", "false") {
                self.set_block(p, d);
            }
            return;
        }
        self.set_block(p, AIR);
    }

    /// Блок, который жидкость смывает (`canBeReplaced` для жидкостей):
    /// растения, факелы, снег слоем, редстоун-пыль и подобное без коллизии.
    fn fluid_replaceable(&self, p: [i32; 3]) -> bool {
        let Some(s) = self.block(p) else { return false };
        self.fluid_replaceable_of(s)
    }

    /// То же по состоянию блока: свет и стрелы уже держат состояние в руках.
    fn fluid_replaceable_of(&self, s: u32) -> bool {
        chunkwake_world::has_flag(s, chunkwake_world::flags::REPLACEABLE)
    }

    /// Имя пути состояния без пространства имён: `torch` у `minecraft:torch`.
    #[allow(dead_code)]
    fn state_path(&self, s: u32) -> &str {
        let n = self.state_name(s);
        n.strip_prefix("minecraft:").unwrap_or(n)
    }

    /// `minecraft:piston[extended=false,facing=north]` (`BlockData.getAsString`).
    fn state_string(&self, s: u32) -> String {
        let pl = &self.survival.placement;
        let props = pl.props(s);
        let name = self.state_name(s);
        if props.is_empty() {
            return name.to_owned();
        }
        let list: Vec<String> = props.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{name}[{}]", list.join(","))
    }

    /// Обратное к `state_string`: имя (можно без `minecraft:`) и свойства.
    fn parse_state(&self, spec: &str) -> Option<u32> {
        let (name, props) = match spec.split_once('[') {
            Some((n, p)) => (n, p.trim_end_matches(']')),
            None => (spec, ""),
        };
        let name = if name.contains(':') {
            name.to_owned()
        } else {
            format!("minecraft:{name}")
        };
        let mut s = *self.survival.block_defaults.get(&name)?;
        for kv in props.split(',').filter(|kv| !kv.is_empty()) {
            let (k, v) = kv.split_once('=')?;
            s = self.survival.placement.with_prop(s, k.trim(), v.trim())?;
        }
        Some(s)
    }

    /// Шипение лавы (`LevelEvent.LAVA_FIZZ`).
    fn fizz(&self, p: [i32; 3]) {
        self.hub.send(
            None,
            play::level_event(self.ids.level_event, crate::fluids::FIZZ, p, 0, false),
        );
    }

    /// Состояние блока жидкости (`None` — воздух).
    fn fluid_state(&self, f: Option<crate::fluids::Fluid>) -> u32 {
        let Some(f) = f else { return AIR };
        let Some(&d) = self.survival.block_defaults.get(f.kind.block()) else {
            return AIR;
        };
        self.survival
            .placement
            .with_prop(d, "level", &f.level().to_string())
            .unwrap_or(d)
    }

    /// Наступившие тики жидкостей: растекание, высыхание, смыв, застывание.
    fn tick_fluids(&self, now: u64) {
        use crate::fluids as fl;
        let due: Vec<[i32; 3]> = {
            let mut list = self.fluid_ticks.lock().unwrap();
            if list.is_empty() {
                return;
            }
            // Вне симуляции тик откладывается, а не теряется.
            let simulated = self.simulated_set();
            list.take_due_in(now, |p| {
                simulated.is_empty() || simulated.contains(&(p[0] >> 4, p[2] >> 4))
            })
        };
        for p in due {
            let changes = {
                let at = |q: [i32; 3]| self.block(q).unwrap_or(AIR);
                fl::Ctx {
                    pl: &self.survival.placement,
                    at: &at,
                }
                .tick(p)
            };
            for c in changes {
                match c {
                    fl::Change::Set { pos, fluid, wash } => {
                        if wash && let Some(s) = self.block(pos) {
                            // Лава сжигает без дропа, вода смывает с дропом.
                            if fluid.is_some_and(|f| f.kind == fl::Kind::Lava) {
                                self.fizz(pos);
                            } else {
                                self.pop_block(pos, s);
                            }
                        }
                        self.set_block(pos, self.fluid_state(fluid));
                    }
                    fl::Change::Solidify { pos, block } => {
                        if let Some(&id) = self.survival.block_defaults.get(block) {
                            self.set_block(pos, id);
                            self.fizz(pos);
                        }
                    }
                }
            }
        }
    }

    /// Компонент без опоры выпадает предметом (`canSurvive` → `destroyBlock`).
    fn pop_block(&self, p: [i32; 3], s: u32) {
        self.set_block(p, AIR);
        self.hub.send(
            None,
            play::level_event(
                self.ids.level_event,
                crate::sounds::DESTROY_BLOCK,
                p,
                s as i32,
                false,
            ),
        );
        let center = [p[0] as f64 + 0.5, p[1] as f64 + 0.25, p[2] as f64 + 0.5];
        for st in self.block_loot(s, None) {
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(center, vel, st, items::BLOCK_DELAY);
        }
    }

    /// Рычаг или кнопка под рукой: переключить, звук, питание соседей.
    async fn use_switch(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        pos: [i32; 3],
    ) -> Result<bool, NetError> {
        let Some(state) = self.block(pos) else {
            return Ok(false);
        };
        let name = self.state_name(state).to_owned();
        let pl = &self.survival.placement;
        // Нотный блок: клик меняет ноту по кругу и играет её
        // (`NoteBlock.useWithoutItem`: `cycle(NOTE)`).
        if crate::redstone::is_note_block(&name) {
            if let Some(next) = self.note_next(pos) {
                self.set_block(pos, next);
                conn.send(&play::block_update(self.ids.block_update, pos, next as i32))
                    .await?;
            }
            self.play_note(pos);
            return Ok(true);
        }
        // Повторитель: задержка 1→4 по кругу (`RepeaterBlock.useWithoutItem`).
        if crate::redstone::is_repeater(&name) {
            let d = pl
                .prop(state, "delay")
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(1)
                % 4
                + 1;
            if let Some(n) = pl.with_prop(state, "delay", &d.to_string()) {
                self.set_block(pos, n);
                conn.send(&play::block_update(self.ids.block_update, pos, n as i32))
                    .await?;
            }
            return Ok(true);
        }
        // Компаратор: сравнение ↔ вычитание (`ComparatorBlock.useWithoutItem`).
        if crate::redstone::is_comparator(&name) {
            let sub = pl.prop(state, "mode") == Some("subtract");
            if let Some(n) = pl.with_prop(state, "mode", if sub { "compare" } else { "subtract" }) {
                self.set_block(pos, n);
                conn.send(&play::block_update(self.ids.block_update, pos, n as i32))
                    .await?;
                self.block_sound(
                    None,
                    "minecraft:block.comparator.click",
                    pos,
                    0.3,
                    if sub { 0.5 } else { 0.55 },
                );
                self.schedule(pos, 2);
            }
            return Ok(true);
        }
        // Провод без соединений: крест ↔ точка (`RedStoneWireBlock.useWithoutItem`).
        if crate::redstone::is_wire(&name) {
            let sides =
                ["north", "south", "west", "east"].map(|k| pl.prop(state, k).unwrap_or("none"));
            let cross = sides.iter().all(|v| *v == "side");
            let dot = sides.iter().all(|v| *v == "none");
            if !cross && !dot {
                return Ok(false);
            }
            let mut n = state;
            for k in ["north", "south", "west", "east"] {
                if let Some(m) = pl.with_prop(n, k, if cross { "none" } else { "side" }) {
                    n = m;
                }
            }
            self.set_block(pos, n);
            self.redstone_update(&crate::redstone::around(pos));
            let now = self.block(pos).unwrap_or(n);
            conn.send(&play::block_update(self.ids.block_update, pos, now as i32))
                .await?;
            return Ok(now != state);
        }
        let lever = crate::redstone::is_lever(&name);
        if !lever && !crate::redstone::is_button(&name) {
            return Ok(false);
        }
        let on = self.survival.placement.prop(state, "powered") != Some("true");
        // Нажатая кнопка не отжимается рукой.
        if !lever && !on {
            return Ok(true);
        }
        // `BlockRedstoneEvent`: плагин может не дать переключить.
        let (old, new) = if on { (0, 15) } else { (15, 0) };
        if (self.redstone_event(pos, old, new) > 0) != on {
            return Ok(true);
        }
        let Some(next) =
            self.survival
                .placement
                .with_prop(state, "powered", if on { "true" } else { "false" })
        else {
            return Ok(false);
        };
        self.set_block(pos, next);
        conn.send(&play::block_update(self.ids.block_update, pos, next as i32))
            .await?;
        let (sound, v, p) = crate::redstone::click_sound(&name, on);
        // Кнопку клиент озвучивает сам, рычаг — нет.
        self.block_sound(if lever { None } else { Some(uuid) }, &sound, pos, v, p);
        if !lever {
            self.schedule(pos, crate::redstone::button_ticks(&name));
        }
        self.update_power(next, pos);
        Ok(true)
    }

    /// Есть ли сигнал у блока `pos` (`hasNeighborSignal`).
    fn powered_at(&self, pos: [i32; 3]) -> bool {
        let at = |p: [i32; 3]| self.block(p).unwrap_or(AIR);
        crate::redstone::World {
            pl: &self.survival.placement,
            at: &at,
            output: &|p| self.comparator_output(p),
            analog: &|p| self.analog_at(p),
        }
        .powered(pos)
    }

    /// После переключения рычага или кнопки в `pos`: соседи его и блока, к
    /// которому он прикреплён (сильный сигнал).
    fn update_power(&self, source: u32, pos: [i32; 3]) {
        let pl = &self.survival.placement;
        let face = pl.prop(source, "face").unwrap_or("wall");
        let facing = pl.prop(source, "facing").unwrap_or("north");
        let base = crate::redstone::attached(pos, face, facing);
        let mut all: Vec<[i32; 3]> = crate::redstone::around(pos).to_vec();
        all.extend_from_slice(&crate::redstone::around(base));
        all.sort();
        all.dedup();
        self.redstone_update(&all);
    }

    /// `neighborChanged` двери, люка или калитки в `c`: открыть или закрыть по
    /// сигналу. `sound` — со звуком (при установке ванила молчит). Возвращает
    /// изменённые позиции.
    fn refresh_openable(&self, c: [i32; 3], sound: bool) -> Vec<[i32; 3]> {
        let pl = &self.survival.placement;
        let mut out = Vec::new();
        let Some(state) = self.block(c) else {
            return out;
        };
        let name = self.state_name(state).to_owned();
        if !crate::redstone::openable(&name) {
            return out;
        }
        // Дверь: сигнал любой из половин.
        let other = pl
            .partner(state)
            .filter(|_| name.ends_with("_door"))
            .map(|o| crate::redstone::add(c, o))
            .filter(|&p| self.block(p).is_some_and(|s| pl.name(s) == pl.name(state)));
        let mut power = self.powered_at(c) || other.is_some_and(|p| self.powered_at(p));
        if pl.prop(state, "powered") == Some(if power { "true" } else { "false" }) {
            return out;
        }
        let old = if power { 0 } else { 15 };
        power = self.redstone_event(c, old, 15 - old) > 0;
        let value = if power { "true" } else { "false" };
        let was_open = pl.prop(state, "open") == Some("true");
        for p in std::iter::once(c).chain(other) {
            if let Some(s) = self.block(p)
                && let Some(n) = pl
                    .with_prop(s, "powered", value)
                    .and_then(|n| pl.with_prop(n, "open", value))
            {
                self.set_block(p, n);
                out.push(p);
            }
        }
        if sound
            && was_open != power
            && let Some(sound) = crate::sounds::door_sound(&name, power)
        {
            self.block_sound(None, &sound, c, 1.0, crate::sounds::pitch_09());
        }
        out
    }

    /// Рецепт костра для предмета.
    fn campfire_offer(&self, item: i32) -> Option<CampOffer> {
        let c = self.survival.recipes.campfire(item)?;
        Some(CampOffer {
            id: c.id.clone(),
            experience: c.experience,
            time: c.time,
            result_item: c.result.0,
            result_count: c.result.1,
        })
    }

    fn campfire_block(&self, pos: [i32; 3]) -> bool {
        matches!(
            self.state_name(self.block(pos).unwrap_or(0)),
            "minecraft:campfire" | "minecraft:soul_campfire"
        )
    }

    fn campfire_room(&self, pos: [i32; 3]) -> bool {
        self.with_campfire(pos, |c| c.has_room()).unwrap_or(false)
    }

    /// Костёр из памяти или из блок-сущности. Нет блока — `None`.
    fn with_campfire<R>(
        &self,
        pos: [i32; 3],
        f: impl FnOnce(&mut crate::campfire::Campfire) -> R,
    ) -> Option<R> {
        if !self.campfire_block(pos) {
            return None;
        }
        if !self.campfires.lock().unwrap().contains_key(&pos) {
            self.campfires
                .lock()
                .unwrap()
                .insert(pos, self.load_campfire(pos));
        }
        self.campfires.lock().unwrap().get_mut(&pos).map(f)
    }

    fn load_campfire(&self, pos: [i32; 3]) -> crate::campfire::Campfire {
        let mut c = crate::campfire::Campfire::new();
        let Some(be) = self.block_entity(pos) else {
            return c;
        };
        let items = self.container_items(pos);
        for i in 0..crate::campfire::SLOTS {
            c.slots[i] = items.get(i).copied().flatten();
        }
        let copy = |src: Option<&[i32]>, dst: &mut [i32]| {
            for (i, &v) in src.unwrap_or(&[]).iter().take(dst.len()).enumerate() {
                dst[i] = v;
            }
        };
        copy(
            be.nbt.get("CookingTimes").and_then(Tag::as_int_array),
            &mut c.progress,
        );
        copy(
            be.nbt.get("CookingTotalTimes").and_then(Tag::as_int_array),
            &mut c.total,
        );
        if let Some(bytes) = be.nbt.get("Paper.StopCooking").and_then(Tag::as_byte_array) {
            for (i, &b) in bytes.iter().take(c.stopped.len()).enumerate() {
                c.stopped[i] = b != 0;
            }
        }
        c
    }

    fn save_campfire(&self, pos: [i32; 3], c: &crate::campfire::Campfire, sync: bool) {
        let mut nbt = Compound::new();
        nbt.insert("Items".into(), Tag::List(self.items_nbt(&c.slots)));
        nbt.insert("CookingTimes".into(), Tag::IntArray(c.progress.to_vec()));
        nbt.insert("CookingTotalTimes".into(), Tag::IntArray(c.total.to_vec()));
        nbt.insert(
            "Paper.StopCooking".into(),
            Tag::ByteArray(c.stopped.iter().map(|&s| i8::from(s)).collect()),
        );
        self.set_block_entity(
            pos,
            Some(BlockEntity {
                id: "minecraft:campfire".into(),
                nbt: nbt.clone(),
            }),
        );
        if sync {
            self.send_campfire(pos, &nbt);
        }
    }

    fn send_campfire(&self, pos: [i32; 3], nbt: &Compound) {
        let Some(types) = chunkwake_world::BLOCK_ENTITY_TYPES.get() else {
            return;
        };
        let Some(&kind) = types.get("minecraft:campfire") else {
            return;
        };
        let bytes = chunkwake_world::nbt::write_network(nbt);
        self.hub.send(
            None,
            play::block_entity_data(self.ids.block_entity_data, pos, kind, &bytes),
        );
    }

    /// Слоты контейнера в позиции (`/loot insert|replace`): сундук, бочка, воронка.
    pub(crate) fn container_slots(&self, pos: [i32; 3]) -> Option<Vec<Slot>> {
        let name = self.block(pos).map(|s| self.state_name(s))?;
        // Печь и костёр хранят слоты в своих структурах, а не в `Items`.
        if furnace_kind(name).is_some() || name.ends_with("campfire") {
            return None;
        }
        if !name.ends_with("chest")
            && !name.ends_with("barrel")
            && !name.ends_with("hopper")
            && !name.ends_with("dispenser")
            && !name.ends_with("dropper")
            && !name.ends_with("shulker_box")
        {
            return None;
        }
        Some(self.container_items(pos))
    }

    /// Записывает слоты контейнера и рассылает `block_entity_data`.
    ///
    /// `None` — в позиции не контейнер. Возвращает `false`, если чанк не
    /// материализован, а запись некуда положить.
    pub(crate) fn set_container_slots(&self, pos: [i32; 3], slots: &[Slot]) -> Option<bool> {
        // Свежий сундук ещё не имеет блок-сущности: её создаёт первая запись.
        let (id, mut nbt) = match self.block_entity(pos) {
            Some(old) => (old.id, old.nbt),
            None => {
                let name = self.block(pos).map(|s| self.state_name(s))?;
                (container_entity(name)?.to_owned(), Compound::new())
            }
        };
        nbt.insert("Items".into(), Tag::List(self.items_nbt(slots)));
        self.set_block_entity(
            pos,
            Some(chunkwake_world::BlockEntity {
                id: id.clone(),
                nbt: nbt.clone(),
            }),
        );
        if let Some(types) = chunkwake_world::BLOCK_ENTITY_TYPES.get()
            && let Some(&kind) = types.get(&id)
        {
            let bytes = chunkwake_world::nbt::write_network(&nbt);
            self.hub.send(
                None,
                play::block_entity_data(self.ids.block_entity_data, pos, kind, &bytes),
            );
        }
        Some(true)
    }

    /// Добавляет предмет в контейнер: свободный слот или докладывает к стопке.
    ///
    /// Возвращает `true`, если предмет целиком поместился.
    pub(crate) fn container_add(&self, pos: [i32; 3], stack: Stack) -> bool {
        let Some(mut slots) = self.container_slots(pos) else {
            return false;
        };
        let limit = self.max_stack(stack.item).max(1);
        // Сначала к существующим стопкам, потом в пустые слоты: как `Container.addItem`.
        let mut left = stack.count;
        for slot in slots.iter_mut().flatten() {
            if left == 0 {
                break;
            }
            if slot.item == stack.item && slot.damage == stack.damage && slot.extra == stack.extra {
                let room = limit - slot.count;
                let take = room.min(left);
                slot.count += take;
                left -= take;
            }
        }
        for slot in slots.iter_mut() {
            if left == 0 {
                break;
            }
            if slot.is_none() {
                let take = limit.min(left);
                *slot = Some(Stack {
                    count: take,
                    ..stack
                });
                left -= take;
            }
        }
        if left == stack.count {
            return false;
        }
        self.set_container_slots(pos, &slots);
        left == 0
    }

    fn campfire_start_arg(&self, pos: [i32; 3], held: Stack, offer: &CampOffer) -> String {
        let result = Stack {
            item: offer.result_item,
            count: offer.result_count,
            damage: 0,
            extra: 0,
        };
        format!(
            "{} {} {}\n{} {} {}\n{}\n{}",
            pos[0],
            pos[1],
            pos[2],
            offer.id,
            offer.experience,
            offer.time,
            crate::stack_nbt::to_line(result, self.item_name(result.item)),
            crate::stack_nbt::to_line(held, self.item_name(held.item)),
        )
    }

    fn campfire_time(reply: Option<String>, fallback: i32) -> i32 {
        reply
            .and_then(|r| r.trim().parse().ok())
            .unwrap_or(fallback)
            .max(0)
    }

    fn campfire_start_sync(&self, pos: [i32; 3], held: Stack, offer: &CampOffer) -> i32 {
        let reply = self.sync_event(Event::CampfireStart, || {
            self.campfire_start_arg(pos, held, offer)
        });
        Self::campfire_time(reply, offer.time)
    }

    /// Кладёт одну штуку, если есть рецепт и свободный слот. Креатив не проверяет.
    fn campfire_place(&self, pos: [i32; 3], held: Stack, time: i32) -> bool {
        let placed = self
            .with_campfire(pos, |c| c.place(held, time))
            .unwrap_or(false);
        if placed {
            let snap = self.campfires.lock().unwrap().get(&pos).cloned();
            if let Some(c) = snap {
                self.save_campfire(pos, &c, true);
            }
        }
        placed
    }

    /// Правый клик едой. `true` — штука легла на костёр (вызывающий тратит её).
    async fn feed_campfire(&self, uuid: Uuid, pos: [i32; 3], held: Stack, _creative: bool) -> bool {
        if !self.campfire_block(pos) {
            return false;
        }
        let Some(offer) = self.campfire_offer(held.item) else {
            return false;
        };
        if !self.campfire_room(pos) {
            return false;
        }
        let reply = self
            .plugin_event(Event::CampfireStart, uuid, || {
                self.campfire_start_arg(pos, held, &offer)
            })
            .await;
        let time = Self::campfire_time(reply, offer.time);
        self.campfire_place(pos, held, time)
    }

    /// `RandomSource.triangle(0, 0.11485000171139836)` по X/Z, Y = 0.2.
    fn campfire_velocity() -> [f64; 3] {
        const SPREAD: f64 = 0.11485000171139836;
        let tri = || {
            let (a, b): (f64, f64) = (rand::random(), rand::random());
            (a - b) * SPREAD
        };
        [tri(), 0.2, tri()]
    }

    fn drop_campfire_result(&self, pos: [i32; 3], stack: Stack) {
        let center = [
            pos[0] as f64 + 0.5,
            pos[1] as f64 + 0.5,
            pos[2] as f64 + 0.5,
        ];
        let mut rng = rand::thread_rng();
        let single = stack.count <= 1;
        for n in
            crate::campfire::split_counts(stack.count, || rand::Rng::gen_range(&mut rng, 10..31))
        {
            // Одна штука — подброс сундука. Стопка режется, и у кусков
            // `triangle` как у `ItemEntity` в патче Paper.
            let vel = if single {
                items::pop_velocity(rand::random(), rand::random())
            } else {
                Self::campfire_velocity()
            };
            self.drop_item(center, vel, Stack { count: n, ..stack }, items::BLOCK_DELAY);
        }
    }

    fn cook_arg(
        &self,
        pos: [i32; 3],
        source: Stack,
        result: Stack,
        offer: Option<&CampOffer>,
    ) -> String {
        let (id, exp, time) =
            offer.map_or(("-", 0.0, 0), |o| (o.id.as_str(), o.experience, o.time));
        format!(
            "{} {} {}\n{id} {exp} {time}\n{}\n{}",
            pos[0],
            pos[1],
            pos[2],
            crate::stack_nbt::to_line(source, self.item_name(source.item)),
            crate::stack_nbt::to_line(result, self.item_name(result.item)),
        )
    }

    /// `BlockCookEvent`: отмена, пустой результат или замена стопки.
    fn cook_finish(&self, pos: [i32; 3], source: Stack, result: Stack) -> crate::campfire::Finish {
        let offer = self.campfire_offer(source.item);
        let reply = self.sync_event(Event::Cook, || {
            self.cook_arg(pos, source, result, offer.as_ref())
        });
        match reply.as_deref().map(str::trim) {
            Some("c") => crate::campfire::Finish::Cancel,
            Some("e") => crate::campfire::Finish::Drop(None),
            Some(s) => {
                let Some(rest) = s.strip_prefix("i ") else {
                    return crate::campfire::Finish::Drop(Some(result));
                };
                let stack = crate::stack_nbt::from_line(rest, |n| self.item_id(n));
                crate::campfire::Finish::Drop(stack)
            }
            None => crate::campfire::Finish::Drop(Some(result)),
        }
    }

    /// Тик костров: горящий готовит, погасший остывает.
    fn tick_campfires(&self) {
        let pending = std::mem::take(&mut *self.pending_campfires.lock().unwrap());
        for pos in pending {
            if self.campfire_block(pos) {
                self.with_campfire(pos, |_| ());
            }
        }
        let simulated = self.simulated_set();
        let positions: Vec<[i32; 3]> = self.campfires.lock().unwrap().keys().copied().collect();
        for pos in positions {
            if !simulated.is_empty() && !simulated.contains(&(pos[0] >> 4, pos[2] >> 4)) {
                continue;
            }
            self.tick_campfire(pos);
        }
    }

    fn tick_campfire(&self, pos: [i32; 3]) {
        if !self.campfire_block(pos) {
            let gone = self.campfires.lock().unwrap().remove(&pos);
            if let Some(c) = gone {
                self.save_campfire(pos, &c, false);
            }
            return;
        }
        let lit = self
            .block(pos)
            .is_some_and(|s| self.survival.placement.prop(s, "lit") == Some("true"));
        if !lit {
            let (snap, settled) = {
                let mut map = self.campfires.lock().unwrap();
                let Some(c) = map.get_mut(&pos) else {
                    return;
                };
                let before = c.progress;
                c.tick_cool();
                // Остывший слот пишем в NBT, иначе перезапуск вернёт старый прогресс.
                let settled = before
                    .iter()
                    .zip(c.progress.iter())
                    .any(|(&was, &now)| was > 0 && now == 0);
                let snap = c.clone();
                if !c.active() {
                    map.remove(&pos);
                }
                (snap, settled)
            };
            if !snap.active() || settled {
                self.save_campfire(pos, &snap, true);
            }
            return;
        }
        {
            let mut map = self.campfires.lock().unwrap();
            if let Some(c) = map.get_mut(&pos) {
                c.begin_tick();
            }
        }
        let mut changed = false;
        loop {
            let ready = {
                let mut map = self.campfires.lock().unwrap();
                let Some(c) = map.get_mut(&pos) else {
                    return;
                };
                c.next_lit(|item| {
                    self.survival
                        .recipes
                        .campfire(item)
                        .map(|r| (r.result.0, r.result.1))
                })
            };
            let crate::campfire::Action::Ready {
                slot,
                source,
                result,
            } = ready
            else {
                break;
            };
            let done = self.cook_finish(pos, source, result);
            let cancel = done == crate::campfire::Finish::Cancel;
            let mut map = self.campfires.lock().unwrap();
            let Some(c) = map.get_mut(&pos) else {
                return;
            };
            if c.slots
                .get(slot)
                .and_then(|s| *s)
                .is_some_and(|s| s.item == source.item)
            {
                if let Some(stack) = c.finish(slot, done) {
                    drop(map);
                    self.drop_campfire_result(pos, stack);
                    changed = true;
                } else if !cancel {
                    changed = true;
                }
            }
            if cancel {
                break;
            }
        }
        let snap = self.campfires.lock().unwrap().get(&pos).cloned();
        if let Some(c) = snap {
            if changed || !c.active() {
                self.save_campfire(pos, &c, true);
            }
            if !c.active() {
                self.campfires.lock().unwrap().remove(&pos);
            }
        }
    }

    /// Тик печей: горение, готовка, свойство `lit` блока, окна смотрящих.
    fn tick_furnaces(&self) {
        let pending = std::mem::take(&mut *self.pending_furnaces.lock().unwrap());
        for pos in pending {
            self.with_furnace(pos, |_| ());
        }
        let mut done = Vec::new();
        {
            let mut map = self.furnaces.lock().unwrap();
            if map.is_empty() {
                return;
            }
            let simulated = self.simulated_set();
            map.retain(|&pos, e| {
                // Печь вне симуляции стоит: без топлива время не идёт.
                if !simulated.is_empty() && !simulated.contains(&(pos[0] >> 4, pos[2] >> 4)) {
                    return true;
                }
                let was_lit = e.f.lit();
                // Погасшая печь без топлива и предметов ничего не делает:
                // `furnace_tick` строит `Env` с четырьмя замыканиями и гоняет
                // весь `Furnace::tick`. Проверка `idle` до него снимает эту
                // работу с каждой пустой печи каждый тик.
                let idle = e.f.idle();
                if idle {
                    // Смотреть в пустую печь нечего, но окно должно жить.
                    if e.viewers.is_empty() {
                        return false;
                    }
                    done.push((pos, e.f.clone(), false, false, false, e.viewers.clone()));
                    return true;
                }
                let before = e.f.slots;
                let changed = self.furnace_tick(&mut e.f);
                let items = before != e.f.slots;
                let active = !e.f.idle();
                if changed || active || !e.viewers.is_empty() {
                    done.push((
                        pos,
                        e.f.clone(),
                        was_lit != e.f.lit(),
                        changed,
                        items,
                        e.viewers.clone(),
                    ));
                }
                active || !e.viewers.is_empty()
            });
        }
        let controls = self.controls.lock().unwrap();
        for (pos, f, _, _, items, viewers) in &done {
            for v in viewers {
                if let Some(tx) = controls.get(v) {
                    let _ = tx.send(Control::Furnace(*pos, f.data(), *items));
                }
            }
        }
        drop(controls);
        for (pos, _, _, _, items, _) in &done {
            if *items {
                self.redstone_update(&crate::redstone::around(*pos));
            }
        }
        for (pos, f, flip, changed, _, _) in done {
            if flip
                && let Some(s) = self.block(pos)
                && let Some(&lit) = self.survival.furnace_lit.get(&s)
            {
                self.set_block(pos, lit);
            }
            if changed || f.idle() {
                self.save_furnace(pos, &f);
            }
        }
    }

    /// Сохраняет сундук окна и сообщает остальным, у кого он открыт.
    fn save_window(&self, w: &Window, me: Uuid) {
        for (i, &pos) in w.positions().iter().enumerate() {
            let n = w.slots.len() / w.positions().len();
            self.save_container(pos, &w.slots[i * n..(i + 1) * n]);
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
            "minecraft:water_bucket" | "minecraft:lava_bucket" | "minecraft:milk_bucket" => {
                "minecraft:bucket"
            }
            "minecraft:honey_bottle" | "minecraft:dragon_breath" => "minecraft:glass_bottle",
            _ => return None,
        };
        Some(Stack {
            item: self.item_id(rest)?,
            count: 1,
            damage: 0,
            extra: 0,
        })
    }

    /// Заборы, стены и стёкла вокруг `pos` (и он сам) пересчитывают соединения
    /// (`updateShape` соседей).
    /// Возвращает изменённые блоки.
    fn update_connections(&self, pos: [i32; 3]) -> Vec<[i32; 3]> {
        let mut changed = Vec::new();
        let pl = &self.survival.placement;
        let at = |p: [i32; 3], d: usize| {
            let o = crate::placement::step(d);
            [p[0] + o[0], p[1] + o[1], p[2] + o[2]]
        };
        for p in [
            pos,
            at(pos, 2),
            at(pos, 3),
            at(pos, 4),
            at(pos, 5),
            at(pos, 0),
        ] {
            let Some(s) = self.block(p).filter(|&s| pl.connects(s)) else {
                continue;
            };
            let side = |d| self.block(at(p, d)).unwrap_or(AIR);
            let new = pl.connect(s, [side(2), side(3), side(4), side(5)], side(1));
            if new != s {
                self.set_block(p, new);
                changed.push(p);
            }
        }
        changed
    }

    /// `TripWireBlock.updateShape`: пересобрать `north/south/west/east` по
    /// соседям. Сторона без соседа получает `false`, а не исчезает.
    fn refresh_tripwire_shape(&self, pos: [i32; 3]) {
        let pl = &self.survival.placement;
        let Some(state) = self.block(pos) else { return };
        let mut next = state;
        for dir in 2..6 {
            let side = crate::redstone::place(pos, dir, 1);
            let Some(neighbor) = self.block(side) else {
                continue;
            };
            let name = self.state_name(neighbor);
            let connects =
                crate::redstone::tripwire_connects(name, pl.prop(neighbor, "facing"), dir);
            let value = if connects { "true" } else { "false" };
            if let Some(n) = pl.with_prop(next, crate::redstone::DIRS[dir], value) {
                next = n;
            }
        }
        if next != state {
            self.set_block(pos, next);
        }
    }

    /// `TripWireBlock.checkPressed` для одной клетки нити.
    ///
    /// В ванили это `getEntities(null, shape.bounds().move(pos))`: нить
    /// задевают игроки, мобы и предметы. Сущности, игнорирующие триггеры
    /// (`isIgnoringBlockTriggers`), не считаются; в сервере так себя ведут
    /// только несуществующие здесь виды, поэтому фильтр — «живая».
    fn tripwire_pressed(&self, pos: [i32; 3], attached: bool) -> bool {
        let hits =
            |min: [f64; 3], max: [f64; 3]| crate::redstone::touches_wire(attached, pos, min, max);
        // Игрок: рамка 0.6×1.8. Живость берётся из `live`, а не из таб-листа.
        let player = {
            let live = self.live.lock().unwrap();
            self.hub.players.lock().unwrap().values().any(|p| {
                let at = p.pos;
                let alive = live.get(&p.entry.uuid).is_some_and(|l| l.health > 0.0);
                alive
                    && hits(
                        [at[0] - 0.3, at[1], at[2] - 0.3],
                        [at[0] + 0.3, at[1] + 1.8, at[2] + 0.3],
                    )
            })
        };
        if player {
            return true;
        }
        let mob = self.mobs.lock().unwrap().values().any(|e| {
            if !e.mob.alive() {
                return false;
            }
            let (w, h) = e.mob.size();
            let at = e.mob.pos;
            hits(
                [at[0] - w / 2.0, at[1], at[2] - w / 2.0],
                [at[0] + w / 2.0, at[1] + h, at[2] + w / 2.0],
            )
        });
        if mob {
            return true;
        }
        // Предмет: куб 0.25 вокруг центра.
        self.dropped.lock().unwrap().values().any(|d| {
            let at = d.pos;
            hits(
                [at[0] - 0.125, at[1], at[2] - 0.125],
                [at[0] + 0.125, at[1] + 0.25, at[2] + 0.125],
            )
        })
    }

    /// `TripWireBlock.checkPressed`: сменить `powered`, обновить крюки,
    /// запланировать перепроверку.
    fn tripwire_check(&self, pos: [i32; 3]) {
        let pl = &self.survival.placement;
        let Some(state) = self.block(pos) else { return };
        let attached = pl.prop(state, "attached") == Some("true");
        let was = pl.prop(state, "powered") == Some("true");
        let pressed = self.tripwire_pressed(pos, attached);
        if pressed != was
            && let Some(next) =
                pl.with_prop(state, "powered", if pressed { "true" } else { "false" })
        {
            self.set_block(pos, next);
            self.update_tripwire_source(pos, next);
        }
        // Пока по нити идут — проверка раз в `RECHECK_PERIOD`; отпустило —
        // через тик, чтобы сигнал снимался сразу.
        if pressed {
            self.schedule(pos, crate::redstone::RECHECK_PERIOD);
        } else if was {
            self.schedule(pos, 1);
        }
    }

    /// `TripWireBlock.updateSource`: пересчитать крюки в обе стороны от нити.
    ///
    /// Ванила смотрит только `SOUTH` и `WEST`: второй крюк найдётся, когда
    /// очередь дойдёт до него от своей позиции.
    fn update_tripwire_source(&self, pos: [i32; 3], _state: u32) {
        for dir in crate::redstone::WIRE_SCAN {
            let Some(hook) = self.tripwire_hook_toward(pos, dir) else {
                continue;
            };
            self.tripwire_hook_state(hook, false, false, -1, None);
        }
    }

    /// `NoteBlock.setInstrument`: инструмент от блока над, иначе от блока под.
    ///
    /// Инструмент считается по `BlockBehaviour.instrument()` соседа, поэтому
    /// нужен именно **состояние** соседа, а не его имя: у камня и дерева
    /// разные инструменты при одинаковом поведении.
    fn note_instrument_state(&self, state: u32) -> chunkwake_data::world::NoteInstrument {
        self.survival.instruments.of(self.state_name(state))
    }

    /// Состояние нотного блока с инструментом по соседям (`setInstrument`).
    fn note_instrument_at(&self, pos: [i32; 3], state: u32) -> u32 {
        let above = self.block([pos[0], pos[1] + 1, pos[2]]);
        let below = self.block([pos[0], pos[1] - 1, pos[2]]);
        let kind = crate::redstone::instrument_for(
            above.map(|s| self.note_instrument_state(s)),
            below.map(|s| self.note_instrument_state(s)),
        );
        self.survival
            .placement
            .with_prop(state, "instrument", kind.name())
            .unwrap_or(state)
    }

    /// `NoteBlock.playNote`: сыграть ноту, если инструмент «над блоком» или
    /// сверху воздух. Возвращает `true`, если звук отправлен.
    ///
    /// Звук — `playSeededSound(…, SoundSource.RECORDS, NOTE_VOLUME, pitch, seed)`,
    /// где pitch для настраиваемых инструментов считается из `note`, а у
    /// голов всегда 1. Сверху летит частица ноты (`ParticleTypes.NOTE`).
    fn play_note(&self, pos: [i32; 3]) -> bool {
        let pl = &self.survival.placement;
        let Some(state) = self.block(pos) else {
            return false;
        };
        if !crate::redstone::is_note_block(self.state_name(state)) {
            return false;
        }
        // Инструмент берётся из свойства **состояния** (`instrument=…`):
        // это и есть звук. Свойство блока (`instrument()` у камня) лишь
        // выбирает его при установке и на смене соседа.
        let kind = pl
            .prop(state, "instrument")
            .and_then(chunkwake_data::world::NoteInstrument::parse)
            .unwrap_or(chunkwake_data::world::NoteInstrument::Harp);
        let above = self.block([pos[0], pos[1] + 1, pos[2]]).unwrap_or(AIR);
        let above_air = chunkwake_world::has_flag(above, chunkwake_world::flags::AIR);
        if !crate::redstone::note_sounds(kind.works_above(), above_air) {
            return false;
        }
        let note: i32 = pl
            .prop(state, "note")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let pitch = if kind.tunable() {
            crate::redstone::note_pitch(note)
        } else {
            1.0
        };
        // Голова игрока: звук берётся из блок-сущности над нотным блоком.
        // Из голов в сервере есть только скелет и прочие мобы-черепа из
        // шаблонов; их `note_block_sound` не пишется, поэтому `custom_head`
        // молчит, как у ванили без звука в блок-сущности.
        let custom = self.note_custom_sound(pos);
        if kind.custom_sound() && custom.is_none() {
            return false;
        }
        let sound = custom.unwrap_or_else(|| kind.sound().to_owned());
        let center = crate::redstone::note_sound_pos(pos);
        self.play_sound(
            None,
            &sound,
            crate::sounds::RECORDS,
            center,
            crate::redstone::NOTE_VOLUME,
            pitch,
        );
        // Частица ноты — только у настраиваемых инструментов.
        if kind.tunable() {
            let (at, delta) = crate::redstone::note_particle(note, pos);
            if let Some(id) = self.particle_id("minecraft:note") {
                self.hub.send(
                    None,
                    chunkwake_protocol::play::level_particles_with(
                        self.ids.level_particles,
                        id,
                        None,
                        true,
                        at,
                        delta,
                        0.0,
                        1,
                    ),
                );
            }
        }
        true
    }

    /// `SkullBlockEntity.getNoteBlockSound()` у блока над нотным блоком.
    fn note_custom_sound(&self, pos: [i32; 3]) -> Option<String> {
        let above = [pos[0], pos[1] + 1, pos[2]];
        let be = self.block_entity(above)?;
        if be.id != "minecraft:skull" {
            return None;
        }
        let chunkwake_world::nbt::Tag::String(sound) = be.nbt.get("note_block_sound")? else {
            return None;
        };
        Some(sound.clone())
    }

    /// `NoteBlock.neighborChanged`: звук на фронте сигнала.
    fn note_signal_update(&self, pos: [i32; 3]) {
        let pl = &self.survival.placement;
        let Some(state) = self.block(pos) else { return };
        let at = |p: [i32; 3]| self.block(p).unwrap_or(AIR);
        let w = crate::redstone::World {
            pl,
            at: &at,
            output: &|p| self.comparator_output(p),
            analog: &|p| self.analog_at(p),
        };
        let signal = w.powered(pos);
        let powered = pl.prop(state, "powered") == Some("true");
        match crate::redstone::note_signal(signal, powered) {
            crate::redstone::NoteAction::Nothing => {}
            crate::redstone::NoteAction::Play => {
                self.play_note(pos);
                if let Some(next) = pl.with_prop(state, "powered", "true") {
                    self.set_block(pos, next);
                }
            }
            crate::redstone::NoteAction::UpdateOnly => {
                if let Some(next) = pl.with_prop(state, "powered", "false") {
                    self.set_block(pos, next);
                }
            }
        }
    }

    /// `NoteBlock.tick`: смена ноты по сигналу кликом — `cycle(NOTE)`.
    fn note_next(&self, pos: [i32; 3]) -> Option<u32> {
        let pl = &self.survival.placement;
        let state = self.block(pos)?;
        let note: i32 = pl
            .prop(state, "note")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let next = (note + 1) % 25;
        pl.with_prop(state, "note", &next.to_string())
    }

    /// Крюк в стороне `dir` от нити: ванильный `updateSource` ищет его в
    /// пределах 42 блоков и требует, чтобы он смотрел навстречу.
    fn tripwire_hook_toward(&self, pos: [i32; 3], dir: usize) -> Option<[i32; 3]> {
        let pl = &self.survival.placement;
        for i in 1..crate::redstone::WIRE_DIST_MAX {
            let p = crate::redstone::place(pos, dir, i as usize);
            let s = self.block(p)?;
            if chunkwake_world::has_rare(s, chunkwake_world::rare::TRIPWIRE_HOOK) {
                return (pl.prop(s, "facing") == Some(crate::redstone::opposite_name(dir)))
                    .then_some(p);
            }
            if !chunkwake_world::has_rare(s, chunkwake_world::rare::TRIPWIRE) {
                return None;
            }
        }
        None
    }

    /// `TripWireHookBlock.calculateState`: от крюка в сторону `facing` найти
    /// второй крюк, посчитать `attached` и `powered` обоих, выставить их,
    /// проиграть звук и обновить соседей.
    ///
    /// `wire_source` — номер клетки, чьё состояние только что изменилось
    /// (`-1`, если смена пришла не от нити), `wire_state` — само состояние.
    fn tripwire_hook_state(
        &self,
        pos: [i32; 3],
        being_destroyed: bool,
        can_update: bool,
        wire_source: i32,
        wire_state: Option<u32>,
    ) {
        let pl = &self.survival.placement;
        let Some(state) = self.block(pos) else { return };
        let Some(facing) = pl.prop(state, "facing") else {
            return;
        };
        let direction = crate::redstone::hook_reaches(facing);
        let was_attached = pl.prop(state, "attached") == Some("true");
        let was_powered = pl.prop(state, "powered") == Some("true");
        // Расчёт цепи — чистой функцией: её проверяет тест
        // `tripwire_chain_matches_vanilla`, и она же не зависит от мира.
        let scan = crate::redstone::scan_chain(
            pl,
            direction,
            being_destroyed,
            wire_source,
            wire_state,
            |i| {
                let at = crate::redstone::place(pos, direction, i);
                self.block(at)
            },
        );
        let attached = scan.attached;
        let powered = scan.powered;
        let receiver = scan.receiver;
        let Some(base) = pl
            .with_prop(state, "attached", if attached { "true" } else { "false" })
            .and_then(|s| pl.with_prop(s, "powered", if powered { "true" } else { "false" }))
        else {
            return;
        };
        // Вход из нити — перепроверка крюка через период (`scheduleTick`).
        if scan.reschedule {
            self.schedule(pos, crate::redstone::RECHECK_PERIOD);
        }
        if receiver > 0 {
            let other = crate::redstone::place(pos, direction, receiver as usize);
            if let Some(o) = pl.with_prop(base, "facing", crate::redstone::opposite_name(direction))
            {
                self.set_block(other, o);
            }
            self.notify_hook_neighbors(other, crate::redstone::opposite(direction));
            // Второй крюк могли снести посреди расчёта.
            if !self
                .block(pos)
                .is_some_and(|s| chunkwake_world::has_rare(s, chunkwake_world::rare::TRIPWIRE_HOOK))
            {
                self.tripwire_hook_removed(base, pos);
                return;
            }
            self.tripwire_hook_sound(other, attached, powered, was_attached, was_powered);
        }
        self.tripwire_hook_sound(pos, attached, powered, was_attached, was_powered);
        if !being_destroyed {
            self.set_block(pos, base);
            if can_update {
                self.notify_hook_neighbors(pos, direction);
            }
        }
        // Нить получает `attached` от крюка.
        if was_attached != attached {
            for i in 1..receiver {
                let Some(wire) = scan.wire(i) else {
                    continue;
                };
                let at = crate::redstone::place(pos, direction, i as usize);
                let Some(n) =
                    pl.with_prop(wire, "attached", if attached { "true" } else { "false" })
                else {
                    continue;
                };
                if self
                    .block(at)
                    .is_some_and(|s| chunkwake_world::has_rare(s, chunkwake_world::rare::TRIPWIRE))
                {
                    self.set_block(at, n);
                }
            }
        }
    }

    /// `TripWireHookBlock.notifyNeighbors`: сам крюк и блок за ним.
    fn notify_hook_neighbors(&self, pos: [i32; 3], facing: usize) {
        self.redstone_update(&crate::redstone::around(pos));
        self.redstone_update(&crate::redstone::around(crate::redstone::place(
            pos, facing, 1,
        )));
    }

    fn tripwire_hook_sound(
        &self,
        pos: [i32; 3],
        attached: bool,
        powered: bool,
        was_attached: bool,
        was_powered: bool,
    ) {
        let random = rand::random::<f32>();
        if let Some((sound, volume, pitch)) =
            crate::redstone::hook_sound(powered, was_powered, attached, was_attached, random)
        {
            self.block_sound(None, sound, pos, volume, pitch);
        }
    }

    /// `onRemoved`: снятый крюк гасит сигнал, если был включён.
    fn tripwire_hook_removed(&self, state: u32, pos: [i32; 3]) {
        let pl = &self.survival.placement;
        let attached = pl.prop(state, "attached") == Some("true");
        let powered = pl.prop(state, "powered") == Some("true");
        let facing = pl
            .prop(state, "facing")
            .map(crate::redstone::hook_reaches)
            .unwrap_or(2);
        if attached || powered {
            self.tripwire_hook_state(pos, true, false, -1, None);
        }
        if powered {
            self.notify_hook_neighbors(pos, facing);
        }
    }

    fn chest_at(&self, pos: [i32; 3]) -> Option<ChestState> {
        self.survival.chests.get(&self.block(pos)?).copied()
    }

    /// Вторая половина двойного сундука и `true`, если `pos` — правая (первая) половина.
    fn chest_partner(&self, pos: [i32; 3]) -> Option<([i32; 3], bool)> {
        let c = self.chest_at(pos).filter(|c| c.kind != CHEST_SINGLE)?;
        let other = offset(pos, c.connected());
        let o = self.chest_at(other)?;
        (o.trapped == c.trapped && o.facing == c.facing && o.kind == 3 - c.kind)
            .then_some((other, c.kind == CHEST_RIGHT))
    }

    /// `ChestBlock.getStateForPlacement`: смотрит на игрока, цепляется к соседу.
    fn chest_placement(
        &self,
        pos: [i32; 3],
        c: ChestState,
        yaw: f32,
        face: i32,
        sneaking: bool,
    ) -> u32 {
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
            kind = if (d + 3) % 4 == (cf + 2) % 4 {
                CHEST_RIGHT
            } else {
                CHEST_LEFT
            };
        }
        if kind == CHEST_SINGLE && !sneaking {
            if partner((facing + 1) % 4) == Some(facing) {
                kind = CHEST_LEFT;
            } else if partner((facing + 3) % 4) == Some(facing) {
                kind = CHEST_RIGHT;
            }
        }
        let want = ChestState { facing, kind, ..c };
        self.survival
            .chest_states
            .get(&want)
            .copied()
            .unwrap_or_else(|| {
                self.survival
                    .chest_states
                    .get(&ChestState {
                        kind: CHEST_SINGLE,
                        ..want
                    })
                    .copied()
                    .unwrap_or(0)
            })
    }

    /// Поставленный двойной сундук: сосед становится второй половиной (`updateShape`).
    fn link_chest(&self, pos: [i32; 3], state: u32) {
        let Some(c) = self
            .survival
            .chests
            .get(&state)
            .copied()
            .filter(|c| c.kind != CHEST_SINGLE)
        else {
            return;
        };
        let other = offset(pos, c.connected());
        if let Some(o) = self.chest_at(other)
            && let Some(&s) = self.survival.chest_states.get(&ChestState {
                kind: 3 - c.kind,
                ..o
            })
        {
            self.set_block(other, s);
        }
    }

    /// Сломанная половина: оставшаяся становится одиночной.
    fn unlink_chest(&self, pos: [i32; 3], old: u32) {
        let Some(c) = self
            .survival
            .chests
            .get(&old)
            .copied()
            .filter(|c| c.kind != CHEST_SINGLE)
        else {
            return;
        };
        let other = offset(pos, c.connected());
        if let Some(o) = self
            .chest_at(other)
            .filter(|o| o.kind == 3 - c.kind && o.facing == c.facing)
            && let Some(&s) = self.survival.chest_states.get(&ChestState {
                kind: CHEST_SINGLE,
                ..o
            })
        {
            self.set_block(other, s);
        }
    }

    fn block_entity(&self, pos: [i32; 3]) -> Option<BlockEntity> {
        let key = (pos[0] >> 4, pos[2] >> 4);
        let world = self.edited.read().unwrap();
        // Диск не читается: это вызывается и из тика.
        match world.get(&key) {
            Some(e) => e.0.block_entities.get(&pos).cloned(),
            None if self.terrain.is_none() => self.template.block_entities.get(&pos).cloned(),
            None => None,
        }
    }

    /// Ставит или убирает блок-сущность; чанк сохранится и перекодируется.
    fn set_block_entity(&self, pos: [i32; 3], be: Option<BlockEntity>) {
        let key = (pos[0] >> 4, pos[2] >> 4);
        if self.terrain.is_some() && !self.edited.read().unwrap().contains_key(&key) {
            return;
        }
        self.materialize_void(key.0, key.1);
        let template = &self.template;
        let mut world = self.edited.write().unwrap();
        let (column, rev) = world.entry(key).or_insert_with(|| (template.clone(), 0));
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
        self.dirty.lock().unwrap().insert(key);
        self.chunk_frames.lock().unwrap().remove(&key);
    }

    /// 27 слотов сундука из `Items` его блок-сущности.
    fn container_items(&self, pos: [i32; 3]) -> Vec<Slot> {
        if let Some(id) = plugin_of(pos) {
            return self
                .plugin_invs
                .lock()
                .unwrap()
                .get(&id)
                .map_or_else(Vec::new, |p| p.slots.clone());
        }
        let size = self
            .block(pos)
            .map_or(27, |s| container_size(self.state_name(s)));
        let mut slots = vec![None; size];
        let Some(be) = self.block_entity(pos) else {
            return slots;
        };
        for t in be.nbt.get("Items").and_then(Tag::as_list).unwrap_or(&[]) {
            let Some(c) = t.as_compound() else { continue };
            let slot = c.get("Slot").and_then(Tag::as_i64).unwrap_or(-1);
            if let Some(dst) = slots.get_mut(slot.max(0) as usize).filter(|_| slot >= 0) {
                *dst = crate::stack_nbt::from_nbt(c, |n| self.item_id(n));
            }
        }
        slots
    }

    /// Сохраняет слоты сундука в его блок-сущность (`Items` как в ванилле).
    fn save_container(&self, pos: [i32; 3], slots: &[Slot]) {
        if let Some(id) = plugin_of(pos) {
            if let Some(p) = self.plugin_invs.lock().unwrap().get_mut(&id) {
                p.slots = slots.to_vec();
            }
            return;
        }
        let id = self
            .block(pos)
            .and_then(|s| container_entity(self.state_name(s)))
            .unwrap_or("minecraft:chest");
        let items = self.items_nbt(slots);
        let mut nbt = self.block_entity(pos).map(|b| b.nbt).unwrap_or_default();
        nbt.insert("Items".into(), Tag::List(items));
        self.set_block_entity(pos, Some(BlockEntity { id: id.into(), nbt }));
        // Компараторы рядом читают заполненность.
        self.redstone_update(&crate::redstone::around(pos));
    }

    /// `Items` контейнера: непустые слоты с номерами.
    fn items_nbt(&self, slots: &[Slot]) -> Vec<Tag> {
        slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let s = (*s)?;
                let mut c = crate::stack_nbt::named_nbt(s, self.item_name(s.item), &|id| {
                    let n = self.item_name(id);
                    (n != "minecraft:air").then(|| n.to_owned())
                })?;
                c.insert("Slot".into(), Tag::Byte(i as i8));
                Some(Tag::Compound(c))
            })
            .collect()
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
    ///
    /// Оставлена для тестов сессии; команда `/give` идёт через `Control::Give`.
    #[allow(dead_code)]
    async fn give_self(&self, _inv: &mut Inventory, args: &str) -> serde_json::Value {
        let mut it = args.split_whitespace();
        let name = it.next().unwrap_or("");
        let name = name.split(['[', '{']).next().unwrap_or("");
        let name = if name.contains(':') {
            name.to_owned()
        } else {
            format!("minecraft:{name}")
        };
        let count = it.next().map_or(Ok(1), str::parse::<i32>);
        let (Some(&item), Ok(count @ 1..=MAX_GIVE)) = (self.survival.item_ids.get(&name), count)
        else {
            return serde_json::json!({
                "text": format!("Usage: /give <item> [1-{MAX_GIVE}]"),
                "color": "red",
            });
        };
        serde_json::json!({"text": format!("Gave {count} {name}"), "item": item})
    }

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
        conn.send(&play::player_abilities(
            ids.player_abilities,
            mode,
            self.allow_flight.load(Ordering::Relaxed),
        ))
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

    /// Скорость движения у клиента (эффекты Скорость/Замедление; клиент сам их не применяет).
    async fn send_speed(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        v: &Vitals,
    ) -> Result<(), NetError> {
        let amp = |name: &str| {
            self.survival
                .effects
                .get(name)
                .and_then(|&id| v.effect(id))
                .map(|e| e.amplifier)
        };
        let mut mods = survival::speed_modifiers(
            v.sprinting,
            amp("minecraft:speed"),
            amp("minecraft:slowness"),
        );
        mods.extend(self.enchant_attribute_modifiers(uuid, "minecraft:movement_speed"));
        let mods = self.merge_attribute_modifiers(uuid, "minecraft:movement_speed", &mods);
        let base = self.attribute_base(
            uuid,
            "minecraft:movement_speed",
            survival::BASE_MOVEMENT_SPEED,
        );
        conn.send(&inv::update_attribute(
            self.ids.update_attributes,
            entity,
            self.survival.attribute_speed,
            base,
            &mods,
        ))
        .await
    }

    /// `DoorBlock`/`TrapDoorBlock`/`FenceGateBlock.useWithoutItem`: открыть или закрыть
    /// (дверь — обе половины), звук остальным. Калитка открывается от игрока.
    async fn toggle_block(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        pos: [i32; 3],
        yaw: f32,
    ) -> Result<bool, NetError> {
        let Some(state) = self.block(pos) else {
            return Ok(false);
        };
        let pl = &self.survival.placement;
        let name = self.state_name(state).to_owned();
        let open = pl.prop(state, "open") != Some("true");
        let Some(sound) = crate::sounds::toggle_sound(&name, open) else {
            return Ok(false);
        };
        let value = if open { "true" } else { "false" };
        let Some(mut next) = pl.with_prop(state, "open", value) else {
            return Ok(false);
        };
        if open && name.ends_with("_fence_gate") {
            // Как в ваниле: открывается от игрока — если смотрит на него, разворот.
            let dir = ["south", "west", "north", "east"]
                [((yaw.rem_euclid(360.0) / 90.0 + 0.5) as usize) % 4];
            let back = |d: &str| match d {
                "north" => "south",
                "south" => "north",
                "east" => "west",
                _ => "east",
            };
            if pl.prop(state, "facing") == Some(back(dir)) {
                next = pl.with_prop(next, "facing", dir).unwrap_or(next);
            }
        }
        let mut changed = vec![(pos, next)];
        if let Some(o) = pl.partner(state).filter(|_| name.ends_with("_door")) {
            let p = [pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]];
            if let Some(other) = self.block(p).filter(|&s| pl.name(s) == pl.name(state))
                && let Some(n) = pl.with_prop(other, "open", value)
            {
                changed.push((p, n));
            }
        }
        for &(p, s) in &changed {
            self.set_block(p, s);
            conn.send(&play::block_update(self.ids.block_update, p, s as i32))
                .await?;
        }
        self.block_sound(Some(uuid), &sound, pos, 1.0, crate::sounds::pitch_09());
        Ok(true)
    }

    /// `Level.playSound(except, …)`: звук всем, кроме `skip`.
    fn play_sound(
        &self,
        skip: Option<Uuid>,
        name: &str,
        source: i32,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
    ) {
        let Some(&id) = self.survival.sounds.get(name) else {
            return;
        };
        self.hub.send(
            skip,
            play::sound(
                self.ids.sound,
                id,
                source,
                pos,
                volume,
                pitch,
                rand::random(),
            ),
        );
    }

    fn block_sound(&self, skip: Option<Uuid>, name: &str, pos: [i32; 3], volume: f32, pitch: f32) {
        let c = [
            pos[0] as f64 + 0.5,
            pos[1] as f64 + 0.5,
            pos[2] as f64 + 0.5,
        ];
        self.play_sound(skip, name, crate::sounds::BLOCKS, c, volume, pitch);
    }

    /// Звук у игрока (`Player.playSound` с `null` — слышат все, и он сам).
    fn player_sound(&self, uuid: Uuid, name: &str, volume: f32, pitch: f32) {
        if let Some((feet, _, _)) = self.player_at(uuid) {
            self.play_sound(None, name, crate::sounds::PLAYERS, feet, volume, pitch);
        }
    }

    /// Звуки опыта: сфера (`handleTakeItemEntity`) и каждые 5 уровней — `levelup`.
    fn xp_sounds(&self, uuid: Uuid, old_level: i32, xp: &crate::xp::Experience) {
        if xp.level > old_level && xp.level % 5 == 0 {
            let f = if xp.level > 30 {
                1.0
            } else {
                xp.level as f32 / 30.0
            };
            self.player_sound(uuid, "minecraft:entity.player.levelup", f * 0.75, 1.0);
        }
    }

    /// Шлёт атрибуты от чар снаряжения (Эффективность, Подводник, Глубинный шаг…),
    /// если они изменились: клиент сам их не считает.
    async fn sync_enchant_attrs(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        inv: &survival::Inventory,
        last: &mut std::collections::BTreeMap<String, Vec<(String, f64, i32)>>,
        gear: &mut [(i32, u32); 6],
    ) -> Result<(), NetError> {
        let piece = |i: usize| inv.slots[i].map(|s| (s.item, s.extra)).unwrap_or((-1, 0));
        let gear_now = [
            piece(inv.held_slot()),
            piece(survival::OFFHAND),
            piece(5),
            piece(6),
            piece(7),
            piece(8),
        ];
        if gear_now == *gear {
            return Ok(());
        }
        let ench = |i: usize| inv.slots[i].map(|s| chunkwake_protocol::components::get(s.extra));
        let slots = [
            ("mainhand", ench(inv.held_slot())),
            ("offhand", ench(survival::OFFHAND)),
            ("head", ench(5)),
            ("chest", ench(6)),
            ("legs", ench(7)),
            ("feet", ench(8)),
        ];
        let equipment: Vec<(&str, &[(i32, i32)])> = slots
            .iter()
            .filter_map(|(n, x)| x.as_ref().map(|x| (*n, x.enchantments.as_slice())))
            .collect();
        let now = crate::enchanting::attribute_modifiers(&self.survival.enchanting, &equipment);
        if now == *last {
            *gear = gear_now;
            return Ok(());
        }
        let names: std::collections::BTreeSet<&String> = now.keys().chain(last.keys()).collect();
        for name in names {
            let mods = now.get(name).map_or(&[][..], Vec::as_slice);
            if last.get(name).map_or(&[][..], Vec::as_slice) == mods {
                continue;
            }
            let Some(&id) = self.survival.attributes.get(name.as_str()) else {
                continue;
            };
            let mut merged = mods.to_vec();
            merged.extend(self.live_motion_modifiers(uuid, name));
            let merged = self.merge_attribute_modifiers(uuid, name, &merged);
            let base = self.attribute_base(uuid, name, self.attribute_fallback(uuid, name));
            conn.send(&inv::update_attribute(
                self.ids.update_attributes,
                entity,
                id,
                base,
                &merged,
            ))
            .await?;
        }
        *last = now;
        *gear = gear_now;
        Ok(())
    }

    /// Меняет ли эффект скорость движения.
    fn moves_speed(&self, id: i32) -> bool {
        ["minecraft:speed", "minecraft:slowness"]
            .iter()
            .any(|n| self.survival.effects.get(*n) == Some(&id))
    }

    fn effect_amp(&self, v: &Vitals, name: &str) -> Option<u8> {
        self.survival
            .effects
            .get(name)
            .and_then(|&id| v.effect(id))
            .map(|e| e.amplifier)
    }

    fn effect_kinds(&self) -> survival::EffectKinds {
        self.effect_kinds
    }

    /// Биты атрибутов, которые надо переслать: 1 скорость, 2 прыжок, 4 гравитация.
    fn motion_mask(&self, id: i32) -> u8 {
        let mut m = 0;
        if self.moves_speed(id) {
            m |= 1;
        }
        if self.survival.effects.get("minecraft:jump_boost") == Some(&id) {
            m |= 2;
        }
        if self.survival.effects.get("minecraft:slow_falling") == Some(&id) {
            m |= 4;
        }
        m
    }

    async fn sync_motion(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        v: &Vitals,
        mask: u8,
    ) -> Result<(), NetError> {
        if mask & 1 != 0 {
            self.send_speed(conn, uuid, entity, v).await?;
        }
        if mask & 2 != 0 {
            self.send_jump(conn, uuid, entity, v).await?;
        }
        if mask & 4 != 0 {
            self.send_gravity(conn, uuid, entity, v).await?;
        }
        Ok(())
    }

    async fn send_jump(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        v: &Vitals,
    ) -> Result<(), NetError> {
        let mods = survival::jump_modifiers(self.effect_amp(v, "minecraft:jump_boost"));
        let mods = self.merge_attribute_modifiers(uuid, "minecraft:jump_strength", &mods);
        let base = self.attribute_base(
            uuid,
            "minecraft:jump_strength",
            survival::BASE_JUMP_STRENGTH,
        );
        conn.send(&inv::update_attribute(
            self.ids.update_attributes,
            entity,
            self.survival.attribute_jump,
            base,
            &mods,
        ))
        .await
    }

    async fn send_gravity(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        entity: i32,
        v: &Vitals,
    ) -> Result<(), NetError> {
        let slow = self.effect_amp(v, "minecraft:slow_falling").is_some();
        let mods = survival::gravity_modifiers(slow);
        let mods = self.merge_attribute_modifiers(uuid, "minecraft:gravity", &mods);
        let base = self.attribute_base(uuid, "minecraft:gravity", survival::BASE_GRAVITY);
        conn.send(&inv::update_attribute(
            self.ids.update_attributes,
            entity,
            self.survival.attribute_gravity,
            base,
            &mods,
        ))
        .await
    }

    /// Эффекты скорости, прыжка и гравитации из зеркала игрока.
    fn live_motion_modifiers(&self, uuid: Uuid, name: &str) -> Vec<(String, f64, i32)> {
        let amp = |effect: &str| {
            let id = *self.survival.effects.get(effect)?;
            self.live
                .lock()
                .unwrap()
                .get(&uuid)?
                .effects
                .iter()
                .find(|active| active.id == id)
                .map(|active| active.amplifier)
        };
        match name {
            "minecraft:movement_speed" => {
                survival::speed_modifiers(false, amp("minecraft:speed"), amp("minecraft:slowness"))
            }
            "minecraft:jump_strength" => survival::jump_modifiers(amp("minecraft:jump_boost")),
            "minecraft:gravity" => {
                survival::gravity_modifiers(amp("minecraft:slow_falling").is_some())
            }
            _ => Vec::new(),
        }
    }

    /// Запасная база, если команда её не меняла: игрок, моб или ноль.
    fn attribute_fallback(&self, uuid: Uuid, name: &str) -> f64 {
        if self.hub.players.lock().unwrap().contains_key(&uuid) {
            return match name {
                "minecraft:movement_speed" => survival::BASE_MOVEMENT_SPEED,
                "minecraft:jump_strength" => survival::BASE_JUMP_STRENGTH,
                "minecraft:gravity" => survival::BASE_GRAVITY,
                _ => crate::enchanting::attribute_base(name),
            };
        }
        crate::enchanting::attribute_base(name)
    }

    /// Рука с питьевым зельем: слот, предмет, компоненты, секунды. Наблюдатель не пьёт.
    fn drink_use(
        &self,
        inv: &Inventory,
        hand: i32,
        dead: bool,
        mode: u8,
    ) -> Option<(usize, i32, u32, f32)> {
        if dead || mode == 3 {
            return None;
        }
        let slot = match hand {
            0 => inv.held_slot(),
            1 => survival::OFFHAND,
            _ => return None,
        };
        let stack = inv.slots.get(slot)?.as_ref()?;
        let drink = self.survival.items.get(stack.item as usize)?.drink?;
        Some((slot, stack.item, stack.extra, drink.seconds))
    }

    /// Допить: в выживании бутылка вместо зелья, затем эффекты (`POTION_DRINK`).
    async fn finish_drink(
        &self,
        conn: &mut Connection,
        me: &Online,
        state: &mut PlayerState,
        slot: usize,
        takes_damage: bool,
    ) -> Result<(), NetError> {
        let Some(stack) = state.inv.slots[slot] else {
            return Ok(());
        };
        let raw = chunkwake_protocol::components::get(stack.extra).raw.clone();
        let contents = raw
            .as_deref()
            .and_then(|b| crate::potions::read_contents(b, self.survival.potion_component));
        if takes_damage {
            let rest = self
                .survival
                .items
                .get(stack.item as usize)
                .and_then(|i| i.drink)
                .map(|d| d.remainder)
                .filter(|&id| id >= 0);
            if stack.count <= 1 {
                state.inv.slots[slot] = rest.map(|id| Stack::new(id, 1));
            } else {
                state.inv.slots[slot] = Some(Stack {
                    count: stack.count - 1,
                    ..stack
                });
                if let Some(id) = rest {
                    let _ = state.inv.add_stack(Stack::new(id, 1), 64);
                }
            }
            self.send_inventory(conn, &mut state.inv).await?;
        }
        if let Some(c) = contents {
            let mut list = Vec::new();
            if let Some(id) = c.potion
                && let Some(name) = self
                    .survival
                    .potions
                    .get(id as usize)
                    .filter(|n| !n.is_empty())
            {
                for brew in crate::potions::effects(name) {
                    if let Some(&eid) = self.survival.effects.get(brew.effect) {
                        list.push(survival::Effect::plain(eid, brew.amplifier, brew.duration));
                    }
                }
            }
            for e in c.effects {
                list.push(survival::Effect {
                    id: e.id,
                    amplifier: e.amplifier,
                    duration: e.duration,
                    ambient: e.ambient,
                    particles: e.particles,
                    show_icon: e.show_icon,
                    age: 0,
                });
            }
            for e in list {
                if !survival::will_apply(&state.vitals.effects, e, false) {
                    continue;
                }
                let old = state.vitals.effect(e.id).copied();
                let action = if old.is_some() { "CHANGED" } else { "ADDED" };
                let name = self.effect_name(e.id).unwrap_or_default();
                if self
                    .potion_cancelled(
                        me.entry.uuid,
                        "POTION_DRINK",
                        action,
                        name,
                        Some(&e),
                        old.as_ref(),
                    )
                    .await
                {
                    continue;
                }
                self.grant_effect(
                    conn,
                    me,
                    &mut state.vitals,
                    &mut state.inv,
                    &mut state.xp,
                    takes_damage,
                    e,
                    false,
                    false,
                )
                .await?;
            }
        }
        conn.send(&inv::entity_event(
            self.ids.entity_event,
            me.entity_id,
            inv::ENTITY_EVENT_USE_ITEM_COMPLETE,
        ))
        .await?;
        Ok(())
    }

    /// `EntityPotionEffectEvent` с сервера. `true` — отменено.
    async fn potion_cancelled(
        &self,
        uuid: Uuid,
        cause: &str,
        action: &str,
        name: &str,
        new_effect: Option<&survival::Effect>,
        old: Option<&survival::Effect>,
    ) -> bool {
        let fmt = |e: &survival::Effect| {
            format!(
                "{} {} {} {} {}",
                e.amplifier,
                e.duration,
                u8::from(e.ambient),
                u8::from(e.particles),
                u8::from(e.show_icon)
            )
        };
        let arg = match (action, new_effect, old) {
            ("CLEARED", _, _) => format!("{cause} CLEARED"),
            (_, Some(n), Some(o)) => format!("{cause} {action} {name} {} {}", fmt(n), fmt(o)),
            (_, Some(n), None) | (_, None, Some(n)) => {
                format!("{cause} {action} {name} {}", fmt(n))
            }
            _ => format!("{cause} {action}"),
        };
        self.plugin_event(Event::Potion, uuid, || arg)
            .await
            .is_some_and(|r| r.trim() == "c")
    }

    /// Наложить эффект: для `/effect` сначала событие COMMAND.
    #[allow(clippy::too_many_arguments)]
    async fn grant_effect(
        &self,
        conn: &mut Connection,
        me: &Online,
        v: &mut Vitals,
        inv: &mut Inventory,
        xp: &mut crate::xp::Experience,
        takes_damage: bool,
        e: survival::Effect,
        force: bool,
        command: bool,
    ) -> Result<(), NetError> {
        if command && !survival::will_apply(&v.effects, e, force) {
            return Ok(());
        }
        if command {
            let old = v.effect(e.id).copied();
            let action = if old.is_some() { "CHANGED" } else { "ADDED" };
            let name = self.effect_name(e.id).unwrap_or_default();
            if self
                .potion_cancelled(
                    me.entry.uuid,
                    "COMMAND",
                    action,
                    name,
                    Some(&e),
                    old.as_ref(),
                )
                .await
            {
                return Ok(());
            }
        }
        if !v.add_effect(e, force) {
            return Ok(());
        }
        conn.send(&inv::update_mob_effect(
            self.ids.update_mob_effect,
            me.entity_id,
            e.id,
            e.amplifier as i32,
            e.duration,
            e.flags(),
        ))
        .await?;
        self.sync_motion(conn, me.entry.uuid, me.entity_id, v, self.motion_mask(e.id))
            .await?;
        let name = self.effect_name(e.id).unwrap_or_default();
        match survival::instant_effect(name, e.amplifier) {
            survival::Instant::Heal(n) if !v.dead => {
                v.heal(n);
                self.send_health(conn, v).await?;
            }
            survival::Instant::Harm(n) if takes_damage && !v.dead => {
                self.hurt(conn, me, v, inv, xp, n, Cause::Magic).await?;
            }
            survival::Instant::Eat(food, sat) if !v.dead => {
                v.eat(food, sat);
                self.send_health(conn, v).await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Снять эффекты. `/effect clear` вызывает COMMAND (`CLEARED` или `REMOVED`).
    async fn revoke_effects(
        &self,
        conn: &mut Connection,
        me: &Online,
        v: &mut Vitals,
        id: Option<i32>,
        command: bool,
    ) -> Result<(), NetError> {
        let targets: Vec<survival::Effect> = v
            .effects
            .iter()
            .copied()
            .filter(|e| id.is_none_or(|w| w == e.id))
            .collect();
        if targets.is_empty() {
            return Ok(());
        }
        if command {
            let cancelled = if id.is_none() {
                self.potion_cancelled(me.entry.uuid, "COMMAND", "CLEARED", "", None, None)
                    .await
            } else {
                let e = targets[0];
                let name = self.effect_name(e.id).unwrap_or_default();
                self.potion_cancelled(me.entry.uuid, "COMMAND", "REMOVED", name, None, Some(&e))
                    .await
            };
            if cancelled {
                return Ok(());
            }
        }
        let mut motion = 0u8;
        for e in &targets {
            conn.send(&inv::remove_mob_effect(
                self.ids.remove_mob_effect,
                me.entity_id,
                e.id,
            ))
            .await?;
            motion |= self.motion_mask(e.id);
        }
        v.effects.retain(|e| id.is_some_and(|w| w != e.id));
        self.sync_motion(conn, me.entry.uuid, me.entity_id, v, motion)
            .await
    }

    async fn send_xp(
        &self,
        conn: &mut Connection,
        xp: &crate::xp::Experience,
    ) -> Result<(), NetError> {
        conn.send(&inv::set_experience(
            self.ids.set_experience,
            xp.progress,
            xp.level,
            xp.total,
        ))
        .await
    }

    /// Варианты стола для предмета в окне (зачарованный или незачаряемый — пусто)
    /// и число полок.
    fn enchant_offer(&self, w: &Window, seed: i32) -> (crate::enchanting::Offer, i32) {
        let none = crate::enchanting::Offer {
            clue: [-1; 3],
            level: [-1; 3],
            ..Default::default()
        };
        let item = w.slots.first().copied().flatten().filter(|s| {
            chunkwake_protocol::components::get(s.extra)
                .enchantments
                .is_empty()
        });
        let Some(s) = item else {
            return (none, 0);
        };
        let e = &self.survival.enchanting;
        let at = |o: [i32; 3]| {
            let p = [w.pos[0] + o[0], w.pos[1] + o[1], w.pos[2] + o[2]];
            self.state_name(self.block(p).unwrap_or(AIR)).to_owned()
        };
        let shelves = crate::enchanting::shelves(e, at);
        (
            crate::enchanting::offer(e, self.item_name(s.item), shelves, seed),
            shelves,
        )
    }

    fn enchant_name(&self, id: i32) -> &str {
        self.survival
            .enchanting
            .defs
            .iter()
            .find(|d| d.id == id)
            .map_or("-", |d| d.name.as_str())
    }

    fn enchant_id(&self, name: &str) -> Option<i32> {
        self.survival
            .enchanting
            .defs
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.id)
    }

    /// `slotsChanged`: при смене предмета, полок или зерна — новые варианты и
    /// `PrepareItemEnchantEvent`; затем шлёт изменившиеся свойства стола.
    /// `PrepareGrindstoneEvent` после клика: плагин может подменить результат.
    async fn refresh_grindstone(
        &self,
        conn: &mut Connection,
        inv: &mut Inventory,
        w: &mut Window,
        uuid: Uuid,
    ) -> Result<(), NetError> {
        let (top, bottom, result) = (
            w.slots.first().copied().flatten(),
            w.slots.get(1).copied().flatten(),
            w.slots.get(2).copied().flatten(),
        );
        let Some(reply) = self
            .plugin_event(Event::PrepareGrindstone, uuid, || {
                format!(
                    "{} {} {}\n{}\n{}\n{}",
                    w.pos[0],
                    w.pos[1],
                    w.pos[2],
                    self.slot_line(top),
                    self.slot_line(bottom),
                    self.slot_line(result),
                )
            })
            .await
        else {
            return Ok(());
        };
        let next = match reply.trim() {
            "" => return Ok(()),
            "e" => None,
            s => {
                let Some(rest) = s.strip_prefix("i ") else {
                    return Ok(());
                };
                crate::stack_nbt::from_line(rest, |n| self.item_id(n))
            }
        };
        if w.slots.get(2).copied().flatten() == next {
            return Ok(());
        }
        if let Some(slot) = w.slots.get_mut(2) {
            *slot = next;
            self.send_window(conn, inv, Some(w), w.id).await?;
        }
        Ok(())
    }

    async fn refresh_enchant(
        &self,
        conn: &mut Connection,
        w: &mut Window,
        seed: i32,
        uuid: Uuid,
    ) -> Result<(), NetError> {
        let item = w.slots.first().copied().flatten();
        let (mut offer, shelves) = self.enchant_offer(w, seed);
        let key = (item, shelves, seed);
        if w.offer.as_ref().is_some_and(|(k, _)| *k == key) {
            offer = w.offer.as_ref().unwrap().1;
        } else if let Some(s) = item.filter(|_| offer.costs.iter().any(|&c| c > 0)) {
            let reply = self
                .plugin_event(Event::PrepareEnchant, uuid, || {
                    let mut arg = format!(
                        "{} {} {} {shelves}\n{}",
                        w.pos[0],
                        w.pos[1],
                        w.pos[2],
                        crate::stack_nbt::to_line(s, self.item_name(s.item))
                    );
                    for i in 0..3 {
                        arg += &format!(
                            "\n{} {} {}",
                            offer.costs[i],
                            if offer.clue[i] < 0 {
                                "-"
                            } else {
                                self.enchant_name(offer.clue[i])
                            },
                            offer.level[i]
                        );
                    }
                    arg
                })
                .await;
            match reply.as_deref().map(str::trim) {
                Some("c") => {
                    offer = crate::enchanting::Offer {
                        clue: [-1; 3],
                        level: [-1; 3],
                        ..Default::default()
                    }
                }
                Some(r) if !r.is_empty() => {
                    for (i, line) in r.lines().take(3).enumerate() {
                        let f: Vec<&str> = line.split(' ').collect();
                        offer.costs[i] = f.first().and_then(|c| c.parse().ok()).unwrap_or(0);
                        offer.clue[i] = f.get(1).and_then(|n| self.enchant_id(n)).unwrap_or(-1);
                        offer.level[i] = f.get(2).and_then(|c| c.parse().ok()).unwrap_or(-1);
                    }
                }
                _ => {}
            }
        }
        w.offer = Some((key, offer));
        let data = offer.data(seed);
        for (i, &v) in data.iter().enumerate() {
            if w.enchant[i] != v {
                conn.send(&inv::container_set_data(
                    self.ids.container_set_data,
                    w.id,
                    i as i16,
                    v as i16,
                ))
                .await?;
            }
        }
        w.enchant = data;
        Ok(())
    }

    /// `EnchantmentMenu.clickMenuButton`: чары варианта `button`, `EnchantItemEvent`,
    /// списание уровней (`button + 1`) и лазурита (кроме творческого), новое зерно.
    async fn enchant(
        &self,
        w: &mut Window,
        button: i32,
        xp: &mut crate::xp::Experience,
        creative: bool,
        uuid: Uuid,
    ) -> bool {
        use chunkwake_protocol::components;
        let Some(b) = usize::try_from(button).ok().filter(|&b| b < 3) else {
            return false;
        };
        let Some((_, offer)) = w.offer else {
            return false;
        };
        let mut cost = offer.costs[b];
        let need = button + 1;
        let Some(item) = w.slots[0] else {
            return false;
        };
        let lapis_id = self.item_id(crate::enchanting::LAPIS);
        let lapis = w.slots[1]
            .filter(|s| Some(s.item) == lapis_id)
            .map_or(0, |s| s.count);
        if cost <= 0 || (!creative && (lapis < need || xp.level < cost)) {
            return false;
        }
        let name = self.item_name(item.item);
        let mut list =
            crate::enchanting::enchantments(&self.survival.enchanting, name, button, cost, xp.seed);
        let reply = self
            .plugin_event(Event::Enchant, uuid, || {
                let hint = if offer.clue[b] < 0 {
                    "-"
                } else {
                    self.enchant_name(offer.clue[b])
                };
                let mut arg = format!(
                    "{} {} {} {cost} {button} {hint} {}\n{}",
                    w.pos[0],
                    w.pos[1],
                    w.pos[2],
                    offer.level[b],
                    crate::stack_nbt::to_line(item, name)
                );
                for &(id, level) in &list {
                    arg += &format!("\n{} {level}", self.enchant_name(id));
                }
                arg
            })
            .await;
        match reply.as_deref().map(str::trim) {
            Some("c") => return false,
            Some(r) if !r.is_empty() => {
                let mut lines = r.lines();
                cost = lines.next().and_then(|c| c.parse().ok()).unwrap_or(cost);
                list = lines
                    .filter_map(|l| {
                        let (n, lv) = l.split_once(' ')?;
                        Some((self.enchant_id(n)?, lv.parse().ok()?))
                    })
                    .collect();
                if !creative && xp.level < cost {
                    return false;
                }
            }
            _ => {}
        }
        if list.is_empty() {
            return false;
        }
        let mut extra = (*components::get(item.extra)).clone();
        let mut out = item;
        if name == crate::enchanting::BOOK {
            out.item = self
                .item_id(crate::enchanting::ENCHANTED_BOOK)
                .unwrap_or(item.item);
            extra.stored = list;
        } else {
            extra.enchantments = list;
        }
        out.extra = components::intern(extra);
        w.slots[0] = Some(out);
        if !creative {
            w.slots[1] = w.slots[1].and_then(|s| {
                (s.count > need).then_some(Stack {
                    count: s.count - need,
                    ..s
                })
            });
        }
        xp.enchanted(need, rand::random());
        true
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
    #[allow(clippy::too_many_arguments)]
    async fn hurt(
        &self,
        conn: &mut Connection,
        me: &Online,
        v: &mut Vitals,
        inv: &mut Inventory,
        xp: &mut crate::xp::Experience,
        amount: f32,
        cause: Cause,
    ) -> Result<(), NetError> {
        let mut amount = survival::resisted(
            amount,
            self.effect_amp(v, "minecraft:resistance"),
            cause.bypasses_resistance(),
        );
        if !cause.bypasses_armor() {
            let (armor, toughness) = (5..=8).fold((0.0, 0.0), |(a, t), i| {
                let Some(stack) = inv.slots[i] else {
                    return (a, t);
                };
                let info = self.survival.items.get(stack.item as usize);
                (
                    a + info.map_or(0.0, |n| n.armor),
                    t + info.map_or(0.0, |n| n.toughness),
                )
            });
            amount = survival::after_armor(amount, armor, toughness);
        }
        // Чары брони (Защита, Невесомость…): `bypasses_enchantments` из наших причин нет.
        let prot = {
            let ench =
                |i: usize| inv.slots[i].map(|s| chunkwake_protocol::components::get(s.extra));
            let armor = [
                ("head", ench(5)),
                ("chest", ench(6)),
                ("legs", ench(7)),
                ("feet", ench(8)),
            ];
            let eq: Vec<(&str, &[(i32, i32)])> = armor
                .iter()
                .filter_map(|(n, x)| x.as_ref().map(|x| (*n, x.enchantments.as_slice())))
                .collect();
            crate::enchanting::damage_protection(
                &self.survival.enchanting,
                &eq,
                cause.damage_type(),
            )
        };
        amount = crate::enchanting::after_protection(amount, prot);
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
        if amount <= 0.0 {
            return Ok(());
        }
        let died = v.damage(amount);
        self.send_health(conn, v).await?;
        let damage_type = match cause {
            Cause::Fall => Some(self.survival.damage_fall),
            Cause::Void => Some(self.survival.damage_void),
            Cause::Starve => Some(self.survival.damage_starve),
            Cause::Poison | Cause::Magic => Some(self.survival.damage_magic),
            Cause::Wither => Some(self.survival.damage_wither),
            Cause::Arrow => Some(self.survival.damage_arrow),
            // `minecraft:outside_border` — сетевой ID из реестра.
            Cause::OutsideBorder => Some(self.survival.damage_void),
            Cause::Generic => None,
        };
        if let Some(t) = damage_type {
            self.hub.send(
                None,
                inv::damage_event(self.ids.damage_event, me.entity_id, t),
            );
        }
        if died {
            self.die(conn, me, inv, xp, cause).await?;
        }
        Ok(())
    }

    /// Смерть (`ServerPlayer.die`): `PlayerDeathEvent`, выпадение инвентаря, сообщение.
    async fn die(
        &self,
        conn: &mut Connection,
        me: &Online,
        inv: &mut Inventory,
        xp: &mut crate::xp::Experience,
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
            let msg = if cause == Cause::Arrow {
                serde_json::json!({"translate": cause.death_key(), "with": [me.entry.name, {"translate": "entity.minecraft.arrow"}]})
            } else {
                serde_json::json!({"translate": cause.death_key(), "with": [me.entry.name]})
            };
            let mut message = Some(msg.to_string());
            let mut keep = false;
            // `getBaseExperienceReward`: уровень × 7, не больше 100 (keepInventory — 0).
            let mut dropped_xp = (xp.level * 7).min(100);
            let mut keep_level = false;
            let mut new = (0, 0, 0);
            let reply = self
                .plugin_event(Event::Death, me.entry.uuid, || {
                    let mut arg = format!("{} {dropped_xp}\n{msg}", cause.damage_type());
                    for s in &drops {
                        arg += "\n";
                        arg += &crate::stack_nbt::to_line(*s, self.item_name(s.item));
                    }
                    arg
                })
                .await;
            if let Some(reply) = reply {
                let mut lines = reply.lines();
                let head: Vec<i32> = lines
                    .next()
                    .unwrap_or("")
                    .split(' ')
                    .map(|f| f.parse().unwrap_or(0))
                    .collect();
                let f = |i: usize| head.get(i).copied().unwrap_or(0);
                keep = f(0) == 1;
                keep_level = f(1) == 1;
                dropped_xp = f(2).max(0);
                new = (f(3).max(0), f(4).max(0), f(5).max(0));
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
                if dropped_xp > 0 {
                    self.spawn_orbs(feet, dropped_xp);
                }
                for s in drops {
                    let vel = items::scatter_velocity(rand::random(), rand::random());
                    self.drop_item(items::throw_origin(feet), vel, s, items::THROW_DELAY);
                }
            }
            // Без keepLevel опыт сменится на newLevel/newExp/newTotalExp (`ServerPlayer.reset`);
            // клиенту — при возрождении.
            if !keep_level {
                let mut next = crate::xp::Experience {
                    seed: xp.seed,
                    level: new.0,
                    ..Default::default()
                };
                next.add_points(new.1);
                next.total = new.2;
                *xp = next;
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
        stream: &mut stream::ChunkStream,
    ) -> Result<(i32, i32), NetError> {
        let now = ((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
        if now != stream.center() {
            self.stream_move(conn, stream, now).await?;
        }
        conn.send(&play::player_position(
            self.ids.player_position,
            teleport_id,
            pos,
            yaw,
            pitch,
        ))
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
        stream: &mut stream::ChunkStream,
        teleport_id: i32,
    ) -> Result<(i32, i32), NetError> {
        let lingering: Vec<i32> = v.effects.iter().map(|e| e.id).collect();
        let mut motion = 0u8;
        for id in lingering {
            let pkt = inv::remove_mob_effect(self.ids.remove_mob_effect, me.entity_id, id);
            conn.send(&pkt).await?;
            self.hub.send(Some(me.entry.uuid), pkt);
            motion |= self.motion_mask(id);
        }
        v.respawn();
        let spawn = self
            .spawn_points
            .lock()
            .unwrap()
            .get(&me.entry.uuid)
            .copied()
            .flatten()
            .map(|[x, y, z]| [x as f64 + 0.5, y as f64, z as f64 + 0.5])
            .unwrap_or_else(|| self.vars.world_spawn().unwrap_or(self.spawn));
        v.reset_fall(spawn);
        conn.send(&inv::respawn(self.ids.respawn, &login.spawn, 0))
            .await?;
        self.send_map_markers(conn).await?;
        for packet in self.joined_packets(me.entry.uuid) {
            conn.send(&packet).await?;
        }
        let spawn_chunk = (
            (spawn[0].floor() as i32) >> 4,
            (spawn[2].floor() as i32) >> 4,
        );
        if spawn_chunk != stream.center() {
            self.stream_move(conn, stream, spawn_chunk).await?;
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
            spawn,
            0.0,
            0.0,
        ))
        .await?;
        self.send_health(conn, v).await?;
        self.send_inventory(conn, inv).await?;
        // Клиент сам не сбрасывает прыжок и гравитацию вместе с эффектом.
        self.sync_motion(conn, me.entry.uuid, me.entity_id, v, motion)
            .await?;
        // Остальные: убрать тело и показать игрока заново в точке спавна.
        self.hub.send_entity(
            Some(me.entry.uuid),
            play::remove_entities(self.ids.remove_entities, &[me.entity_id]),
            Some(EntityEvent::Gone(me.entity_id)),
        );
        self.moved(
            me.entry.uuid,
            Move {
                pos: Some(spawn),
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
                // Кадр удаления одной сущности не меняется между получателями:
                // берём его из кэша вместо кодирования и сжатия на каждого,
                // кто её видел (при уходе в чащу это все игроки разом).
                out.extend(self.remove_frame(e.entity_id));
                visible.remove(&e.entity_id);
            }
            (false, false) => {}
        }
    }
}

/// Возвращает буфер тика в запас мира: следующий тик возьмёт его пустым.
fn return_tick_buffer<T>(slot: &Mutex<Vec<T>>, buf: &mut Vec<T>) {
    buf.clear();
    *slot.lock().unwrap() = std::mem::take(buf);
}

fn acknowledge_player_save(
    pending: &mut FastMap<Uuid, PlayerSave>,
    uuid: Uuid,
    written: &PlayerSave,
) {
    if pending.get(&uuid) == Some(written) {
        pending.remove(&uuid);
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
    use super::{acknowledge_player_save, chunks_around, dominant_axis, near};
    use crate::survival::PlayerState;
    use uuid::Uuid;

    /// `DaylightDetectorBlock.updateSignalStrength`: сигнал — небесный свет,
    /// после полудня угол тянется к `2π` и `power` множится на косинус.
    #[test]
    fn daylight_detector_power_matches_vanilla() {
        let power = |sky: i32, inverted: bool, day: i64| -> i32 {
            let mut target = sky;
            if inverted {
                target = 15 - target;
            } else if target > 0 {
                let day = day.rem_euclid(24000) as f32;
                let mut angle = super::sun_angle(day) * (std::f32::consts::PI / 180.0);
                let offset = if angle < std::f32::consts::PI {
                    0.0
                } else {
                    std::f32::consts::PI * 2.0
                };
                angle += (offset - angle) * 0.2;
                target = (target as f32 * angle.cos()).round() as i32;
            }
            target.clamp(0, 15)
        };
        // Полдень (6000): угол солнца 0, косинус 1 — полный сигнал.
        assert_eq!(power(15, false, 6000), 15);
        // Через сутки снова полдень — снова 15: угол замкнут на 360.
        assert_eq!(power(15, false, 30000), 15);
        // Полночь (18000): угол 180°, косинус −1 — датчик погас.
        assert_eq!(power(15, false, 18000), 0);
        // Инвертированный датчик угол не учитывает.
        assert_eq!(power(0, true, 18000), 15);
        assert_eq!(power(15, true, 6000), 0);
        // Без неба сигнала нет.
        assert_eq!(power(0, false, 6000), 0);
    }

    /// `TargetBlock.getRedstoneStrength`: центр грани даёт 15, край — 1.
    #[test]
    fn target_power_matches_vanilla() {
        // Попадание по оси Y (сверху): считаются X и Z.
        assert_eq!(dominant_axis([0.0, -1.0, 0.0]), 1);
        assert_eq!(dominant_axis([1.0, 0.0, 0.0]), 0);
        assert_eq!(dominant_axis([0.0, 0.0, -1.0]), 2);
        // По оси Y сильнее всех — берём её.
        assert_eq!(dominant_axis([0.3, -0.9, 0.2]), 1);

        let power = |pos: [f64; 3], axis: usize| {
            let frac = |v: f64| (v - v.floor() - 0.5).abs();
            let (dx, dy, dz) = (frac(pos[0]), frac(pos[1]), frac(pos[2]));
            // `TargetBlock.getRedstoneStrength`: по оси Y — `max(distX, distZ)`,
            // по Z — `max(distX, distY)`, по X — `max(distY, distZ)`.
            let dist = match axis {
                1 => dx.max(dz),
                2 => dx.max(dy),
                _ => dy.max(dz),
            };
            let s = ((0.5 - dist) / 0.5).clamp(0.0, 1.0);
            (15.0 * s).ceil().max(1.0) as i32
        };
        // Точно в центр грани: 15.
        assert_eq!(power([0.5, 4.5, 0.5], 1), 15);
        // Значения ниже — из формулы ванили на тех же double: `0.7 - 0.5`
        // не ровно 0.2, поэтому `ceil` уходит на шаг вверх.
        assert_eq!(power([0.6, 4.5, 0.5], 1), 12);
        assert_eq!(power([0.7, 4.5, 0.5], 1), 10);
        assert_eq!(power([0.8, 4.5, 0.5], 1), 6);
        assert_eq!(power([0.9, 4.5, 0.5], 1), 3);
        // На самом краю: минимум 1.
        assert_eq!(power([0.99, 4.5, 0.5], 1), 1);
        // Половина клетки от центра — это край грани, тоже 1.
        assert_eq!(power([1.0, 4.5, 0.5], 1), 1);
    }

    /// Изменение зеркала в тесте: имя поля и как его поменять.
    type LiveChange = Box<dyn Fn(&mut crate::live::Live)>;

    /// Кэш снимка обязан отдавать ровно то же, что полный рендер.
    ///
    /// Это и есть гарантия совместимости: плагин видит те же значения в те
    /// же моменты, меняется только то, что повторный рендер не делается.
    #[test]
    fn cached_state_matches_full_render() {
        use crate::live::Live;
        use crate::survival::Effect;
        let mut live = Live {
            health: 20.0,
            food: 20,
            saturation: 5.0,
            exhaustion: 0.0,
            game_mode: 0,
            selected: 0,
            slots: [None; crate::survival::SLOTS],
            pos: [1.5, 64.0, -2.25],
            yaw: 90.0,
            pitch: 0.0,
            effects: Vec::new(),
            xp: Default::default(),
            sneaking: false,
        };
        // Отпечаток меняется ровно тогда, когда меняется вывод.
        let text = |l: &Live| {
            l.render(
                |id| match id {
                    1 => Some("minecraft:stone"),
                    _ => None,
                },
                |_| Some("minecraft:haste"),
            )
        };
        let base = live.fingerprint();
        assert_eq!(base, live.fingerprint(), "отпечаток детерминирован");

        let mut changes: Vec<(&str, LiveChange)> = vec![
            ("health", Box::new(|l: &mut Live| l.health = 13.0)),
            ("food", Box::new(|l: &mut Live| l.food = 7)),
            ("pos", Box::new(|l: &mut Live| l.pos = [2.0, 65.0, 3.0])),
            ("yaw", Box::new(|l: &mut Live| l.yaw = 45.0)),
            ("mode", Box::new(|l: &mut Live| l.game_mode = 1)),
            ("selected", Box::new(|l: &mut Live| l.selected = 3)),
            (
                "slot",
                Box::new(|l: &mut Live| {
                    l.slots[36] = Some(chunkwake_protocol::inventory::Stack {
                        item: 1,
                        count: 5,
                        damage: 0,
                        extra: 0,
                    })
                }),
            ),
            (
                "effect",
                Box::new(|l: &mut Live| {
                    l.effects.push(Effect {
                        id: 2,
                        amplifier: 1,
                        duration: 200,
                        ambient: false,
                        particles: false,
                        show_icon: true,
                        age: 0,
                    })
                }),
            ),
        ];
        for (what, change) in changes.drain(..) {
            let before = text(&live);
            let old_fp = live.fingerprint();
            change(&mut live);
            let after = text(&live);
            assert_ne!(
                before, after,
                "{what}: снимок изменился, отпечаток обязан измениться"
            );
            assert_ne!(old_fp, live.fingerprint(), "{what}: отпечаток не заметил");
        }
    }

    #[test]
    fn player_snapshot_is_acknowledged_only_if_still_current() {
        let id = Uuid::nil();
        let old = PlayerState::default().save([1.0, 64.0, 1.0], 0.0, 0.0);
        let newer = PlayerState::default().save([2.0, 64.0, 1.0], 0.0, 0.0);
        let mut pending = std::collections::HashMap::from_iter([(id, old.clone())]);
        // A failed write never calls acknowledge; a later snapshot must survive
        // even if the earlier write eventually completes.
        assert_eq!(pending.get(&id), Some(&old));
        pending.insert(id, newer.clone());
        acknowledge_player_save(&mut pending, id, &old);
        assert_eq!(pending.get(&id), Some(&newer));
        acknowledge_player_save(&mut pending, id, &newer);
        assert!(!pending.contains_key(&id));
    }

    #[test]
    fn same_mod3_group_never_shares_a_face() {
        // Пролив пишет свой чанк и четыре его грани. У двух чанков одной
        // группы по mod 3 эти пятёрки не пересекаются, иначе параллельный
        // пролив затёр бы чужую запись.
        let group = |(x, z): (i32, i32)| (x.rem_euclid(3), z.rem_euclid(3));
        let cross = |(x, z): (i32, i32)| [(x, z), (x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)];
        let ring: Vec<(i32, i32)> = (-8..=8)
            .flat_map(|x| (-8..=8).map(move |z| (x, z)))
            .collect();
        for (i, &a) in ring.iter().enumerate() {
            for &b in &ring[i + 1..] {
                if group(a) != group(b) {
                    continue;
                }
                for p in cross(a) {
                    assert!(!cross(b).contains(&p), "{a:?} and {b:?} both write {p:?}");
                }
            }
        }
    }

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

    #[test]
    fn simulation_square_covers_exactly_the_radius() {
        let mut set = super::FastSet::default();
        super::chunk_square((0, 0), 1, &mut set);
        assert_eq!(set.len(), 9, "радиус 1 — это 3×3 чанка");
        assert!(set.contains(&(1, 1)));
        assert!(!set.contains(&(2, 0)));
        // Центр со сдвигом: квадрат едет вместе с ним, без далёких чанков.
        let mut set = super::FastSet::default();
        super::chunk_square((10, -4), 2, &mut set);
        assert_eq!(set.len(), 25);
        assert!(set.contains(&(12, -2)));
        assert!(
            set.contains(&(8, -4)),
            "ровно два чанка влево — ещё в радиусе"
        );
        assert!(!set.contains(&(7, -4)), "три чанка влево — уже за радиусом");
        assert!(!set.contains(&(13, -4)));
    }

    #[test]
    fn chunk_of_maps_negative_positions_like_the_server() {
        assert_eq!(super::chunk_of([0.5, 64.0, 0.5]), (0, 0));
        assert_eq!(super::chunk_of([-0.5, 64.0, -0.5]), (-1, -1));
        assert_eq!(super::chunk_of([-16.5, 64.0, 15.9]), (-2, 0));
    }

    #[test]
    fn elytra_flag_is_the_high_bit() {
        assert_eq!(super::player_flags(false, false, true), 0x80);
        assert_eq!(super::player_flags(true, true, true), 0x80 | 0x08 | 0x02);
        assert_eq!(super::player_flags(true, true, false) & 0x80, 0);
    }

    #[test]
    fn relative_coordinates_and_daytime() {
        use crate::play::commands::args;
        assert_eq!(args::Coord::parse("~1.5"), Some(args::Coord::Rel(1.5)));
        assert_eq!(args::Coord::parse("64"), Some(args::Coord::Abs(64.0)));
        assert_eq!(args::Coord::parse("^1"), Some(args::Coord::Local(1.0)));
        assert_eq!(args::Coord::parse("nope"), None);
        assert_eq!(args::daytime("noon"), Some(6000));
        assert_eq!(args::daytime("night"), Some(13000));
        assert_eq!(args::daytime("24000"), Some(0));
        assert_eq!(args::daytime("1000"), Some(1000));
        assert!(args::is_coord("~"));
        assert!(!args::is_coord("Steve"));
    }

    #[test]
    fn tick_stats_report_windows() {
        let mut ticks = chunkwake_metrics::TickStats::new();
        assert_eq!(ticks.line(), "TPS: 20.0 MSPT: 0.0 interval: 50.0 ms");
        ticks.record(5.0, 50.0);
        ticks.record(15.0, 50.0);
        assert_eq!(ticks.line(), "TPS: 20.0 MSPT: 10.0 interval: 50.0 ms");
        ticks.record(10.0, 100.0);
        assert_eq!(ticks.line(), "TPS: 15.0 MSPT: 10.0 interval: 100.0 ms");
    }

    /// Ускоренный цикл виден: раньше обрезка `min(20.0)` скрывала его.
    #[test]
    fn tick_stats_show_faster_than_twenty() {
        let mut ticks = chunkwake_metrics::TickStats::new();
        ticks.record(1.0, 25.0);
        assert_eq!(ticks.line(), "TPS: 40.0 MSPT: 1.0 interval: 25.0 ms");
    }

    #[test]
    fn parallel_chunk_map_matches_sequential() {
        let coords = chunks_around((5, -3), 2, None);
        let codec = chunkwake_protocol::frame::FrameCodec {
            threshold: Some(256),
        };
        let encode = |&(x, z): &(i32, i32)| {
            let mut packet = Vec::new();
            packet.extend(x.to_le_bytes());
            packet.extend(z.to_le_bytes());
            packet.extend(std::iter::repeat_n((x as u8).wrapping_add(z as u8), 3000));
            let mut out = Vec::new();
            codec.encode(&packet, &mut out);
            out
        };
        let seq: Vec<_> = coords.iter().map(encode).collect();
        let par = super::PlayPlan::map_limited(&coords, 4, encode);
        assert_eq!(par, seq);
    }

    #[test]
    fn reach_and_body_and_fall() {
        let eye = super::eye_of([0.5, 64.0, 0.5]);
        assert!(super::block_reach(eye, [0, 64, 0], 4.5));
        assert!(!super::block_reach(eye, [5, 64, 0], 4.5));
        assert!(super::block_reach(eye, [5, 64, 0], 5.0));
        assert!(super::player_hits_cube([0.5, 64.0, 0.5], [0, 65, 0]));
        assert!(!super::player_hits_cube([0.5, 64.0, 0.5], [2, 64, 0]));
        assert_eq!(super::fall_kept("minecraft:hay_block"), 0.2);
        assert_eq!(super::fall_kept("minecraft:slime_block"), 0.0);
        assert_eq!(super::fall_kept("minecraft:red_bed"), 0.0);
        assert_eq!(super::fall_kept("minecraft:stone"), 1.0);
        let spawn = [8.5, 64.0, 8.5];
        assert!(super::in_spawn(spawn, 16, [8, 64, 8]));
        assert!(super::in_spawn(spawn, 16, [23, 64, 8]));
        assert!(!super::in_spawn(spawn, 16, [24, 64, 8]));
        assert!(!super::in_spawn(spawn, 0, [8, 64, 8]));
    }
}
