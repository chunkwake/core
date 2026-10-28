//! Стопка на диске как у ванилы (`ItemStack.CODEC`): `id`, `count` и
//! `components` — износ, чары (`{"minecraft:efficiency": 5}`), имя и описание
//! (текстовые компоненты в NBT).

use chunkwake_protocol::components::{self, Extra};
use chunkwake_protocol::inventory::Stack;
use chunkwake_world::nbt::{Compound, Tag};
use serde_json::Value;

/// `{id, count, components}`; `None` — у предмета нет имени.
pub fn to_nbt(s: Stack, name: &str) -> Option<Compound> {
    if name.is_empty() {
        return None;
    }
    let mut c = Compound::from([
        ("id".into(), Tag::String(name.to_owned())),
        ("count".into(), Tag::Int(s.count)),
    ]);
    let mut comps = Compound::new();
    if s.damage > 0 {
        comps.insert("minecraft:damage".into(), Tag::Int(s.damage));
    }
    let e = components::get(s.extra);
    if !e.enchantments.is_empty() {
        let levels: Compound = e
            .enchantments
            .iter()
            .filter_map(|&(id, lvl)| {
                Some((components::enchantment_name(id)?.to_owned(), Tag::Int(lvl)))
            })
            .collect();
        comps.insert("minecraft:enchantments".into(), Tag::Compound(levels));
    }
    if let Some(n) = &e.custom_name {
        comps.insert("minecraft:custom_name".into(), json_to_tag(&parse(n)));
    }
    if !e.lore.is_empty() {
        let lines = e.lore.iter().map(|l| json_to_tag(&parse(l))).collect();
        comps.insert("minecraft:lore".into(), Tag::List(lines));
    }
    if !comps.is_empty() {
        c.insert("components".into(), Tag::Compound(comps));
    }
    if let Some(raw) = &e.raw {
        c.insert(
            "chunkwake:component_patch".into(),
            Tag::ByteArray(raw.iter().map(|&b| b as i8).collect()),
        );
    }
    Some(c)
}

/// Обратно; неизвестные компоненты пропускаются.
pub fn from_nbt(c: &Compound, item_id: impl Fn(&str) -> Option<i32>) -> Option<Stack> {
    let item = item_id(c.get("id")?.as_str()?)?;
    let count = c.get("count").and_then(Tag::as_i64).unwrap_or(1) as i32;
    if count <= 0 {
        return None;
    }
    let mut s = Stack::new(item, count);
    let mut e = Extra::default();
    if let Some(Tag::Compound(m)) = c.get("components") {
        s.damage = m
            .get("minecraft:damage")
            .and_then(Tag::as_i64)
            .unwrap_or(0)
            .max(0) as i32;
        if let Some(Tag::Compound(levels)) = m.get("minecraft:enchantments") {
            for (name, lvl) in levels {
                if let (Some(id), Some(lvl)) = (components::enchantment_id(name), lvl.as_i64()) {
                    e.enchantments.push((id, lvl as i32));
                }
            }
        }
        e.custom_name = m
            .get("minecraft:custom_name")
            .map(|t| tag_to_json(t).to_string());
        if let Some(lines) = m.get("minecraft:lore").and_then(Tag::as_list) {
            e.lore = lines.iter().map(|t| tag_to_json(t).to_string()).collect();
        }
    }
    e.raw = c
        .get("chunkwake:component_patch")
        .and_then(Tag::as_byte_array)
        .map(|b| b.iter().map(|&x| x as u8).collect());
    s.extra = components::intern(e);
    Some(s)
}

/// Строка для плагинов: `item count` и, если есть компоненты, JSON
/// `{"damage", "enchantments": {имя: уровень}, "name", "lore"}` до конца строки.
pub fn to_line(s: Stack, name: &str) -> String {
    let mut out = format!("{name} {}", s.count);
    let e = components::get(s.extra);
    if s.damage == 0 && e.is_empty() {
        return out;
    }
    let mut j = serde_json::Map::new();
    if s.damage > 0 {
        j.insert("damage".into(), s.damage.into());
    }
    if !e.enchantments.is_empty() {
        let levels = e
            .enchantments
            .iter()
            .filter_map(|&(id, lvl)| {
                Some((
                    components::enchantment_name(id)?.to_owned(),
                    Value::from(lvl),
                ))
            })
            .collect();
        j.insert("enchantments".into(), Value::Object(levels));
    }
    if let Some(n) = &e.custom_name {
        j.insert("name".into(), parse(n));
    }
    if !e.lore.is_empty() {
        j.insert("lore".into(), e.lore.iter().map(|l| parse(l)).collect());
    }
    // Непонятые сервером компоненты (зелья и т. п.) — шестнадцатеричной строкой, чтобы вернулись целыми.
    if let Some(raw) = &e.raw {
        j.insert(
            "raw".into(),
            raw.iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into(),
        );
    }
    out.push(' ');
    out += &Value::Object(j).to_string();
    out
}

/// Обратно из [`to_line`]; неизвестный предмет или пустая стопка — `None`.
pub fn from_line(line: &str, item_id: impl Fn(&str) -> Option<i32>) -> Option<Stack> {
    let mut it = line.trim().splitn(3, ' ');
    let item = item_id(it.next()?)?;
    let count: i32 = it.next()?.parse().ok()?;
    if count <= 0 {
        return None;
    }
    let mut s = Stack::new(item, count);
    if let Some(Value::Object(j)) = it.next().and_then(|t| serde_json::from_str(t).ok()) {
        s.damage = j.get("damage").and_then(Value::as_i64).unwrap_or(0).max(0) as i32;
        let mut e = Extra::default();
        if let Some(Value::Object(levels)) = j.get("enchantments") {
            for (name, lvl) in levels {
                if let (Some(id), Some(lvl)) = (components::enchantment_id(name), lvl.as_i64()) {
                    e.enchantments.push((id, lvl.clamp(1, 255) as i32));
                }
            }
        }
        e.custom_name = j.get("name").filter(|v| !v.is_null()).map(Value::to_string);
        if let Some(Value::Array(lore)) = j.get("lore") {
            e.lore = lore.iter().map(Value::to_string).collect();
        }
        e.raw = j.get("raw").and_then(Value::as_str).and_then(|h| {
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok())
                .collect()
        });
        s.extra = components::intern(e);
    }
    Some(s)
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}

/// Текстовый компонент JSON → NBT (строки, числа, флаги-байты, списки, словари).
pub fn json_to_tag(v: &Value) -> Tag {
    match v {
        Value::Null => Tag::String(String::new()),
        Value::Bool(b) => Tag::Byte(*b as i8),
        Value::Number(n) => match n.as_i64() {
            Some(i) if i32::try_from(i).is_ok() => Tag::Int(i as i32),
            Some(i) => Tag::Long(i),
            None => Tag::Double(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => Tag::String(s.clone()),
        Value::Array(a) => Tag::List(a.iter().map(json_to_tag).collect()),
        Value::Object(o) => {
            Tag::Compound(o.iter().map(|(k, v)| (k.clone(), json_to_tag(v))).collect())
        }
    }
}

/// NBT → JSON; байт 0/1 — флаг стиля (`bold` и т. п.).
pub fn tag_to_json(t: &Tag) -> Value {
    match t {
        Tag::Byte(0) => Value::Bool(false),
        Tag::Byte(1) => Value::Bool(true),
        Tag::Byte(b) => Value::from(*b),
        Tag::Short(v) => Value::from(*v),
        Tag::Int(v) => Value::from(*v),
        Tag::Long(v) => Value::from(*v),
        Tag::Float(v) => Value::from(*v as f64),
        Tag::Double(v) => Value::from(*v),
        Tag::String(s) => Value::String(s.clone()),
        Tag::List(l) => Value::Array(l.iter().map(tag_to_json).collect()),
        Tag::Compound(c) => {
            Value::Object(c.iter().map(|(k, v)| (k.clone(), tag_to_json(v))).collect())
        }
        Tag::ByteArray(a) => Value::from(a.iter().map(|&b| b as i64).collect::<Vec<_>>()),
        Tag::IntArray(a) => Value::from(a.clone()),
        Tag::LongArray(a) => Value::from(a.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_components() {
        components::set_enchantments(vec![
            "minecraft:efficiency".into(),
            "minecraft:unbreaking".into(),
        ]);
        let e = Extra {
            enchantments: vec![(0, 5), (1, 3)],
            custom_name: Some(r#"{"text":"Кирка","italic":false}"#.into()),
            lore: vec![r#""строка""#.into()],
            raw: None,
        };
        let s = Stack {
            item: 7,
            count: 1,
            damage: 12,
            extra: components::intern(e),
        };
        let c = to_nbt(s, "minecraft:diamond_pickaxe").unwrap();
        let comps = c["components"].as_compound().unwrap();
        assert_eq!(
            comps["minecraft:enchantments"].as_compound().unwrap()["minecraft:efficiency"],
            Tag::Int(5)
        );
        let back = from_nbt(&c, |n| (n == "minecraft:diamond_pickaxe").then_some(7)).unwrap();
        assert_eq!(back, s);

        let potion = Stack {
            item: 3,
            count: 1,
            damage: 0,
            extra: components::intern(Extra {
                raw: Some(vec![1, 0, 42, 7]),
                ..Default::default()
            }),
        };
        let saved = to_nbt(potion, "minecraft:potion").unwrap();
        let loaded = from_nbt(&saved, |n| (n == "minecraft:potion").then_some(3)).unwrap();
        assert_eq!(loaded, potion);

        for st in [s, potion] {
            let line = to_line(st, "minecraft:x");
            assert!(line.starts_with("minecraft:x 1 {"), "{line}");
            assert_eq!(from_line(&line, |_| Some(st.item)), Some(st));
        }
        assert_eq!(
            from_line("minecraft:stone 3", |_| Some(1)),
            Some(Stack::new(1, 3))
        );
    }
}
