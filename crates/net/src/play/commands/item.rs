//! Разбор `item_stack`: `id[component=snbt,…]` в поля [`Extra`].

use super::snbt;
use chunkwake_protocol::components::{self, Extra};
use chunkwake_world::nbt::Tag;
use std::collections::HashMap;

/// Предмет команды: id, компоненты и износ, если компонент `damage` был задан.
pub struct ParsedItem {
    pub id: i32,
    pub extra: u32,
    /// `Some` — в аргументе был `minecraft:damage`. `None` — износ не фильтруем.
    pub damage: Option<i32>,
    /// Зелье из `potion_contents` (`"minecraft:water"` или `{potion:…}`):
    /// реестра зелий здесь нет, сырой компонент собирает вызывающий.
    pub potion: Option<String>,
}

/// `minecraft:stone[minecraft:custom_name="Hi"]` → id и интернированные компоненты.
pub fn parse_item(token: &str, ids: &HashMap<String, i32>) -> Option<ParsedItem> {
    let (name, body) = split_item(token)?;
    let name = super::args::resource_name(name);
    let id = *ids.get(&name)?;
    let (extra, damage, potion) = if let Some(body) = body {
        components_of(body)?
    } else {
        (Extra::default(), None, None)
    };
    Some(ParsedItem {
        id,
        extra: components::intern(extra),
        damage,
        potion,
    })
}

/// Имя и тело `[…]`, если оно есть. `{…}` после имени — тот же compound компонентов.
fn split_item(token: &str) -> Option<(&str, Option<&str>)> {
    let token = token.trim();
    if let Some(start) = token.find('[').filter(|_| token.ends_with(']')) {
        let body = &token[start + 1..token.len() - 1];
        return Some((&token[..start], Some(body)));
    }
    if let Some(start) = token.find('{').filter(|_| token.ends_with('}')) {
        return Some((&token[..start], Some(&token[start..])));
    }
    (!token.is_empty()).then_some((token, None))
}

type Parsed = (Extra, Option<i32>, Option<String>);

fn components_of(body: &str) -> Option<Parsed> {
    let body = body.trim();
    if body.is_empty() {
        return Some((Extra::default(), None, None));
    }
    // Старый `{nbt}` целиком — compound компонентов, не список пар `key=value`.
    if body.starts_with('{') {
        let Tag::Compound(map) = snbt::parse_value(body)? else {
            return None;
        };
        return Some(apply_map(map.into_iter().collect()));
    }
    let mut pairs = Vec::new();
    for part in split_top(body, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let eq = part.find('=')?;
        let key = part[..eq].trim().to_owned();
        let value = snbt::parse_value(part[eq + 1..].trim())?;
        pairs.push((key, value, part.to_owned()));
    }
    Some(apply_pairs(pairs))
}

fn apply_map(pairs: Vec<(String, Tag)>) -> Parsed {
    apply_pairs(
        pairs
            .into_iter()
            .map(|(k, v)| {
                let shown = format!("{k}={}", snbt::format_tag(&v));
                (k, v, shown)
            })
            .collect(),
    )
}

fn apply_pairs(pairs: Vec<(String, Tag, String)>) -> Parsed {
    let mut extra = Extra::default();
    let mut damage = None;
    let mut potion = None;
    let mut rest = Vec::new();
    for (key, value, shown) in pairs {
        if bare(&key) == "potion_contents" {
            let name = match &value {
                Tag::String(n) => Some(n.clone()),
                Tag::Compound(c) => c.get("potion").and_then(|p| match p {
                    Tag::String(n) => Some(n.clone()),
                    _ => None,
                }),
                _ => None,
            };
            potion = name.map(|n| super::args::resource_name(&n));
        }
        if !apply_one(&mut extra, &mut damage, &key, value) {
            rest.push(shown);
        }
    }
    if !rest.is_empty() {
        extra.rest = Some(rest.join(","));
    }
    (extra, damage, potion)
}

fn apply_one(extra: &mut Extra, damage: &mut Option<i32>, key: &str, value: Tag) -> bool {
    match bare(key) {
        "damage" => {
            *damage = snbt::number(&value).map(|n| n.max(0.0) as i32);
            damage.is_some()
        }
        "custom_name" | "item_name" => {
            extra.custom_name = Some(text_of(&value));
            true
        }
        "lore" => {
            let Some(lines) = value.as_list() else {
                return false;
            };
            extra.lore = lines.iter().map(text_of).collect();
            true
        }
        "enchantments" => {
            extra.enchantments = levels_of(&value);
            true
        }
        "stored_enchantments" => {
            extra.stored = levels_of(&value);
            true
        }
        "banner_patterns" => {
            // Узор без пространства имён — `minecraft:`.
            let Some(layers) = value.as_list() else {
                return false;
            };
            let full: Vec<Tag> = layers
                .iter()
                .map(|l| {
                    let mut l = l.as_compound().cloned().unwrap_or_default();
                    if let Some(Tag::String(p)) = l.get("pattern")
                        && !p.contains(':')
                    {
                        let p = format!("minecraft:{p}");
                        l.insert("pattern".into(), Tag::String(p));
                    }
                    Tag::Compound(l)
                })
                .collect();
            extra.banner_patterns = crate::stack_nbt::banner_layers_from_tag(&Tag::List(full));
            extra.banner_patterns.len() == layers.len()
        }
        "custom_data" => {
            let Some(map) = value.as_compound() else {
                return false;
            };
            if !map.is_empty() {
                extra.custom_data = Some(crate::stack_nbt::compound_to_net(map));
            }
            true
        }
        _ => false,
    }
}

fn bare(key: &str) -> &str {
    key.rsplit(':').next().unwrap_or(key)
}

fn text_of(tag: &Tag) -> String {
    crate::stack_nbt::tag_to_json(tag).to_string()
}

fn levels_of(tag: &Tag) -> Vec<(i32, i32)> {
    let Some(map) = tag.as_compound() else {
        return Vec::new();
    };
    let levels = map.get("levels").and_then(Tag::as_compound).unwrap_or(map);
    let mut out = Vec::new();
    for (name, level) in levels {
        if name == "levels" || name == "show_in_tooltip" {
            continue;
        }
        let Some(id) = components::enchantment_id(name) else {
            continue;
        };
        let Some(level) = snbt::number(level) else {
            continue;
        };
        out.push((id, level.clamp(1.0, 255.0) as i32));
    }
    out
}

/// Делит строку по разделителю вне кавычек и скобок.
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut curly = 0i32;
    let mut square = 0i32;
    let mut quote = None::<char>;
    for (i, c) in text.char_indices() {
        if let Some(q) = quote {
            if c == '\\' {
                continue;
            }
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '{' => curly += 1,
            '}' => curly -= 1,
            '[' => square += 1,
            ']' => square -= 1,
            c if c == sep && curly == 0 && square == 0 => {
                out.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

/// Id предметов тега `#minecraft:dirt`. `None` — такого тега нет.
pub fn tag_ids(
    tags: &HashMap<String, std::collections::HashSet<String>>,
    ids: &HashMap<String, i32>,
    token: &str,
) -> Option<Vec<i32>> {
    let name = token.strip_prefix('#')?;
    let name = super::args::resource_name(name);
    let set = tags.get(&name)?;
    let mut out: Vec<i32> = set
        .iter()
        .filter_map(|item| ids.get(item).copied())
        .collect();
    out.sort_unstable();
    out.dedup();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_damage_and_keeps_the_rest() {
        let ids = HashMap::from([("minecraft:stone".into(), 1)]);
        let parsed = parse_item(
            r#"minecraft:stone[minecraft:custom_name="Ada",minecraft:damage=3,minecraft:potion_contents={}]"#,
            &ids,
        )
        .unwrap();
        assert_eq!(parsed.id, 1);
        assert_eq!(parsed.damage, Some(3));
        let extra = components::get(parsed.extra);
        assert!(extra.custom_name.as_deref().unwrap().contains("Ada"));
        assert!(extra.rest.as_deref().unwrap().contains("potion_contents"));
        assert_eq!(parsed.potion, None);
    }

    #[test]
    fn reads_potion_in_both_forms() {
        let ids = HashMap::from([("minecraft:potion".into(), 7)]);
        for token in [
            r#"minecraft:potion[potion_contents={potion:"minecraft:water"}]"#,
            r#"potion[minecraft:potion_contents="water"]"#,
        ] {
            let parsed = parse_item(token, &ids).unwrap();
            assert_eq!(parsed.potion.as_deref(), Some("minecraft:water"), "{token}");
        }
    }
}
