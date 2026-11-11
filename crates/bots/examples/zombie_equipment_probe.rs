//! Экипировка зомби: NBT `equipment`/`drop_chances`, `set_equipment`,
//! выпадение при смерти, лук скелета и чары снаряжения при спавне.
//!
//! `zombie_equipment_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_attack: i32,
    sb_set_carried: i32,
    sb_move_pos: i32,
    sb_tick_end: i32,
    cb_chat: i32,
    cb_equipment: i32,
    chat: String,
    /// Сколько раз клиенту пришёл `set_equipment` (для проверки после merge).
    equipment_packets: u32,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_equipment {
                self.equipment_packets += 1;
                continue;
            }
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
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {}
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        self.send_cmd(line).await;
        self.pump(300).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn data(&mut self, query: &str) -> String {
        self.drain();
        self.chat.clear();
        self.pump(120).await;
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn spawn(&mut self, name: &str, kind: i32, at: [f64; 3]) -> i32 {
        self.send_cmd(&format!(
            "summon minecraft:{name} {} {} {}",
            at[0], at[1], at[2]
        ))
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_millis(2000);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.c.next_event()).await {
                Ok(Ok(Event::EntitySpawned {
                    entity_id,
                    kind: k,
                    pos,
                })) if k == kind => {
                    let d = (0..3).map(|i| (pos[i] - at[i]).powi(2)).sum::<f64>();
                    if d <= 2.25 {
                        self.drain();
                        return entity_id;
                    }
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        panic!("моб {name} не появился у {at:?}");
    }

    /// `/kill` оставляет тушу на 20 тиков — ждём, чтобы `@e` её не видел.
    async fn clear(&mut self, name: &str) {
        self.cmd(&format!("kill @e[type=minecraft:{name}]")).await;
        self.pump(1300).await;
    }

    /// Клетка 1×2 вокруг блока: зомби не убегает и не выходит из неё
    /// от отбрасывания, а после смерти игрок встаёт туда же за дропом.
    async fn cage(&mut self, c: [i32; 3]) {
        for dy in 0..2 {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                self.cmd(&format!(
                    "setblock {} {} {} minecraft:stone",
                    c[0] + dx,
                    c[1] + dy,
                    c[2] + dz
                ))
                .await;
            }
        }
    }

    async fn attack(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_attack);
        w.var_int(entity);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn move_ground(&mut self, pos: [f64; 3]) {
        let mut w = self.packet(self.sb_move_pos);
        for v in pos {
            w.f64(v);
        }
        w.u8(1);
        self.c.send_raw(&w.buf).await.unwrap();
        let end = self.packet(self.sb_tick_end);
        self.c.send_raw(&end.buf).await.unwrap();
    }
}

/// Плоский текст компонента: `text`, строки и `with`.
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

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a[3]).unwrap()).unwrap();
    let ty = |n: &str| {
        reg["minecraft:entity_type"]["entries"][n]["protocol_id"]
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
    let cb_chat = cb("system_chat");
    let cb_equipment = cb("set_equipment");
    c.capture_also = vec![cb_chat, cb_equipment];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_attack: sb("attack"),
        sb_set_carried: sb("set_carried_item"),
        sb_move_pos: sb("move_player_pos"),
        sb_tick_end: sb("client_tick_end"),
        cb_chat,
        cb_equipment,
        chat: String::new(),
        equipment_packets: 0,
    };
    let (x, z) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
    let (bx, bz) = (x + 40, z + 40);
    let by = -60;
    let spot = [bx as f64 + 2.5, by as f64, bz as f64 + 0.5];
    let zombie = ty("minecraft:zombie");
    let skeleton = ty("minecraft:skeleton");

    p.cmd("gamemode creative").await;
    p.cmd("difficulty hard").await;
    // `time set` в 26.3 — абсолютные тики: позднее время даёт
    // `specialMultiplier` 0.5 на сложной (максимум для трёх сложностей).
    p.cmd("time set 2000000").await;
    p.cmd("clear @s").await;
    p.cmd("item replace entity @s hotbar.0 with minecraft:diamond_sword 1")
        .await;
    let mut slot = Writer::new();
    slot.var_int(p.sb_set_carried).u16(0);
    p.c.send_raw(&slot.buf).await.unwrap();
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 1,
        by,
        bz - 1,
        bx + 7,
        by + 2,
        bz + 1
    ))
    .await;
    p.cmd(&format!(
        "tp @s {} {} {}",
        bx as f64 + 0.5,
        by as f64,
        bz as f64 + 0.5
    ))
    .await;
    p.move_ground([bx as f64 + 0.5, by as f64, bz as f64 + 0.5])
        .await;
    p.clear("zombie").await;
    p.clear("skeleton").await;

    // Скелет всегда приходит с луком (`AbstractSkeleton...`).
    let sk = [bx as f64 + 6.5, by as f64, bz as f64 + 0.5];
    p.spawn("skeleton", skeleton, sk).await;
    p.pump(200).await;
    let sk_gear = p
        .data("@e[type=minecraft:skeleton,limit=1,sort=nearest] equipment")
        .await;
    assert!(
        sk_gear.contains("minecraft:bow"),
        "скелет без лука: {sk_gear}"
    );

    // `Mob.populateDefaultEquipmentEnchantments`: у скелета всегда лук, шанс
    // чар `0.25 × special` (на сложной с поздним временем — 12.5 %). Перебираем
    // спавны, пока не попадётся зачарованный лук; проверяем, что чары именно
    // из тега `on_mob_spawn_equipment` (у лука это power/punch).
    p.clear("skeleton").await;
    let mut enchanted = None;
    for _ in 0..60 {
        p.send_cmd(&format!(
            "summon minecraft:skeleton {} {} {}",
            sk[0], sk[1], sk[2]
        ))
        .await;
        p.pump(120).await;
        let gear = p
            .data("@e[type=minecraft:skeleton,limit=1,sort=nearest] equipment")
            .await;
        if gear.contains("minecraft:enchantments") {
            enchanted = Some(gear);
            break;
        }
        // `/tp` не берёт не-игрока, поэтому убираем проверенного смертью.
        p.cmd("kill @e[type=minecraft:skeleton,limit=1,sort=nearest]")
            .await;
        p.pump(1100).await;
    }
    let gear = enchanted.expect("60 спавнов скелета без зачарованного лука");
    assert!(
        gear.contains("minecraft:power") || gear.contains("minecraft:punch"),
        "чары не из тега спавна снаряжения: {gear}"
    );
    p.clear("skeleton").await;

    // `/summon <entity> <pos> <nbt>`: NBT применяется, `finalizeSpawn`
    // не вызывается — случайной брони нет, всё из NBT.
    let nb = [bx as f64 + 5.5, by as f64, bz as f64 + 0.5];
    // Крыша: на солнце зомби горит и теряет здоровье до проверки.
    p.cmd(&format!(
        "setblock {} {} {} minecraft:stone",
        bx + 5,
        by + 2,
        bz
    ))
    .await;
    p.send_cmd(&format!(
        "summon minecraft:zombie {} {} {} {}",
        nb[0],
        nb[1],
        nb[2],
        r#"{equipment:{mainhand:{id:"minecraft:netherite_sword",count:1},head:{id:"minecraft:golden_helmet",count:1}},drop_chances:{head:2.0f},Health:7.0f,CustomName:"Summoned"}"#
    ))
    .await;
    p.pump(500).await;
    let gear = p
        .data("@e[type=minecraft:zombie,limit=1,sort=nearest] equipment")
        .await;
    assert!(
        gear.contains("minecraft:netherite_sword") && gear.contains("minecraft:golden_helmet"),
        "NBT summon не применил equipment: {gear}"
    );
    assert!(
        !gear.contains("chestplate") && !gear.contains("leggings") && !gear.contains("boots"),
        "NBT summon получил случайную броню: {gear}"
    );
    let hp = p
        .data("@e[type=minecraft:zombie,limit=1,sort=nearest] Health")
        .await;
    assert!(hp.contains("7.0"), "NBT summon не применил Health: {hp}");
    let name = p
        .data("@e[type=minecraft:zombie,limit=1,sort=nearest] CustomName")
        .await;
    assert!(
        name.to_lowercase().contains("summoned"),
        "NBT summon не применил CustomName: {name}"
    );
    let chances = p
        .data("@e[type=minecraft:zombie,limit=1,sort=nearest] drop_chances")
        .await;
    assert!(
        chances.contains("head:2f"),
        "NBT summon не применил drop_chances: {chances}"
    );
    p.cmd(&format!(
        "kill @e[type=minecraft:zombie,x={},y={},z={},distance=..2]",
        nb[0], nb[1], nb[2]
    ))
    .await;
    p.pump(1400).await;

    // Зомби в клетке: NBT-экипировка и шансы выпадения через `/data merge`.
    p.cage([bx + 2, by, bz]).await;
    let zid = p.spawn("zombie", zombie, spot).await;
    p.pump(200).await;
    p.drain();
    p.c.captured_by.clear();
    p.equipment_packets = 0;
    p.cmd(
        "data merge entity @e[type=minecraft:zombie,limit=1,sort=nearest] {equipment:{mainhand:{id:\"minecraft:iron_sword\",count:1},head:{id:\"minecraft:diamond_helmet\",count:1}},drop_chances:{mainhand:2.0f,head:0.0f}}",
    )
    .await;
    let equipment = p
        .data("@e[type=minecraft:zombie,limit=1,sort=nearest] equipment")
        .await;
    assert!(
        equipment.contains("minecraft:iron_sword")
            && equipment.contains("minecraft:diamond_helmet"),
        "equipment не сохранился: {equipment}"
    );
    let chances = p
        .data("@e[type=minecraft:zombie,limit=1,sort=nearest] drop_chances")
        .await;
    assert!(
        chances.contains("mainhand:2f") && chances.contains("head:0f"),
        "drop_chances не сохранились: {chances}"
    );
    // Клиентам ушёл `set_equipment` после смены снаряжения.
    assert!(
        p.equipment_packets > 0,
        "клиенту не отправлен set_equipment"
    );

    // Убийство игроком: меч с шансом 2.0 выпадает, шлем с 0.0 — нет.
    for _ in 0..8 {
        p.attack(zid).await;
        p.pump(650).await;
    }
    p.pump(800).await;
    p.cmd(&format!("tp @s {} {} {}", spot[0], spot[1], spot[2]))
        .await;
    p.pump(1500).await;
    // Предмет выпадает с задержкой подбора 10 тиков и сразу подбирается.
    p.pump(6000).await;
    // `data get entity @s` читает `live`; сервер публикует актуальные слоты
    // перед каждой командой, так что предмет из подбора уже виден.
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("minecraft:iron_sword"),
        "меч не выпал и не подобрался: {inv}"
    );
    assert!(
        !inv.contains("minecraft:diamond_helmet"),
        "шлем выпал, хотя шанс 0.0: {inv}"
    );

    println!("zombie_equipment: ok");
}
