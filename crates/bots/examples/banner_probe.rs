//! Знамёна живым ботом: тот же сценарий на нашем сервере и на `server.jar`,
//! вывод — нормализованный журнал для сравнения.
//!
//! Красное знамя с узором и именем ставится рукой (`block_entity_data`),
//! заполненная карта по нему ставит и снимает метку, знамя без блока
//! теряет метку при перерисовке; сломанное `destroy` знамя выпадает с
//! узором и именем (слот после подбора).
//!
//! `banner_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const RED: [i32; 3] = [10, 200, 8];
const WHITE: [i32; 3] = [12, 200, 8];

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

fn unpack(v: i64) -> [i32; 3] {
    [
        (v >> 38) as i32,
        (v << 52 >> 52) as i32,
        (v << 26 >> 38) as i32,
    ]
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

struct Probe {
    c: Client,
    table: PacketTable,
    seq: i32,
    cb: HashMap<i32, &'static str>,
    log: Vec<String>,
    /// Последний список значков карты (повторы не пишутся).
    last_deco: String,
}

impl Probe {
    fn sb(&self, n: &str) -> i32 {
        self.table
            .id(State::Play, Direction::Serverbound, n)
            .unwrap()
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {}
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if let Some(kind) = self.cb.get(&id) {
                self.read(kind, &body);
            }
        }
    }

    fn read(&mut self, kind: &str, b: &[u8]) {
        let mut r = Reader::new(b);
        match kind {
            "block_entity_data" => {
                let p = unpack(r.i64().unwrap());
                if p == RED || p == WHITE {
                    let t = r.var_int().unwrap();
                    self.log
                        .push(format!("be {p:?} type {t} {}", hex(r.remaining())));
                }
            }
            "map_item_data" => {
                let id = r.var_int().unwrap();
                let (_scale, _locked) = (r.u8().unwrap(), r.bool().unwrap());
                if !r.bool().unwrap() {
                    return;
                }
                let n = r.var_int().unwrap();
                let mut out = Vec::new();
                for _ in 0..n {
                    let t = r.var_int().unwrap();
                    let (x, z, rot) = (
                        r.u8().unwrap() as i8,
                        r.u8().unwrap() as i8,
                        r.u8().unwrap(),
                    );
                    // Игрок (0) и его стрелка за краем (6) — не про знамёна.
                    let name = if r.bool().unwrap() {
                        // Имя — корневой NBT-текст: строка (`8`, длина, байты).
                        let tag = r.u8().unwrap();
                        if tag == 8 {
                            let len = r.u16().unwrap() as usize;
                            format!(" name {}", String::from_utf8_lossy(r.bytes(len).unwrap()))
                        } else {
                            format!(" name tag {tag} {}", hex(r.remaining()))
                        }
                    } else {
                        String::new()
                    };
                    if t != 0 && t != 6 {
                        out.push(format!("type {t} x {x} z {z} rot {rot}{name}"));
                    }
                }
                let s = format!("map {id} decorations [{}]", out.join("; "));
                if s != self.last_deco {
                    self.last_deco = s.clone();
                    self.log.push(s);
                }
            }
            "container_set_slot" => {
                let _win = r.var_int().unwrap();
                let _state = r.var_int().unwrap();
                let slot = r.u16().unwrap();
                self.log.push(format!("slot {slot} {}", hex(r.remaining())));
            }
            _ => {}
        }
    }

    async fn cmd(&mut self, s: &str) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(s);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn hold(&mut self, slot: i16) {
        let mut w = Writer::new();
        w.var_int(self.sb("set_carried_item")).u16(slot as u16);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(100).await;
    }

    async fn use_item(&mut self) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item"))
            .var_int(0)
            .var_int(self.seq)
            .f32(0.0)
            .f32(0.0);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn use_on(&mut self, p: [i32; 3], face: i32) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on"))
            .var_int(0)
            .i64(pack(p))
            .var_int(face)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    fn step(&mut self, title: &str) {
        self.log.push(format!("== {title}"));
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let mut cb = HashMap::new();
    for n in ["block_entity_data", "map_item_data", "container_set_slot"] {
        let id = table.id(State::Play, Direction::Clientbound, n).unwrap();
        cb.insert(id, n);
        c.capture_also.push(id);
    }
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
    let mut p = Probe {
        c,
        table,
        seq: 0,
        cb,
        log: Vec::new(),
        last_deco: String::new(),
    };
    p.pump(2000).await;
    for s in [
        "gamemode survival",
        "fill 4 199 4 14 199 14 minecraft:stone",
        "fill 4 200 4 14 203 14 minecraft:air",
        "tp framer 8.5 200 8.5 -90 0",
        "kill @e[type=minecraft:item]",
        "clear framer",
        r#"give framer minecraft:red_banner[banner_patterns=[{pattern:"minecraft:stripe_top",color:"blue"}],custom_name="Base"]"#,
        "give framer minecraft:map",
        "give framer minecraft:white_banner",
    ] {
        p.cmd(s).await;
    }
    p.pump(1500).await;
    p.log.clear();

    p.step("place the red banner");
    p.hold(0).await;
    p.use_on([RED[0], RED[1] - 1, RED[2]], 1).await;
    p.step("white banner");
    p.hold(2).await;
    p.use_on([WHITE[0], WHITE[1] - 1, WHITE[2]], 1).await;
    p.step("fill the map");
    p.hold(1).await;
    p.use_item().await;
    p.pump(2000).await;
    p.step("map on the red banner");
    p.use_on(RED, 1).await;
    p.pump(1000).await;
    p.step("map on the white banner");
    p.use_on(WHITE, 1).await;
    p.pump(1000).await;
    p.step("red again: marker off");
    p.use_on(RED, 1).await;
    p.pump(1000).await;
    p.step("red again: marker on");
    p.use_on(RED, 1).await;
    p.pump(1000).await;
    p.step("white banner removed: marker goes on redraw");
    p.cmd("setblock 12 200 8 minecraft:air").await;
    p.pump(2500).await;
    p.step("red destroyed: drop keeps pattern and name");
    p.cmd("clear framer").await;
    p.pump(300).await;
    p.cmd("setblock 10 200 8 minecraft:air destroy").await;
    p.cmd("tp framer 10.5 200 8.5").await;
    p.pump(2500).await;
    for l in &p.log {
        println!("{l}");
    }
}
