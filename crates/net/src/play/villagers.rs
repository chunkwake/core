//! Жители и торговля (`Villager`, `MerchantMenu`, `MerchantOffer`).
//!
//! Житель — сущность `minecraft:villager` со своей профессией, уровнем,
//! опытом и списком сделок. ПКМ без приседа открывает `merchant`: слоты 0-1
//! под покупку, 2 — результат, 3-38 — инвентарь. `select_trade` выбирает
//! сделку, `container_click` по слоту 2 проводит обмен: забирает цену,
//! кладёт товар в курсор и увеличивает `uses`/`Xp` жителя; при достижении
//! порога (10/70/150/250 опыта) растёт уровень 1-5.

use super::*;

/// Пороги уровней (`VillagerData.NEXT_LEVEL_XP_THRESHOLDS`).
pub const LEVEL_XP: [i32; 5] = [0, 10, 70, 150, 250];

#[derive(Clone, Copy, Debug)]
pub struct Offer {
    pub buy: (i32, i32),
    pub buy_b: Option<(i32, i32)>,
    pub sell: Stack,
    pub uses: i32,
    pub max_uses: i32,
    pub xp: i32,
    pub price_mul: f32,
    pub demand: i32,
}

impl Offer {
    fn out_of_stock(&self) -> bool {
        self.uses >= self.max_uses
    }

    /// `MerchantOffer.satisfiedBy`: цена есть в слоте в нужном количестве.
    fn satisfied(&self, a: Slot, b: Slot) -> bool {
        let check = |s: Slot, (item, count): (i32, i32)| {
            s.is_some_and(|s| s.item == item && s.count >= count)
        };
        if !check(a, self.buy) {
            return false;
        }
        match self.buy_b {
            Some(cost) => check(b, cost),
            None => true,
        }
    }

    pub(super) fn price(&self) -> i32 {
        // `MerchantOffer.getModifiedCostCount`: base + specialPrice + demand.
        self.buy.1.max(1)
    }
}

#[derive(Clone, Debug)]
pub struct Villager {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub uuid: Uuid,
    pub kind: String,
    pub profession: String,
    pub level: i32,
    pub xp: i32,
    pub offers: Vec<Offer>,
    pub custom_name: Option<String>,
    pub persistent: bool,
}

/// Состояние окна торговли: к какому жителю и какая сделка выбрана.
#[derive(Clone, Copy, Debug)]
pub struct MerchantWindow {
    pub villager: i32,
    pub selected: Option<usize>,
}

impl PlayPlan {
    fn villager_type(&self) -> Option<i32> {
        self.survival
            .entity_types
            .get("minecraft:villager")
            .copied()
    }

    fn profession_id(&self, name: &str) -> i32 {
        self.survival
            .villager_professions
            .get(name)
            .copied()
            .unwrap_or(0)
    }

    fn villager_type_id(&self, name: &str) -> i32 {
        self.survival.villager_types.get(name).copied().unwrap_or(0)
    }

    /// Стандартный набор сделок фермера (первая — 20 пшеницы за изумруд).
    fn farmer_offers(&self) -> Vec<Offer> {
        let item = |n: &str| self.item_id(n);
        let mut out = Vec::new();
        let mut push = |buy: (&str, i32),
                        buy_b: Option<(&str, i32)>,
                        sell: (&str, i32),
                        max_uses: i32,
                        xp: i32| {
            let (Some(buy_item), Some(sell_item)) = (item(buy.0), item(sell.0)) else {
                return;
            };
            out.push(Offer {
                buy: (buy_item, buy.1),
                buy_b: buy_b.and_then(|(n, c)| item(n).map(|i| (i, c))),
                sell: Stack::new(sell_item, sell.1),
                uses: 0,
                max_uses,
                xp,
                price_mul: 0.05,
                demand: 0,
            });
        };
        push(
            ("minecraft:wheat", 20),
            None,
            ("minecraft:emerald", 1),
            16,
            2,
        );
        push(
            ("minecraft:potato", 26),
            None,
            ("minecraft:emerald", 1),
            16,
            2,
        );
        push(
            ("minecraft:emerald", 1),
            None,
            ("minecraft:bread", 6),
            16,
            2,
        );
        push(
            ("minecraft:emerald", 1),
            None,
            ("minecraft:pumpkin_pie", 4),
            12,
            2,
        );
        out
    }

    fn villager_named(&self, kind: &str, profession: &str) -> String {
        let short = |n: &str| n.trim_start_matches("minecraft:").to_owned();
        if profession == "minecraft:none" {
            return format!("entity.minecraft.villager.{}", short(kind));
        }
        format!("entity.minecraft.villager.{}", short(profession))
    }

    /// `/summon minecraft:villager <pos> <nbt>`.
    pub(super) fn summon_villager(&self, pos: [f64; 3], nbt: Option<&Compound>) -> Option<i32> {
        let mut v = Villager {
            pos,
            yaw: 0.0,
            uuid: Uuid::from_u128(rand::random()),
            kind: "minecraft:plains".into(),
            profession: "minecraft:farmer".into(),
            level: 1,
            xp: 0,
            offers: self.farmer_offers(),
            custom_name: None,
            persistent: true,
        };
        if let Some(nbt) = nbt {
            apply_villager_nbt(&mut v, nbt, |n| self.item_id(n));
        }
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        self.sim().villagers.lock().unwrap().insert(id, v.clone());
        self.mark_work();
        self.sim().entity_new.lock().unwrap().insert((
            (v.pos[0].floor() as i32) >> 4,
            (v.pos[2].floor() as i32) >> 4,
        ));
        self.hub.spawn_tracked(
            Tracked {
                id,
                pos: v.pos,
                range: self.tracking_range(self.villager_type()?),
            },
            self.villager_packets(id, &v).unwrap_or_default(),
        );
        Some(id)
    }

    fn villager_packets(&self, id: i32, v: &Villager) -> Option<Vec<Vec<u8>>> {
        Some(vec![
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                v.uuid,
                self.villager_type()?,
                v.pos,
                [0.0; 3],
                v.yaw,
                0.0,
                0,
            ),
            // `Villager.DATA_VILLAGER_DATA` (индекс 18) и
            // `DATA_VILLAGER_DATA_FINALIZED` (19).
            play::entity_meta(
                self.ids.set_entity_data,
                id,
                &[
                    (
                        18,
                        play::Meta::VillagerData {
                            kind: self.villager_type_id(&v.kind),
                            profession: self.profession_id(&v.profession),
                            level: v.level,
                        },
                    ),
                    (19, play::Meta::Bool(true)),
                ],
            ),
        ])
    }

    /// Жители из `entities/*.mca` появляются после первого тика.
    pub(super) fn tick_villagers(&self) {
        let loaded = std::mem::take(&mut *self.sim().villagers_loaded.lock().unwrap());
        for v in loaded {
            let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
            let packets = self.villager_packets(id, &v).unwrap_or_default();
            let tracked = Tracked {
                id,
                pos: v.pos,
                range: self.tracking_range(self.villager_type().unwrap_or(0)),
            };
            self.sim().villagers.lock().unwrap().insert(id, v);
            self.hub.spawn_tracked(tracked, packets);
            self.mark_work();
        }
    }

    pub(super) fn track_villagers(&self, out: &mut Vec<Tracked>) {
        let Some(ty) = self.villager_type() else {
            return;
        };
        let range = self.tracking_range(ty);
        for (&id, v) in self.sim().villagers.lock().unwrap().iter() {
            out.push(Tracked {
                id,
                pos: v.pos,
                range,
            });
        }
    }

    pub(super) fn villager_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let villagers = self.sim().villagers.lock().unwrap();
        let v = villagers.get(&id)?;
        self.villager_packets(id, v)
    }

    /// Позиция жителя и его UUID (для `@e` и `interact`).
    pub(super) fn villager_at(&self, id: i32) -> Option<([f64; 3], f32)> {
        self.sim()
            .villagers
            .lock()
            .unwrap()
            .get(&id)
            .map(|v| (v.pos, v.yaw))
    }

    pub(super) fn is_villager(&self, id: i32) -> bool {
        self.sim().villagers.lock().unwrap().contains_key(&id)
    }

    /// Заголовок окна торговли: имя профессии (`Villager.getTitle`).
    pub(super) fn villager_title(&self, id: i32) -> serde_json::Value {
        let villagers = self.sim().villagers.lock().unwrap();
        let Some(v) = villagers.get(&id) else {
            return serde_json::json!({"translate": "entity.minecraft.villager"});
        };
        v.custom_name
            .as_ref()
            .map(|n| serde_json::json!({ "text": n }))
            .unwrap_or_else(|| {
                let key = self.villager_named(&v.kind, &v.profession);
                serde_json::json!({"translate": key})
            })
    }

    /// `ClientboundMerchantOffersPacket` для окна.
    pub(super) fn villager_offers_packet(&self, window: i32, id: i32) -> Option<Vec<u8>> {
        let villagers = self.sim().villagers.lock().unwrap();
        let v = villagers.get(&id)?;
        let offers: Vec<inv::MerchantOffer> = v
            .offers
            .iter()
            .map(|o| inv::MerchantOffer {
                buy: (o.buy.0, o.price().max(1)),
                buy_b: o.buy_b,
                sell: Some(o.sell),
                out_of_stock: o.out_of_stock(),
                uses: o.uses,
                max_uses: o.max_uses,
                xp: o.xp,
                price_diff: 0,
                price_mul: o.price_mul,
                demand: o.demand,
            })
            .collect();
        Some(inv::merchant_offers(
            self.ids.merchant_offers,
            window,
            &offers,
            v.level,
            v.xp,
            v.xp > 0 && v.level < 5,
            true,
        ))
    }

    /// Сделка жителя по индексу.
    pub(super) fn villager_offer(&self, id: i32, idx: usize) -> Option<Offer> {
        self.sim()
            .villagers
            .lock()
            .unwrap()
            .get(&id)
            .and_then(|v| v.offers.get(idx).copied())
    }

    /// `MerchantContainer.getRecipeFor`: выбранная сделка, если цена в слотах
    /// подходит, иначе первая подходящая нераспроданная.
    pub(super) fn merchant_match(&self, m: MerchantWindow, slots: &[Slot]) -> Option<usize> {
        let villagers = self.sim().villagers.lock().unwrap();
        let v = villagers.get(&m.villager)?;
        let (a, b) = (*slots.first()?, *slots.get(1)?);
        let first = m
            .selected
            .filter(|&i| {
                v.offers
                    .get(i)
                    .is_some_and(|o| !o.out_of_stock() && o.satisfied(a, b))
            })
            .or_else(|| {
                v.offers
                    .iter()
                    .position(|o| !o.out_of_stock() && o.satisfied(a, b))
            })?;
        Some(first)
    }

    /// Результат обмена для слота 2 (`MerchantOffer.assemble`).
    pub(super) fn merchant_result(&self, m: MerchantWindow, slots: &[Slot]) -> Slot {
        let idx = self.merchant_match(m, slots)?;
        self.villager_offer(m.villager, idx).map(|o| o.sell)
    }

    /// `MerchantResultSlot.onTake`: `uses += 1`, `Xp += xp`, уровень по
    /// порогам. Возвращает `(сделка, сменился ли уровень)`.
    pub(super) fn notify_trade(&self, id: i32, idx: usize) -> Option<(Offer, bool)> {
        let mut villagers = self.sim().villagers.lock().unwrap();
        let v = villagers.get_mut(&id)?;
        let offer = *v.offers.get(idx)?;
        if let Some(o) = v.offers.get_mut(idx) {
            o.uses += 1;
        }
        v.xp += offer.xp;
        let level = level_for_xp(v.xp);
        let changed = level != v.level;
        v.level = level;
        Some((offer, changed))
    }

    /// Таблица сделок в NBT (`Offers.Recipes`).
    fn offers_nbt(&self, v: &Villager) -> Tag {
        let mut recipes = Vec::new();
        for o in &v.offers {
            let mut c = Compound::new();
            c.insert("buy".into(), item_cost_nbt(o.buy, &|i| self.item_name(i)));
            if let Some(b) = o.buy_b {
                c.insert("buyB".into(), item_cost_nbt(b, &|i| self.item_name(i)));
            }
            c.insert(
                "sell".into(),
                crate::stack_nbt::to_nbt(o.sell, self.item_name(o.sell.item))
                    .map_or(Tag::Compound(Compound::new()), Tag::Compound),
            );
            c.insert("uses".into(), Tag::Int(o.uses));
            c.insert("maxUses".into(), Tag::Int(o.max_uses));
            c.insert("xp".into(), Tag::Int(o.xp));
            c.insert("rewardExp".into(), Tag::Byte(1));
            c.insert("priceMultiplier".into(), Tag::Float(o.price_mul));
            c.insert("demand".into(), Tag::Int(o.demand));
            recipes.push(Tag::Compound(c));
        }
        let mut offers = Compound::new();
        offers.insert("Recipes".into(), Tag::List(recipes));
        Tag::Compound(offers)
    }

    /// NBT одного жителя.
    fn villager_to_nbt(&self, v: &Villager) -> Compound {
        let mut c = Compound::new();
        c.insert("id".into(), Tag::String("minecraft:villager".into()));
        c.insert("UUID".into(), Tag::IntArray(uuid_ints(v.uuid)));
        c.insert(
            "Pos".into(),
            Tag::List(v.pos.into_iter().map(Tag::Double).collect()),
        );
        c.insert(
            "Rotation".into(),
            Tag::List(vec![Tag::Float(v.yaw), Tag::Float(0.0)]),
        );
        let mut data = Compound::new();
        data.insert("type".into(), Tag::String(v.kind.clone()));
        data.insert("profession".into(), Tag::String(v.profession.clone()));
        data.insert("level".into(), Tag::Int(v.level));
        c.insert("VillagerData".into(), Tag::Compound(data));
        c.insert("Xp".into(), Tag::Int(v.xp));
        c.insert("Offers".into(), self.offers_nbt(v));
        if let Some(n) = &v.custom_name {
            c.insert("CustomName".into(), Tag::String(n.clone()));
        }
        if v.persistent {
            c.insert("PersistenceRequired".into(), Tag::Byte(1));
        }
        c
    }

    /// NBT жителя по ID (для `data get`).
    pub(super) fn villager_nbt_of(&self, id: i32) -> Option<Compound> {
        let villagers = self.sim().villagers.lock().unwrap();
        villagers.get(&id).map(|v| self.villager_to_nbt(v))
    }

    /// NBT жителя для `entities/*.mca`.
    pub(super) fn villagers_nbt(&self) -> Vec<((i32, i32), Compound)> {
        let villagers = self.sim().villagers.lock().unwrap();
        villagers
            .values()
            .map(|v| {
                (
                    (
                        (v.pos[0].floor() as i32) >> 4,
                        (v.pos[2].floor() as i32) >> 4,
                    ),
                    self.villager_to_nbt(v),
                )
            })
            .collect()
    }

    /// Загрузка жителя из `entities/*.mca`; `true` — сущность распознана.
    pub(super) fn load_villager(&self, c: &Compound) -> bool {
        if !matches!(c.get("id"), Some(Tag::String(id)) if id == "minecraft:villager") {
            return false;
        }
        let Some(Tag::List(pos)) = c.get("Pos") else {
            return false;
        };
        let nums: Vec<f64> = pos
            .iter()
            .filter_map(|t| match t {
                Tag::Double(v) => Some(*v),
                Tag::Float(v) => Some(*v as f64),
                _ => None,
            })
            .collect();
        if nums.len() < 3 {
            return false;
        }
        let mut v = Villager {
            pos: [nums[0], nums[1], nums[2]],
            yaw: 0.0,
            uuid: Uuid::from_u128(rand::random()),
            kind: "minecraft:plains".into(),
            profession: "minecraft:farmer".into(),
            level: 1,
            xp: 0,
            offers: self.farmer_offers(),
            custom_name: None,
            persistent: true,
        };
        apply_villager_nbt(&mut v, c, |n| self.item_id(n));
        self.sim().villagers_loaded.lock().unwrap().push(v);
        true
    }
}

/// `VillagerData.nextLevelXp` / `getLevel`.
pub fn level_for_xp(xp: i32) -> i32 {
    let mut level = 1;
    for (i, &need) in LEVEL_XP.iter().enumerate() {
        if xp >= need {
            level = i as i32 + 1;
        }
    }
    level.clamp(1, 5)
}

fn item_cost_nbt<'a>((item, count): (i32, i32), name_of: &dyn Fn(i32) -> &'a str) -> Tag {
    let mut c = Compound::new();
    c.insert("id".into(), Tag::String(name_of(item).to_owned()));
    c.insert("count".into(), Tag::Int(count));
    Tag::Compound(c)
}

fn uuid_ints(uuid: Uuid) -> Vec<i32> {
    let u = uuid.as_u128();
    (0..4)
        .rev()
        .map(|i| (u >> (32 * i)) as u32 as i32)
        .collect()
}

fn apply_villager_nbt(v: &mut Villager, c: &Compound, item: impl Fn(&str) -> Option<i32>) {
    if let Some(Tag::Compound(data)) = c.get("VillagerData") {
        if let Some(Tag::String(kind)) = data.get("type") {
            v.kind.clone_from(kind);
        }
        if let Some(Tag::String(profession)) = data.get("profession") {
            v.profession.clone_from(profession);
        }
        if let Some(level) = data.get("level").and_then(Tag::as_i64) {
            v.level = (level as i32).clamp(1, 5);
        }
    }
    if let Some(xp) = c.get("Xp").and_then(Tag::as_i64) {
        v.xp = xp.max(0) as i32;
        v.level = level_for_xp(v.xp).max(v.level);
    }
    if let Some(Tag::Compound(offers)) = c.get("Offers")
        && let Some(Tag::List(recipes)) = offers.get("Recipes")
    {
        let parsed: Vec<Offer> = recipes
            .iter()
            .filter_map(|r| {
                let Tag::Compound(r) = r else { return None };
                let cost = |tag: &Tag| -> Option<(i32, i32)> {
                    let Tag::Compound(c) = tag else { return None };
                    let Tag::String(id) = c.get("id")? else {
                        return None;
                    };
                    let item = item(id)?;
                    let count = c.get("count").and_then(Tag::as_i64).unwrap_or(1) as i32;
                    Some((item, count))
                };
                let buy = cost(r.get("buy")?)?;
                let buy_b = r.get("buyB").and_then(cost);
                let sell_tag = r.get("sell")?;
                let Tag::Compound(sell_c) = sell_tag else {
                    return None;
                };
                let sell = crate::stack_nbt::from_nbt(sell_c, &item)?;
                Some(Offer {
                    buy,
                    buy_b,
                    sell,
                    uses: r.get("uses").and_then(Tag::as_i64).unwrap_or(0) as i32,
                    max_uses: r.get("maxUses").and_then(Tag::as_i64).unwrap_or(4) as i32,
                    xp: r.get("xp").and_then(Tag::as_i64).unwrap_or(1) as i32,
                    price_mul: match r.get("priceMultiplier") {
                        Some(Tag::Float(f)) => *f,
                        Some(Tag::Double(f)) => *f as f32,
                        _ => 0.0,
                    },
                    demand: r.get("demand").and_then(Tag::as_i64).unwrap_or(0) as i32,
                })
            })
            .collect();
        if !parsed.is_empty() {
            v.offers = parsed;
        }
    }
    if let Some(Tag::String(n)) = c.get("CustomName") {
        v.custom_name = Some(n.clone());
    }
    if let Some(Tag::Byte(p)) = c.get("PersistenceRequired") {
        v.persistent = *p != 0;
    }
    if let Some(Tag::List(rot)) = c.get("Rotation")
        && let Some(Tag::Float(yaw)) = rot.first()
    {
        v.yaw = *yaw;
    }
    if let Some(Tag::IntArray(u)) = c.get("UUID")
        && u.len() == 4
    {
        v.uuid = Uuid::from_u128(u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn villager_levels_follow_xp_thresholds() {
        assert_eq!(level_for_xp(0), 1);
        assert_eq!(level_for_xp(9), 1);
        assert_eq!(level_for_xp(10), 2);
        assert_eq!(level_for_xp(69), 2);
        assert_eq!(level_for_xp(70), 3);
        assert_eq!(level_for_xp(149), 3);
        assert_eq!(level_for_xp(150), 4);
        assert_eq!(level_for_xp(249), 4);
        assert_eq!(level_for_xp(250), 5);
        assert_eq!(level_for_xp(10_000), 5);
    }
}
