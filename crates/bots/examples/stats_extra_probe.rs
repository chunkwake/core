//! Остальные категории статистики: mined, crafted, broken, picked_up,
//! dropped, killed, killed_by и play_time.
//!
//! `stats_extra_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const BX: i32 = 4400;
const BZ: i32 = 4400;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_action: i32,
    sb_attack: i32,
    sb_click: i32,
    sb_place_recipe: i32,
    sb_set_carried: i32,
    sb_input: i32,
    cb_chat: i32,
    zombie_ty: i32,
    chat: String,
    seq: i32,
    stats: HashMap<(i32, i32), i32>,
    book: Vec<(i32, u8)>,
    adv: HashMap<String, Vec<String>>,
    zombie: Option<i32>,
}

fn flatten(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        return s.to_owned();
    }
    let mut out = String::new();
    if let Some(args) = v.get("with").and_then(serde_json::Value::as_array) {
        for arg in args {
            out.push_str(&flatten(arg));
            out.push(' ');
        }
    }
    out
}

fn pack_pos([x, y, z]: [i32; 3]) -> i64 {
    ((x as i64 & 0x3ffffff) << 38) | ((z as i64 & 0x3ffffff) << 12) | (y as i64 & 0xfff)
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id != self.cb_chat {
                continue;
            }
            let mut r = Reader::new(&body);
            if let Ok(component) = text::read_component(&mut r) {
                self.chat.push_str(&flatten(&component));
                self.chat.push(' ');
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
            match ev {
                Event::Stats { stats } => {
                    for (key, n) in stats {
                        self.stats.insert(key, n);
                    }
                }
                Event::RecipeBookAdd { entries, .. } => self.book.extend(entries),
                Event::Advancements { progress, .. } => {
                    for (id, criteria) in progress {
                        if !criteria.is_empty() {
                            self.adv.entry(id).or_default().extend(criteria);
                        }
                    }
                }
                Event::EntitySpawned {
                    entity_id,
                    kind,
                    pos,
                } if kind == self.zombie_ty
                    && (pos[0] - BX as f64 - 2.5).abs() < 4.0
                    && (pos[2] - BZ as f64 - 0.5).abs() < 4.0 =>
                {
                    self.zombie = Some(entity_id);
                }
                _ => {}
            }
        }
        self.drain();
    }

    fn stat(&self, kind: i32, value: i32) -> i32 {
        self.stats.get(&(kind, value)).copied().unwrap_or(0)
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(350).await;
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_set_carried);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn input(&mut self, flags: u8) {
        let mut w = self.packet(self.sb_input);
        w.u8(flags);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
    }

    async fn action(&mut self, action: i32, pos: [i32; 3]) {
        self.seq += 1;
        let mut w = self.packet(self.sb_action);
        w.var_int(action).i64(pack_pos(pos)).u8(0).var_int(self.seq);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    async fn mine(&mut self, pos: [i32; 3]) {
        self.action(0, pos).await;
        self.pump(700).await;
        self.action(3, pos).await;
    }

    async fn attack(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_attack);
        w.var_int(entity);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(700).await;
    }

    async fn drop_one(&mut self) {
        self.action(5, [0, 0, 0]).await;
        self.pump(600).await;
    }

    async fn place(&mut self, window: i32, display: i32) {
        let mut w = self.packet(self.sb_place_recipe);
        w.var_int(window).var_int(display).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(500).await;
    }

    async fn click(&mut self, window: i32, slot: u16) {
        self.seq += 1;
        let mut w = self.packet(self.sb_click);
        w.var_int(window)
            .var_int(self.seq)
            .u16(slot)
            .u8(0)
            .var_int(0)
            .var_int(0)
            .bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&a[2]).with_file_name("registries.json"))
            .unwrap(),
    )
    .unwrap();
    let entry = |registry: &str, name: &str| {
        reg[registry]["entries"][name]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let stat = |name: &str| entry("minecraft:stat_type", name);
    let item = |name: &str| entry("minecraft:item", name);
    let block = |name: &str| entry("minecraft:block", name);
    let entity = |name: &str| entry("minecraft:entity_type", name);
    let (mined, crafted, broken) = (
        stat("minecraft:mined"),
        stat("minecraft:crafted"),
        stat("minecraft:broken"),
    );
    let (picked, dropped, killed, killed_by) = (
        stat("minecraft:picked_up"),
        stat("minecraft:dropped"),
        stat("minecraft:killed"),
        stat("minecraft:killed_by"),
    );
    let custom = stat("minecraft:custom");
    let cstat = |name: &str| entry("minecraft:custom_stat", name);
    let play_time = cstat("minecraft:play_time");
    let total_time = cstat("minecraft:total_world_time");
    let sneak_time = cstat("minecraft:sneak_time");
    let time_death = cstat("minecraft:time_since_death");
    let deaths = cstat("minecraft:deaths");
    let mob_kills = cstat("minecraft:mob_kills");
    let (stone, planks, pickaxe) = (
        block("minecraft:stone"),
        item("minecraft:oak_planks"),
        item("minecraft:wooden_pickaxe"),
    );
    let zombie = entity("minecraft:zombie");
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_action: sb("player_action"),
        sb_attack: sb("attack"),
        sb_click: sb("container_click"),
        sb_place_recipe: sb("place_recipe"),
        sb_set_carried: sb("set_carried_item"),
        sb_input: sb("player_input"),
        cb_chat,
        zombie_ty: zombie,
        chat: String::new(),
        seq: 0,
        stats: HashMap::new(),
        book: Vec::new(),
        adv: HashMap::new(),
        zombie: None,
    };
    p.pump(2000).await;

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "kill @e[type=minecraft:zombie]".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 6,
            by,
            BZ - 6,
            BX + 6,
            by + 5,
            BZ + 6
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 6,
            by - 1,
            BZ - 6,
            BX + 6,
            by - 1,
            BZ + 6
        ),
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "clear boater".to_string(),
        format!("setblock {} {} {} minecraft:stone", BX + 1, by, BZ),
        "give boater minecraft:wooden_pickaxe[minecraft:damage=58] 1".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.select(0).await;
    let base = p.stats.clone();

    // 0. sneak_time: приседание на 1.2 с.
    let sneak_before = p.stat(custom, sneak_time);
    p.input(0x20).await;
    p.pump(1200).await;
    p.input(0).await;
    assert!(
        p.stat(custom, sneak_time) >= sneak_before + 5,
        "sneak_time не растёт: {} -> {}",
        sneak_before,
        p.stat(custom, sneak_time)
    );
    println!("sneak_time ok");

    // 1. mined + broken: деревянная кирка (damage 58) ломает камень.
    p.mine([BX + 1, by, BZ]).await;
    p.pump(500).await;
    assert!(
        p.stat(mined, stone) > base.get(&(mined, stone)).copied().unwrap_or(0),
        "mined stone не вырос"
    );
    assert!(
        p.stat(broken, pickaxe) > base.get(&(broken, pickaxe)).copied().unwrap_or(0),
        "broken wooden_pickaxe не вырос"
    );
    println!("mined + broken ok");

    // 2. crafted: книга рецептов и доски из бревна.
    p.cmd("recipe take @s minecraft:oak_planks").await;
    p.book.clear();
    p.cmd("recipe give @s minecraft:oak_planks").await;
    let planks_display = p.book.last().map(|e| e.0).expect("рецепт досок не выдан");
    p.cmd("give boater minecraft:oak_log 1").await;
    p.select(0).await;
    let crafted_before = p.stat(crafted, planks);
    p.place(0, planks_display).await;
    p.click(0, 0).await;
    p.click(0, 40).await;
    p.pump(300).await;
    assert!(
        p.stat(crafted, planks) >= crafted_before + 4,
        "crafted oak_planks не вырос: {} -> {}",
        crafted_before,
        p.stat(crafted, planks)
    );
    println!("crafted ok");

    // 3. dropped + picked_up: выбросить и подобрать камень.
    p.cmd("clear boater").await;
    p.cmd("tp boater 4400.5 64 4400.5 0 90").await;
    p.cmd("give boater minecraft:stone 1").await;
    p.select(0).await;
    let drop_before = p.stat(dropped, stone);
    let pick_before = p.stat(picked, stone);
    p.drop_one().await;
    p.pump(3000).await;
    assert!(
        p.stat(dropped, stone) > drop_before,
        "dropped stone не вырос"
    );
    assert!(
        p.stat(picked, stone) > pick_before,
        "picked_up stone не вырос"
    );
    println!("dropped + picked_up ok");

    // 4. killed: зомби с 1 HP.
    p.cmd("clear boater").await;
    p.cmd("give boater minecraft:iron_sword 1").await;
    p.select(0).await;
    p.cmd("tp boater 4401.5 64 4400.5 -90 0").await;
    p.cmd("difficulty normal").await;
    p.cmd("time set midnight").await;
    p.cmd("advancement grant @s only minecraft:adventure/root")
        .await;
    p.zombie = None;
    p.cmd(&format!(
        "summon minecraft:zombie {} {} {} {{Health:1f,NoAI:1b}}",
        BX as f64 + 2.5,
        by as f64,
        BZ as f64 + 0.5
    ))
    .await;
    p.pump(700).await;
    let zombie_id = p.zombie.expect("зомби не появился");
    let killed_before = p.stat(killed, zombie);
    let mob_before = p.stat(custom, mob_kills);
    for _ in 0..8 {
        p.attack(zombie_id).await;
    }
    p.pump(800).await;
    assert!(
        p.stat(killed, zombie) > killed_before,
        "killed zombie не вырос"
    );
    assert!(p.stat(custom, mob_kills) > mob_before, "mob_kills не вырос");
    println!("killed ok");
    assert!(
        p.adv
            .get("minecraft:adventure/kill_a_mob")
            .is_some_and(|v| v.iter().any(|c| c == "minecraft:zombie")),
        "advancement kill_a_mob не получил критерий zombie: {:?}",
        p.adv.get("minecraft:adventure/kill_a_mob")
    );
    println!("player_killed_entity advancement ok");

    // 5. play_time: +~20 тиков за секунду.
    let play_before = p.stat(custom, play_time);
    let total_before = p.stat(custom, total_time);
    p.pump(1000).await;
    let played = p.stat(custom, play_time) - play_before;
    assert!(played >= 10, "play_time не растёт: +{played} за секунду");
    assert!(
        p.stat(custom, total_time) > total_before,
        "total_world_time не растёт"
    );
    println!("play_time ok (+{played})");

    // 6. killed_by + deaths + сброс time_since_death.
    let before = p.stat(killed_by, entity("minecraft:player"));
    let deaths_before = p.stat(custom, deaths);
    let since_before = p.stat(custom, time_death);
    p.cmd("damage @s 100 minecraft:player_attack").await;
    p.pump(1500).await;
    assert!(
        p.stat(killed_by, entity("minecraft:player")) > before,
        "killed_by player не вырос"
    );
    assert_eq!(p.stat(custom, deaths), deaths_before + 1, "deaths не вырос");
    assert!(
        p.stat(custom, time_death) < since_before,
        "time_since_death не сбросился: {} -> {}",
        since_before,
        p.stat(custom, time_death)
    );
    println!("killed_by + deaths + time_since_death ok");

    println!("stats_extra: ok");
}
