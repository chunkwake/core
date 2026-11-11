//! Маяк: пирамида, окно, оплата, `set_beacon`, эффект и данные блока.
//!
//! `beacon_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::inventory::{self as inv, Slot};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const SWAP: i32 = 2;
/// Координаты маяка между обычным прогоном и проверкой после перезапуска.
const POS_FILE: &str = "target/beacon_probe.pos";

fn pack_block([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF)
}

fn saved_pos() -> Option<[i32; 3]> {
    let s = std::fs::read_to_string(POS_FILE).ok()?;
    let v: Vec<i32> = s
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_item_on: i32,
    sb_click: i32,
    sb_close: i32,
    sb_set_beacon: i32,
    cb_open: i32,
    cb_content: i32,
    cb_data: i32,
    cb_effect: i32,
    sequence: i32,
    window: i32,
    state: i32,
    slots: Vec<Slot>,
    data: HashMap<u16, i16>,
    effects: Vec<(i32, i32, i32)>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {}
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            let mut r = Reader::new(&body);
            if id == self.cb_open {
                self.window = r.var_int().unwrap_or(-1);
            } else if id == self.cb_content {
                let (Ok(window), Ok(state)) = (r.var_int(), r.var_int()) else {
                    continue;
                };
                let Ok(n) = r.var_int() else { continue };
                let mut slots = Vec::new();
                for _ in 0..n.clamp(0, 128) {
                    let Ok(slot) = inv::read_stack(&mut r) else {
                        break;
                    };
                    slots.push(slot);
                }
                if window == self.window {
                    self.state = state;
                    self.slots = slots;
                }
            } else if id == self.cb_data {
                let (Ok(window), Ok(key), Ok(value)) = (r.var_int(), r.u16(), r.u16()) else {
                    continue;
                };
                if window == self.window {
                    self.data.insert(key, value as i16);
                }
            } else if id == self.cb_effect {
                let (Ok(_entity), Ok(effect), Ok(amp), Ok(duration)) =
                    (r.var_int(), r.var_int(), r.var_int(), r.var_int())
                else {
                    continue;
                };
                self.effects.push((effect, amp, duration));
            }
        }
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    async fn open(&mut self, pos: [i32; 3]) -> bool {
        self.sequence += 1;
        let mut w = self.packet(self.sb_use_item_on);
        w.var_int(0)
            .i64(pack_block(pos))
            .var_int(1)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.sequence);
        self.c.send_raw(&w.buf).await.unwrap();
        self.window = -1;
        self.slots.clear();
        self.data.clear();
        self.pump(600).await;
        self.window != -1
    }

    async fn click(&mut self, slot: i16, button: i8, mode: i32) {
        let mut w = self.packet(self.sb_click);
        w.var_int(self.window)
            .var_int(self.state)
            .u16(slot as u16)
            .u8(button as u8)
            .var_int(mode)
            .var_int(0)
            .u8(0);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(350).await;
    }

    async fn close(&mut self) {
        let mut w = self.packet(self.sb_close);
        w.var_int(self.window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
        self.window = -1;
    }

    /// Убрать маяк и слои 2–4, чтобы прошлый прогон не влиял на новый.
    async fn clean_site(&mut self, b: [i32; 3]) {
        self.cmd(&format!(
            "fill {} {} {} {} {} {} minecraft:air",
            b[0] - 4,
            b[1] - 4,
            b[2] - 1,
            b[0] + 4,
            b[1] - 2,
            b[2] + 1
        ))
        .await;
        self.cmd(&format!(
            "setblock {} {} {} minecraft:air",
            b[0], b[1], b[2]
        ))
        .await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let item = |n: &str| {
        reg["minecraft:item"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let effect = |n: &str| {
        reg["minecraft:mob_effect"]["entries"][n]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let sb = |name| table.id(State::Play, Direction::Serverbound, name).unwrap();
    let cb = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let spawn = loop {
        if let Event::Position(at) = c.next_event().await.unwrap() {
            break at;
        }
    };
    let (cb_open, cb_content, cb_data, cb_effect) = (
        cb("open_screen"),
        cb("container_set_content"),
        cb("container_set_data"),
        cb("update_mob_effect"),
    );
    c.capture_also = vec![cb_open, cb_content, cb_data, cb_effect];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_item_on: sb("use_item_on"),
        sb_click: sb("container_click"),
        sb_close: sb("container_close"),
        sb_set_beacon: sb("set_beacon"),
        cb_open,
        cb_content,
        cb_data,
        cb_effect,
        sequence: 0,
        window: -1,
        state: 0,
        slots: Vec::new(),
        data: HashMap::new(),
        effects: Vec::new(),
    };
    let (x, y, z) = (
        spawn[0].floor() as i32,
        spawn[1].floor() as i32,
        spawn[2].floor() as i32,
    );
    let mut beacon = [x + 3, y, z];
    // Проверка после перезапуска: уровни и эффекты должны прийти из NBT.
    if std::env::var("BEACON_MODE").as_deref() == Ok("check") {
        if let Some(pos) = saved_pos() {
            beacon = pos;
        }
        p.cmd("gamemode creative").await;
        p.cmd(&format!(
            "tp @s {} {} {}",
            beacon[0] as f64 - 2.5,
            beacon[1] as f64,
            beacon[2] as f64 + 0.5
        ))
        .await;
        p.pump(500).await;
        let speed = effect("minecraft:speed") as i16 + 1;
        assert!(p.open(beacon).await, "маяк не загрузился из NBT");
        assert_eq!(p.data.get(&0).copied(), Some(4), "уровень из NBT");
        assert_eq!(
            p.data.get(&1).copied(),
            Some(speed),
            "основной эффект из NBT"
        );
        assert_eq!(p.data.get(&2).copied(), Some(speed), "второй эффект из NBT");
        println!("beacon: ok (nbt)");
        return;
    }
    p.cmd("gamemode creative").await;
    p.cmd("difficulty peaceful").await;
    p.cmd("time set day").await;
    p.cmd("clear @s").await;
    // Прошлый маяк мог остаться в другом месте: снимаем все маяки рядом.
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air replace minecraft:beacon",
        x - 20,
        y - 10,
        z - 20,
        x + 20,
        y + 10,
        z + 20
    ))
    .await;
    // Чистим и прошлый маяк (он мог остаться в другом месте), и текущее место.
    let sites: Vec<[i32; 3]> = saved_pos()
        .into_iter()
        .chain(std::iter::once(beacon))
        .collect();
    for site in sites {
        p.clean_site(site).await;
    }
    let _ = std::fs::write(
        POS_FILE,
        format!("{} {} {}", beacon[0], beacon[1], beacon[2]),
    );
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:iron_block",
        beacon[0] - 1,
        beacon[1] - 1,
        beacon[2] - 1,
        beacon[0] + 1,
        beacon[1] - 1,
        beacon[2] + 1
    ))
    .await;
    p.cmd(&format!(
        "setblock {} {} {} minecraft:beacon",
        beacon[0], beacon[1], beacon[2]
    ))
    .await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:iron_ingot 1")
        .await;
    let mut w = Writer::new();
    w.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&w.buf).await.unwrap();
    p.pump(150).await;

    assert!(p.open(beacon).await, "окно маяка не открылось");
    assert_eq!(p.data.get(&0).copied(), Some(1), "пирамида не найдена");
    p.click(0, 0, SWAP).await;
    assert!(
        p.slots
            .first()
            .copied()
            .flatten()
            .is_some_and(|s| s.item == item("minecraft:iron_ingot")),
        "оплата не попала в слот"
    );

    let speed = effect("minecraft:speed");
    let mut w = p.packet(p.sb_set_beacon);
    w.bool(true).var_int(speed).bool(false);
    p.c.send_raw(&w.buf).await.unwrap();
    p.pump(300).await;
    assert_eq!(
        p.data.get(&1).copied(),
        Some(speed as i16 + 1),
        "эффект не выбран"
    );
    assert!(
        p.slots.first().copied().flatten().is_none(),
        "оплата не списалась"
    );

    // Эффект выдаётся раз в 80 тиков; ждём до 5 секунд.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        p.pump(200).await;
        if p.effects
            .iter()
            .any(|&(id, amp, dur)| id == speed && amp == 0 && dur == 220)
        {
            break;
        }
    }
    assert!(
        p.effects
            .iter()
            .any(|&(id, amp, dur)| id == speed && amp == 0 && dur == 220),
        "эффект скорости не пришёл: {:?}",
        p.effects
    );

    // 4 уровня: второй эффект и усиление одинаковой пары до II.
    p.close().await;
    for step in 2..=4 {
        p.cmd(&format!(
            "fill {} {} {} {} {} {} minecraft:iron_block",
            beacon[0] - step,
            beacon[1] - step,
            beacon[2] - step,
            beacon[0] + step,
            beacon[1] - step,
            beacon[2] + step
        ))
        .await;
    }
    p.cmd("item replace entity @s hotbar.0 with minecraft:iron_ingot 1")
        .await;
    let mut w = Writer::new();
    w.var_int(sb("set_carried_item")).u16(0);
    p.c.send_raw(&w.buf).await.unwrap();
    p.pump(150).await;
    assert!(p.open(beacon).await, "окно маяка не открылось снова");
    assert_eq!(p.data.get(&0).copied(), Some(4), "пирамида не доросла до 4");
    p.click(0, 0, SWAP).await;
    p.effects.clear();

    let mut w = p.packet(p.sb_set_beacon);
    w.bool(true).var_int(speed).bool(true).var_int(speed);
    p.c.send_raw(&w.buf).await.unwrap();
    p.pump(300).await;
    assert_eq!(p.data.get(&1).copied(), Some(speed as i16 + 1));
    assert_eq!(p.data.get(&2).copied(), Some(speed as i16 + 1));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        p.pump(200).await;
        if p.effects
            .iter()
            .any(|&(id, amp, dur)| id == speed && amp == 1 && dur == 340)
        {
            break;
        }
    }
    assert!(
        p.effects
            .iter()
            .any(|&(id, amp, dur)| id == speed && amp == 1 && dur == 340),
        "усиленная скорость не пришла: {:?}",
        p.effects
    );
    p.cmd("save-all").await;
    println!("beacon: ok");
}
