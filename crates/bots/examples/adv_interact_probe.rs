//! Триггер `player_interacted_with_entity`: пиглин и золото
//! (`nether/distract_piglin`).
//!
//! `adv_interact_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const ADV: &str = "minecraft:nether/distract_piglin";
const PIGLIN: [f64; 3] = [40.5, 100.0, 8.5];

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_select: i32,
    cb_add_entity: i32,
    piglin_type: i32,
    piglin: Option<i32>,
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
            let (Ok(entity), Ok(_uuid), Ok(ty)) =
                (r.var_int(), r.bytes(16).map(|_| ()), r.var_int())
            else {
                continue;
            };
            if ty == self.piglin_type {
                self.piglin = Some(entity);
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
        self.adv.get(ADV).is_some_and(|v| {
            v.iter()
                .any(|c| c == "distract_piglin" || c == "distract_piglin_directly")
        })
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_select);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(120).await;
    }

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(600).await;
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
    let piglin_type = reg["minecraft:entity_type"]["entries"]["minecraft:piglin"]["protocol_id"]
        .as_i64()
        .unwrap() as i32;
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_add_entity = cb("add_entity");
    c.capture_also = vec![cb_add_entity];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_interact: sb("interact"),
        sb_select: sb("set_carried_item"),
        cb_add_entity,
        piglin_type,
        piglin: None,
        adv: HashMap::new(),
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty normal".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "clear @s".into(),
        "time set noon".into(),
        // Площадка и место для пиглина.
        "fill 36 99 4 44 99 12 minecraft:stone".into(),
        "fill 36 100 4 44 102 12 minecraft:air".into(),
        "tp @s 42.5 100 8.5".into(),
        format!("advancement revoke @s only {ADV}"),
        "advancement grant @s only minecraft:nether/root".into(),
        "item replace entity @s armor.head with minecraft:golden_helmet".into(),
        "item replace entity @s hotbar.0 with minecraft:gold_ingot 4".into(),
        format!(
            "summon minecraft:piglin {} {} {}",
            PIGLIN[0], PIGLIN[1], PIGLIN[2]
        ),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    p.select(0).await;
    p.pump(800).await;
    let piglin = p.piglin.expect("add_entity пиглина не пришёл");
    println!("piglin entity {piglin}");

    // В золотой броне игрок не должен получить достижение.
    p.interact(piglin).await;
    assert!(
        !p.done(),
        "игрок в золотой броне получил достижение: {:?}",
        p.adv.get(ADV)
    );
    println!("gold armor skipped ok");

    // Снимаем броню, даём ещё золото и взаимодействуем снова.
    p.cmd("clear @s minecraft:golden_helmet").await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:gold_ingot 4")
        .await;
    p.select(0).await;
    p.interact(piglin).await;
    p.pump(400).await;
    assert!(
        p.done(),
        "player_interacted_with_entity не сработал: {:?}",
        p.adv.get(ADV)
    );
    println!("distract_piglin ok");
}
