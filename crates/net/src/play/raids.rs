//! Рейды (`net.minecraft.world.entity.raid.Raid`): Bad Omen у деревни
//! запускает волны разбойников; после последней волны герои получают
//! `hero_of_the_village`, статистику `raid_win` и достижение.
//!
//! Состав волн взят из ванильного `Raid.RaiderType`: базовые количества по
//! таблицам `spawnsPerWaveBeforeBonus`, плюс вероятностные добавки по
//! сложности (`getPotentialBonusSpawns`); при уровне Bad Omen II и выше
//! добавляется бонусная волна силой финальной. Ездовые разбойники на
//! раввагерах спавнятся отдельными мобами (верхом ездить сервер пока не
//! умеет), знамя капитана и лечение ведьмами не реализованы.

use super::*;
use crate::mobs::dist2;
use std::collections::{HashMap, HashSet};

/// `Raid.RAID_TIMEOUT_TICKS`.
const RAID_TIMEOUT_TICKS: i64 = 48_000;
/// `Raid.DEFAULT_PRE_RAID_TICKS`: зарядка бара и пауза между волнами.
const PRE_RAID_TICKS: i64 = 300;
/// `Raid.POST_RAID_TICK_LIMIT`.
const POST_RAID_TICK_LIMIT: i32 = 40;
/// `Raid.MAX_CELEBRATION_TICKS` — бар «победа» висит 30 секунд.
const MAX_CELEBRATION_TICKS: i32 = 600;
/// `Raid.HERO_OF_THE_VILLAGE_DURATION`.
const HERO_OF_THE_VILLAGE_DURATION: i32 = 48_000;

/// Виды разбойников (`Raid.RaiderType`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RaiderKind {
    Vindicator,
    Evoker,
    Pillager,
    Witch,
    Ravager,
}

impl RaiderKind {
    /// Значения из `spawnsPerWaveBeforeBonus` (индекс — номер волны).
    fn wave_counts(self) -> [i32; 8] {
        match self {
            RaiderKind::Vindicator => [0, 0, 2, 0, 1, 4, 2, 5],
            RaiderKind::Evoker => [0, 0, 0, 0, 0, 1, 1, 2],
            RaiderKind::Pillager => [0, 4, 3, 3, 4, 4, 4, 2],
            RaiderKind::Witch => [0, 0, 0, 0, 3, 0, 0, 1],
            RaiderKind::Ravager => [0, 0, 0, 1, 0, 1, 0, 2],
        }
    }

    fn species(self) -> crate::mobs::Species {
        match self {
            RaiderKind::Vindicator => crate::mobs::Species::Vindicator,
            RaiderKind::Evoker => crate::mobs::Species::Evoker,
            RaiderKind::Pillager => crate::mobs::Species::Pillager,
            RaiderKind::Witch => crate::mobs::Species::Witch,
            RaiderKind::Ravager => crate::mobs::Species::Ravager,
        }
    }
}

/// Порядок обхода `RaiderType.VALUES`.
const RAIDER_KINDS: [RaiderKind; 5] = [
    RaiderKind::Vindicator,
    RaiderKind::Evoker,
    RaiderKind::Pillager,
    RaiderKind::Witch,
    RaiderKind::Ravager,
];

/// Активный рейд (центр деревни, волна, живые разбойники).
pub(super) struct Raid {
    pub dim: Dimension,
    pub center: [f64; 3],
    /// `groupsSpawned`.
    pub groups: i32,
    /// `numGroups` (3/5/7 по сложности).
    pub num_groups: i32,
    /// `raidOmenLevel` (усиление Bad Omen + 1).
    pub omen_level: i32,
    /// UUID разбойников текущей волны.
    pub raiders: Vec<u128>,
    /// `raidCooldownTicks`: 300 до первой волны и между волнами.
    pub cooldown: i64,
    /// `ticksActive` (по нему же таймаут рейда).
    pub ticks: i64,
    /// `postRaidTicks`.
    pub post_raid: i32,
    /// `celebrationTicks` после победы/поражения.
    pub celebration: i32,
    /// `heroesOfTheVillage`: игроки, убившие разбойника.
    pub heroes: HashSet<Uuid>,
    /// UUID босс-бара (`Raid.raidBossBar`).
    bar: Uuid,
    /// Игроки, которым сейчас показан бар.
    bar_players: HashSet<Uuid>,
    /// Суммарное здоровье разбойников текущей волны (`totalHealth`).
    wave_health: f32,
    /// Последний отправленный прогресс.
    bar_progress: f32,
    /// Последнее показанное число оставшихся (для имени бара).
    bar_remaining: i32,
}

impl PlayPlan {
    /// `PoiManager.getInRange` по кроватям (`minecraft:home`).
    fn raid_bed_near(&self, origin: [i32; 3], radius: i32) -> bool {
        let (cx, cz) = (origin[0] >> 4, origin[2] >> 4);
        let reach = radius.div_euclid(16) + 1;
        let map = self.dim_state().pois.lock().unwrap();
        for ((x, z), list) in map.iter() {
            if (x - cx).abs() > reach || (z - cz).abs() > reach {
                continue;
            }
            for (pos, kind) in list {
                if *kind != "minecraft:home" {
                    continue;
                }
                let d = (pos[0] as i64 - origin[0] as i64).pow(2)
                    + (pos[1] as i64 - origin[1] as i64).pow(2)
                    + (pos[2] as i64 - origin[2] as i64).pow(2);
                if d <= (radius as i64).pow(2) {
                    return true;
                }
            }
        }
        false
    }

    /// UUID босс-бара рейда: уникальный для каждого запуска.
    fn raid_bar_uuid(&self) -> Uuid {
        let n = self.next_entity_id.fetch_add(1, Ordering::Relaxed) as u128;
        Uuid::from_u128(0x7261_6964_5a1d_4e70_8000_0000_0000_0000 ^ n)
    }

    /// `Raid.getNumGroups`: 3/5/7 волн, на мирной — 0.
    fn raid_num_groups(&self) -> i32 {
        match self.difficulty() {
            survival::Difficulty::Easy => 3,
            survival::Difficulty::Normal => 5,
            survival::Difficulty::Hard => 7,
            survival::Difficulty::Peaceful => 0,
        }
    }

    /// `Raid.updateBossbar`: доля живого здоровья текущей волны.
    fn raid_bar_progress(raid: &Raid, alive_health: f32) -> f32 {
        if raid.wave_health > 0.0 {
            (alive_health / raid.wave_health).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// `RaiderType` плюс `getPotentialBonusSpawns`.
    fn raid_group_counts(&self, group_number: i32, bonus_group: bool) -> Vec<(RaiderKind, i32)> {
        let difficulty = self.difficulty();
        let easy = difficulty == survival::Difficulty::Easy;
        let normal = difficulty == survival::Difficulty::Normal;
        let index = if bonus_group {
            self.raid_num_groups()
        } else {
            group_number
        }
        .clamp(0, 7) as usize;
        let next = |n: u32| -> i32 {
            if n == 0 {
                0
            } else {
                (rand::random::<u32>() % n) as i32
            }
        };
        let mut out = Vec::new();
        for kind in RAIDER_KINDS {
            let base = kind.wave_counts()[index];
            let extra = match kind {
                // `WITCH`: не на Easy, с 3-й волны кроме 4-й.
                RaiderKind::Witch => {
                    if !easy && group_number > 2 && group_number != 4 {
                        next(2)
                    } else {
                        0
                    }
                }
                // `VINDICATOR`/`PILLAGER`: Easy 0..1, Normal 0..1, Hard 0..2.
                RaiderKind::Vindicator | RaiderKind::Pillager => {
                    next(if normal || easy { 2 } else { 3 })
                }
                // `RAVAGER`: только в бонусной волне и не на Easy.
                RaiderKind::Ravager => {
                    if !easy && bonus_group {
                        next(2)
                    } else {
                        0
                    }
                }
                RaiderKind::Evoker => 0,
            };
            let count = base + extra;
            if count > 0 {
                out.push((kind, count));
            }
        }
        out
    }

    /// `Raid.createOrExtendRaid`: первый игрок с Bad Omen у деревни.
    fn raid_start(&self, uuid: Uuid, center: [f64; 3], omen_level: i32) {
        let num_groups = self.raid_num_groups();
        if num_groups == 0 {
            return;
        }
        let raid = Raid {
            dim: Dimension::Overworld,
            center,
            groups: 0,
            num_groups,
            omen_level,
            raiders: Vec::new(),
            cooldown: PRE_RAID_TICKS,
            ticks: 0,
            post_raid: 0,
            celebration: 0,
            heroes: HashSet::new(),
            bar: self.raid_bar_uuid(),
            bar_players: HashSet::new(),
            wave_health: 0.0,
            bar_progress: -1.0,
            bar_remaining: -1,
        };
        self.raids.lock().unwrap().push(raid);
        // `absorbRaidOmen`: статистика только до первой волны.
        self.award_stat(uuid, "minecraft:custom", "minecraft:raid_trigger", 1);
        // `Raid` забирает эффект: игрок получает Bad Omen только один раз.
        self.push_control(
            uuid,
            Control::ClearEffect {
                id: None,
                command: false,
            },
        );
    }

    /// Спавн одной волны: `Raid.spawnGroup`.
    ///
    /// Возвращает UUID разбойников и суммарное здоровье волны.
    fn raid_spawn_group(
        &self,
        center: [f64; 3],
        group_number: i32,
        bonus_group: bool,
    ) -> (Vec<u128>, f32) {
        let counts = self.raid_group_counts(group_number, bonus_group);
        let total: i32 = counts.iter().map(|(_, n)| n).sum();
        if total == 0 {
            return (Vec::new(), 0.0);
        }
        let (x, z) = (center[0].floor() as i32, center[2].floor() as i32);
        let base = center[1].floor() as i32;
        let ground = (0..8)
            .rev()
            .find(|dy| {
                self.block([x, base + dy, z])
                    .is_some_and(|s| !self.state_name(s).ends_with(":air"))
            })
            .map_or(base, |dy| base + dy + 1);
        let mut out = Vec::new();
        let mut health = 0.0;
        let mut index = 0i32;
        for (kind, count) in counts {
            for _ in 0..count {
                let angle = f64::from(index) * std::f64::consts::TAU / f64::from(total);
                index += 1;
                let pos = [
                    center[0] + angle.cos() * 6.0,
                    f64::from(ground),
                    center[2] + angle.sin() * 6.0,
                ];
                if let Some(id) = self.spawn_mob(kind.species(), pos, 0.0f32, false, Some("RAID"))
                    && let Some(entry) = self.sim().mobs.lock().unwrap().get(&id)
                {
                    out.push(entry.mob.uuid);
                    health += entry.mob.health;
                }
            }
        }
        (out, health)
    }

    /// `Raid.hasMoreWaves`: бонусная волна при Bad Omen II и выше.
    fn raid_has_more_waves(raid: &Raid) -> bool {
        if raid.omen_level > 1 {
            raid.groups <= raid.num_groups
        } else {
            raid.groups < raid.num_groups
        }
    }

    /// Игрок убил разбойника: `Raider.die` → `addHeroOfTheVillage`.
    pub(super) fn raid_hero(&self, player: Uuid, mob: u128) {
        for raid in self.raids.lock().unwrap().iter_mut() {
            if raid.raiders.contains(&mob) {
                raid.heroes.insert(player);
            }
        }
    }

    /// Пересчёт видимых игроков и рассылка пакетов босс-бара.
    #[allow(clippy::type_complexity)]
    fn raid_bossbar(
        &self,
        raid: &mut Raid,
        players: &[(Uuid, [f64; 3], Dimension)],
        packets: &mut Vec<(Uuid, Vec<u8>)>,
    ) {
        let in_range: HashSet<Uuid> = players
            .iter()
            .filter(|(_, pos, dim)| *dim == raid.dim && dist2(*pos, raid.center) <= 32.0 * 32.0)
            .map(|(uuid, _, _)| *uuid)
            .collect();
        let lost: Vec<Uuid> = raid.bar_players.difference(&in_range).copied().collect();
        for uuid in lost {
            raid.bar_players.remove(&uuid);
            packets.push((
                uuid,
                chunkwake_protocol::play::boss_event_remove(self.ids.boss_event, raid.bar),
            ));
        }
        let progress = raid.bar_progress.max(0.0);
        for uuid in in_range {
            if raid.bar_players.insert(uuid) {
                packets.push((
                    uuid,
                    chunkwake_protocol::play::boss_event_add(
                        self.ids.boss_event,
                        raid.bar,
                        &serde_json::json!({"translate": "event.minecraft.raid"}),
                        progress,
                        0,
                        2,
                    ),
                ));
            }
        }
    }

    /// Тик рейдов: старт по Bad Omen у кроватей, волны, босс-бар, победа.
    pub(super) fn raid_tick(&self) {
        if self.difficulty() == survival::Difficulty::Peaceful {
            // Мирная сложность снимает рейды вместе с их барами.
            let old: Vec<Raid> = std::mem::take(&mut *self.raids.lock().unwrap());
            for raid in old {
                for uuid in raid.bar_players {
                    self.send_to(
                        uuid,
                        chunkwake_protocol::play::boss_event_remove(self.ids.boss_event, raid.bar),
                    );
                }
            }
            return;
        }
        with_dim(Dimension::Overworld, || {
            let Some(bad_omen) = self.survival.effects.get("minecraft:bad_omen").copied() else {
                return;
            };
            // 1) Старт новых рейдов.
            let candidates: Vec<(Uuid, [f64; 3], u8)> = self
                .live
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, l)| l.dimension == Dimension::Overworld)
                .filter_map(|(uuid, l)| {
                    l.effects
                        .iter()
                        .find(|e| e.id == bad_omen)
                        .map(|e| (*uuid, l.pos, e.amplifier))
                })
                .collect();
            for (uuid, pos, amplifier) in candidates {
                let feet = [
                    pos[0].floor() as i32,
                    pos[1].floor() as i32,
                    pos[2].floor() as i32,
                ];
                if !self.raid_bed_near(feet, 32) {
                    continue;
                }
                if self
                    .raids
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|r| dist2(r.center, pos) < 96.0 * 96.0)
                {
                    continue;
                }
                // `Raid.absorbRaidOmen`: усиление + 1, максимум 5.
                let level = (i32::from(amplifier) + 1).clamp(1, 5);
                self.raid_start(uuid, pos, level);
            }

            // 2) Прогресс волн и босс-бар.
            let (alive, raider_health): (HashSet<u128>, HashMap<u128, f32>) = {
                let mobs = self.sim().mobs.lock().unwrap();
                let mut alive = HashSet::new();
                let mut health = HashMap::new();
                for entry in mobs.values() {
                    if entry.mob.alive() {
                        alive.insert(entry.mob.uuid);
                        health.insert(entry.mob.uuid, entry.mob.health);
                    }
                }
                (alive, health)
            };
            let players: Vec<(Uuid, [f64; 3], Dimension)> = self
                .live
                .lock()
                .unwrap()
                .iter()
                .map(|(uuid, l)| (*uuid, l.pos, l.dimension))
                .collect();
            let mut spawn = Vec::new();
            let mut heroes = Vec::new();
            let mut packets: Vec<(Uuid, Vec<u8>)> = Vec::new();
            {
                let mut raids = self.raids.lock().unwrap();
                let mut i = 0;
                while i < raids.len() {
                    let raid = &mut raids[i];
                    raid.ticks += 1;
                    if raid.ticks >= RAID_TIMEOUT_TICKS {
                        for uuid in raid.bar_players.drain() {
                            packets.push((
                                uuid,
                                chunkwake_protocol::play::boss_event_remove(
                                    self.ids.boss_event,
                                    raid.bar,
                                ),
                            ));
                        }
                        raids.remove(i);
                        continue;
                    }
                    // `Raid.updateRaiders`: разбойники дальше 112 блоков
                    // выбывают из рейда (мобы при этом остаются жить).
                    let alive_health: f32 = raid
                        .raiders
                        .iter()
                        .filter_map(|u| raider_health.get(u).copied())
                        .sum();
                    let alive_count =
                        raid.raiders.iter().filter(|u| alive.contains(u)).count() as i32;

                    if raid.celebration > 0 {
                        // Бар «победа» ещё 600 тиков, затем снять.
                        raid.celebration += 1;
                        if raid.celebration >= MAX_CELEBRATION_TICKS {
                            for uuid in raid.bar_players.drain() {
                                packets.push((
                                    uuid,
                                    chunkwake_protocol::play::boss_event_remove(
                                        self.ids.boss_event,
                                        raid.bar,
                                    ),
                                ));
                            }
                            raids.remove(i);
                            continue;
                        }
                        Self::raid_bossbar(self, raid, &players, &mut packets);
                        if raid.celebration % 20 == 0 {
                            for uuid in &raid.bar_players {
                                packets.push((
                                    *uuid,
                                    chunkwake_protocol::play::boss_event_progress(
                                        self.ids.boss_event,
                                        raid.bar,
                                        0.0,
                                    ),
                                ));
                            }
                        }
                        i += 1;
                        continue;
                    }

                    // Пауза/зарядка бара: 300 тиков до первой волны и между.
                    if alive_count == 0 && raid.cooldown > 0 {
                        raid.cooldown -= 1;
                        raid.bar_progress = ((PRE_RAID_TICKS - raid.cooldown) as f32
                            / PRE_RAID_TICKS as f32)
                            .clamp(0.0, 1.0);
                    } else if alive_count == 0
                        && raid.cooldown == 0
                        && raid.groups > 0
                        && Self::raid_has_more_waves(raid)
                    {
                        // Волна зачищена — снова заряжаем бар.
                        raid.cooldown = PRE_RAID_TICKS;
                        raid.bar_progress = 0.0;
                        raid.bar_remaining = -1;
                        for uuid in &raid.bar_players {
                            packets.push((
                                *uuid,
                                chunkwake_protocol::play::boss_event_name(
                                    self.ids.boss_event,
                                    raid.bar,
                                    &serde_json::json!({"translate": "event.minecraft.raid"}),
                                ),
                            ));
                        }
                    } else {
                        raid.bar_progress = Self::raid_bar_progress(raid, alive_health);
                    }

                    // `shouldSpawnGroup`: пора выпускать следующую волну.
                    if raid.cooldown == 0
                        && alive_count == 0
                        && (raid.groups < raid.num_groups
                            || (raid.groups == raid.num_groups && raid.omen_level > 1))
                    {
                        let bonus_group = raid.groups >= raid.num_groups;
                        spawn.push((i, raid.center, raid.groups + 1, bonus_group));
                    }

                    // Победа: финальная волна зачищена, 40 тиков паузы.
                    let final_group = if raid.omen_level > 1 {
                        raid.groups > raid.num_groups
                    } else {
                        raid.groups >= raid.num_groups
                    };
                    if final_group && alive_count == 0 {
                        if raid.post_raid < POST_RAID_TICK_LIMIT {
                            raid.post_raid += 1;
                        } else {
                            let level = raid.omen_level;
                            for hero in raid.heroes.clone() {
                                heroes.push((hero, level));
                            }
                            raid.bar_progress = 0.0;
                            raid.celebration = 1;
                            for uuid in &raid.bar_players {
                                packets.push((
                                    *uuid,
                                    chunkwake_protocol::play::boss_event_name(
                                        self.ids.boss_event,
                                        raid.bar,
                                        &serde_json::json!({
                                            "translate": "event.minecraft.raid.victory.full"
                                        }),
                                    ),
                                ));
                                packets.push((
                                    *uuid,
                                    chunkwake_protocol::play::boss_event_progress(
                                        self.ids.boss_event,
                                        raid.bar,
                                        0.0,
                                    ),
                                ));
                            }
                            i += 1;
                            continue;
                        }
                    }

                    // Имя бара при малом числе разбойников.
                    let remaining = if alive_count > 0 && alive_count <= 2 {
                        alive_count
                    } else {
                        -1
                    };
                    if remaining != raid.bar_remaining {
                        raid.bar_remaining = remaining;
                        let name = if remaining > 0 {
                            serde_json::json!({
                                "translate": "event.minecraft.raid",
                                "extra": [{
                                    "text": " - ",
                                }, {
                                    "translate": "event.minecraft.raid.raiders_remaining",
                                    "with": [{"text": remaining.to_string()}],
                                }],
                            })
                        } else {
                            serde_json::json!({"translate": "event.minecraft.raid"})
                        };
                        for uuid in &raid.bar_players {
                            packets.push((
                                *uuid,
                                chunkwake_protocol::play::boss_event_name(
                                    self.ids.boss_event,
                                    raid.bar,
                                    &name,
                                ),
                            ));
                        }
                    }

                    Self::raid_bossbar(self, raid, &players, &mut packets);
                    let progress = raid.bar_progress;
                    for uuid in &raid.bar_players {
                        packets.push((
                            *uuid,
                            chunkwake_protocol::play::boss_event_progress(
                                self.ids.boss_event,
                                raid.bar,
                                progress,
                            ),
                        ));
                    }
                    i += 1;
                }
            }
            // `Raid.spawnGroup` — уже без удержания замка.
            let spawn: Vec<_> = spawn
                .into_iter()
                .filter(|(index, ..)| *index < self.raids.lock().unwrap().len())
                .collect();
            for (index, center, group, bonus) in spawn {
                let (raiders, health) = self.raid_spawn_group(center, group, bonus);
                if let Some(raid) = self.raids.lock().unwrap().get_mut(index) {
                    raid.groups = group;
                    raid.raiders = raiders;
                    raid.wave_health = health;
                    raid.bar_progress = 1.0;
                }
                self.play_sound(
                    None,
                    "minecraft:event.raid.horn",
                    crate::sounds::NEUTRAL,
                    center,
                    64.0,
                    1.0,
                );
            }
            for (uuid, packet) in packets {
                self.send_to(uuid, packet);
            }
            // 3) Победа: эффект, статистика и достижение героям рейда.
            for (uuid, level) in heroes {
                if let Some(&effect_id) = self.survival.effects.get("minecraft:hero_of_the_village")
                {
                    let mut effect = survival::Effect::plain(
                        effect_id,
                        (level - 1).clamp(0, 4) as u8,
                        HERO_OF_THE_VILLAGE_DURATION,
                    );
                    effect.particles = false;
                    self.push_control(
                        uuid,
                        Control::Effect {
                            effect,
                            force: false,
                            command: false,
                        },
                    );
                }
                self.award_stat(uuid, "minecraft:custom", "minecraft:raid_win", 1);
                self.advancement_hero_of_the_village(uuid);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Базовые количества волн совпадают с `Raid.RaiderType`.
    #[test]
    fn raid_wave_counts_match_vanilla() {
        let counts = |kind: RaiderKind, wave: usize| kind.wave_counts()[wave];
        // Easy (3 волны): 4 пиллера, 2 виндикатора + 3 пиллера, 3 пиллера + раввагер.
        assert_eq!(counts(RaiderKind::Pillager, 1), 4);
        assert_eq!(counts(RaiderKind::Vindicator, 2), 2);
        assert_eq!(counts(RaiderKind::Pillager, 3), 3);
        assert_eq!(counts(RaiderKind::Ravager, 3), 1);
        // Normal (5 волн): ведьмы на 4-й, эвокер на 5-й.
        assert_eq!(counts(RaiderKind::Witch, 4), 3);
        assert_eq!(counts(RaiderKind::Evoker, 5), 1);
        // Hard (7 волн): финальная волна 5 виндикаторов, 2 раввагера.
        assert_eq!(counts(RaiderKind::Vindicator, 7), 5);
        assert_eq!(counts(RaiderKind::Ravager, 7), 2);
        assert_eq!(counts(RaiderKind::Witch, 7), 1);
    }

    /// Полоса рейда показывает долю живого здоровья текущей волны.
    #[test]
    fn raid_bar_progress_is_health_fraction() {
        let mut r = Raid {
            dim: Dimension::Overworld,
            center: [0.0; 3],
            groups: 1,
            num_groups: 3,
            omen_level: 1,
            raiders: Vec::new(),
            cooldown: 0,
            ticks: 0,
            post_raid: 0,
            celebration: 0,
            heroes: HashSet::new(),
            bar: Uuid::nil(),
            bar_players: HashSet::new(),
            wave_health: 36.0,
            bar_progress: -1.0,
            bar_remaining: -1,
        };
        assert!((PlayPlan::raid_bar_progress(&r, 36.0) - 1.0).abs() < 1e-6);
        assert!((PlayPlan::raid_bar_progress(&r, 18.0) - 0.5).abs() < 1e-6);
        assert_eq!(PlayPlan::raid_bar_progress(&r, 0.0), 0.0);
        r.wave_health = 0.0;
        assert_eq!(PlayPlan::raid_bar_progress(&r, 10.0), 0.0);
    }
}
