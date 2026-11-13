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
}

/// Состояние окна торговли: к какому жителю и какая сделка выбрана.
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
    fn offers_for(&self, profession: &str) -> Vec<Offer> {
        match profession {
            "minecraft:none" => Vec::new(),
            "minecraft:wandering_trader" => self.wandering_offers(),
            _ => self.farmer_offers(),
        }
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
            v.offers = self.offers_for(&v.profession);
            return Some((id, v.kind.clone(), v.profession.clone(), v.level));
        }
        None
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
            offers: self.offers_for(match trader {
                TraderKind::Wandering => "minecraft:wandering_trader",
                TraderKind::Villager => "minecraft:farmer",
            }),
            despawn_delay: 0,
            last_restock: 0,
            restocks_today: 0,
            last_work_check: self.game_time(),
            job_site: None,
            last_job_check: 0,
            custom_name: None,
            persistent: trader == TraderKind::Villager,
        };
        if let Some(nbt) = nbt {
            apply_villager_nbt(&mut v, nbt, |n| self.item_id(n));
            if v.profession == "minecraft:none" {
                v.offers.clear();
            }
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
            offers: self.offers_for(match trader {
                TraderKind::Wandering => "minecraft:wandering_trader",
                TraderKind::Villager => "minecraft:farmer",
            }),
            despawn_delay: 0,
            last_restock: 0,
            restocks_today: 0,
            last_work_check: self.game_time(),
            job_site: None,
            last_job_check: 0,
            custom_name: None,
            persistent: trader == TraderKind::Villager,
        };
        apply_villager_nbt(&mut v, c, |n| self.item_id(n));
        if v.profession == "minecraft:none" {
            v.offers.clear();
        }
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
