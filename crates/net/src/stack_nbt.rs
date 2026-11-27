//! Стопка на диске как у ванилы (`ItemStack.CODEC`): `id`, `count` и
//! `components` — износ, чары (`{"minecraft:efficiency": 5}`), имя и описание
//! (текстовые компоненты в NBT).

use chunkwake_protocol::components::{self, Extra};
use chunkwake_protocol::inventory::Stack;
use chunkwake_world::nbt::{Compound, Tag};
use serde_json::Value;

/// `{id, count, components}`; `None` — у предмета нет имени.
/// hex-кодирование байтов NBT для передачи через мост `world`.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 15) as usize] as char);
    }
    out
}

/// Обратное к [`hex`]; `None` — нечётная длина или не hex.
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        out.push((digit(pair[0])? << 4) | digit(pair[1])?);
    }
    Some(out)
}

pub fn to_nbt(s: Stack, name: &str) -> Option<Compound> {
    named_nbt(s, name, &|_| None)
}

/// Как [`to_nbt`], и вложенные снаряды арбалета по именам предметов.
pub fn named_nbt(
    s: Stack,
    name: &str,
    item_name: &dyn Fn(i32) -> Option<String>,
) -> Option<Compound> {
    if name.is_empty() {
        return None;
    }
    let mut c = Compound::from([
        ("DataVersion".into(), Tag::Int(5023)),
        ("id".into(), Tag::String(name.to_owned())),
        ("count".into(), Tag::Int(s.count)),
    ]);
    let mut comps = Compound::new();
    if s.damage > 0 {
        comps.insert("minecraft:damage".into(), Tag::Int(s.damage));
    }
    let e = components::get(s.extra);
    if let Some(book) = &e.book {
        let (key, tag) = book_to_tag(book);
        comps.insert(key.into(), tag);
    } else if let Some(pages) = e.book_pages {
        c.insert("chunkwake:book_pages".into(), Tag::Int(pages));
    }
    if e.repair_cost > 0 {
        comps.insert("minecraft:repair_cost".into(), Tag::Int(e.repair_cost));
    }
    if let Some(id) = e.map_id {
        comps.insert("minecraft:map_id".into(), Tag::Int(id));
    }
    if let Some((material, pattern)) = e.trim.and_then(|(m, p)| components::trim_names(m, p)) {
        comps.insert(
            "minecraft:trim".into(),
            Tag::Compound(Compound::from([
                ("material".into(), Tag::String(material.to_owned())),
                ("pattern".into(), Tag::String(pattern.to_owned())),
            ])),
        );
    }
    if !e.banner_patterns.is_empty() {
        comps.insert(
            "minecraft:banner_patterns".into(),
            banner_layers_to_tag(&e.banner_patterns),
        );
    }
    if let Some(color) = e.base_color
        && let Some(name) = components::DYE_COLORS.get(color as usize)
    {
        comps.insert(
            "minecraft:base_color".into(),
            Tag::String((*name).to_owned()),
        );
    }
    if let Some(sound) = &e.note_block_sound {
        comps.insert(
            "minecraft:note_block_sound".into(),
            Tag::String(sound.clone()),
        );
    }
    if let Some(rarity) = e.rarity
        && let Some(name) = components::RARITY_NAMES.get(rarity as usize)
    {
        comps.insert("minecraft:rarity".into(), Tag::String((*name).to_owned()));
    }
    if let Some(lock) = &e.lock
        && let Some(tag) = crate::play::commands::snbt::parse_value(lock)
    {
        comps.insert("minecraft:lock".into(), tag);
    }
    if let Some(loot) = &e.container_loot
        && let Some(tag) = crate::play::commands::snbt::parse_value(loot)
    {
        comps.insert("minecraft:container_loot".into(), tag);
    }
    if let Some(data) = &e.block_entity_data
        && let Some(block) = components::block_entity_type_name(data.kind)
        && let Some(mut c) = net_to_compound(&data.tag)
    {
        c.insert("id".into(), Tag::String(block.to_owned()));
        comps.insert("minecraft:block_entity_data".into(), Tag::Compound(c));
    }
    if let Some(data) = &e.entity_data
        && let Some(entity) = components::entity_type_name(data.kind)
        && let Some(mut c) = net_to_compound(&data.tag)
    {
        c.insert("id".into(), Tag::String(entity.to_owned()));
        comps.insert("minecraft:entity_data".into(), Tag::Compound(c));
    }
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
    if !e.stored.is_empty() {
        comps.insert(
            "minecraft:stored_enchantments".into(),
            Tag::Compound(levels_nbt(&e.stored)),
        );
    }
    if let Some(n) = &e.custom_name {
        comps.insert("minecraft:custom_name".into(), json_to_tag(&parse(n)));
    }
    if let Some(n) = &e.item_name {
        comps.insert("minecraft:item_name".into(), json_to_tag(&parse(n)));
    }
    if let Some(m) = &e.item_model {
        comps.insert("minecraft:item_model".into(), Tag::String(m.clone()));
    }
    if let Some(t) = &e.tooltip_style {
        comps.insert("minecraft:tooltip_style".into(), Tag::String(t.clone()));
    }
    if e.hide_tooltip {
        let mut display = Compound::new();
        display.insert("hide_tooltip".into(), Tag::Byte(1));
        display.insert("hidden_components".into(), Tag::List(Vec::new()));
        comps.insert("minecraft:tooltip_display".into(), Tag::Compound(display));
    }
    if let Some(v) = e.glint_override {
        comps.insert(
            "minecraft:enchantment_glint_override".into(),
            Tag::Byte(i8::from(v)),
        );
    }
    if let Some(name) = e.potion.and_then(components::potion_name) {
        let mut contents = Compound::new();
        contents.insert("potion".into(), Tag::String(name.to_owned()));
        comps.insert("minecraft:potion_contents".into(), Tag::Compound(contents));
    }
    if let Some(fw) = &e.fireworks {
        let mut fireworks = Compound::new();
        fireworks.insert("flight_duration".into(), Tag::Byte(fw.flight as i8));
        let explosions = fw
            .explosions
            .iter()
            .map(|ex| {
                let mut c = Compound::new();
                c.insert("shape".into(), Tag::String(firework_shape_name(ex.shape)));
                c.insert(
                    "colors".into(),
                    Tag::List(ex.colors.iter().copied().map(Tag::Int).collect()),
                );
                c.insert(
                    "fade_colors".into(),
                    Tag::List(ex.fade.iter().copied().map(Tag::Int).collect()),
                );
                c.insert("has_trail".into(), Tag::Byte(i8::from(ex.trail)));
                c.insert("has_twinkle".into(), Tag::Byte(i8::from(ex.twinkle)));
                Tag::Compound(c)
            })
            .collect();
        fireworks.insert("explosions".into(), Tag::List(explosions));
        comps.insert("minecraft:fireworks".into(), Tag::Compound(fireworks));
    }
    if let Some(name) = e.instrument.and_then(components::instrument_name) {
        comps.insert("minecraft:instrument".into(), Tag::String(name.to_owned()));
    }
    if let Some(value) = e.custom_model_data {
        // Legacy int хранится первым float компонента (`CustomModelData.CODEC`).
        let mut cmd = Compound::new();
        cmd.insert("floats".into(), Tag::List(vec![Tag::Float(value as f32)]));
        comps.insert("minecraft:custom_model_data".into(), Tag::Compound(cmd));
    }
    if e.unbreakable {
        // `minecraft:unbreakable` — unit-компонент: пустой compound.
        comps.insert(
            "minecraft:unbreakable".into(),
            Tag::Compound(Compound::new()),
        );
    }
    if !e.lore.is_empty() {
        let lines = e.lore.iter().map(|l| json_to_tag(&parse(l))).collect();
        comps.insert("minecraft:lore".into(), Tag::List(lines));
    }
    if !e.charged.is_empty() {
        let list = e
            .charged
            .iter()
            .filter_map(|st| {
                let name = item_name(st.item)?;
                named_nbt(*st, &name, item_name).map(Tag::Compound)
            })
            .collect();
        comps.insert("minecraft:charged_projectiles".into(), Tag::List(list));
    }
    if !e.container.is_empty() {
        let list = e
            .container
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let st = (*slot)?;
                let name = item_name(st.item)?;
                let item = named_nbt(st, &name, item_name)?;
                Some(Tag::Compound(Compound::from([
                    ("slot".into(), Tag::Int(index as i32)),
                    ("item".into(), Tag::Compound(item)),
                ])))
            })
            .collect();
        comps.insert("minecraft:container".into(), Tag::List(list));
    }
    if let Some(color) = e.dyed_color {
        comps.insert("minecraft:dyed_color".into(), Tag::Int(color));
    }
    if !comps.is_empty() {
        c.insert("components".into(), Tag::Compound(comps));
    }
    if let Some(custom) = e.custom_data.as_deref().and_then(net_to_compound) {
        c.entry("components".into())
            .or_insert_with(|| Tag::Compound(Compound::new()));
        if let Some(Tag::Compound(comps)) = c.get_mut("components") {
            comps.insert("minecraft:custom_data".into(), Tag::Compound(custom));
        }
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
    from_nbt_ref(c, &item_id, 0)
}

fn from_nbt_ref(c: &Compound, item_id: &dyn Fn(&str) -> Option<i32>, depth: u32) -> Option<Stack> {
    if depth > 4 {
        return None;
    }
    let item = item_id(c.get("id")?.as_str()?)?;
    let count = c.get("count").and_then(Tag::as_i64).unwrap_or(1) as i32;
    if count <= 0 {
        return None;
    }
    let mut s = Stack::new(item, count);
    let mut e = Extra {
        book_pages: c
            .get("chunkwake:book_pages")
            .and_then(Tag::as_i64)
            .map(|n| n.clamp(0, 100) as i32),
        ..Extra::default()
    };
    if let Some(Tag::Compound(m)) = c.get("components") {
        e.book = book_from_components(m);
        if let Some(book) = &e.book {
            e.book_pages = Some(book.page_count());
        }
        if e.book_pages.is_none() {
            e.book_pages = [
                "minecraft:written_book_content",
                "minecraft:writable_book_content",
            ]
            .iter()
            .find_map(|name| {
                m.get(*name)
                    .and_then(Tag::as_compound)
                    .and_then(|book| book.get("pages"))
                    .and_then(Tag::as_list)
                    .map(|pages| pages.len().clamp(1, 100) as i32)
            });
        }
        e.repair_cost = m
            .get("minecraft:repair_cost")
            .and_then(Tag::as_i64)
            .unwrap_or(0)
            .clamp(0, i32::MAX as i64) as i32;
        e.map_id = m
            .get("minecraft:map_id")
            .and_then(Tag::as_i64)
            .map(|v| v as i32);
        e.dyed_color = m.get("minecraft:dyed_color").and_then(|t| match t {
            Tag::Int(v) => Some(*v),
            other => other.as_i64().map(|v| v as i32),
        });
        e.trim = m
            .get("minecraft:trim")
            .and_then(Tag::as_compound)
            .and_then(|trim| {
                components::trim_ids(
                    trim.get("material")?.as_str()?,
                    trim.get("pattern")?.as_str()?,
                )
            });
        if let Some(layers) = m.get("minecraft:banner_patterns") {
            e.banner_patterns = banner_layers_from_tag(layers);
        }
        e.base_color = m
            .get("minecraft:base_color")
            .and_then(Tag::as_str)
            .and_then(|name| {
                components::DYE_COLORS
                    .iter()
                    .position(|&c| c == name)
                    .map(|i| i as i32)
            });
        e.note_block_sound = m
            .get("minecraft:note_block_sound")
            .and_then(Tag::as_str)
            .map(str::to_owned);
        e.rarity = m
            .get("minecraft:rarity")
            .and_then(Tag::as_str)
            .and_then(|name| components::RARITY_NAMES.iter().position(|&n| n == name))
            .map(|i| i as u8);
        e.lock = m
            .get("minecraft:lock")
            .map(crate::play::commands::snbt::format_tag);
        e.container_loot = m
            .get("minecraft:container_loot")
            .map(crate::play::commands::snbt::format_tag);
        e.block_entity_data = m
            .get("minecraft:block_entity_data")
            .and_then(Tag::as_compound)
            .and_then(|c| {
                let kind = components::block_entity_type_id(c.get("id")?.as_str()?)?;
                let mut data = c.clone();
                data.remove("id");
                Some(components::BlockEntityData {
                    kind,
                    tag: compound_to_net(&data),
                })
            });
        e.entity_data = m
            .get("minecraft:entity_data")
            .and_then(Tag::as_compound)
            .and_then(|c| {
                let kind = components::entity_type_id(c.get("id")?.as_str()?)?;
                let mut data = c.clone();
                data.remove("id");
                Some(components::BlockEntityData {
                    kind,
                    tag: compound_to_net(&data),
                })
            });
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
        if let Some(Tag::Compound(levels)) = m.get("minecraft:stored_enchantments") {
            e.stored = levels
                .iter()
                .filter_map(|(name, lvl)| {
                    Some((components::enchantment_id(name)?, lvl.as_i64()? as i32))
                })
                .collect();
        }
        e.custom_name = m
            .get("minecraft:custom_name")
            .map(|t| tag_to_json(t).to_string());
        e.item_name = m
            .get("minecraft:item_name")
            .map(|t| tag_to_json(t).to_string());
        e.item_model = m
            .get("minecraft:item_model")
            .and_then(Tag::as_str)
            .map(str::to_owned);
        e.tooltip_style = m
            .get("minecraft:tooltip_style")
            .and_then(Tag::as_str)
            .map(str::to_owned);
        e.hide_tooltip = m
            .get("minecraft:tooltip_display")
            .and_then(Tag::as_compound)
            .and_then(|c| c.get("hide_tooltip"))
            .and_then(Tag::as_i64)
            .is_some_and(|v| v != 0);
        e.glint_override = m
            .get("minecraft:enchantment_glint_override")
            .and_then(Tag::as_i64)
            .map(|v| v != 0);
        e.potion = m
            .get("minecraft:potion_contents")
            .and_then(Tag::as_compound)
            .and_then(|c| c.get("potion"))
            .and_then(Tag::as_str)
            .and_then(components::potion_id);
        e.fireworks = m
            .get("minecraft:fireworks")
            .and_then(Tag::as_compound)
            .map(|c| components::Fireworks {
                flight: c.get("flight_duration").and_then(Tag::as_i64).unwrap_or(0) as i32,
                explosions: c
                    .get("explosions")
                    .and_then(Tag::as_list)
                    .map(|list| {
                        list.iter()
                            .filter_map(Tag::as_compound)
                            .map(|ex| components::Explosion {
                                shape: ex
                                    .get("shape")
                                    .and_then(Tag::as_str)
                                    .and_then(firework_shape_id)
                                    .unwrap_or(0),
                                colors: int_list(ex.get("colors")),
                                fade: int_list(ex.get("fade_colors")),
                                trail: ex
                                    .get("has_trail")
                                    .and_then(Tag::as_i64)
                                    .is_some_and(|v| v != 0),
                                twinkle: ex
                                    .get("has_twinkle")
                                    .and_then(Tag::as_i64)
                                    .is_some_and(|v| v != 0),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        e.instrument = m
            .get("minecraft:instrument")
            .and_then(Tag::as_str)
            .and_then(components::instrument_id);
        e.custom_model_data = m
            .get("minecraft:custom_model_data")
            .and_then(Tag::as_compound)
            .and_then(|c| c.get("floats"))
            .and_then(Tag::as_list)
            .and_then(|floats| floats.first())
            .and_then(|t| match t {
                Tag::Float(v) => Some(*v as i32),
                Tag::Double(v) => Some(*v as i32),
                other => other.as_i64().map(|v| v as i32),
            });
        e.unbreakable = m.contains_key("minecraft:unbreakable");
        if let Some(lines) = m.get("minecraft:lore").and_then(Tag::as_list) {
            e.lore = lines.iter().map(|t| tag_to_json(t).to_string()).collect();
        }
        e.custom_data = m
            .get("minecraft:custom_data")
            .and_then(Tag::as_compound)
            .filter(|c| !c.is_empty())
            .map(compound_to_net);
        if let Some(list) = m
            .get("minecraft:charged_projectiles")
            .and_then(Tag::as_list)
        {
            e.charged = list
                .iter()
                .filter_map(|t| from_nbt_ref(t.as_compound()?, item_id, depth + 1))
                .collect();
        }
        if let Some(list) = m.get("minecraft:container").and_then(Tag::as_list) {
            for entry in list.iter().take(256) {
                let Some(slot) = entry.as_compound() else {
                    continue;
                };
                let Some(index) = slot.get("slot").and_then(Tag::as_i64) else {
                    continue;
                };
                if !(0..=255).contains(&index) {
                    continue;
                }
                let Some(stack) = slot
                    .get("item")
                    .and_then(Tag::as_compound)
                    .and_then(|item| from_nbt_ref(item, item_id, depth + 1))
                else {
                    continue;
                };
                let index = index as usize;
                if e.container.len() <= index {
                    e.container.resize(index + 1, None);
                }
                e.container[index] = Some(stack);
            }
        }
    }
    e.raw = c
        .get("chunkwake:component_patch")
        .and_then(Tag::as_byte_array)
        .map(|b| b.iter().map(|&x| x as u8).collect());
    s.extra = components::intern(e);
    Some(s)
}

/// `Filterable.codec`: всегда полной записью `{raw, filtered?}`.
fn filterable_tag(v: &components::Filterable, value: &dyn Fn(&str) -> Tag) -> Tag {
    let mut c = Compound::from([("raw".into(), value(&v.0))]);
    if let Some(f) = &v.1 {
        c.insert("filtered".into(), value(f));
    }
    Tag::Compound(c)
}

/// Обратно; принимает и голое значение (`Filterable.passThrough`).
fn filterable_from(
    t: &Tag,
    value: &dyn Fn(&Tag) -> Option<String>,
) -> Option<components::Filterable> {
    match t {
        Tag::Compound(c) if c.contains_key("raw") => {
            Some((value(c.get("raw")?)?, c.get("filtered").and_then(value)))
        }
        other => Some((value(other)?, None)),
    }
}

/// Компонент книги на диске: страницы подписанной — JSON строкой
/// (`ComponentSerialization.flatRestrictedCodec`).
fn book_to_tag(b: &components::Book) -> (&'static str, Tag) {
    let string = |s: &str| Tag::String(s.to_owned());
    let pages = Tag::List(b.pages.iter().map(|p| filterable_tag(p, &string)).collect());
    let mut c = Compound::new();
    if !b.pages.is_empty() {
        c.insert("pages".into(), pages);
    }
    if !b.written {
        return ("minecraft:writable_book_content", Tag::Compound(c));
    }
    c.insert("title".into(), filterable_tag(&b.title, &string));
    c.insert("author".into(), Tag::String(b.author.clone()));
    if b.generation != 0 {
        c.insert("generation".into(), Tag::Int(b.generation));
    }
    if b.resolved {
        c.insert("resolved".into(), Tag::Byte(1));
    }
    ("minecraft:written_book_content", Tag::Compound(c))
}

/// Книга из `components`: подписанная важнее черновой.
fn book_from_components(m: &Compound) -> Option<components::Book> {
    let string = |t: &Tag| t.as_str().map(str::to_owned);
    // Страница подписанной: JSON строкой или текстовым компонентом NBT.
    let text = |t: &Tag| match t {
        Tag::String(s) => Some(
            serde_json::from_str::<Value>(s)
                .unwrap_or_else(|_| Value::String(s.clone()))
                .to_string(),
        ),
        other => Some(tag_to_json(other).to_string()),
    };
    let pages = |c: &Compound, value: &dyn Fn(&Tag) -> Option<String>| {
        c.get("pages")
            .and_then(Tag::as_list)
            .map(|l| {
                l.iter()
                    .take(100)
                    .filter_map(|p| filterable_from(p, value))
                    .collect()
            })
            .unwrap_or_default()
    };
    if let Some(c) = m
        .get("minecraft:written_book_content")
        .and_then(Tag::as_compound)
    {
        return Some(components::Book {
            written: true,
            pages: pages(c, &text),
            title: c
                .get("title")
                .and_then(|t| filterable_from(t, &string))
                .unwrap_or_default(),
            author: c
                .get("author")
                .and_then(Tag::as_str)
                .unwrap_or_default()
                .to_owned(),
            generation: c
                .get("generation")
                .and_then(Tag::as_i64)
                .unwrap_or(0)
                .clamp(0, 3) as i32,
            resolved: c.get("resolved").and_then(Tag::as_i64).unwrap_or(0) != 0,
        });
    }
    let c = m.get("minecraft:writable_book_content")?.as_compound()?;
    Some(components::Book::writable(pages(c, &string)))
}

/// Книга для моста: страницы черновой — строки, подписанной — JSON компонентов.
fn book_to_json(b: &components::Book) -> Value {
    let pages: Vec<Value> = b
        .pages
        .iter()
        .map(|(raw, _)| {
            if b.written {
                parse(raw)
            } else {
                Value::String(raw.clone())
            }
        })
        .collect();
    let mut j = serde_json::json!({"written": b.written, "pages": pages});
    if b.written {
        j["title"] = b.title.0.clone().into();
        j["author"] = b.author.clone().into();
        j["generation"] = b.generation.into();
        j["resolved"] = b.resolved.into();
    }
    j
}

fn book_from_json(j: &Value) -> Option<components::Book> {
    let written = j.get("written")?.as_bool()?;
    let pages = j
        .get("pages")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .take(100)
                .map(|p| {
                    let text = match p {
                        Value::String(s) if !written => s.clone(),
                        other => other.to_string(),
                    };
                    (text, None)
                })
                .collect()
        })
        .unwrap_or_default();
    Some(components::Book {
        written,
        pages,
        title: (
            j.get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            None,
        ),
        author: j
            .get("author")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        generation: j
            .get("generation")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 3) as i32,
        resolved: j.get("resolved").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// NBT-предикат в JSON для моста плагинов (и обратно).
fn snbt_to_json(text: &str) -> Option<Value> {
    crate::play::commands::snbt::parse_value(text).map(|tag| tag_to_json(&tag))
}

fn json_to_snbt(value: &Value) -> String {
    crate::play::commands::snbt::format_tag(&json_to_tag(value))
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
    if e.repair_cost > 0 {
        j.insert("repair_cost".into(), e.repair_cost.into());
    }
    if let Some(id) = e.map_id {
        j.insert("map_id".into(), id.into());
    }
    if let Some((material, pattern)) = e.trim.and_then(|(m, p)| components::trim_names(m, p)) {
        j.insert(
            "trim".into(),
            serde_json::json!({"material": material, "pattern": pattern}),
        );
    }
    if !e.banner_patterns.is_empty() {
        let layers: Vec<_> = e
            .banner_patterns
            .iter()
            .filter_map(|&(pattern, color)| {
                Some(serde_json::json!({
                    "pattern": components::banner_pattern_name(pattern)?,
                    "color": components::DYE_COLORS.get(color as usize)?,
                }))
            })
            .collect();
        j.insert("banner_patterns".into(), Value::Array(layers));
    }
    if let Some(color) = e.base_color
        && let Some(name) = components::DYE_COLORS.get(color as usize)
    {
        j.insert("base_color".into(), Value::from(*name));
    }
    if let Some(sound) = &e.note_block_sound {
        j.insert("note_block_sound".into(), Value::from(sound.clone()));
    }
    if let Some(rarity) = e.rarity
        && let Some(name) = components::RARITY_NAMES.get(rarity as usize)
    {
        j.insert("rarity".into(), Value::from(*name));
    }
    if let Some(value) = e.lock.as_deref().and_then(snbt_to_json) {
        j.insert("lock".into(), value);
    }
    if let Some(value) = e.container_loot.as_deref().and_then(snbt_to_json) {
        j.insert("container_loot".into(), value);
    }
    if let Some(data) = &e.block_entity_data {
        j.insert(
            "block_entity_data".into(),
            serde_json::json!({"kind": data.kind, "tag": hex(&data.tag)}),
        );
    }
    if let Some(data) = &e.entity_data {
        j.insert(
            "entity_data".into(),
            serde_json::json!({"kind": data.kind, "tag": hex(&data.tag)}),
        );
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
    if !e.stored.is_empty() {
        let levels = levels_nbt(&e.stored)
            .iter()
            .map(|(k, v)| (k.clone(), tag_to_json(v)))
            .collect();
        j.insert("stored".into(), Value::Object(levels));
    }
    if let Some(n) = &e.custom_name {
        j.insert("name".into(), parse(n));
    }
    if let Some(n) = &e.item_name {
        j.insert("item_name".into(), parse(n));
    }
    if let Some(m) = &e.item_model {
        j.insert("item_model".into(), Value::from(m.clone()));
    }
    if let Some(t) = &e.tooltip_style {
        j.insert("tooltip_style".into(), Value::from(t.clone()));
    }
    if e.hide_tooltip {
        j.insert("hide_tooltip".into(), Value::Bool(true));
    }
    if let Some(v) = e.glint_override {
        j.insert("glint_override".into(), Value::Bool(v));
    }
    if let Some(name) = e.potion.and_then(components::potion_name) {
        j.insert("potion".into(), Value::from(name));
    }
    if let Some(fw) = &e.fireworks {
        let explosions: Vec<Value> = fw
            .explosions
            .iter()
            .map(|ex| {
                serde_json::json!({
                    "shape": ex.shape,
                    "colors": ex.colors,
                    "fade": ex.fade,
                    "trail": ex.trail,
                    "twinkle": ex.twinkle,
                })
            })
            .collect();
        j.insert(
            "fireworks".into(),
            serde_json::json!({"flight": fw.flight, "explosions": explosions}),
        );
    }
    if let Some(name) = e.instrument.and_then(components::instrument_name) {
        j.insert("instrument".into(), Value::from(name));
    }
    if let Some(value) = e.custom_model_data {
        j.insert("custom_model_data".into(), Value::from(value));
    }
    if e.unbreakable {
        j.insert("unbreakable".into(), Value::Bool(true));
    }
    if !e.lore.is_empty() {
        j.insert("lore".into(), e.lore.iter().map(|l| parse(l)).collect());
    }
    if let Some(book) = &e.book {
        j.insert("book".into(), book_to_json(book));
    }
    if let Some(custom) = e.custom_data.as_deref().and_then(net_to_compound) {
        j.insert("custom".into(), typed_compound(&custom));
    }
    // Непонятые сервером компоненты (зелья и т. п.) — шестнадцатеричной строкой, чтобы вернулись целыми.
    if !e.charged.is_empty() {
        j.insert(
            "charged".into(),
            e.charged
                .iter()
                .map(|st| {
                    serde_json::json!({
                        "item": st.item,
                        "count": st.count,
                        "damage": st.damage,
                        "extra": st.extra,
                    })
                })
                .collect(),
        );
    }
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
        let mut e = Extra {
            repair_cost: j
                .get("repair_cost")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .clamp(0, i32::MAX as i64) as i32,
            ..Extra::default()
        };
        e.map_id = j.get("map_id").and_then(Value::as_i64).map(|v| v as i32);
        e.trim = j.get("trim").and_then(|trim| {
            components::trim_ids(
                trim.get("material")?.as_str()?,
                trim.get("pattern")?.as_str()?,
            )
        });
        if let Some(layers) = j.get("banner_patterns").and_then(Value::as_array) {
            e.banner_patterns = layers
                .iter()
                .filter_map(|layer| {
                    let pattern = components::banner_pattern_id(layer.get("pattern")?.as_str()?)?;
                    let color_name = layer.get("color")?.as_str()?;
                    let color = components::DYE_COLORS
                        .iter()
                        .position(|&n| n == color_name)? as i32;
                    Some((pattern, color))
                })
                .collect();
        }
        e.base_color = j
            .get("base_color")
            .and_then(Value::as_str)
            .and_then(|name| {
                components::DYE_COLORS
                    .iter()
                    .position(|&c| c == name)
                    .map(|i| i as i32)
            });
        e.note_block_sound = j
            .get("note_block_sound")
            .and_then(Value::as_str)
            .map(str::to_owned);
        e.rarity = j
            .get("rarity")
            .and_then(Value::as_str)
            .and_then(|name| components::RARITY_NAMES.iter().position(|&n| n == name))
            .map(|i| i as u8);
        e.lock = j.get("lock").map(json_to_snbt);
        e.container_loot = j.get("container_loot").map(json_to_snbt);
        e.block_entity_data = j.get("block_entity_data").and_then(|v| {
            Some(components::BlockEntityData {
                kind: v.get("kind")?.as_i64()? as i32,
                tag: unhex(v.get("tag")?.as_str()?)?,
            })
        });
        e.entity_data = j.get("entity_data").and_then(|v| {
            Some(components::BlockEntityData {
                kind: v.get("kind")?.as_i64()? as i32,
                tag: unhex(v.get("tag")?.as_str()?)?,
            })
        });
        if let Some(Value::Object(levels)) = j.get("enchantments") {
            for (name, lvl) in levels {
                if let (Some(id), Some(lvl)) = (components::enchantment_id(name), lvl.as_i64()) {
                    e.enchantments.push((id, lvl.clamp(1, 255) as i32));
                }
            }
        }
        if let Some(Value::Object(levels)) = j.get("stored") {
            e.stored = levels
                .iter()
                .filter_map(|(name, lvl)| {
                    Some((
                        components::enchantment_id(name)?,
                        lvl.as_i64()?.clamp(1, 255) as i32,
                    ))
                })
                .collect();
        }
        e.custom_name = j.get("name").filter(|v| !v.is_null()).map(Value::to_string);
        e.item_name = j
            .get("item_name")
            .filter(|v| !v.is_null())
            .map(Value::to_string);
        e.item_model = j
            .get("item_model")
            .and_then(Value::as_str)
            .map(str::to_owned);
        e.tooltip_style = j
            .get("tooltip_style")
            .and_then(Value::as_str)
            .map(str::to_owned);
        e.hide_tooltip = j
            .get("hide_tooltip")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        e.glint_override = j.get("glint_override").and_then(Value::as_bool);
        e.potion = j
            .get("potion")
            .and_then(Value::as_str)
            .and_then(components::potion_id);
        e.fireworks = j.get("fireworks").map(|fw| components::Fireworks {
            flight: fw.get("flight").and_then(Value::as_i64).unwrap_or(0) as i32,
            explosions: fw
                .get("explosions")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .map(|ex| components::Explosion {
                            shape: ex.get("shape").and_then(Value::as_u64).unwrap_or(0) as u8,
                            colors: json_ints(ex.get("colors")),
                            fade: json_ints(ex.get("fade")),
                            trail: ex.get("trail").and_then(Value::as_bool).unwrap_or(false),
                            twinkle: ex.get("twinkle").and_then(Value::as_bool).unwrap_or(false),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        });
        e.instrument = j
            .get("instrument")
            .and_then(Value::as_str)
            .and_then(components::instrument_id);
        e.custom_model_data = j
            .get("custom_model_data")
            .and_then(Value::as_i64)
            .map(|v| v as i32);
        e.unbreakable = j
            .get("unbreakable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(Value::Array(lore)) = j.get("lore") {
            e.lore = lore.iter().map(Value::to_string).collect();
        }
        e.custom_data = j
            .get("custom")
            .and_then(untyped_compound)
            .filter(|c| !c.is_empty())
            .map(|c| compound_to_net(&c));
        if let Some(Value::Array(list)) = j.get("charged") {
            e.charged = list
                .iter()
                .filter_map(|v| {
                    Some(Stack {
                        item: v.get("item")?.as_i64()? as i32,
                        count: v.get("count").and_then(Value::as_i64).unwrap_or(1) as i32,
                        damage: v.get("damage").and_then(Value::as_i64).unwrap_or(0).max(0) as i32,
                        extra: v.get("extra").and_then(Value::as_i64).unwrap_or(0) as u32,
                    })
                })
                .filter(|st| st.count > 0)
                .collect();
        }
        e.book = j.get("book").and_then(book_from_json);
        if let Some(book) = &e.book {
            e.book_pages = Some(book.page_count());
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

/// Сетевой NBT (`10` + содержимое) → compound.
pub fn net_to_compound(net: &[u8]) -> Option<Compound> {
    let mut named = vec![10, 0, 0];
    named.extend_from_slice(net.get(1..)?);
    chunkwake_world::nbt::read_root(&named).ok()
}

/// Compound → сетевой NBT (корень без имени).
pub fn compound_to_net(c: &Compound) -> Vec<u8> {
    let named = chunkwake_world::nbt::write_root(c);
    let mut out = vec![10];
    out.extend_from_slice(&named[3..]);
    out
}

/// NBT для моста с типами: `{"b":1}`, `{"s":..}`, `{"i":..}`, `{"l":"..."}`,
/// `{"f":..}`, `{"d":..}`, `{"str":".."}`, `{"ba":[..]}`, `{"ia":[..]}`,
/// `{"la":["..."]}`, `{"list":[..]}`, `{"c":{..}}`. Compound — объект без обёртки.
fn typed(t: &Tag) -> Value {
    let one = |k: &str, v: Value| Value::Object(serde_json::Map::from_iter([(k.to_owned(), v)]));
    match t {
        Tag::Byte(v) => one("b", (*v).into()),
        Tag::Short(v) => one("s", (*v).into()),
        Tag::Int(v) => one("i", (*v).into()),
        Tag::Long(v) => one("l", v.to_string().into()),
        Tag::Float(v) => one("f", (*v as f64).into()),
        Tag::Double(v) => one("d", (*v).into()),
        Tag::String(v) => one("str", v.clone().into()),
        Tag::ByteArray(v) => one("ba", v.iter().map(|&x| Value::from(x)).collect()),
        Tag::IntArray(v) => one("ia", v.iter().map(|&x| Value::from(x)).collect()),
        Tag::LongArray(v) => one("la", v.iter().map(|x| Value::from(x.to_string())).collect()),
        Tag::List(v) => one("list", v.iter().map(typed).collect()),
        Tag::Compound(c) => one("c", typed_compound(c)),
    }
}

pub(crate) fn typed_compound(c: &Compound) -> Value {
    Value::Object(c.iter().map(|(k, v)| (k.clone(), typed(v))).collect())
}

fn untyped(v: &Value) -> Option<Tag> {
    let (k, v) = v.as_object()?.iter().next()?;
    let arr = || v.as_array().map(Vec::as_slice).unwrap_or_default();
    Some(match k.as_str() {
        "b" => Tag::Byte(v.as_i64()? as i8),
        "s" => Tag::Short(v.as_i64()? as i16),
        "i" => Tag::Int(v.as_i64()? as i32),
        "l" => Tag::Long(v.as_str()?.parse().ok()?),
        "f" => Tag::Float(v.as_f64()? as f32),
        "d" => Tag::Double(v.as_f64()?),
        "str" => Tag::String(v.as_str()?.to_owned()),
        "ba" => Tag::ByteArray(
            arr()
                .iter()
                .filter_map(|x| Some(x.as_i64()? as i8))
                .collect(),
        ),
        "ia" => Tag::IntArray(
            arr()
                .iter()
                .filter_map(|x| Some(x.as_i64()? as i32))
                .collect(),
        ),
        "la" => Tag::LongArray(
            arr()
                .iter()
                .filter_map(|x| x.as_str()?.parse().ok())
                .collect(),
        ),
        "list" => Tag::List(arr().iter().filter_map(untyped).collect()),
        "c" => Tag::Compound(untyped_compound(v)?),
        _ => return None,
    })
}

pub(crate) fn untyped_compound(v: &Value) -> Option<Compound> {
    Some(
        v.as_object()?
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), untyped(v)?)))
            .collect(),
    )
}

/// Чары `(ID, уровень)` → `{имя: уровень}`.
fn levels_nbt(list: &[(i32, i32)]) -> Compound {
    list.iter()
        .filter_map(|&(id, lvl)| {
            Some((components::enchantment_name(id)?.to_owned(), Tag::Int(lvl)))
        })
        .collect()
}

/// Текстовый компонент из сохранённой JSON-строки (как `custom_name`).
pub fn text_json(text: &str) -> Value {
    parse(text)
}

/// Слои `banner_patterns` → список NBT `{pattern, color}`.
pub fn banner_layers_to_tag(layers: &[(i32, i32)]) -> Tag {
    Tag::List(
        layers
            .iter()
            .filter_map(|&(pattern, color)| {
                Some(Tag::Compound(Compound::from([
                    (
                        "pattern".into(),
                        Tag::String(components::banner_pattern_name(pattern)?.to_owned()),
                    ),
                    (
                        "color".into(),
                        Tag::String(components::DYE_COLORS.get(color as usize)?.to_string()),
                    ),
                ])))
            })
            .collect(),
    )
}

/// Список NBT `{pattern, color}` → слои `banner_patterns`.
pub fn banner_layers_from_tag(t: &Tag) -> Vec<(i32, i32)> {
    t.as_list()
        .map(|l| {
            l.iter()
                .filter_map(|layer| {
                    let layer = layer.as_compound()?;
                    let pattern = components::banner_pattern_id(layer.get("pattern")?.as_str()?)?;
                    let name = layer.get("color")?.as_str()?;
                    Some((
                        pattern,
                        components::DYE_COLORS.iter().position(|&n| n == name)? as i32,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn firework_shape_name(shape: u8) -> String {
    match shape {
        1 => "large_ball".into(),
        2 => "star".into(),
        3 => "creeper".into(),
        4 => "burst".into(),
        _ => "small_ball".into(),
    }
}

fn firework_shape_id(name: &str) -> Option<u8> {
    Some(match name {
        "small_ball" => 0,
        "large_ball" => 1,
        "star" => 2,
        "creeper" => 3,
        "burst" => 4,
        _ => return None,
    })
}

fn int_list(tag: Option<&Tag>) -> Vec<i32> {
    tag.and_then(Tag::as_list)
        .map(|l| l.iter().filter_map(Tag::as_i64).map(|v| v as i32).collect())
        .unwrap_or_default()
}

fn json_ints(v: Option<&Value>) -> Vec<i32> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_i64)
                .map(|v| v as i32)
                .collect()
        })
        .unwrap_or_default()
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
    fn hex_roundtrip() {
        let bytes: Vec<u8> = (0u8..=255).collect();
        let text = hex(&bytes);
        assert_eq!(text.len(), 512);
        assert_eq!(unhex(&text).as_deref(), Some(bytes.as_slice()));
        assert!(unhex("abc").is_none());
        assert!(unhex("zz").is_none());
        assert_eq!(hex(&[]), "");
        assert_eq!(unhex("").as_deref(), Some(&[][..]));
    }

    #[test]
    fn banner_patterns_round_trip_through_playerdata() {
        components::set_banner_pattern_names(vec!["minecraft:stripe_bottom".into()]);
        let stack = Stack {
            extra: components::intern(Extra {
                banner_patterns: vec![(0, 14)],
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let nbt = to_nbt(stack, "minecraft:white_banner").unwrap();
        let back = from_nbt(&nbt, |n| (n == "minecraft:white_banner").then_some(7)).unwrap();
        assert_eq!(back, stack);
    }

    #[test]
    fn base_color_round_trips_through_playerdata() {
        components::set_banner_pattern_names(vec!["minecraft:stripe_bottom".into()]);
        let stack = Stack {
            extra: components::intern(Extra {
                banner_patterns: vec![(0, 14)],
                base_color: Some(14),
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let nbt = to_nbt(stack, "minecraft:red_banner").unwrap();
        let back = from_nbt(&nbt, |n| (n == "minecraft:red_banner").then_some(7)).unwrap();
        assert_eq!(back, stack);
    }

    #[test]
    fn rarity_round_trips_through_playerdata() {
        let stack = Stack {
            extra: components::intern(Extra {
                rarity: Some(3),
                ..Default::default()
            }),
            ..Stack::new(1, 1)
        };
        let nbt = to_nbt(stack, "minecraft:diamond_sword").unwrap();
        let back = from_nbt(&nbt, |n| (n == "minecraft:diamond_sword").then_some(1)).unwrap();
        assert_eq!(back, stack);
    }

    #[test]
    fn note_block_sound_round_trips_through_playerdata() {
        let stack = Stack {
            extra: components::intern(Extra {
                note_block_sound: Some("minecraft:block.note_block.basedrum".into()),
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let nbt = to_nbt(stack, "minecraft:player_head").unwrap();
        let back = from_nbt(&nbt, |n| (n == "minecraft:player_head").then_some(7)).unwrap();
        assert_eq!(back, stack);
    }

    #[test]
    fn book_page_count_survives_playerdata_and_lectern_nbt() {
        let stack = Stack {
            extra: components::intern(Extra {
                book_pages: Some(5),
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let nbt = to_nbt(stack, "minecraft:written_book").unwrap();
        assert_eq!(crate::redstone::book_pages(&nbt), 5);
        let restored = from_nbt(&nbt, |n| (n == "minecraft:written_book").then_some(7)).unwrap();
        assert_eq!(components::get(restored.extra).book_pages, Some(5));
    }

    #[test]
    fn books_round_trip_through_nbt_and_bridge_line() {
        let book = components::Book {
            written: true,
            pages: vec![
                ("{\"text\":\"Hi\"}".into(), None),
                ("\"plain\"".into(), None),
            ],
            title: ("Title".into(), None),
            author: "Steve".into(),
            generation: 2,
            resolved: false,
        };
        let stack = Stack {
            extra: components::intern(Extra {
                book_pages: Some(2),
                book: Some(book),
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let nbt = to_nbt(stack, "minecraft:written_book").unwrap();
        // Ваниль: страницы подписанной книги — JSON строкой в `{raw: ...}`.
        let comps = nbt.get("components").and_then(Tag::as_compound).unwrap();
        let content = comps
            .get("minecraft:written_book_content")
            .and_then(Tag::as_compound)
            .unwrap();
        assert_eq!(content.get("generation").and_then(Tag::as_i64), Some(2));
        assert_eq!(crate::redstone::book_pages(&nbt), 2);
        let id = |n: &str| (n == "minecraft:written_book").then_some(7);
        assert_eq!(from_nbt(&nbt, id).unwrap(), stack);
        let line = to_line(stack, "minecraft:written_book");
        assert!(line.contains("\"title\":\"Title\""), "{line}");
        assert_eq!(from_line(&line, id).unwrap(), stack);

        let draft = Stack {
            extra: components::intern(Extra {
                book_pages: Some(1),
                book: Some(components::Book::writable(vec![("a {b}".into(), None)])),
                ..Default::default()
            }),
            ..Stack::new(8, 1)
        };
        let id = |n: &str| (n == "minecraft:writable_book").then_some(8);
        let nbt = to_nbt(draft, "minecraft:writable_book").unwrap();
        assert_eq!(from_nbt(&nbt, id).unwrap(), draft);
        assert_eq!(
            from_line(&to_line(draft, "minecraft:writable_book"), id).unwrap(),
            draft
        );
    }

    #[test]
    fn trim_round_trips_through_playerdata() {
        components::set_trim_names(
            vec!["minecraft:gold".into()],
            vec!["minecraft:coast".into()],
        );
        let stack = Stack {
            extra: components::intern(Extra {
                trim: Some((0, 0)),
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let nbt = to_nbt(stack, "minecraft:diamond_chestplate").unwrap();
        let trim = nbt["components"].as_compound().unwrap()["minecraft:trim"]
            .as_compound()
            .unwrap();
        assert_eq!(trim["material"].as_str(), Some("minecraft:gold"));
        assert_eq!(
            from_nbt(&nbt, |name| (name == "minecraft:diamond_chestplate")
                .then_some(7)),
            Some(stack)
        );
    }

    #[test]
    fn legacy_custom_model_data_and_unbreakable_round_trip() {
        let stack = Stack {
            extra: components::intern(Extra {
                custom_model_data: Some(42),
                unbreakable: true,
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let id = |n: &str| (n == "minecraft:diamond_sword").then_some(7);
        let nbt = to_nbt(stack, "minecraft:diamond_sword").unwrap();
        let comps = nbt.get("components").and_then(Tag::as_compound).unwrap();
        let cmd = comps
            .get("minecraft:custom_model_data")
            .and_then(Tag::as_compound)
            .unwrap();
        assert_eq!(
            cmd.get("floats").and_then(Tag::as_list),
            Some(&[Tag::Float(42.0)][..])
        );
        assert_eq!(
            comps.get("minecraft:unbreakable"),
            Some(&Tag::Compound(Compound::new()))
        );
        assert_eq!(from_nbt(&nbt, id), Some(stack));

        let line = to_line(stack, "minecraft:diamond_sword");
        assert!(line.contains("\"custom_model_data\":42"), "{line}");
        assert!(line.contains("\"unbreakable\":true"), "{line}");
        assert_eq!(from_line(&line, id), Some(stack));
    }

    #[test]
    fn custom_model_data_reads_first_float_and_unbreakable_by_presence() {
        let nbt = Compound::from([
            ("id".into(), Tag::String("minecraft:paper".into())),
            ("count".into(), Tag::Int(1)),
            (
                "components".into(),
                Tag::Compound(Compound::from([
                    (
                        "minecraft:custom_model_data".into(),
                        Tag::Compound(Compound::from([
                            (
                                "floats".into(),
                                Tag::List(vec![Tag::Float(17.0), Tag::Float(99.0)]),
                            ),
                            ("flags".into(), Tag::List(vec![Tag::Byte(1)])),
                            ("strings".into(), Tag::List(vec![Tag::String("x".into())])),
                            ("colors".into(), Tag::List(vec![Tag::Int(0xff0000)])),
                        ])),
                    ),
                    (
                        "minecraft:unbreakable".into(),
                        Tag::Compound(Compound::new()),
                    ),
                ])),
            ),
        ]);
        let stack = from_nbt(&nbt, |n| (n == "minecraft:paper").then_some(3)).unwrap();
        let e = components::get(stack.extra);
        assert_eq!(e.custom_model_data, Some(17));
        assert!(e.unbreakable);
    }

    #[test]
    fn lock_and_container_loot_round_trip_through_nbt_and_line() {
        let lock = r#"{items:"minecraft:paper"}"#;
        let loot = r#"{loot_table:"minecraft:chests/simple_dungeon",seed:42}"#;
        let stack = Stack {
            extra: components::intern(Extra {
                lock: Some(lock.into()),
                container_loot: Some(loot.into()),
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let id = |n: &str| (n == "minecraft:shulker_box").then_some(7);
        let nbt = to_nbt(stack, "minecraft:shulker_box").unwrap();
        let comps = nbt.get("components").and_then(Tag::as_compound).unwrap();
        assert_eq!(
            comps
                .get("minecraft:lock")
                .map(crate::play::commands::snbt::format_tag),
            Some(lock.to_owned())
        );
        assert_eq!(from_nbt(&nbt, id).unwrap(), stack);

        let line = to_line(stack, "minecraft:shulker_box");
        assert_eq!(from_line(&line, id).unwrap(), stack);
    }

    #[test]
    fn block_entity_data_round_trips_through_nbt_and_line() {
        components::set_block_entity_types(vec![
            "minecraft:jukebox".into(),
            "minecraft:chest".into(),
        ]);
        let data = Compound::from([(
            "RecordItem".into(),
            Tag::Compound(Compound::from([
                ("id".into(), Tag::String("minecraft:music_disc_cat".into())),
                ("count".into(), Tag::Int(1)),
            ])),
        )]);
        let stack = Stack {
            extra: components::intern(Extra {
                block_entity_data: Some(components::BlockEntityData {
                    kind: 0,
                    tag: compound_to_net(&data),
                }),
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let id = |n: &str| (n == "minecraft:jukebox").then_some(7);
        let nbt = to_nbt(stack, "minecraft:jukebox").unwrap();
        let comps = nbt.get("components").and_then(Tag::as_compound).unwrap();
        let tag = comps
            .get("minecraft:block_entity_data")
            .and_then(Tag::as_compound)
            .unwrap();
        assert_eq!(
            tag.get("id").and_then(Tag::as_str),
            Some("minecraft:jukebox")
        );
        assert_eq!(tag.get("RecordItem"), data.get("RecordItem"));
        assert_eq!(from_nbt(&nbt, id).unwrap(), stack);
        let line = to_line(stack, "minecraft:jukebox");
        assert_eq!(from_line(&line, id).unwrap(), stack);
    }

    #[test]
    fn entity_data_round_trips_through_nbt_and_line() {
        components::set_entity_types(vec![
            "minecraft:item_frame".into(),
            "minecraft:glow_item_frame".into(),
        ]);
        let data = Compound::from([
            ("ItemRotation".into(), Tag::Byte(3)),
            ("Invisible".into(), Tag::Byte(1)),
        ]);
        let stack = Stack {
            extra: components::intern(Extra {
                entity_data: Some(components::BlockEntityData {
                    kind: 0,
                    tag: compound_to_net(&data),
                }),
                ..Default::default()
            }),
            ..Stack::new(7, 1)
        };
        let id = |n: &str| (n == "minecraft:item_frame").then_some(7);
        let nbt = to_nbt(stack, "minecraft:item_frame").unwrap();
        let comps = nbt.get("components").and_then(Tag::as_compound).unwrap();
        let tag = comps
            .get("minecraft:entity_data")
            .and_then(Tag::as_compound)
            .unwrap();
        assert_eq!(
            tag.get("id").and_then(Tag::as_str),
            Some("minecraft:item_frame")
        );
        assert_eq!(tag.get("ItemRotation"), data.get("ItemRotation"));
        assert_eq!(from_nbt(&nbt, id).unwrap(), stack);
        let line = to_line(stack, "minecraft:item_frame");
        assert_eq!(from_line(&line, id).unwrap(), stack);
    }

    #[test]
    fn container_contents_survive_nbt() {
        let names = |id: i32| {
            Some(
                match id {
                    10 => "minecraft:dirt",
                    11 => "minecraft:stone",
                    20 => "minecraft:shulker_box",
                    _ => return None,
                }
                .to_owned(),
            )
        };
        let ids = |name: &str| match name {
            "minecraft:dirt" => Some(10),
            "minecraft:stone" => Some(11),
            "minecraft:shulker_box" => Some(20),
            _ => None,
        };
        let stack = Stack {
            extra: components::intern(Extra {
                container: vec![Some(Stack::new(10, 3)), None, Some(Stack::new(11, 1))],
                ..Default::default()
            }),
            ..Stack::new(20, 1)
        };
        let nbt = named_nbt(stack, "minecraft:shulker_box", &names).unwrap();
        assert_eq!(from_nbt(&nbt, ids), Some(stack));
    }

    #[test]
    fn round_trip_with_components() {
        components::set_enchantments(vec![
            "minecraft:efficiency".into(),
            "minecraft:unbreaking".into(),
        ]);
        components::set_potions(vec!["minecraft:water".into()]);
        components::set_instruments(vec!["minecraft:ponder_goat_horn".into()]);
        let e = Extra {
            enchantments: vec![(0, 5), (1, 3)],
            stored: vec![(1, 2)],
            custom_name: Some(r#"{"text":"Кирка","italic":false}"#.into()),
            repair_cost: 5,
            trim: None,
            banner_patterns: Vec::new(),
            base_color: None,
            book_pages: None,
            book: None,
            lore: vec![r#""строка""#.into()],
            raw: None,
            raw_tail: None,
            rest: None,
            map_id: Some(12),
            map_post_processing: None,
            dyed_color: None,
            item_name: Some(r#""Visual Stone""#.into()),
            item_model: Some("minecraft:apple".into()),
            tooltip_style: Some("minecraft:default".into()),
            note_block_sound: None,
            rarity: None,
            lock: None,
            container_loot: None,
            block_entity_data: None,
            entity_data: None,
            hide_tooltip: true,
            glint_override: Some(false),
            potion: Some(0),
            instrument: Some(0),
            fireworks: Some(components::Fireworks {
                flight: 2,
                explosions: vec![components::Explosion {
                    shape: 2,
                    colors: vec![0xff0000],
                    fade: vec![0x0000ff],
                    trail: true,
                    twinkle: false,
                }],
            }),
            charged: Vec::new(),
            container: Vec::new(),
            custom_model_data: Some(9),
            unbreakable: true,
            custom_data: Some(compound_to_net(&Compound::from([(
                "PublicBukkitValues".into(),
                Tag::Compound(Compound::from([
                    ("test:n".into(), Tag::Long(1 << 40)),
                    ("test:a".into(), Tag::IntArray(vec![1, -2])),
                    ("test:l".into(), Tag::List(vec![Tag::String("x".into())])),
                ])),
            )]))),
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
        assert_eq!(
            comps["minecraft:item_model"].as_str(),
            Some("minecraft:apple")
        );
        assert_eq!(
            comps["minecraft:tooltip_style"].as_str(),
            Some("minecraft:default")
        );
        assert_eq!(comps["minecraft:item_name"].as_str(), Some("Visual Stone"));
        assert_eq!(
            comps["minecraft:tooltip_display"].as_compound().unwrap()["hide_tooltip"],
            Tag::Byte(1)
        );
        assert_eq!(comps["minecraft:enchantment_glint_override"], Tag::Byte(0));
        assert_eq!(
            comps["minecraft:potion_contents"].as_compound().unwrap()["potion"].as_str(),
            Some("minecraft:water")
        );
        let fw = comps["minecraft:fireworks"].as_compound().unwrap();
        assert_eq!(fw["flight_duration"], Tag::Byte(2));
        let ex = fw["explosions"].as_list().unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(ex["shape"].as_str(), Some("star"));
        assert_eq!(ex["colors"].as_list().unwrap()[0], Tag::Int(0xff0000));
        assert_eq!(ex["has_trail"], Tag::Byte(1));
        assert_eq!(
            comps["minecraft:instrument"].as_str(),
            Some("minecraft:ponder_goat_horn")
        );
        assert_eq!(back, s);
        let line = to_line(s, "minecraft:diamond_pickaxe");
        assert_eq!(
            from_line(&line, |n| (n == "minecraft:diamond_pickaxe").then_some(7)),
            Some(s)
        );

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
