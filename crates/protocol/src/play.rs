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
    pub const CHANGE_GAME_MODE: u8 = 3;
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

/// `player_abilities` по режиму игры (`GameType.updatePlayerAbilities`):
/// неуязвимость, полёт, мгновенная ломка; скорости полёта и ходьбы ванилы.
pub fn player_abilities(id: i32, game_mode: u8) -> Vec<u8> {
    let flags: u8 = match game_mode {
        1 => 0x01 | 0x04 | 0x08,
        3 => 0x01 | 0x02 | 0x04,
        _ => 0,
    };
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

/// `ServerboundPlayerCommandPacket.Action` (после ID сущности).
pub const START_SPRINTING: i32 = 1;
pub const STOP_SPRINTING: i32 = 2;

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
    pub sequence: i32,
}

pub fn read_use_item_on(r: &mut Reader) -> Result<UseItemOn> {
    let hand = r.var_int()?;
    let pos = unpack_block_pos(r.i64()?);
    let face = r.var_int()?;
    // Точка попадания (3×float), inside, worldBorderHit.
    r.bytes(12 + 2)?;
    Ok(UseItemOn {
        hand,
        pos,
        face,
        sequence: r.var_int()?,
    })
}

#[cfg(test)]
mod block_tests {
    use super::*;

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
