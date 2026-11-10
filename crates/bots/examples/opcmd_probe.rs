//! Команды оператора (`/plugins`, `/version`, `/mspt`, `/help`): шлёт команды
//! и печатает текст ответов из `system_chat` (строки NBT подряд), а также
//! какие из имён `CHECK` есть в дереве `commands` у этого игрока.
//!
//! `opcmd_probe <addr> <packets.json> <name> <cmd>...`; `CHECK=mspt,plugins`,
//! `DIG=x,y,z` — после команд сломать блок.
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::Writer;
use std::time::Duration;

/// Печатаемые UTF-8 строки пакета: грубо, но для глаз хватает.
fn strings(body: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(body);
    text.split(|c: char| c.is_control() || c == '\u{FFFD}')
        .filter(|s| s.chars().count() >= 2)
        .map(str::to_owned)
        .collect()
}

/// Читает события `ms` миллисекунд: захваченные пакеты копятся в клиенте.
async fn wait(c: &mut Client, ms: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Ok(_)) = tokio::time::timeout_at(deadline, c.next_event()).await {}
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
    let id = |d, n| table.id(State::Play, d, n).unwrap();
    let cmd = id(Direction::Serverbound, "chat_command");
    let chat = id(Direction::Clientbound, "system_chat");
    let tree = id(Direction::Clientbound, "commands");
    let mut c = Client::connect(&a[1], &a[3], ids, 777, "26.3").await.unwrap();
    c.capture_also.push(chat);
    c.capture_also.push(tree);
    loop {
        if let Event::Position(_) = c.next_event().await.unwrap() {
            break;
        }
    }
    let check: Vec<String> = std::env::var("CHECK")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let drain = |c: &mut Client| {
        for (pid, body) in std::mem::take(&mut c.captured_by) {
            if pid == tree {
                let names = strings(&body);
                for n in &check {
                    println!("  tree has {n}: {}", names.iter().any(|s| s.ends_with(n.as_str())));
                }
            } else {
                println!("  | {}", strings(&body).join(""));
            }
        }
    };
    wait(&mut c, 1500).await;
    drain(&mut c);
    for s in &a[4..] {
        println!("> /{s}");
        let mut w = Writer::new();
        w.var_int(cmd).string(s);
        c.send_raw(&w.buf).await.unwrap();
        let gap = std::env::var("GAP_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1500);
        wait(&mut c, gap).await;
        drain(&mut c);
    }
    // `INPUTS=1`: слот 3, шифт вкл/выкл, бег вкл/выкл — события
    // `PlayerItemHeld`/`ToggleSneak`/`ToggleSprint`.
    if std::env::var("INPUTS").is_ok() {
        let held = id(Direction::Serverbound, "set_carried_item");
        let input = id(Direction::Serverbound, "player_input");
        let command = id(Direction::Serverbound, "player_command");
        println!("> inputs");
        let mut w = Writer::new();
        w.var_int(held).u16(3);
        c.send_raw(&w.buf).await.unwrap();
        for flags in [0x20u8, 0] {
            let mut w = Writer::new();
            w.var_int(input).u8(flags);
            c.send_raw(&w.buf).await.unwrap();
            wait(&mut c, 200).await;
        }
        for action in [1, 2] {
            let mut w = Writer::new();
            w.var_int(command).var_int(0).var_int(action).var_int(0);
            c.send_raw(&w.buf).await.unwrap();
            wait(&mut c, 200).await;
        }
        wait(&mut c, 1000).await;
        drain(&mut c);
    }
    // `RESPAWN=1`: «Возродиться» после смерти — `PlayerRespawnEvent`.
    if std::env::var("RESPAWN").is_ok() {
        let client_command = id(Direction::Serverbound, "client_command");
        println!("> respawn");
        let mut w = Writer::new();
        w.var_int(client_command).var_int(0);
        c.send_raw(&w.buf).await.unwrap();
        wait(&mut c, 2000).await;
        drain(&mut c);
    }
    // `SIGN=x,y,z`: поставить табличку из слота 0 на блок под `x,y,z`, ответить
    // на редактор строками и кликнуть по ней снова (редактор `INTERACT`).
    if let Some(v) = std::env::var("SIGN").ok().map(|s| {
        s.split(',').map(|n| n.trim().parse::<i64>().unwrap()).collect::<Vec<_>>()
    }) {
        let pack = |x: i64, y: i64, z: i64| ((x & 0x3FF_FFFF) << 38) | ((z & 0x3FF_FFFF) << 12) | (y & 0xFFF);
        let use_on = id(Direction::Serverbound, "use_item_on");
        let update = id(Direction::Serverbound, "sign_update");
        let editor = id(Direction::Clientbound, "open_sign_editor");
        c.capture_also.push(editor);
        let click = |pos: i64, face: i32, seq: i32| {
            let mut w = Writer::new();
            w.var_int(use_on).var_int(0).i64(pos).var_int(face);
            w.f32(0.5).f32(1.0).f32(0.5).bool(false).bool(false).var_int(seq);
            w.buf
        };
        let opened = |c: &mut Client| {
            let n = c.captured_by.iter().filter(|(p, _)| *p == editor).count();
            c.captured_by.retain(|(p, _)| *p != editor);
            n
        };
        println!("> place sign");
        c.send_raw(&click(pack(v[0], v[1] - 1, v[2]), 1, 1)).await.unwrap();
        wait(&mut c, 1500).await;
        println!("  editor opened: {}", opened(&mut c));
        let mut w = Writer::new();
        w.var_int(update).i64(pack(v[0], v[1], v[2])).bool(true);
        for l in ["Hello", "§cfrom", "bot", ""] {
            w.string(l);
        }
        c.send_raw(&w.buf).await.unwrap();
        wait(&mut c, 1500).await;
        println!("> click sign");
        c.send_raw(&click(pack(v[0], v[1], v[2]), 3, 2)).await.unwrap();
        wait(&mut c, 1500).await;
        println!("  editor opened: {}", opened(&mut c));
        drain(&mut c);
    }
    // `BOOK=1`: книга с пером в слоте 0 — правка, подпись с названием, ПКМ
    // (ждём `open_book`), затем две правки подряд (кик Paper «too quickly»).
    if std::env::var("BOOK").is_ok() {
        let edit = id(Direction::Serverbound, "edit_book");
        let use_item = id(Direction::Serverbound, "use_item");
        let open = id(Direction::Clientbound, "open_book");
        c.capture_also.push(open);
        let book = |pages: &[&str], title: Option<&str>| {
            let mut w = Writer::new();
            w.var_int(edit).var_int(0).var_int(pages.len() as i32);
            for p in pages {
                w.string(p);
            }
            w.bool(title.is_some());
            if let Some(t) = title {
                w.string(t);
            }
            w.buf
        };
        println!("> edit book");
        c.send_raw(&book(&["first page", "second"], None)).await.unwrap();
        wait(&mut c, 1500).await;
        println!("> sign book");
        c.send_raw(&book(&["first page", "second"], Some("Probe Book"))).await.unwrap();
        wait(&mut c, 1500).await;
        println!("> open book");
        let mut w = Writer::new();
        w.var_int(use_item).var_int(0).var_int(3).f32(0.0).f32(0.0);
        c.send_raw(&w.buf).await.unwrap();
        wait(&mut c, 1500).await;
        let opened = c.captured_by.iter().filter(|(p, _)| *p == open).count();
        c.captured_by.retain(|(p, _)| *p != open);
        println!("  open_book: {opened}");
        drain(&mut c);
    }
    // `DIG=x,y,z`: начать ломать блок (в творческом он ломается сразу) —
    // события ломания у плагинов (`BlockBreakEvent`).
    if let Some(p) = std::env::var("DIG").ok().map(|s| {
        let v: Vec<i64> = s.split(',').map(|n| n.trim().parse().unwrap()).collect();
        ((v[0] & 0x3FF_FFFF) << 38) | ((v[2] & 0x3FF_FFFF) << 12) | (v[1] & 0xFFF)
    }) {
        let action = id(Direction::Serverbound, "player_action");
        println!("> dig");
        let mut w = Writer::new();
        w.var_int(action).var_int(0).i64(p).u8(1).var_int(1);
        c.send_raw(&w.buf).await.unwrap();
        wait(&mut c, 2000).await;
        drain(&mut c);
    }
}
