//! Питьевые зелья: состав из патча `potion_contents` и таблица ванильных зелий.
//!
//! Патч — обычный (без длины у компонента): число добавленных, число снятых,
//! затем пары «тип, данные». `PotionContents`: необязательный держатель зелья,
//! необязательный цвет, список своих эффектов, необязательное имя.
//! Свой эффект: держатель эффекта, затем amplifier, duration, три флага и
//! необязательный скрытый эффект.

use std::sync::atomic::Ordering;

use chunkwake_protocol::components::{
    CUSTOM_NAME_COMPONENT, ENCHANTMENTS_COMPONENT, LORE_COMPONENT,
};
use chunkwake_protocol::inventory::DAMAGE_COMPONENT;
use chunkwake_protocol::{Reader, text};

/// Один эффект ванильного зелья (`Potions`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brew {
    pub effect: &'static str,
    pub duration: i32,
    pub amplifier: u8,
}

/// Эффект, записанный прямо в предмете, а не в типе зелья.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Custom {
    pub id: i32,
    pub amplifier: u8,
    pub duration: i32,
    pub ambient: bool,
    pub particles: bool,
    pub show_icon: bool,
}

/// Разобранный `minecraft:potion_contents`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contents {
    /// Сетевой ID в реестре `potion`; `None` — зелье без типа (только свои эффекты).
    pub potion: Option<i32>,
    pub effects: Vec<Custom>,
}

/// Эффекты зелья по имени (`minecraft:swiftness` или `swiftness`). Пусто — вода и прочие без эффекта.
pub fn effects(name: &str) -> &'static [Brew] {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    TABLE
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(&[], |(_, e)| *e)
}

/// InstantaneousMobEffect.applyInstantenousEffect rounds the scaled amount.
pub fn splash_instant_amount(amount: f32, strength: f64) -> f32 {
    (amount * strength.clamp(0.0, 1.0) as f32 + 0.5).floor()
}

/// A directly hit entity receives full strength regardless of its center offset.
pub fn splash_strength(distance: f64, direct_hit: bool) -> f64 {
    if direct_hit { 1.0 } else { (1.0 - distance / 4.0).clamp(0.0, 1.0) }
}

/// Ищет `potion_contents` в сыром патче компонентов и читает его.
/// `None` — компонента нет или перед ним стоит компонент, который нельзя пропустить.
pub fn read_contents(raw: &[u8], component: i32) -> Option<Contents> {
    if component < 0 {
        return None;
    }
    let mut r = Reader::new(raw);
    let added = r.var_int().ok()?;
    let _removed = r.var_int().ok()?;
    if !(0..=256).contains(&added) {
        return None;
    }
    for _ in 0..added {
        let kind = r.var_int().ok()?;
        if kind == component {
            return read_potion(&mut r).ok();
        }
        if !skip_known(&mut r, kind) {
            return None;
        }
    }
    None
}

fn read_potion(r: &mut Reader<'_>) -> Result<Contents, ()> {
    let potion = if r.bool().map_err(|_| ())? {
        Some(r.var_int().map_err(|_| ())?)
    } else {
        None
    };
    if r.bool().map_err(|_| ())? {
        r.bytes(4).map_err(|_| ())?;
    }
    let n = r.var_int().map_err(|_| ())?;
    if !(0..=32).contains(&n) {
        return Err(());
    }
    let mut effects = Vec::with_capacity(n as usize);
    for _ in 0..n {
        effects.push(read_effect(r)?);
    }
    if r.bool().map_err(|_| ())? {
        r.string(1024).map_err(|_| ())?;
    }
    Ok(Contents { potion, effects })
}

fn read_effect(r: &mut Reader<'_>) -> Result<Custom, ()> {
    let id = r.var_int().map_err(|_| ())?;
    let (amplifier, duration, ambient, particles, show_icon) = read_details(r, 0)?;
    Ok(Custom {
        id,
        amplifier,
        duration,
        ambient,
        particles,
        show_icon,
    })
}

/// `MobEffectInstance.Details`: amplifier, duration, три флага, скрытый эффект.
fn read_details(r: &mut Reader<'_>, depth: u8) -> Result<(u8, i32, bool, bool, bool), ()> {
    let amplifier = r.var_int().map_err(|_| ())?.clamp(0, 255) as u8;
    let duration = r.var_int().map_err(|_| ())?;
    let ambient = r.bool().map_err(|_| ())?;
    let particles = r.bool().map_err(|_| ())?;
    let show_icon = r.bool().map_err(|_| ())?;
    if r.bool().map_err(|_| ())? {
        if depth >= 4 {
            return Err(());
        }
        let _ = read_details(r, depth + 1)?;
    }
    Ok((amplifier, duration, ambient, particles, show_icon))
}

fn skip_known(r: &mut Reader<'_>, kind: i32) -> bool {
    if kind < 0 {
        return false;
    }
    if kind == DAMAGE_COMPONENT.load(Ordering::Relaxed) {
        return r.var_int().is_ok();
    }
    if kind == ENCHANTMENTS_COMPONENT.load(Ordering::Relaxed) {
        let Ok(n) = r.var_int() else {
            return false;
        };
        if !(0..=256).contains(&n) {
            return false;
        }
        for _ in 0..n {
            if r.var_int().is_err() || r.var_int().is_err() {
                return false;
            }
        }
        return true;
    }
    if kind == CUSTOM_NAME_COMPONENT.load(Ordering::Relaxed) {
        return text::read_component(r).is_ok();
    }
    if kind == LORE_COMPONENT.load(Ordering::Relaxed) {
        let Ok(n) = r.var_int() else {
            return false;
        };
        if !(0..=256).contains(&n) {
            return false;
        }
        for _ in 0..n {
            if text::read_component(r).is_err() {
                return false;
            }
        }
        return true;
    }
    false
}

/// Длительности из `Potions` 26.3 (тики). Вода, густое, неуклюжее и обычное — без эффектов.
const TABLE: &[(&str, &[Brew])] = &[
    (
        "night_vision",
        &[Brew {
            effect: "minecraft:night_vision",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "long_night_vision",
        &[Brew {
            effect: "minecraft:night_vision",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "invisibility",
        &[Brew {
            effect: "minecraft:invisibility",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "long_invisibility",
        &[Brew {
            effect: "minecraft:invisibility",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "leaping",
        &[Brew {
            effect: "minecraft:jump_boost",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "long_leaping",
        &[Brew {
            effect: "minecraft:jump_boost",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "strong_leaping",
        &[Brew {
            effect: "minecraft:jump_boost",
            duration: 1800,
            amplifier: 1,
        }],
    ),
    (
        "fire_resistance",
        &[Brew {
            effect: "minecraft:fire_resistance",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "long_fire_resistance",
        &[Brew {
            effect: "minecraft:fire_resistance",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "swiftness",
        &[Brew {
            effect: "minecraft:speed",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "long_swiftness",
        &[Brew {
            effect: "minecraft:speed",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "strong_swiftness",
        &[Brew {
            effect: "minecraft:speed",
            duration: 1800,
            amplifier: 1,
        }],
    ),
    (
        "slowness",
        &[Brew {
            effect: "minecraft:slowness",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "long_slowness",
        &[Brew {
            effect: "minecraft:slowness",
            duration: 4800,
            amplifier: 0,
        }],
    ),
    (
        "strong_slowness",
        &[Brew {
            effect: "minecraft:slowness",
            duration: 400,
            amplifier: 3,
        }],
    ),
    (
        "turtle_master",
        &[
            Brew {
                effect: "minecraft:slowness",
                duration: 400,
                amplifier: 3,
            },
            Brew {
                effect: "minecraft:resistance",
                duration: 400,
                amplifier: 2,
            },
        ],
    ),
    (
        "long_turtle_master",
        &[
            Brew {
                effect: "minecraft:slowness",
                duration: 800,
                amplifier: 3,
            },
            Brew {
                effect: "minecraft:resistance",
                duration: 800,
                amplifier: 2,
            },
        ],
    ),
    (
        "strong_turtle_master",
        &[
            Brew {
                effect: "minecraft:slowness",
                duration: 400,
                amplifier: 5,
            },
            Brew {
                effect: "minecraft:resistance",
                duration: 400,
                amplifier: 3,
            },
        ],
    ),
    (
        "water_breathing",
        &[Brew {
            effect: "minecraft:water_breathing",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "long_water_breathing",
        &[Brew {
            effect: "minecraft:water_breathing",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "healing",
        &[Brew {
            effect: "minecraft:instant_health",
            duration: 1,
            amplifier: 0,
        }],
    ),
    (
        "strong_healing",
        &[Brew {
            effect: "minecraft:instant_health",
            duration: 1,
            amplifier: 1,
        }],
    ),
    (
        "harming",
        &[Brew {
            effect: "minecraft:instant_damage",
            duration: 1,
            amplifier: 0,
        }],
    ),
    (
        "strong_harming",
        &[Brew {
            effect: "minecraft:instant_damage",
            duration: 1,
            amplifier: 1,
        }],
    ),
    (
        "poison",
        &[Brew {
            effect: "minecraft:poison",
            duration: 900,
            amplifier: 0,
        }],
    ),
    (
        "long_poison",
        &[Brew {
            effect: "minecraft:poison",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "strong_poison",
        &[Brew {
            effect: "minecraft:poison",
            duration: 432,
            amplifier: 1,
        }],
    ),
    (
        "regeneration",
        &[Brew {
            effect: "minecraft:regeneration",
            duration: 900,
            amplifier: 0,
        }],
    ),
    (
        "long_regeneration",
        &[Brew {
            effect: "minecraft:regeneration",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "strong_regeneration",
        &[Brew {
            effect: "minecraft:regeneration",
            duration: 450,
            amplifier: 1,
        }],
    ),
    (
        "strength",
        &[Brew {
            effect: "minecraft:strength",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "long_strength",
        &[Brew {
            effect: "minecraft:strength",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "strong_strength",
        &[Brew {
            effect: "minecraft:strength",
            duration: 1800,
            amplifier: 1,
        }],
    ),
    (
        "weakness",
        &[Brew {
            effect: "minecraft:weakness",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "long_weakness",
        &[Brew {
            effect: "minecraft:weakness",
            duration: 4800,
            amplifier: 0,
        }],
    ),
    (
        "luck",
        &[Brew {
            effect: "minecraft:luck",
            duration: 6000,
            amplifier: 0,
        }],
    ),
    (
        "slow_falling",
        &[Brew {
            effect: "minecraft:slow_falling",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "long_slow_falling",
        &[Brew {
            effect: "minecraft:slow_falling",
            duration: 4800,
            amplifier: 0,
        }],
    ),
    (
        "wind_charged",
        &[Brew {
            effect: "minecraft:wind_charged",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "weaving",
        &[Brew {
            effect: "minecraft:weaving",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "oozing",
        &[Brew {
            effect: "minecraft:oozing",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "infested",
        &[Brew {
            effect: "minecraft:infested",
            duration: 3600,
            amplifier: 0,
        }],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use chunkwake_protocol::Writer;

    #[test]
    fn instant_splash_scales_with_distance() {
        assert_eq!(splash_instant_amount(6.0, 1.0), 6.0);
        assert_eq!(splash_instant_amount(6.0, 0.5), 3.0);
        assert_eq!(splash_instant_amount(6.0, 0.0), 0.0);
    }

    #[test]
    fn direct_splash_hit_has_full_strength() {
        assert_eq!(splash_strength(2.0, false), 0.5);
        assert_eq!(splash_strength(2.0, true), 1.0);
        assert_eq!(splash_instant_amount(6.0, splash_strength(2.0, true)), 6.0);
        assert_eq!(splash_instant_amount(6.0, splash_strength(2.0, false)), 3.0);
    }

    #[test]
    fn potion_patch_reads_holder_and_custom_effect() {
        let mut w = Writer::new();
        w.var_int(1)
            .var_int(0)
            .var_int(53)
            .bool(true)
            .var_int(13)
            .bool(false)
            .var_int(0)
            .bool(false);
        let c = read_contents(&w.buf, 53).unwrap();
        assert_eq!(c.potion, Some(13));
        assert!(c.effects.is_empty());

        let mut w = Writer::new();
        w.var_int(1).var_int(0).var_int(53);
        w.bool(false).bool(true).i32(0xff0000).var_int(1);
        w.var_int(1).var_int(0).var_int(3600);
        w.bool(false).bool(true).bool(true).bool(false);
        w.bool(false);
        let c = read_contents(&w.buf, 53).unwrap();
        assert_eq!(c.potion, None);
        assert_eq!(c.effects.len(), 1);
        assert_eq!(
            (
                c.effects[0].id,
                c.effects[0].amplifier,
                c.effects[0].duration,
                c.effects[0].particles,
                c.effects[0].show_icon
            ),
            (1, 0, 3600, true, true)
        );
        assert!(read_contents(&w.buf, 7).is_none());
    }

    #[test]
    fn vanilla_swiftness_and_turtle_master() {
        assert_eq!(effects("minecraft:swiftness")[0].duration, 3600);
        assert_eq!(effects("minecraft:swiftness")[0].effect, "minecraft:speed");
        assert_eq!(effects("strong_swiftness")[0].amplifier, 1);
        assert_eq!(effects("strong_swiftness")[0].duration, 1800);
        assert_eq!(effects("turtle_master").len(), 2);
        assert_eq!(effects("healing")[0].duration, 1);
        assert!(effects("water").is_empty());
        assert!(effects("minecraft:awkward").is_empty());
    }
}
