//! Маяк (`BeaconBlock`, `BeaconBlockEntity`, `BeaconMenu`).
//!
//! Пирамида из блоков `#minecraft:beacon_base_blocks` даёт уровень 1–4
//! (основание 3×3, 5×5, 7×7, 9×9). Эффекты выдаются игрокам в радиусе
//! `10 + 10·уровень` раз в 80 тиков на `(9 + 2·уровень)·20` тиков. На 4-м
//! уровне можно выбрать второй эффект; если он совпадает с основным,
//! амплитуда основного повышается до II. Регенерация доступна только второй.
//!
//! В NBT хранятся `primary_effect`, `secondary_effect`, `Levels` и
//! `CustomName`; клиенту уровень уходит тем же `block_entity_data`, по нему
//! он рисует луч. Оплата — предмет из `#minecraft:beacon_payment_items`,
//! ровно один, тратится при успешном `set_beacon`.

use super::*;

/// `BeaconBlockEntity.BEACON_EFFECTS`: уровень способности → эффекты.
const EFFECTS: [&[&str]; 4] = [
    &["minecraft:speed", "minecraft:haste"],
    &["minecraft:resistance", "minecraft:jump_boost"],
    &["minecraft:strength"],
    &["minecraft:regeneration"],
];

/// `BeaconBlockEntity.updateBase` проверяется раз в 80 тиков.
const CHECK_INTERVAL: i64 = 80;

#[derive(Clone, Debug, Default)]
pub(crate) struct Beacon {
    pub levels: i32,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub custom_name: Option<Tag>,
}

/// `getRequiredLevelsFor`: минимальный уровень пирамиды для эффекта.
fn required_level(name: &str) -> Option<i32> {
    EFFECTS
        .iter()
        .position(|list| list.contains(&name))
        .map(|i| i as i32 + 1)
}

/// `BeaconBlockEntity.validateEffects`.
pub(crate) fn valid_effects(primary: Option<&str>, secondary: Option<&str>, levels: i32) -> bool {
    if secondary.is_some() && levels < 4 {
        return false;
    }
    let primary_level = primary.and_then(required_level).unwrap_or(0);
    let secondary_level = secondary.and_then(required_level).unwrap_or(0);
    if primary_level > levels || secondary_level > levels {
        return false;
    }
    // Регенерация (4-й уровень списка) не может быть основной.
    if primary_level >= 4 {
        return false;
    }
    secondary_level == 0 || secondary_level >= 4 || primary == secondary
}

impl PlayPlan {
    /// `#minecraft:beacon_base_blocks`.
    fn beacon_base(&self, state: u32) -> bool {
        self.survival
            .block_tags
            .get("minecraft:beacon_base_blocks")
            .is_some_and(|set| set.contains(self.state_name(state)))
    }

    /// `BeaconBlockEntity.updateBase`: сколько полных слоёв под маяком.
    pub(super) fn beacon_levels(&self, [x, y, z]: [i32; 3]) -> i32 {
        let mut levels = 0;
        for step in 1..=4 {
            let ly = y - step;
            let mut ok = true;
            'layer: for lx in x - step..=x + step {
                for lz in z - step..=z + step {
                    if !self
                        .block([lx, ly, lz])
                        .is_some_and(|s| self.beacon_base(s))
                    {
                        ok = false;
                        break 'layer;
                    }
                }
            }
            if !ok {
                break;
            }
            levels = step;
        }
        levels
    }

    fn load_beacon(&self, pos: [i32; 3]) -> Beacon {
        let mut b = Beacon {
            levels: self.beacon_levels(pos),
            ..Default::default()
        };
        let Some(be) = self.block_entity(pos) else {
            return b;
        };
        let text = |key: &str| be.nbt.get(key).and_then(Tag::as_str).map(str::to_owned);
        b.primary = text("primary_effect");
        b.secondary = text("secondary_effect");
        b.custom_name = be.nbt.get("CustomName").cloned();
        if let Some(levels) = be.nbt.get("Levels").and_then(Tag::as_i64) {
            b.levels = levels as i32;
        }
        b
    }

    fn save_beacon(&self, pos: [i32; 3], b: &Beacon, sync: bool) {
        let mut nbt = Compound::new();
        if let Some(name) = &b.primary {
            nbt.insert("primary_effect".into(), Tag::String(name.clone()));
        }
        if let Some(name) = &b.secondary {
            nbt.insert("secondary_effect".into(), Tag::String(name.clone()));
        }
        nbt.insert("Levels".into(), Tag::Int(b.levels));
        if let Some(name) = &b.custom_name {
            nbt.insert("CustomName".into(), name.clone());
        }
        self.set_block_entity(
            pos,
            Some(BlockEntity {
                id: "minecraft:beacon".into(),
                nbt: nbt.clone(),
            }),
        );
        if sync {
            self.send_beacon(pos, &nbt);
        }
    }

    fn send_beacon(&self, pos: [i32; 3], nbt: &Compound) {
        let Some(types) = chunkwake_world::BLOCK_ENTITY_TYPES.get() else {
            return;
        };
        let Some(&kind) = types.get("minecraft:beacon") else {
            return;
        };
        let bytes = chunkwake_world::nbt::write_network(nbt);
        self.hub.send(
            None,
            play::block_entity_data(self.ids.block_entity_data, pos, kind, &bytes),
        );
    }

    /// Стойка маяка из кэша; при первом обращении читает блок-сущность.
    fn with_beacon<R>(&self, pos: [i32; 3], f: impl FnOnce(&mut Beacon) -> R) -> Option<R> {
        if !self
            .block(pos)
            .is_some_and(|s| self.state_name(s) == "minecraft:beacon")
        {
            return None;
        }
        if !self.sim().beacons.lock().unwrap().contains_key(&pos) {
            let b = self.load_beacon(pos);
            self.sim().beacons.lock().unwrap().insert(pos, b);
        }
        self.sim().beacons.lock().unwrap().get_mut(&pos).map(f)
    }

    /// `BeaconMenu.ContainerData`: уровень, основная и вторая способности
    /// (`id + 1`, 0 — нет).
    pub(super) fn beacon_data(&self, pos: [i32; 3]) -> [i32; 4] {
        let levels = self.beacon_levels(pos);
        let Some((old, b)) = self.with_beacon(pos, |b| {
            let old = b.levels;
            b.levels = levels;
            (old, b.clone())
        }) else {
            return [0; 4];
        };
        if old != levels {
            self.save_beacon(pos, &b, true);
        }
        let encode = |name: Option<String>| {
            name.and_then(|n| self.survival.effects.get(&n).copied())
                .map_or(0, |id| id + 1)
        };
        [levels, encode(b.primary), encode(b.secondary), 0]
    }

    /// Есть ли в слоте оплаты подходящий предмет.
    pub(super) fn beacon_payment(&self, item: i32) -> bool {
        self.survival
            .item_tags
            .get("minecraft:beacon_payment_items")
            .is_some_and(|set| set.contains(self.item_name(item)))
    }

    /// `BeaconMenu.updateEffects`: проверить и записать выбранные эффекты.
    pub(super) fn set_beacon_effects(
        &self,
        pos: [i32; 3],
        primary: Option<i32>,
        secondary: Option<i32>,
    ) -> bool {
        if !self
            .block(pos)
            .is_some_and(|s| self.state_name(s) == "minecraft:beacon")
        {
            return false;
        }
        let name = |id: Option<i32>| -> Option<Option<String>> {
            match id {
                None => Some(None),
                Some(id) => self.effect_name(id).map(|n| Some(n.to_owned())),
            }
        };
        let (Some(primary), Some(secondary)) = (name(primary), name(secondary)) else {
            return false;
        };
        let levels = self.beacon_levels(pos);
        if !valid_effects(primary.as_deref(), secondary.as_deref(), levels) {
            return false;
        }
        let Some(b) = self.with_beacon(pos, |b| {
            b.levels = levels;
            b.primary = primary.clone();
            b.secondary = secondary.clone();
            b.clone()
        }) else {
            return false;
        };
        self.save_beacon(pos, &b, true);
        if levels > 0 {
            self.play_sound(
                None,
                "minecraft:block.beacon.power_select",
                crate::sounds::BLOCKS,
                [
                    pos[0] as f64 + 0.5,
                    pos[1] as f64 + 0.5,
                    pos[2] as f64 + 0.5,
                ],
                1.0,
                1.0,
            );
        }
        true
    }

    /// `BeaconBlockEntity.applyEffects`: выдать эффекты игрокам в радиусе.
    fn beacon_effects(&self, pos: [i32; 3], b: &Beacon) {
        let Some(primary) = b.primary.as_deref() else {
            return;
        };
        let Some(&primary_id) = self.survival.effects.get(primary) else {
            return;
        };
        let levels = b.levels;
        let range = levels as f64 * 10.0 + 10.0;
        let duration = (9 + levels * 2) * 20;
        let primary_amp = u8::from(levels >= 4 && b.secondary.as_deref() == Some(primary));
        let secondary = (levels >= 4)
            .then_some(b.secondary.as_deref())
            .flatten()
            .filter(|s| Some(*s) != b.primary.as_deref())
            .and_then(|name| self.survival.effects.get(name).copied());
        let (cx, cz) = (pos[0] as f64 + 0.5, pos[2] as f64 + 0.5);
        let alive: Vec<Uuid> = self
            .live
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, l)| l.health > 0.0)
            .map(|(&uuid, _)| uuid)
            .collect();
        let players: Vec<(Uuid, [f64; 3])> = self
            .hub
            .players
            .lock()
            .unwrap()
            .iter()
            .filter(|(uuid, _)| alive.contains(uuid))
            .map(|(&uuid, p)| (uuid, p.pos))
            .collect();
        for (uuid, p) in players {
            if (p[0] - cx).powi(2) + (p[2] - cz).powi(2) > range * range {
                continue;
            }
            if p[1] < pos[1] as f64 - range {
                continue;
            }
            let mut e = survival::Effect::plain(primary_id, primary_amp, duration);
            e.ambient = true;
            self.push_control(
                uuid,
                Control::Effect {
                    effect: e,
                    force: false,
                    command: false,
                },
            );
            if let Some(id) = secondary {
                let mut e = survival::Effect::plain(id, 0, duration);
                e.ambient = true;
                self.push_control(
                    uuid,
                    Control::Effect {
                        effect: e,
                        force: false,
                        command: false,
                    },
                );
            }
        }
    }

    /// Тик маяков: раз в 80 тиков пересчитать пирамиду и выдать эффекты.
    pub(super) fn tick_beacons(&self) {
        let pending = std::mem::take(&mut *self.sim().pending_beacons.lock().unwrap());
        for pos in pending {
            self.with_beacon(pos, |_| ());
        }
        let simulated = self.simulated_set();
        let positions: Vec<[i32; 3]> = self.sim().beacons.lock().unwrap().keys().copied().collect();
        for pos in positions {
            if !simulated.is_empty() && !simulated.contains(&(pos[0] >> 4, pos[2] >> 4)) {
                continue;
            }
            self.tick_beacon(pos);
        }
    }

    fn tick_beacon(&self, pos: [i32; 3]) {
        if !self
            .block(pos)
            .is_some_and(|s| self.state_name(s) == "minecraft:beacon")
        {
            self.sim().beacons.lock().unwrap().remove(&pos);
            return;
        }
        if self.game_time.load(Ordering::Relaxed) % CHECK_INTERVAL != 0 {
            return;
        }
        let levels = self.beacon_levels(pos);
        let (old, b) = {
            let mut map = self.sim().beacons.lock().unwrap();
            let b = map.entry(pos).or_insert_with(|| self.load_beacon(pos));
            let old = b.levels;
            b.levels = levels;
            (old, b.clone())
        };
        if old != levels {
            self.save_beacon(pos, &b, true);
        }
        if levels > 0 {
            self.beacon_effects(pos, &b);
            self.play_sound(
                None,
                "minecraft:block.beacon.ambient",
                crate::sounds::BLOCKS,
                [
                    pos[0] as f64 + 0.5,
                    pos[1] as f64 + 0.5,
                    pos[2] as f64 + 0.5,
                ],
                1.0,
                1.0,
            );
        }
        if old == 0 && levels > 0 {
            self.play_sound(
                None,
                "minecraft:block.beacon.activate",
                crate::sounds::BLOCKS,
                [
                    pos[0] as f64 + 0.5,
                    pos[1] as f64 + 0.5,
                    pos[2] as f64 + 0.5,
                ],
                1.0,
                1.0,
            );
        } else if old > 0 && levels == 0 {
            self.play_sound(
                None,
                "minecraft:block.beacon.deactivate",
                crate::sounds::BLOCKS,
                [
                    pos[0] as f64 + 0.5,
                    pos[1] as f64 + 0.5,
                    pos[2] as f64 + 0.5,
                ],
                1.0,
                1.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_levels_match_vanilla() {
        assert_eq!(required_level("minecraft:speed"), Some(1));
        assert_eq!(required_level("minecraft:haste"), Some(1));
        assert_eq!(required_level("minecraft:resistance"), Some(2));
        assert_eq!(required_level("minecraft:jump_boost"), Some(2));
        assert_eq!(required_level("minecraft:strength"), Some(3));
        assert_eq!(required_level("minecraft:regeneration"), Some(4));
        assert_eq!(required_level("minecraft:poison"), None);
    }

    #[test]
    fn effect_pairs_are_validated_like_vanilla() {
        // Второй эффект только на 4-м уровне.
        assert!(!valid_effects(
            Some("minecraft:speed"),
            Some("minecraft:regeneration"),
            3
        ));
        // Одинаковые эффекты разрешены.
        assert!(valid_effects(
            Some("minecraft:speed"),
            Some("minecraft:speed"),
            4
        ));
        // Регенерация только второй.
        assert!(!valid_effects(Some("minecraft:regeneration"), None, 4));
        assert!(valid_effects(
            Some("minecraft:strength"),
            Some("minecraft:regeneration"),
            4
        ));
        // Уровень эффекта не выше пирамиды.
        assert!(!valid_effects(Some("minecraft:strength"), None, 2));
        assert!(valid_effects(None, None, 0));
    }
}
