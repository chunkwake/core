//! Пакеты состояния play (формат 26.3). ID передаются параметром.

use crate::Writer;

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
