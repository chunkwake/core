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
    keep_alive: i32,
    sb_keep_alive: i32,
    sb_move_pos: i32,
    player_position: i32,
    sb_accept_teleportation: i32,
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
            keep_alive: id(Play, C, "keep_alive")?,
            sb_keep_alive: id(Play, S, "keep_alive")?,
            sb_move_pos: id(Play, S, "move_player_pos")?,
            player_position: id(Play, C, "player_position")?,
            sb_accept_teleportation: id(Play, S, "accept_teleportation")?,
        })
    }
}

pub struct Client {
    conn: Connection,
    ids: Ids,
    pub join_time: Duration,
    pub packets_in: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub keep_alives: u64,
}

#[derive(Debug, PartialEq)]
pub enum Event {
    Position([f64; 3]),
    KeepAlive,
    Other,
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
            bytes_in: 0,
            bytes_out: 0,
            keep_alives: 0,
        })
    }

    pub async fn next_event(&mut self) -> Result<Event, String> {
        let p = self.conn.read_packet().await.map_err(err)?;
        self.packets_in += 1;
        self.bytes_in += p.len() as u64;
        let mut r = Reader::new(&p);
        let id = r.var_int().map_err(err)?;
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
            let mut w = Writer::new();
            w.var_int(self.ids.sb_accept_teleportation)
                .var_int(teleport_id);
            self.bytes_out += w.buf.len() as u64;
            self.conn.send(&w.buf).await.map_err(err)?;
            Ok(Event::Position(pos))
        } else {
            Ok(Event::Other)
        }
    }

    pub async fn move_to(&mut self, pos: [f64; 3]) -> Result<(), String> {
        let mut w = Writer::new();
        w.var_int(self.ids.sb_move_pos);
        for v in pos {
            w.f64(v);
        }
        w.u8(1);
        self.bytes_out += w.buf.len() as u64;
        self.conn.send(&w.buf).await.map_err(err)
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
