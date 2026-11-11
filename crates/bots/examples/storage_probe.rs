//! Эндер-сундук и шалкер-ящики: открытие, перенос предмета, повторное
//! открытие и перенос содержимого шалкера в дроп и обратно.
//!
//! `storage_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::inventory::{self as inv, Slot, Stack};
use chunkwake_protocol::{Reader, Writer};
use std::time::Duration;

const SWAP: i32 = 2;
const THROW: i32 = 4;

fn pack_block([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF)
}

struct Probe {
    c: Client,
    sb_use_item_on: i32,
    sb_click: i32,
    sb_close: i32,
    sb_command: i32,
    cb_open: i32,
    cb_content: i32,
    sequence: i32,
    window: i32,
    state: i32,
    slots: Vec<Slot>,
    dirt: i32,
    item_kind: i32,
    spawned: Vec<(i32, [f64; 3])>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned { kind, pos, .. })) => {
                    self.spawned.push((kind, pos));
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_open {
                let mut r = Reader::new(&body);
                if let Ok(window) = r.var_int() {
                    self.window = window;
                }
            } else if id == self.cb_content {
                let mut r = Reader::new(&body);
                let (Ok(window), Ok(state)) = (r.var_int(), r.var_int()) else {
                    continue;
                };
                let Ok(n) = r.var_int() else { continue };
                let mut slots = Vec::with_capacity(n.clamp(0, 128) as usize);
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
            }
        }
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(350).await;
    }

    async fn open(&mut self, pos: [i32; 3]) -> bool {
        self.sequence += 1;
        let mut w = self.packet(self.sb_use_item_on);
        w.var_int(0)
            .i64(pack_block(pos))
            .var_int(4)
            .f32(0.5)
            .f32(0.5)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.sequence);
        self.c.send_raw(&w.buf).await.unwrap();
        self.window = -1;
        self.slots.clear();
        self.pump(500).await;
        self.window != -1
    }

    async fn place(&mut self, support: [i32; 3]) {
        self.sequence += 1;
        let mut w = self.packet(self.sb_use_item_on);
        w.var_int(0)
            .i64(pack_block(support))
            .var_int(1)
            .f32(0.5)
            .f32(1.0)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.sequence);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
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

    fn slot(&self, index: usize) -> Option<Stack> {
        self.slots.get(index).copied().flatten()
    }

    fn dirt_at(&self, index: usize, count: i32) -> bool {
        self.slot(index).map(|s| (s.item, s.count)) == Some((self.dirt, count))
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let item_kind = reg["minecraft:entity_type"]["entries"]["minecraft:item"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let dirt = reg["minecraft:item"]["entries"]["minecraft:dirt"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
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
    let cb_open = cb("open_screen");
    let cb_content = cb("container_set_content");
    c.capture_also.extend([cb_open, cb_content]);
    let mut p = Probe {
        c,
        sb_use_item_on: sb("use_item_on"),
        sb_click: sb("container_click"),
        sb_close: sb("container_close"),
        sb_command: sb("chat_command"),
        cb_open,
        cb_content,
        sequence: 0,
        window: -1,
        state: 0,
        slots: Vec::new(),
        dirt,
        item_kind,
        spawned: Vec::new(),
    };
    let base = [
        spawn[0].floor() as i32 + 2,
        spawn[1].floor() as i32,
        spawn[2].floor() as i32,
    ];
    let shulker = [base[0] + 2, base[1], base[2]];
    p.cmd("gamemode creative").await;
    p.cmd("clear @s").await;

    // Эндер-сундук: предмет переживает повторное открытие.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:ender_chest",
        base[0], base[1], base[2]
    ))
    .await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:dirt 5")
        .await;
    assert!(p.open(base).await, "эндер-сундук не открылся");
    // Проба запускается повторно: чистим слот от прошлого прогона.
    p.click(0, 1, THROW).await;
    p.click(0, 0, SWAP).await;
    p.close().await;
    assert!(p.open(base).await, "эндер-сундук не открылся снова");
    assert!(p.dirt_at(0, 5), "предмет не сохранился в эндер-сундуке");
    p.close().await;

    // Выходим и заходим снова: `EnderItems` должны пережить playerdata.
    p.cmd("save-all").await;
    drop(p);
    tokio::time::sleep(Duration::from_millis(800)).await;
    let ids = Ids::load(&table).unwrap();
    let mut c2 = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    loop {
        if let Event::Position(_) = c2.next_event().await.unwrap() {
            break;
        }
    }
    c2.capture_also.extend([cb_open, cb_content]);
    p = Probe {
        c: c2,
        sb_use_item_on: sb("use_item_on"),
        sb_click: sb("container_click"),
        sb_close: sb("container_close"),
        sb_command: sb("chat_command"),
        cb_open,
        cb_content,
        sequence: 0,
        window: -1,
        state: 0,
        slots: Vec::new(),
        dirt,
        item_kind,
        spawned: Vec::new(),
    };
    assert!(p.open(base).await, "эндер-сундук не открылся после входа");
    assert!(p.dirt_at(0, 5), "эндер-сундук не пережил переподключение");
    p.close().await;

    // Шалкер: содержимое блока и его дроп.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:shulker_box",
        shulker[0], shulker[1], shulker[2]
    ))
    .await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:dirt 3")
        .await;
    assert!(p.open(shulker).await, "шалкер не открылся");
    p.click(0, 0, SWAP).await;
    p.close().await;
    assert!(p.open(shulker).await, "шалкер не открылся снова");
    assert!(p.dirt_at(0, 3), "предмет не сохранился в шалкере");
    p.close().await;

    // Шалкер в шалкер не кладётся: клик откатывается.
    p.cmd("item replace entity @s hotbar.0 with minecraft:shulker_box 1")
        .await;
    assert!(p.open(shulker).await, "шалкер не открылся для проверки");
    p.click(0, 0, SWAP).await;
    assert!(p.dirt_at(0, 3), "шалкер удалось положить внутрь шалкера");
    p.close().await;

    p.cmd("clear @s").await;
    p.spawned.clear();
    p.cmd(&format!(
        "setblock {} {} {} minecraft:air destroy",
        shulker[0], shulker[1], shulker[2]
    ))
    .await;
    let item_pos = p
        .spawned
        .iter()
        .find(|(kind, _)| *kind == p.item_kind)
        .map(|(_, pos)| *pos)
        .expect("шалкер не выпал предметом");
    p.cmd(&format!(
        "tp @s {} {} {}",
        item_pos[0], item_pos[1], item_pos[2]
    ))
    .await;
    p.pump(900).await;

    // Подобранный шалкер ставим заново и проверяем содержимое.
    let place = [shulker[0] + 2, shulker[1], shulker[2]];
    p.cmd(&format!(
        "tp @s {} {} {}",
        place[0] as f64 - 2.5,
        spawn[1],
        place[2] as f64 + 0.5
    ))
    .await;
    p.place([place[0], place[1] - 1, place[2]]).await;
    assert!(p.open(place).await, "подобранный шалкер не поставился");
    assert!(
        p.dirt_at(0, 3),
        "содержимое не перенеслось из дропа в новый шалкер"
    );

    println!("storage: ok");
}
