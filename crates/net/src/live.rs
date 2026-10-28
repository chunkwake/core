//! Состояние игрока для плагинов: зеркало, которое сессия обновляет каждый тик,
//! и команды управления от плагинов (`Player.setHealth`, `getInventory().setItem`…).
//!
//! Формат обмена с JVM — текст: строки через `\n`, поля через пробел.

use crate::survival::{OFFHAND, SLOTS};
use chunkwake_protocol::inventory::{Slot, Stack};

/// Размер `PlayerInventory` Bukkit: хотбар, основной, броня, вторая рука.
pub const BUKKIT_SLOTS: usize = 41;

/// Индекс Bukkit (`PlayerInventory`) → слот окна игрока (`InventoryMenu`).
/// 0–8 хотбар, 9–35 основной, 36–39 ботинки..шлем, 40 вторая рука.
pub fn bukkit_to_window(i: usize) -> Option<usize> {
    match i {
        0..=8 => Some(36 + i),
        9..=35 => Some(i),
        36..=39 => Some(8 - (i - 36)),
        40 => Some(OFFHAND),
        _ => None,
    }
}

/// Снимок игрока для плагинов.
#[derive(Clone, Debug, PartialEq)]
pub struct Live {
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub game_mode: u8,
    pub selected: usize,
    pub slots: [Slot; SLOTS],
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}

impl Live {
    /// Первая строка — `health food saturation exhaustion mode selected x y z yaw pitch`, затем
    /// [`BUKKIT_SLOTS`] строк `item count` (или `-`) в порядке Bukkit.
    pub fn render(&self, item_name: impl Fn(i32) -> Option<String>) -> String {
        let [x, y, z] = self.pos;
        let mut out = format!(
            "{} {} {} {} {} {} {x} {y} {z} {} {}",
            self.health,
            self.food,
            self.saturation,
            self.exhaustion,
            self.game_mode,
            self.selected,
            self.yaw,
            self.pitch
        );
        for i in 0..BUKKIT_SLOTS {
            out.push('\n');
            let stack = bukkit_to_window(i).and_then(|w| self.slots[w]);
            match stack.and_then(|s| item_name(s.item).map(|n| (n, s.count))) {
                Some((name, count)) => out += &format!("{name} {count}"),
                None => out.push('-'),
            }
        }
        out
    }
}

/// Команда сессии игрока от плагина.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Health(f32),
    Food(i32),
    Saturation(f32),
    Exhaustion(f32),
    /// Режим без события (плагин уже вызвал `PlayerGameModeChangeEvent`).
    GameMode(u8),
    /// Слот окна и стопка.
    Slot(usize, Slot),
    Selected(usize),
    /// Телепорт: позиция, поворот.
    Teleport([f64; 3], f32, f32),
    /// Внутреннее (не от плагинов): содержимое сундука изменил другой игрок.
    ContainerChanged([i32; 3]),
    /// Внутреннее: тик печи — свойства окна и сменились ли предметы.
    Furnace([i32; 3], [i32; 4], bool),
}

impl Control {
    /// Разбор строки от JVM: `health 10`, `food 20`, `mode 1`,
    /// `slot <bukkit> <item> <count>` или `slot <bukkit> -`, `held 3`.
    pub fn parse(line: &str, item_id: impl Fn(&str) -> Option<i32>) -> Option<Self> {
        let mut it = line.split_whitespace();
        let f = |s: Option<&str>| {
            s.and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite())
        };
        Some(match it.next()? {
            "health" => Control::Health(f(it.next())?.max(0.0)),
            "food" => Control::Food(it.next()?.parse::<i32>().ok()?.clamp(0, 20)),
            "saturation" => Control::Saturation(f(it.next())?.max(0.0)),
            "exhaustion" => Control::Exhaustion(f(it.next())?.max(0.0)),
            "mode" => Control::GameMode(it.next()?.parse::<u8>().ok().filter(|&m| m <= 3)?),
            "tp" => {
                let mut n = || it.next().and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite());
                let pos = [n()?, n()?, n()?];
                Control::Teleport(pos, n()? as f32, n()? as f32)
            }
            "held" => Control::Selected(it.next()?.parse::<usize>().ok().filter(|&s| s <= 8)?),
            "slot" => {
                let window = bukkit_to_window(it.next()?.parse().ok()?)?;
                let stack = match it.next()? {
                    "-" => None,
                    name => {
                        let count: i32 = it.next()?.parse().ok()?;
                        (count > 0).then_some(Stack {
                            item: item_id(name)?,
                            count,
                            damage: 0,
                        })
                    }
                };
                Control::Slot(window, stack)
            }
            _ => return None,
        })
    }

    /// Применяет команду к зеркалу (чтобы плагин сразу видел своё изменение).
    pub fn apply(&self, live: &mut Live) {
        match *self {
            Control::Health(h) => live.health = h,
            Control::Food(f) => live.food = f,
            Control::Saturation(s) => live.saturation = s,
            Control::Exhaustion(e) => live.exhaustion = e,
            Control::GameMode(m) => live.game_mode = m,
            Control::Slot(w, s) => live.slots[w] = s,
            Control::Selected(s) => live.selected = s,
            Control::Teleport(p, yaw, pitch) => {
                live.pos = p;
                live.yaw = yaw;
                live.pitch = pitch;
            }
            Control::ContainerChanged(_) | Control::Furnace(..) => {}
        }
    }
}

/// События для плагинов (`PluginHooks::event`); номера совпадают с
/// `dev.chunkwake.bridge.Bridge.EVENT_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Event {
    /// `EntityDamageEvent`: `<CAUSE> <amount>` → `c` (отмена) или `d <amount>`.
    Damage = 1,
    /// `PlayerDeathEvent`: `<damage_type>\n<message json>\n<item count>…` →
    /// `<keepInventory 0|1>\n<message json или пусто>\n<item count>…`.
    Death = 2,
    /// `PlayerDropItemEvent`: `<item> <count>` → `c` или пусто.
    Drop = 3,
    /// `EntityPickupItemEvent`: `<item> <count> <remaining>` → `c` или пусто.
    Pickup = 4,
    /// `PlayerGameModeChangeEvent`: `<mode> <CAUSE>` → `c` или пусто.
    GameMode = 5,
    /// `AsyncChatEvent`/`AsyncPlayerChatEvent`: текст → `c` или JSON сообщения для всех.
    Chat = 6,
    /// `PlayerCommandPreprocessEvent`: строка без `/` → `c`, `m <новая строка>` или пусто.
    CommandPreprocess = 7,
    /// Таб-комплит команд плагинов: строка без `/` → варианты через `\n`.
    TabComplete = 8,
    /// `BlockPlaceEvent`: `x y z block ax ay az hand` → `c` или пусто.
    Place = 9,
    /// `PlayerMoveEvent`: `from(x y z yaw pitch) to(x y z yaw pitch)` →
    /// `c`, `t x y z yaw pitch` (новая цель) или пусто.
    Move = 10,
    /// `PlayerInteractEvent`: `ACTION x y z FACE hand` (`~` вместо координат без блока) → `c` или пусто.
    Interact = 11,
}

impl Event {
    /// Бит в маске слушателей (`Bridge.LISTEN_*`).
    pub fn listener_bit(self) -> u64 {
        4 << self as i32
    }
}

/// Стопки из строк `item count` (имена неизвестных предметов пропускаются).
pub fn parse_stacks<'a>(
    lines: impl Iterator<Item = &'a str>,
    item_id: impl Fn(&str) -> Option<i32>,
) -> Vec<Stack> {
    lines
        .filter_map(|l| {
            let (name, count) = l.trim().split_once(' ')?;
            let count: i32 = count.parse().ok()?;
            (count > 0).then_some(Stack {
                item: item_id(name)?,
                count,
                damage: 0,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(name: &str) -> Option<i32> {
        match name {
            "minecraft:stone" => Some(1),
            "minecraft:bread" => Some(2),
            _ => None,
        }
    }

    #[test]
    fn slot_mapping_matches_inventory_menu() {
        assert_eq!(bukkit_to_window(0), Some(36));
        assert_eq!(bukkit_to_window(8), Some(44));
        assert_eq!(bukkit_to_window(9), Some(9));
        assert_eq!(bukkit_to_window(35), Some(35));
        // Ботинки (36) → слот 8, шлем (39) → 5.
        assert_eq!(bukkit_to_window(36), Some(8));
        assert_eq!(bukkit_to_window(39), Some(5));
        assert_eq!(bukkit_to_window(40), Some(OFFHAND));
        assert_eq!(bukkit_to_window(41), None);
    }

    #[test]
    fn render_and_control() {
        let mut live = Live {
            health: 20.0,
            food: 20,
            saturation: 5.0,
            exhaustion: 0.0,
            game_mode: 0,
            selected: 0,
            slots: [None; SLOTS],
            pos: [1.0, 64.0, -2.5],
            yaw: 90.0,
            pitch: 0.0,
        };
        let c = Control::parse("slot 0 minecraft:bread 3", ids).unwrap();
        assert_eq!(c, Control::Slot(36, Some(Stack { item: 2, count: 3 , damage: 0})));
        c.apply(&mut live);
        Control::parse("health 7.5", ids).unwrap().apply(&mut live);
        let names = |id| match id {
            2 => Some("minecraft:bread".to_owned()),
            _ => None,
        };
        let text = live.render(names);
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("7.5 20 5 0 0 0 1 64 -2.5 90 0"));
        assert_eq!(lines.next(), Some("minecraft:bread 3"));
        assert_eq!(text.lines().count(), 1 + BUKKIT_SLOTS);
        assert_eq!(
            Control::parse("slot 3 -", ids),
            Some(Control::Slot(39, None))
        );
        assert_eq!(Control::parse("mode 9", ids), None);
        assert_eq!(
            Control::parse("tp 1 2.5 3 90 -10", ids),
            Some(Control::Teleport([1.0, 2.5, 3.0], 90.0, -10.0))
        );
        assert_eq!(Control::parse("tp 1 NaN 3 0 0", ids), None);
        assert_eq!(Control::parse("slot 0 minecraft:nope 1", ids), None);
    }

    #[test]
    fn stacks_from_lines() {
        let s = parse_stacks("minecraft:stone 5\nbad\nminecraft:nope 1\n".lines(), ids);
        assert_eq!(s, vec![Stack { item: 1, count: 5 , damage: 0}]);
    }
}
