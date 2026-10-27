//! Минимальная play-сессия M2: игрок появляется в плоском мире и остаётся
//! подключённым (keep-alive). Мир статичен, движение не обрабатывается.

use crate::{Connection, LoggedIn, NetError};
use chunkwake_data::DataError;
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Reader;
use chunkwake_protocol::play::{self, Login, PlayerEntry, game_event};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use uuid::Uuid;

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
    forget_level_chunk: i32,
    player_info_update: i32,
    player_info_remove: i32,
    system_chat: i32,
    sb_chat: i32,
    sb_keep_alive: i32,
    sb_move_pos: i32,
    sb_move_pos_rot: i32,
}

pub struct PlayPlan {
    ids: Ids,
    login: Login,
    /// Тело чанка (одинаковое для всех координат плоского мира).
    chunk_body: Vec<u8>,
    spawn: [f64; 3],
    next_entity_id: AtomicI32,
    hub: Hub,
}

/// Пакет для всех игроков; `skip` — кому не слать (обычно автору события).
#[derive(Clone)]
struct Broadcast {
    skip: Option<Uuid>,
    packet: Arc<Vec<u8>>,
}

/// Общее состояние сессий: таб-лист и широковещательный канал.
struct Hub {
    players: Mutex<HashMap<Uuid, PlayerEntry>>,
    tx: broadcast::Sender<Broadcast>,
}

impl Hub {
    fn send(&self, skip: Option<Uuid>, packet: Vec<u8>) {
        // Ошибка = нет подписчиков; это нормально.
        let _ = self.tx.send(Broadcast {
            skip,
            packet: Arc::new(packet),
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

impl PlayPlan {
    pub fn build(
        packets: &PacketTable,
        login: Login,
        chunk_body: Vec<u8>,
        spawn: [f64; 3],
    ) -> Result<Self, DataError> {
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
            },
            login,
            chunk_body,
            spawn,
            next_entity_id: AtomicI32::new(1),
            hub: Hub {
                players: Mutex::new(HashMap::new()),
                tx: broadcast::channel(1024).0,
            },
        })
    }

    async fn send_chunks(
        &self,
        conn: &mut Connection,
        center: (i32, i32),
        coords: &[(i32, i32)],
    ) -> Result<(), NetError> {
        let ids = self.ids;
        conn.send(&play::set_chunk_cache_center(
            ids.set_chunk_cache_center,
            center.0,
            center.1,
        ))
        .await?;
        conn.send(&play::chunk_batch_start(ids.chunk_batch_start))
            .await?;
        for &(x, z) in coords {
            conn.send(&play::level_chunk_with_light(
                ids.level_chunk_with_light,
                x,
                z,
                &self.chunk_body,
            ))
            .await?;
        }
        conn.send(&play::chunk_batch_finished(
            ids.chunk_batch_finished,
            coords.len() as i32,
        ))
        .await
    }

    /// Игрок перешёл в другой чанк: выгружаем ушедшие из радиуса, шлём новые.
    async fn move_center(
        &self,
        conn: &mut Connection,
        old: (i32, i32),
        new: (i32, i32),
        r: i32,
    ) -> Result<(), NetError> {
        for (x, z) in chunks_around(old, r, Some((new, r))) {
            conn.send(&play::forget_level_chunk(self.ids.forget_level_chunk, x, z))
                .await?;
        }
        self.send_chunks(conn, new, &chunks_around(new, r, Some((old, r))))
            .await
    }

    /// Сессия игрока: вход в таб-лист, игра, выход с оповещением остальных.
    pub async fn run(&self, player: &LoggedIn, conn: &mut Connection) -> Result<(), NetError> {
        let ids = self.ids;
        let me = PlayerEntry {
            uuid: player.uuid,
            name: player.name.clone(),
            properties: player.properties.clone(),
            game_mode: self.login.spawn.game_mode as i32,
            latency_ms: 0,
        };
        // Подписка до вставки в список: ни одно событие после снимка не теряется.
        let rx = self.hub.tx.subscribe();
        self.hub.players.lock().unwrap().insert(me.uuid, me.clone());
        self.hub.send(
            Some(me.uuid),
            play::player_info_add(ids.player_info_update, std::slice::from_ref(&me)),
        );
        self.hub.send(
            None,
            play::system_chat(
                ids.system_chat,
                &format!("{} joined the game", me.name),
                false,
            ),
        );

        let result = self.session(conn, rx, me.uuid, &me.name).await;

        self.hub.players.lock().unwrap().remove(&me.uuid);
        self.hub.send(
            None,
            play::player_info_remove(ids.player_info_remove, &[me.uuid]),
        );
        self.hub.send(
            None,
            play::system_chat(
                ids.system_chat,
                &format!("{} left the game", me.name),
                false,
            ),
        );
        result
    }

    async fn session(
        &self,
        conn: &mut Connection,
        mut rx: broadcast::Receiver<Broadcast>,
        me: Uuid,
        name: &str,
    ) -> Result<(), NetError> {
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
        let r_view = login.view_distance;
        self.send_chunks(conn, (cx, cz), &chunks_around((cx, cz), r_view, None))
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
        let everyone: Vec<PlayerEntry> =
            self.hub.players.lock().unwrap().values().cloned().collect();
        conn.send(&play::player_info_add(ids.player_info_update, &everyone))
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
                event = rx.recv() => match event {
                    Ok(b) if b.skip == Some(me) => {}
                    Ok(b) => conn.send(&b.packet).await?,
                    // Отстали — пропущенное не догоняем (таб-лист обновится следующими событиями).
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                },
                packet = conn.read_packet() => {
                    let packet = packet?;
                    let mut r = Reader::new(&packet);
                    let id = r.var_int()?;
                    if id == ids.sb_move_pos || id == ids.sb_move_pos_rot {
                        let (x, _y, z) = (r.f64()?, r.f64()?, r.f64()?);
                        let now = ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4);
                        if now != center {
                            self.move_center(conn, center, now, r_view).await?;
                            center = now;
                        }
                        continue;
                    }
                    if id == ids.sb_chat {
                        let text = play::read_chat_message(&mut r)?;
                        let line = format!("<{name}> {text}");
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
    use super::chunks_around;

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
