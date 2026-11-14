//! Рейд: Bad Omen у кровати запускает волны разбойников, после зачистки всех
//! волн приходят статистики `minecraft:raid_trigger` и `minecraft:raid_win`.
//!
//! `adv_raid_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer};
use std::collections::HashMap;
use std::time::Duration;

const X: i32 = 2176;
const Z: i32 = 8;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_attack: i32,
    cb_boss_event: i32,
    /// protocol id вида разбойника → имя.
    kinds: HashMap<i32, &'static str>,
    /// id сущности → последняя известная позиция.
    mobs: HashMap<i32, [f64; 3]>,
    custom_kind: i32,
    raid_trigger: i32,
    raid_win: i32,
    trigger: i32,
    win: i32,
    /// Значения статистик при входе (могли сохраниться с прошлых запусков).
    trigger0: i32,
    win0: i32,
    bar_added: bool,
    bar_removed: bool,
    bar_progress: f32,
    bar_updates: i32,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
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
        let mut w = self.packet(self.sb_attack);
        w.var_int(target);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(1100).await;
    }

    /// `boss_event`: ADD/REMOVE/UPDATE_PROGRESS (нужен только прогресс).
    fn boss_packet(&mut self, body: &[u8]) {
        let mut r = Reader::new(body);
        if r.bytes(16).is_err() {
            return;
        }
        let Ok(action) = r.var_int() else { return };
        match action {
            0 => {
                let Ok(_name) = chunkwake_protocol::text::read_component(&mut r) else {
                    return;
                };
                if let Ok(progress) = r.f32() {
                    self.bar_added = true;
                    self.bar_progress = self.bar_progress.max(progress);
                }
            }
            1 => self.bar_removed = true,
            2 => {
                if let Ok(progress) = r.f32() {
                    self.bar_progress = self.bar_progress.max(progress);
                    self.bar_updates += 1;
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
            match &ev {
                Event::EntitySpawned {
                    entity_id,
                    kind,
                    pos,
                } => {
                    if self.kinds.contains_key(kind) {
                        self.mobs.insert(*entity_id, *pos);
                    }
                }
                Event::EntityMoved {
                    entity_id, delta, ..
                } => {
                    if let Some(pos) = self.mobs.get_mut(entity_id) {
                        pos[0] += delta[0];
                        pos[1] += delta[1];
                        pos[2] += delta[2];
                    }
                }
                Event::EntityRemoved(ids) => {
                    for id in ids {
                        self.mobs.remove(id);
                    }
                }
                _ => {}
            }
            if let Event::Stats { stats } = ev {
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
        }
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_boss_event {
                self.boss_packet(&body);
            }
        }
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(180).await;
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
    let custom_kind = pid("minecraft:stat_type", "minecraft:custom");
    let raid_trigger = pid("minecraft:custom_stat", "minecraft:raid_trigger");
    let raid_win = pid("minecraft:custom_stat", "minecraft:raid_win");
    let ids = Ids::load(&table).unwrap();
    let sb = |n| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let cb = |n| table.id(State::Play, Direction::Clientbound, n).unwrap();
    let cb_boss_event = cb("boss_event");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    c.capture_also = vec![cb_boss_event];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_boss_event,
        custom_kind,
        sb_attack: sb("attack"),
        kinds: {
            let mut m = HashMap::new();
            for name in [
                "minecraft:pillager",
                "minecraft:vindicator",
                "minecraft:evoker",
                "minecraft:witch",
                "minecraft:ravager",
            ] {
                m.insert(pid("minecraft:entity_type", name), name);
            }
            m
        },
        mobs: HashMap::new(),
        raid_trigger,
        raid_win,
        trigger: 0,
        win: 0,
        trigger0: 0,
        win0: 0,
        bar_added: false,
        bar_removed: false,
        bar_progress: 0.0,
        bar_updates: 0,
    };
    p.pump(2000).await;

    let setup: Vec<String> = vec![
        "difficulty easy".into(),
        "gamemode survival".into(),
        "effect clear @s".into(),
        "kill @e[type=!minecraft:player,distance=..64]".into(),
        "execute in minecraft:overworld run tp @s 2176.5 100 11.5 180 20".into(),
        format!(
            "fill {} 99 {} {} 104 {} minecraft:stone",
            X - 12,
            Z - 16,
            X + 12,
            Z + 16
        ),
        format!(
            "fill {} 100 {} {} 104 {} minecraft:air",
            X - 12,
            Z - 16,
            X + 12,
            Z + 16
        ),
        // Кровать-«деревня»: POI `minecraft:home`.
        format!("setblock {X} 100 {Z} minecraft:red_bed[part=foot,facing=south]"),
        format!(
            "setblock {X} 100 {} minecraft:red_bed[part=head,facing=south]",
            Z + 1
        ),
        "item replace entity @s weapon.mainhand with minecraft:netherite_axe".to_string(),
    ];
    for line in setup {
        p.cmd(&line).await;
    }
    // Статистика сохраняется между запусками: считаем только прирост.
    p.trigger0 = p.trigger;
    p.win0 = p.win;
    // Bad Omen даём после снятия базовых значений, иначе рейд успевает
    // начаться прямо во время `pump` предыдущей команды.
    p.cmd("effect give @s minecraft:bad_omen 600 0").await;

    // Ждём старт рейда: статистика `raid_trigger` и первый разбойник.
    for _ in 0..120 {
        p.pump(250).await;
        if p.trigger > p.trigger0 && !p.mobs.is_empty() {
            break;
        }
    }
    assert!(p.trigger > p.trigger0, "raid_trigger не пришёл");
    println!("raid_trigger ok");

    // Герой: игрок лично добивает разбойника (`Raider.die`).
    let victim = *p.mobs.keys().next().expect("разбойник не появился");
    for _ in 0..8 {
        p.hit(victim).await;
        if !p.mobs.contains_key(&victim) {
            break;
        }
    }
    assert!(
        !p.mobs.contains_key(&victim),
        "не удалось добить разбойника"
    );

    // Зачищаем волны (easy = 3), ждём `raid_win`.
    let mut waves = 0;
    for _ in 0..120 {
        if p.win > p.win0 {
            break;
        }
        p.pump(250).await;
        // `kill` пустой селектор безопасен; считаем фактические убийства.
        let before = p.win;
        p.cmd("kill @e[type=!minecraft:player,distance=..64]").await;
        if p.win == before {
            waves += 1;
        }
    }
    assert!(p.win > p.win0, "raid_win не пришёл (волн: {waves})");
    println!("raid_win ok (волн зачищено {waves})");
    // Бар «победа» висит 600 тиков (30 секунд), затем REMOVE.
    for _ in 0..160 {
        if p.bar_removed {
            break;
        }
        p.pump(250).await;
    }
    // Босс-бар `event.minecraft.raid`: ADD → UPDATE_PROGRESS → REMOVE.
    assert!(p.bar_added, "босс-бар рейда не появился");
    assert!(
        p.bar_progress > 0.5,
        "прогресс босс-бара не рос: {}",
        p.bar_progress
    );
    assert!(p.bar_updates > 0, "не было UPDATE_PROGRESS");
    assert!(p.bar_removed, "босс-бар не снят после победы");
    println!(
        "raid boss bar ok (progress {:.2}, updates {})",
        p.bar_progress, p.bar_updates
    );
    println!("adv_raid: ok");
}
