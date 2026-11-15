//! Рейд: знамя капитана у лидера волны, Bad Omen при его убийстве и
//! лечение ведьмой раненого союзного разбойника.
//!
//! `raid_captain_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::HashMap;
use std::time::Duration;

const X: i32 = 2688;
const Z: i32 = 8;
const Y: i32 = 100;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_attack: i32,
    cb_add: i32,
    cb_meta: i32,
    cb_equipment: i32,
    cb_effect: i32,
    cb_sound: i32,
    cb_remove: i32,
    cb_chat: i32,
    chat: String,
    pillager_type: i32,
    banner_item: i32,
    bad_omen_id: i32,
    witch_throw_sound: i32,
    mobs: HashMap<i32, [f64; 3]>,
    /// Все известные пиллагеры (id) и их здоровье из метаданных.
    pillagers: HashMap<i32, f32>,
    leader: Option<i32>,
    bad_omen: Option<(i32, i32)>,
    witch_throw: bool,
    witch_pillager: Option<i32>,
    witch_health: f32,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            let mut r = Reader::new(&body);
            if id == self.cb_add {
                let (Ok(entity), Ok(_uuid)) = (r.var_int(), r.bytes(16)) else {
                    continue;
                };
                let Ok(kind) = r.var_int() else { continue };
                let (Ok(x), Ok(y), Ok(z)) = (r.f64(), r.f64(), r.f64()) else {
                    continue;
                };
                self.mobs.insert(entity, [x, y, z]);
                if kind == self.pillager_type {
                    self.pillagers.entry(entity).or_insert(0.0);
                }
            } else if id == self.cb_meta {
                let Ok(entity) = r.var_int() else { continue };
                while let Ok(index) = r.u8() {
                    let Ok(kind) = r.var_int() else { break };
                    match kind {
                        0 => {
                            if r.u8().is_err() {
                                break;
                            }
                        }
                        1 => {
                            if r.var_int().is_err() {
                                break;
                            }
                        }
                        3 => {
                            let Ok(v) = r.f32() else { break };
                            if index == 9 {
                                if Some(entity) == self.witch_pillager {
                                    self.witch_health = self.witch_health.max(v);
                                }
                                if self.pillagers.contains_key(&entity) {
                                    self.pillagers.insert(entity, v);
                                }
                            }
                        }
                        8 => {
                            if r.bool().is_err() {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
            } else if id == self.cb_equipment {
                let Ok(entity) = r.var_int() else { continue };
                while let Ok(slot) = r.u8() {
                    let Ok(stack) = chunkwake_protocol::inventory::read_stack(&mut r) else {
                        break;
                    };
                    if slot & 0x7F == 4
                        && let Some(banner) = stack
                        && banner.item == self.banner_item
                    {
                        self.leader = Some(entity);
                    }
                    if slot & 0x80 == 0 {
                        break;
                    }
                }
            } else if id == self.cb_effect {
                let (Ok(_entity), Ok(effect)) = (r.var_int(), r.var_int()) else {
                    continue;
                };
                if effect == self.bad_omen_id
                    && let (Ok(amplifier), Ok(duration)) = (r.var_int(), r.var_int())
                {
                    self.bad_omen = Some((amplifier, duration));
                }
            } else if id == self.cb_sound && r.var_int().ok() == Some(self.witch_throw_sound + 1) {
                self.witch_throw = true;
            } else if id == self.cb_chat {
                if let Ok(component) = text::read_component(&mut r) {
                    flatten(&component, &mut self.chat);
                    self.chat.push(' ');
                }
            } else if id == self.cb_remove {
                let count = r.var_int().unwrap_or(0);
                for _ in 0..count {
                    let Ok(entity) = r.var_int() else { break };
                    self.mobs.remove(&entity);
                    self.pillagers.remove(&entity);
                }
            }
        }
    }

    fn apply(&mut self, ev: Event) {
        match ev {
            Event::EntityMoved {
                entity_id, delta, ..
            } => {
                if let Some(pos) = self.mobs.get_mut(&entity_id) {
                    pos[0] += delta[0];
                    pos[1] += delta[1];
                    pos[2] += delta[2];
                }
            }
            Event::EntityRemoved(ids) => {
                for id in ids {
                    self.mobs.remove(&id);
                    self.pillagers.remove(&id);
                }
            }
            _ => {}
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
            self.apply(ev);
        }
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
    }

    async fn hit(&mut self, target: i32) {
        let mut w = self.packet(self.sb_attack);
        w.var_int(target);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(1100).await;
    }

    /// Один телепорт к лидеру и серия ударов топором (иначе каждый
    /// телепорт включает load-gate и удары теряются).
    async fn kill(&mut self, target: i32) {
        let Some(pos) = self.mobs.get(&target).copied() else {
            return;
        };
        self.cmd(&format!("tp @s {:.2} {:.2} {:.2}", pos[0], pos[1], pos[2]))
            .await;
        self.pump(400).await;
        self.c.confirm_loaded().await.unwrap();
        self.pump(300).await;
        for _ in 0..8 {
            self.hit(target).await;
            if !self.mobs.contains_key(&target) {
                break;
            }
        }
    }
}

fn flatten(v: &serde_json::Value, out: &mut String) {
    if let Some(s) = v.as_str() {
        out.push_str(s);
        return;
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(s);
    }
    for key in ["with", "extra"] {
        if let Some(args) = v.get(key).and_then(serde_json::Value::as_array) {
            for arg in args {
                flatten(arg, out);
                out.push(' ');
            }
        }
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
    let pid = |reg_name: &str, name: &str| {
        reg[reg_name]["entries"][name]["protocol_id"]
            .as_i64()
            .unwrap() as i32
    };
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_add = cb("add_entity");
    let cb_meta = cb("set_entity_data");
    let cb_equipment = cb("set_equipment");
    let cb_effect = cb("update_mob_effect");
    let cb_sound = cb("sound");
    let cb_remove = cb("remove_entities");
    let cb_chat = cb("system_chat");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![
        cb_add,
        cb_meta,
        cb_equipment,
        cb_effect,
        cb_sound,
        cb_remove,
        cb_chat,
    ];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_attack: sb("attack"),
        cb_add,
        cb_meta,
        cb_equipment,
        cb_effect,
        cb_sound,
        cb_remove,
        cb_chat,
        chat: String::new(),
        pillager_type: pid("minecraft:entity_type", "minecraft:pillager"),
        banner_item: pid("minecraft:item", "minecraft:white_banner"),
        bad_omen_id: pid("minecraft:mob_effect", "minecraft:bad_omen"),
        witch_throw_sound: pid("minecraft:sound_event", "minecraft:entity.witch.throw"),
        mobs: HashMap::new(),
        pillagers: HashMap::new(),
        leader: None,
        bad_omen: None,
        witch_throw: false,
        witch_pillager: None,
        witch_health: 0.0,
    };
    p.pump(2000).await;

    for line in [
        "difficulty hard".to_string(),
        "gamemode creative".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!(
            "fill {} 99 {} {} 99 {} minecraft:stone",
            X - 16,
            Z - 16,
            X + 32,
            Z + 24
        ),
        format!(
            "fill {} {Y} {} {} 112 {} minecraft:air",
            X - 16,
            Z - 16,
            X + 32,
            Z + 24
        ),
        format!("tp @s {X}.5 {Y} {Z}.5 180 20"),
        format!("setblock {X} 100 {Z} minecraft:red_bed[part=foot,facing=south]"),
        format!(
            "setblock {X} 100 {} minecraft:red_bed[part=head,facing=south]",
            Z + 1
        ),
        "item replace entity @s weapon.mainhand with minecraft:netherite_axe".to_string(),
        format!("tp @s {X}.5 {Y} {}.5 180 20", Z + 4),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();
    p.cmd("effect give @s minecraft:bad_omen 600 0").await;

    // Первая волна Hard (4+ пиллагера) и её лидер со знаменем.
    for _ in 0..160 {
        p.pump(250).await;
        if p.leader.is_some() {
            break;
        }
    }
    let leader = p.leader.expect("у лидера волны нет знамени в HEAD");
    // `Raid.getBannerComponentPatch`: 8 слоёв — rhombus/cyan,
    // stripe_bottom/light_gray, stripe_center/gray, border/light_gray,
    // stripe_middle/black, half_horizontal/light_gray, circle/light_gray,
    // border/black. Проверяем по выпавшему знамени (серверный NBT):
    // клиент бота не инициализирует таблицу компонентов предметов.
    // Убираем старый Bad Omen (мог остаться с прошлой сессии), чтобы
    // пакет от убийства капитана был новым.
    p.cmd("effect clear @s").await;
    p.bad_omen = None;
    p.kill(leader).await;
    for _ in 0..40 {
        p.pump(100).await;
        if p.bad_omen.is_some() {
            break;
        }
    }
    let (amplifier, duration) = p.bad_omen.expect("за убийство капитана не выдан Bad Omen");
    assert_eq!(amplifier, 0, "у Bad Omen капитана должно быть усиление 0");
    assert!(
        duration > 119_000,
        "длительность Bad Omen не 120000: {duration}"
    );
    println!("raid captain bad omen ok (duration {duration})");

    // Знамя капитана падает всегда (`drop_chances[head] = 2.0`); в его NBT
    // 8 слоёв `minecraft:banner_patterns`.
    let mut banner = String::new();
    for _ in 0..10 {
        p.chat.clear();
        p.cmd("data get entity @e[type=minecraft:item,distance=..16,sort=nearest,limit=1] Item")
            .await;
        banner = p.chat.clone();
        if banner.contains("banner_patterns") {
            break;
        }
        p.pump(200).await;
    }
    assert!(
        banner.contains("banner_patterns"),
        "в NBT выпавшего знамени нет banner_patterns: {banner}"
    );
    for pattern in [
        "rhombus",
        "stripe_bottom",
        "stripe_center",
        "border",
        "stripe_middle",
        "half_horizontal",
        "circle",
    ] {
        assert!(
            banner.contains(pattern),
            "в NBT знамени нет узора {pattern}: {banner}"
        );
    }
    assert!(
        banner.contains("cyan")
            && banner.contains("light_gray")
            && banner.contains("gray")
            && banner.contains("black"),
        "в NBT знамени нет ожидаемых цветов: {banner}"
    );
    let layers = banner.matches("pattern:").count();
    assert_eq!(layers, 8, "слоёв знамени не 8: {banner}");
    println!("raid captain banner ok (entity {leader}, {layers} layers, NBT names)");

    // Ведьма лечит раненого разбойника: HP 3 → 7 от splash healing.
    p.witch_pillager = None;
    let known: std::collections::HashSet<i32> = p.pillagers.keys().copied().collect();
    let px = format!("{}.5", X + 4);
    let wx = format!("{}.5", X + 8);
    p.cmd(&format!(
        "summon minecraft:pillager {px} {Y} {}.5 {{Health:3f,PersistenceRequired:1b}}",
        Z
    ))
    .await;
    // Запоминаем только что созданного пиллагера (новый id).
    if let Some(&id) = p.pillagers.keys().find(|id| !known.contains(id)) {
        p.witch_pillager = Some(id);
        p.witch_health = 3.0;
    }
    p.cmd(&format!(
        "summon minecraft:witch {wx} {Y} {}.5 {{PersistenceRequired:1b}}",
        Z
    ))
    .await;
    for _ in 0..40 {
        p.pump(150).await;
        if p.witch_throw && p.witch_health >= 6.0 {
            break;
        }
    }
    assert!(p.witch_throw, "ведьма не бросила зелье лечения");
    assert!(
        p.witch_health >= 6.0,
        "пиллагер не вылечен: HP {}",
        p.witch_health
    );
    println!("witch heal ok (pillager HP {})", p.witch_health);
    println!("raid_captain_probe: ok");
}
