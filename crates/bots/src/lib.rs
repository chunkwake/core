//! Shared offline protocol client used by the load bots and chunkwake-lab.

use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_net::Connection;
use chunkwake_protocol::configuration::{KnownPack, select_known_packs};
use chunkwake_protocol::packets::{Handshake, NextState};
use chunkwake_protocol::{Reader, Writer};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

#[derive(Clone, Copy)]
pub struct Ids {
    cfg_known_packs: i32,
    cfg_finish: i32,
    cfg_keep_alive: i32,
    sb_cfg_known_packs: i32,
    sb_cfg_finish: i32,
    sb_cfg_keep_alive: i32,
    sb_cfg_client_information: i32,
    keep_alive: i32,
    sb_keep_alive: i32,
    sb_move_pos: i32,
    sb_client_tick_end: i32,
    player_position: i32,
    block_update: i32,
    section_blocks_update: i32,
    sb_accept_teleportation: i32,
    sb_player_loaded: i32,
    add_entity: i32,
    move_entity_pos_rot: i32,
    move_entity_pos: i32,
    move_entity_rot: i32,
    teleport_entity: i32,
    entity_position_sync: i32,
    remove_entities: i32,
    set_time: i32,
    level_chunk: i32,
    forget_level_chunk: i32,
    chunk_batch_finished: i32,
    sb_chunk_batch_received: i32,
}

impl Ids {
    pub fn load(t: &PacketTable) -> Result<Self, String> {
        use Direction::{Clientbound as C, Serverbound as S};
        use State::{Configuration as Cfg, Play};
        let id = |s, d, n| t.id(s, d, n).map_err(|e| e.to_string());
        Ok(Self {
            cfg_known_packs: id(Cfg, C, "select_known_packs")?,
            cfg_finish: id(Cfg, C, "finish_configuration")?,
            cfg_keep_alive: id(Cfg, C, "keep_alive")?,
            sb_cfg_known_packs: id(Cfg, S, "select_known_packs")?,
            sb_cfg_finish: id(Cfg, S, "finish_configuration")?,
            sb_cfg_keep_alive: id(Cfg, S, "keep_alive")?,
            sb_cfg_client_information: id(Cfg, S, "client_information")?,
            keep_alive: id(Play, C, "keep_alive")?,
            sb_keep_alive: id(Play, S, "keep_alive")?,
            sb_move_pos: id(Play, S, "move_player_pos")?,
            sb_client_tick_end: id(Play, S, "client_tick_end")?,
            player_position: id(Play, C, "player_position")?,
            block_update: id(Play, C, "block_update")?,
            section_blocks_update: id(Play, C, "section_blocks_update")?,
            sb_accept_teleportation: id(Play, S, "accept_teleportation")?,
            sb_player_loaded: id(Play, S, "player_loaded")?,
            add_entity: id(Play, C, "add_entity")?,
            move_entity_pos_rot: id(Play, C, "move_entity_pos_rot")?,
            move_entity_pos: id(Play, C, "move_entity_pos")?,
            move_entity_rot: id(Play, C, "move_entity_rot")?,
            teleport_entity: id(Play, C, "teleport_entity")?,
            entity_position_sync: id(Play, C, "entity_position_sync")?,
            remove_entities: id(Play, C, "remove_entities")?,
            set_time: id(Play, C, "set_time")?,
            level_chunk: id(Play, C, "level_chunk_with_light")?,
            forget_level_chunk: id(Play, C, "forget_level_chunk")?,
            chunk_batch_finished: id(Play, C, "chunk_batch_finished")?,
            sb_chunk_batch_received: id(Play, S, "chunk_batch_received")?,
        })
    }
}

pub struct Client {
    conn: Connection,
    ids: Ids,
    pub join_time: Duration,
    pub packets_in: u64,
    pub bytes_in: u64,
    /// Пришедшие `level_chunk_with_light`.
    pub chunks_in: u64,
    /// Пакеты с этим ID сохраняются целиком в `captured` (без ID): отладка
    /// кодирования против декодера клиента.
    pub capture: Option<i32>,
    /// Чанки, которые сейчас есть у клиента (`level_chunk_with_light` минус
    /// `forget_level_chunk`).
    pub loaded: std::collections::HashSet<(i32, i32)>,
    pub captured: Vec<Vec<u8>>,
    /// Ещё ID для записи: пакеты попадают в `captured_by` вместе с ID.
    pub capture_also: Vec<i32>,
    pub captured_by: Vec<(i32, Vec<u8>)>,
    pub bytes_out: u64,
    pub keep_alives: u64,
    sent_player_loaded: bool,
}

#[derive(Debug, PartialEq)]
pub enum Event {
    Position([f64; 3]),
    BlockUpdate([i32; 3], i32),
    BlockUpdates(Vec<([i32; 3], i32)>),
    KeepAlive,
    /// Сущность появилась (или переехала телепортом): её текущая позиция.
    EntitySpawned {
        entity_id: i32,
        kind: i32,
        pos: [f64; 3],
    },
    /// Сущность сдвинулась на дельту (в блоках) относительно прежней позиции.
    EntityMoved {
        entity_id: i32,
        delta: [f64; 3],
        /// Шаговая дельта (`VecDelta.Stepped`): тики шага и смещение от прежней
        /// позиции к концу шага. Для линейной пусто.
        steps: Vec<(i32, [f64; 3])>,
    },
    EntityRemoved(Vec<i32>),
    /// Игровое время из `set_time`: сервер шлёт раз в 20 тиков.
    GameTime(i64),
    Other,
}

/// Что бот знает о чужой сущности.
#[derive(Debug, Clone, Copy)]
pub struct EntityTrack {
    pub entity_id: i32,
    /// Тип сущности из `add_entity`; -1 — неизвестен (только телепорт).
    pub kind: i32,
    /// Последняя известная позиция.
    pub pos: [f64; 3],
    /// Просуммированный путь по горизонтали (блоков).
    pub path: f64,
    /// Сколько пакетов движения получено.
    pub moves: u32,
    /// Сколько тиков сущность уже наблюдается.
    pub watched: u32,
    /// Самый длинный шаг за один пакет (блоков): скорость, когда моб идёт.
    pub longest_step: f64,
}

/// Учёт чужих сущностей по событиям клиента: путь и число пакетов движения.
#[derive(Default)]
pub struct Entities {
    by_id: std::collections::HashMap<i32, EntityTrack>,
}

impl Entities {
    /// Применяет событие; `true` — оно относилось к сущности.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            } => {
                self.by_id.insert(
                    *entity_id,
                    EntityTrack {
                        entity_id: *entity_id,
                        kind: if *kind >= 0 { *kind } else { 0 },
                        pos: *pos,
                        path: 0.0,
                        moves: 0,
                        watched: 0,
                        longest_step: 0.0,
                    },
                );
                true
            }
            Event::EntityMoved {
                entity_id, delta, ..
            } => {
                if let Some(e) = self.by_id.get_mut(entity_id) {
                    e.pos = [
                        e.pos[0] + delta[0],
                        e.pos[1] + delta[1],
                        e.pos[2] + delta[2],
                    ];
                    e.path += (delta[0] * delta[0] + delta[2] * delta[2]).sqrt();
                    e.longest_step = e
                        .longest_step
                        .max((delta[0] * delta[0] + delta[2] * delta[2]).sqrt());
                    e.moves += 1;
                }
                true
            }
            Event::EntityRemoved(ids) => {
                for id in ids {
                    self.by_id.remove(id);
                }
                true
            }
            _ => false,
        }
    }

    /// Тик наблюдения для всех сущностей.
    pub fn tick(&mut self) {
        for e in self.by_id.values_mut() {
            e.watched += 1;
        }
    }

    pub fn get(&self, entity_id: i32) -> Option<&EntityTrack> {
        self.by_id.get(&entity_id)
    }

    /// Все сущности, отсортированные по id.
    pub fn all(&self) -> Vec<EntityTrack> {
        let mut v: Vec<_> = self.by_id.values().copied().collect();
        v.sort_by_key(|e| e.entity_id);
        v
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Сбросить накопленный путь, оставив позиции: удобно для замера за окно.
    pub fn reset_paths(&mut self) {
        for e in self.by_id.values_mut() {
            e.path = 0.0;
            e.moves = 0;
            e.watched = 0;
            e.longest_step = 0.0;
        }
    }
}

impl Client {
    pub async fn connect(
        addr: &str,
        name: &str,
        ids: Ids,
        protocol: i32,
        version: &str,
    ) -> Result<Self, String> {
        let started = Instant::now();
        let stream = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        let port = stream.peer_addr().map_err(|e| e.to_string())?.port();
        let mut conn = Connection::new(stream);
        conn.send(
            &Handshake {
                protocol_version: protocol,
                server_address: "localhost",
                server_port: port,
                next_state: NextState::Login,
            }
            .encode(),
        )
        .await
        .map_err(err)?;
        let mut w = Writer::new();
        w.var_int(0).string(name).uuid(uuid::Uuid::nil());
        conn.send(&w.buf).await.map_err(err)?;
        loop {
            let p = conn.read_packet().await.map_err(err)?;
            let mut r = Reader::new(&p);
            match r.var_int().map_err(err)? {
                0 => {
                    return Err(format!(
                        "login disconnected: {}",
                        String::from_utf8_lossy(&p)
                    ));
                }
                2 => break,
                3 => {
                    let threshold = r.var_int().map_err(err)?;
                    conn.codec.threshold = (threshold >= 0).then_some(threshold as usize);
                }
                id => return Err(format!("unexpected login packet {id}")),
            }
        }
        conn.send(&[3]).await.map_err(err)?;
        // Настоящий клиент шлёт настройки сразу: без них сервер (как ваниль)
        // считает обзор равным 2. `CHUNKWAKE_BOT_VIEW` меняет запрошенный обзор.
        let view = std::env::var("CHUNKWAKE_BOT_VIEW")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(32);
        conn.send(&chunkwake_protocol::configuration::client_information(
            ids.sb_cfg_client_information,
            &chunkwake_protocol::configuration::ClientInfo {
                view_distance: view,
                skin_parts: 0x7f,
                main_hand: 1,
                allows_listing: true,
            },
        ))
        .await
        .map_err(err)?;
        loop {
            let p = conn.read_packet().await.map_err(err)?;
            let mut r = Reader::new(&p);
            let id = r.var_int().map_err(err)?;
            if id == ids.cfg_known_packs {
                conn.send(&select_known_packs(
                    ids.sb_cfg_known_packs,
                    &[KnownPack::vanilla(version)],
                ))
                .await
                .map_err(err)?;
            } else if id == ids.cfg_keep_alive {
                let mut w = Writer::new();
                w.var_int(ids.sb_cfg_keep_alive).i64(r.i64().map_err(err)?);
                conn.send(&w.buf).await.map_err(err)?;
            } else if id == ids.cfg_finish {
                let mut w = Writer::new();
                w.var_int(ids.sb_cfg_finish);
                conn.send(&w.buf).await.map_err(err)?;
                break;
            }
        }
        Ok(Self {
            conn,
            ids,
            join_time: started.elapsed(),
            packets_in: 0,
            chunks_in: 0,
            capture: None,
            loaded: std::collections::HashSet::new(),
            captured: Vec::new(),
            capture_also: Vec::new(),
            captured_by: Vec::new(),
            bytes_in: 0,
            bytes_out: 0,
            keep_alives: 0,
            sent_player_loaded: false,
        })
    }

    pub async fn next_event(&mut self) -> Result<Event, String> {
        let p = self.conn.read_packet().await.map_err(err)?;
        self.packets_in += 1;
        self.bytes_in += p.len() as u64;
        let mut r = Reader::new(&p);
        let id = r.var_int().map_err(err)?;
        if id == self.ids.level_chunk {
            self.chunks_in += 1;
            let b = r.remaining();
            if b.len() >= 8 {
                let x = i32::from_be_bytes([b[0], b[1], b[2], b[3]]);
                let z = i32::from_be_bytes([b[4], b[5], b[6], b[7]]);
                self.loaded.insert((x, z));
            }
        } else if id == self.ids.forget_level_chunk {
            // `ChunkPos.toLong`: x в младших 32 битах, z в старших.
            let mut c = Reader::new(r.remaining());
            if let Ok(v) = c.i64() {
                self.loaded.remove(&(v as i32, (v >> 32) as i32));
            }
        }
        if self.capture == Some(id) {
            self.captured.push(r.remaining().to_vec());
        }
        if self.capture_also.contains(&id) {
            self.captured_by.push((id, r.remaining().to_vec()));
        }
        if id == self.ids.chunk_batch_finished {
            // Как клиент: подтвердить пачку, иначе ваниль держит чанки «в
            // отправке» и не показывает в них сущности (`ChunkMap.isChunkTracked`).
            let mut w = Writer::new();
            w.var_int(self.ids.sb_chunk_batch_received).f32(64.0);
            self.bytes_out += w.buf.len() as u64;
            self.conn.send(&w.buf).await.map_err(err)?;
        }
        if id == self.ids.keep_alive {
            let mut w = Writer::new();
            w.var_int(self.ids.sb_keep_alive).i64(r.i64().map_err(err)?);
            self.bytes_out += w.buf.len() as u64;
            self.conn.send(&w.buf).await.map_err(err)?;
            self.keep_alives += 1;
            Ok(Event::KeepAlive)
        } else if id == self.ids.player_position {
            let teleport_id = r.var_int().map_err(err)?;
            let pos = [
                r.f64().map_err(err)?,
                r.f64().map_err(err)?,
                r.f64().map_err(err)?,
            ];
            for _ in 0..3 {
                r.f64().map_err(err)?;
            }
            let yaw = r.f32().map_err(err)?;
            let pitch = r.f32().map_err(err)?;
            let mut w = Writer::new();
            w.var_int(self.ids.sb_accept_teleportation)
                .var_int(teleport_id);
            for coordinate in pos {
                w.f64(coordinate);
            }
            w.f32(yaw).f32(pitch);
            self.bytes_out += w.buf.len() as u64;
            self.conn.send(&w.buf).await.map_err(err)?;
            if !self.sent_player_loaded {
                let mut loaded = Writer::new();
                loaded.var_int(self.ids.sb_player_loaded);
                self.bytes_out += loaded.buf.len() as u64;
                self.conn.send(&loaded.buf).await.map_err(err)?;
                self.sent_player_loaded = true;
            }
            Ok(Event::Position(pos))
        } else if id == self.ids.block_update {
            let packed = r.i64().map_err(err)?;
            let position = [
                (packed >> 38) as i32,
                (packed << 52 >> 52) as i32,
                (packed << 26 >> 38) as i32,
            ];
            Ok(Event::BlockUpdate(position, r.var_int().map_err(err)?))
        } else if id == self.ids.section_blocks_update {
            let packed = r.i64().map_err(err)?;
            let section = [
                (packed >> 42) as i32,
                (packed << 44 >> 44) as i32,
                (packed << 22 >> 42) as i32,
            ];
            let count = r.var_int().map_err(err)?;
            if !(0..=4096).contains(&count) {
                return Err(format!("invalid section block count {count}"));
            }
            let mut blocks = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let entry = r.var_long().map_err(err)? as u64;
                let local = entry & 0xfff;
                blocks.push((
                    [
                        section[0] * 16 + ((local >> 8) & 15) as i32,
                        section[1] * 16 + (local & 15) as i32,
                        section[2] * 16 + ((local >> 4) & 15) as i32,
                    ],
                    (entry >> 12) as i32,
                ));
            }
            Ok(Event::BlockUpdates(blocks))
        } else if id == self.ids.add_entity {
            let entity_id = r.var_int().map_err(err)?;
            let _uuid = r.uuid().map_err(err)?;
            let kind = r.var_int().map_err(err)?;
            let pos = [
                r.f64().map_err(err)?,
                r.f64().map_err(err)?,
                r.f64().map_err(err)?,
            ];
            Ok(Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            })
        } else if id == self.ids.entity_position_sync {
            // `PositionPath`: 0 — одна точка, 1 — список шагов (точка, тики); берём конец.
            let entity_id = r.var_int().map_err(err)?;
            let mut pos = [0.0; 3];
            let stepped = r.var_int().map_err(err)? == 1;
            let steps = if stepped {
                r.var_int().map_err(err)?
            } else {
                1
            };
            for _ in 0..steps {
                for v in &mut pos {
                    *v = r.f64().map_err(err)?;
                }
                if stepped {
                    r.var_int().map_err(err)?;
                }
            }
            Ok(Event::EntitySpawned {
                entity_id,
                kind: -1,
                pos,
            })
        } else if id == self.ids.teleport_entity {
            let entity_id = r.var_int().map_err(err)?;
            let pos = [
                r.f64().map_err(err)?,
                r.f64().map_err(err)?,
                r.f64().map_err(err)?,
            ];
            Ok(Event::EntitySpawned {
                entity_id,
                kind: -1,
                pos,
            })
        } else if id == self.ids.move_entity_rot {
            Ok(Event::Other)
        } else if id == self.ids.move_entity_pos_rot || id == self.ids.move_entity_pos {
            let entity_id = r.var_int().map_err(err)?;
            let props = r.var_int().map_err(err)?;
            // Младший бит — onGround, остальные — число шагов (0 — линейная дельта).
            let steps = props >> 1;
            // `VecDelta`: линейная — три short; шаговая — на каждый шаг varint тиков
            // и три short от предыдущего шага. В 1/4096 блока.
            let mut raw = [0i64; 3];
            let mut path = Vec::new();
            for _ in 0..steps.max(1) {
                let ticks = if steps > 0 {
                    r.var_int().map_err(err)?
                } else {
                    1
                };
                for v in &mut raw {
                    *v += r.u16().map_err(err)? as i16 as i64;
                }
                if steps > 0 {
                    path.push((ticks, raw.map(|v| v as f64 / 4096.0)));
                }
            }
            Ok(Event::EntityMoved {
                entity_id,
                delta: raw.map(|v| v as f64 / 4096.0),
                steps: path,
            })
        } else if id == self.ids.remove_entities {
            let count = r.var_int().map_err(err)?;
            let mut ids = Vec::new();
            for _ in 0..count.max(0) {
                ids.push(r.var_int().map_err(err)?);
            }
            Ok(Event::EntityRemoved(ids))
        } else if id == self.ids.set_time {
            let game_time = r.i64().map_err(err)?;
            Ok(Event::GameTime(game_time))
        } else {
            Ok(Event::Other)
        }
    }

    /// Отправить готовый пакет (ID и тело) как есть.
    pub async fn send_raw(&mut self, packet: &[u8]) -> Result<(), String> {
        self.bytes_out += packet.len() as u64;
        self.conn.send(packet).await.map_err(err)?;
        self.conn.flush().await.map_err(err)
    }

    pub async fn move_to(&mut self, pos: [f64; 3]) -> Result<(), String> {
        let mut w = Writer::new();
        w.var_int(self.ids.sb_move_pos);
        for v in pos {
            w.f64(v);
        }
        w.u8(1);
        self.bytes_out += w.buf.len() as u64;
        self.conn.send(&w.buf).await.map_err(err)?;
        let mut tick_end = Writer::new();
        tick_end.var_int(self.ids.sb_client_tick_end);
        self.bytes_out += tick_end.buf.len() as u64;
        self.conn.send(&tick_end.buf).await.map_err(err)?;
        // Соединение копит кадры: движение должно уйти сейчас, а не к следующему чтению.
        self.conn.flush().await.map_err(err)
    }
}

pub async fn status(addr: &str, protocol: i32) -> Result<serde_json::Value, String> {
    let stream = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
    let port = stream.peer_addr().map_err(|e| e.to_string())?.port();
    let mut conn = Connection::new(stream);
    conn.send(
        &Handshake {
            protocol_version: protocol,
            server_address: "localhost",
            server_port: port,
            next_state: NextState::Status,
        }
        .encode(),
    )
    .await
    .map_err(err)?;
    conn.send(&[0]).await.map_err(err)?;
    let p = conn.read_packet().await.map_err(err)?;
    let mut r = Reader::new(&p);
    if r.var_int().map_err(err)? != 0 {
        return Err("unexpected status response".into());
    }
    let value = serde_json::from_str(r.string(32767).map_err(err)?).map_err(err)?;
    let mut w = Writer::new();
    w.var_int(1).i64(0x1234_5678);
    conn.send(&w.buf).await.map_err(err)?;
    let pong = conn.read_packet().await.map_err(err)?;
    let mut r = Reader::new(&pong);
    if r.var_int().map_err(err)? != 1 || r.i64().map_err(err)? != 0x1234_5678 {
        return Err("invalid status pong".into());
    }
    Ok(value)
}

fn err(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
