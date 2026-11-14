//! Рейд на Hard: волны содержат всех разбойников (`Raid.RaiderType`),
//! убийство разбойника игроком делает его героем, победа выдаёт
//! `hero_of_the_village`, `raid_win` и достижение.
//!
//! `raid_groups_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const X: i32 = 2304;
const Z: i32 = 8;

const RAIDERS: [&str; 5] = [
    "minecraft:pillager",
    "minecraft:vindicator",
    "minecraft:evoker",
    "minecraft:witch",
    "minecraft:ravager",
];

struct Probe {
    c: Client,
    sb_command: i32,
    sb_attack: i32,
    cb_chat: i32,
    cb_boss: i32,
    cb_effect: i32,
    /// protocol id `minecraft:hero_of_the_village`.
    hero_effect_id: i32,
    /// Усиление и длительность полученного эффекта героя.
    hero_effect: Option<(i32, i32)>,
    custom_kind: i32,
    raid_win: i32,
    raid_trigger: i32,
    trigger: i32,
    win: i32,
    trigger0: i32,
    win0: i32,
    /// protocol id вида → короткое имя.
    kinds: HashMap<i32, &'static str>,
    seen: HashSet<&'static str>,
    /// id сущности → последняя известная позиция.
    mobs: HashMap<i32, [f64; 3]>,
    hero_adv: bool,
    bar_added: bool,
    bar_removed: bool,
    chat: String,
}

fn flatten(v: &serde_json::Value, out: &mut String) {
    if let Some(s) = v.as_str() {
        out.push_str(s);
        return;
    }
    if let Some(s) = v.get("text").and_then(serde_json::Value::as_str) {
        out.push_str(s);
    }
    if let Some(args) = v.get("with").and_then(serde_json::Value::as_array) {
        for arg in args {
            flatten(arg, out);
            out.push(' ');
        }
    }
    if let Some(args) = v.get("extra").and_then(serde_json::Value::as_array) {
        for arg in args {
            flatten(arg, out);
        }
    }
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn apply(&mut self, ev: Event) {
        match ev {
            Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            } => {
                if let Some(name) = self.kinds.get(&kind).copied() {
                    self.seen.insert(name);
                    self.mobs.insert(entity_id, pos);
                }
            }
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
                }
            }
            Event::Stats { stats } => {
                for ((kind, value), n) in stats {
                    if kind != self.custom_kind {
                        continue;
                    }
                    if value == self.raid_trigger {
                        self.trigger = n;
                    } else if value == self.raid_win {
                        self.win = n;
                    }
                }
            }
            Event::Advancements { added, .. }
                if added
                    .iter()
                    .any(|a| a == "minecraft:adventure/hero_of_the_village") =>
            {
                self.hero_adv = true;
            }
            _ => {}
        }
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    flatten(&component, &mut self.chat);
                    self.chat.push('\n');
                }
            } else if id == self.cb_boss {
                let mut r = Reader::new(&body);
                if r.bytes(16).is_err() {
                    continue;
                }
                match r.var_int() {
                    Ok(0) => self.bar_added = true,
                    Ok(1) => self.bar_removed = true,
                    _ => {}
                }
            } else if id == self.cb_effect {
                let mut r = Reader::new(&body);
                let (Ok(_entity), Ok(effect)) = (r.var_int(), r.var_int()) else {
                    continue;
                };
                if effect == self.hero_effect_id
                    && let (Ok(amplifier), Ok(duration)) = (r.var_int(), r.var_int())
                {
                    self.hero_effect = Some((amplifier, duration));
                }
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

    async fn attack(&mut self, target: i32) {
        let mut w = self.packet(self.sb_attack);
        w.var_int(target);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(1100).await;
    }

    /// Телепорт к известной позиции моба и удар топором.
    async fn hit(&mut self, target: i32) {
        let Some(pos) = self.mobs.get(&target).copied() else {
            return;
        };
        self.cmd(&format!("tp @s {:.2} {:.2} {:.2}", pos[0], pos[1], pos[2]))
            .await;
        self.c.confirm_loaded().await.unwrap();
        self.pump(120).await;
        self.attack(target).await;
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
    let mut kinds: HashMap<i32, &'static str> = HashMap::new();
    for name in RAIDERS {
        kinds.insert(pid("minecraft:entity_type", name), name);
    }
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_chat = cb("system_chat");
    let cb_boss = cb("boss_event");
    let cb_effect = cb("update_mob_effect");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_chat, cb_boss, cb_effect];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_attack: sb("attack"),
        cb_chat,
        cb_boss,
        cb_effect,
        hero_effect_id: pid("minecraft:mob_effect", "minecraft:hero_of_the_village"),
        hero_effect: None,
        custom_kind: pid("minecraft:stat_type", "minecraft:custom"),
        raid_win: pid("minecraft:custom_stat", "minecraft:raid_win"),
        raid_trigger: pid("minecraft:custom_stat", "minecraft:raid_trigger"),
        trigger: 0,
        win: 0,
        trigger0: 0,
        win0: 0,
        kinds,
        seen: HashSet::new(),
        mobs: HashMap::new(),
        hero_adv: false,
        bar_added: false,
        bar_removed: false,
        chat: String::new(),
    };
    p.pump(2000).await;

    for line in [
        "difficulty hard".to_string(),
        "gamemode survival".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..96]".to_string(),
        format!("execute in minecraft:overworld run tp @s {X}.5 100 11.5 180 20"),
        format!(
            "fill {} 99 {} {} 104 {} minecraft:stone",
            X - 16,
            Z - 16,
            X + 16,
            Z + 24
        ),
        format!(
            "fill {} 100 {} {} 104 {} minecraft:air",
            X - 16,
            Z - 16,
            X + 16,
            Z + 24
        ),
        format!("setblock {X} 100 {Z} minecraft:red_bed[part=foot,facing=south]"),
        format!(
            "setblock {X} 100 {} minecraft:red_bed[part=head,facing=south]",
            Z + 1
        ),
        "item replace entity @s weapon.mainhand with minecraft:netherite_axe".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();
    p.trigger0 = p.trigger;
    p.win0 = p.win;
    p.chat.clear();
    p.cmd("effect give @s minecraft:bad_omen 600 0").await;

    // Старт рейда: ждём `raid_trigger` и первого разбойника.
    for _ in 0..120 {
        p.pump(250).await;
        if p.trigger > p.trigger0 && !p.mobs.is_empty() {
            break;
        }
    }
    assert!(p.trigger > p.trigger0, "raid_trigger не пришёл");
    println!("raid_trigger ok");

    // Герой: игрок должен лично добить хотя бы одного разбойника.
    let victim = *p.mobs.keys().next().expect("разбойник не появился");
    for _ in 0..8 {
        p.hit(victim).await;
        if !p.mobs.contains_key(&victim) {
            break;
        }
    }
    assert!(
        !p.mobs.contains_key(&victim),
        "не удалось добить разбойника игроком"
    );
    println!("hero kill ok");

    // Все волны Hard (7): зачищаем каждую и ждём `raid_win`.
    for _ in 0..240 {
        if p.win > p.win0 {
            break;
        }
        for kind in RAIDERS {
            p.cmd(&format!("kill @e[type={kind},distance=..96]")).await;
        }
        p.pump(300).await;
    }
    assert!(
        p.win > p.win0,
        "raid_win не пришёл (герой не получил победу)"
    );
    for name in RAIDERS {
        assert!(
            p.seen.contains(name),
            "в волнах не появился {name}: {:?}",
            p.seen
        );
    }
    println!("raid waves ok: {:?}", {
        let mut v: Vec<_> = p.seen.iter().copied().collect();
        v.sort_unstable();
        v
    });

    p.pump(400).await;
    assert!(p.hero_adv, "достижение hero_of_the_village не выдано");
    println!("hero advancement ok");

    let (amplifier, duration) = p.hero_effect.expect("эффект hero_of_the_village не пришёл");
    assert_eq!(amplifier, 0, "усиление героя должно быть 0 при Bad Omen I");
    assert!(
        duration > 46_000,
        "длительность эффекта героя не 48000: {duration}"
    );
    println!("hero effect ok (duration {duration})");

    // Бар «победа» держится 600 тиков, затем снимается.
    for _ in 0..160 {
        if p.bar_removed {
            break;
        }
        p.pump(250).await;
    }
    assert!(p.bar_added, "босс-бар рейда не появился");
    assert!(p.bar_removed, "босс-бар не снят после победы");
    println!("raid boss bar ok");
    println!("raid_groups_probe: ok");
}
