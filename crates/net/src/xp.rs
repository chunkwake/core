//! Опыт игрока (`Player.giveExperiencePoints` и соседи): уровень, полоска,
//! всего очков и зерно стола зачарования.

/// Опыт и зерно чар (`XpLevel`, `XpP`, `XpTotal`, `XpSeed` в `playerdata`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Experience {
    pub level: i32,
    /// Доля до следующего уровня, 0..1.
    pub progress: f32,
    pub total: i32,
    /// `enchantmentSeed`: от него варианты на столе; меняется после зачарования.
    pub seed: i32,
}

/// Очков до следующего уровня (`getXpNeededForNextLevel`).
pub fn needed(level: i32) -> i32 {
    if level >= 30 {
        112 + (level - 30) * 9
    } else if level >= 15 {
        37 + (level - 15) * 5
    } else {
        7 + level * 2
    }
}

impl Experience {
    /// `giveExperiencePoints`: очки в полоску с переносом на уровни.
    pub fn add_points(&mut self, points: i32) {
        self.progress += points as f32 / needed(self.level) as f32;
        self.total = self.total.saturating_add(points).max(0);
        while self.progress < 0.0 {
            let lost = self.progress * needed(self.level) as f32;
            if self.level > 0 {
                self.add_levels(-1);
                self.progress = 1.0 + lost / needed(self.level) as f32;
            } else {
                self.add_levels(-1);
                self.progress = 0.0;
            }
        }
        while self.progress >= 1.0 {
            self.progress = (self.progress - 1.0) * needed(self.level) as f32;
            self.add_levels(1);
            self.progress /= needed(self.level) as f32;
        }
    }

    /// `giveExperienceLevels`: ниже нуля — всё обнуляется.
    pub fn add_levels(&mut self, levels: i32) {
        self.level = self.level.saturating_add(levels);
        if self.level < 0 {
            *self = Self {
                seed: self.seed,
                ..Self::default()
            };
        }
    }

    /// `/xp set … points`: полоска внутри текущего уровня.
    pub fn set_points(&mut self, points: i32) -> bool {
        let max = needed(self.level);
        if !(0..max).contains(&points) {
            return false;
        }
        self.progress = points as f32 / max as f32;
        true
    }

    /// Очков в полоске сейчас (`/xp query … points`).
    pub fn points(&self) -> i32 {
        (self.progress * needed(self.level) as f32) as i32
    }

    /// `onEnchantmentPerformed`: списать уровни и сменить зерно.
    pub fn enchanted(&mut self, levels: i32, new_seed: i32) {
        self.add_levels(-levels);
        self.seed = new_seed;
    }
}

/// Изменение опыта от команды или плагина.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum XpChange {
    /// `giveExp`, `/xp add … points`.
    Points(i32),
    /// `giveExpLevels`, `/xp add … levels`.
    Levels(i32),
    /// `setLevel`, `/xp set … levels`.
    SetLevel(i32),
    /// `/xp set … points`.
    SetPoints(i32),
    /// `setExp`: доля полоски.
    SetProgress(f32),
    /// `setTotalExperience`: только счётчик.
    SetTotal(i32),
}

impl Experience {
    /// Применяет изменение; `false` — недопустимое значение.
    pub fn apply(&mut self, c: XpChange) -> bool {
        match c {
            XpChange::Points(n) => self.add_points(n),
            XpChange::Levels(n) => self.add_levels(n),
            XpChange::SetLevel(n) if n >= 0 => self.level = n,
            XpChange::SetPoints(n) => return self.set_points(n),
            XpChange::SetProgress(p) if (0.0..=1.0).contains(&p) => self.progress = p,
            XpChange::SetTotal(n) if n >= 0 => self.total = n,
            _ => return false,
        }
        true
    }
}

/// Опыт за руду (`DropExperienceBlock`/`RedStoneOreBlock`): диапазон очков;
/// с Шёлковым касанием опыта нет.
pub fn ore_range(block: &str) -> Option<(i32, i32)> {
    let name = block.strip_prefix("minecraft:").unwrap_or(block);
    let base = name.strip_prefix("deepslate_").unwrap_or(name);
    Some(match base {
        "coal_ore" => (0, 2),
        "diamond_ore" | "emerald_ore" => (3, 7),
        "lapis_ore" | "nether_quartz_ore" => (2, 5),
        "redstone_ore" => (1, 5),
        "nether_gold_ore" => (0, 1),
        "sculk" => (1, 1),
        "sculk_catalyst" | "sculk_shrieker" | "sculk_sensor" => (5, 5),
        "calibrated_sculk_sensor" => (5, 5),
        "spawner" => (15, 43),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_like_vanilla() {
        let mut x = Experience::default();
        // 0→1 — 7 очков, 1→2 — 9: 16 очков — ровно 2 уровня.
        x.add_points(16);
        assert_eq!((x.level, x.total), (2, 16));
        assert!(x.progress.abs() < 1e-6);
        x.add_points(5);
        assert_eq!(x.points(), 5);
        x.enchanted(3, 42);
        assert_eq!(
            x,
            Experience {
                seed: 42,
                ..Default::default()
            }
        );
        assert_eq!((needed(15), needed(30), needed(31)), (37, 112, 121));
        assert_eq!(ore_range("minecraft:deepslate_diamond_ore"), Some((3, 7)));
    }
}
