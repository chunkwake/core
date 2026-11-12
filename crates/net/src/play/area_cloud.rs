//! Облако оседающего зелья (`AreaEffectCloud`).
//!
//! При попадании `lingering_potion` создаётся облако радиусом 3 блока на
//! 600 тиков: 10 тиков ожидания, затем эффекты применяются каждые 5 тиков,
//! повторно одной сущности — не чаще раза в 20 тиков. Радиус плавно
//! уменьшается до 0.5 и облако исчезает.

use super::*;
use chunkwake_world::nbt::{Compound, Tag};
use play::Meta;

/// `AreaEffectCloud.DEFAULT_DURATION` / `DEFAULT_WAIT_TIME` для зелья.
const DURATION: i32 = 600;
const WAIT: i32 = 10;
/// `TIME_BETWEEN_APPLICATIONS`.
const APPLY_EVERY: i32 = 5;
/// `DEFAULT_REAPPLICATION_DELAY`.
const REAPPLY: i32 = 20;
const DEFAULT_RADIUS: f32 = 3.0;
const MINIMAL_RADIUS: f32 = 0.5;
const MAXIMAL_RADIUS: f32 = 32.0;
const HEIGHT: f64 = 0.5;
/// `ThrownLingeringPotion.onHitAsPotion`: `setRadiusOnUse(-0.5)`, каждое
/// срабатывание на сущности уменьшает облако.
const RADIUS_ON_USE: f32 = -0.5;
/// Конструктор `AreaEffectCloud`: у summoned-облака `duration = -1`,
/// `waitTime = 20`, `radiusPerTick = 0`, `radiusOnUse = 0`.
const SUMMON_DURATION: i32 = -1;
const SUMMON_WAIT: i32 = 20;
/// Индексы `SynchedEntityData` для `AreaEffectCloud`.
const META_RADIUS: u8 = 8;
const META_WAITING: u8 = 9;

#[derive(Clone, Debug)]
pub(super) struct Cloud {
    pub pos: [f64; 3],
    pub effects: Vec<survival::Effect>,
    pub radius: f32,
    pub radius_per_tick: f32,
    /// `AreaEffectCloud.duration`: `-1` без ограничения по возрасту.
    pub duration: i32,
    /// `AreaEffectCloud.waitTime`: сколько тиков облако ждёт.
    pub wait: i32,
    /// `AreaEffectCloud.reapplicationDelay`: пауза между эффектами.
    pub reapply: i32,
    pub radius_on_use: f32,
    pub duration_on_use: i32,
    pub age: i32,
    /// Ключ сущности (UUID игрока или ID моба) -> тик, до которого повторно
    /// эффект не накладывается.
    pub victims: HashMap<u128, i32>,
}

impl Cloud {
    /// Облако оседающего зелья (`ThrownLingeringPotion.onHitAsPotion`).
    fn lingering(pos: [f64; 3], effects: Vec<survival::Effect>) -> Self {
        Self {
            pos,
            effects,
            radius: DEFAULT_RADIUS,
            radius_per_tick: -DEFAULT_RADIUS / DURATION as f32,
            duration: DURATION,
            wait: WAIT,
            reapply: REAPPLY,
            radius_on_use: RADIUS_ON_USE,
            duration_on_use: 0,
            age: 0,
            victims: HashMap::new(),
        }
    }

    /// `/summon minecraft:area_effect_cloud`: ванильные значения конструктора.
    fn summoned(pos: [f64; 3]) -> Self {
        Self {
            pos,
            effects: Vec::new(),
            radius: DEFAULT_RADIUS,
            radius_per_tick: 0.0,
            duration: SUMMON_DURATION,
            wait: SUMMON_WAIT,
            reapply: REAPPLY,
            radius_on_use: 0.0,
            duration_on_use: 0,
            age: 0,
            victims: HashMap::new(),
        }
    }

    /// `Level.getEntitiesOfClass` по AABB облака плюс проверка
    /// `dx² + dz² <= radius²` (как в `serverTick`).
    fn contains(&self, feet: [f64; 3], size: (f64, f64)) -> bool {
        let r = f64::from(self.radius);
        let (dx, dz) = (feet[0] - self.pos[0], feet[2] - self.pos[2]);
        if dx.abs() > r + size.0 / 2.0 || dz.abs() > r + size.0 / 2.0 {
            return false;
        }
        if feet[1] >= self.pos[1] + HEIGHT || feet[1] + size.1 <= self.pos[1] {
            return false;
        }
        dx * dx + dz * dz <= r * r
    }
}

/// `MobEffectInstance.withScaledDuration`: `max(1, floor(d · scale))`,
/// у бесконечного эффекта (`-1`) длительность не трогается.
fn scaled_duration(duration: i32, scale: f32) -> i32 {
    if duration == -1 {
        return -1;
    }
    ((duration as f32 * scale).floor() as i32).max(1)
}

impl PlayPlan {
    /// `ThrownLingeringPotion.onHitAsPotion`: облако в точке попадания.
    pub(super) fn spawn_lingering(&self, extra: u32, pos: [f64; 3]) {
        let effects = self.potion_effects(extra);
        if effects.is_empty() {
            return;
        }
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        self.spawn_cloud(id, Cloud::lingering(pos, effects));
    }

    /// `/summon minecraft:area_effect_cloud`: NBT как `AreaEffectCloud.load`.
    pub(super) fn summon_cloud(&self, pos: [f64; 3], nbt: Option<&Compound>) -> Option<i32> {
        let mut cloud = Cloud::summoned(pos);
        if let Some(nbt) = nbt {
            let number = crate::play::commands::snbt::number;
            if let Some(v) = nbt.get("Radius").and_then(number) {
                cloud.radius = (v as f32).clamp(0.0, MAXIMAL_RADIUS);
            }
            if let Some(v) = nbt.get("RadiusPerTick").and_then(number) {
                cloud.radius_per_tick = v as f32;
            }
            if let Some(v) = nbt.get("RadiusOnUse").and_then(number) {
                cloud.radius_on_use = v as f32;
            }
            if let Some(v) = nbt.get("Duration").and_then(number) {
                cloud.duration = v as i32;
            }
            if let Some(v) = nbt.get("WaitTime").and_then(number) {
                cloud.wait = v as i32;
            }
            if let Some(v) = nbt.get("ReapplicationDelay").and_then(number) {
                cloud.reapply = v as i32;
            }
            if let Some(v) = nbt.get("DurationOnUse").and_then(number) {
                cloud.duration_on_use = v as i32;
            }
            if let Some(v) = nbt.get("Age").and_then(number) {
                cloud.age = v as i32;
            }
            if let Some(contents) = nbt.get("potion_contents") {
                cloud.effects = self.cloud_effects(contents);
            }
        }
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        self.spawn_cloud(id, cloud);
        Some(id)
    }

    /// `potion_contents` в SNBT: `potion` и `custom_effects`; длительности
    /// масштабируются `potion_duration_scale` (`PotionContents.forEachEffect`).
    fn cloud_effects(&self, tag: &Tag) -> Vec<survival::Effect> {
        let Tag::Compound(contents) = tag else {
            return Vec::new();
        };
        let scale = contents
            .get("potion_duration_scale")
            .and_then(crate::play::commands::snbt::number)
            .map_or(1.0, |v| v as f32);
        let mut effects = Vec::new();
        if let Some(Tag::String(name)) = contents.get("potion") {
            let name = name.strip_prefix("minecraft:").unwrap_or(name);
            for effect in crate::potions::effects(name) {
                let Some(&id) = self.survival.effects.get(effect.effect) else {
                    continue;
                };
                effects.push(survival::Effect::plain(
                    id,
                    effect.amplifier,
                    scaled_duration(effect.duration, scale),
                ));
            }
        }
        if let Some(Tag::List(list)) = contents.get("custom_effects") {
            for tag in list {
                let Tag::Compound(e) = tag else { continue };
                let Some(Tag::String(name)) = e.get("id") else {
                    continue;
                };
                let name = name.strip_prefix("minecraft:").unwrap_or(name);
                let Some(&id) = self.survival.effects.get(name) else {
                    continue;
                };
                let amplifier = match e.get("amplifier") {
                    Some(Tag::Byte(v)) => *v as u8,
                    _ => 0,
                };
                let duration = match e.get("duration") {
                    Some(Tag::Int(v)) => *v,
                    _ => 0,
                };
                if duration == 0 {
                    continue;
                }
                effects.push(survival::Effect {
                    id,
                    amplifier,
                    duration: scaled_duration(duration, scale),
                    ambient: matches!(e.get("ambient"), Some(Tag::Byte(v)) if *v != 0),
                    particles: !matches!(e.get("show_particles"), Some(Tag::Byte(v)) if *v == 0),
                    show_icon: !matches!(e.get("show_icon"), Some(Tag::Byte(v)) if *v == 0),
                    age: 0,
                });
            }
        }
        effects
    }

    fn spawn_cloud(&self, id: i32, cloud: Cloud) {
        let pos = cloud.pos;
        let radius = cloud.radius;
        let waiting = cloud.age < cloud.wait;
        self.sim().clouds.lock().unwrap().insert(id, cloud);
        self.mark_work();
        self.hub.send(
            None,
            play::add_entity_moving(
                self.ids.add_entity,
                id,
                Uuid::from_u128(rand::random()),
                self.survival.area_cloud_entity_type,
                pos,
                [0.0; 3],
                0.0,
                0.0,
                0,
            ),
        );
        self.hub.send(
            None,
            play::entity_meta(
                self.ids.set_entity_data,
                id,
                &[
                    (META_RADIUS, Meta::Float(radius)),
                    (META_WAITING, Meta::Bool(waiting)),
                ],
            ),
        );
    }

    pub(super) fn tick_area_clouds(&self) {
        if self.sim().clouds.lock().unwrap().is_empty() {
            return;
        }
        let players: Vec<(Uuid, [f64; 3])> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.collects)
            .map(|p| (p.entry.uuid, p.pos))
            .collect();
        let mobs: Vec<(i32, [f64; 3], (f64, f64))> = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, e)| e.mob.alive())
            .map(|(&id, e)| (id, e.mob.pos, e.mob.size()))
            .collect();
        let mut meta = Vec::new();
        let mut gone = Vec::new();
        let mut player_hits: Vec<(Uuid, survival::Effect)> = Vec::new();
        let mut mob_hits: Vec<(i32, survival::Effect)> = Vec::new();
        {
            let mut clouds = self.sim().clouds.lock().unwrap();
            for (&id, cloud) in clouds.iter_mut() {
                cloud.age += 1;
                // `tickCount >= waitTime + duration` (у summoned `duration = -1`).
                if cloud.age >= cloud.wait.saturating_add(cloud.duration) {
                    gone.push(id);
                    continue;
                }
                let waiting = cloud.age < cloud.wait;
                if cloud.age == cloud.wait {
                    meta.push(play::entity_meta(
                        self.ids.set_entity_data,
                        id,
                        &[(META_WAITING, Meta::Bool(false))],
                    ));
                }
                if waiting {
                    continue;
                }
                // `radiusPerTick`: без изменения радиуса и порог 0.5 не проверяется.
                if cloud.radius_per_tick != 0.0 {
                    cloud.radius =
                        (cloud.radius + cloud.radius_per_tick).clamp(0.0, MAXIMAL_RADIUS);
                    if cloud.radius < MINIMAL_RADIUS {
                        gone.push(id);
                        continue;
                    }
                    if cloud.age % 20 == 0 {
                        meta.push(play::entity_meta(
                            self.ids.set_entity_data,
                            id,
                            &[(META_RADIUS, Meta::Float(cloud.radius))],
                        ));
                    }
                }
                if cloud.age % APPLY_EVERY != 0 {
                    continue;
                }
                cloud.victims.retain(|_, until| *until > cloud.age);
                if cloud.effects.is_empty() {
                    continue;
                }
                let mut used = false;
                for &(uuid, feet) in &players {
                    if !cloud.contains(feet, crate::mobs::PLAYER_SIZE) {
                        continue;
                    }
                    let key = uuid.as_u128();
                    if cloud.victims.contains_key(&key) {
                        continue;
                    }
                    cloud.victims.insert(key, cloud.age + cloud.reapply);
                    for effect in &cloud.effects {
                        player_hits.push((uuid, *effect));
                    }
                    used = true;
                }
                for &(mob_id, feet, size) in &mobs {
                    if !cloud.contains(feet, size) {
                        continue;
                    }
                    // Мобы не имеют UUID игрока: старший бит отделяет ключи.
                    let key = mob_id as u128 | 1 << 127;
                    if cloud.victims.contains_key(&key) {
                        continue;
                    }
                    cloud.victims.insert(key, cloud.age + cloud.reapply);
                    for effect in &cloud.effects {
                        mob_hits.push((mob_id, *effect));
                    }
                    used = true;
                }
                // Радиус и срок меняются один раз за применение, а не на цель.
                if used {
                    if cloud.radius_on_use != 0.0 {
                        cloud.radius =
                            (cloud.radius + cloud.radius_on_use).clamp(0.0, MAXIMAL_RADIUS);
                        if cloud.radius < MINIMAL_RADIUS {
                            gone.push(id);
                            continue;
                        }
                    }
                    if cloud.duration_on_use != 0 && cloud.duration != -1 {
                        cloud.duration += cloud.duration_on_use;
                        if cloud.duration <= 0 {
                            gone.push(id);
                        }
                    }
                }
            }
        }
        for packet in meta {
            self.hub.send(None, packet);
        }
        if !gone.is_empty() {
            self.hub
                .send(None, play::remove_entities(self.ids.remove_entities, &gone));
        }
        for (uuid, effect) in player_hits {
            let Some(name) = self.effect_name(effect.id) else {
                continue;
            };
            match survival::instant_effect(name, effect.amplifier) {
                survival::Instant::Heal(amount) => {
                    self.push_control(
                        uuid,
                        Control::Heal(crate::potions::splash_instant_amount(amount, 0.5)),
                    );
                }
                survival::Instant::Harm(amount) => {
                    self.push_control(
                        uuid,
                        Control::Hurt(
                            crate::potions::splash_instant_amount(amount, 0.5),
                            Cause::Magic,
                            None,
                            0.0,
                        ),
                    );
                }
                survival::Instant::Eat(..) => {}
                survival::Instant::None => {
                    // `AreaEffectCloud` накладывает эффект без порога длительности,
                    // в отличие от splash-зелья.
                    self.push_control(
                        uuid,
                        Control::Effect {
                            effect,
                            force: false,
                            command: false,
                        },
                    );
                }
            }
        }
        for (id, effect) in mob_hits {
            let Some(name) = self.effect_name(effect.id) else {
                continue;
            };
            let instant = survival::instant_effect(name, effect.amplifier);
            self.mob_potion_effect(id, effect, instant, 0.5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_aabb_and_radius() {
        let mut cloud = Cloud::lingering([0.0, 64.0, 0.0], Vec::new());
        // Игрок 0.6×1.8 в центре и у края круга радиуса 3.
        assert!(cloud.contains([0.0, 64.0, 0.0], crate::mobs::PLAYER_SIZE));
        assert!(cloud.contains([2.9, 64.0, 0.0], crate::mobs::PLAYER_SIZE));
        // Угол AABB вне круга, как `dx² + dz² <= radius²`.
        assert!(!cloud.contains([2.9, 64.0, 2.9], crate::mobs::PLAYER_SIZE));
        assert!(!cloud.contains([3.2, 64.0, 0.0], crate::mobs::PLAYER_SIZE));
        // Высота облака 0.5: стоящий сверху уже не задет.
        assert!(!cloud.contains([0.0, 64.5, 0.0], crate::mobs::PLAYER_SIZE));
        // Радиус уменьшается на 3/600 за тик; порог 0.5 достигается раньше
        // срока, как `AreaEffectCloud.serverTick`.
        for _ in 0..600 {
            cloud.radius += cloud.radius_per_tick;
        }
        assert!(cloud.radius < MINIMAL_RADIUS);
        assert_eq!((DURATION, WAIT, REAPPLY), (600, 10, 20));
        assert_eq!(RADIUS_ON_USE, -0.5);
    }

    #[test]
    fn summoned_cloud_uses_vanilla_defaults() {
        let cloud = Cloud::summoned([0.0, 64.0, 0.0]);
        assert_eq!((cloud.duration, cloud.wait), (SUMMON_DURATION, SUMMON_WAIT));
        assert_eq!((cloud.duration, cloud.wait), (-1, 20));
        assert_eq!(cloud.reapply, REAPPLY);
        assert_eq!((cloud.radius, cloud.radius_per_tick), (3.0, 0.0));
        assert_eq!((cloud.radius_on_use, cloud.duration_on_use), (0.0, 0));
        assert!(cloud.effects.is_empty());
        // Радиус 3: точка в круге, но за его пределами — уже нет.
        assert!(cloud.contains([2.6, 64.0, 0.0], crate::mobs::PLAYER_SIZE));
        assert!(!cloud.contains([3.2, 64.0, 0.0], crate::mobs::PLAYER_SIZE));
    }

    #[test]
    fn effect_durations_scale_like_vanilla() {
        assert_eq!(scaled_duration(200, 0.5), 100);
        assert_eq!(scaled_duration(1, 0.5), 1);
        assert_eq!(scaled_duration(-1, 0.5), -1);
        assert_eq!(scaled_duration(600, 1.0), 600);
    }
}
