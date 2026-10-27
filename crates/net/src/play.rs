//! Минимальная play-сессия M2: игрок появляется в плоском мире и остаётся
//! подключённым (keep-alive). Мир статичен, движение не обрабатывается.

use crate::{Connection, NetError};
use chunkwake_data::DataError;
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Reader;
use chunkwake_protocol::play::{self, Login, game_event};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(30);

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
    sb_keep_alive: i32,
}

pub struct PlayPlan {
    ids: Ids,
    login: Login,
    /// Тело чанка (одинаковое для всех координат плоского мира).
    chunk_body: Vec<u8>,
    spawn: [f64; 3],
    next_entity_id: AtomicI32,
}

impl std::fmt::Debug for PlayPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayPlan")
            .field("chunk_body", &self.chunk_body.len())
            .finish_non_exhaustive()
    }
}

impl PlayPlan {
    pub fn build(
        packets: &PacketTable,
        login: Login,
        chunk_body: Vec<u8>,
        spawn: [f64; 3],
    ) -> Result<Self, DataError> {
        let cb = |name| packets.id(State::Play, Direction::Clientbound, name);
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
                sb_keep_alive: packets.id(State::Play, Direction::Serverbound, "keep_alive")?,
            },
            login,
            chunk_body,
            spawn,
            next_entity_id: AtomicI32::new(1),
        })
    }

    pub async fn run(&self, conn: &mut Connection) -> Result<(), NetError> {
        let ids = self.ids;
        let mut login = self.login.clone();
        login.entity_id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        conn.send(&play::login(ids.login, &login)).await?;
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
        conn.send(&play::set_chunk_cache_center(
            ids.set_chunk_cache_center,
            cx,
            cz,
        ))
        .await?;

        // Чанки радиуса view distance, ближние первыми.
        let r = login.view_distance;
        let mut coords: Vec<(i32, i32)> = (-r..=r)
            .flat_map(|dx| (-r..=r).map(move |dz| (dx, dz)))
            .collect();
        coords.sort_by_key(|&(dx, dz)| dx * dx + dz * dz);
        conn.send(&play::chunk_batch_start(ids.chunk_batch_start))
            .await?;
        for &(dx, dz) in &coords {
            conn.send(&play::level_chunk_with_light(
                ids.level_chunk_with_light,
                cx + dx,
                cz + dz,
                &self.chunk_body,
            ))
            .await?;
        }
        conn.send(&play::chunk_batch_finished(
            ids.chunk_batch_finished,
            coords.len() as i32,
        ))
        .await?;
        conn.send(&play::player_position(
            ids.player_position,
            1,
            self.spawn,
            0.0,
            0.0,
        ))
        .await?;

        // Keep-alive: шлём раз в 15 с, ответ ждём не дольше 30 с.
        let mut ticker = tokio::time::interval(KEEP_ALIVE_INTERVAL);
        ticker.tick().await;
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
                packet = conn.read_packet() => {
                    let packet = packet?;
                    let mut r = Reader::new(&packet);
                    if r.var_int()? == ids.sb_keep_alive && pending.is_some_and(|(p, _)| p == r.i64().unwrap_or(0)) {
                        pending = None;
                    }
                    // Остальное (движение, настройки, подтверждение телепорта) — с M3.
                }
            }
        }
    }
}
