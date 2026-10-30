//! Вывод снимка: компонент для чата и ANSI-таблица для терминала.
//!
//! Обе формы собираются из одного [`Snapshot`], поэтому `/top` в игре и
//! `chunkwake top` в терминале показывают одни и те же числа. Полоски
//! рисуются символами `█` и `░`: они есть в стандартных шрифтах Minecraft.

pub mod ansi;
pub mod chat;

pub use ansi::render as ansi;
pub use chat::render as chat;

/// Секции снимка.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    All,
    Cpu,
    Tick,
    Memory,
    World,
    Net,
}

impl Section {
    pub fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "all" => Section::All,
            "cpu" => Section::Cpu,
            "tick" | "tps" | "mspt" => Section::Tick,
            "mem" | "memory" => Section::Memory,
            "world" => Section::World,
            "net" | "network" => Section::Net,
            _ => return None,
        })
    }
}

/// Полоска шириной `width` для значения 0..max.
pub fn bar(value: f64, max: f64, width: usize) -> String {
    let ratio = if max <= 0.0 {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    };
    let filled = (ratio * width as f64).round() as usize;
    let mut out = String::with_capacity(width * 3);
    for _ in 0..filled {
        out.push('█');
    }
    for _ in filled..width {
        out.push('░');
    }
    out
}

/// Цвет по загрузке: зелёный, жёлтый, красный.
pub fn load_color(percent: f64) -> &'static str {
    if percent < 70.0 {
        "green"
    } else if percent < 90.0 {
        "yellow"
    } else {
        "red"
    }
}

/// Цвет TPS: 20 — зелёный, ниже 18 — жёлтый, ниже 15 — красный.
pub fn tps_color(tps: f64) -> &'static str {
    if tps >= 19.5 {
        "green"
    } else if tps >= 17.0 {
        "yellow"
    } else {
        "red"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_scale() {
        assert_eq!(bar(0.0, 100.0, 5), "░░░░░");
        assert_eq!(bar(100.0, 100.0, 5), "█████");
        assert_eq!(bar(50.0, 100.0, 4), "██░░");
        assert_eq!(bar(200.0, 100.0, 2), "██");
        assert_eq!(bar(1.0, 0.0, 2), "░░");
    }

    #[test]
    fn colors_by_load() {
        assert_eq!(load_color(10.0), "green");
        assert_eq!(load_color(80.0), "yellow");
        assert_eq!(load_color(99.0), "red");
        assert_eq!(tps_color(20.0), "green");
        assert_eq!(tps_color(18.0), "yellow");
        assert_eq!(tps_color(12.0), "red");
    }

    #[test]
    fn sections_parse() {
        assert_eq!(Section::parse("cpu"), Some(Section::Cpu));
        assert_eq!(Section::parse("tps"), Some(Section::Tick));
        assert_eq!(Section::parse("memory"), Some(Section::Memory));
        assert_eq!(Section::parse("nope"), None);
    }
}
