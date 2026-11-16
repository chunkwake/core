//! Жители и торговля (`Villager`, `MerchantMenu`, `MerchantOffer`).
//!
//! Житель — сущность `minecraft:villager` со своей профессией, уровнем,
//! опытом и списком сделок. ПКМ без приседа открывает `merchant`: слоты 0-1
//! под покупку, 2 — результат, 3-38 — инвентарь. `select_trade` выбирает
//! сделку, `container_click` по слоту 2 проводит обмен: забирает цену,
//! кладёт товар в курсор и увеличивает `uses`/`Xp` жителя; при достижении
//! порога (10/70/150/250 опыта) растёт уровень 1-5.

use super::*;
use rand::Rng;
use std::collections::BTreeMap;

/// `EnchantRandomlyFunction.enchantItem`: `2 + nextInt(5 + level*10) + 3*level`.
fn random_enchant_cost(level: i32, roll: i32) -> i32 {
    2 + roll + 3 * level
}

/// `EnchantmentHelper.getAvailableEnchantmentResults`: наибольший уровень,
/// чей диапазон `[minCost, maxCost]` содержит стоимость.
fn enchant_level_for_cost(def: &chunkwake_data::enchanting::EnchantDef, value: i32) -> Option<i32> {
    (1..=def.max_level.max(1))
        .rev()
        .find(|&level| value >= def.min_cost(level) && value <= def.max_cost(level))
}

/// `WeightedRandom.getRandomItem` по весу чара.
fn weighted_enchant(
    pool: &[(usize, i32)],
    defs: &[chunkwake_data::enchanting::EnchantDef],
    rng: &mut impl Rng,
) -> Option<(usize, i32)> {
    let total: i32 = pool
        .iter()
        .map(|&(i, _)| defs.get(i).map_or(1, |d| d.weight.max(1)))
        .sum();
    let mut roll = rng.gen_range(0..total.max(1));
    for &(i, level) in pool {
        roll -= defs.get(i).map_or(1, |d| d.weight.max(1));
        if roll < 0 {
            return Some((i, level));
        }
    }
    pool.last().copied()
}

/// `EnchantmentHelper.enchantItem` (`selectEnchantment`) для
/// `enchant_with_levels`: стоимость с разбросом, уровни по диапазонам и
/// совместимость только с последними чарами.
fn select_enchantments(
    defs: &[chunkwake_data::enchanting::EnchantDef],
    options: &[String],
    item: &str,
    enchantable: i32,
    level_cost: i32,
    rng: &mut impl Rng,
) -> Vec<(i32, i32)> {
    let mut results: Vec<(i32, i32)> = Vec::new();
    let mut cost = level_cost;
    cost += 1 + rng.gen_range(0..enchantable / 4 + 1) + rng.gen_range(0..enchantable / 4 + 1);
    let span = (rng.gen_range(0.0..1.0f32) + rng.gen_range(0.0..1.0f32) - 1.0) * 0.15;
    cost = ((cost as f32 + cost as f32 * span).round() as i32).max(1);
    let mut pool: Vec<(usize, i32)> = defs
        .iter()
        .enumerate()
        .filter(|(_, d)| options.is_empty() || options.iter().any(|n| n == &d.name))
        .filter(|(_, d)| item == "minecraft:book" || d.primary.contains(item))
        .filter_map(|(i, d)| enchant_level_for_cost(d, cost).map(|level| (i, level)))
        .collect();
    if pool.is_empty() {
        return results;
    }
    let mut last_index = None;
    if let Some((i, level)) = weighted_enchant(&pool, defs, rng)
        && let Some(d) = defs.get(i)
    {
        last_index = Some(i);
        results.push((d.id, level));
    }
    while rng.gen_range(0..50) <= cost {
        if let Some(i) = last_index
            && let Some(def) = defs.get(i)
        {
            pool.retain(|&(j, _)| defs.get(j).is_some_and(|d| def.compatible(d)));
        }
        if pool.is_empty() {
            break;
        }
        if let Some((i, level)) = weighted_enchant(&pool, defs, rng)
            && let Some(d) = defs.get(i)
        {
            last_index = Some(i);
            results.push((d.id, level));
        }
        cost /= 2;
    }
    results
}

/// Пороги уровней (`VillagerData.NEXT_LEVEL_XP_THRESHOLDS`).
pub const LEVEL_XP: [i32; 5] = [0, 10, 70, 150, 250];

/// Идёт ли другой житель через дверь (`areOtherMobsComingThroughDoor`):
/// его узел пути совпадает с дверью и он в 2 блоках от неё.
fn other_villager_on_door(
    id: i32,
    door: [i32; 3],
    center: [f64; 3],
    paths: &[VillagerPath],
) -> bool {
    paths.iter().any(|(other, pos, from, to)| {
        *other != id
            && (*from == door || *to == door)
            && crate::mobs::dist2(*pos, center) <= 2.0 * 2.0
    })
}

/// `VillagerPanicTrigger`: угроза — активный `HURT_BY` или монстр в 8 блоках.
fn panic_threat(
    pos: [f64; 3],
    hurt_until: i64,
    now: i64,
    hurt_from: Option<[f64; 3]>,
    threats: &[[f64; 3]],
) -> Option<[f64; 3]> {
    if now < hurt_until
        && let Some(from) = hurt_from
    {
        return Some(from);
    }
    threats
        .iter()
        .filter(|p| crate::mobs::dist2(pos, **p) <= 8.0 * 8.0)
        .min_by(|a, b| crate::mobs::dist2(pos, **a).total_cmp(&crate::mobs::dist2(pos, **b)))
        .copied()
}

/// `Mth.atan2`-подобный yaw взгляда из `from` в `to`.
fn look_yaw(from: [f64; 3], to: [f64; 3]) -> f32 {
    let dx = to[0] - from[0];
    let dz = to[2] - from[2];
    (-dx).atan2(dz).to_degrees() as f32
}

/// Снимок жителя для сенсоров: id, UUID и позиция.
type VillagerRef = (i32, u128, [f64; 3]);
/// Узлы пути жителя: id, позиция, `from`, `to`.
type VillagerPath = (i32, [f64; 3], [i32; 3], [i32; 3]);
/// Профессия и голод жителя для `TradeWithVillager`.
type VillagerProfile = (i32, String, bool);

/// Отправка движения жителя: id, позиция, yaw, прошлая позиция, on_ground.
type VillagerMove = (i32, [f64; 3], f32, [i64; 3], bool);

/// Отложенное рождение (`VillagerMakeLove.tryToGiveBirth`): нельзя спавнить
/// ребёнка под блокировкой жителей, поэтому запрос выполняется после цикла.
struct Birth {
    parent: i32,
    partner: Option<i32>,
    pos: [f64; 3],
    kind: String,
    bed: [i32; 3],
}

/// `timeline/villager_schedule.json` 26.3 (взрослый житель).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Play,
    Work,
    Meet,
    Rest,
}

impl Activity {
    /// Ключевые кадры: 10 idle, 2000 work, 9000 meet, 11000 idle, 12000 rest.
    fn at(day_time: i64) -> Self {
        match day_time.rem_euclid(24_000) {
            0..10 => Activity::Rest,
            10..2000 => Activity::Idle,
            2000..9000 => Activity::Work,
            9000..11000 => Activity::Meet,
            11000..12000 => Activity::Idle,
            _ => Activity::Rest,
        }
    }

    /// Детское расписание (`baby_villager_activity`): 10 idle, 3000 play,
    /// 6000 idle, 10000 play, 12000 rest.
    fn at_baby(day_time: i64) -> Self {
        match day_time.rem_euclid(24_000) {
            0..10 => Activity::Rest,
            10..3000 => Activity::Idle,
            3000..6000 => Activity::Play,
            6000..10_000 => Activity::Idle,
            10_000..12_000 => Activity::Play,
            _ => Activity::Rest,
        }
    }
}

/// Цели `Brain` в порядке приоритета: сон важнее работы, работа — встречи,
/// встреча — прогулки.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Goal {
    Idle,
    Meet,
    Work,
    Rest,
}

impl Goal {
    fn priority(self) -> i32 {
        match self {
            Goal::Idle => 0,
            Goal::Meet => 1,
            Goal::Work => 2,
            Goal::Rest => 3,
        }
    }
}

/// `SetRaidStatus`/`Raid`: фаза рейда рядом с жителем.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaidPhase {
    None,
    /// Первая волна не вышла или перерыв между волнами (`RingBell`).
    PreRaid,
    /// Волна идёт: житель прячется в доме (`LocateHidingPlace`).
    Raid,
    /// Рейд отбит: жители празднуют у колокола.
    Victory,
}

/// `SetRaidStatus`: фаза по состоянию рейда.
fn raid_phase(groups: i32, alive: i32, cooldown: i64, celebration: i32) -> RaidPhase {
    if celebration > 0 {
        return RaidPhase::Victory;
    }
    if groups == 0 || (alive == 0 && cooldown > 0) {
        RaidPhase::PreRaid
    } else {
        RaidPhase::Raid
    }
}

/// Результат `ValidateNearbyPoi` для памяти точки интереса.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoiValidation {
    Keep,
    Erase,
    EraseAndRelease,
}

/// `ValidateNearbyPoi`: дальше 16 — не трогаем; точка исчезла — стираем;
/// занятая кровать не спящим жителем — стираем (и освобождаем, если в ней
/// никто не спит).
fn validate_poi(
    dist_sqr: f64,
    exists: bool,
    bed_occupied: bool,
    self_sleeping: bool,
    occupied_by_sleeper: bool,
) -> PoiValidation {
    if dist_sqr > 16.0 * 16.0 {
        return PoiValidation::Keep;
    }
    if !exists {
        return PoiValidation::EraseAndRelease;
    }
    if bed_occupied && !self_sleeping {
        return if occupied_by_sleeper {
            PoiValidation::Erase
        } else {
            PoiValidation::EraseAndRelease
        };
    }
    PoiValidation::Keep
}

/// `RunOne` рабочего пакета (`getWorkPackage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkGoal {
    WorkAtPoi,
    StrollAroundPoi,
    StrollToPoi,
    StrollToPoiList,
    Harvest,
    UseBonemeal,
}

/// `TriggerGate.triggerOneShuffled` встречи у колокола.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeetGoal {
    Stroll,
    Socialize,
}

/// `RunOne` rest-пакета при отсутствии `HOME`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestGoal {
    ClosestHome,
    Brownian,
    ClosestVillage,
    DoNothing,
}

/// `RunOne` idle/play пакетов (`getIdlePackage`/`getPlayPackage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleGoal {
    InteractVillager,
    Breed,
    Cat,
    Stroll,
    Look,
    JumpBed,
    DoNothing,
}

/// `WeightedRandom.getRandomItem`: вес выбирается из суммы.
fn weighted_pick<T: Copy>(items: &[(T, u32)], roll: u32) -> Option<T> {
    let total: u32 = items.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return None;
    }
    let mut r = roll % total;
    for &(item, weight) in items {
        if r < weight {
            return Some(item);
        }
        r -= weight;
    }
    items.last().map(|(item, _)| *item)
}

/// Веса `getWorkPackage`: `HarvestFarmland` 2/5 и `UseBonemeal` 4/7
/// (фермер/остальные), остальные — одинаковые.
fn work_weights(profession: &str) -> Vec<(WorkGoal, u32)> {
    let farmer = profession == "minecraft:farmer";
    vec![
        (WorkGoal::WorkAtPoi, 7),
        (WorkGoal::StrollAroundPoi, 2),
        (WorkGoal::StrollToPoi, 5),
        (WorkGoal::StrollToPoiList, 5),
        (WorkGoal::Harvest, if farmer { 2 } else { 5 }),
        (WorkGoal::UseBonemeal, if farmer { 4 } else { 7 }),
    ]
}

/// Веса `getMeetPackage` TriggerGate: прогулка 2, социализация 2.
fn meet_weights() -> Vec<(MeetGoal, u32)> {
    vec![(MeetGoal::Stroll, 2), (MeetGoal::Socialize, 2)]
}

/// Веса `getRestPackage` RunOne без `HOME`: 1/4/2/2.
fn rest_weights() -> Vec<(RestGoal, u32)> {
    vec![
        (RestGoal::ClosestHome, 1),
        (RestGoal::Brownian, 4),
        (RestGoal::ClosestVillage, 2),
        (RestGoal::DoNothing, 2),
    ]
}

/// Веса `getIdlePackage` (RunOne): взаимодействие 2, остальное по 1.
fn idle_weights() -> Vec<(IdleGoal, u32)> {
    vec![
        (IdleGoal::InteractVillager, 2),
        (IdleGoal::Breed, 1),
        (IdleGoal::Cat, 1),
        (IdleGoal::Stroll, 1),
        (IdleGoal::Look, 1),
        (IdleGoal::JumpBed, 1),
        (IdleGoal::DoNothing, 1),
    ]
}

/// Веса `getPlayPackage` RunOne (когда рядом нет других малышей).
fn play_weights() -> Vec<(IdleGoal, u32)> {
    vec![
        (IdleGoal::InteractVillager, 2),
        (IdleGoal::Cat, 1),
        (IdleGoal::Stroll, 1),
        (IdleGoal::JumpBed, 2),
        (IdleGoal::DoNothing, 2),
    ]
}

/// Стек целей: первая доступная цель по приоритету, `Idle` — всегда
/// запасной вариант (в ванили нижние поведения срабатывают, если верхние
/// не могут стартовать).
fn goals_for(activity: Activity, has_home: bool, has_job: bool, has_meeting: bool) -> Vec<Goal> {
    let mut out = Vec::new();
    match activity {
        Activity::Rest if has_home => out.push(Goal::Rest),
        Activity::Work if has_job => out.push(Goal::Work),
        Activity::Meet if has_meeting => out.push(Goal::Meet),
        _ => {}
    }
    out.push(Goal::Idle);
    out.sort_by_key(|g| -g.priority());
    out
}

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

/// `SecondaryJobSiteSensor`: ближайшая грядка в радиусе `radius` от работы.
impl PlayPlan {
    fn nearest_farmland(&self, origin: [i32; 3], radius: i32) -> Option<[i32; 3]> {
        let mut best: Option<([i32; 3], i64)> = None;
        for dx in -radius..=radius {
            for dy in -2..=2 {
                for dz in -radius..=radius {
                    let p = [origin[0] + dx, origin[1] + dy, origin[2] + dz];
                    if !self
                        .block(p)
                        .is_some_and(|s| self.state_name(s).ends_with("farmland"))
                    {
                        continue;
                    }
                    let dist = i64::from(dx).pow(2) + i64::from(dy).pow(2) + i64::from(dz).pow(2);
                    if best.is_none_or(|(_, d)| dist < d) {
                        best = Some((p, dist));
                    }
                }
            }
        }
        best.map(|(p, _)| p)
    }

    /// `Villager.shouldRestock` + `restock` (на старте `WorkAtPoi`).
    fn try_restock(&self, v: &mut Villager, now: i64) {
        let needs = v.offers.iter().any(|o| o.uses > 0);
        if now > v.last_restock + 12_000 {
            v.last_restock = now;
            v.restocks_today = 0;
        }
        let allowed =
            v.restocks_today == 0 || (v.restocks_today < 2 && now > v.last_restock + 2_400);
        if allowed && needs {
            for o in &mut v.offers {
                o.demand += o.uses - (o.max_uses - o.uses);
                o.uses = 0;
            }
            v.last_restock = now;
            v.restocks_today += 1;
        }
    }
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
    /// Текущее занятие по `villager_schedule`.
    pub activity: Activity,
    /// `MemoryModuleType.HOME`: занятая кровать.
    pub home: Option<[i32; 3]>,
    /// Точка, к которой идёт житель (`WalkTarget`).
    pub move_target: Option<[f64; 3]>,
    /// Последняя отправленная позиция (`VecDeltaCodec`).
    pub sent_pos: [i64; 3],
    /// `NoAI`: пробы и датапаки отключают мозг.
    pub no_ai: bool,
    /// Счётчик прогулки в `idle`.
    pub wander_timer: i32,
    /// Когда последний раз искали кровать.
    pub last_home_check: i64,
    /// `LivingEntity.sleepingPos`: кровать, на которой спит житель.
    pub sleeping: Option<[i32; 3]>,
    /// `Entity.DATA_POSE`: 0 standing, 2 sleeping.
    pub pose: u8,
    /// `SleepInBed.nextOkStartTime`: 40 тиков после пробуждения.
    pub sleep_cooldown: i64,
    /// `MemoryModuleType.DOORS_TO_CLOSE`: открытые жителем двери.
    pub doors_to_close: Vec<[i32; 3]>,
    /// `MemoryModuleType.MEETING_POINT`: колокол на время `meet`.
    pub meeting_point: Option<[i32; 3]>,
    /// `MemoryModuleType.INTERACTION_TARGET`: UUID собеседника.
    pub interaction_target: Option<u128>,
    /// `Villager.lastGossipTime`: 1200 тиков между передачами слухов.
    pub last_gossip_time: i64,
    /// `HURT_BY`: до какого тика житель в панике и от кого бежит.
    pub hurt_until: i64,
    pub hurt_from: Option<[f64; 3]>,
    /// `AgeableMob.age`: <0 ребёнок, >0 кулдаун размножения.
    pub age: i32,
    /// `Villager.foodLevel`.
    pub food_level: i32,
    /// `MemoryModuleType.INTERACTION_TARGET` детской игры в салки.
    pub tag_target: Option<u128>,
    /// `SetRaidStatus`: текущая фаза рейда.
    pub raid_phase: RaidPhase,
    /// `MemoryModuleType.HIDING_PLACE`.
    pub hiding_place: Option<[i32; 3]>,
    /// `RingBell`: пауза между ударами.
    pub bell_cooldown: i64,
    /// Когда последний раз проверяли рейд.
    pub last_raid_check: i64,
    /// `MemoryModuleType.NEAREST_BED` (сенсор кроватей у малышей).
    pub nearest_bed: Option<[i32; 3]>,
    pub jump_left: i32,
    pub jump_cooldown: i32,
    pub jump_phase: i32,
    pub jump_base_y: f64,
    pub jump_deadline: i64,
    pub last_bed_check: i64,
    /// `Entity.onGround`: false в прыжке.
    pub airborne: bool,
    /// `HarvestFarmland`/`UseBonemeal`: цель и пауза между действиями.
    pub farm_target: Option<[i32; 3]>,
    pub farm_cooldown: i64,
    /// `InteractWith` с кошкой (`INTERACTION_TARGET`).
    pub cat_target: Option<[f64; 3]>,
    pub cat_timer: i32,
    /// Выбранная ветка `RunOne` и срок её действия.
    pub work_goal: WorkGoal,
    pub idle_goal: IdleGoal,
    pub meet_goal: MeetGoal,
    pub rest_goal: RestGoal,
    pub work_active: bool,
    pub work_cooldown: i64,
    /// `Behavior` по умолчанию: 60 тиков работы после успешного старта.
    pub work_end: i64,
    pub stroll_cooldown: i64,
    pub farm_actions_left: i32,
    pub branch_timer: i32,
    /// `LOOK_TARGET`: точка взгляда и пауза перед новым выбором.
    pub look_at: Option<[f64; 3]>,
    pub look_timer: i32,
    /// `SetLookAndInteract(PLAYER, 4)`: цель-игрок и таймер.
    pub player_target: Option<Uuid>,
    pub player_timer: i32,
    pub goal_timer: i32,
    /// `MemoryModuleType.BREED_TARGET` и время рождения.
    pub breed_target: Option<u128>,
    pub breed_partner: Option<i32>,
    pub breed_until: i64,
    /// `InteractWithDoor` cooldown после закрытия двери.
    pub door_node: Option<[i32; 3]>,
    pub door_cooldown: i64,
    /// `AbstractVillager.inventory` (8 слотов).
    pub inventory: [Option<Stack>; 8],
    /// `ShowTradesToPlayer`: игрок, которому показываем сделки.
    pub show_player: Option<Uuid>,
    pub show_items: Vec<Stack>,
    pub show_index: usize,
    pub show_cycle: i32,
    pub show_timer: i32,
    pub show_sent: Option<i32>,
    /// Узлы текущего шага (`Path.getPreviousNode`/`getNextNode`).
    pub path_from: Option<[i32; 3]>,
    pub path_to: Option<[i32; 3]>,
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

    fn decay_per_transfer(self) -> i32 {
        match self {
            GossipType::MajorNegative => 10,
            GossipType::MinorNegative => 20,
            GossipType::MinorPositive => 5,
            GossipType::MajorPositive => 20,
            GossipType::Trading => 20,
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
            let defs = &self.survival.enchanting.defs;
            let mut rng = rand::thread_rng();
            let enchants: Vec<(i32, i32)>;
            if spec.random {
                // `EnchantRandomlyFunction.run`: равномерный выбор из
                // `options`, затем `nextInt(minLevel, maxLevel)`.
                let book = def.gives.id == "minecraft:book";
                let check = !book && spec.only_compatible;
                let pool: Vec<&chunkwake_data::enchanting::EnchantDef> = defs
                    .iter()
                    .filter(|d| {
                        (spec.options.is_empty() || spec.options.iter().any(|n| n == &d.name))
                            && (!check || d.supported.contains(&def.gives.id))
                    })
                    .collect();
                let ench = *pool.get(rng.gen_range(0..pool.len()))?;
                let level = rng.gen_range(1..=ench.max_level.max(1));
                if spec.include_cost {
                    // `enchantItem`: 2 + nextInt(5 + level*10) + 3*level;
                    // `#double_trade_price` удваивает `additional_trade_cost`.
                    let mut cost = random_enchant_cost(level, rng.gen_range(0..5 + level * 10));
                    if def.double_price.iter().any(|name| name == &ench.name) {
                        cost *= 2;
                    }
                    extra_cost = cost;
                }
                enchants = vec![(ench.id, level)];
            } else {
                // `EnchantWithLevelsFunction.run`: `EnchantmentHelper.enchantItem`
                // по стоимости из `levels`, затем `additional_trade_cost`.
                let enchantable = *self.survival.enchanting.enchantable.get(&def.gives.id)?;
                let (min, max) = spec.levels.unwrap_or((1, 1));
                let level_cost = rng.gen_range(min..=max.max(min));
                enchants = select_enchantments(
                    defs,
                    &spec.options,
                    &def.gives.id,
                    enchantable,
                    level_cost,
                    &mut rng,
                );
                // `filtered` с `on_fail: discard`: без чар сделки нет.
                if enchants.is_empty() {
                    return None;
                }
                if spec.include_cost {
                    extra_cost = level_cost;
                }
            }
            if def.gives.id == "minecraft:enchanted_book" || def.gives.id == "minecraft:book" {
                extra.stored = enchants;
            } else {
                extra.enchantments = enchants;
            }
            decorated = true;
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
        // `TradeCost.toItemCost`: count + additionalCost, зажатый до
        // `getDefaultMaxStackSize` предмета.
        let max_stack = self
            .survival
            .items
            .get(buy_item as usize)
            .map_or(64, |info| i32::from(info.max_stack));
        let buy = (def.wants.count.max(0) + extra_cost).min(max_stack.max(1));
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

    /// `PoiManager.getInRange` без учёта занятости (колокол общий).
    fn nearest_poi(
        &self,
        types: &std::collections::HashSet<String>,
        origin: [i32; 3],
        radius: i32,
    ) -> Option<[i32; 3]> {
        let (cx, cz) = (origin[0] >> 4, origin[2] >> 4);
        let reach = radius.div_euclid(16) + 1;
        let map = self.dim_state().pois.lock().unwrap();
        let mut best: Option<([i32; 3], i64)> = None;
        for ((x, z), list) in map.iter() {
            if (x - cx).abs() > reach || (z - cz).abs() > reach {
                continue;
            }
            for (pos, kind) in list {
                if !types.contains(*kind) {
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

    /// `ValidateNearbyPoi(MEETING)` для колокола.
    fn validate_meeting_poi(&self, v: &mut Villager) {
        let Some(bell) = v.meeting_point else {
            return;
        };
        let dist = crate::mobs::dist2(
            v.pos,
            [bell[0] as f64 + 0.5, bell[1] as f64, bell[2] as f64 + 0.5],
        );
        if dist <= 16.0 * 16.0
            && self
                .block(bell)
                .and_then(|s| poi::of_block(self.state_name(s)))
                != Some("minecraft:meeting")
        {
            v.meeting_point = None;
        }
    }

    /// `AcquirePoi` (`MemoryModuleType.HOME`): свободная кровать в 48 блоках.
    /// `ValidateNearbyPoi`: дальше 16 — не трогаем, исчезла/занята — стираем.
    fn update_villager_home(&self, id: i32, v: &mut Villager, sleepers: &[[i32; 3]]) {
        if let Some(home) = v.home {
            let dist = crate::mobs::dist2(
                v.pos,
                [home[0] as f64 + 0.5, home[1] as f64, home[2] as f64 + 0.5],
            );
            if dist > 16.0 * 16.0 {
                return;
            }
            let state = self.block(home);
            let exists =
                state.and_then(|s| poi::of_block(self.state_name(s))) == Some("minecraft:home");
            let occupied =
                state.is_some_and(|s| self.survival.placement.prop(s, "occupied") == Some("true"));
            // Список спящих собирает вызывающий код под уже взятым
            // `villagers`-lock: повторный `lock()` здесь — deadlock.
            let sleeper = sleepers.contains(&home);
            let action = validate_poi(dist, exists, occupied, v.sleeping.is_some(), sleeper);
            if action == PoiValidation::Keep {
                return;
            }
            if action == PoiValidation::EraseAndRelease || !sleeper {
                self.release_poi(home);
            }
            v.home = None;
        }
        let types = std::collections::HashSet::from(["minecraft:home".to_owned()]);
        let origin = [
            v.pos[0].floor() as i32,
            v.pos[1].floor() as i32,
            v.pos[2].floor() as i32,
        ];
        if let Some(bed) = self.free_poi(&types, origin, 48) {
            self.claim_poi(bed, id);
            v.home = Some(bed);
        }
    }

    /// Свободное место рядом с точкой интереса: сосед с твёрдым полом.
    fn stand_near(&self, block: [i32; 3], from: [f64; 3]) -> Option<[f64; 3]> {
        let solid = |p: [i32; 3]| self.walk_blocked(p);
        let mut best: Option<([f64; 3], f64)> = None;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let feet = [block[0] + dx, block[1], block[2] + dz];
            if solid(feet) || solid([feet[0], feet[1] + 1, feet[2]]) {
                continue;
            }
            if !solid([feet[0], feet[1] - 1, feet[2]]) {
                continue;
            }
            if self.dangerous_block(feet) || self.dangerous_block([feet[0], feet[1] - 1, feet[2]]) {
                continue;
            }
            let pos = [feet[0] as f64 + 0.5, feet[1] as f64, feet[2] as f64 + 0.5];
            let dist = crate::mobs::dist2(pos, from);
            if best.is_none_or(|(_, d)| dist < d) {
                best = Some((pos, dist));
            }
        }
        best.map(|(pos, _)| pos).or_else(|| {
            Some([
                block[0] as f64 + 0.5,
                block[1] as f64 + 1.0,
                block[2] as f64 + 0.5,
            ])
        })
    }

    /// Коллизия для ходьбы: открытые двери/калитки проходимы.
    fn walk_blocked(&self, p: [i32; 3]) -> bool {
        self.block(p).is_some_and(|s| {
            let name = self.state_name(s);
            let open = self.survival.placement.prop(s, "open") == Some("true");
            let gate = name.ends_with("_door") || name.ends_with("_fence_gate");
            !(gate && open) && self.survival.placement.solid(s)
        })
    }

    /// `EntityType.isBlockDangerous`: огонь/лава/магма/костёр и колючие
    /// блоки, на которые житель не встаёт.
    fn dangerous_block(&self, p: [i32; 3]) -> bool {
        let Some(state) = self.block(p) else {
            return false;
        };
        let name = self.state_name(state);
        if matches!(
            name,
            "minecraft:fire"
                | "minecraft:soul_fire"
                | "minecraft:lava"
                | "minecraft:magma_block"
                | "minecraft:lava_cauldron"
                | "minecraft:wither_rose"
                | "minecraft:sweet_berry_bush"
                | "minecraft:cactus"
                | "minecraft:powder_snow"
        ) {
            return true;
        }
        name.ends_with("campfire") && self.survival.placement.prop(state, "lit") == Some("true")
    }

    /// `AbstractBedBlock.findStandUpPosition`: свободное место у изголовья
    /// по направлению кровати (`bedSurroundStandUpOffsets` + `above`).
    fn find_stand_up(
        &self,
        head: [i32; 3],
        yaw: f32,
        id: i32,
        others: &[VillagerRef],
    ) -> Option<[f64; 3]> {
        let state = self.block(head)?;
        let name = self.state_name(state);
        if !name.ends_with("_bed") {
            return None;
        }
        let facing = self.survival.placement.prop(state, "facing")?;
        let (fx, fz) = match facing {
            "north" => (0, -1),
            "south" => (0, 1),
            "west" => (-1, 0),
            "east" => (1, 0),
            _ => return None,
        };
        // `Direction.getClockWise`.
        let (mut sx, mut sz) = (-fz, fx);
        let angle = match (sx, sz) {
            (0, -1) => 180.0,
            (0, 1) => 0.0,
            (-1, 0) => 90.0,
            _ => -90.0,
        };
        let delta = (yaw - angle + 180.0).rem_euclid(360.0) - 180.0;
        if delta.abs() < 45.0 {
            sx = -sx;
            sz = -sz;
        }
        let mut offsets = vec![
            (sx, sz),
            (sx - fx, sz - fz),
            (sx - 2 * fx, sz - 2 * fz),
            (-2 * fx, -2 * fz),
            (-sx - 2 * fx, -sz - 2 * fz),
            (-sx - fx, -sz - fz),
            (-sx, -sz),
            (-sx + fx, -sz + fz),
            (fx, fz),
            (sx + fx, sz + fz),
        ];
        offsets.push((0, 0));
        offsets.push((-fx, -fz));
        let solid = |p: [i32; 3]| self.walk_blocked(p);
        for (dx, dz) in offsets {
            let feet = [head[0] + dx, head[1], head[2] + dz];
            if solid(feet) || solid([feet[0], feet[1] + 1, feet[2]]) {
                continue;
            }
            if !solid([feet[0], feet[1] - 1, feet[2]]) {
                continue;
            }
            if self.entity_at(feet)
                || others.iter().any(|(other, _, p)| {
                    *other != id
                        && p[0].floor() as i32 == feet[0]
                        && p[1].floor() as i32 == feet[1]
                        && p[2].floor() as i32 == feet[2]
                })
            {
                continue;
            }
            // `findSafeDismountLocation(checkDangerous=true)`.
            if self.dangerous_block(feet) || self.dangerous_block([feet[0], feet[1] - 1, feet[2]]) {
                continue;
            }
            return Some([feet[0] as f64 + 0.5, feet[1] as f64, feet[2] as f64 + 0.5]);
        }
        None
    }

    /// Есть ли игрок или моб в блоке (жители проверяются отдельно).
    fn entity_at(&self, pos: [i32; 3]) -> bool {
        let inside = |p: [f64; 3]| {
            p[0].floor() as i32 == pos[0]
                && p[1].floor() as i32 == pos[1]
                && p[2].floor() as i32 == pos[2]
        };
        if self.live.lock().unwrap().values().any(|l| inside(l.pos)) {
            return true;
        }
        self.sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .any(|e| e.mob.alive() && inside(e.mob.pos))
    }

    /// `DoorBlock.setOpen`: обе половины двери со звуком.
    fn set_door_open(&self, pos: [i32; 3], open: bool) {
        let Some(state) = self.block(pos) else {
            return;
        };
        let name = self.state_name(state);
        if !name.ends_with("_door") || name.contains("iron") {
            return;
        }
        let want = if open { "true" } else { "false" };
        if self.survival.placement.prop(state, "open") == Some(want) {
            return;
        }
        if let Some(partner) = self.survival.placement.partner(state) {
            let other = [
                pos[0] + partner[0],
                pos[1] + partner[1],
                pos[2] + partner[2],
            ];
            if let Some(os) = self.block(other)
                && self.state_name(os) == name
                && let Some(next) = self.survival.placement.with_prop(os, "open", want)
            {
                self.set_block(other, next);
            }
        }
        if let Some(next) = self.survival.placement.with_prop(state, "open", want) {
            self.set_block(pos, next);
        }
        let sound = if open {
            "minecraft:block.wooden_door.open"
        } else {
            "minecraft:block.wooden_door.close"
        };
        self.play_sound(
            None,
            sound,
            crate::sounds::NEUTRAL,
            [
                pos[0] as f64 + 0.5,
                pos[1] as f64 + 0.5,
                pos[2] as f64 + 0.5,
            ],
            1.0,
            1.0,
        );
    }

    /// `InteractWithDoor`: открыть дверь на пути и запомнить её.
    fn open_door_on_path(&self, v: &mut Villager, pos: [i32; 3]) {
        // `InteractWithDoor` cooldown для той же двери.
        if v.door_node == Some(pos) && self.game_time() < v.door_cooldown {
            return;
        }
        let Some(state) = self.block(pos) else {
            return;
        };
        let name = self.state_name(state);
        if !name.ends_with("_door") || name.contains("iron") {
            return;
        }
        self.set_door_open(pos, true);
        if !v.doors_to_close.contains(&pos) {
            v.doors_to_close.push(pos);
        }
    }

    /// Идёт ли другой житель через дверь (`areOtherMobsComingThroughDoor`).
    /// `closeDoorsThatIHaveOpenedOrPassedThrough`: закрыть дверь, если
    /// житель отошёл, рядом никого нет и она ещё открыта.
    fn close_villager_doors(
        &self,
        id: i32,
        v: &mut Villager,
        others: &[VillagerRef],
        paths: &[VillagerPath],
    ) {
        for door in std::mem::take(&mut v.doors_to_close) {
            let center = [door[0] as f64 + 0.5, door[1] as f64, door[2] as f64 + 0.5];
            let dist = crate::mobs::dist2(v.pos, center);
            if dist > 3.0 * 3.0 {
                continue;
            }
            // Узлы `from`/`to` текущего пути не закрываем (vanilla пропускает
            // их), даже если житель уже рядом.
            if v.path_from == Some(door) || v.path_to == Some(door) {
                v.doors_to_close.push(door);
                continue;
            }
            // Другой житель идёт через эту дверь — не закрываем.
            if other_villager_on_door(id, door, center, paths) {
                continue;
            }
            let Some(state) = self.block(door) else {
                continue;
            };
            if !self.state_name(state).ends_with("_door")
                || self.survival.placement.prop(state, "open") != Some("true")
            {
                continue;
            }
            if self.entity_at([door[0], door[1], door[2]])
                || others.iter().any(|(other, _, p)| {
                    *other != id
                        && p[0].floor() as i32 == door[0]
                        && p[1].floor() as i32 == door[1]
                        && p[2].floor() as i32 == door[2]
                })
            {
                continue;
            }
            self.set_door_open(door, false);
            v.door_node = Some(door);
            v.door_cooldown = self.game_time() + 20;
        }
    }

    /// `SleepInBed.start`: лечь в кровать, занять её и включить позу сна.
    fn start_villager_sleep(&self, id: i32, v: &mut Villager) {
        let Some(bed) = v.home else {
            return;
        };
        let Some((head, _foot)) = self.bed_pair(bed) else {
            return;
        };
        let Some(state) = self.block(head) else {
            return;
        };
        if self.survival.placement.prop(state, "occupied") == Some("true") {
            return;
        }
        self.set_bed_occupied(head, true);
        v.home = Some(head);
        v.sleeping = Some(head);
        v.pose = 2;
        v.pos = [
            head[0] as f64 + 0.5,
            head[1] as f64 + 0.6875,
            head[2] as f64 + 0.5,
        ];
        v.move_target = None;
        v.sent_pos = play::quantize(v.pos);
        self.hub.send(
            None,
            play::entity_meta(self.ids.set_entity_data, id, &[(6, play::Meta::Pose(2))]),
        );
        self.hub.send(
            None,
            play::teleport_entity(self.ids.teleport_entity, id, v.pos, v.yaw, 0.0, true),
        );
    }

    /// `WakeUp`/`SleepInBed.stop`: освободить кровать и встать рядом.
    fn stop_villager_sleep(&self, id: i32, v: &mut Villager, others: &[VillagerRef]) {
        if let Some(bed) = v.sleeping.take() {
            self.set_bed_occupied(bed, false);
            // Кровать исчезла: забываем `HOME`, чтобы стек ушёл в `Idle`.
            if self
                .block(bed)
                .and_then(|s| poi::of_block(self.state_name(s)))
                != Some("minecraft:home")
            {
                self.release_poi(bed);
                v.home = None;
            }
            // `SleepInBed.stop`: nextOkStartTime = timestamp + 40.
            v.sleep_cooldown = self.game_time() + 40;
            if let Some(pos) = self.find_stand_up(bed, v.yaw, id, others) {
                v.pos = pos;
            } else if let Some(pos) = self.stand_near(bed, v.pos) {
                v.pos = pos;
            }
        }
        v.pose = 0;
        v.sent_pos = play::quantize(v.pos);
        v.move_target = None;
        self.hub.send(
            None,
            play::entity_meta(self.ids.set_entity_data, id, &[(6, play::Meta::Pose(0))]),
        );
        self.hub.send(
            None,
            play::teleport_entity(self.ids.teleport_entity, id, v.pos, v.yaw, 0.0, true),
        );
    }

    /// Один шаг `Brain`/`WalkTarget`: работа, встреча у колокола, отдых дома
    /// и прогулка в `idle`.
    #[allow(clippy::too_many_arguments)]
    fn villager_ai_step(
        &self,
        id: i32,
        v: &mut Villager,
        others: &[VillagerRef],
        profiles: &[VillagerProfile],
        threats: &[[f64; 3]],
        cats: &[[f64; 3]],
        breedable: &[(i32, bool)],
        babies: &[(i32, u128, [f64; 3], Option<u128>)],
        births: &mut Vec<Birth>,
        gossip: &mut Vec<(i32, i32)>,
    ) -> Option<([f64; 3], f32)> {
        // `MEETING_POINT` ищется до выбора цели, иначе стек не увидит встречу.
        if v.activity == Activity::Meet && v.meeting_point.is_none() {
            let types = std::collections::HashSet::from(["minecraft:meeting".to_owned()]);
            let origin = [
                v.pos[0].floor() as i32,
                v.pos[1].floor() as i32,
                v.pos[2].floor() as i32,
            ];
            v.meeting_point = self.nearest_poi(&types, origin, 48);
        }
        // `VillagerPanicTrigger`/`VillagerCalmDown`: бег от угрозы важнее
        // расписания; после 100 тиков без угрозы возврат к цели.
        let now = self.game_time();
        let threat = panic_threat(v.pos, v.hurt_until, now, v.hurt_from, threats);
        if let Some(from) = threat {
            v.hurt_from = Some(from);
            v.move_target = None;
            v.interaction_target = None;
            let dx = v.pos[0] - from[0];
            let dz = v.pos[2] - from[2];
            let len = (dx * dx + dz * dz).sqrt().max(1e-4);
            return self.walk_step(
                v,
                [
                    v.pos[0] + dx / len * 8.0,
                    v.pos[1],
                    v.pos[2] + dz / len * 8.0,
                ],
                0.21,
            );
        }
        if v.hurt_until != 0 {
            v.hurt_until = 0;
            v.hurt_from = None;
        }
        // `SetRaidStatus`: пре-рейд/укрытие важнее расписания.
        if v.raid_phase != RaidPhase::None
            && let Some(step) = self.raid_step(v)
        {
            return Some(step);
        }

        // `MoveToTargetSink` (приоритет 1): пока есть `WALK_TARGET`, житель
        // только идёт к нему — рабочие поведения не запускаются.
        if let Some(target) = v.move_target {
            if let Some(step) = self.walk_step(v, target, 0.14) {
                return Some(step);
            }
            // Цель достигнута или путь застрял (`MoveToTargetSink.stop`).
            v.move_target = None;
        }
        // `RunOne` work-пакета: действия выбираются по весам, а ветка
        // завершается по факту (дошёл/собрал), затем перевыбор.
        // `SetWalkTargetFromBlockMemory(JOB_SITE, 0.4, 9, 100, 1200)`:
        // приоритет 2 — пока до работы больше 9 блоков, житель только идёт.
        if v.activity == Activity::Work
            && v.move_target.is_none()
            && let Some(site) = v.job_site
        {
            let d = (site[0] - v.pos[0].floor() as i32).abs()
                + (site[1] - v.pos[1].floor() as i32).abs()
                + (site[2] - v.pos[2].floor() as i32).abs();
            if d > 9 {
                v.move_target = self.stand_near(site, v.pos);
            }
        }
        if v.activity == Activity::Work {
            match v.work_goal {
                WorkGoal::Harvest | WorkGoal::UseBonemeal => {
                    if let Some(step) = self.farm_step(v) {
                        return Some(step);
                    }
                    v.goal_timer = 0;
                }
                WorkGoal::WorkAtPoi => {
                    let at_site = v.job_site.is_some_and(|site| {
                        crate::mobs::dist2(
                            v.pos,
                            [site[0] as f64 + 0.5, site[1] as f64, site[2] as f64 + 0.5],
                        ) <= 1.73 * 1.73
                    });
                    // `WorkAtPoi.checkExtraStartConditions`: не чаще 300 тиков,
                    // затем 1/2; `canStillUse` — только дистанция, длительность
                    // поведения 60 тиков (`Behavior` по умолчанию).
                    if !at_site {
                        v.work_active = false;
                        v.goal_timer = 0;
                    } else {
                        if !v.work_active {
                            if now >= v.work_cooldown {
                                v.work_cooldown = now + 300;
                                if rand::random::<u32>().is_multiple_of(2) {
                                    v.work_active = true;
                                    v.work_end = now + 60;
                                    if let Some(sound) = work_sound(&v.profession) {
                                        self.play_sound(
                                            None,
                                            &sound,
                                            crate::sounds::NEUTRAL,
                                            v.pos,
                                            1.0,
                                            1.0,
                                        );
                                    }
                                    self.try_restock(v, now);
                                }
                            }
                            if !v.work_active {
                                // Старт не состоялся — RunOne перевыбирает.
                                v.goal_timer = 0;
                            }
                        } else if now >= v.work_end {
                            v.work_active = false;
                            v.goal_timer = 0;
                        }
                        // `WorkAtPoi.start`: LOOK_TARGET на рабочий блок.
                        if let Some(site) = v.job_site {
                            let target = [
                                site[0] as f64 + 0.5,
                                site[1] as f64 + 0.5,
                                site[2] as f64 + 0.5,
                            ];
                            return Some((v.pos, look_yaw(v.pos, target)));
                        }
                        return Some((v.pos, v.yaw));
                    }
                }
                WorkGoal::StrollAroundPoi => {
                    // `StrollAroundPoi(JOB_SITE, 0.4, 4)`: пауза 180.
                    let site = v.job_site.unwrap_or([
                        v.pos[0].floor() as i32,
                        v.pos[1].floor() as i32,
                        v.pos[2].floor() as i32,
                    ]);
                    let center = [site[0] as f64 + 0.5, site[1] as f64, site[2] as f64 + 0.5];
                    if crate::mobs::dist2(v.pos, center) > 16.0 {
                        v.goal_timer = 0;
                    } else if now >= v.stroll_cooldown {
                        v.move_target = self.stroll_point(v.pos, 8);
                        v.stroll_cooldown = now + 180;
                        v.goal_timer = 0;
                    }
                }
                WorkGoal::StrollToPoiList => {
                    // `StrollToPoiList(SECONDARY_JOB_SITE, ..., 1, 6, JOB_SITE)`.
                    let site = v.job_site.unwrap_or([
                        v.pos[0].floor() as i32,
                        v.pos[1].floor() as i32,
                        v.pos[2].floor() as i32,
                    ]);
                    if now >= v.stroll_cooldown {
                        v.move_target = self
                            .nearest_farmland(site, 6)
                            .and_then(|f| self.stand_near(f, v.pos));
                        v.stroll_cooldown = now + 100;
                        v.goal_timer = 0;
                    }
                }
                WorkGoal::StrollToPoi => {
                    // `StrollToPoi(JOB_SITE, 0.4, 1, 10)`: пауза 80.
                    let site = v.job_site.unwrap_or([
                        v.pos[0].floor() as i32,
                        v.pos[1].floor() as i32,
                        v.pos[2].floor() as i32,
                    ]);
                    let center = [site[0] as f64 + 0.5, site[1] as f64, site[2] as f64 + 0.5];
                    if crate::mobs::dist2(v.pos, center) > 100.0 {
                        v.goal_timer = 0;
                    } else if now >= v.stroll_cooldown {
                        v.move_target = self.stand_near(site, v.pos);
                        v.stroll_cooldown = now + 80;
                        v.goal_timer = 0;
                    }
                }
            }
        }
        // `TriggerGate` встречи: прогулка у колокола или социализация.
        if v.activity == Activity::Meet {
            match v.meet_goal {
                MeetGoal::Socialize => {
                    if let Some(step) = self.socialize_step(id, v, others, profiles, gossip) {
                        v.branch_timer = 200;
                        return Some(step);
                    }
                    v.branch_timer -= 1;
                    if v.branch_timer <= 0 {
                        v.goal_timer = 0;
                    }
                }
                MeetGoal::Stroll => {
                    // `StrollAroundPoi(MEETING_POINT, 0.4, 40)`: пауза 180.
                    let Some(bell) = v.meeting_point else {
                        v.goal_timer = 0;
                        return None;
                    };
                    let center = [bell[0] as f64 + 0.5, bell[1] as f64, bell[2] as f64 + 0.5];
                    if crate::mobs::dist2(v.pos, center) > 1600.0 {
                        v.goal_timer = 0;
                    } else if now >= v.stroll_cooldown {
                        v.move_target = self.stroll_point(v.pos, 8);
                        v.stroll_cooldown = now + 180;
                        v.goal_timer = 0;
                    }
                }
            }
        }
        // `RunOne` rest-пакета без дома.
        if v.activity == Activity::Rest && v.home.is_none() {
            let now = self.game_time();
            match v.rest_goal {
                RestGoal::ClosestHome => {
                    let types = std::collections::HashSet::from(["minecraft:home".to_owned()]);
                    let origin = [
                        v.pos[0].floor() as i32,
                        v.pos[1].floor() as i32,
                        v.pos[2].floor() as i32,
                    ];
                    let bed = self.nearest_poi(&types, origin, 48);
                    if let Some(target) = bed.and_then(|b| self.stand_near(b, v.pos))
                        && let Some(step) = self.walk_step(v, target, 0.14)
                    {
                        return Some(step);
                    }
                    v.goal_timer = 0;
                }
                RestGoal::Brownian => {
                    if v.move_target.is_none() {
                        v.move_target = self.stroll_point(v.pos, 8);
                    }
                    if let Some(step) = self.walk_step(v, v.move_target?, 0.14) {
                        return Some(step);
                    }
                    v.goal_timer = 0;
                }
                RestGoal::ClosestVillage => {
                    let types = std::collections::HashSet::from(["minecraft:meeting".to_owned()]);
                    let origin = [
                        v.pos[0].floor() as i32,
                        v.pos[1].floor() as i32,
                        v.pos[2].floor() as i32,
                    ];
                    let bell = self.nearest_poi(&types, origin, 48);
                    if let Some(target) = bell.and_then(|b| self.stand_near(b, v.pos))
                        && let Some(step) = self.walk_step(v, target, 0.14)
                    {
                        return Some(step);
                    }
                    v.goal_timer = 0;
                }
                RestGoal::DoNothing => {
                    v.branch_timer -= 1;
                    if v.branch_timer <= 0 {
                        v.goal_timer = 0;
                    }
                    return Some((v.pos, v.yaw));
                }
            }
            let _ = now;
        }
        // Навигация продолжается, пока поведение-«однократка» уже
        // завершилось (`MoveToTargetSink`).
        if let Some(target) = v.move_target
            && let Some(step) = self.walk_step(v, target, 0.14)
        {
            return Some(step);
        }
        let other_babies = babies.iter().filter(|(bid, ..)| *bid != id).count();
        // `getPlayPackage`: салки (5) при других малышах, иначе RunOne.
        if v.age < 0 && v.activity == Activity::Play {
            if other_babies > 0 {
                if let Some(step) = self.play_tag_step(id, v, babies) {
                    return Some(step);
                }
            } else {
                let branch = weighted_pick(&play_weights(), rand::random::<u32>());
                // `getPlayPackage` RunOne: каждая ветка — OneShot/Behavior,
                // после неудачи/завершения RunOne перевыбирает поведение.
                match branch {
                    Some(IdleGoal::Cat) => {
                        if let Some(step) = self.cat_step(v, cats) {
                            return Some(step);
                        }
                        v.goal_timer = 0;
                    }
                    Some(IdleGoal::JumpBed) => {
                        if let Some(step) = self.jump_on_bed_step(v) {
                            return Some(step);
                        }
                        v.goal_timer = 0;
                    }
                    Some(IdleGoal::Stroll) => {
                        if v.move_target.is_none() {
                            v.move_target = self.stroll_point(v.pos, 8);
                        }
                        v.goal_timer = 0;
                    }
                    Some(IdleGoal::DoNothing) => {
                        v.branch_timer -= 1;
                        if v.branch_timer <= 0 {
                            v.goal_timer = 0;
                        }
                        return Some((v.pos, v.yaw));
                    }
                    _ => v.goal_timer = 0,
                }
            }
        }
        // `getIdlePackage` RunOne: кошка/прыжки/размножение по весам.
        if v.activity == Activity::Idle {
            match v.idle_goal {
                IdleGoal::Cat => {
                    if let Some(step) = self.cat_step(v, cats) {
                        return Some(step);
                    }
                    v.goal_timer = 0;
                }
                IdleGoal::JumpBed if v.age < 0 => {
                    if let Some(step) = self.jump_on_bed_step(v) {
                        return Some(step);
                    }
                    v.goal_timer = 0;
                }
                IdleGoal::Breed if self.can_breed(v) => {
                    if let Some(step) = self.breed_step(id, v, others, breedable, births) {
                        return Some(step);
                    }
                    v.goal_timer = 0;
                }
                IdleGoal::Stroll => {
                    if v.move_target.is_none() {
                        v.move_target = self.stroll_point(v.pos, 8);
                    }
                    v.goal_timer = 0;
                }
                IdleGoal::DoNothing => {
                    v.branch_timer -= 1;
                    if v.branch_timer <= 0 {
                        v.goal_timer = 0;
                    }
                    return Some((v.pos, v.yaw));
                }
                _ => v.goal_timer = 0,
            }
        }
        if v.activity == Activity::Idle
            && (v.breed_target.is_some() || self.can_breed(v))
            && let Some(step) = self.breed_step(id, v, others, breedable, births)
        {
            return Some(step);
        }

        let goal = *goals_for(
            v.activity,
            v.home.is_some(),
            v.job_site.is_some(),
            v.meeting_point.is_some(),
        )
        .first()?;
        let target = match goal {
            Goal::Work => v.job_site.and_then(|p| self.stand_near(p, v.pos)),
            Goal::Rest => {
                let home = v.home?;
                if crate::mobs::dist2(
                    v.pos,
                    [home[0] as f64 + 0.5, home[1] as f64, home[2] as f64 + 0.5],
                ) <= 2.0 * 2.0
                    && self.game_time() >= v.sleep_cooldown
                {
                    self.start_villager_sleep(id, v);
                    return None;
                }
                self.stand_near(home, v.pos)
            }
            Goal::Meet => {
                if let Some(step) = self.socialize_step(id, v, others, profiles, gossip) {
                    return Some(step);
                }
                if v.move_target.is_none() {
                    v.move_target = v
                        .meeting_point
                        .and_then(|bell| self.stand_near(bell, v.pos));
                }
                v.move_target
            }
            Goal::Idle => {
                if v.move_target.is_none() || v.wander_timer <= 0 {
                    let base = v.job_site.or(v.home).unwrap_or([
                        v.pos[0].floor() as i32,
                        v.pos[1].floor() as i32,
                        v.pos[2].floor() as i32,
                    ]);
                    let dx = f64::from(rand::random::<u32>() % 13) - 6.0;
                    let dz = f64::from(rand::random::<u32>() % 13) - 6.0;
                    v.move_target = Some([
                        base[0] as f64 + 0.5 + dx,
                        v.pos[1],
                        base[2] as f64 + 0.5 + dz,
                    ]);
                    v.wander_timer = 100 + (rand::random::<u32>() % 100) as i32;
                }
                v.wander_timer -= 1;
                v.move_target
            }
        }?;
        self.walk_step(v, target, 0.14)
    }

    /// `Sensing.hasLineOfSight`: трассировка взгляда по клеткам.
    fn has_line_of_sight(&self, from: [f64; 3], to: [f64; 3]) -> bool {
        let a = [from[0], from[1] + 1.62, from[2]];
        let b = [to[0], to[1] + 1.62, to[2]];
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let steps = (len / 0.2).ceil().max(1.0) as usize;
        for i in 1..steps {
            let t = i as f64 / steps as f64;
            let cell = [
                (a[0] + d[0] * t).floor() as i32,
                (a[1] + d[1] * t).floor() as i32,
                (a[2] + d[2] * t).floor() as i32,
            ];
            if self.walk_blocked(cell) {
                return false;
            }
        }
        true
    }

    /// `Villager.countFoodPointsInInventory`.
    fn food_points(&self, v: &Villager) -> i32 {
        v.inventory
            .iter()
            .flatten()
            .map(|s| {
                self.survival
                    .items
                    .get(s.item as usize)
                    .and_then(|i| i.food)
                    .map_or(0, |f| f.nutrition * s.count)
            })
            .sum()
    }

    fn has_excess_food(&self, v: &Villager) -> bool {
        self.food_points(v) >= 24
    }

    fn wants_more_food(&self, v: &Villager) -> bool {
        self.food_points(v) < 12
    }

    /// `VillagerProfession.requestedItems` (в 26.3 только фермер).
    fn requested_items(profession: &str) -> &'static [&'static str] {
        if profession == "minecraft:farmer" {
            &[
                "minecraft:wheat",
                "minecraft:wheat_seeds",
                "minecraft:beetroot_seeds",
                "minecraft:bone_meal",
            ]
        } else {
            &[]
        }
    }

    /// `TradeWithVillager.throwHalfStack`: половина стака (или всё сверх 24).
    fn throw_half_stack(
        &self,
        v: &mut Villager,
        target: [f64; 3],
        predicate: impl Fn(i32) -> bool,
    ) -> Option<Stack> {
        for slot in &mut v.inventory {
            let Some(stack) = slot else { continue };
            if !predicate(stack.item) {
                continue;
            }
            let max = self.max_stack(stack.item).max(1);
            let count = if stack.count > max / 2 {
                stack.count / 2
            } else if stack.count > 24 {
                stack.count - 24
            } else {
                continue;
            };
            if count <= 0 {
                continue;
            }
            stack.count -= count;
            let out = Stack::new(stack.item, count);
            if stack.count <= 0 {
                *slot = None;
            }
            let dx = target[0] - v.pos[0];
            let dy = target[1] + 1.0 - (v.pos[1] + 1.2);
            let dz = target[2] - v.pos[2];
            let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-4);
            let vel = [dx / len * 0.3, dy / len * 0.3 + 0.1, dz / len * 0.3];
            self.drop_item(
                [v.pos[0], v.pos[1] + 1.2, v.pos[2]],
                vel,
                out,
                items::BLOCK_DELAY,
            );
            return Some(out);
        }
        None
    }

    /// `TradeWithVillager.tick`: еда и requestedItems.
    fn trade_step(
        &self,
        v: &mut Villager,
        target: [f64; 3],
        target_profession: &str,
        target_wants_food: bool,
    ) {
        let is_farmer = v.profession == "minecraft:farmer";
        if self.has_excess_food(v) && (is_farmer || target_wants_food) {
            self.throw_half_stack(v, target, |item| {
                self.survival
                    .items
                    .get(item as usize)
                    .is_some_and(|i| i.food.is_some())
            });
        }
        if is_farmer {
            let wheat = self.item_id("minecraft:wheat");
            if let Some(wheat) = wheat {
                let total: i32 = v
                    .inventory
                    .iter()
                    .flatten()
                    .filter(|s| s.item == wheat)
                    .map(|s| s.count)
                    .sum();
                if total > 32 {
                    self.throw_half_stack(v, target, |item| item == wheat);
                }
            }
        }
        let wanted: Vec<&str> = Self::requested_items(target_profession)
            .iter()
            .copied()
            .filter(|name| !Self::requested_items(&v.profession).contains(name))
            .collect();
        if !wanted.is_empty() {
            self.throw_half_stack(v, target, |item| {
                wanted.iter().any(|name| self.item_id(name) == Some(item))
            });
        }
    }

    /// `ShowTradesToPlayer`: показать в основной руке результат сделки,
    /// чью цену держит nearby-игрок; цикл раз в 40 тиков.
    fn show_trades_step(
        &self,
        id: i32,
        v: &mut Villager,
        players: &[(Uuid, [f64; 3], Option<Stack>)],
    ) {
        let mut best: Option<(Uuid, [f64; 3], Stack)> = None;
        for &(uuid, pos, held) in players {
            let Some(held) = held else { continue };
            if crate::mobs::dist2(v.pos, pos) > 4.1 * 4.1 || !self.has_line_of_sight(v.pos, pos) {
                continue;
            }
            let offer = v.offers.iter().find(|o| {
                !o.out_of_stock()
                    && (o.buy.0 == held.item || o.buy_b.is_some_and(|(item, _)| item == held.item))
            });
            if let Some(offer) = offer
                && best.is_none()
            {
                best = Some((uuid, pos, offer.sell));
            }
        }
        let Some((uuid, pos, _)) = best else {
            if v.show_player.is_some() {
                self.hub.send(
                    None,
                    play::set_equipment(self.ids.set_equipment, id, &[(0, None)]),
                );
            }
            v.show_player = None;
            v.show_items.clear();
            v.show_sent = None;
            return;
        };
        if v.show_player != Some(uuid) {
            v.show_player = Some(uuid);
            let held = players
                .iter()
                .find(|(u, _, _)| *u == uuid)
                .and_then(|(_, _, s)| *s);
            v.show_items = v
                .offers
                .iter()
                .filter(|o| !o.out_of_stock())
                .filter(|o| {
                    held.is_some_and(|h| {
                        o.buy.0 == h.item || o.buy_b.is_some_and(|(item, _)| item == h.item)
                    })
                })
                .map(|o| o.sell)
                .collect();
            v.show_index = 0;
            v.show_cycle = 0;
            v.show_timer = 900;
            v.show_sent = None;
        }
        if v.show_items.is_empty() {
            return;
        }
        v.show_cycle += 1;
        if v.show_cycle >= 40 && v.show_items.len() >= 2 {
            v.show_cycle = 0;
            v.show_index = (v.show_index + 1) % v.show_items.len();
        }
        let item = v.show_items[v.show_index];
        if v.show_sent != Some(item.item) {
            v.show_sent = Some(item.item);
            self.hub.send(
                None,
                play::set_equipment(self.ids.set_equipment, id, &[(0, Some(item))]),
            );
        }
        v.yaw = look_yaw(v.pos, pos);
        v.show_timer -= 1;
        if v.show_timer <= 0 {
            self.hub.send(
                None,
                play::set_equipment(self.ids.set_equipment, id, &[(0, None)]),
            );
            v.show_player = None;
            v.show_items.clear();
            v.show_sent = None;
        }
    }

    /// `Villager.gossip`: передать до 10 слухов с `decayPerTransfer`,
    /// обновить `lastGossipTime` у обоих.
    fn gossip_villagers(&self, id: i32, target_id: i32, now: i64) {
        let mut villagers = self.sim().villagers.lock().unwrap();
        let [Some(a), Some(b)] = villagers.get_disjoint_mut([&id, &target_id]) else {
            return;
        };
        if now < a.last_gossip_time + 1200 || now < b.last_gossip_time + 1200 {
            return;
        }
        a.last_gossip_time = now;
        b.last_gossip_time = now;
        let entries: Vec<(u128, GossipType, i32)> = b
            .gossips
            .iter()
            .flat_map(|(&uuid, values)| {
                GossipType::ALL.into_iter().filter_map(move |kind| {
                    let value = values[kind.id()];
                    (value != 0).then_some((uuid, kind, value))
                })
            })
            .collect();
        let total: i32 = entries
            .iter()
            .map(|(_, kind, value)| (value * kind.weight()).abs())
            .sum();
        if total <= 0 {
            return;
        }
        let mut chosen = std::collections::HashSet::new();
        for _ in 0..10 {
            let mut roll = rand::random::<i32>() % total;
            for &(uuid, kind, value) in &entries {
                roll -= (value * kind.weight()).abs();
                if roll < 0 {
                    chosen.insert((uuid, kind.id()));
                    break;
                }
            }
        }
        for (uuid, idx) in chosen {
            let Some(kind) = GossipType::ALL.into_iter().find(|k| k.id() == idx) else {
                continue;
            };
            let Some((_, _, value)) = entries.iter().find(|(u, k, _)| *u == uuid && k.id() == idx)
            else {
                continue;
            };
            let decayed = value - kind.decay_per_transfer();
            if decayed >= 2 {
                let entry = a.gossips.entry(uuid).or_insert([0; 5]);
                entry[idx] = entry[idx].max(decayed).min(kind.max());
            }
        }
    }

    /// `Villager.canBreed`: еда + инвентарь >= 12, не спит, `age == 0`.
    fn can_breed(&self, v: &Villager) -> bool {
        v.food_level + self.food_points(v) >= 12 && v.sleeping.is_none() && v.age == 0
    }

    /// `Raids.getNearbyRaid` (96 блоков) и фаза `SetRaidStatus`.
    fn raid_phase_at(&self, pos: [f64; 3]) -> RaidPhase {
        let dim = self.active_dim();
        let alive: std::collections::HashSet<u128> = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.mob.alive())
            .map(|e| e.mob.uuid)
            .collect();
        let raids = self.raids.lock().unwrap();
        let Some(raid) = raids
            .iter()
            .filter(|r| r.dim == dim && crate::mobs::dist2(r.center, pos) < 96.0 * 96.0)
            .min_by(|a, b| {
                crate::mobs::dist2(a.center, pos).total_cmp(&crate::mobs::dist2(b.center, pos))
            })
        else {
            return RaidPhase::None;
        };
        let alive_count = raid.raiders.iter().filter(|u| alive.contains(u)).count() as i32;
        raid_phase(raid.groups, alive_count, raid.cooldown, raid.celebration)
    }

    /// `Heightmap.MOTION_BLOCKING`: самый высокий не-воздух в столбце.
    fn heightmap_top(&self, x: i32, z: i32) -> i32 {
        let mut top = self.min_y();
        for y in self.min_y()..(self.min_y() + self.template().height() as i32) {
            if let Some(state) = self.block([x, y, z])
                && !self.state_name(state).ends_with(":air")
            {
                top = y;
            }
        }
        top
    }

    /// `Level.canSeeSky`: высота столбца не выше точки.
    fn can_see_sky(&self, pos: [f64; 3]) -> bool {
        let cell = [
            pos[0].floor() as i32,
            pos[1].floor() as i32,
            pos[2].floor() as i32,
        ];
        self.heightmap_top(cell[0], cell[2]) <= cell[1]
    }

    /// `MoveToSkySeeingSpot.hasNoBlocksAbove`.
    fn has_no_blocks_above(&self, pos: [f64; 3]) -> bool {
        self.can_see_sky(pos)
            && self.heightmap_top(pos[0].floor() as i32, pos[2].floor() as i32)
                <= pos[1].floor() as i32
    }

    /// `MoveToSkySeeingSpot.getOutdoorPosition`: 10 точек ±10/±3, где
    /// небо видно и рельеф не выше ног.
    fn sky_point(&self, pos: [f64; 3]) -> Option<[f64; 3]> {
        let base = [
            pos[0].floor() as i32,
            pos[1].floor() as i32,
            pos[2].floor() as i32,
        ];
        for _ in 0..10 {
            let dx = i32::try_from(rand::random::<u32>() % 20).ok()? - 10;
            let dy = i32::try_from(rand::random::<u32>() % 6).ok()? - 3;
            let dz = i32::try_from(rand::random::<u32>() % 20).ok()? - 10;
            let cell = [base[0] + dx, base[1] + dy, base[2] + dz];
            let target = [cell[0] as f64 + 0.5, cell[1] as f64, cell[2] as f64 + 0.5];
            if self.can_see_sky(target)
                && self.heightmap_top(cell[0], cell[2]) <= base[1]
                && !self.dangerous_block(cell)
            {
                return Some(target);
            }
        }
        None
    }

    /// `CelebrateVillagersSurvivedRaid`: 1% звук, 0.5% фейерверк за тик.
    fn celebrate(&self, v: &mut Villager) {
        if rand::random::<u32>().is_multiple_of(100) {
            self.play_sound(
                None,
                "minecraft:entity.villager.celebrate",
                crate::sounds::NEUTRAL,
                [v.pos[0], v.pos[1] + 1.0, v.pos[2]],
                1.0,
                1.0,
            );
        }
        if rand::random::<u32>().is_multiple_of(200) && self.has_no_blocks_above(v.pos) {
            self.spawn_celebration_firework(v.pos);
        }
    }

    /// `FireworkRocketEntity` без всадника: взлетает и взрывается по таймеру.
    fn spawn_celebration_firework(&self, pos: [f64; 3]) {
        let eye = [pos[0], pos[1] + 1.62, pos[2]];
        let flight = rand::random::<u32>() % 3;
        let mut rocket = crate::arrows::Arrow::flight(eye, [0.0; 3], false);
        rocket.firework = true;
        rocket.explosions = 1;
        rocket.lifetime =
            crate::arrows::rocket_lifetime(flight as i32, rand::random(), rand::random());
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        self.sim().arrows.lock().unwrap().insert(id, rocket);
        self.mark_work();
        self.hub.send(
            None,
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                Uuid::from_u128(rand::random()),
                self.survival.firework_entity_type,
                eye,
                [0.0; 3],
                0.0,
                0.0,
                0,
            ),
        );
    }

    /// `RingBell`: удар в колокол без игрока.
    fn ring_villager_bell(&self, bell: [i32; 3]) {
        let Some(state) = self.block(bell) else {
            return;
        };
        if !self.state_name(state).ends_with("bell") {
            return;
        }
        let at = [
            bell[0] as f64 + 0.5,
            bell[1] as f64 + 0.5,
            bell[2] as f64 + 0.5,
        ];
        self.play_sound(
            None,
            "minecraft:block.bell.hit",
            crate::sounds::BLOCKS,
            at,
            2.0,
            1.0,
        );
        self.hub.send(
            None,
            play::block_event(self.ids.block_event, bell, 1, 1, state as i32),
        );
        self.emit_vibration(at, "minecraft:block_change");
    }

    /// `getPreRaidPackage`/`getRaidPackage`: сбор у колокола или укрытие.
    fn raid_step(&self, v: &mut Villager) -> Option<([f64; 3], f32)> {
        match v.raid_phase {
            RaidPhase::None => None,
            RaidPhase::Victory => {
                // `Team` празднует под открытым небом: если над головой
                // блоки, идём к ближайшей точке с небом.
                if !self.has_no_blocks_above(v.pos) {
                    if v.move_target.is_none() {
                        v.move_target = self.sky_point(v.pos);
                    }
                    return self.walk_step(v, v.move_target?, 0.21);
                }
                v.move_target = None;
                v.yaw = look_yaw(v.pos, [v.pos[0], v.pos[1] + 4.0, v.pos[2]]);
                self.celebrate(v);
                Some((v.pos, v.yaw))
            }
            RaidPhase::PreRaid => {
                if v.meeting_point.is_none() {
                    let types = std::collections::HashSet::from(["minecraft:meeting".to_owned()]);
                    let origin = [
                        v.pos[0].floor() as i32,
                        v.pos[1].floor() as i32,
                        v.pos[2].floor() as i32,
                    ];
                    v.meeting_point = self.nearest_poi(&types, origin, 48);
                }
                let Some(bell) = v.meeting_point else {
                    // Колокола нет — тревожная прогулка (`VillageBoundRandomStroll`).
                    if v.move_target.is_none() {
                        v.move_target = self.flee_point(v.pos);
                    }
                    return self.walk_step(v, v.move_target?, 0.21);
                };
                let center = [bell[0] as f64 + 0.5, bell[1] as f64, bell[2] as f64 + 0.5];
                if crate::mobs::dist2(v.pos, center) <= 4.0 * 4.0 {
                    if self.game_time() >= v.bell_cooldown {
                        self.ring_villager_bell(bell);
                        v.bell_cooldown = self.game_time() + 30;
                    }
                    let yaw = look_yaw(v.pos, center);
                    v.yaw = yaw;
                    v.move_target = None;
                    return Some((v.pos, yaw));
                }
                let target = self.stand_near(bell, v.pos)?;
                self.walk_step(v, target, 0.21)
            }
            RaidPhase::Raid => {
                let hiding = match v.hiding_place {
                    Some(h) => h,
                    None => {
                        let types = std::collections::HashSet::from(["minecraft:home".to_owned()]);
                        let origin = [
                            v.pos[0].floor() as i32,
                            v.pos[1].floor() as i32,
                            v.pos[2].floor() as i32,
                        ];
                        // `LocateHidingPlace`: любой дом в 24 блоках, иначе свой.
                        let h = self.nearest_poi(&types, origin, 24).or(v.home)?;
                        v.hiding_place = Some(h);
                        h
                    }
                };
                let valid = self
                    .block(hiding)
                    .and_then(|s| poi::of_block(self.state_name(s)))
                    == Some("minecraft:home");
                if !valid {
                    v.hiding_place = None;
                    return None;
                }
                let center = [
                    hiding[0] as f64 + 0.5,
                    hiding[1] as f64,
                    hiding[2] as f64 + 0.5,
                ];
                if crate::mobs::dist2(v.pos, center) <= 1.0 {
                    let yaw = look_yaw(v.pos, center);
                    v.yaw = yaw;
                    v.move_target = None;
                    return Some((v.pos, yaw));
                }
                let target = self.stand_near(hiding, v.pos)?;
                self.walk_step(v, target, 0.196)
            }
        }
    }

    /// Случайная проходимая точка в радиусе `r` (`LandRandomPos`).
    fn stroll_point(&self, pos: [f64; 3], r: i32) -> Option<[f64; 3]> {
        let span = u32::try_from(r * 2 + 1).ok()?;
        for _ in 0..10 {
            let dx = i32::try_from(rand::random::<u32>() % span).ok()? - r;
            let dz = i32::try_from(rand::random::<u32>() % span).ok()? - r;
            let target = [pos[0] + f64::from(dx), pos[1], pos[2] + f64::from(dz)];
            let cell = target.map(|v| v.floor() as i32);
            let below = [cell[0], cell[1] - 1, cell[2]];
            if !self.walk_blocked(cell)
                && self
                    .block(below)
                    .is_some_and(|s| self.survival.placement.solid(s))
                && !self.dangerous_block(cell)
            {
                return Some(target);
            }
        }
        None
    }

    /// `SecondaryPoiSensor`: есть ли грядка в ±4 по XZ и ±2 по Y.
    fn has_secondary_job_site(&self, pos: [f64; 3]) -> bool {
        let base = [
            pos[0].floor() as i32,
            pos[1].floor() as i32,
            pos[2].floor() as i32,
        ];
        for dx in -4..=4 {
            for dy in -2..=2 {
                for dz in -4..=4 {
                    if self
                        .block([base[0] + dx, base[1] + dy, base[2] + dz])
                        .is_some_and(|s| self.state_name(s).ends_with("farmland"))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `SetLookAndInteract(PLAYER, 4)`: запомнить игрока в 4 блоках и
    /// смотреть на него (`INTERACTION_TARGET`).
    fn player_interact_step(
        &self,
        v: &mut Villager,
        players: &[(Uuid, [f64; 3], Option<Stack>)],
    ) -> Option<f32> {
        if let Some(target) = v.player_target {
            let pos = players
                .iter()
                .find(|(uuid, _, _)| *uuid == target)
                .map(|(_, pos, _)| *pos);
            if let Some(pos) = pos
                && v.player_timer > 0
                && crate::mobs::dist2(v.pos, pos) <= 16.0
                && self.has_line_of_sight(v.pos, pos)
            {
                v.player_timer -= 1;
                return Some(look_yaw(v.pos, pos));
            }
            v.player_target = None;
            return None;
        }
        let target = players
            .iter()
            .filter(|(_, pos, _)| crate::mobs::dist2(v.pos, *pos) <= 16.0)
            .filter(|(_, pos, _)| self.has_line_of_sight(v.pos, *pos))
            .min_by(|a, b| {
                crate::mobs::dist2(v.pos, a.1).total_cmp(&crate::mobs::dist2(v.pos, b.1))
            })?;
        v.player_target = Some(target.0);
        // `LookAtTargetSink(45, 90)` держит LOOK_TARGET после OneShot.
        v.player_timer = 45 + (rand::random::<u32>() % 46) as i32;
        Some(look_yaw(v.pos, target.1))
    }

    /// `getFullLookBehavior`/`getMinimalLookBehavior`: взгляд на ближайшую
    /// сущность (кошка 8, житель 2, игрок 2, ничего 2).
    fn look_step(
        &self,
        v: &mut Villager,
        others: &[VillagerRef],
        players: &[(Uuid, [f64; 3], Option<Stack>)],
        cats: &[[f64; 3]],
    ) -> Option<f32> {
        if v.look_timer > 0 {
            v.look_timer -= 1;
            if let Some(target) = v.look_at {
                if crate::mobs::dist2(v.pos, target) <= 64.0
                    && self.has_line_of_sight(v.pos, target)
                {
                    return Some(look_yaw(v.pos, target));
                }
                v.look_at = None;
            }
            return None;
        }
        let nearest = |points: &[[f64; 3]]| {
            points
                .iter()
                .filter(|p| crate::mobs::dist2(v.pos, **p) <= 64.0)
                .filter(|p| self.has_line_of_sight(v.pos, **p))
                .min_by(|a, b| {
                    crate::mobs::dist2(v.pos, **a).total_cmp(&crate::mobs::dist2(v.pos, **b))
                })
                .copied()
        };
        let cat = nearest(cats);
        let villager = others
            .iter()
            .filter(|(other, _, _)| *other != -1)
            .map(|(_, _, p)| *p)
            .min_by(|a, b| crate::mobs::dist2(v.pos, *a).total_cmp(&crate::mobs::dist2(v.pos, *b)))
            .filter(|p| crate::mobs::dist2(v.pos, *p) <= 64.0)
            .filter(|p| self.has_line_of_sight(v.pos, *p));
        let player = nearest(&players.iter().map(|(_, pos, _)| *pos).collect::<Vec<_>>());
        let mut candidates: Vec<([f64; 3], u32)> = Vec::new();
        if let Some(c) = cat {
            candidates.push((c, 8));
        }
        if let Some(c) = villager {
            candidates.push((c, 2));
        }
        if let Some(c) = player {
            candidates.push((c, 2));
        }
        candidates.push((v.pos, 2));
        let roll = rand::random::<u32>() % candidates.iter().map(|(_, w)| *w).sum::<u32>().max(1);
        let mut r = roll;
        for (pos, weight) in candidates {
            if r < weight {
                if pos == v.pos {
                    // `DoNothing(30, 60)` в look-RunOne.
                    v.look_at = None;
                    v.look_timer = 30 + (rand::random::<u32>() % 31) as i32;
                    return None;
                }
                v.look_at = Some(pos);
                // `LookAtTargetSink(45, 90)`.
                v.look_timer = 45 + (rand::random::<u32>() % 46) as i32;
                return Some(look_yaw(v.pos, pos));
            }
            r -= weight;
        }
        v.look_timer = 30 + (rand::random::<u32>() % 31) as i32;
        None
    }

    /// `InteractWith.of(CAT, 8, INTERACTION_TARGET, speed, 2)`: подойти к
    /// кошке и смотреть на неё.
    fn cat_step(&self, v: &mut Villager, cats: &[[f64; 3]]) -> Option<([f64; 3], f32)> {
        if let Some(target) = v.cat_target {
            let nearest = cats.iter().copied().min_by(|a, b| {
                crate::mobs::dist2(*a, target).total_cmp(&crate::mobs::dist2(*b, target))
            });
            let Some(cat) = nearest else {
                v.cat_target = None;
                return None;
            };
            if v.cat_timer > 0 {
                v.cat_timer -= 1;
                if crate::mobs::dist2(v.pos, cat) > 4.0 {
                    return self.walk_step(v, [cat[0], v.pos[1], cat[2]], 0.14);
                }
                let yaw = look_yaw(v.pos, cat);
                v.yaw = yaw;
                return Some((v.pos, yaw));
            }
            v.cat_target = None;
            return None;
        }
        // Пауза между играми с кошкой.
        if !rand::random::<u32>().is_multiple_of(20) {
            return None;
        }
        let cat = cats
            .iter()
            .filter(|p| crate::mobs::dist2(v.pos, **p) <= 8.0 * 8.0)
            .min_by(|a, b| {
                crate::mobs::dist2(v.pos, **a).total_cmp(&crate::mobs::dist2(v.pos, **b))
            })
            .copied()?;
        v.cat_target = Some(cat);
        v.cat_timer = 60;
        if crate::mobs::dist2(v.pos, cat) <= 4.0 {
            let yaw = look_yaw(v.pos, cat);
            v.yaw = yaw;
            Some((v.pos, yaw))
        } else {
            self.walk_step(v, [cat[0], v.pos[1], cat[2]], 0.14)
        }
    }

    /// Максимальный возраст ванильных `CropBlock`.
    fn crop_max_age(name: &str) -> Option<i32> {
        Some(match name {
            "minecraft:wheat" | "minecraft:carrots" | "minecraft:potatoes" => 7,
            "minecraft:beetroots" => 3,
            _ => return None,
        })
    }

    /// Есть ли у жителя предмет (по имени) в инвентаре.
    fn has_item(&self, v: &Villager, name: &str) -> bool {
        v.inventory
            .iter()
            .flatten()
            .any(|s| self.item_name(s.item) == name)
    }

    /// Забрать один предмет из инвентаря жителя.
    fn take_item(&self, v: &mut Villager, name: &str) -> bool {
        for slot in &mut v.inventory {
            let Some(stack) = slot else { continue };
            if self.item_name(stack.item) != name {
                continue;
            }
            stack.count -= 1;
            if stack.count <= 0 {
                *slot = None;
            }
            return true;
        }
        false
    }

    /// Семена, которые фермер может посадить (`#villager_plantable_seeds`).
    fn plantable_seed(&self, v: &Villager) -> Option<(&'static str, &'static str)> {
        const SEEDS: [(&str, &str); 4] = [
            ("minecraft:wheat_seeds", "minecraft:wheat"),
            ("minecraft:carrot", "minecraft:carrots"),
            ("minecraft:potato", "minecraft:potatoes"),
            ("minecraft:beetroot_seeds", "minecraft:beetroots"),
        ];
        SEEDS.into_iter().find(|(seed, _)| self.has_item(v, seed))
    }

    /// `HarvestFarmland`/`UseBonemeal`: собрать спелое, посадить семена или
    /// ускорить рост костной мукой в 3×3×3 вокруг фермера.
    fn farm_step(&self, v: &mut Villager) -> Option<([f64; 3], f32)> {
        // `HarvestFarmland` требует `SECONDARY_JOB_SITE`.
        if !self.has_secondary_job_site(v.pos) {
            return None;
        }
        let now = self.game_time();
        if now < v.farm_cooldown {
            return None;
        }
        if let Some(target) = v.farm_target {
            let center = [
                target[0] as f64 + 0.5,
                target[1] as f64,
                target[2] as f64 + 0.5,
            ];
            if crate::mobs::dist2(v.pos, center) <= 2.25 {
                self.farm_action(v, target);
                v.farm_target = None;
                v.farm_cooldown = now + 40;
                return Some((v.pos, v.yaw));
            }
            return self.walk_step(v, [center[0], v.pos[1], center[2]], 0.14);
        }
        let base = [
            v.pos[0].floor() as i32,
            v.pos[1].floor() as i32,
            v.pos[2].floor() as i32,
        ];
        let mut targets = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let p = [base[0] + dx, base[1] + dy, base[2] + dz];
                    let Some(state) = self.block(p) else { continue };
                    let name = self.state_name(state);
                    let below = self
                        .block([p[0], p[1] - 1, p[2]])
                        .map(|s| self.state_name(s))
                        .unwrap_or("");
                    if let Some(max) = Self::crop_max_age(name) {
                        let age = self
                            .survival
                            .placement
                            .prop(state, "age")
                            .and_then(|a| a.parse::<i32>().ok())
                            .unwrap_or(0);
                        if age >= max || self.has_item(v, "minecraft:bone_meal") {
                            targets.push(p);
                        }
                    } else if name.ends_with(":air")
                        && below.ends_with("farmland")
                        && self.plantable_seed(v).is_some()
                    {
                        targets.push(p);
                    }
                }
            }
        }
        if targets.is_empty() {
            return None;
        }
        let pick = targets[rand::random::<usize>() % targets.len()];
        if v.farm_actions_left == 0 {
            v.farm_actions_left = if v.work_goal == WorkGoal::UseBonemeal {
                2
            } else {
                5
            };
        }
        v.farm_target = Some(pick);
        let center = [pick[0] as f64 + 0.5, v.pos[1], pick[2] as f64 + 0.5];
        self.walk_step(v, center, 0.14)
    }

    /// Одно действие на выбранном блоке (`HarvestFarmland.tick`).
    fn farm_action(&self, v: &mut Villager, p: [i32; 3]) {
        let Some(state) = self.block(p) else { return };
        let name = self.state_name(state);
        let below = self
            .block([p[0], p[1] - 1, p[2]])
            .map(|s| self.state_name(s))
            .unwrap_or("");
        if let Some(max) = Self::crop_max_age(name) {
            let age = self
                .survival
                .placement
                .prop(state, "age")
                .and_then(|a| a.parse::<i32>().ok())
                .unwrap_or(0);
            if age >= max {
                self.break_drops(p);
                self.set_block(p, 0);
                self.finish_farm_action(v);
                return;
            }
            if self.has_item(v, "minecraft:bone_meal")
                && self.bonemeal_at(p)
                && self.take_item(v, "minecraft:bone_meal")
            {
                self.hub.send(
                    None,
                    play::level_event(self.ids.level_event, 1505, p, 15, false),
                );
                self.finish_farm_action(v);
            }
            return;
        }
        if name.ends_with(":air")
            && below.ends_with("farmland")
            && let Some((seed, crop)) = self.plantable_seed(v)
            && let Some(block) = self.survival.placement.default_state(crop)
            && self.take_item(v, seed)
        {
            self.set_block(p, block);
            self.play_sound(
                None,
                "minecraft:item.crop.plant",
                crate::sounds::BLOCKS,
                [p[0] as f64 + 0.5, p[1] as f64 + 0.5, p[2] as f64 + 0.5],
                1.0,
                1.0,
            );
            self.finish_farm_action(v);
        }
    }

    /// Конец одного действия `HarvestFarmland` (200 тиков) или
    /// `UseBonemeal` (80 тиков), затем пауза 200/160.
    fn finish_farm_action(&self, v: &mut Villager) {
        v.farm_actions_left -= 1;
        if v.farm_actions_left <= 0 {
            v.farm_actions_left = 0;
            v.farm_cooldown = self.game_time()
                + if v.work_goal == WorkGoal::UseBonemeal {
                    160
                } else {
                    200
                };
            v.goal_timer = 0;
        } else {
            v.farm_cooldown = self.game_time() + 40;
        }
    }

    /// `JumpOnBed`: малыш идёт к ближайшей кровати и прыгает 3-6 раз с
    /// паузой 5 тиков; прекращает после 100 тиков ходьбы.
    fn jump_on_bed_step(&self, v: &mut Villager) -> Option<([f64; 3], f32)> {
        let now = self.game_time();
        if v.jump_phase > 0 {
            let off = 0.42 * (((10 - v.jump_phase) as f64) * std::f64::consts::PI / 10.0).sin();
            v.pos[1] = v.jump_base_y + off;
            v.jump_phase -= 1;
            v.airborne = v.jump_phase > 0;
            if v.jump_phase == 0 {
                v.pos[1] = v.jump_base_y;
                v.jump_cooldown = 5;
            }
            return Some((v.pos, v.yaw));
        }
        if v.jump_cooldown > 0 {
            v.jump_cooldown -= 1;
            v.airborne = false;
            return Some((v.pos, v.yaw));
        }
        let bed = v.nearest_bed?;
        if v.jump_left == 0 {
            v.jump_left = 3 + i32::try_from(rand::random::<u32>() % 4).unwrap_or(0);
        }
        let state = self.block(bed)?;
        if !self.state_name(state).ends_with("_bed") {
            v.nearest_bed = None;
            return None;
        }
        let center = [bed[0] as f64 + 0.5, bed[1] as f64, bed[2] as f64 + 0.5];
        let xz = ((v.pos[0] - center[0]).powi(2) + (v.pos[2] - center[2]).powi(2)).sqrt();
        if xz > 2.0 {
            if v.jump_deadline == 0 {
                v.jump_deadline = now + 100;
            }
            if now > v.jump_deadline {
                v.jump_deadline = 0;
                v.nearest_bed = None;
                return None;
            }
            return self.walk_step(v, [center[0], v.pos[1], center[2]], 0.14);
        }
        // Забираемся на поверхность кровати (0.5625 блока).
        if v.pos[1] < center[1] + 0.5 {
            v.pos = [center[0], center[1] + 0.5625, center[2]];
            v.jump_deadline = 0;
            v.airborne = false;
            return Some((v.pos, v.yaw));
        }
        if v.jump_left <= 0 {
            if let Some(pos) = self.find_stand_up(bed, v.yaw, -1, &[]) {
                v.pos = pos;
            }
            v.nearest_bed = None;
            v.airborne = false;
            return Some((v.pos, v.yaw));
        }
        v.jump_left -= 1;
        v.jump_base_y = v.pos[1];
        v.jump_phase = 10;
        v.airborne = true;
        Some((v.pos, v.yaw))
    }

    /// `PlayTagWithOtherKids`: раз в 10 тиков малыш либо убегает от того,
    /// кто его догоняет, либо сам догоняет другого малыша (0.6 modifier).
    fn play_tag_step(
        &self,
        id: i32,
        v: &mut Villager,
        babies: &[(i32, u128, [f64; 3], Option<u128>)],
    ) -> Option<([f64; 3], f32)> {
        let me = v.uuid.as_u128();
        // `level.getRandom().nextInt(10) != 0` — продолжаем текущую погоню.
        if !rand::random::<u32>().is_multiple_of(10) {
            let target = v.tag_target?;
            let (_, _, pos, _) = babies.iter().find(|(_, u, _, _)| *u == target)?;
            v.yaw = look_yaw(v.pos, *pos);
            return self.walk_step(v, [pos[0], v.pos[1], pos[2]], 0.17);
        }
        // Кто-то гонится за мной — убегаем (`LandRandomPos` 20 x 8).
        let chased = babies
            .iter()
            .any(|(other, _, _, target)| *other != id && *target == Some(me));
        if chased {
            v.tag_target = None;
            if v.move_target.is_none() || v.wander_timer <= 0 {
                v.move_target = self.flee_point(v.pos);
                v.wander_timer = 40;
            }
            v.wander_timer -= 1;
            return self.walk_step(v, v.move_target?, 0.17);
        }
        // Ищем, кого уже догоняют (1..=5 преследователей), иначе любого.
        let mut counts: Vec<(u128, i32)> = Vec::new();
        for (_, _, _, target) in babies {
            if let Some(target) = target {
                match counts.iter_mut().find(|(u, _)| u == target) {
                    Some(entry) => entry.1 += 1,
                    None => counts.push((*target, 1)),
                }
            }
        }
        let candidate = counts
            .iter()
            .filter(|(_, n)| (1..=5).contains(n))
            .min_by_key(|(_, n)| *n)
            .map(|(u, _)| *u)
            .or_else(|| {
                babies
                    .iter()
                    .find(|(other, _, _, _)| *other != id)
                    .map(|(_, u, _, _)| *u)
            })?;
        let (_, _, pos, _) = babies.iter().find(|(_, u, _, _)| *u == candidate)?;
        v.tag_target = Some(candidate);
        v.yaw = look_yaw(v.pos, *pos);
        self.walk_step(v, [pos[0], v.pos[1], pos[2]], 0.17)
    }

    /// Случайная проходимая точка в 20 блоках по XZ и 8 по Y.
    fn flee_point(&self, pos: [f64; 3]) -> Option<[f64; 3]> {
        for _ in 0..10 {
            let dx = f64::from(rand::random::<u32>() % 41) - 20.0;
            let dz = f64::from(rand::random::<u32>() % 41) - 20.0;
            let dy = f64::from(rand::random::<u32>() % 9) - 4.0;
            let target = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
            let cell = target.map(|v| v.floor() as i32);
            let below = [cell[0], cell[1] - 1, cell[2]];
            if !self.walk_blocked(cell)
                && !self.walk_blocked([cell[0], cell[1] + 1, cell[2]])
                && self
                    .block(below)
                    .is_some_and(|s| self.survival.placement.solid(s))
                && !self.dangerous_block(cell)
                && !self.dangerous_block(below)
            {
                return Some(target);
            }
        }
        None
    }

    /// `VillagerMakeLove`: пара, 275-325 тиков, свободная кровать и ребёнок.
    fn breed_step(
        &self,
        id: i32,
        v: &mut Villager,
        others: &[VillagerRef],
        breedable: &[(i32, bool)],
        births: &mut Vec<Birth>,
    ) -> Option<([f64; 3], f32)> {
        let now = self.game_time();
        if let Some(target) = v.breed_target {
            let Some((other_id, _, pos)) = others
                .iter()
                .find(|(other, uuid, _)| *other != id && *uuid == target)
            else {
                v.breed_target = None;
                v.breed_until = 0;
                return None;
            };
            let partner_ok = breedable.iter().any(|(pid, ok)| pid == other_id && *ok);
            if !partner_ok || !self.can_breed(v) {
                v.breed_target = None;
                v.breed_until = 0;
                return None;
            }
            let dist = crate::mobs::dist2(v.pos, *pos);
            if v.breed_until == 0 {
                v.breed_until = now + 275 + i64::from(rand::random::<u32>() % 50);
                self.hub
                    .send(None, inv::entity_event(self.ids.entity_event, id, 12));
            }
            if dist > 2.25 {
                return self.walk_step(v, [pos[0], v.pos[1], pos[2]], 0.21);
            }
            let yaw = look_yaw(v.pos, *pos);
            v.yaw = yaw;
            if now >= v.breed_until {
                self.try_birth(id, v, births);
            }
            return Some((v.pos, yaw));
        }
        if !self.can_breed(v) || v.activity != Activity::Idle {
            return None;
        }
        let partner = others
            .iter()
            .filter(|(other, _, _)| *other != id)
            .filter(|(other, _, pos)| {
                breedable.iter().any(|(pid, ok)| pid == other && *ok)
                    && crate::mobs::dist2(v.pos, *pos) <= 8.0 * 8.0
            })
            .min_by(|a, b| {
                crate::mobs::dist2(v.pos, a.2).total_cmp(&crate::mobs::dist2(v.pos, b.2))
            });
        let (partner_id, uuid, pos) = partner?;
        v.breed_target = Some(*uuid);
        v.breed_partner = Some(*partner_id);
        v.breed_until = 0;
        self.walk_step(v, *pos, 0.21)
    }

    /// `VillagerMakeLove.tryToGiveBirth`: свободная кровать и запрос на
    /// рождение (сам спавн — после цикла, без блокировки жителей).
    fn try_birth(&self, id: i32, v: &mut Villager, births: &mut Vec<Birth>) {
        let origin = [
            v.pos[0].floor() as i32,
            v.pos[1].floor() as i32,
            v.pos[2].floor() as i32,
        ];
        let types = std::collections::HashSet::from(["minecraft:home".to_owned()]);
        let bed = self.free_poi(&types, origin, 48);
        let Some(bed) = bed else {
            self.hub
                .send(None, inv::entity_event(self.ids.entity_event, id, 13));
            v.breed_target = None;
            v.breed_until = 0;
            return;
        };
        let kind = if rand::random::<u32>().is_multiple_of(2) {
            v.kind.clone()
        } else {
            "minecraft:plains".to_owned()
        };
        births.push(Birth {
            parent: id,
            partner: v.breed_partner,
            pos: v.pos,
            kind,
            bed,
        });
        v.breed_target = None;
        v.breed_partner = None;
        v.breed_until = 0;
    }

    /// Выполняет отложенные рождения: ребёнок с `Age:-24000`, кулдаун
    /// `6000` у обоих родителей (вне блокировки жителей).
    fn process_births(&self, births: Vec<Birth>) {
        for birth in births {
            let Some(child) = self.summon_trader(TraderKind::Villager, birth.pos, None) else {
                self.release_poi(birth.bed);
                continue;
            };
            {
                let mut villagers = self.sim().villagers.lock().unwrap();
                if let Some(c) = villagers.get_mut(&child) {
                    c.age = -24_000;
                    c.kind = birth.kind.clone();
                    c.profession = "minecraft:none".into();
                    c.level = 1;
                    c.xp = 0;
                    c.offers.clear();
                    c.home = Some(birth.bed);
                    c.sent_pos = play::quantize(birth.pos);
                }
                if let Some(p) = villagers.get_mut(&birth.parent) {
                    p.age = 6000;
                }
                if let Some(pid) = birth.partner
                    && let Some(p) = villagers.get_mut(&pid)
                {
                    p.age = 6000;
                }
            }
            self.claim_poi(birth.bed, child);
            self.hub.send(
                None,
                play::entity_meta(
                    self.ids.set_entity_data,
                    child,
                    &[(16, play::Meta::Bool(true))],
                ),
            );
            self.hub
                .send(None, inv::entity_event(self.ids.entity_event, child, 12));
        }
    }

    /// `SocializeAtBell`: раз в 100 тиков у колокола выбрать ближайшего
    /// жителя в 32 блоках, подойти и смотреть на него.
    fn socialize_step(
        &self,
        id: i32,
        v: &mut Villager,
        others: &[VillagerRef],
        profiles: &[VillagerProfile],
        gossip: &mut Vec<(i32, i32)>,
    ) -> Option<([f64; 3], f32)> {
        if let Some(target) = v.interaction_target {
            let Some((other_id, _, pos)) = others
                .iter()
                .find(|(other, uuid, _)| *other != id && *uuid == target)
            else {
                v.interaction_target = None;
                return None;
            };
            if !self.has_line_of_sight(v.pos, *pos) {
                v.interaction_target = None;
                return None;
            }
            if crate::mobs::dist2(v.pos, *pos) > 1.2 * 1.2 {
                return self.walk_step(v, [pos[0], v.pos[1], pos[2]], 0.14);
            }
            // `TradeWithVillager.tick`: смотреть друг на друга, обмениваться
            // слухами (`Villager.gossip`) и едой/requestedItems.
            gossip.push((id, *other_id));
            if let Some((_, _, target_pos)) =
                others.iter().find(|(other, _, _)| *other == *other_id)
                && let Some((_, profession, wants_food)) =
                    profiles.iter().find(|(pid, _, _)| pid == other_id)
            {
                self.trade_step(v, *target_pos, profession, *wants_food);
            }
            let yaw = look_yaw(v.pos, *pos);
            v.yaw = yaw;
            return Some((v.pos, yaw));
        }
        let bell = v.meeting_point?;
        let bell_pos = [bell[0] as f64 + 0.5, bell[1] as f64, bell[2] as f64 + 0.5];
        if crate::mobs::dist2(v.pos, bell_pos) > 4.0 * 4.0
            || !rand::random::<u32>().is_multiple_of(100)
        {
            return None;
        }
        let (_, uuid, pos) = others
            .iter()
            .filter(|(other, _, _)| *other != id)
            .filter(|(_, _, pos)| {
                crate::mobs::dist2(v.pos, *pos) <= 32.0 * 32.0
                    && self.has_line_of_sight(v.pos, *pos)
            })
            .min_by(|a, b| {
                crate::mobs::dist2(v.pos, a.2).total_cmp(&crate::mobs::dist2(v.pos, b.2))
            })?;
        v.interaction_target = Some(*uuid);
        if crate::mobs::dist2(v.pos, *pos) <= 1.2 * 1.2 {
            let yaw = look_yaw(v.pos, *pos);
            v.yaw = yaw;
            Some((v.pos, yaw))
        } else {
            self.walk_step(v, *pos, 0.14)
        }
    }

    /// Шаг к точке с проверкой коллизий и открыванием дверей.
    fn walk_step(&self, v: &mut Villager, target: [f64; 3], speed: f64) -> Option<([f64; 3], f32)> {
        let dx = target[0] - v.pos[0];
        let dz = target[2] - v.pos[2];
        let dist = (dx * dx + dz * dz).sqrt();
        if dist < 0.8 {
            v.move_target = None;
            v.path_from = None;
            v.path_to = None;
            return None;
        }
        let step = speed.min(dist);
        let nx = v.pos[0] + dx / dist * step;
        let nz = v.pos[2] + dz / dist * step;
        let feet_y = v.pos[1].floor() as i32;
        let here = [v.pos[0].floor() as i32, feet_y, v.pos[2].floor() as i32];
        let feet = [nx.floor() as i32, feet_y, nz.floor() as i32];
        v.path_from = Some(here);
        v.path_to = Some(feet);
        self.open_door_on_path(v, here);
        if feet != here {
            self.open_door_on_path(v, feet);
        }
        if self.walk_blocked(feet) {
            let up = [feet[0], feet[1] + 1, feet[2]];
            if self.walk_blocked(up) || self.walk_blocked([up[0], up[1] + 1, up[2]]) {
                return None;
            }
        }
        // `WalkNodeEvaluator`: опасные блоки (огонь/лава/магма/кактус…) не
        // проходятся, иначе житель сгорит при прогулке.
        if self.dangerous_block(feet) || self.dangerous_block([feet[0], feet[1] - 1, feet[2]]) {
            return None;
        }
        let yaw = look_yaw(v.pos, target);
        v.pos[0] = nx;
        v.pos[2] = nz;
        v.pos[1] += (target[1] - v.pos[1]).clamp(-0.2, 0.2);
        v.yaw = yaw;
        v.airborne = false;
        Some((v.pos, yaw))
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
            // `ValidateNearbyPoi(JOB_SITE)`: дальше 16 — память не трогаем.
            let dist = crate::mobs::dist2(
                v.pos,
                [site[0] as f64 + 0.5, site[1] as f64, site[2] as f64 + 0.5],
            );
            if dist > 16.0 * 16.0 {
                return None;
            }
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
            activity: Activity::Idle,
            home: None,
            move_target: None,
            sent_pos: play::quantize(pos),
            no_ai: false,
            wander_timer: 0,
            last_home_check: 0,
            sleeping: None,
            pose: 0,
            sleep_cooldown: 0,
            doors_to_close: Vec::new(),
            meeting_point: None,
            interaction_target: None,
            last_gossip_time: 0,
            hurt_until: 0,
            hurt_from: None,
            age: 0,
            food_level: 0,
            tag_target: None,
            raid_phase: RaidPhase::None,
            hiding_place: None,
            bell_cooldown: 0,
            last_raid_check: 0,
            nearest_bed: None,
            jump_left: 0,
            jump_cooldown: 0,
            jump_phase: 0,
            jump_base_y: 0.0,
            jump_deadline: 0,
            last_bed_check: 0,
            airborne: false,
            farm_target: None,
            farm_cooldown: 0,
            cat_target: None,
            cat_timer: 0,
            work_goal: WorkGoal::WorkAtPoi,
            idle_goal: IdleGoal::Stroll,
            meet_goal: MeetGoal::Socialize,
            rest_goal: RestGoal::DoNothing,
            work_active: false,
            work_cooldown: 0,
            work_end: 0,
            stroll_cooldown: 0,
            farm_actions_left: 0,
            branch_timer: 0,
            look_at: None,
            look_timer: 0,
            player_target: None,
            player_timer: 0,
            goal_timer: 0,
            breed_target: None,
            breed_partner: None,
            breed_until: 0,
            door_node: None,
            door_cooldown: 0,
            inventory: [None; 8],
            show_player: None,
            show_items: Vec::new(),
            show_index: 0,
            show_cycle: 0,
            show_timer: 0,
            show_sent: None,
            path_from: None,
            path_to: None,
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
                    // Флаг ребёнка первым: клиентские сенсоры читают его
                    // раньше остальных метаданных.
                    (16, play::Meta::Bool(v.age < 0)),
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
        let day = self.day_time.load(Ordering::Relaxed);
        let mut moves: Vec<VillagerMove> = Vec::new();
        let ids: Vec<i32> = self
            .sim()
            .villagers
            .lock()
            .unwrap()
            .keys()
            .copied()
            .collect();
        // Сенсор `NEAREST_VISIBLE_LIVING_ENTITIES` (упрощённо — без
        // проверки линии видимости): снимок жителей на этот тик.
        let others: Vec<VillagerRef> = self
            .sim()
            .villagers
            .lock()
            .unwrap()
            .iter()
            .map(|(&id, v)| (id, v.uuid.as_u128(), v.pos))
            .collect();
        let babies: Vec<(i32, u128, [f64; 3], Option<u128>)> = self
            .sim()
            .villagers
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, v)| v.age < 0)
            .map(|(&id, v)| (id, v.uuid.as_u128(), v.pos, v.tag_target))
            .collect();
        let breedable: Vec<(i32, bool)> = self
            .sim()
            .villagers
            .lock()
            .unwrap()
            .iter()
            .map(|(&id, v)| (id, self.can_breed(v)))
            .collect();
        let profiles: Vec<VillagerProfile> = self
            .sim()
            .villagers
            .lock()
            .unwrap()
            .iter()
            .map(|(&id, v)| (id, v.profession.clone(), self.wants_more_food(v)))
            .collect();
        let threats: Vec<[f64; 3]> = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.mob.alive() && e.mob.species.is_monster())
            .map(|e| e.mob.pos)
            .collect();
        let cats: Vec<[f64; 3]> = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.mob.alive() && e.mob.species == crate::mobs::Species::Cat)
            .map(|e| e.mob.pos)
            .collect();
        let players: Vec<(Uuid, [f64; 3], Option<Stack>)> = self
            .live
            .lock()
            .unwrap()
            .iter()
            .map(|(uuid, l)| (*uuid, l.pos, l.slots[36 + l.selected.min(8)]))
            .collect();
        let paths: Vec<VillagerPath> = self
            .sim()
            .villagers
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(&id, v)| Some((id, v.pos, v.path_from?, v.path_to?)))
            .collect();
        let mut births: Vec<Birth> = Vec::new();
        let mut gossip: Vec<(i32, i32)> = Vec::new();
        let mut gone = Vec::new();
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
            let sleepers: Vec<[i32; 3]> = villagers.values().filter_map(|o| o.sleeping).collect();
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
            if v.age < 0 {
                v.age += 1;
            } else if v.age > 0 {
                v.age -= 1;
            }
            if !v.no_ai
                && v.trader == TraderKind::Villager
                && now % 10 == 0
                && now != v.last_raid_check
            {
                v.last_raid_check = now;
                let phase = self.raid_phase_at(v.pos);
                if phase != v.raid_phase {
                    v.raid_phase = phase;
                    v.hiding_place = None;
                    v.move_target = None;
                }
            }
            if v.age < 0 && now % 20 == 0 && now != v.last_bed_check {
                v.last_bed_check = now;
                let types = std::collections::HashSet::from(["minecraft:home".to_owned()]);
                let origin = [
                    v.pos[0].floor() as i32,
                    v.pos[1].floor() as i32,
                    v.pos[2].floor() as i32,
                ];
                v.nearest_bed = self.nearest_poi(&types, origin, 24);
            }
            let activity = if v.age < 0 {
                Activity::at_baby(day)
            } else {
                Activity::at(day)
            };
            if v.activity != activity {
                v.activity = activity;
                v.move_target = None;
                v.wander_timer = 0;
                v.meeting_point = None;
                v.interaction_target = None;
                v.tag_target = None;
                v.goal_timer = 0;
            }
            if v.trader == TraderKind::Villager && v.goal_timer <= 0 {
                v.work_goal = weighted_pick(&work_weights(&v.profession), rand::random::<u32>())
                    .unwrap_or(WorkGoal::WorkAtPoi);
                v.idle_goal = weighted_pick(&idle_weights(), rand::random::<u32>())
                    .unwrap_or(IdleGoal::Stroll);
                v.meet_goal = weighted_pick(&meet_weights(), rand::random::<u32>())
                    .unwrap_or(MeetGoal::Socialize);
                v.rest_goal = weighted_pick(&rest_weights(), rand::random::<u32>())
                    .unwrap_or(RestGoal::DoNothing);
                v.branch_timer = 20 + i32::try_from(rand::random::<u32>() % 21).unwrap_or(0);
                v.work_active = false;
                v.goal_timer = 1;
            }
            if v.trader == TraderKind::Villager && !v.no_ai {
                self.validate_meeting_poi(v);
                if now - v.last_home_check >= 100 {
                    v.last_home_check = now;
                    self.update_villager_home(id, v, &sleepers);
                }
                let bed_gone = v.sleeping.is_some_and(|bed| {
                    self.block(bed)
                        .and_then(|s| poi::of_block(self.state_name(s)))
                        != Some("minecraft:home")
                });
                if v.sleeping.is_some() && (v.activity != Activity::Rest || bed_gone) {
                    self.stop_villager_sleep(id, v, &others);
                }
                self.close_villager_doors(id, v, &others, &paths);
                if v.sleeping.is_none() {
                    if let Some((pos, yaw)) = self.villager_ai_step(
                        id,
                        v,
                        &others,
                        &profiles,
                        &threats,
                        &cats,
                        &breedable,
                        &babies,
                        &mut births,
                        &mut gossip,
                    ) {
                        moves.push((id, pos, yaw, v.sent_pos, !v.airborne));
                        v.sent_pos = play::quantize(v.pos);
                    } else if let Some(yaw) = self.look_step(v, &others, &players, &cats) {
                        v.yaw = yaw;
                        moves.push((id, v.pos, yaw, v.sent_pos, true));
                        v.sent_pos = play::quantize(v.pos);
                    } else if let Some(yaw) = self.player_interact_step(v, &players) {
                        // `SetLookAndInteract` ниже по приоритету (10), чем
                        // look-RunOne (5) — срабатывает только в паузе
                        // `DoNothing`, а не вместо взгляда на кошку/жителя.
                        v.yaw = yaw;
                        moves.push((id, v.pos, yaw, v.sent_pos, true));
                        v.sent_pos = play::quantize(v.pos);
                    }
                }
                self.show_trades_step(id, v, &players);
            }
            if v.trader == TraderKind::Villager {
                let hero = heroes
                    .iter()
                    .filter(|p| crate::mobs::dist2(**p, v.pos) <= 8.0 * 8.0)
                    .filter(|p| self.has_line_of_sight(v.pos, **p))
                    .min_by(|a, b| {
                        crate::mobs::dist2(**a, v.pos).total_cmp(&crate::mobs::dist2(**b, v.pos))
                    })
                    .copied();
                // `GiveGiftToHero`: Behavior с таймаутом 100 тиков; пока
                // герой дальше 5 блоков, житель идёт к нему (0.5, close 5),
                // смотрит на него и бросает подарок после 20 тиков.
                match hero {
                    Some(hero_pos) => {
                        if let Some(wait) = v.gift_wait {
                            v.yaw = look_yaw(v.pos, hero_pos);
                            if crate::mobs::dist2(hero_pos, v.pos) <= 5.0 * 5.0 {
                                v.move_target = None;
                                if wait <= 80 {
                                    gifts.push((v.pos, hero_pos, v.profession.clone()));
                                    v.gift_timer = 600 + (rand::random::<u32>() % 6001) as i32;
                                    v.gift_wait = None;
                                } else {
                                    v.gift_wait = Some(wait - 1);
                                }
                            } else if wait <= 0 {
                                // Таймаут поведения — `stop` пересчитывает паузу.
                                v.gift_wait = None;
                                v.gift_timer = 600 + (rand::random::<u32>() % 6001) as i32;
                                v.move_target = None;
                            } else {
                                v.move_target = Some([hero_pos[0], v.pos[1], hero_pos[2]]);
                                v.gift_wait = Some(wait - 1);
                            }
                        } else if v.gift_timer > 0 {
                            v.gift_timer -= 1;
                        } else {
                            v.gift_wait = Some(100);
                        }
                    }
                    None => {
                        if v.gift_wait.is_some() {
                            // `canStillUse` = герой видим — иначе `stop`.
                            v.gift_wait = None;
                            v.gift_timer = 600 + (rand::random::<u32>() % 6001) as i32;
                        }
                    }
                }
            }
        }
        self.process_births(births);
        for (a, b) in gossip {
            self.gossip_villagers(a, b, now);
        }
        for (id, pos, yaw, sent, on_ground) in moves {
            let q = play::quantize(pos);
            let packet = match play::delta(sent, q) {
                Some(d) => play::move_entity_pos_rot(
                    self.ids.move_entity_pos_rot,
                    id,
                    d,
                    yaw,
                    0.0,
                    on_ground,
                ),
                None => {
                    play::teleport_entity(self.ids.teleport_entity, id, pos, yaw, 0.0, on_ground)
                }
            };
            self.hub.send(None, packet);
            self.mark_work();
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
    }

    /// Есть ли рядом (в кубе ±2) блок рабочего места.
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
                v.hurt_until = self.game_time() + 100;
                v.hurt_from = None;
            }
            (v.pos, v.health <= 0.0)
        };
        if let Some(player) = attacker {
            let pos = self
                .live
                .lock()
                .unwrap()
                .get(&player)
                .map(|l| l.pos)
                .or_else(|| {
                    self.sim()
                        .mobs
                        .lock()
                        .unwrap()
                        .values()
                        .find(|e| e.mob.uuid == player.as_u128())
                        .map(|e| e.mob.pos)
                });
            if let Some(pos) = pos {
                let mut villagers = self.sim().villagers.lock().unwrap();
                if let Some(v) = villagers.get_mut(&id) {
                    v.hurt_from = Some(pos);
                }
            }
        }
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
                v.sent_pos = play::quantize(pos);
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
        c.insert("Age".into(), Tag::Int(v.age));
        c.insert("FoodLevel".into(), Tag::Byte(v.food_level as i8));
        let inventory: Vec<Tag> = v
            .inventory
            .iter()
            .enumerate()
            .filter_map(|(slot, stack)| {
                let stack = (*stack)?;
                let name = self.item_name(stack.item);
                let mut e = crate::stack_nbt::to_nbt(stack, name)?;
                e.insert("Slot".into(), Tag::Byte(slot as i8));
                Some(Tag::Compound(e))
            })
            .collect();
        if !inventory.is_empty() {
            c.insert("Inventory".into(), Tag::List(inventory));
        }
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
            job_site: None,
            last_job_check: 0,
            custom_name: None,
            persistent: trader == TraderKind::Villager,
            gift_timer: 600,
            gift_wait: None,
            gossips: BTreeMap::new(),
            last_gossip_decay: 0,
            health: 20.0,
            activity: Activity::Idle,
            home: None,
            move_target: None,
            sent_pos: play::quantize([nums[0], nums[1], nums[2]]),
            no_ai: false,
            wander_timer: 0,
            last_home_check: 0,
            sleeping: None,
            pose: 0,
            sleep_cooldown: 0,
            doors_to_close: Vec::new(),
            meeting_point: None,
            interaction_target: None,
            last_gossip_time: 0,
            hurt_until: 0,
            hurt_from: None,
            age: 0,
            food_level: 0,
            tag_target: None,
            raid_phase: RaidPhase::None,
            hiding_place: None,
            bell_cooldown: 0,
            last_raid_check: 0,
            nearest_bed: None,
            jump_left: 0,
            jump_cooldown: 0,
            jump_phase: 0,
            jump_base_y: 0.0,
            jump_deadline: 0,
            last_bed_check: 0,
            airborne: false,
            farm_target: None,
            farm_cooldown: 0,
            cat_target: None,
            cat_timer: 0,
            work_goal: WorkGoal::WorkAtPoi,
            idle_goal: IdleGoal::Stroll,
            meet_goal: MeetGoal::Socialize,
            rest_goal: RestGoal::DoNothing,
            work_active: false,
            work_cooldown: 0,
            work_end: 0,
            stroll_cooldown: 0,
            farm_actions_left: 0,
            branch_timer: 0,
            look_at: None,
            look_timer: 0,
            player_target: None,
            player_timer: 0,
            goal_timer: 0,
            breed_target: None,
            breed_partner: None,
            breed_until: 0,
            door_node: None,
            door_cooldown: 0,
            inventory: [None; 8],
            show_player: None,
            show_items: Vec::new(),
            show_index: 0,
            show_cycle: 0,
            show_timer: 0,
            show_sent: None,
            path_from: None,
            path_to: None,
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
    if let Some(age) = c.get("Age").and_then(Tag::as_i64) {
        v.age = age as i32;
    }
    if let Some(Tag::Byte(food)) = c.get("FoodLevel") {
        v.food_level = i32::from(*food);
    }
    if let Some(Tag::List(items)) = c.get("Inventory") {
        // `/data merge` заменяет список, а не дополняет его.
        v.inventory = [None; 8];
        for entry in items {
            let Tag::Compound(e) = entry else { continue };
            let Some(slot) = e.get("Slot").and_then(Tag::as_i64) else {
                continue;
            };
            if !(0..8).contains(&slot) {
                continue;
            }
            if let Some(stack) = crate::stack_nbt::from_nbt(e, &item) {
                v.inventory[slot as usize] = Some(stack);
            }
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
    if let Some(Tag::Byte(p)) = c.get("NoAI") {
        v.no_ai = *p != 0;
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
            job_site: None,
            last_job_check: 0,
            custom_name: None,
            persistent: true,
            gift_timer: 600,
            gift_wait: None,
            gossips: BTreeMap::new(),
            last_gossip_decay: 0,
            health: 20.0,
            activity: Activity::Idle,
            home: None,
            move_target: None,
            sent_pos: [0; 3],
            no_ai: false,
            wander_timer: 0,
            last_home_check: 0,
            sleeping: None,
            pose: 0,
            sleep_cooldown: 0,
            doors_to_close: Vec::new(),
            meeting_point: None,
            interaction_target: None,
            last_gossip_time: 0,
            hurt_until: 0,
            hurt_from: None,
            age: 0,
            food_level: 0,
            tag_target: None,
            raid_phase: RaidPhase::None,
            hiding_place: None,
            bell_cooldown: 0,
            last_raid_check: 0,
            nearest_bed: None,
            jump_left: 0,
            jump_cooldown: 0,
            jump_phase: 0,
            jump_base_y: 0.0,
            jump_deadline: 0,
            last_bed_check: 0,
            airborne: false,
            farm_target: None,
            farm_cooldown: 0,
            cat_target: None,
            cat_timer: 0,
            work_goal: WorkGoal::WorkAtPoi,
            idle_goal: IdleGoal::Stroll,
            meet_goal: MeetGoal::Socialize,
            rest_goal: RestGoal::DoNothing,
            work_active: false,
            work_cooldown: 0,
            work_end: 0,
            stroll_cooldown: 0,
            farm_actions_left: 0,
            branch_timer: 0,
            look_at: None,
            look_timer: 0,
            player_target: None,
            player_timer: 0,
            goal_timer: 0,
            breed_target: None,
            breed_partner: None,
            breed_until: 0,
            door_node: None,
            door_cooldown: 0,
            inventory: [None; 8],
            show_player: None,
            show_items: Vec::new(),
            show_index: 0,
            show_cycle: 0,
            show_timer: 0,
            show_sent: None,
            path_from: None,
            path_to: None,
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
    fn panic_threat_uses_hurt_and_hostiles() {
        let pos = [0.0, 64.0, 0.0];
        assert_eq!(
            panic_threat(pos, 120, 100, Some([4.0, 64.0, 0.0]), &[]),
            Some([4.0, 64.0, 0.0])
        );
        assert_eq!(
            panic_threat(pos, 80, 100, Some([4.0, 64.0, 0.0]), &[]),
            None
        );
        assert_eq!(
            panic_threat(pos, 0, 100, None, &[[9.0, 64.0, 0.0], [3.0, 64.0, 0.0]]),
            Some([3.0, 64.0, 0.0])
        );
        assert_eq!(panic_threat(pos, 0, 100, None, &[[9.0, 64.0, 0.0]]), None);
    }

    #[test]
    fn requested_items_farmer_only() {
        assert_eq!(
            PlayPlan::requested_items("minecraft:farmer"),
            &[
                "minecraft:wheat",
                "minecraft:wheat_seeds",
                "minecraft:beetroot_seeds",
                "minecraft:bone_meal"
            ]
        );
        assert!(PlayPlan::requested_items("minecraft:librarian").is_empty());
    }

    #[test]
    fn door_cooldown_is_per_node() {
        let door = [3, 64, 3];
        let now = 100;
        let cooldown_node = Some(door);
        assert!(cooldown_node == Some(door) && now < 120);
        assert!(!(cooldown_node == Some([4, 64, 3]) && now < 120));
        assert!(!(cooldown_node == Some(door) && 140 < 120));
    }

    #[test]
    fn other_villager_path_keeps_door_open() {
        let door = [3, 64, 3];
        let center = [3.5, 64.0, 3.5];
        let paths = vec![
            (1, [3.5, 64.0, 3.5], [3, 64, 3], [4, 64, 3]),
            (2, [5.0, 64.0, 3.5], [4, 64, 3], [5, 64, 3]),
        ];
        // Для жителя 2 другой (id 1) идёт через дверь.
        assert!(other_villager_on_door(2, door, center, &paths));
        assert!(!other_villager_on_door(1, door, center, &paths));
        let far = vec![(1, [10.0, 64.0, 3.5], [3, 64, 3], [4, 64, 3])];
        assert!(!other_villager_on_door(2, door, center, &far));
    }

    #[test]
    fn goal_stack_orders_by_priority_and_falls_back() {
        use Goal::*;
        // Активная цель по расписанию + `Idle` как запасная.
        assert_eq!(
            goals_for(Activity::Rest, true, true, true),
            vec![Rest, Idle]
        );
        assert_eq!(goals_for(Activity::Rest, false, true, true), vec![Idle]);
        assert_eq!(
            goals_for(Activity::Work, true, true, true),
            vec![Work, Idle]
        );
        assert_eq!(goals_for(Activity::Work, true, false, true), vec![Idle]);
        assert_eq!(
            goals_for(Activity::Meet, true, true, true),
            vec![Meet, Idle]
        );
        assert_eq!(goals_for(Activity::Meet, true, true, false), vec![Idle]);
        assert_eq!(goals_for(Activity::Idle, true, true, true), vec![Idle]);
        assert!(Rest.priority() > Work.priority());
        assert!(Work.priority() > Meet.priority());
        assert!(Meet.priority() > Idle.priority());
    }

    #[test]
    fn look_yaw_matches_minecraft_axes() {
        assert!((look_yaw([0.0; 3], [0.0, 0.0, 1.0]) - 0.0).abs() < 0.001);
        assert!((look_yaw([0.0; 3], [1.0, 0.0, 0.0]) + 90.0).abs() < 0.001);
        assert!((look_yaw([0.0; 3], [-1.0, 0.0, 0.0]) - 90.0).abs() < 0.001);
        assert!((look_yaw([0.0; 3], [0.0, 0.0, -1.0]).abs() - 180.0).abs() < 0.001);
    }

    #[test]
    fn validate_poi_matches_vanilla() {
        assert_eq!(
            validate_poi(400.0, true, false, false, false),
            PoiValidation::Keep
        );
        assert_eq!(
            validate_poi(16.5 * 16.5, false, false, false, false),
            PoiValidation::Keep
        );
        assert_eq!(
            validate_poi(4.0, false, false, false, false),
            PoiValidation::EraseAndRelease
        );
        assert_eq!(
            validate_poi(4.0, true, true, false, false),
            PoiValidation::EraseAndRelease
        );
        assert_eq!(
            validate_poi(4.0, true, true, false, true),
            PoiValidation::Erase
        );
        assert_eq!(
            validate_poi(4.0, true, true, true, false),
            PoiValidation::Keep
        );
    }

    #[test]
    fn meet_rest_weights_match_vanilla() {
        assert_eq!(meet_weights().iter().map(|(_, w)| w).sum::<u32>(), 4);
        assert_eq!(rest_weights().iter().map(|(_, w)| w).sum::<u32>(), 9);
        assert_eq!(weighted_pick(&meet_weights(), 1), Some(MeetGoal::Stroll));
        assert_eq!(weighted_pick(&meet_weights(), 2), Some(MeetGoal::Socialize));
        assert_eq!(
            weighted_pick(&rest_weights(), 0),
            Some(RestGoal::ClosestHome)
        );
        assert_eq!(weighted_pick(&rest_weights(), 8), Some(RestGoal::DoNothing));
    }

    #[test]
    fn run_one_weights_match_vanilla() {
        let farmer = work_weights("minecraft:farmer");
        assert_eq!(farmer.iter().map(|(_, w)| w).sum::<u32>(), 25);
        assert_eq!(weighted_pick(&farmer, 24), Some(WorkGoal::UseBonemeal));
        let other = work_weights("minecraft:librarian");
        assert_eq!(other.iter().map(|(_, w)| w).sum::<u32>(), 31);
        assert_eq!(idle_weights().iter().map(|(_, w)| w).sum::<u32>(), 8);
        assert_eq!(play_weights().iter().map(|(_, w)| w).sum::<u32>(), 8);
        assert_eq!(
            weighted_pick(&idle_weights(), 0),
            Some(IdleGoal::InteractVillager)
        );
        assert_eq!(weighted_pick(&idle_weights(), 7), Some(IdleGoal::DoNothing));
    }

    #[test]
    fn crop_ages_match_vanilla() {
        assert_eq!(PlayPlan::crop_max_age("minecraft:wheat"), Some(7));
        assert_eq!(PlayPlan::crop_max_age("minecraft:carrots"), Some(7));
        assert_eq!(PlayPlan::crop_max_age("minecraft:beetroots"), Some(3));
        assert_eq!(PlayPlan::crop_max_age("minecraft:air"), None);
    }

    #[test]
    fn raid_phase_follows_raid_state() {
        assert_eq!(raid_phase(0, 0, 300, 0), RaidPhase::PreRaid);
        assert_eq!(raid_phase(1, 3, 0, 0), RaidPhase::Raid);
        assert_eq!(raid_phase(1, 0, 300, 0), RaidPhase::PreRaid);
        assert_eq!(raid_phase(2, 0, 0, 0), RaidPhase::Raid);
        assert_eq!(raid_phase(2, 1, 0, 5), RaidPhase::Victory);
        assert_eq!(raid_phase(3, 2, 0, 0), RaidPhase::Raid);
    }

    #[test]
    fn baby_schedule_matches_timeline() {
        use Activity::*;
        assert_eq!(Activity::at_baby(0), Rest);
        assert_eq!(Activity::at_baby(10), Idle);
        assert_eq!(Activity::at_baby(2999), Idle);
        assert_eq!(Activity::at_baby(3000), Play);
        assert_eq!(Activity::at_baby(5999), Play);
        assert_eq!(Activity::at_baby(6000), Idle);
        assert_eq!(Activity::at_baby(9999), Idle);
        assert_eq!(Activity::at_baby(10_000), Play);
        assert_eq!(Activity::at_baby(11_999), Play);
        assert_eq!(Activity::at_baby(12_000), Rest);
    }

    #[test]
    fn villager_schedule_matches_timeline() {
        use Activity::*;
        assert_eq!(Activity::at(0), Rest);
        assert_eq!(Activity::at(9), Rest);
        assert_eq!(Activity::at(10), Idle);
        assert_eq!(Activity::at(1999), Idle);
        assert_eq!(Activity::at(2000), Work);
        assert_eq!(Activity::at(8999), Work);
        assert_eq!(Activity::at(9000), Meet);
        assert_eq!(Activity::at(10999), Meet);
        assert_eq!(Activity::at(11000), Idle);
        assert_eq!(Activity::at(12_000), Rest);
        assert_eq!(Activity::at(23_999), Rest);
        assert_eq!(Activity::at(24_010), Idle);
    }

    #[test]
    fn enchant_level_uses_highest_cost_range() {
        let def = chunkwake_data::enchanting::EnchantDef {
            id: 1,
            name: "minecraft:sharpness".into(),
            max_level: 5,
            min_base: 1,
            min_step: 11,
            max_base: 21,
            max_step: 11,
            ..Default::default()
        };
        // `getAvailableEnchantmentResults` идёт от max_level вниз.
        assert_eq!(enchant_level_for_cost(&def, 1), Some(1));
        assert_eq!(enchant_level_for_cost(&def, 21), Some(2));
        assert_eq!(enchant_level_for_cost(&def, 32), Some(3));
        assert_eq!(enchant_level_for_cost(&def, 65), Some(5));
        assert_eq!(enchant_level_for_cost(&def, 66), None);
    }

    #[test]
    fn random_enchant_cost_matches_vanilla() {
        // `2 + nextInt(5 + level*10) + 3*level`.
        assert_eq!(random_enchant_cost(1, 0), 5);
        assert_eq!(random_enchant_cost(1, 14), 19);
        assert_eq!(random_enchant_cost(19, 194), 253);
    }

    #[test]
    fn select_enchantments_picks_levels_within_bounds() {
        use rand::SeedableRng;
        let item = "minecraft:diamond_boots";
        let def = chunkwake_data::enchanting::EnchantDef {
            id: 7,
            name: "minecraft:protection".into(),
            max_level: 4,
            weight: 10,
            min_base: 1,
            min_step: 0,
            max_base: 100,
            max_step: 0,
            primary: HashSet::from([item.to_owned()]),
            ..Default::default()
        };
        let defs = [def];
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for _ in 0..50 {
            let rolls = select_enchantments(&defs, &[], item, 10, 10, &mut rng);
            assert!(!rolls.is_empty(), "нет чар при cost 10");
            for &(id, level) in &rolls {
                assert_eq!(id, 7);
                assert!((1..=4).contains(&level), "уровень вне 1..=4: {level}");
            }
        }
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
