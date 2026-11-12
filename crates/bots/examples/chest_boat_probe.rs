//! Лодка с сундуком: окно, перенос предмета, сохранение и дроп.
//!
//! `chest_boat_probe <addr> <packets.json> put|check`
//! `put` кладёт алмаз в лодку и сохраняет мир, `check` (после перезапуска)
//! ломает лодку и ждёт два дропа: саму лодку и алмаз.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::collections::HashMap;
use std::time::{Duration, Instant};

struct Probe {
    c: Client,
    table: PacketTable,
    seen: HashMap<i32, (i32, [f64; 3])>,
    removed: Vec<i32>,
}

impl Probe {
    fn sb(&self, name: &str) -> i32 {
        self.table
            .id(State::Play, Direction::Serverbound, name)
            .unwrap()
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while let Ok(Ok(event)) =
            tokio::time::timeout_at(deadline.into(), self.c.next_event()).await
        {
            match event {
                Event::EntitySpawned {
                    entity_id,
                    kind,
                    pos,
                } => {
                    self.seen.insert(entity_id, (kind, pos));
                }
                Event::EntityRemoved(ids) => self.removed.extend(ids),
                _ => {}
            }
        }
    }

    async fn command(&mut self, line: String) {
        let mut w = Writer::new();
        w.var_int(self.sb("chat_command")).string(&line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(350).await;
    }

    async fn interact(&mut self, entity: i32, sneaking: bool) {
        let mut w = Writer::new();
        w.var_int(self.sb("interact"))
            .var_int(entity)
            .var_int(0)
            .u8(0)
            .bool(sneaking);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(450).await;
    }

    async fn shift_click(&mut self, window: i32, slot: i16) {
        let mut w = Writer::new();
        w.var_int(self.sb("container_click"))
            .var_int(window)
            .var_int(0)
            .u16(slot as u16)
            .u8(0)
            .var_int(1)
            .var_int(0)
            .bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    async fn close(&mut self, window: i32) {
        let mut w = Writer::new();
        w.var_int(self.sb("container_close")).var_int(window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    fn select(&self, slot: u16) -> Writer {
        let mut w = Writer::new();
        w.var_int(self.sb("set_carried_item")).u16(slot);
        w
    }

    async fn select_slot(&mut self, slot: u16) {
        let w = self.select(slot);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn smash(&mut self, entity: i32) -> usize {
        let at = self.seen.get(&entity).map(|s| s.1).unwrap_or_default();
        let before: Vec<i32> = self.seen.keys().copied().collect();
        for _ in 0..8 {
            let mut w = Writer::new();
            w.var_int(self.sb("attack")).var_int(entity);
            self.c.send_raw(&w.buf).await.unwrap();
            // 200 мс ≈ 4 тика: заряд удара и спад урона лодки.
            self.pump(200).await;
        }
        self.pump(1000).await;
        self.seen
            .iter()
            .filter(|(id, s)| !before.contains(id) && (0..3).all(|i| (s.1[i] - at[i]).abs() < 2.0))
            .count()
    }

    fn kind(&self, name: &str) -> i32 {
        self.registries()["minecraft:entity_type"]["entries"][name]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    }

    fn registries(&self) -> serde_json::Value {
        let path = self.table_path();
        let path = std::path::Path::new(&path);
        serde_json::from_str(
            &std::fs::read_to_string(path.with_file_name("registries.json")).unwrap(),
        )
        .unwrap()
    }

    fn table_path(&self) -> String {
        std::env::args().nth(2).unwrap()
    }

    fn find(&self, name: &str) -> Option<(i32, [f64; 3])> {
        let kind = self.kind(name);
        self.seen
            .iter()
            .filter(|(id, s)| s.0 == kind && !self.removed.contains(id))
            .map(|(id, s)| (*id, s.1))
            .max_by_key(|(id, _)| *id)
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture = table
        .id(State::Play, Direction::Clientbound, "open_screen")
        .ok();
    let _spawn = loop {
        if let Event::Position(p) = c.next_event().await.unwrap() {
            break p;
        }
    };
    let mut p = Probe {
        c,
        table,
        seen: HashMap::new(),
        removed: Vec::new(),
    };
    p.pump(1500).await;
    // Фиксированная площадка: позиция входа у бота — это его сохранённая
    // точка, она меняется от прогона к прогону.
    let base = [1000.5, -60.0, 1000.5];
    let (bx, by, bz) = (1000, -60, 1000);
    let mode = a.get(3).map_or("put", String::as_str);

    if mode == "check" {
        // Игрок после перезапуска стоит на спавне: сначала подойти к лодке,
        // иначе она не попадает в зону отслеживания.
        p.command(format!(
            "tp boater {} {} {}",
            base[0],
            base[1],
            base[2] + 2.0
        ))
        .await;
        p.pump(2500).await;
        let Some((id, at)) = p.find("minecraft:oak_chest_boat") else {
            panic!("лодка с сундуком не пережила перезапуск");
        };
        println!("boat after restart rel {:.1}", at[0] - bx as f64);
        // Урон лодки спадает на 1 за тик: бьём мечом, иначе не набрать 40.
        p.command("give boater minecraft:netherite_sword".into())
            .await;
        p.select_slot(0).await;
        let drops = p.smash(id).await;
        println!("chest_boat: drops {drops}");
        assert_eq!(drops, 2, "ждали лодку и алмаз, выпало {drops}");
        println!("chest_boat: check ok");
        return;
    }

    for line in [
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            bx - 3,
            by,
            bz - 3,
            bx + 3,
            by + 3,
            bz + 3
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            bx - 3,
            by - 1,
            bz - 3,
            bx + 3,
            by - 1,
            bz + 3
        ),
        "difficulty peaceful".into(),
        "gamemode survival".into(),
        "clear boater".into(),
        "give boater minecraft:diamond 1".into(),
        format!("tp boater {} {} {}", base[0], base[1], base[2] + 2.0),
    ] {
        p.command(line).await;
    }
    p.select_slot(0).await;
    p.command(format!(
        "summon minecraft:oak_chest_boat {} {} {}",
        base[0], base[1], base[2]
    ))
    .await;
    p.pump(600).await;
    let Some((boat, _)) = p.find("minecraft:oak_chest_boat") else {
        panic!("лодка не появилась");
    };

    // Шифт+ПКМ открывает сундук: окно 1, слоты 0–26 сундук, 54–62 хотбар.
    let before = p.c.captured.len();
    p.interact(boat, true).await;
    assert!(
        p.c.captured.len() > before,
        "лодка с сундуком не открыла окно"
    );
    p.shift_click(1, 54).await;
    p.close(1).await;
    // Повторное открытие и закрытие: окно не должно падать.
    p.interact(boat, true).await;
    p.close(1).await;
    p.command("save-all".into()).await;
    p.pump(800).await;
    // Мягкая остановка: `Stop-Process` не успевает дописать `entities/*.mca`.
    p.command("stop".into()).await;
    p.pump(1500).await;
    println!("chest_boat: put ok");
}
