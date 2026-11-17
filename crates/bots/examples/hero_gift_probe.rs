//! Скидка героя деревни в торговле и подарки жителей
//! (`Villager.updateSpecialPrices` + `GiveGiftToHero`).
//!
//! `hero_gift_probe <addr> <packets.json> <registries.json>`
use chunkwake_bots::{Client, Event, Ids};
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_protocol::{Reader, Writer, text};
use std::time::Duration;

const BX: i32 = 4000;
const BZ: i32 = 4000;
const BY: i32 = 64;

struct Probe {
    c: Client,
    sb_command: i32,
    sb_interact: i32,
    sb_click: i32,
    sb_select_trade: i32,
    sb_close: i32,
    cb_chat: i32,
    cb_offers: i32,
    cb_sound: i32,
    villager_type: i32,
    item_type: i32,
    /// `AbstractVillager.notifyTrade*`: `entity.villager.yes`/`.no`.
    yes_sound: i32,
    no_sound: i32,
    saw_yes: bool,
    saw_no: bool,
    chat: String,
    villager: Option<i32>,
    window: Option<i32>,
    /// (цена первого предложения, specialPriceDiff) из `merchant_offers`.
    first_offer: Option<(i32, i32)>,
    gift_items: i32,
    seq: i32,
}

impl Probe {
    fn packet(&self, id: i32) -> Writer {
        let mut w = Writer::new();
        w.var_int(id);
        w
    }

    fn parse_offers(&mut self, body: &[u8]) {
        let mut r = Reader::new(body);
        let (Ok(_window), Ok(count)) = (r.var_int(), r.var_int()) else {
            return;
        };
        for i in 0..count {
            let (Ok(_buy_item), Ok(buy_count), Ok(_pred)) = (r.var_int(), r.var_int(), r.var_int())
            else {
                return;
            };
            if chunkwake_protocol::inventory::read_stack(&mut r).is_err() {
                return;
            }
            let Ok(has_b) = r.bool() else { return };
            if has_b && (r.var_int().is_err() || r.var_int().is_err() || r.var_int().is_err()) {
                return;
            }
            if r.bool().is_err() {
                return;
            }
            let Ok(_uses) = read_i32(&mut r) else { return };
            let Ok(_max) = read_i32(&mut r) else { return };
            let Ok(_xp) = read_i32(&mut r) else { return };
            let Ok(price_diff) = read_i32(&mut r) else {
                return;
            };
            if i == 0 {
                self.first_offer = Some((buy_count, price_diff));
            }
            if r.f32().is_err() || read_i32(&mut r).is_err() {
                return;
            }
        }
    }

    fn apply(&mut self, ev: Event) {
        match ev {
            Event::EntitySpawned {
                entity_id,
                kind,
                pos,
            } => {
                if (pos[0] - f64::from(BX + 2)).abs() > 10.0
                    || (pos[2] - f64::from(BZ)).abs() > 10.0
                {
                    return;
                }
                if kind == self.villager_type {
                    self.villager = Some(entity_id);
                } else if kind == self.item_type {
                    self.gift_items += 1;
                }
            }
            Event::OpenScreen { window, .. } => self.window = Some(window),
            _ => {}
        }
    }

    fn drain(&mut self) {
        for (id, body) in std::mem::take(&mut self.c.captured_by) {
            if id == self.cb_chat {
                let mut r = Reader::new(&body);
                if let Ok(component) = text::read_component(&mut r) {
                    flatten(&component, &mut self.chat);
                    self.chat.push(' ');
                }
            } else if id == self.cb_offers {
                self.parse_offers(&body);
            } else if id == self.cb_sound {
                let mut r = Reader::new(&body);
                if let Ok(ev) = r.var_int() {
                    if ev == self.yes_sound + 1 {
                        self.saw_yes = true;
                    }
                    if ev == self.no_sound + 1 {
                        self.saw_no = true;
                    }
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
        self.pump(300).await;
    }

    async fn data(&mut self, query: &str) -> String {
        self.pump(150).await;
        self.chat.clear();
        self.cmd(&format!("data get entity {query}")).await;
        self.chat.clone()
    }

    async fn interact(&mut self, entity: i32) {
        let mut w = self.packet(self.sb_interact);
        w.var_int(entity).var_int(0).u8(0).bool(false);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(500).await;
    }

    async fn close(&mut self, window: i32) {
        let mut w = self.packet(self.sb_close);
        w.var_int(window);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(250).await;
    }

    async fn select_trade(&mut self, idx: i32) {
        let mut w = self.packet(self.sb_select_trade);
        w.var_int(idx);
        self.c.send_raw(&w.buf).await.unwrap();
        self.pump(200).await;
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
        self.pump(250).await;
    }
}

fn read_i32(r: &mut Reader) -> Result<i32, chunkwake_protocol::ProtocolError> {
    let bytes = r.bytes(4)?;
    Ok(i32::from_be_bytes(bytes.try_into().unwrap()))
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
    let cb_chat = cb("system_chat");
    let cb_offers = cb("merchant_offers");
    let mut c = Client::connect(&a[1], "boater", ids, 777, "26.3")
        .await
        .unwrap();
    let cb_sound = cb("sound");
    c.capture_also = vec![cb_chat, cb_offers, cb_sound];
    let mut p = Probe {
        c,
        sb_command: sb("chat_command"),
        sb_interact: sb("interact"),
        sb_click: sb("container_click"),
        sb_select_trade: sb("select_trade"),
        sb_close: sb("container_close"),
        cb_chat,
        cb_offers,
        cb_sound,
        yes_sound: pid("minecraft:sound_event", "minecraft:entity.villager.yes"),
        no_sound: pid("minecraft:sound_event", "minecraft:entity.villager.no"),
        saw_yes: false,
        saw_no: false,
        villager_type: pid("minecraft:entity_type", "minecraft:villager"),
        item_type: pid("minecraft:entity_type", "minecraft:item"),
        chat: String::new(),
        villager: None,
        window: None,
        first_offer: None,
        gift_items: 0,
        seq: 0,
    };
    p.pump(1500).await;

    for line in [
        "difficulty peaceful".to_string(),
        "gamemode survival".to_string(),
        "effect clear @s".to_string(),
        "kill @e[type=!minecraft:player,distance=..64]".to_string(),
        format!("execute in minecraft:overworld run tp @s {BX}.5 {BY} {BZ}.5 180 20"),
        format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            BX - 8,
            BY - 1,
            BZ - 8,
            BX + 8,
            BY - 1,
            BZ + 8
        ),
        format!(
            "fill {} {BY} {} {} {} {} minecraft:air",
            BX - 8,
            BZ - 8,
            BX + 8,
            BY + 5,
            BZ + 8
        ),
        format!("tp @s {BX}.5 {BY} {BZ}.5 180 20"),
        "clear @s".to_string(),
    ] {
        p.cmd(&line).await;
    }
    p.c.confirm_loaded().await.unwrap();

    // Житель без ИИ: не уходит от игрока.
    p.cmd(&format!(
        "summon minecraft:villager {}.5 {BY} {}.5 {{NoAI:1b,VillagerData:{{profession:\"minecraft:farmer\",level:1}},Xp:0}}",
        BX + 2,
        BZ
    ))
    .await;
    p.pump(600).await;
    let villager = p.villager.expect("житель не появился");
    // Скидку героя проверяем на фиксированной сделке (20 пшеницы), чтобы
    // не зависеть от случайного выбора `TradeSet`.
    p.cmd(
        "data merge entity @e[type=minecraft:villager,limit=1,sort=nearest] {Offers:{Recipes:[{buy:{id:\"minecraft:wheat\",count:20},sell:{id:\"minecraft:emerald\",count:1},uses:0,maxUses:16,xp:2,priceMultiplier:0.05f,demand:0}]}}",
    )
    .await;

    // 1. Базовая цена без героя (20 пшеницы).
    p.first_offer = None;
    p.interact(villager).await;
    let window = p.window.expect("окно торговли не открылось");
    p.pump(300).await;
    let (base, _) = p.first_offer.expect("нет merchant_offers");
    p.close(window).await;
    assert_eq!(base, 20, "базовая цена фермера не 20: {base}");

    // 2. Герой деревни: скидка floor(0.3 * 20) = 6.
    p.cmd("effect give @s minecraft:hero_of_the_village 1200 0")
        .await;
    p.window = None;
    p.first_offer = None;
    p.interact(villager).await;
    let window = p.window.expect("окно торговли не открылось повторно");
    p.pump(300).await;
    let (discounted, price_diff) = p.first_offer.expect("нет merchant_offers после героя");
    assert_eq!(discounted, 14, "скидка героя не применилась: {discounted}");
    assert_eq!(price_diff, -6, "specialPriceDiff не -6: {price_diff}");
    println!("hero discount ok (20 -> 14, diff {price_diff})");

    // 3. Оплата по скидочной цене: 14 пшеницы достаточно.
    p.cmd("item replace entity @s hotbar.0 with minecraft:wheat 14")
        .await;
    p.select_trade(0).await;
    p.click(window, 30).await; // пшеница из хотбара
    p.click(window, 0).await; // в слот покупки
    p.click(window, 2).await; // забрать изумруд
    p.click(window, 31).await; // изумруд в хотбар
    p.pump(300).await;
    let inv = p.data("@s Inventory").await;
    assert!(inv.contains("emerald"), "сделка по скидке не прошла: {inv}");
    println!("hero discount payment ok (14 wheat -> emerald)");
    // `AbstractVillager.notifyTradeUpdated` / `notifyTrade`: пустой
    // результат — `entity.villager.no`, валидный — `yes`.
    assert!(p.saw_yes, "нет entity.villager.yes при сделке");
    if !p.saw_no {
        for _ in 0..4 {
            p.select_trade(0).await;
            p.pump(400).await;
            if p.saw_no {
                break;
            }
        }
    }
    assert!(p.saw_no, "нет entity.villager.no на пустой сделке");
    println!("trade sounds: yes on trade, no on empty offer");
    p.close(window).await;

    // 4. Подарок герою: таймер 600 тиков, затем предмет у жителя.
    p.cmd("kill @e[type=minecraft:item,distance=..32]").await;
    p.gift_items = 0;
    p.pump(500).await;
    p.gift_items = 0;
    for _ in 0..500 {
        p.pump(100).await;
        if p.gift_items > 0 {
            break;
        }
    }
    assert!(p.gift_items > 0, "житель не бросил подарок герою");
    println!("hero gift ok ({} item entity)", p.gift_items);
    println!("hero_gift_probe: ok");
}
