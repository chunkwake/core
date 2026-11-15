//! Жители и торговля (`Villager`, `MerchantMenu`, `MerchantOffer`).
//!
//! Житель — сущность `minecraft:villager` со своей профессией, уровнем,
//! опытом и списком сделок. ПКМ без приседа открывает `merchant`: слоты 0-1
//! под покупку, 2 — результат, 3-38 — инвентарь. `select_trade` выбирает
//! сделку, `container_click` по слоту 2 проводит обмен: забирает цену,
//! кладёт товар в курсор и увеличивает `uses`/`Xp` жителя; при достижении
//! порога (10/70/150/250 опыта) растёт уровень 1-5.

use super::*;
use std::collections::BTreeMap;

/// Пороги уровней (`VillagerData.NEXT_LEVEL_XP_THRESHOLDS`).
pub const LEVEL_XP: [i32; 5] = [0, 10, 70, 150, 250];

/// Вид торговца: обычный житель или странствующий торговец.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraderKind {
    Villager,
    Wandering,
}

impl TraderKind {
    pub fn entity_name(self) -> &'static str {
        match self {
            TraderKind::Villager => "minecraft:villager",
            TraderKind::Wandering => "minecraft:wandering_trader",
        }
    }
}

/// Рабочий блок профессии (`VillagerProfession.workstation`).
fn workstation(profession: &str) -> Option<&'static str> {
    Some(match profession {
        "minecraft:farmer" => "minecraft:composter",
        "minecraft:librarian" => "minecraft:lectern",
        "minecraft:fisherman" => "minecraft:barrel",
        "minecraft:fletcher" => "minecraft:fletching_table",
        "minecraft:cleric" => "minecraft:brewing_stand",
        "minecraft:armorer" => "minecraft:blast_furnace",
        "minecraft:weaponsmith" => "minecraft:grindstone",
        "minecraft:toolsmith" => "minecraft:smithing_table",
        "minecraft:butcher" => "minecraft:smoker",
        "minecraft:leatherworker" => "minecraft:cauldron",
        "minecraft:cartographer" => "minecraft:cartography_table",
        "minecraft:mason" => "minecraft:stonecutter",
        "minecraft:shepherd" => "minecraft:loom",
        _ => return None,
    })
}

/// Звук работы (`VillagerProfession.workSound`).
fn work_sound(profession: &str) -> Option<String> {
    let short = profession.strip_prefix("minecraft:")?;
    workstation(profession)?;
    Some(format!("minecraft:entity.villager.work_{short}"))
}

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
    /// `MerchantOffer.specialPriceDiff` (скидка героя и репутация).
    pub special_price: i32,
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
        if !check(a, (self.buy.0, self.price())) {
            return false;
        }
        match self.buy_b {
            Some(cost) => check(b, cost),
            None => true,
        }
    }

    pub(super) fn price(&self) -> i32 {
        // `MerchantOffer.getModifiedCostCount`: base + demand + specialPrice.
        let base = self.buy.1.max(1);
        let demand = ((base as f32 * self.demand as f32 * self.price_mul).floor() as i32).max(0);
        (base + demand + self.special_price).clamp(1, 64)
    }

    /// `Villager.updateSpecialPrices`: репутация и скидка героя деревни.
    fn apply_special_prices(&mut self, amplifier: Option<u8>, reputation: i32) {
        self.special_price = 0;
        if reputation != 0 {
            // `addToSpecialPriceDiff(-floor(reputation * priceMultiplier))`.
            self.special_price -= (reputation as f32 * self.price_mul).floor() as i32;
        }
        if let Some(amplifier) = amplifier {
            let modifier = 0.3 + 0.0625 * f64::from(amplifier);
            let reduction = (modifier * f64::from(self.buy.1.max(1))).floor() as i32;
            self.special_price -= reduction.max(1);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Villager {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub uuid: Uuid,
    pub trader: TraderKind,
    pub kind: String,
    pub profession: String,
    pub level: i32,
    pub xp: i32,
    pub offers: Vec<Offer>,
    /// `WanderingTrader.DespawnDelay`: 0 — не исчезает.
    pub despawn_delay: i32,
    /// Игровое время последнего пополнения (`LastRestock`).
    pub last_restock: i64,
    /// `RestocksToday`.
    pub restocks_today: i32,
    /// Последняя проверка рабочего блока (в игровых тиках).
    pub last_work_check: i64,
    /// Занятое рабочее место; `None` — безработный или место потеряно.
    pub job_site: Option<[i32; 3]>,
    /// Тик последней проверки профессии по точкам интереса.
    pub last_job_check: i64,
    pub custom_name: Option<String>,
    pub persistent: bool,
    /// `GiveGiftToHero.timeUntilNextGift`: тики до подарка.
    pub gift_timer: i32,
    /// `GiveGiftToHero`: пауза на поворот головы перед броском.
    pub gift_wait: Option<i32>,
    /// `GossipContainer`: UUID игрока -> значения `GossipType::ALL`.
    pub gossips: BTreeMap<u128, [i32; 5]>,
    /// `lastGossipDecayTime`.
    pub last_gossip_decay: i64,
    /// `LivingEntity.health`.
    pub health: f32,
}

/// Состояние окна торговли: к какому жителю и какая сделка выбрана.
impl Villager {
    /// `Villager.updateSpecialPrices`: сброс скидок, репутация и герой.
    fn update_special_prices(&mut self, amplifier: Option<u8>, reputation: i32) {
        for offer in &mut self.offers {
            offer.apply_special_prices(amplifier, reputation);
        }
    }

    /// `GossipContainer.add`: слияние, предел и отбрасывание значений < 2.
    pub(super) fn add_gossip(&mut self, target: u128, kind: GossipType, amount: i32) {
        let idx = kind.id();
        let old = self.gossips.get(&target).map_or(0, |v| v[idx]);
        let sum = old + amount;
        let mut value = if sum > kind.max() {
            old.max(kind.max())
        } else {
            sum
        };
        if value > kind.max() {
            value = kind.max();
        }
        let entry = self.gossips.entry(target).or_insert([0; 5]);
        if value < 2 {
            entry[idx] = 0;
            if entry.iter().all(|&v| v == 0) {
                self.gossips.remove(&target);
            }
        } else {
            entry[idx] = value;
        }
    }

    /// `Villager.getPlayerReputation`: взвешенная сумма слухов.
    pub(super) fn reputation(&self, target: u128) -> i32 {
        self.gossips.get(&target).map_or(0, |values| {
            GossipType::ALL
                .iter()
                .map(|kind| values[kind.id()] * kind.weight())
                .sum()
        })
    }

    /// `Villager.maybeDecayGossip`: раз в 24000 тиков.
    fn maybe_decay_gossip(&mut self, now: i64) {
        if self.last_gossip_decay == 0 {
            self.last_gossip_decay = now;
            return;
        }
        if now < self.last_gossip_decay + 24_000 {
            return;
        }
        self.gossips.retain(|_, values| {
            for kind in GossipType::ALL {
                let next = values[kind.id()] - kind.decay_per_day();
                values[kind.id()] = if next < 2 { 0 } else { next };
            }
            values.iter().any(|&v| v != 0)
        });
        self.last_gossip_decay = now;
    }
}

/// `GiveGiftToHero.getLootTableToThrow`: подарок по профессии.
fn gift_table(profession: &str) -> String {
    let short = profession.strip_prefix("minecraft:").unwrap_or(profession);
    if short == "none" {
        "minecraft:gameplay/hero_of_the_village/unemployed_gift".to_owned()
    } else {
        format!("minecraft:gameplay/hero_of_the_village/{short}_gift")
    }
}

/// `GossipType` 26.3: вес, предел и суточный спад.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GossipType {
    MajorNegative,
    MinorNegative,
    MinorPositive,
    MajorPositive,
    Trading,
}

impl GossipType {
    pub const ALL: [GossipType; 5] = [
        GossipType::MajorNegative,
        GossipType::MinorNegative,
        GossipType::MinorPositive,
        GossipType::MajorPositive,
        GossipType::Trading,
    ];

    /// Индекс в массиве значений `Villager.gossips`.
    fn id(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            GossipType::MajorNegative => "major_negative",
            GossipType::MinorNegative => "minor_negative",
            GossipType::MinorPositive => "minor_positive",
            GossipType::MajorPositive => "major_positive",
            GossipType::Trading => "trading",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "major_negative" => GossipType::MajorNegative,
            "minor_negative" => GossipType::MinorNegative,
            "minor_positive" => GossipType::MinorPositive,
            "major_positive" => GossipType::MajorPositive,
            "trading" => GossipType::Trading,
            _ => return None,
        })
    }

    fn weight(self) -> i32 {
        match self {
            GossipType::MajorNegative => -5,
            GossipType::MinorNegative => -1,
            GossipType::MinorPositive => 1,
            GossipType::MajorPositive => 5,
            GossipType::Trading => 1,
        }
    }

    fn max(self) -> i32 {
        match self {
            GossipType::MajorNegative => 100,
            GossipType::MinorNegative => 200,
            GossipType::MinorPositive => 25,
            GossipType::MajorPositive => 20,
            GossipType::Trading => 25,
        }
    }

    fn decay_per_day(self) -> i32 {
        match self {
            GossipType::MajorNegative => 10,
            GossipType::MinorNegative => 20,
            GossipType::MinorPositive => 1,
            GossipType::MajorPositive => 0,
            GossipType::Trading => 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MerchantWindow {
    pub villager: i32,
    pub selected: Option<usize>,
}

impl PlayPlan {
    fn type_of(&self, trader: TraderKind) -> Option<i32> {
        self.survival
            .entity_types
            .get(trader.entity_name())
            .copied()
    }

    fn profession_id(&self, name: &str) -> i32 {
        self.survival
            .villager_professions
            .get(name)
            .copied()
            .unwrap_or(0)
    }

    /// Сделки для профессии: у нас детально описаны только фермер и
    /// странствующий торговец, остальные получают фермерский набор.
    /// Все сделки уровней `1..=level` (`Villager.updateTrades`).
    fn offers_for(&self, profession: &str, level: i32, kind: &str, origin: [f64; 3]) -> Vec<Offer> {
        if profession == "minecraft:none" {
            return Vec::new();
        }
        if profession == "minecraft:wandering_trader" {
            return self.wandering_offers();
        }
        self.offers_for_levels(profession, 1, level.max(1), kind, origin)
    }

    /// Сделки для диапазона уровней (при повышении уровня добавляются
    /// только новые).
    fn offers_for_levels(
        &self,
        profession: &str,
        from: i32,
        to: i32,
        kind: &str,
        origin: [f64; 3],
    ) -> Vec<Offer> {
        let mut out = Vec::new();
        for level in from.max(1)..=to.max(1) {
            out.extend(self.offers_for_level(profession, level, kind, origin));
        }
        out
    }

    /// Один `TradeSet` уровня: `amount` сделок без повторов, если
    /// `allow_duplicates` не задан.
    fn offers_for_level(
        &self,
        profession: &str,
        level: i32,
        kind: &str,
        origin: [f64; 3],
    ) -> Vec<Offer> {
        let short = profession.strip_prefix("minecraft:").unwrap_or(profession);
        let Some(set) = self.survival.trades.set(short, level) else {
            return Vec::new();
        };
        let allowed: Vec<&String> = set
            .trades
            .iter()
            .filter(|id| {
                self.survival.trades.defs.get(*id).is_some_and(|def| {
                    !def.unsupported
                        && def
                            .variants
                            .as_ref()
                            .is_none_or(|variants| variants.iter().any(|v| v == kind))
                })
            })
            .collect();
        if allowed.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut pool = allowed.clone();
        while (out.len() as i32) < set.amount {
            if pool.is_empty() {
                break;
            }
            let index = rand::random::<usize>() % pool.len();
            let id = pool[index];
            match self
                .survival
                .trades
                .defs
                .get(id)
                .and_then(|def| self.offer_from_def(def, origin))
            {
                Some(offer) => {
                    out.push(offer);
                    if !set.allow_duplicates {
                        pool.remove(index);
                    }
                }
                None => {
                    pool.remove(index);
                }
            }
        }
        out
    }

    /// `VillagerTrade.getOffer`: предметы, цена, `max_uses`, `xp` и
    /// зачарование из `given_item_modifier`.
    fn offer_from_def(
        &self,
        def: &chunkwake_data::trades::TradeDef,
        origin: [f64; 3],
    ) -> Option<Offer> {
        let buy_item = self.item_id(&def.wants.id)?;
        let sell_item = self.item_id(&def.gives.id)?;
        let mut sell = Stack::new(sell_item, def.gives.count.max(1));
        let mut extra = chunkwake_protocol::components::Extra::default();
        let mut decorated = false;
        let mut extra_cost = 0i32;
        if let Some(spec) = &def.enchant {
            let ench = self.pick_enchantment(spec, &def.gives.id)?;
            let level = 1 + rand::random::<i32>() % ench.max_level.max(1);
            if def.gives.id == "minecraft:enchanted_book" {
                extra.stored = vec![(ench.id, level)];
            } else {
                extra.enchantments = vec![(ench.id, level)];
            }
            decorated = true;
            if spec.include_cost {
                extra_cost = if spec.random {
                    // `EnchantRandomlyFunction`: 2 + [0, 5+10*level) + 3*level,
                    // затем x2 для `#double_trade_price`.
                    let mut cost = 2 + rand::random::<i32>() % (5 + level * 10) + 3 * level;
                    if def.double_price.iter().any(|name| name == &ench.name) {
                        cost *= 2;
                    }
                    cost
                } else {
                    let (min, max) = spec.levels.unwrap_or((1, 1));
                    min + rand::random::<i32>() % (max - min + 1).max(1)
                };
            }
        }
        // `ExplorationMapFunction`: ближайшая структура тега, карта с
        // `map_id` и значком `+` нужного типа (`map_decoration_type`).
        if let Some(spec) = &def.exploration {
            let origin = [
                origin[0].floor() as i32,
                origin[1].floor() as i32,
                origin[2].floor() as i32,
            ];
            let pos = self.locate_structure(&spec.destination, origin)?;
            let decoration = *self.survival.map_decorations.get(&spec.decoration)?;
            let map_id = self.create_map(pos[0], pos[2], spec.zoom, true, true);
            self.add_map_target(map_id, decoration, pos[0], pos[2], "+");
            extra.map_id = Some(map_id);
            decorated = true;
        }
        if decorated {
            sell.extra = chunkwake_protocol::components::intern(extra);
        }
        let buy = def.wants.count.max(0) + extra_cost;
        if buy < 1 {
            return None;
        }
        Some(Offer {
            buy: (buy_item, buy),
            buy_b: def
                .additional
                .as_ref()
                .and_then(|item| Some((self.item_id(&item.id)?, item.count.max(1)))),
            sell,
            uses: 0,
            max_uses: def.max_uses.max(1),
            xp: def.xp.max(0),
            price_mul: def.price_mul,
            demand: 0,
            special_price: 0,
        })
    }

    /// Случайное зачарование из `options`, совместимое с предметом.
    fn pick_enchantment(
        &self,
        spec: &chunkwake_data::trades::EnchantSpec,
        item: &str,
    ) -> Option<&chunkwake_data::enchanting::EnchantDef> {
        let pool: Vec<&chunkwake_data::enchanting::EnchantDef> = self
            .survival
            .enchanting
            .defs
            .iter()
            .filter(|def| {
                (spec.options.is_empty() || spec.options.iter().any(|name| name == &def.name))
                    && (!spec.only_compatible
                        || item == "minecraft:enchanted_book"
                        || def.supported.contains(item))
            })
            .collect();
        if pool.is_empty() {
            return None;
        }
        let total: i32 = pool.iter().map(|def| def.weight.max(1)).sum();
        let mut roll = rand::random::<i32>() % total.max(1);
        for def in pool {
            roll -= def.weight.max(1);
            if roll < 0 {
                return Some(def);
            }
        }
        None
    }

    fn claim_poi(&self, pos: [i32; 3], id: i32) {
        self.dim_state().poi_claims.lock().unwrap().insert(pos, id);
    }

    /// `PoiManager.release`: освободить рабочую точку.
    pub(crate) fn release_poi(&self, pos: [i32; 3]) {
        self.dim_state().poi_claims.lock().unwrap().remove(&pos);
    }

    /// `PoiManager.getInRange`: ближайшая свободная точка нужных типов.
    fn free_poi(
        &self,
        types: &std::collections::HashSet<String>,
        origin: [i32; 3],
        radius: i32,
    ) -> Option<[i32; 3]> {
        let (cx, cz) = (origin[0] >> 4, origin[2] >> 4);
        let reach = radius.div_euclid(16) + 1;
        let map = self.dim_state().pois.lock().unwrap();
        let claims = self.dim_state().poi_claims.lock().unwrap();
        let mut best: Option<([i32; 3], i64)> = None;
        for ((x, z), list) in map.iter() {
            if (x - cx).abs() > reach || (z - cz).abs() > reach {
                continue;
            }
            for (pos, kind) in list {
                if !types.contains(*kind) || claims.contains_key(pos) {
                    continue;
                }
                let dist = (pos[0] as i64 - origin[0] as i64).pow(2)
                    + (pos[1] as i64 - origin[1] as i64).pow(2)
                    + (pos[2] as i64 - origin[2] as i64).pow(2);
                if dist <= (radius as i64).pow(2) && best.is_none_or(|(_, d)| dist < d) {
                    best = Some((*pos, dist));
                }
            }
        }
        best.map(|(pos, _)| pos)
    }

    /// `Villager` + `AcquirePoi`/`WorkAtPoi`: профессия по рабочему блоку.
    ///
    /// Безработный занимает ближайшую свободную точку в 48 блоках и
    /// получает её профессию; если блок исчез или сменился, профессия
    /// сбрасывается в `none`, а место освобождается. Возвращает данные для
    /// `DATA_VILLAGER_DATA`, если профессия/уровень изменились.
    fn update_villager_job(&self, id: i32, v: &mut Villager) -> Option<(i32, String, String, i32)> {
        if v.trader != TraderKind::Villager {
            return None;
        }
        if let Some(site) = v.job_site {
            let state = self.block(site)?;
            let kind = poi::of_block(self.state_name(state));
            if kind == Some(v.profession.as_str()) {
                return None;
            }
            self.release_poi(site);
            v.job_site = None;
            v.profession = "minecraft:none".into();
            v.xp = 0;
            v.level = 1;
            v.offers.clear();
            return Some((id, v.kind.clone(), v.profession.clone(), v.level));
        }
        let types = if v.profession == "minecraft:none" {
            poi::job_types()
        } else {
            poi::query(&self.poi_tags, &v.profession)?
        };
        let origin = [
            v.pos[0].floor() as i32,
            v.pos[1].floor() as i32,
            v.pos[2].floor() as i32,
        ];
        let site = self.free_poi(&types, origin, 48)?;
        let kind = self
            .block(site)
            .and_then(|s| poi::of_block(self.state_name(s)))?;
        if v.profession != "minecraft:none" && kind != v.profession {
            return None;
        }
        self.claim_poi(site, id);
        v.job_site = Some(site);
        if v.profession != kind {
            v.profession = kind.to_owned();
            v.xp = 0;
            v.level = 1;
            v.offers = self.offers_for(&v.profession, v.level, &v.kind, v.pos);
            return Some((id, v.kind.clone(), v.profession.clone(), v.level));
        }
        None
    }

    fn villager_type_id(&self, name: &str) -> i32 {
        self.survival.villager_types.get(name).copied().unwrap_or(0)
    }

    /// Сделки странствующего торговца (`TradeSets.WANDERING_TRADER_*`):
    /// 2 покупки, 5 обычных и 2 редких, каждый раз случайная выборка.
    fn wandering_offers(&self) -> Vec<Offer> {
        // (цена, товар) для покупки у игрока: `wants` → `gives`.
        const BUYING: &[(&str, i32, &str, i32, i32)] = &[
            ("minecraft:water_bottle", 1, "minecraft:emerald", 1, 2),
            ("minecraft:water_bucket", 1, "minecraft:emerald", 2, 2),
            ("minecraft:milk_bucket", 1, "minecraft:emerald", 2, 2),
            (
                "minecraft:fermented_spider_eye",
                1,
                "minecraft:emerald",
                3,
                2,
            ),
            ("minecraft:baked_potato", 4, "minecraft:emerald", 1, 2),
            ("minecraft:hay_block", 1, "minecraft:emerald", 1, 2),
        ];
        const COMMON: &[(&str, i32, &str, i32, i32)] = &[
            ("minecraft:emerald", 1, "minecraft:white_dye", 3, 12),
            ("minecraft:emerald", 1, "minecraft:orange_dye", 3, 12),
            ("minecraft:emerald", 1, "minecraft:blue_dye", 3, 12),
            ("minecraft:emerald", 1, "minecraft:red_dye", 3, 12),
            ("minecraft:emerald", 1, "minecraft:glowstone", 1, 12),
            ("minecraft:emerald", 1, "minecraft:sea_pickle", 1, 12),
            ("minecraft:emerald", 1, "minecraft:slime_ball", 1, 12),
            ("minecraft:emerald", 1, "minecraft:sugar_cane", 1, 12),
            ("minecraft:emerald", 1, "minecraft:pumpkin", 1, 12),
            ("minecraft:emerald", 1, "minecraft:kelp", 1, 12),
            ("minecraft:emerald", 1, "minecraft:cactus", 1, 12),
            ("minecraft:emerald", 1, "minecraft:sand", 8, 12),
            ("minecraft:emerald", 1, "minecraft:fern", 1, 12),
            ("minecraft:emerald", 1, "minecraft:oak_sapling", 1, 12),
        ];
        const UNCOMMON: &[(&str, i32, &str, i32, i32)] = &[
            ("minecraft:emerald", 1, "minecraft:packed_ice", 1, 6),
            ("minecraft:emerald", 1, "minecraft:blue_ice", 1, 6),
            ("minecraft:emerald", 1, "minecraft:gunpowder", 4, 2),
            ("minecraft:emerald", 1, "minecraft:podzol", 3, 6),
            ("minecraft:emerald", 1, "minecraft:oak_log", 8, 12),
            ("minecraft:emerald", 1, "minecraft:birch_log", 8, 12),
            ("minecraft:emerald", 1, "minecraft:acacia_log", 8, 12),
            ("minecraft:emerald", 5, "minecraft:nautilus_shell", 1, 5),
            ("minecraft:emerald", 1, "minecraft:name_tag", 1, 5),
        ];
        let item = |n: &str| self.item_id(n);
        let mut out = Vec::new();
        let mut add = |table: &[(&str, i32, &str, i32, i32)], amount: usize| {
            let mut picks: Vec<usize> = (0..table.len()).collect();
            for i in (1..picks.len()).rev() {
                let j = rand::random::<usize>() % (i + 1);
                picks.swap(i, j);
            }
            for &k in picks.iter().take(amount) {
                let (buy_name, buy_count, sell_name, sell_count, max_uses) = table[k];
                let (Some(buy), Some(sell)) = (item(buy_name), item(sell_name)) else {
                    continue;
                };
                out.push(Offer {
                    buy: (buy, buy_count),
                    buy_b: None,
                    sell: Stack::new(sell, sell_count),
                    uses: 0,
                    max_uses,
                    xp: 0,
                    price_mul: 0.05,
                    demand: 0,
                    special_price: 0,
                });
            }
        };
        add(BUYING, 2);
        add(UNCOMMON, 2);
        add(COMMON, 5);
        out
    }

    fn villager_named(&self, trader: TraderKind, kind: &str, profession: &str) -> String {
        if trader == TraderKind::Wandering {
            return "entity.minecraft.wandering_trader".into();
        }
        let short = |n: &str| n.trim_start_matches("minecraft:").to_owned();
        if profession == "minecraft:none" {
            return format!("entity.minecraft.villager.{}", short(kind));
        }
        format!("entity.minecraft.villager.{}", short(profession))
    }

    /// `/summon minecraft:villager <pos> <nbt>`.
    pub(super) fn summon_villager(&self, pos: [f64; 3], nbt: Option<&Compound>) -> Option<i32> {
        self.summon_trader(TraderKind::Villager, pos, nbt)
    }

    /// `/summon minecraft:wandering_trader <pos> <nbt>`.
    pub(super) fn summon_wandering_trader(
        &self,
        pos: [f64; 3],
        nbt: Option<&Compound>,
    ) -> Option<i32> {
        self.summon_trader(TraderKind::Wandering, pos, nbt)
    }

    fn summon_trader(
        &self,
        trader: TraderKind,
        pos: [f64; 3],
        nbt: Option<&Compound>,
    ) -> Option<i32> {
        let mut v = Villager {
            pos,
            yaw: 0.0,
            uuid: Uuid::from_u128(rand::random()),
            trader,
            kind: "minecraft:plains".into(),
            profession: "minecraft:farmer".into(),
            level: 1,
            xp: 0,
            offers: Vec::new(),
            despawn_delay: 0,
            last_restock: 0,
            restocks_today: 0,
            gift_timer: 600,
            gift_wait: None,
            gossips: BTreeMap::new(),
            last_gossip_decay: 0,
            health: 20.0,
            last_work_check: self.game_time(),
            job_site: None,
            last_job_check: 0,
            custom_name: None,
            persistent: trader == TraderKind::Villager,
        };
        if let Some(nbt) = nbt {
            apply_villager_nbt(&mut v, nbt, |n| self.item_id(n));
        }
        if v.profession == "minecraft:none" {
            v.offers.clear();
        } else if v.offers.is_empty() {
            v.offers = self.offers_for(&v.profession, v.level, &v.kind, v.pos);
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
                range: self.tracking_range(self.type_of(v.trader)?),
            },
            self.villager_packets(id, &v).unwrap_or_default(),
        );
        Some(id)
    }

    fn villager_packets(&self, id: i32, v: &Villager) -> Option<Vec<Vec<u8>>> {
        let mut out = vec![play::add_entity_moving(
            self.ids.add_entity,
            id,
            v.uuid,
            self.type_of(v.trader)?,
            v.pos,
            [0.0; 3],
            v.yaw,
            0.0,
            0,
        )];
        if v.trader == TraderKind::Villager {
            // `Villager.DATA_VILLAGER_DATA` (индекс 18) и
            // `DATA_VILLAGER_DATA_FINALIZED` (19).
            out.push(play::entity_meta(
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
            ));
        }
        Some(out)
    }

    /// Тик загруженных жителей: спавн, исчезновение торговца и пополнение
    /// запасов у рабочего блока (`WorkAtPoi` + `Villager.shouldRestock`).
    pub(super) fn tick_villagers(&self) {
        let loaded = std::mem::take(&mut *self.sim().villagers_loaded.lock().unwrap());
        for v in loaded {
            let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
            let packets = self.villager_packets(id, &v).unwrap_or_default();
            let tracked = Tracked {
                id,
                pos: v.pos,
                range: self.tracking_range(self.type_of(v.trader).unwrap_or(0)),
            };
            self.sim().villagers.lock().unwrap().insert(id, v);
            self.hub.spawn_tracked(tracked, packets);
            self.mark_work();
        }
        if self.sim().villagers.lock().unwrap().is_empty() {
            return;
        }
        let now = self.game_time();
        let ids: Vec<i32> = self
            .sim()
            .villagers
            .lock()
            .unwrap()
            .keys()
            .copied()
            .collect();
        let mut gone = Vec::new();
        let mut work: Vec<([f64; 3], String)> = Vec::new();
        let mut jobs: Vec<(i32, String, String, i32)> = Vec::new();
        let hero_id = self
            .survival
            .effects
            .get("minecraft:hero_of_the_village")
            .copied();
        let heroes: Vec<[f64; 3]> = match hero_id {
            Some(id) => self
                .live
                .lock()
                .unwrap()
                .values()
                .filter(|l| l.effects.iter().any(|e| e.id == id))
                .map(|l| l.pos)
                .collect(),
            None => Vec::new(),
        };
        let mut gifts: Vec<([f64; 3], [f64; 3], String)> = Vec::new();
        for id in ids {
            let mut villagers = self.sim().villagers.lock().unwrap();
            let Some(v) = villagers.get_mut(&id) else {
                continue;
            };
            if v.despawn_delay > 0 {
                v.despawn_delay -= 1;
                if v.despawn_delay == 0 {
                    gone.push(id);
                    continue;
                }
            }
            if v.trader == TraderKind::Villager && now - v.last_job_check >= 20 {
                v.last_job_check = now;
                if let Some(change) = self.update_villager_job(id, v) {
                    jobs.push(change);
                }
            }
            v.maybe_decay_gossip(now);
            if v.trader == TraderKind::Villager {
                let hero = heroes
                    .iter()
                    .filter(|p| crate::mobs::dist2(**p, v.pos) <= 8.0 * 8.0)
                    .min_by(|a, b| {
                        crate::mobs::dist2(**a, v.pos).total_cmp(&crate::mobs::dist2(**b, v.pos))
                    })
                    .copied();
                match hero {
                    Some(hero_pos) => {
                        if let Some(wait) = &mut v.gift_wait {
                            *wait -= 1;
                            if *wait <= 0 && crate::mobs::dist2(hero_pos, v.pos) <= 5.0 * 5.0 {
                                gifts.push((v.pos, hero_pos, v.profession.clone()));
                                v.gift_timer = 600 + (rand::random::<u32>() % 6001) as i32;
                                v.gift_wait = None;
                            }
                        } else if v.gift_timer > 0 {
                            v.gift_timer -= 1;
                        } else {
                            v.gift_wait = Some(20);
                        }
                    }
                    None => v.gift_wait = None,
                }
            }
            if v.trader == TraderKind::Villager && now - v.last_work_check >= 300 {
                v.last_work_check = now;
                if let Some(ws) = workstation(&v.profession)
                    && self.near_block(v.pos, ws)
                {
                    let needs = v.offers.iter().any(|o| o.uses > 0);
                    let day = now.div_euclid(24_000);
                    if day > v.last_restock.div_euclid(24_000) {
                        v.last_restock = now;
                        v.restocks_today = 0;
                    }
                    let allowed = v.restocks_today == 0
                        || (v.restocks_today < 2 && now > v.last_restock + 2_400);
                    if allowed && needs {
                        for o in &mut v.offers {
                            o.demand += o.uses - (o.max_uses - o.uses);
                            o.uses = 0;
                        }
                        v.last_restock = now;
                        v.restocks_today += 1;
                        work.push((v.pos, v.profession.clone()));
                    }
                }
            }
        }
        for (vpos, ppos, profession) in gifts {
            let table = gift_table(&profession);
            let no_prop = |_: &str| None;
            let level = |_: &str| 0;
            let smelt = |_: &str| None;
            let ctx = crate::loot::Ctx {
                prop: &no_prop,
                block: &table,
                tool: None,
                level: &level,
                entity: None,
                killed_by_player: false,
                smelt: &smelt,
            };
            let mut rng = rand::thread_rng();
            for (name, count) in crate::loot::roll(&self.survival.loot, &ctx, &mut rng) {
                let Some(item) = self.item_id(&name) else {
                    continue;
                };
                let dx = ppos[0] - vpos[0];
                let dy = ppos[1] + 1.0 - (vpos[1] + 1.2);
                let dz = ppos[2] - vpos[2];
                let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-4);
                let vel = [dx / len * 0.3, dy / len * 0.3 + 0.1, dz / len * 0.3];
                self.drop_item(
                    [vpos[0], vpos[1] + 1.2, vpos[2]],
                    vel,
                    Stack::new(item, count.min(self.max_stack(item).max(1))),
                    items::BLOCK_DELAY,
                );
            }
        }
        if !gone.is_empty() {
            let mut villagers = self.sim().villagers.lock().unwrap();
            for id in &gone {
                villagers.remove(id);
            }
            drop(villagers);
            self.hub.remove_tracked(&gone);
            self.mark_work();
        }
        // Точки исчезнувших жителей освобождаем (обычно их снимает
        // `release_poi` при сбросе профессии, здесь — страховка).
        if now % 100 == 0 {
            let alive: std::collections::HashSet<i32> = self
                .sim()
                .villagers
                .lock()
                .unwrap()
                .keys()
                .copied()
                .collect();
            self.dim_state()
                .poi_claims
                .lock()
                .unwrap()
                .retain(|_, id| alive.contains(id));
        }
        for (id, kind, profession, level) in jobs {
            self.hub.send(
                None,
                play::entity_meta(
                    self.ids.set_entity_data,
                    id,
                    &[(
                        18,
                        play::Meta::VillagerData {
                            kind: self.villager_type_id(&kind),
                            profession: self.profession_id(&profession),
                            level,
                        },
                    )],
                ),
            );
        }
        for (pos, profession) in work {
            if let Some(sound) = work_sound(&profession) {
                self.play_sound(None, &sound, crate::sounds::NEUTRAL, pos, 1.0, 1.0);
            }
            let at = [pos[0], pos[1] + 1.0, pos[2]];
            if let Some(&particle) = self
                .particle_types
                .lock()
                .unwrap()
                .get("minecraft:happy_villager")
            {
                self.hub.send(
                    None,
                    play::level_particles(
                        self.ids.level_particles,
                        particle,
                        false,
                        at,
                        [0.0; 3],
                        0.0,
                        3,
                    ),
                );
            }
            self.mark_work();
        }
    }

    /// Есть ли рядом (в кубе ±2) блок рабочего места.
    fn near_block(&self, pos: [f64; 3], name: &str) -> bool {
        let c = [
            pos[0].floor() as i32,
            pos[1].floor() as i32,
            pos[2].floor() as i32,
        ];
        for dy in -2..=2 {
            for dx in -2..=2 {
                for dz in -2..=2 {
                    let p = [c[0] + dx, c[1] + dy, c[2] + dz];
                    if self.block(p).is_some_and(|s| self.state_name(s) == name) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(super) fn track_villagers(&self, out: &mut Vec<Tracked>) {
        for (&id, v) in self.sim().villagers.lock().unwrap().iter() {
            out.push(Tracked {
                id,
                pos: v.pos,
                range: self.tracking_range(self.type_of(v.trader).unwrap_or(0)),
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
                let key = self.villager_named(v.trader, &v.kind, &v.profession);
                serde_json::json!({"translate": key})
            })
    }

    /// `Villager.startTrading`: обновить скидки героя деревни.
    fn hero_amplifier(&self, uuid: Uuid) -> Option<u8> {
        let id = *self.survival.effects.get("minecraft:hero_of_the_village")?;
        let live = self.live.lock().unwrap();
        live.get(&uuid)?
            .effects
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.amplifier)
    }

    /// `Villager.hurt`: урон жителю, слухи о нападавшем и смерть.
    pub(super) fn hurt_villager(&self, id: i32, amount: f32, attacker: Option<Uuid>) -> bool {
        let (pos, died) = {
            let mut villagers = self.sim().villagers.lock().unwrap();
            let Some(v) = villagers.get_mut(&id) else {
                return false;
            };
            if v.health <= 0.0 {
                return false;
            }
            v.health = (v.health - amount.max(0.0)).max(0.0);
            if let Some(player) = attacker {
                // `Villager.setLastHurtByMob` -> `VILLAGER_HURT`.
                v.add_gossip(player.as_u128(), GossipType::MinorNegative, 25);
            }
            (v.pos, v.health <= 0.0)
        };
        self.play_sound(
            None,
            if died {
                "minecraft:entity.villager.death"
            } else {
                "minecraft:entity.villager.hurt"
            },
            crate::sounds::NEUTRAL,
            pos,
            1.0,
            1.0,
        );
        self.emit_vibration(pos, "minecraft:entity_damage");
        if !died {
            self.hub
                .send_about(id, play::hurt_animation(self.ids.hurt_animation, id, 0.0));
            return true;
        }
        self.emit_vibration(pos, "minecraft:entity_die");
        // `tellWitnessesThatIWasMurdered`: VILLAGER_KILLED свидетелям.
        if let Some(player) = attacker {
            let mut villagers = self.sim().villagers.lock().unwrap();
            for (&other, v) in villagers.iter_mut() {
                if other != id && crate::mobs::dist2(v.pos, pos) <= 16.0 * 16.0 {
                    v.add_gossip(player.as_u128(), GossipType::MajorNegative, 25);
                }
            }
        }
        self.hub
            .send_about(id, inv::entity_event(self.ids.entity_event, id, 3));
        self.remove_entity(id);
        self.hub
            .send(None, play::remove_entities(self.ids.remove_entities, &[id]));
        true
    }

    /// `ClientboundMerchantOffersPacket` для окна.
    pub(super) fn villager_offers_packet(
        &self,
        window: i32,
        id: i32,
        uuid: Uuid,
    ) -> Option<Vec<u8>> {
        let amplifier = self.hero_amplifier(uuid);
        let mut villagers = self.sim().villagers.lock().unwrap();
        let v = villagers.get_mut(&id)?;
        let reputation = v.reputation(uuid.as_u128());
        v.update_special_prices(amplifier, reputation);
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
                price_diff: o.special_price,
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
    /// `/data merge entity` для жителя: применяет NBT к живому состоянию
    /// (`Offers`, `VillagerData`, `Xp`, `Health`, `Pos`, `Gossips`, ...).
    pub(super) fn store_villager_nbt(&self, id: i32, nbt: &Compound) -> bool {
        let meta = {
            let mut villagers = self.sim().villagers.lock().unwrap();
            let Some(v) = villagers.get_mut(&id) else {
                return false;
            };
            apply_villager_nbt(v, nbt, |n| self.item_id(n));
            if let Some(health) = nbt.get("Health").and_then(number) {
                v.health = (health as f32).clamp(0.0, 20.0);
            }
            if let Some(pos) = list3(nbt.get("Pos")) {
                v.pos = pos;
            }
            if let Some(Tag::List(rot)) = nbt.get("Rotation")
                && let Some(Tag::Float(yaw)) = rot.first()
            {
                v.yaw = *yaw;
            }
            if let Some(name) = nbt.get("CustomName").and_then(|t| match t {
                Tag::String(s) => Some(s.clone()),
                _ => None,
            }) {
                v.custom_name = Some(name);
            }
            (
                v.trader == TraderKind::Villager,
                self.villager_type_id(&v.kind),
                self.profession_id(&v.profession),
                v.level,
                v.pos,
                v.yaw,
            )
        };
        if meta.0 {
            self.hub.send_about(
                id,
                play::entity_meta(
                    self.ids.set_entity_data,
                    id,
                    &[
                        (
                            18,
                            play::Meta::VillagerData {
                                kind: meta.1,
                                profession: meta.2,
                                level: meta.3,
                            },
                        ),
                        (19, play::Meta::Bool(true)),
                    ],
                ),
            );
        }
        self.hub.send_about(
            id,
            play::teleport_entity(self.ids.teleport_entity, id, meta.4, meta.5, 0.0, true),
        );
        true
    }

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
    pub(super) fn notify_trade(&self, id: i32, idx: usize, player: Uuid) -> Option<(Offer, bool)> {
        let amplifier = self.hero_amplifier(player);
        let mut villagers = self.sim().villagers.lock().unwrap();
        let v = villagers.get_mut(&id)?;
        let offer = *v.offers.get(idx)?;
        if let Some(o) = v.offers.get_mut(idx) {
            o.uses += 1;
        }
        // `Villager.onReputationEventFrom(TRADE)`: +2 к `trading`.
        v.add_gossip(player.as_u128(), GossipType::Trading, 2);
        v.xp += offer.xp;
        let level = level_for_xp(v.xp);
        let changed = level != v.level;
        let old_level = v.level;
        v.level = level;
        if changed {
            v.offers.extend(self.offers_for_levels(
                &v.profession,
                old_level + 1,
                level,
                &v.kind,
                v.pos,
            ));
        }
        let reputation = v.reputation(player.as_u128());
        v.update_special_prices(amplifier, reputation);
        let pos = v.pos;
        let wandering = v.trader == TraderKind::Wandering;
        drop(villagers);
        if wandering {
            // `WanderingTrader.rewardTradeXp`: 3 + nextInt(4).
            self.spawn_orbs(
                [pos[0], pos[1] + 0.5, pos[2]],
                3 + rand::random::<i32>().rem_euclid(4),
            );
        }
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
            c.insert("specialPrice".into(), Tag::Int(o.special_price));
            recipes.push(Tag::Compound(c));
        }
        let mut offers = Compound::new();
        offers.insert("Recipes".into(), Tag::List(recipes));
        Tag::Compound(offers)
    }

    /// NBT одного жителя.
    fn villager_to_nbt(&self, v: &Villager) -> Compound {
        let mut c = Compound::new();
        c.insert("id".into(), Tag::String(v.trader.entity_name().into()));
        c.insert("UUID".into(), Tag::IntArray(uuid_ints(v.uuid)));
        c.insert(
            "Pos".into(),
            Tag::List(v.pos.into_iter().map(Tag::Double).collect()),
        );
        c.insert(
            "Rotation".into(),
            Tag::List(vec![Tag::Float(v.yaw), Tag::Float(0.0)]),
        );
        if v.trader == TraderKind::Villager {
            let mut data = Compound::new();
            data.insert("type".into(), Tag::String(v.kind.clone()));
            data.insert("profession".into(), Tag::String(v.profession.clone()));
            data.insert("level".into(), Tag::Int(v.level));
            c.insert("VillagerData".into(), Tag::Compound(data));
            c.insert("Xp".into(), Tag::Int(v.xp));
            c.insert("LastRestock".into(), Tag::Long(v.last_restock));
            c.insert("RestocksToday".into(), Tag::Int(v.restocks_today));
        } else {
            c.insert("DespawnDelay".into(), Tag::Int(v.despawn_delay));
        }
        c.insert("Offers".into(), self.offers_nbt(v));
        if !v.gossips.is_empty() {
            let list: Vec<Tag> = v
                .gossips
                .iter()
                .flat_map(|(&target, values)| {
                    GossipType::ALL.into_iter().filter_map(move |kind| {
                        let value = values[kind.id()];
                        if value == 0 {
                            return None;
                        }
                        let mut e = Compound::new();
                        e.insert(
                            "Target".into(),
                            Tag::IntArray(uuid_ints(Uuid::from_u128(target))),
                        );
                        e.insert("Type".into(), Tag::String(kind.name().into()));
                        e.insert("Value".into(), Tag::Int(value));
                        Some(Tag::Compound(e))
                    })
                })
                .collect();
            c.insert("Gossips".into(), Tag::List(list));
        }
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

    /// Загрузка жителя или торговца из `entities/*.mca`; `true` — распознан.
    pub(super) fn load_villager(&self, c: &Compound) -> bool {
        let trader = match c.get("id") {
            Some(Tag::String(id)) if id == "minecraft:villager" => TraderKind::Villager,
            Some(Tag::String(id)) if id == "minecraft:wandering_trader" => TraderKind::Wandering,
            _ => return false,
        };
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
            trader,
            kind: "minecraft:plains".into(),
            profession: "minecraft:farmer".into(),
            level: 1,
            xp: 0,
            offers: Vec::new(),
            despawn_delay: 0,
            last_restock: 0,
            restocks_today: 0,
            last_work_check: self.game_time(),
            job_site: None,
            last_job_check: 0,
            custom_name: None,
            persistent: trader == TraderKind::Villager,
            gift_timer: 600,
            gift_wait: None,
            gossips: BTreeMap::new(),
            last_gossip_decay: 0,
            health: 20.0,
        };
        apply_villager_nbt(&mut v, c, |n| self.item_id(n));
        if v.profession == "minecraft:none" {
            v.offers.clear();
        } else if v.offers.is_empty() {
            v.offers = self.offers_for(&v.profession, v.level, &v.kind, v.pos);
        }
        self.sim().villagers_loaded.lock().unwrap().push(v);
        true
    }
}

/// Число из NBT (Int/Float/Double/Long).
fn number(tag: &Tag) -> Option<f64> {
    match tag {
        Tag::Int(v) => Some(f64::from(*v)),
        Tag::Float(v) => Some(f64::from(*v)),
        Tag::Double(v) => Some(*v),
        Tag::Long(v) => Some(*v as f64),
        _ => None,
    }
}

/// Три координаты из NBT-списка.
fn list3(tag: Option<&Tag>) -> Option<[f64; 3]> {
    let Tag::List(list) = tag? else {
        return None;
    };
    let mut out = [0.0; 3];
    for (i, tag) in list.iter().take(3).enumerate() {
        out[i] = number(tag)?;
    }
    (list.len() >= 3).then_some(out)
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
                    special_price: r.get("specialPrice").and_then(Tag::as_i64).unwrap_or(0) as i32,
                })
            })
            .collect();
        if !parsed.is_empty() {
            v.offers = parsed;
        }
    }
    if let Some(Tag::List(gossips)) = c.get("Gossips") {
        for entry in gossips {
            let Tag::Compound(e) = entry else { continue };
            let Some(Tag::IntArray(target)) = e.get("Target") else {
                continue;
            };
            if target.len() != 4 {
                continue;
            }
            let Some(Tag::String(kind)) = e.get("Type") else {
                continue;
            };
            let Some(kind) = GossipType::from_name(kind) else {
                continue;
            };
            let Some(value) = e.get("Value").and_then(Tag::as_i64) else {
                continue;
            };
            let uuid = ((target[0] as u32 as u128) << 96)
                | ((target[1] as u32 as u128) << 64)
                | ((target[2] as u32 as u128) << 32)
                | (target[3] as u32 as u128);
            v.gossips.entry(uuid).or_insert([0; 5])[kind.id()] =
                (value as i32).clamp(0, kind.max());
        }
    }
    if let Some(Tag::String(n)) = c.get("CustomName") {
        v.custom_name = Some(n.clone());
    }
    if let Some(delay) = c.get("DespawnDelay").and_then(Tag::as_i64) {
        v.despawn_delay = delay.max(0) as i32;
    }
    if let Some(last) = c.get("LastRestock").and_then(Tag::as_i64) {
        v.last_restock = last;
    }
    if let Some(n) = c.get("RestocksToday").and_then(Tag::as_i64) {
        v.restocks_today = n as i32;
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
    #[test]
    fn hero_discount_matches_vanilla() {
        let mut offer = Offer {
            buy: (1, 20),
            buy_b: None,
            sell: Stack::new(2, 1),
            uses: 0,
            max_uses: 12,
            xp: 2,
            price_mul: 0.05,
            demand: 0,
            special_price: 0,
        };
        offer.apply_special_prices(Some(0), 0);
        // floor(0.3 * 20) = 6.
        assert_eq!(offer.price(), 14);
        offer.apply_special_prices(Some(4), 0);
        // floor((0.3 + 4 * 0.0625) * 20) = 11.
        assert_eq!(offer.price(), 9);
        offer.apply_special_prices(None, 0);
        assert_eq!(offer.price(), 20);
        // Скидка не опускает цену ниже 1.
        let mut cheap = offer;
        cheap.buy.1 = 1;
        cheap.apply_special_prices(Some(0), 0);
        assert_eq!(cheap.price(), 1);
    }

    #[test]
    fn gossip_merge_decay_and_reputation() {
        let mut offer = Offer {
            buy: (1, 20),
            buy_b: None,
            sell: Stack::new(2, 1),
            uses: 0,
            max_uses: 12,
            xp: 2,
            price_mul: 0.05,
            demand: 0,
            special_price: 0,
        };
        // Один трейд: trading = 2, репутация +2, скидка floor(0.1) = 0.
        offer.apply_special_prices(None, 2);
        assert_eq!(offer.price(), 20);
        // Удар по жителю: minor_negative = 25, репутация -25,
        // +floor(25*0.05) = +2 к цене.
        offer.apply_special_prices(None, -25);
        assert_eq!(offer.price(), 22);
        assert_eq!(offer.special_price, 2);

        let mut v = Villager {
            pos: [0.0; 3],
            yaw: 0.0,
            uuid: Uuid::nil(),
            trader: TraderKind::Villager,
            kind: "minecraft:plains".into(),
            profession: "minecraft:farmer".into(),
            level: 1,
            xp: 0,
            offers: vec![offer],
            despawn_delay: 0,
            last_restock: 0,
            restocks_today: 0,
            last_work_check: 0,
            job_site: None,
            last_job_check: 0,
            custom_name: None,
            persistent: true,
            gift_timer: 600,
            gift_wait: None,
            gossips: BTreeMap::new(),
            last_gossip_decay: 0,
            health: 20.0,
        };
        let player = Uuid::from_u128(7);
        let target = player.as_u128();
        v.add_gossip(target, GossipType::Trading, 2);
        assert_eq!(v.reputation(target), 2);
        // `MINOR_NEGATIVE` +25 при весе -1.
        v.add_gossip(target, GossipType::MinorNegative, 25);
        assert_eq!(v.reputation(target), 2 - 25);
        // Предел `TRADING` = 25.
        for _ in 0..20 {
            v.add_gossip(target, GossipType::Trading, 2);
        }
        assert_eq!(v.gossips[&target][GossipType::Trading.id()], 25);
        // Спад раз в 24000 тиков: trading 25 -> 23, minor_negative 25 -> 5.
        v.last_gossip_decay = 1;
        v.maybe_decay_gossip(24_001);
        assert_eq!(v.gossips[&target][GossipType::Trading.id()], 23);
        assert_eq!(v.gossips[&target][GossipType::MinorNegative.id()], 5);
        // Повторный спад убирает записи со значением < 2.
        v.maybe_decay_gossip(48_002);
        assert_eq!(v.gossips[&target][GossipType::Trading.id()], 21);
        assert_eq!(v.gossips[&target][GossipType::MinorNegative.id()], 0);
    }

    #[test]
    fn gift_tables_match_professions() {
        assert_eq!(
            gift_table("minecraft:farmer"),
            "minecraft:gameplay/hero_of_the_village/farmer_gift"
        );
        assert_eq!(
            gift_table("minecraft:none"),
            "minecraft:gameplay/hero_of_the_village/unemployed_gift"
        );
    }
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
