//! Книга рецептов: `/recipe give|take`, `recipe_book_add/remove`,
//! `place_recipe` (2×2), ghost-рецепт и сохранение в `playerdata`.
//!
//! `recipe_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 4300;
const BZ: i32 = 4300;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_click: i32,
    sb_place_recipe: i32,
    sb_settings: i32,
    sb_seen: i32,
    sb_set_carried: i32,
    cb_chat: i32,
    chat: String,
    seq: i32,
    add: Vec<(i32, u8)>,
    replace: bool,
    removed: Vec<i32>,
    ghost: Option<i32>,
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
            self.apply(ev);
        }
        self.drain();
    }

    fn apply(&mut self, ev: Event) {
        match ev {
            Event::RecipeBookAdd { entries, replace } => {
                if replace {
                    self.add.clear();
                }
                self.add.extend(entries);
                self.replace = replace;
            }
            Event::RecipeBookRemove(ids) => self.removed.extend(ids),
            Event::PlaceGhostRecipe(container) => self.ghost = Some(container),
            _ => {}
        }
    }

    async fn cmd(&mut self, line: &str) {
        let mut w = self.packet(self.sb_command);
        w.string(line);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(400).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(120).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        if self.chat.trim().is_empty() {
            self.pump(300).await;
            self.cmd(&format!("data get entity {query}")).await;
        }
        self.chat.clone()
    }

    async fn select(&mut self, slot: u16) {
        let mut w = self.packet(self.sb_set_carried);
        w.u16(slot);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(150).await;
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

    async fn place(&mut self, window: i32, display: i32, use_max: bool) {
        self.add.clear();
        self.ghost = None;
        let mut w = self.packet(self.sb_place_recipe);
        w.var_int(window).var_int(display).bool(use_max);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(500).await;
    }

    async fn settings(&mut self, kind: i32, open: bool, filtering: bool) {
        let mut w = self.packet(self.sb_settings);
        w.var_int(kind).bool(open).bool(filtering);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }

    async fn seen(&mut self, display: i32) {
        let mut w = self.packet(self.sb_seen);
        w.var_int(display);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(300).await;
    }
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let table = PacketTable::load(a[2].as_ref()).unwrap();
    let ids = Ids::load(&table).unwrap();
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
        sb_click: sb("container_click"),
        sb_place_recipe: sb("place_recipe"),
        sb_settings: sb("recipe_book_change_settings"),
        sb_seen: sb("recipe_book_seen_recipe"),
        sb_set_carried: sb("set_carried_item"),
        cb_chat,
        chat: String::new(),
        seq: 0,
        add: Vec::new(),
        replace: false,
        removed: Vec::new(),
        ghost: None,
    };
    p.pump(1500).await;
    assert!(
        p.replace,
        "при входе не пришла книга рецептов (replace=true)"
    );
    println!("initial book: {} entries", p.add.len());

    let by = 64;
    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        format!(
            "fill {} {} {} {} {} {} minecraft:air",
            BX - 8,
            by,
            BZ - 8,
            BX + 8,
            by + 5,
            BZ + 8
        ),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 8,
            by - 1,
            BZ - 8,
            BX + 8,
            by - 1,
            BZ + 8
        ),
        format!("tp boater {} {} {}", BX as f64 + 0.5, by, BZ as f64 + 0.5),
        "clear boater".to_string(),
        "recipe take @s *".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.add.clear();

    // 1. `/recipe give` одного рецепта и раскладка 2×2.
    p.cmd("recipe give @s minecraft:oak_planks").await;
    assert_eq!(p.add.len(), 1, "ожидался один рецепт: {:?}", p.add);
    let planks = p.add[0].0;
    assert_eq!(p.add[0].1 & 2, 2, "новая запись должна быть подсвечена");
    println!("recipe given: display {planks}");

    p.cmd("give boater minecraft:oak_log 4").await;
    p.select(0).await;
    p.place(0, planks, false).await;
    assert!(p.ghost.is_none(), "неожиданный ghost-рецепт");
    p.click(0, 0).await;
    p.click(0, 40).await;
    let inv = p.data("@s Inventory").await;
    assert!(
        inv.contains("oak_planks"),
        "после place_recipe не получились доски: {inv}"
    );
    println!("place_recipe 2x2 ok");

    // 2. Ghost-рецепт: рецепт известен, ингредиентов нет.
    p.cmd("clear boater").await;
    p.place(0, planks, false).await;
    assert_eq!(
        p.ghost,
        Some(0),
        "не пришёл place_ghost_recipe для пустого инвентаря"
    );
    println!("place_ghost_recipe ok");

    // 3. `/recipe take` присылает `recipe_book_remove`.
    p.removed.clear();
    p.cmd("recipe take @s minecraft:oak_planks").await;
    assert!(
        p.removed.contains(&planks),
        "recipe_book_remove не пришёл: {:?}",
        p.removed
    );
    println!("recipe take ok");

    // 4. `*`: все обучаемые рецепты.
    p.add.clear();
    p.cmd("recipe give @s *").await;
    assert!(
        p.add.len() > 1500,
        "по `*` пришло слишком мало рецептов: {}",
        p.add.len()
    );
    println!("recipe give *: {} entries", p.add.len());
    p.cmd("recipe take @s *").await;

    // 5. Настройки и «просмотрено».
    p.settings(0, true, true).await;
    p.seen(planks).await;
    println!("recipe book settings/seen ok");

    // 6. Сохранение: выдаём crafting_table, перезаходим и раскладываем.
    p.add.clear();
    p.cmd("recipe give @s minecraft:crafting_table").await;
    assert_eq!(p.add.len(), 1, "crafting_table не выдан");
    let table = p.add[0].0;
    p.cmd("save-all").await;
    p.pump(500).await;
    drop(p);
    tokio::time::sleep(Duration::from_millis(2000)).await;
    let table2 = PacketTable::load(std::path::Path::new(&a[2])).unwrap();
    let mut c2 = Client::connect(&a[1], "boater", Ids::load(&table2).unwrap(), 777, "26.3")
        .await
        .unwrap();
    let mut add = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(2500);
    while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, c2.next_event()).await {
        if let Event::RecipeBookAdd { entries, replace } = ev
            && replace
        {
            add = entries;
        }
    }
    assert!(
        add.iter().any(|&(d, _)| d == table),
        "выданный рецепт не загрузился при входе: {add:?}"
    );
    println!("recipe book persistence ok (display {table})");

    println!("recipe: ok");
}
