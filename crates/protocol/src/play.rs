//! Пакеты состояния play (формат 26.3). ID передаются параметром.

use crate::configuration::write_nbt_string_root;
use crate::packets::ProfileProperty;
use crate::{Reader, Result, Writer};
use uuid::Uuid;

/// `CommonPlayerSpawnInfo` из login/respawn.
#[derive(Clone, Debug)]
pub struct SpawnInfo {
    /// Сетевой ID в реестре `dimension_type`.
    pub dimension_type: i32,
    /// Ключ измерения (`minecraft:overworld`).
    pub dimension: String,
    pub hashed_seed: i64,
    pub game_mode: u8,
    pub previous_game_mode: Option<u8>,
    pub is_debug: bool,
    pub is_flat: bool,
    pub portal_cooldown: i32,
    pub sea_level: i32,
}

#[derive(Clone, Debug)]
pub struct Login {
    pub entity_id: i32,
    pub hardcore: bool,
    pub dimensions: Vec<String>,
    pub max_players: i32,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub reduced_debug_info: bool,
    pub show_death_screen: bool,
    pub limited_crafting: bool,
    pub spawn: SpawnInfo,
    pub online_mode: bool,
    pub enforces_secure_chat: bool,
}

pub fn login(id: i32, l: &Login) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .i32(l.entity_id)
        .bool(l.hardcore)
        .var_int(l.dimensions.len() as i32);
    for d in &l.dimensions {
        w.string(d);
    }
    w.var_int(l.max_players)
        .var_int(l.view_distance)
        .var_int(l.simulation_distance)
        .bool(l.reduced_debug_info)
        .bool(l.show_death_screen)
        .bool(l.limited_crafting);
    write_spawn_info(&mut w, &l.spawn);
    w.bool(l.online_mode).bool(l.enforces_secure_chat);
    w.buf
}

/// `CommonPlayerSpawnInfo`: общий для `login` и `respawn`.
pub fn write_spawn_info(w: &mut Writer, s: &SpawnInfo) {
    w.var_int(s.dimension_type)
        .string(&s.dimension)
        .i64(s.hashed_seed)
        .var_int(s.game_mode as i32)
        // OPTIONAL_VAR_INT: 0 — нет значения, иначе значение + 1.
        .var_int(s.previous_game_mode.map_or(0, |g| g as i32 + 1))
        .bool(s.is_debug)
        .bool(s.is_flat)
        .bool(false) // last death location
        .var_int(s.portal_cooldown)
        .var_int(s.sea_level);
}

/// Коды `ClientboundGameEventPacket.Type`.
pub mod game_event {
    /// `NO_RESPAWN_BLOCK_AVAILABLE`: «кровать или якорь недоступны».
    pub const NO_RESPAWN_BLOCK_AVAILABLE: u8 = 0;
    pub const CHANGE_GAME_MODE: u8 = 3;
    /// `WIN_GAME`: 0 — сразу респаун, 1 — титры.
    pub const WIN_GAME: u8 = 4;
    pub const LEVEL_CHUNKS_LOAD_START: u8 = 13;
}

pub fn game_event(id: i32, event: u8, param: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).u8(event).f32(param);
    w.buf
}

pub fn set_chunk_cache_center(id: i32, x: i32, z: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(x).var_int(z);
    w.buf
}

/// `set_default_spawn_position`: измерение, `BlockPos` и угол взгляда спавна.
/// Без этого пакета Xaero не выбирает мир карты и не рисует тайлы.
pub fn set_default_spawn_position(
    id: i32,
    dimension: &str,
    pos: [i32; 3],
    yaw: f32,
    pitch: f32,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .string(dimension)
        .i64(pack_block_pos(pos))
        .f32(yaw)
        .f32(pitch);
    w.buf
}

/// Снимок границы для `initialize_border`.
#[derive(Clone, Copy, Debug)]
pub struct BorderState {
    pub center_x: f64,
    pub center_z: f64,
    /// Диаметр в начале анимации. Без анимации равен `new_size`.
    pub old_size: f64,
    pub new_size: f64,
    /// Оставшееся время сдвига, миллисекунды. 0 — граница уже стоит.
    pub lerp_millis: i64,
    pub warning_blocks: i32,
    pub warning_time: i32,
}

impl BorderState {
    /// Ванильный `WorldBorder`: центр в нуле, диаметр 59999968.
    pub fn vanilla() -> Self {
        const SIZE: f64 = 5.9999968e7;
        Self {
            center_x: 0.0,
            center_z: 0.0,
            old_size: SIZE,
            new_size: SIZE,
            lerp_millis: 0,
            warning_blocks: 5,
            warning_time: 15,
        }
    }
}

/// `initialize_border` с ванильными значениями `WorldBorder`.
/// Xaero по этому пакету понимает, что сервер без мода, и включает локальную карту.
pub fn initialize_border(id: i32) -> Vec<u8> {
    initialize_border_state(id, BorderState::vanilla())
}

/// `initialize_border` с текущей границей мира.
pub fn initialize_border_state(id: i32, border: BorderState) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .f64(border.center_x)
        .f64(border.center_z)
        .f64(border.old_size)
        .f64(border.new_size)
        .var_long(border.lerp_millis.max(0))
        .var_int(29999984)
        .var_int(border.warning_blocks)
        .var_int(border.warning_time);
    w.buf
}

/// `set_border_center`: новый центр границы.
pub fn set_border_center(id: i32, x: f64, z: f64) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).f64(x).f64(z);
    w.buf
}

/// `set_border_size`: диаметр без анимации.
pub fn set_border_size(id: i32, size: f64) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).f64(size);
    w.buf
}

/// `set_border_lerp_size`: старый и новый диаметр и время сдвига в миллисекундах.
pub fn set_border_lerp_size(id: i32, old: f64, new: f64, millis: i64) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).f64(old).f64(new).var_long(millis.max(0));
    w.buf
}

/// `set_border_warning_delay`: за сколько тиков экран краснеет у границы.
pub fn set_border_warning_delay(id: i32, ticks: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(ticks);
    w.buf
}

/// `set_border_warning_distance`: за сколько блоков до границы экран краснеет.
pub fn set_border_warning_distance(id: i32, blocks: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(blocks);
    w.buf
}

/// `set_camera`: чья сущность становится камерой (`/spectate`).
pub fn set_camera(id: i32, entity_id: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    w.buf
}

/// `swing_animation`: рука, вид взмаха (`none`/`whack`/`stab`) и длительность.
pub fn swing_animation(id: i32, entity_id: i32, hand: i32, kind: i32, duration: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .var_int(hand)
        .var_int(kind)
        .var_int(duration);
    w.buf
}

/// `boss_event` ADD: имя, заполнение 0..1, цвет, деление и флаги (небо, музыка, туман).
pub fn boss_event_add(
    id: i32,
    uuid: Uuid,
    name: &serde_json::Value,
    progress: f32,
    color: i32,
    overlay: i32,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).uuid(uuid).var_int(0);
    crate::text::write_component(&mut w, name);
    w.f32(progress.clamp(0.0, 1.0))
        .var_int(color)
        .var_int(overlay)
        .u8(0);
    w.buf
}

/// `boss_event` REMOVE.
pub fn boss_event_remove(id: i32, uuid: Uuid) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).uuid(uuid).var_int(1);
    w.buf
}

/// `boss_event` UPDATE_PROGRESS.
pub fn boss_event_progress(id: i32, uuid: Uuid, progress: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .uuid(uuid)
        .var_int(2)
        .f32(progress.clamp(0.0, 1.0));
    w.buf
}

/// `boss_event` UPDATE_NAME.
pub fn boss_event_name(id: i32, uuid: Uuid, name: &serde_json::Value) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).uuid(uuid).var_int(3);
    crate::text::write_component(&mut w, name);
    w.buf
}

/// `boss_event` UPDATE_STYLE: цвет и деление.
pub fn boss_event_style(id: i32, uuid: Uuid, color: i32, overlay: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .uuid(uuid)
        .var_int(4)
        .var_int(color)
        .var_int(overlay);
    w.buf
}

/// `post_effects`: полный список эффектов экрана игрока.
pub fn post_effects(id: i32, effects: &[String]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(effects.len() as i32);
    for effect in effects {
        w.string(effect);
    }
    w.buf
}

/// `set_passengers`: кто сидит на сущности (`/ride`).
pub fn set_passengers(id: i32, vehicle: i32, passengers: &[i32]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(vehicle)
        .var_int(passengers.len() as i32);
    for entity in passengers {
        w.var_int(*entity);
    }
    w.buf
}

/// `level_chunk_with_light`: координаты + заранее закодированное тело чанка.
pub fn level_chunk_with_light(id: i32, x: i32, z: i32, body: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).i32(x).i32(z);
    w.buf.extend_from_slice(body);
    w.buf
}

/// Позиция чанка упакована как в `ChunkPos::pack`: x в младших 32 битах, z в старших.
pub fn forget_level_chunk(id: i32, x: i32, z: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    w.i64(((z as i64) << 32) | (x as u32 as i64));
    w.buf
}

pub fn chunk_batch_start(id: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    w.buf
}

pub fn chunk_batch_finished(id: i32, batch_size: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(batch_size);
    w.buf
}

/// Абсолютный телепорт (`relatives` = 0), скорость нулевая.
pub fn player_position(id: i32, teleport_id: i32, pos: [f64; 3], yaw: f32, pitch: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(teleport_id);
    for v in pos {
        w.f64(v);
    }
    for _ in 0..3 {
        w.f64(0.0);
    }
    w.f32(yaw).f32(pitch).i32(0);
    w.buf
}

pub fn keep_alive(id: i32, payload: i64) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).i64(payload);
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Reader;

    #[test]
    fn player_position_layout() {
        let p = player_position(0x46, 1, [8.5, -60.0, 8.5], 90.0, 0.0);
        let mut r = Reader::new(&p);
        assert_eq!(r.var_int().unwrap(), 0x46);
        assert_eq!(r.var_int().unwrap(), 1);
        // 3 double позиции + 3 double скорости + 2 float + int relatives.
        assert_eq!(r.remaining().len(), 6 * 8 + 2 * 4 + 4);
    }

    #[test]
    fn optional_game_mode() {
        let mut l = Login {
            entity_id: 1,
            hardcore: false,
            dimensions: vec!["minecraft:overworld".into()],
            max_players: 20,
            view_distance: 8,
            simulation_distance: 8,
            reduced_debug_info: false,
            show_death_screen: true,
            limited_crafting: false,
            spawn: SpawnInfo {
                dimension_type: 0,
                dimension: "minecraft:overworld".into(),
                hashed_seed: 0,
                game_mode: 1,
                previous_game_mode: None,
                is_debug: false,
                is_flat: true,
                portal_cooldown: 0,
                sea_level: 63,
            },
            online_mode: false,
            enforces_secure_chat: false,
        };
        let none = login(0x30, &l);
        l.spawn.previous_game_mode = Some(0);
        let some = login(0x30, &l);
        assert_eq!(none.len(), some.len(), "both 1-byte VarInts");
        assert_ne!(none, some);
    }

    #[test]
    fn spawn_and_border_packets() {
        let pkt = set_default_spawn_position(7, "minecraft:overworld", [8, -60, 8], 0.0, 90.0);
        let mut r = Reader::new(&pkt);
        assert_eq!(r.var_int().unwrap(), 7);
        assert_eq!(r.string(256).unwrap(), "minecraft:overworld");
        assert_eq!(unpack_block_pos(r.i64().unwrap()), [8, -60, 8]);
        assert_eq!(r.f32().unwrap(), 0.0);
        assert_eq!(r.f32().unwrap(), 90.0);
        assert!(r.remaining().is_empty());

        let border = initialize_border(3);
        let mut r = Reader::new(&border);
        assert_eq!(r.var_int().unwrap(), 3);
        assert_eq!((r.f64().unwrap(), r.f64().unwrap()), (0.0, 0.0));
        assert_eq!(r.f64().unwrap(), 5.9999968e7);
        assert_eq!(r.f64().unwrap(), 5.9999968e7);
        assert_eq!(r.var_long().unwrap(), 0);
        assert_eq!(r.var_int().unwrap(), 29999984);
        assert_eq!(r.var_int().unwrap(), 5);
        assert_eq!(r.var_int().unwrap(), 15);
        assert!(r.remaining().is_empty());
    }

    #[test]
    fn border_swing_and_passengers_packets() {
        let center = set_border_center(1, 4.0, -2.0);
        let mut r = Reader::new(&center);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!((r.f64().unwrap(), r.f64().unwrap()), (4.0, -2.0));
        assert!(r.remaining().is_empty());

        let size = set_border_size(2, 32.0);
        let mut r = Reader::new(&size);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.f64().unwrap(), 32.0);

        let lerp = set_border_lerp_size(3, 32.0, 64.0, 1000);
        let mut r = Reader::new(&lerp);
        assert_eq!(r.var_int().unwrap(), 3);
        assert_eq!(r.f64().unwrap(), 32.0);
        assert_eq!(r.f64().unwrap(), 64.0);
        assert_eq!(r.var_long().unwrap(), 1000);

        let swing = swing_animation(4, 9, 1, 2, 6);
        let mut r = Reader::new(&swing);
        assert_eq!(r.var_int().unwrap(), 4);
        assert_eq!(r.var_int().unwrap(), 9);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.var_int().unwrap(), 6);
        assert!(r.remaining().is_empty());

        let ride = set_passengers(5, 3, &[8, 9]);
        let mut r = Reader::new(&ride);
        assert_eq!(r.var_int().unwrap(), 5);
        assert_eq!(r.var_int().unwrap(), 3);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.var_int().unwrap(), 8);
        assert_eq!(r.var_int().unwrap(), 9);
        assert!(r.remaining().is_empty());
    }

    #[test]
    fn boss_and_post_effect_packets() {
        let uuid = Uuid::from_u128(7);
        let gone = boss_event_remove(9, uuid);
        let mut r = Reader::new(&gone);
        assert_eq!(r.var_int().unwrap(), 9);
        assert_eq!(r.uuid().unwrap(), uuid);
        assert_eq!(r.var_int().unwrap(), 1);
        assert!(r.remaining().is_empty());

        let progress = boss_event_progress(9, uuid, 0.25);
        let mut r = Reader::new(&progress);
        assert_eq!(r.var_int().unwrap(), 9);
        assert_eq!(r.uuid().unwrap(), uuid);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.f32().unwrap(), 0.25);
        assert!(r.remaining().is_empty());

        let effects = post_effects(4, &["minecraft:blur".into(), "minecraft:creeper".into()]);
        let mut r = Reader::new(&effects);
        assert_eq!(r.var_int().unwrap(), 4);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.string(64).unwrap(), "minecraft:blur");
        assert_eq!(r.string(64).unwrap(), "minecraft:creeper");
        assert!(r.remaining().is_empty());

        let biomes = chunks_biomes(13, &[(0, 0, &[1, 2, 3])]);
        let mut r = Reader::new(&biomes);
        assert_eq!(r.var_int().unwrap(), 13);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.i64().unwrap(), 0);
        assert_eq!(r.var_int().unwrap(), 3);
        assert_eq!(r.remaining(), &[1, 2, 3]);

        let survival = player_abilities(1, 0, false);
        assert_eq!(survival[1], 0);
        let allowed = player_abilities(1, 0, true);
        assert_eq!(allowed[1], 0x04);

        let point = waypoint_position(141, 0, uuid, "minecraft:default", Some(0xFF5555), [1, 2, 3]);
        let mut r = Reader::new(&point);
        assert_eq!(r.var_int().unwrap(), 141);
        assert_eq!(r.var_int().unwrap(), 0);
        assert!(r.bool().unwrap());
        assert_eq!(r.uuid().unwrap(), uuid);
        assert_eq!(r.string(64).unwrap(), "minecraft:default");
        assert!(r.bool().unwrap());
        assert_eq!(r.u8().unwrap(), 0xFF);
        assert_eq!(r.u8().unwrap(), 0x55);
        assert_eq!(r.u8().unwrap(), 0x55);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.var_int().unwrap(), 3);
        assert!(r.remaining().is_empty());

        let clear = clear_dialog(142);
        let mut r = Reader::new(&clear);
        assert_eq!(r.var_int().unwrap(), 142);
        assert!(r.remaining().is_empty());

        let flame = level_particles(48, 16, true, [1.0, 2.0, 3.0], [0.1, 0.0, 0.0], 0.5, 4);
        let mut r = Reader::new(&flame);
        assert_eq!(r.var_int().unwrap(), 48);
        assert_eq!(r.var_int().unwrap(), 16);
        assert!(r.bool().unwrap());
        assert!(!r.bool().unwrap());
        assert_eq!(r.f64().unwrap(), 1.0);
        assert_eq!(r.f64().unwrap(), 2.0);
        assert_eq!(r.f64().unwrap(), 3.0);
        assert_eq!(r.f32().unwrap(), 0.1);
        assert_eq!(r.f32().unwrap(), 0.0);
        assert_eq!(r.f32().unwrap(), 0.0);
        assert_eq!(r.f32().unwrap(), 0.5);
        assert_eq!(r.f32().unwrap(), 0.5);
        assert_eq!(r.f32().unwrap(), 0.5);
        assert_eq!(r.var_int().unwrap(), 4);
        assert_eq!(r.var_int().unwrap(), 0);
        assert!(r.remaining().is_empty());

        // `dust`: тело из цвета ARGB и масштаба идёт сразу после id типа.
        let body = dust_body(0xFF11_2233u32 as i32, 1.5);
        let dust = level_particles_with(48, 14, Some(&body), false, [0.0; 3], [0.0; 3], 0.0, 1);
        let mut r = Reader::new(&dust);
        assert_eq!(r.var_int().unwrap(), 48);
        assert_eq!(r.var_int().unwrap(), 14);
        // Цвет ARGB — четыре байта: старшие и младшие половины.
        let high = r.u16().unwrap() as u32;
        let low = r.u16().unwrap() as u32;
        assert_eq!((high << 16) | low, 0xFF11_2233u32, "цвет ARGB в теле dust");
        assert_eq!(r.f32().unwrap(), 1.5);
        // Дальше: force, alwaysShow, позиция (3×f64), разброс (3×f32), три скорости,
        // количество и тип рандомизации.
        assert!(!r.bool().unwrap());
        assert!(!r.bool().unwrap());
        for _ in 0..3 {
            assert_eq!(r.f64().unwrap(), 0.0);
        }
        for _ in 0..6 {
            assert_eq!(r.f32().unwrap(), 0.0);
        }
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 0);
        assert!(r.remaining().is_empty());
    }
}

/// `chunks_biomes`: позиции чанков (`ChunkPos.pack`) и контейнеры биомов секций.
pub fn chunks_biomes(id: i32, chunks: &[(i32, i32, &[u8])]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(chunks.len() as i32);
    for &(x, z, data) in chunks {
        let packed = (x as u32 as u64) | ((z as u32 as u64) << 32);
        w.i64(packed as i64).byte_array(data);
    }
    w.buf
}

/// `system_chat`: текстовый компонент (NBT-строка) и флаг action bar.
pub fn system_chat(id: i32, text: &str, overlay: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    write_nbt_string_root(&mut w, text);
    w.bool(overlay);
    w.buf
}

/// Запись таб-листа для `player_info_update`.
#[derive(Clone, Debug)]
pub struct PlayerEntry {
    pub uuid: Uuid,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
    pub game_mode: i32,
    pub latency_ms: i32,
    /// `isModelPartShown(HAT)`: второй слой головы в таб-листе.
    pub show_hat: bool,
}

/// Действия `ClientboundPlayerInfoUpdatePacket$Action` (порядковые номера в EnumSet).
const ADD_PLAYER: u8 = 1 << 0;
const UPDATE_GAME_MODE: u8 = 1 << 2;
const UPDATE_LISTED: u8 = 1 << 3;
const UPDATE_LATENCY: u8 = 1 << 4;

/// `player_info_update` с добавлением игроков в таб-лист.
///
/// Как `createPlayerInitializing`, кроме чата, имени в списке и порядка:
/// они у сервера пока не меняются и совпадают с умолчаниями клиента.
pub fn player_info_add(id: i32, players: &[PlayerEntry]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    // EnumSet из 8 действий — фиксированный битсет в 1 байт.
    w.u8(ADD_PLAYER | UPDATE_GAME_MODE | UPDATE_LISTED | UPDATE_LATENCY | UPDATE_HAT);
    w.var_int(players.len() as i32);
    for p in players {
        w.uuid(p.uuid).string(&p.name);
        w.var_int(p.properties.len() as i32);
        for prop in &p.properties {
            w.string(&prop.name)
                .string(&prop.value)
                .bool(prop.signature.is_some());
            if let Some(sig) = &prop.signature {
                w.string(sig);
            }
        }
        w.var_int(p.game_mode)
            .bool(true)
            .var_int(p.latency_ms)
            .bool(p.show_hat);
    }
    w.buf
}

/// `UPDATE_HAT` — последнее из восьми действий.
const UPDATE_HAT: u8 = 1 << 7;

/// `player_info_update` с флагом шляпы (`ServerboundClientInformation` сменил
/// `PlayerModelPart.HAT`): таб-лист рисует голову со вторым слоем или без.
pub fn player_info_hat(id: i32, uuid: Uuid, shown: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .u8(UPDATE_HAT)
        .var_int(1)
        .uuid(uuid)
        .bool(shown);
    w.buf
}

/// `player_info_update` только с режимом игры (таб-лист).
pub fn player_info_game_mode(id: i32, uuid: Uuid, game_mode: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .u8(UPDATE_GAME_MODE)
        .var_int(1)
        .uuid(uuid)
        .var_int(game_mode);
    w.buf
}

/// `waypoint`: операция (`TRACK` 0, `UNTRACK` 1, `UPDATE` 2) и точка с позицией.
///
/// Иконка — идентификатор стиля и необязательный цвет RGB. Позиция — три varint.
pub fn waypoint_position(
    id: i32,
    operation: i32,
    uuid: Uuid,
    style: &str,
    color: Option<i32>,
    pos: [i32; 3],
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(operation)
        .bool(true)
        .uuid(uuid)
        .string(style);
    match color {
        Some(rgb) => {
            w.bool(true)
                .u8(((rgb >> 16) & 0xff) as u8)
                .u8(((rgb >> 8) & 0xff) as u8)
                .u8((rgb & 0xff) as u8);
        }
        None => {
            w.bool(false);
        }
    }
    w.var_int(1).var_int(pos[0]).var_int(pos[1]).var_int(pos[2]);
    w.buf
}

/// `clear_dialog`: пустой пакет, клиент закрывает открытый диалог.
pub fn clear_dialog(id: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    w.buf
}

/// `level_particles` для простой частицы: id типа, затем позиция и разброс.
///
/// Три скорости — одно значение, как у короткого конструктора пакета.
/// `randomization` 0 — `DEFAULT`.
pub fn level_particles(
    id: i32,
    particle: i32,
    force: bool,
    pos: [f64; 3],
    delta: [f32; 3],
    speed: f32,
    count: i32,
) -> Vec<u8> {
    level_particles_with(id, particle, None, force, pos, delta, speed, count)
}

/// `level_particles` с телом частицы: у `dust` это цвет ARGB и масштаб.
///
/// Порядок полей тела — порядок `STREAM_CODEC` самого варианта: id типа
/// пишется кодеком реестра, дальше идут поля опций.
#[allow(clippy::too_many_arguments)]
pub fn level_particles_with(
    id: i32,
    particle: i32,
    body: Option<&[u8]>,
    force: bool,
    pos: [f64; 3],
    delta: [f32; 3],
    speed: f32,
    count: i32,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(particle);
    if let Some(body) = body {
        w.buf.extend_from_slice(body);
    }
    w.bool(force)
        .bool(false)
        .f64(pos[0])
        .f64(pos[1])
        .f64(pos[2])
        .f32(delta[0])
        .f32(delta[1])
        .f32(delta[2])
        .f32(speed)
        .f32(speed)
        .f32(speed)
        .var_int(count)
        .var_int(0);
    w.buf
}

/// Тело `TrailParticleOption`: точка, к которой летит след, цвет RGB и
/// длительность в тиках.
pub fn trail_body(target: [f64; 3], color: i32, duration: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.f64(target[0])
        .f64(target[1])
        .f64(target[2])
        .i32(color)
        .var_int(duration);
    w.buf
}

/// Тело `DustParticleOptions`: цвет ARGB и масштаб.
pub fn dust_body(argb: i32, scale: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.i32(argb).f32(scale);
    w.buf
}

/// `player_abilities` по режиму игры (`GameType.updatePlayerAbilities`):
/// неуязвимость, полёт, мгновенная ломка; скорости полёта и ходьбы ванилы.
///
/// `allow_flight` включает бит may-fly и в выживании с приключением.
pub fn player_abilities(id: i32, game_mode: u8, allow_flight: bool) -> Vec<u8> {
    let mut flags: u8 = match game_mode {
        1 => 0x01 | 0x04 | 0x08,
        3 => 0x01 | 0x02 | 0x04,
        _ => 0,
    };
    if allow_flight {
        flags |= 0x04;
    }
    let mut w = Writer::new();
    w.var_int(id).u8(flags).f32(0.05).f32(0.1);
    w.buf
}

pub fn player_info_remove(id: i32, uuids: &[Uuid]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(uuids.len() as i32);
    for &u in uuids {
        w.uuid(u);
    }
    w.buf
}

/// Текст из `ServerboundChatPacket` (поле после ID; подпись и прочее игнорируем).
pub fn read_chat_message<'a>(r: &mut Reader<'a>) -> Result<&'a str> {
    r.string(256)
}

#[cfg(test)]
mod chat_tests {
    use super::*;

    #[test]
    fn player_info_layout() {
        let u = Uuid::from_u128(7);
        let p = player_info_add(
            0x40,
            &[PlayerEntry {
                uuid: u,
                name: "Steve".into(),
                properties: vec![],
                game_mode: 1,
                latency_ms: 5,
                show_hat: true,
            }],
        );
        let mut r = Reader::new(&p);
        assert_eq!(r.var_int().unwrap(), 0x40);
        assert_eq!(r.u8().unwrap(), 0b1001_1101);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.uuid().unwrap(), u);
        assert_eq!(r.string(16).unwrap(), "Steve");
        assert_eq!(r.var_int().unwrap(), 0);
        assert_eq!(
            (
                r.var_int().unwrap(),
                r.bool().unwrap(),
                r.var_int().unwrap()
            ),
            (1, true, 5)
        );
        assert!(r.bool().unwrap());
        assert!(r.remaining().is_empty());
    }
}

/// Угол в градусах → байт (1/256 оборота), как `Mth.packDegrees`.
pub fn angle(deg: f32) -> u8 {
    (deg * 256.0 / 360.0).floor() as i32 as u8
}

/// `add_entity` без начальной скорости (LpVec3 нулевого вектора — один байт 0).
#[allow(clippy::too_many_arguments)]
pub fn add_entity(
    id: i32,
    entity_id: i32,
    uuid: Uuid,
    entity_type: i32,
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
    data: i32,
) -> Vec<u8> {
    add_entity_moving(
        id,
        entity_id,
        uuid,
        entity_type,
        pos,
        [0.0; 3],
        yaw,
        pitch,
        data,
    )
}

/// `add_entity` с начальной скоростью (блоков за тик).
#[allow(clippy::too_many_arguments)]
pub fn add_entity_moving(
    id: i32,
    entity_id: i32,
    uuid: Uuid,
    entity_type: i32,
    pos: [f64; 3],
    movement: [f64; 3],
    yaw: f32,
    pitch: f32,
    data: i32,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .uuid(uuid)
        .var_int(entity_type);
    for v in pos {
        w.f64(v);
    }
    lp_vec3(&mut w, movement);
    w.u8(angle(pitch))
        .u8(angle(yaw))
        .u8(angle(yaw)) // голова
        .var_int(data);
    w.buf
}

/// `LpVec3`: вектор скорости, 15 бит на ось с общим масштабом
/// (`net.minecraft.network.LpVec3.write`).
pub fn lp_vec3(w: &mut Writer, v: [f64; 3]) {
    let v = v.map(|c| {
        if c.is_nan() {
            0.0
        } else {
            c.clamp(-1.7179869183e10, 1.7179869183e10)
        }
    });
    let max = v[0].abs().max(v[1].abs()).max(v[2].abs());
    if max < 3.051944088384301e-5 {
        w.u8(0);
        return;
    }
    let scale = max.ceil() as i64;
    let more = scale & 3 != scale;
    let markers = if more { scale & 3 | 4 } else { scale };
    let pack = |c: f64| ((c / scale as f64 * 0.5 + 0.5) * 32766.0).round() as i64;
    let packed = markers | pack(v[0]) << 3 | pack(v[1]) << 18 | pack(v[2]) << 33;
    w.u8(packed as u8)
        .u8((packed >> 8) as u8)
        .i32((packed >> 16) as u32 as i32);
    if more {
        w.var_int((scale >> 2) as i32);
    }
}

/// `set_entity_motion`: скорость сущности (блоков за тик).
pub fn set_entity_motion(id: i32, entity_id: i32, movement: [f64; 3]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    lp_vec3(&mut w, movement);
    w.buf
}

/// `teleport_entity`: абсолютная позиция (PositionMoveRotation), без относительных флагов.
pub fn teleport_entity(
    id: i32,
    entity_id: i32,
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
    on_ground: bool,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    for v in pos {
        w.f64(v);
    }
    for _ in 0..3 {
        w.f64(0.0);
    }
    w.f32(yaw).f32(pitch).i32(0).bool(on_ground);
    w.buf
}

/// `ServerboundMoveVehiclePacket`: позиция, поворот и «на земле».
pub fn read_move_vehicle(r: &mut Reader) -> Result<([f64; 3], f32, f32, bool)> {
    Ok((
        [r.f64()?, r.f64()?, r.f64()?],
        r.f32()?,
        r.f32()?,
        r.bool()?,
    ))
}

pub fn rotate_head(id: i32, entity_id: i32, yaw: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id).u8(angle(yaw));
    w.buf
}

pub fn remove_entities(id: i32, entity_ids: &[i32]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_ids.len() as i32);
    for &e in entity_ids {
        w.var_int(e);
    }
    w.buf
}

/// Поля `ServerboundMovePlayerPacket` после ID: что есть в пакете, то `Some`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Move {
    pub pos: Option<[f64; 3]>,
    pub rot: Option<(f32, f32)>,
    pub on_ground: bool,
}

pub fn read_move(r: &mut Reader, has_pos: bool, has_rot: bool) -> Result<Move> {
    let pos = if has_pos {
        Some([r.f64()?, r.f64()?, r.f64()?])
    } else {
        None
    };
    let rot = if has_rot {
        Some((r.f32()?, r.f32()?))
    } else {
        None
    };
    Ok(Move {
        pos,
        rot,
        on_ground: r.u8()? & 1 != 0,
    })
}

/// `move_entity_pos_rot` с линейной дельтой (шагов 0): смещение в 1/4096 блока.
pub fn move_entity_pos_rot(
    id: i32,
    entity_id: i32,
    delta: [i16; 3],
    yaw: f32,
    pitch: f32,
    on_ground: bool,
) -> Vec<u8> {
    let mut w = Writer::new();
    // Свойства: бит 0 — onGround, остальные — число шагов (0 = линейная дельта).
    w.var_int(id).var_int(entity_id).var_int(on_ground as i32);
    for d in delta {
        w.u16(d as u16);
    }
    w.u8(angle(yaw)).u8(angle(pitch));
    w.buf
}

/// Позиция в единицах дельты (`VecDeltaCodec.encode`: round(v * 4096)).
pub fn quantize(pos: [f64; 3]) -> [i64; 3] {
    pos.map(|v| (v * 4096.0).round() as i64)
}

/// Дельта между квантованными позициями, если каждая компонента влезает в short.
pub fn delta(from: [i64; 3], to: [i64; 3]) -> Option<[i16; 3]> {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    Some([
        i16::try_from(d[0]).ok()?,
        i16::try_from(d[1]).ok()?,
        i16::try_from(d[2]).ok()?,
    ])
}

#[cfg(test)]
mod move_tests {
    use super::*;

    #[test]
    fn deltas() {
        let a = quantize([1.0, 2.0, 3.0]);
        assert_eq!(delta(a, quantize([1.5, 2.0, 2.0])), Some([2048, 0, -4096]));
        assert_eq!(
            delta(a, quantize([9.0, 2.0, 3.0])),
            None,
            "8 блоков не влезает"
        );
        let p = move_entity_pos_rot(0x33, 7, [1, -1, 0], 90.0, 0.0, true);
        assert_eq!(p, [0x33, 7, 1, 0, 1, 0xff, 0xff, 0, 0, 64, 0]);
    }
}

/// `BlockPos.asLong`: x — 26 бит (старшие), z — 26, y — 12 (младшие).
pub fn pack_block_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ff_ffff) << 38) | ((z as i64 & 0x3ff_ffff) << 12) | (y as i64 & 0xfff)
}

pub fn unpack_block_pos(v: i64) -> [i32; 3] {
    [
        (v >> 38) as i32,
        (v << 52 >> 52) as i32,
        (v << 26 >> 38) as i32,
    ]
}

pub fn block_update(id: i32, pos: [i32; 3], state: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).i64(pack_block_pos(pos)).var_int(state);
    w.buf
}

/// `block_entity_data`: позиция, тип блок-сущности и сетевой NBT (без имени).
pub fn block_entity_data(id: i32, pos: [i32; 3], kind: i32, nbt: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).i64(pack_block_pos(pos)).var_int(kind);
    w.buf.extend_from_slice(nbt);
    w.buf
}

/// `ClientboundOpenSignEditorPacket`: редактор таблички у клиента (`is_front_text`).
pub fn open_sign_editor(id: i32, pos: [i32; 3], front: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).i64(pack_block_pos(pos)).bool(front);
    w.buf
}

/// `ClientboundTransferPacket`: клиент переподключается к `host:port`.
pub fn transfer(id: i32, host: &str, port: u16) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).string(host).var_int(i32::from(port));
    w.buf
}

/// `ClientboundSoundPacket`: звук из реестра в точке (координаты × 8), с зерном.
pub fn sound(
    id: i32,
    sound: i32,
    source: i32,
    pos: [f64; 3],
    volume: f32,
    pitch: f32,
    seed: i64,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(sound + 1)
        .var_int(source)
        .i32((pos[0] * 8.0) as i32)
        .i32((pos[1] * 8.0) as i32)
        .i32((pos[2] * 8.0) as i32)
        .f32(volume)
        .f32(pitch)
        .i64(seed);
    w.buf
}

/// `ClientboundLevelEventPacket`: событие мира (2001 — ломка блока с частицами и звуком).
pub fn level_event(id: i32, event: i32, pos: [i32; 3], data: i32, global: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .i32(event)
        .i64(pack_block_pos(pos))
        .i32(data)
        .bool(global);
    w.buf
}

/// `block_event`: крышка сундука (`action` 1, `param` — сколько игроков смотрят).
/// `block` — ID в реестре `block`, не состояние.
pub fn block_event(id: i32, pos: [i32; 3], action: u8, param: u8, block: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .i64(pack_block_pos(pos))
        .u8(action)
        .u8(param)
        .var_int(block);
    w.buf
}

/// `ClientboundSetTitleTextPacket` / `set_subtitle_text` / `set_action_bar_text`:
/// текстовый компонент без дополнительных полей.
pub fn plain_component_packet(id: i32, text: &serde_json::Value) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    crate::text::write_component(&mut w, text);
    w.buf
}

/// `ClientboundSetTitlesAnimationPacket`: тики появления, пребывания, исчезновения.
pub fn titles_animation(id: i32, fade_in: i32, stay: i32, fade_out: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).i32(fade_in).i32(stay).i32(fade_out);
    w.buf
}

/// `ClientboundClearTitlesPacket`: сбросить заголовок (`reset` — сразу).
pub fn clear_titles(id: i32, reset: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).bool(reset);
    w.buf
}

/// `ClientboundStopSoundPacket`: остановить звук (`sound` — `None` значит все).
///
/// Источник `Some(0)` — категория `master`. `None` — категория не задана.
pub fn stop_sound(id: i32, sound: Option<i32>, source: Option<i32>) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    // Флаги: 1 — есть источник, 2 — есть звук.
    let mut flags = 0u8;
    if source.is_some() {
        flags |= 1;
    }
    if sound.is_some() {
        flags |= 2;
    }
    w.u8(flags);
    if let Some(source) = source {
        w.var_int(source);
    }
    if let Some(sound) = sound {
        // Реестровые ID звуков сдвинуты на 1: 0 у ванилы значит «нет звука».
        w.var_int(sound + 1);
    }
    w.buf
}

/// Формат числа на счёте: пусто, стиль или фиксированный текст.
#[derive(Clone, Debug, PartialEq)]
pub enum NumberFormat {
    Blank,
    Styled(serde_json::Value),
    Fixed(serde_json::Value),
}

/// `ClientboundSetObjectivePacket`: `mode` 0 — создать, 1 — убрать, 2 — обновить.
pub fn set_objective(
    id: i32,
    name: &str,
    mode: u8,
    display_name: &serde_json::Value,
    render_type: i32,
    format: Option<&NumberFormat>,
    has_display_name: bool,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).string(name).u8(mode);
    if mode != 1 {
        crate::text::write_component(&mut w, display_name);
        w.var_int(render_type);
        // OPTIONAL_NUMBER_FORMAT: 0 — нет формата. Тело идёт сразу за флагом.
        match format {
            Some(format) => {
                w.bool(true);
                write_number_format(&mut w, format);
            }
            None => {
                w.bool(false);
            }
        }
    }
    w.bool(!has_display_name || mode != 1);
    w.buf
}

fn write_number_format(w: &mut Writer, format: &NumberFormat) {
    match format {
        NumberFormat::Blank => {
            w.var_int(0);
        }
        NumberFormat::Styled(value) => {
            w.var_int(1);
            crate::text::write_component(w, value);
        }
        NumberFormat::Fixed(value) => {
            w.var_int(2);
            crate::text::write_component(w, value);
        }
    }
}

/// `ClientboundSetScorePacket`: значение счёта держателя.
pub fn set_score(
    id: i32,
    holder: &str,
    objective: &str,
    value: i32,
    display_name: Option<&serde_json::Value>,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).string(holder).var_int(value);
    match display_name {
        Some(name) => {
            w.bool(true);
            crate::text::write_component(&mut w, name);
        }
        None => {
            w.bool(false);
        }
    }
    w.string(objective);
    w.buf
}

/// `ClientboundResetScorePacket`: снять счёт (или всю цель держателя).
pub fn reset_score(id: i32, holder: &str, objective: Option<&str>) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).string(holder);
    match objective {
        Some(objective) => {
            w.bool(true);
            w.string(objective);
        }
        None => {
            w.bool(false);
        }
    }
    w.buf
}

/// `ClientboundSetDisplayObjectivePacket`: цель в слоте (`sidebar`, `list`, `below_name`).
pub fn set_display_objective(id: i32, slot: &str, objective: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).string(slot).string(objective);
    w.buf
}

/// `set_time` 26.3: возраст мира и одна запись часов верхнего мира.
/// Часы — `minecraft:overworld` (сетевой id 0, записи реестра отсортированы).
/// Состояние часов: VarLong тиков, доля тика, скорость (`0` — солнце стоит между пакетами).
pub fn set_time(id: i32, game_time: i64, day_time: i64, ticking: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .i64(game_time)
        .var_int(1)
        .var_int(0)
        .var_long(day_time)
        .f32(0.0)
        .f32(if ticking { 1.0 } else { 0.0 });
    w.buf
}

/// `change_difficulty`: байт сложности (0 мирный … 3 сложный) и флаг блокировки.
pub fn change_difficulty(id: i32, difficulty: u8) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).u8(difficulty).bool(false);
    w.buf
}

/// `ClientboundBlockDestructionPacket`: трещины на блоке; стадия вне 0..=9 убирает их.
pub fn block_destruction(id: i32, entity: i32, pos: [i32; 3], stage: i8) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity)
        .i64(pack_block_pos(pos))
        .u8(stage as u8);
    w.buf
}

/// Несколько блоков одной секции: `SectionPos.asLong` (x, z по 22 бита, y 20),
/// затем VarLong `state << 12 | x << 8 | z << 4 | y` (локальные координаты).
pub fn section_blocks_update(id: i32, section: [i32; 3], blocks: &[(u16, u32)]) -> Vec<u8> {
    let [x, y, z] = section;
    let pos =
        ((x as i64 & 0x3f_ffff) << 42) | ((z as i64 & 0x3f_ffff) << 20) | (y as i64 & 0xf_ffff);
    let mut w = Writer::new();
    w.var_int(id).i64(pos).var_int(blocks.len() as i32);
    for &(local, state) in blocks {
        w.var_long(((state as i64) << 12) | local as i64);
    }
    w.buf
}

pub fn block_changed_ack(id: i32, sequence: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(sequence);
    w.buf
}

/// `Direction.get3DDataValue` → смещение соседнего блока.
pub fn direction_offset(d: i32) -> Option<[i32; 3]> {
    Some(match d {
        0 => [0, -1, 0],
        1 => [0, 1, 0],
        2 => [0, 0, -1],
        3 => [0, 0, 1],
        4 => [-1, 0, 0],
        5 => [1, 0, 0],
        _ => return None,
    })
}

/// `ServerboundPlayerActionPacket` после ID.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerAction {
    /// Код `Action` (см. константы ниже).
    pub action: i32,
    pub pos: [i32; 3],
    pub face: u8,
    pub sequence: i32,
}

/// `ServerboundPlayerActionPacket.Action`.
pub const START_DESTROY_BLOCK: i32 = 0;
pub const ABORT_DESTROY_BLOCK: i32 = 2;
pub const STOP_DESTROY_BLOCK: i32 = 3;
pub const DROP_ALL_ITEMS: i32 = 4;
pub const DROP_ITEM: i32 = 5;
pub const RELEASE_USE_ITEM: i32 = 6;
pub const SWAP_ITEM_WITH_OFFHAND: i32 = 7;
/// `STAB`: удар копьём (`PiercingWeapon.attack`).
pub const STAB: i32 = 8;

/// `ServerboundPlayerCommandPacket.Action` (26.3, после ID сущности).
/// 0 — встать с кровати, 3/4 — прыжок на скакуне, 5 — инвентарь скакуна.
/// Присед приходит в `player_input`, не здесь.
pub const STOP_SLEEPING: i32 = 0;
pub const START_SPRINTING: i32 = 1;
pub const STOP_SPRINTING: i32 = 2;
/// Клиент уже включил бит «полёт на элитрах» локально и ждёт, что сервер его погасит.
pub const START_FALL_FLYING: i32 = 6;

pub fn read_player_action(r: &mut Reader) -> Result<PlayerAction> {
    Ok(PlayerAction {
        action: r.var_int()?,
        pos: unpack_block_pos(r.i64()?),
        face: r.u8()?,
        sequence: r.var_int()?,
    })
}

/// `ServerboundUseItemOnPacket` после ID: рука, BlockHitResult, sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UseItemOn {
    pub hand: i32,
    pub pos: [i32; 3],
    pub face: i32,
    /// Точка попадания внутри блока.
    pub cursor: [f32; 3],
    pub sequence: i32,
}

pub fn read_use_item_on(r: &mut Reader) -> Result<UseItemOn> {
    let hand = r.var_int()?;
    let pos = unpack_block_pos(r.i64()?);
    let face = r.var_int()?;
    let cursor = [r.f32()?, r.f32()?, r.f32()?];
    // inside, worldBorderHit.
    r.bytes(2)?;
    Ok(UseItemOn {
        hand,
        pos,
        face,
        cursor,
        sequence: r.var_int()?,
    })
}

/// `ServerboundSignUpdatePacket` после ID: позиция, сторона и четыре строки
/// (каждая до 384 символов).
#[derive(Clone, Debug, PartialEq)]
pub struct SignUpdate {
    pub pos: [i32; 3],
    pub front: bool,
    pub lines: [String; 4],
}

pub fn read_sign_update(r: &mut Reader) -> Result<SignUpdate> {
    let pos = unpack_block_pos(r.i64()?);
    let front = r.bool()?;
    let mut lines: [String; 4] = Default::default();
    for l in &mut lines {
        *l = r.string(384)?.to_owned();
    }
    Ok(SignUpdate { pos, front, lines })
}

/// `ServerboundEditBookPacket` после ID: слот инвентаря (0–8 или 40),
/// до 100 страниц по 1024 символа и название до 32 — если книгу подписывают.
#[derive(Clone, Debug, PartialEq)]
pub struct EditBook {
    pub slot: i32,
    pub pages: Vec<String>,
    pub title: Option<String>,
}

pub fn read_edit_book(r: &mut Reader) -> Result<EditBook> {
    let slot = r.var_int()?;
    let n = r.var_int()?;
    if !(0..=100).contains(&n) {
        return Err(crate::ProtocolError::BadLength(n));
    }
    let mut pages = Vec::with_capacity(n as usize);
    for _ in 0..n {
        pages.push(r.string(1024)?.to_owned());
    }
    let title = if r.bool()? {
        Some(r.string(32)?.to_owned())
    } else {
        None
    };
    Ok(EditBook { slot, pages, title })
}

/// `ClientboundOpenBookPacket`: открыть подписанную книгу в руке (0 — основная).
pub fn open_book(id: i32, hand: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(hand);
    w.buf
}

#[cfg(test)]
mod block_tests {
    use super::*;

    #[test]
    fn edit_book_reads_pages_and_title() {
        let mut w = Writer::new();
        w.var_int(40)
            .var_int(2)
            .string("a")
            .string("b")
            .bool(true)
            .string("T");
        let b = read_edit_book(&mut crate::Reader::new(&w.buf)).unwrap();
        assert_eq!(b.slot, 40);
        assert_eq!(b.pages, ["a", "b"]);
        assert_eq!(b.title.as_deref(), Some("T"));
    }

    #[test]
    fn block_event_time_and_difficulty_roundtrip() {
        let ev = block_event(7, [1, 64, -2], 1, 3, 42);
        let mut r = crate::Reader::new(&ev);
        assert_eq!(r.var_int().unwrap(), 7);
        assert_eq!(unpack_block_pos(r.i64().unwrap()), [1, 64, -2]);
        assert_eq!(r.u8().unwrap(), 1);
        assert_eq!(r.u8().unwrap(), 3);
        assert_eq!(r.var_int().unwrap(), 42);

        let time = set_time(9, 10, 1000, true);
        let mut r = crate::Reader::new(&time);
        assert_eq!(r.var_int().unwrap(), 9);
        assert_eq!(r.i64().unwrap(), 10);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 0);
        assert_eq!(r.var_int().unwrap(), 1000);
        assert_eq!(r.f32().unwrap(), 0.0);
        assert_eq!(r.f32().unwrap(), 1.0);
        assert!(r.remaining().is_empty());

        let diff = change_difficulty(4, 2);
        let mut r = crate::Reader::new(&diff);
        assert_eq!(r.var_int().unwrap(), 4);
        assert_eq!(r.u8().unwrap(), 2);
        assert!(!r.bool().unwrap());
        assert!(r.remaining().is_empty());
        assert_eq!(START_FALL_FLYING, 6);
        assert_eq!(START_SPRINTING, 1);
        assert_eq!(STOP_SPRINTING, 2);
    }

    #[test]
    fn block_pos_roundtrip() {
        for p in [
            [0, 0, 0],
            [1, -64, -1],
            [-30_000_000, 2047, 29_999_999],
            [8, -61, 8],
        ] {
            assert_eq!(unpack_block_pos(pack_block_pos(p)), p);
        }
        // Известное значение: (1, 2, 3).
        assert_eq!(pack_block_pos([1, 2, 3]), (1 << 38) | (3 << 12) | 2);
    }
}

/// `LpVec3` обратно (`net.minecraft.network.LpVec3.read`).
pub fn read_lp_vec3(r: &mut Reader) -> Result<[f64; 3]> {
    let a = r.u8()? as i64;
    if a == 0 {
        return Ok([0.0; 3]);
    }
    let b = r.u8()? as i64;
    let c = u32::from_be_bytes(r.bytes(4)?.try_into().unwrap()) as i64;
    let packed = c << 16 | b << 8 | a;
    let mut scale = a & 3;
    if a & 4 != 0 {
        scale |= (r.var_int()? as i64) << 2;
    }
    let unpack = |v: i64| ((v & 32767).min(32766) as f64 * 2.0 / 32766.0 - 1.0) * scale as f64;
    Ok([
        unpack(packed >> 3),
        unpack(packed >> 18),
        unpack(packed >> 33),
    ])
}

/// `interact`: правый клик по сущности (рука, точка на рамке, шифт).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interact {
    pub entity: i32,
    pub hand: i32,
    pub at: [f64; 3],
    pub sneaking: bool,
}

pub fn read_interact(r: &mut Reader) -> Result<Interact> {
    Ok(Interact {
        entity: r.var_int()?,
        hand: r.var_int()?,
        at: read_lp_vec3(r)?,
        sneaking: r.bool()?,
    })
}

/// `set_beacon`: два необязательных эффекта (`Optional<Holder<MobEffect>>`).
pub fn read_set_beacon(r: &mut Reader) -> Result<(Option<i32>, Option<i32>)> {
    let primary = if r.bool()? { Some(r.var_int()?) } else { None };
    let secondary = if r.bool()? { Some(r.var_int()?) } else { None };
    Ok((primary, secondary))
}

/// Метаданные живого моба (`set_entity_data`): здоровье (9, `FLOAT`), детёныш
/// (`AgeableMob.DATA_BABY_ID` 16, `BOOLEAN`), шерсть овцы (`Sheep.DATA_WOOL_ID` 18, `BYTE`),
/// имя (2, `OPTIONAL_COMPONENT`) и его видимость (3, `BOOLEAN`).
pub fn mob_data(
    id: i32,
    entity_id: i32,
    health: f32,
    baby: Option<bool>,
    wool: Option<u8>,
    name: Option<&serde_json::Value>,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    w.u8(9).var_int(3).f32(health);
    if let Some(baby) = baby {
        w.u8(16).var_int(8).bool(baby);
    }
    if let Some(wool) = wool {
        w.u8(18).var_int(0).u8(wool);
    }
    if let Some(name) = name {
        w.u8(2).var_int(6).bool(true);
        crate::text::write_component(&mut w, name);
        w.u8(3).var_int(8).bool(true);
    }
    w.u8(0xff);
    w.buf
}

/// Флаги моба (`Mob.DATA_MOB_FLAGS_ID`, индекс 15): 1 — без ИИ, 2 — левша,
/// 4 — агрессивен (зомби поднимает руки).
pub fn mob_flags(id: i32, entity_id: i32, flags: u8) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    w.u8(15).var_int(0).u8(flags);
    w.u8(0xff);
    w.buf
}

/// Значение `SynchedEntityData`: байт, целое (`VarInt`) или логическое.
#[derive(Clone, Debug, PartialEq)]
pub enum Meta {
    Byte(u8),
    Int(i32),
    Float(f32),
    Bool(bool),
    /// `EntityDataSerializers.ROTATIONS` (id 9): три угла `FLOAT`.
    Rotations([f32; 3]),
    /// `EntityDataSerializers.PARTICLES` (id 17): список частиц одного типа.
    Particles {
        particle: i32,
        colors: Vec<i32>,
    },
}

/// `set_entity_data` с произвольными полями `(индекс, значение)`.
pub fn entity_meta(id: i32, entity_id: i32, fields: &[(u8, Meta)]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    for (i, v) in fields {
        match v {
            Meta::Byte(b) => {
                w.u8(*i).var_int(0).u8(*b);
            }
            Meta::Int(n) => {
                w.u8(*i).var_int(1).var_int(*n);
            }
            Meta::Float(v) => {
                w.u8(*i).var_int(3).f32(*v);
            }
            Meta::Bool(b) => {
                w.u8(*i).var_int(8).bool(*b);
            }
            Meta::Rotations(r) => {
                w.u8(*i).var_int(9);
                for v in r {
                    w.f32(*v);
                }
            }
            Meta::Particles { particle, colors } => {
                w.u8(*i).var_int(17).var_int(colors.len() as i32);
                for color in colors {
                    w.var_int(*particle).i32(*color);
                }
            }
        };
    }
    w.u8(0xff);
    w.buf
}

/// `set_equipment`: слоты (`EquipmentSlot` id, 0 — основная рука) и стопки.
pub fn set_equipment(id: i32, entity_id: i32, slots: &[(u8, crate::inventory::Slot)]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    for (n, &(slot, stack)) in slots.iter().enumerate() {
        let more = if n + 1 < slots.len() { 0x80 } else { 0 };
        w.u8(slot | more);
        crate::inventory::write_stack(&mut w, stack);
    }
    w.buf
}

/// `hurt_animation`: покраснение и наклон от удара с направления `yaw`.
pub fn hurt_animation(id: i32, entity_id: i32, yaw: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id).f32(yaw);
    w.buf
}

/// `animate`: действие над сущностью (1 — крит, 2 — магический крит).
pub fn animate(id: i32, entity_id: i32, action: u8) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id).u8(action);
    w.buf
}

#[cfg(test)]
mod lp_tests {
    use super::*;

    #[test]
    fn lp_vec3_roundtrip() {
        for v in [[0.0, 0.0, 0.0], [0.3, -0.7, 0.45], [5.5, 0.25, -9.0]] {
            let mut w = Writer::new();
            lp_vec3(&mut w, v);
            let back = read_lp_vec3(&mut Reader::new(&w.buf)).unwrap();
            for i in 0..3 {
                assert!(
                    (back[i] - v[i]).abs() < 1e-3 * v[i].abs().max(1.0),
                    "{v:?} {back:?}"
                );
            }
        }
    }

    #[test]
    fn particles_meta_uses_serializer_17() {
        let p = entity_meta(
            1,
            2,
            &[(
                10,
                Meta::Particles {
                    particle: 33,
                    colors: vec![0x26FF_0000u32 as i32],
                },
            )],
        );
        let mut r = Reader::new(&p);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.u8().unwrap(), 10);
        assert_eq!(r.var_int().unwrap(), 17);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 33);
        assert_eq!(
            i32::from_be_bytes(r.bytes(4).unwrap().try_into().unwrap()),
            0x26FF_0000u32 as i32
        );
        assert_eq!(r.u8().unwrap(), 0xff);
    }

    #[test]
    fn rotations_meta_uses_serializer_9() {
        let p = entity_meta(1, 2, &[(16, Meta::Rotations([1.5, -2.0, 3.0]))]);
        let mut r = Reader::new(&p);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.u8().unwrap(), 16);
        assert_eq!(r.var_int().unwrap(), 9);
        assert_eq!(r.f32().unwrap(), 1.5);
        assert_eq!(r.f32().unwrap(), -2.0);
        assert_eq!(r.f32().unwrap(), 3.0);
        assert_eq!(r.u8().unwrap(), 0xff);
    }

    #[test]
    fn float_meta_uses_serializer_3() {
        let p = entity_meta(1, 2, &[(8, Meta::Float(2.5))]);
        let mut r = Reader::new(&p);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.u8().unwrap(), 8);
        assert_eq!(r.var_int().unwrap(), 3);
        assert_eq!(r.f32().unwrap(), 2.5);
        assert_eq!(r.u8().unwrap(), 0xff);
    }
}
