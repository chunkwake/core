//! Телепорт не-живых сущностей: стрелы, лодки и вагонетки через `/tp`.
//!
//! `tp_entity_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

struct Probe {
    c: Client,
    sb_command: i32,
    cb_chat: i32,
    chat: String,
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
        while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, self.c.next_event()).await {}
        self.drain();
    }

    async fn cmd(&mut self, line: &str) {
        self.send_cmd(line).await;
        self.pump(350).await;
    }

    async fn send_cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(150).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn pos(&mut self, kind: &str) -> [f64; 3] {
        let query = format!("@e[type={kind},limit=1,sort=nearest] Pos");
        let mut text = self.data(&query).await;
        // Первый ответ может потеряться на холодном сервере — один повтор.
        if vec3(&text).is_none() {
            text = self.data(&query).await;
        }
        vec3(&text).unwrap_or_else(|| panic!("нет Pos у {kind}: {text}"))
    }

    async fn rotation(&mut self, kind: &str) -> [f32; 2] {
        let text = self
            .data(&format!("@e[type={kind},limit=1,sort=nearest] Rotation"))
            .await;
        let v = numbers(&text);
        assert!(v.len() >= 2, "нет Rotation у {kind}: {text}");
        [v[0] as f32, v[1] as f32]
    }
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

fn numbers(text: &str) -> Vec<f64> {
    let body = match (text.find('['), text.rfind(']')) {
        (Some(a), Some(b)) if b > a => &text[a + 1..b],
        _ => return Vec::new(),
    };
    body.split(',')
        .filter_map(|t| {
            t.trim()
                .trim_end_matches('d')
                .trim_end_matches('f')
                .parse()
                .ok()
        })
        .collect()
}

fn last_number(text: &str) -> Option<f64> {
    text.split_whitespace()
        .rev()
        .find_map(|t| t.trim_end_matches(['f', 'b', 's']).parse().ok())
}

fn vec3(text: &str) -> Option<[f64; 3]> {
    let v = numbers(text);
    (v.len() >= 3).then(|| [v[0], v[1], v[2]])
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |name| table.id(State::Play, Direction::Serverbound, name).unwrap();
    let cb = |name| table.id(State::Play, Direction::Clientbound, name).unwrap();
    let mut c = Client::connect(&a[1], "framer", ids, 777, "26.3")
        .await
        .unwrap();
    let spawn = loop {
        if let Event::Position(at) = c.next_event().await.unwrap() {
            break at;
        }
    };
    let cb_chat = cb("system_chat");
    c.capture_also = vec![cb_chat];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        cb_chat,
        chat: String::new(),
    };
    let (x, z) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
    let (bx, bz) = (x + 90, z + 90);
    let by = -60;
    let base = [bx as f64 + 0.5, by as f64, bz as f64 + 0.5];

    p.cmd("gamemode survival").await;
    p.cmd("difficulty peaceful").await;
    p.cmd("time set midnight").await;
    p.cmd("clear @s").await;
    p.cmd("effect clear @s").await;
    p.cmd(&format!(
        "fill {} {} {} {} {} {} minecraft:air",
        bx - 4,
        by,
        bz - 4,
        bx + 4,
        by + 3,
        bz + 6
    ))
    .await;
    p.cmd(&format!("tp @s {} {} {} 0 0", base[0], base[1], base[2]))
        .await;
    // Чистим хвосты прошлых прогонов, чтобы `sort=nearest` не уводил к старым.
    for kind in [
        "minecraft:arrow",
        "minecraft:oak_boat",
        "minecraft:minecart",
        "minecraft:tnt",
        "minecraft:experience_orb",
    ] {
        p.cmd(&format!("kill @e[type={kind}]")).await;
    }
    p.pump(800).await;

    // Стрела: /summon в ванили создаёт projectile, /tp двигает его.
    let arrow_from = [base[0], base[1] + 1.0, base[2] + 3.0];
    let arrow_to = [base[0] + 3.0, base[1] + 1.0, base[2] - 2.0];
    p.cmd(&format!(
        "summon minecraft:arrow {} {} {}",
        arrow_from[0], arrow_from[1], arrow_from[2]
    ))
    .await;
    p.cmd(&format!(
        "tp @e[type=minecraft:arrow,limit=1,sort=nearest] {} {} {}",
        arrow_to[0], arrow_to[1], arrow_to[2]
    ))
    .await;
    let at = p.pos("minecraft:arrow").await;
    assert!(
        (at[0] - arrow_to[0]).abs() < 0.1 && (at[2] - arrow_to[2]).abs() < 0.1,
        "стрела не телепортировалась: {at:?}, ждали {arrow_to:?}"
    );

    // Лодка: позиция и поворот из `/tp ... <yaw> <pitch>`.
    let boat_from = [base[0] - 2.0, base[1], base[2] + 3.0];
    let boat_to = [base[0] - 2.0, base[1], base[2] - 2.0];
    p.cmd(&format!(
        "summon minecraft:oak_boat {} {} {}",
        boat_from[0], boat_from[1], boat_from[2]
    ))
    .await;
    p.cmd(&format!(
        "tp @e[type=minecraft:oak_boat,limit=1,sort=nearest] {} {} {} 90 0",
        boat_to[0], boat_to[1], boat_to[2]
    ))
    .await;
    let at = p.pos("minecraft:oak_boat").await;
    let rot = p.rotation("minecraft:oak_boat").await;
    assert!(
        (at[0] - boat_to[0]).abs() < 0.1 && (at[2] - boat_to[2]).abs() < 0.1,
        "лодка не телепортировалась: {at:?}, ждали {boat_to:?}"
    );
    assert!((rot[0] - 90.0).abs() < 1.0, "лодка не повернулась: {rot:?}");

    // Вагонетка.
    let cart_from = [base[0] + 2.0, base[1], base[2] + 3.0];
    let cart_to = [base[0] + 2.0, base[1], base[2] - 2.0];
    p.cmd(&format!(
        "summon minecraft:minecart {} {} {}",
        cart_from[0], cart_from[1], cart_from[2]
    ))
    .await;
    p.cmd(&format!(
        "tp @e[type=minecraft:minecart,limit=1,sort=nearest] {} {} {}",
        cart_to[0], cart_to[1], cart_to[2]
    ))
    .await;
    let at = p.pos("minecraft:minecart").await;
    assert!(
        (at[0] - cart_to[0]).abs() < 0.1 && (at[2] - cart_to[2]).abs() < 0.1,
        "вагонетка не телепортировалась: {at:?}, ждали {cart_to:?}"
    );

    // ТНТ: /summon с длинным фитилём, затем /tp.
    let tnt_from = [base[0], base[1] + 1.0, base[2] + 3.0];
    let tnt_to = [base[0] - 3.0, base[1] + 1.0, base[2] - 2.0];
    p.cmd(&format!(
        "summon minecraft:tnt {} {} {} {{fuse:600}}",
        tnt_from[0], tnt_from[1], tnt_from[2]
    ))
    .await;
    p.cmd(&format!(
        "tp @e[type=minecraft:tnt,limit=1,sort=nearest] {} {} {}",
        tnt_to[0], tnt_to[1], tnt_to[2]
    ))
    .await;
    let at = p.pos("minecraft:tnt").await;
    assert!(
        (at[0] - tnt_to[0]).abs() < 0.1 && (at[2] - tnt_to[2]).abs() < 0.1,
        "ТНТ не телепортировался: {at:?}, ждали {tnt_to:?}"
    );

    // Сфера опыта: /summon с Value и /tp. Держим обе точки дальше 8 блоков
    // от игрока, иначе орб притягивается (`ExperienceOrb.playerTouch`) и
    // горизонтальная скорость уносит его от точки телепорта (ванильный
    // `TeleportCommand` сохраняет `x`/`z` скорость).
    let orb_from = [base[0] + 1.0, base[1] + 1.0, base[2] + 14.0];
    let orb_to = [base[0] + 1.0, base[1] + 1.0, base[2] + 10.0];
    p.cmd(&format!(
        "summon minecraft:experience_orb {} {} {} {{Value:7}}",
        orb_from[0], orb_from[1], orb_from[2]
    ))
    .await;
    p.cmd(&format!(
        "tp @e[type=minecraft:experience_orb,limit=1,sort=nearest] {} {} {}",
        orb_to[0], orb_to[1], orb_to[2]
    ))
    .await;
    let at = p.pos("minecraft:experience_orb").await;
    assert!(
        (at[0] - orb_to[0]).abs() < 0.1 && (at[2] - orb_to[2]).abs() < 0.1,
        "сфера опыта не телепортировалась: {at:?}, ждали {orb_to:?}"
    );
    let value = p
        .data("@e[type=minecraft:experience_orb,limit=1,sort=nearest] Value")
        .await;
    assert!(
        last_number(&value) == Some(7.0),
        "Value не сохранился: {value}"
    );

    p.cmd("kill @e[type=minecraft:arrow]").await;
    p.cmd("kill @e[type=minecraft:oak_boat]").await;
    p.cmd("kill @e[type=minecraft:minecart]").await;
    p.cmd("kill @e[type=minecraft:tnt]").await;
    p.cmd("kill @e[type=minecraft:experience_orb]").await;
    println!("tp_entity: ok");
}
