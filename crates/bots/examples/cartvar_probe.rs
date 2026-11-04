//! Вагонетки-виды живым ботом.
//!
//! `place`: воронка-вагонетка под сундуком с алмазами, вагонетка с сундуком
//! (бот кладёт землю и ломает её — выпадает вагонетка и земля), печная
//! (уголь — едет от игрока), ТНТ на включённом активаторе (взрывается через
//! 4 с, рельс цел). `check` (после перезапуска): воронку-вагонетку ломают —
//! выпадают вагонетка и алмазы, то есть инвентарь записан.
//!
//! `cartvar_probe <addr> <packets.json> place|check`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::Duration;

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

struct Probe {
    c: Client,
    table: PacketTable,
    seq: i32,
    /// Сущности: id → (тип, позиция).
    seen: HashMap<i32, (i32, [f64; 3])>,
    removed: Vec<i32>,
    updates: Vec<([i32; 3], i32)>,
    /// Последнее игровое время из `set_time`.
    time: i64,
}

impl Probe {
    fn sb(&self, n: &str) -> i32 {
        self.table.id(State::Play, Direction::Serverbound, n).unwrap()
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {
            match ev {
                Event::EntitySpawned { entity_id, kind, pos } => {
                    let k = if kind >= 0 { kind } else { self.seen.get(&entity_id).map_or(kind, |s| s.0) };
                    self.seen.insert(entity_id, (k, pos));
                }
                Event::EntityMoved { entity_id, delta } => {
                    if let Some(s) = self.seen.get_mut(&entity_id) {
                        for i in 0..3 {
                            s.1[i] += delta[i];
                        }
                    }
                }
                Event::EntityRemoved(ids) => self.removed.extend(ids),
                Event::BlockUpdate(p, s) => self.updates.push((p, s)),
                Event::BlockUpdates(v) => self.updates.extend(v),
                Event::GameTime(t) => self.time = t,
                _ => {}
            }
        }
    }

    /// Ждать `n` игровых тиков (сервер шлёт время раз в 20 тиков), не дольше 120 с.
    async fn ticks(&mut self, n: i64) {
        let start = std::time::Instant::now();
        while self.time == 0 && start.elapsed().as_secs() < 30 {
            self.pump(200).await;
        }
        let until = self.time + n;
        while self.time < until && start.elapsed().as_secs() < 120 {
            self.pump(200).await;
        }
    }

    async fn cmd(&mut self, s: String) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(&s);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn hold(&mut self, slot: i16) {
        let mut w = Writer::new();
        w.var_int(self.sb("set_carried_item")).u16(slot as u16);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(100).await;
    }

    async fn use_on(&mut self, p: [i32; 3]) {
        self.seq += 1;
        let mut w = Writer::new();
        w.var_int(self.sb("use_item_on")).var_int(0).i64(pack(p)).var_int(1).f32(0.5).f32(1.0).f32(0.5).u8(0).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn interact(&mut self, e: i32) {
        let mut w = Writer::new();
        w.var_int(self.sb("interact")).var_int(e).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn shift_click(&mut self, window: i32, slot: i16) {
        let mut w = Writer::new();
        w.var_int(self.sb("container_click")).var_int(window).var_int(0).u16(slot as u16).u8(0).var_int(1).var_int(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
        let mut w = Writer::new();
        w.var_int(self.sb("container_close")).var_int(window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    /// Ломать кулаком (по 10 урона, ломается за 40): предметы, выпавшие рядом.
    async fn smash(&mut self, e: i32) -> usize {
        let at = self.seen.get(&e).map(|s| s.1).unwrap_or_default();
        let before: Vec<i32> = self.seen.keys().copied().collect();
        for _ in 0..6 {
            let mut w = Writer::new();
            w.var_int(self.sb("attack")).var_int(e);
            self.c.send_raw(&w.buf).await.unwrap();
            self.pump(120).await;
        }
        self.pump(800).await;
        let item = self.kind("minecraft:item");
        self.seen
            .iter()
            .filter(|(id, s)| !before.contains(id) && s.0 == item && (0..3).all(|i| (s.1[i] - at[i]).abs() < 2.0))
            .count()
    }

    fn kind(&self, n: &str) -> i32 {
        let dir = std::path::Path::new(self.table_path());
        let reg: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.with_file_name("registries.json")).unwrap()).unwrap();
        reg["minecraft:entity_type"]["entries"][n]["protocol_id"].as_i64().unwrap() as i32
    }

    fn table_path(&self) -> &str {
        Box::leak(std::env::args().nth(2).unwrap().into_boxed_str())
    }

    fn find(&self, n: &str) -> Option<(i32, [f64; 3])> {
        let k = self.kind(n);
        self.seen.iter().filter(|(id, s)| s.0 == k && !self.removed.contains(id)).map(|(id, s)| (*id, s.1)).max_by_key(|(id, _)| *id)
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "carter", ids, 777, "26.3").await.unwrap();
    c.capture = table.id(State::Play, Direction::Clientbound, "open_screen").ok();
    let spawn = loop {
        if let Event::Position(p) = c.next_event().await.unwrap() {
            break p;
        }
    };
    println!("spawn {spawn:?}");
    let mut p = Probe { c, table, seq: 0, seen: HashMap::new(), removed: Vec::new(), updates: Vec::new(), time: 0 };
    p.pump(2000).await;
    let [x, y, z] = spawn.map(|v| v.floor() as i32);
    if a[3] == "check" {
        p.pump(3000).await;
        let Some((h, at)) = p.find("minecraft:hopper_minecart") else {
            println!("hopper cart after restart: none");
            return;
        };
        println!("hopper cart after restart rel {:?}", [at[0] - x as f64, at[1] - y as f64, at[2] - z as f64]);
        if let Some((_, f)) = p.find("minecraft:furnace_minecart") {
            println!("furnace cart after restart rel z {:.2}", f[2] - z as f64);
        }
        p.cmd(format!("tp carter {}.5 {y} {}.5", x + 2, z + 2)).await;
        let drops = p.smash(h).await;
        println!("hopper cart broken: {drops} drops (2 = cart + diamonds)");
        return;
    }
    let (hx, fx, cx, tx) = (x, x + 2, x + 4, x - 10);
    for s in [
        format!("fill {} {y} {} {} {} {} minecraft:air", x - 12, z + 1, x + 6, y + 3, z + 32),
        format!("fill {} {} {} {} {} {} minecraft:stone", x - 12, y - 1, z + 1, x + 6, y - 1, z + 32),
        // Воронка: короткий рельс, сундук над серединой.
        format!("fill {hx} {y} {} {hx} {y} {} minecraft:rail", z + 3, z + 5),
        format!("setblock {hx} {} {} minecraft:chest", y + 1, z + 4),
        // Печная: длинная линия, стена в конце.
        format!("fill {fx} {y} {} {fx} {y} {} minecraft:rail", z + 3, z + 30),
        format!("setblock {fx} {y} {} minecraft:stone", z + 31),
        // Сундук-вагонетка.
        format!("setblock {cx} {y} {} minecraft:rail", z + 4),
        // ТНТ: включённый активатор.
        format!("fill {tx} {y} {} {tx} {y} {} minecraft:rail", z + 3, z + 5),
        format!("setblock {tx} {y} {} minecraft:activator_rail", z + 4),
        "gamerule tnt_explodes true".into(),
        "clear carter".into(),
        "gamemode survival".into(),
        "give carter minecraft:hopper_minecart".into(),
        "give carter minecraft:chest_minecart".into(),
        "give carter minecraft:furnace_minecart".into(),
        "give carter minecraft:tnt_minecart".into(),
        "give carter minecraft:diamond 5".into(),
        "give carter minecraft:coal".into(),
        "give carter minecraft:dirt 3".into(),
        format!("tp carter {}.5 {y} {}.5", x + 2, z + 2),
    ] {
        p.cmd(s).await;
    }
    p.pump(800).await;
    // 1. Алмазы в сундук над рельсом (окно: 0–26 сундук, 54–62 хотбар).
    p.use_on([hx, y + 1, z + 4]).await;
    p.shift_click(1, 58).await;
    // 2. Вагонетки на рельсы.
    p.hold(0).await;
    p.use_on([hx, y, z + 4]).await;
    p.hold(1).await;
    p.use_on([cx, y, z + 4]).await;
    p.hold(2).await;
    p.use_on([fx, y, z + 4]).await;
    p.pump(1500).await;
    let hopper = p.find("minecraft:hopper_minecart");
    let chest = p.find("minecraft:chest_minecart");
    let furnace = p.find("minecraft:furnace_minecart");
    println!("placed hopper {hopper:?} chest {chest:?} furnace {furnace:?}");
    // 3. Сундук-вагонетка: окно, земля внутрь (хотбар 6 → слот 27+27+6), сломать.
    if let Some((id, _)) = chest {
        let before = p.c.captured.len();
        p.interact(id).await;
        println!("chest cart opened: {}", p.c.captured.len() > before);
        p.shift_click(2, 60).await;
        let drops = p.smash(id).await;
        println!("chest cart broken: {drops} drops (2 = cart + dirt)");
    }
    // 4. Воронка-вагонетка: окно открывается.
    if let Some((id, _)) = hopper {
        let before = p.c.captured.len();
        p.interact(id).await;
        println!("hopper cart opened: {}", p.c.captured.len() > before);
        let mut w = Writer::new();
        w.var_int(p.sb("container_close")).var_int(3);
        p.c.send_raw(&w.buf).await.unwrap();
    }
    // 5. Печная: уголь в руке, клик — едет от игрока (+z).
    if let Some((id, at)) = furnace {
        p.hold(5).await;
        p.interact(id).await;
        p.ticks(60).await;
        let now = p.seen.get(&id).map_or(at, |s| s.1);
        println!("furnace cart moved {:.2} blocks along z in ~60 ticks", now[2] - at[2]);
    }
    // 6. ТНТ: на активатор, через 4 с — взрыв, рельс под ней цел.
    p.cmd(format!("tp carter {}.5 {y} {}.5", tx + 2, z + 2)).await;
    p.cmd(format!("setblock {} {y} {} minecraft:redstone_block", tx - 1, z + 4)).await;
    p.hold(3).await;
    p.use_on([tx, y, z + 4]).await;
    p.cmd(format!("tp carter {}.5 {y} {}.5", x + 2, z + 2)).await;
    let tnt = p.find("minecraft:tnt_minecart");
    println!("tnt cart {tnt:?}");
    p.updates.clear();
    p.ticks(110).await;
    if let Some((id, _)) = tnt {
        println!("tnt cart exploded: {}", p.removed.contains(&id));
    }
    let air_near = p.updates.iter().filter(|(q, s)| *s == 0 && (q[0] - tx).abs() <= 6 && (q[2] - z - 4).abs() <= 6).count();
    let rails_lost = p.updates.iter().filter(|(q, s)| *s == 0 && q[0] == tx && q[1] == y && (z + 3..=z + 5).contains(&q[2])).count();
    println!("blocks blown near tnt lane: {air_near}, rails lost: {rails_lost}");
    if let Some((_, at)) = p.find("minecraft:hopper_minecart") {
        println!("hopper cart rel z {:.2}", at[2] - z as f64);
    }
    p.cmd("save-all".into()).await;
    p.pump(1500).await;
}
