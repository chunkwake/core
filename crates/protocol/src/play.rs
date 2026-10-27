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
    let s = &l.spawn;
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
    w.bool(l.online_mode).bool(l.enforces_secure_chat);
    w.buf
}

/// Коды `ClientboundGameEventPacket.Type`.
pub mod game_event {
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
}

/// Действия `ClientboundPlayerInfoUpdatePacket$Action` (порядковые номера в EnumSet).
const ADD_PLAYER: u8 = 1 << 0;
const UPDATE_GAME_MODE: u8 = 1 << 2;
const UPDATE_LISTED: u8 = 1 << 3;
const UPDATE_LATENCY: u8 = 1 << 4;

/// `player_info_update` с добавлением игроков в таб-лист.
pub fn player_info_add(id: i32, players: &[PlayerEntry]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    // EnumSet из 8 действий — фиксированный битсет в 1 байт.
    w.u8(ADD_PLAYER | UPDATE_GAME_MODE | UPDATE_LISTED | UPDATE_LATENCY);
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
        w.var_int(p.game_mode).bool(true).var_int(p.latency_ms);
    }
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
            }],
        );
        let mut r = Reader::new(&p);
        assert_eq!(r.var_int().unwrap(), 0x40);
        assert_eq!(r.u8().unwrap(), 0b1_1101);
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
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .uuid(uuid)
        .var_int(entity_type);
    for v in pos {
        w.f64(v);
    }
    w.u8(0) // movement
        .u8(angle(pitch))
        .u8(angle(yaw))
        .u8(angle(yaw)) // голова
        .var_int(data);
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
