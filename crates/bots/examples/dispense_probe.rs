//! Раздатчик с вагонеткой (рельс впереди) и раздатчик с лодкой (вода под
//! воздухом впереди): бот кладёт предметы шифт-кликом, включает редстоун и
//! печатает появившиеся сущности.
//!
//! `dispense_probe <addr> <packets.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

fn pack(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let sb = |n: &str| table.id(State::Play, Direction::Serverbound, n).unwrap();
    let dir = std::path::Path::new(&a[2]);
    let reg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.with_file_name("registries.json")).unwrap(),
    )
    .unwrap();
    let names: std::collections::HashMap<i32, String> = reg["minecraft:entity_type"]["entries"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (v["protocol_id"].as_i64().unwrap() as i32, k.clone()))
        .collect();
    let mut c = Client::connect(&a[1], "carter", ids, 777, "26.3")
        .await
        .unwrap();
    let spawn = loop {
        if let Event::Position(p) = c.next_event().await.unwrap() {
            break p;
        }
    };
    // Свежий мир: чанки у спавна ещё догенерируются, команды сразу после входа не встанут.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let [x, y, z] = spawn.map(|v| v.floor() as i32);
    let cmd = |s: String| {
        let mut w = Writer::new();
        w.var_int(sb("chat_command")).string(&s);
        w.buf
    };
    let (d1, d2) = ([x, y, z + 3], [x, y, z + 5]);
    for s in [
        format!(
            "fill {} {y} {} {} {} {} minecraft:air",
            x - 1,
            z + 2,
            x + 3,
            y + 2,
            z + 6
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            x - 1,
            y - 1,
            z + 2,
            x + 3,
            y - 1,
            z + 6
        ),
        format!(
            "setblock {} {} {} minecraft:dispenser[facing=east]",
            d1[0], d1[1], d1[2]
        ),
        format!(
            "setblock {} {} {} minecraft:rail[shape=east_west]",
            x + 1,
            y,
            z + 3
        ),
        format!(
            "setblock {} {} {} minecraft:dispenser[facing=east]",
            d2[0], d2[1], d2[2]
        ),
        format!("setblock {} {} {} minecraft:water", x + 1, y - 1, z + 5),
        format!("tp carter {}.5 {y} {}.5", x + 2, z + 4),
        "gamemode survival".into(),
        "give carter minecraft:minecart".into(),
        "give carter minecraft:oak_boat".into(),
    ] {
        c.send_raw(&cmd(s)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    tokio::time::sleep(Duration::from_millis(800)).await;
    // Открыть раздатчик, шифт-кликом переложить слот хотбара (окно: 0–8
    // раздатчик, 36–44 хотбар), закрыть.
    let mut window = 0;
    for (d, slot) in [(d1, 36i16), (d2, 37)] {
        window += 1;
        let mut w = Writer::new();
        w.var_int(sb("use_item_on"))
            .var_int(0)
            .i64(pack(d))
            .var_int(1)
            .f32(0.5)
            .f32(1.0)
            .f32(0.5)
            .u8(0)
            .u8(0)
            .var_int(window);
        c.send_raw(&w.buf).await.unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        let mut w = Writer::new();
        w.var_int(sb("container_click"))
            .var_int(window)
            .var_int(0)
            .u16(slot as u16)
            .u8(0)
            .var_int(1)
            .var_int(0)
            .bool(false);
        c.send_raw(&w.buf).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut w = Writer::new();
        w.var_int(sb("container_close")).var_int(window);
        c.send_raw(&w.buf).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    for d in [d1, d2] {
        c.send_raw(&cmd(format!(
            "setblock {} {} {} minecraft:redstone_block",
            d[0] - 1,
            d[1],
            d[2]
        )))
        .await
        .unwrap();
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, c.next_event()).await {
        if let Event::EntitySpawned {
            entity_id,
            kind,
            pos,
        } = ev
            && kind >= 0
            && (pos[0] - x as f64).abs() < 8.0
            && (pos[1] - y as f64).abs() < 8.0
            && (pos[2] - z as f64).abs() < 10.0
        {
            let rel = [pos[0] - x as f64, pos[1] - y as f64, pos[2] - z as f64];
            println!(
                "{entity_id} {} rel {rel:?}",
                names.get(&kind).map_or("?", |s| s)
            );
        }
    }
}
