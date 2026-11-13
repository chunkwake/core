//! Триггер `summoned_entity` для визера: три черепа визер-скелета на
//! soul sand (`nether/summon_wither`).
//!
//! `adv_wither_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:nether/summon_wither";
const CRITERION: &str = "summoned";
const X: i32 = 1400;
const Y: i32 = 100;
const Z: i32 = 8;

fn pack_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ffffff) << 38) | ((z as i64 & 0x3ffffff) << 12) | (y as i64 & 0xfff)
}

struct Probe {
    c: Client,
    sb_command: i32,
    sb_use_on: i32,
    cb_add_entity: i32,
    wither_type: i32,
    wither: Option<i32>,
    seq: i32,
    adv: HashMap<String, Vec<String>>,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id != self.cb_add_entity {
                continue;
            }
            let mut r = Reader::new(&body);
            let Ok(entity) = r.var_int() else { continue };
            if r.bytes(16).is_err() {
                continue;
            }
            let Ok(ty) = r.var_int() else { continue };
            if ty == self.wither_type {
                self.wither = Some(entity);
            }
        }
    }

    async fn pump(&mut self, ms: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let ev = match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(ev)) => ev,
                Ok(Err(e)) => {
                    eprintln!("event error: {e}");
                    break;
                }
                Err(_) => break,
            };
            if let Event::Advancements {
                reset, progress, ..
            } = ev
            {
                if reset {
                    self.adv.clear();
                }
                for (id, criteria) in progress {
                    if criteria.is_empty() {
                        self.adv.remove(&id);
                    } else {
                        self.adv.entry(id).or_default().extend(criteria);
                    }
                }
            }
        }
        self.drain();
    }

    fn done(&self) -> bool {
        self.adv
            .get(ADV)
            .is_some_and(|v| v.iter().any(|c| c == CRITERION))
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    /// Поставить череп на верхнюю грань блока.
    async fn place_on_top(&mut self, pos: [i32; 3]) {
        self.seq += 1;
        let mut w = self.packet(self.sb_use_on);
        w.var_int(0)
            .i64(pack_pos(pos))
            .var_int(1)
            .f32(0.5)
            .f32(1.0)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(800).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let wither_type = reg["minecraft:entity_type"]["entries"]["minecraft:wither"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_add_entity = cb("add_entity");
    c.capture_also = vec![cb_add_entity];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_use_on: sb("use_item_on"),
        cb_add_entity,
        wither_type,
        wither: None,
        seq: 0,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        // Сначала мирная сложность: ваниль не даёт призвать визера.
        "difficulty peaceful".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "clear @s".into(),
        "time set noon".into(),
        "kill @e[type=!minecraft:player,distance=..64]".into(),
        "fill 1396 99 4 1404 99 12 minecraft:stone".into(),
        "fill 1396 100 4 1404 104 12 minecraft:air".into(),
        "tp @s 1402.5 100 8.5 -90 20".into(),
        // Основание: soul sand снизу и средний ряд.
        format!("setblock {X} {Y} {Z} minecraft:soul_sand"),
        format!("setblock 1399 101 {Z} minecraft:soul_sand"),
        format!("setblock {X} 101 {Z} minecraft:soul_sand"),
        format!("setblock 1401 101 {Z} minecraft:soul_sand"),
        // Два черепа командой, третий поставит игрок.
        format!("setblock 1399 102 {Z} minecraft:wither_skeleton_skull"),
        format!("setblock 1401 102 {Z} minecraft:wither_skeleton_skull"),
        "item replace entity @s hotbar.0 with minecraft:wither_skeleton_skull 3".into(),
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:nether/get_wither_skull".into(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.pump(600).await;

    // Мирная сложность: череп ставится, визера нет.
    p.place_on_top([X, 101, Z]).await;
    assert!(
        p.wither.is_none() && !p.done(),
        "визер призван на мирной сложности: {:?}",
        p.adv.get(ADV)
    );
    println!("peaceful skipped ok");

    // Убираем череп, включаем обычную сложность и ставим снова.
    p.cmd(&format!("setblock {X} 102 {Z} minecraft:air")).await;
    p.cmd("difficulty normal").await;
    p.cmd(&format!("advancement revoke @s only {ADV}")).await;
    p.place_on_top([X, 101, Z]).await;
    p.pump(600).await;

    let wither = p.wither.expect("add_entity визера не пришёл");
    assert!(p.done(), "summon_wither не закрылся: {:?}", p.adv.get(ADV));
    println!("wither entity {wither}");
    println!("adv_wither: ok");
}
