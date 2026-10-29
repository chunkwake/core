//! Образ данных запуска: всё, что сервер собирает из дерева ванильных JSON,
//! упаковывается в `chunkwake-derived-v1/boot.v1.bin` и дальше читается одним
//! последовательным файлом вместо тысяч открытий и разборов.
//!
//! Сборка образа вызывает обычные загрузчики; повторный старт их не трогает.
//! Образ годен, только если совпали схема формата и `world_version` из
//! `version.json`: после смены версии или формата он собирается заново.

use crate::DataError;
use crate::enchanting::{AttrEffect, EnchantDef, Enchanting, LevelValue};
use crate::packets::{Direction, PacketTable, State};
use crate::recipes::{CampfireCook, Cooking, Ingredient, Recipe, Recipes};
use crate::registries::{Builtin, Registry, RegistryData, RegistryTags};
use crate::tools::{Tool, ToolRule};
use crate::world::{Blocks, Drink, Food, Fuel, ItemInfo, Properties};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Версия формата. Меняется вместе с любым полем образа.
const SCHEMA: u32 = 1;
const MAGIC: u32 = 0x434B_424F; // "CKBO"
const FILE: &str = "boot.v1.bin";

/// Слоты экипировки в порядке кодов образа (`ItemInfo::equip`).
const EQUIP: [&str; 8] = [
    "mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle",
];

/// Всё, что зависит только от данных версии, а не от мира.
#[derive(Debug)]
pub struct Boot {
    /// `world_version` из `version.json` (DataVersion чанков).
    pub world_version: i32,
    pub packets: PacketTable,
    /// `reports/registries.json`: реестр → запись → числовой ID.
    pub builtin: Builtin,
    pub registries: RegistryData,
    pub blocks: Blocks,
    /// `reports/light.csv`: излучение, затухание, небесный свет вниз.
    pub light: Vec<(u8, u8, bool)>,
    pub hardness: Vec<f32>,
    pub sturdy: Vec<u8>,
    pub requires_tool: Vec<bool>,
    /// Свойства предметов по ID в реестре `item`.
    pub items: Vec<ItemInfo>,
    pub tools: Vec<Option<Tool>>,
    pub recipes: Recipes,
    pub enchanting: Enchanting,
    /// Имена чар в порядке сетевых ID.
    pub enchantment_names: Vec<String>,
    /// Строки чар для плагинов (запрос `enchantments`).
    pub enchantment_lines: Vec<String>,
    pub dimension: crate::world::DimensionType,
}

/// Образ рядом с остальными сводками.
pub fn path(root: &Path) -> PathBuf {
    crate::world::derived(root, FILE)
}

/// Образ из кэша или, если его нет либо он устарел, сборка из дерева данных.
/// Ошибка записи образа не мешает запуску: следующий старт соберёт его снова.
pub fn load_or_build(root: &Path) -> Result<Boot, DataError> {
    let started = std::time::Instant::now();
    let world_version = read_world_version(root)?;
    let path = path(root);
    if let Ok(bytes) = std::fs::read(&path) {
        let read = started.elapsed();
        if let Ok(boot) = decode(&bytes, world_version) {
            tracing::debug!(
                "boot image {} KiB: read {} ms, decode {} ms",
                bytes.len() / 1024,
                read.as_millis(),
                started.elapsed().as_millis() - read.as_millis()
            );
            return Ok(boot);
        }
        tracing::debug!("boot image is stale; rebuilding from data files");
    }
    let boot = build(root, world_version)?;
    let _ = crate::world::write_derived_bytes(&path, &encode(&boot));
    Ok(boot)
}

/// `world_version` (DataVersion) из `version.json` в кэше данных.
pub fn read_world_version(root: &Path) -> Result<i32, DataError> {
    let path = root.join("version.json");
    let json = crate::world::read_json(&path)?;
    json["world_version"]
        .as_i64()
        .map(|n| n as i32)
        .ok_or_else(|| DataError::Format {
            file: path.display().to_string(),
            msg: "no world_version".into(),
        })
}

fn build(root: &Path, world_version: i32) -> Result<Boot, DataError> {
    let builtin = crate::registries::load_builtin(root)?;
    let packets = PacketTable::load(&root.join("reports/packets.json"))?;
    let registries = RegistryData::load_with(root, &builtin)?;
    let blocks = Blocks::load(root)?;
    let rows = crate::world::load_light_rows(root)?;
    let dimension = crate::world::DimensionType::load(root, "overworld")?;

    let items_map = builtin
        .get("item")
        .cloned()
        .ok_or_else(|| DataError::Io("unknown registry item".into()))?;
    let items = crate::world::load_items_cached(root, &items_map)?;
    let tools = crate::tools::load_tools_cached(root, &items_map)?;
    let recipes = crate::recipes::load_recipes(root, &items_map)?;
    let enchantment_names: Vec<String> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "enchantment")
        .map(|r| r.entries.clone())
        .unwrap_or_default();
    let enchanting = crate::enchanting::load(root, &enchantment_names)?;
    let enchantment_lines = crate::enchanting::plugin_lines(root, &enchantment_names);

    Ok(Boot {
        world_version,
        packets,
        builtin,
        registries,
        blocks,
        light: rows.light,
        hardness: rows.hardness,
        sturdy: rows.sturdy,
        requires_tool: rows.requires_tool,
        items,
        tools,
        recipes,
        enchanting,
        enchantment_names,
        enchantment_lines,
        dimension,
    })
}

// ------------------------------------------------------------------ запись

/// Пул строк: имена блоков, ключи и значения свойств повторяются тысячи раз.
#[derive(Default)]
struct Pool {
    ids: HashMap<String, u32>,
    list: Vec<String>,
}

impl Pool {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.ids.get(s) {
            return id;
        }
        let id = self.list.len() as u32;
        self.list.push(s.to_owned());
        self.ids.insert(s.to_owned(), id);
        id
    }
}

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    fn bool(&mut self, v: bool) {
        self.0.push(v as u8);
    }

    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }

    /// Множество строк в устойчивом порядке.
    fn set(&mut self, s: &HashSet<String>) {
        let mut v: Vec<&String> = s.iter().collect();
        v.sort();
        self.u32(v.len() as u32);
        for s in v {
            self.str(s);
        }
    }

    fn opt<T>(&mut self, v: Option<T>, f: impl FnOnce(&mut Self, T)) {
        match v {
            Some(v) => {
                self.u8(1);
                f(self, v);
            }
            None => self.u8(0),
        }
    }
}

fn encode(boot: &Boot) -> Vec<u8> {
    let mut w = W(Vec::with_capacity(1 << 20));
    w.u32(MAGIC);
    w.u32(SCHEMA);
    w.i32(boot.world_version);
    w_packets(&mut w, &boot.packets);
    w_builtin(&mut w, &boot.builtin);
    w_registries(&mut w, &boot.registries);
    w_blocks(&mut w, &boot.blocks);
    w.u32(boot.light.len() as u32);
    for (e, d, s) in &boot.light {
        w.u8(*e);
        w.u8(*d);
        w.bool(*s);
    }
    w.u32(boot.hardness.len() as u32);
    for v in &boot.hardness {
        w.f32(*v);
    }
    w.u32(boot.sturdy.len() as u32);
    w.0.extend_from_slice(&boot.sturdy);
    w.u32(boot.requires_tool.len() as u32);
    for v in &boot.requires_tool {
        w.bool(*v);
    }
    w_items(&mut w, &boot.items);
    w_tools(&mut w, &boot.tools);
    w_recipes(&mut w, &boot.recipes);
    w_enchanting(&mut w, &boot.enchanting);
    w.u32(boot.enchantment_names.len() as u32);
    for s in &boot.enchantment_names {
        w.str(s);
    }
    w.u32(boot.enchantment_lines.len() as u32);
    for s in &boot.enchantment_lines {
        w.str(s);
    }
    w.u8(u8::from(boot.dimension.has_skylight));
    w.i32(boot.dimension.min_y);
    w.i32(boot.dimension.height);
    w.0
}

fn w_packets(w: &mut W, t: &PacketTable) {
    let mut entries: Vec<(u8, u8, &str, i32)> = t
        .entries()
        .map(|(s, d, n, id)| (state_code(s), dir_code(d), n, id))
        .collect();
    entries.sort();
    w.u32(entries.len() as u32);
    for (s, d, n, id) in entries {
        w.u8(s);
        w.u8(d);
        w.str(n);
        w.i32(id);
    }
}

fn w_builtin(w: &mut W, b: &Builtin) {
    let mut regs: Vec<&String> = b.keys().collect();
    regs.sort();
    w.u32(regs.len() as u32);
    for name in regs {
        w.str(name);
        let mut entries: Vec<(&String, &i32)> = b[name].iter().collect();
        entries.sort();
        w.u32(entries.len() as u32);
        for (entry, id) in entries {
            w.str(entry);
            w.i32(*id);
        }
    }
}

fn w_registries(w: &mut W, r: &RegistryData) {
    w.u32(r.synchronized.len() as u32);
    for reg in &r.synchronized {
        w.str(&reg.name);
        w.u32(reg.entries.len() as u32);
        for e in &reg.entries {
            w.str(e);
        }
    }
    w.u32(r.tags.len() as u32);
    for t in &r.tags {
        w.str(&t.registry);
        w.u32(t.tags.len() as u32);
        for (name, ids) in &t.tags {
            w.str(name);
            w.u32(ids.len() as u32);
            for id in ids {
                w.i32(*id);
            }
        }
    }
}

fn w_blocks(w: &mut W, b: &Blocks) {
    /// Состояние в образе: имя и пары ключ-значение из пула строк.
    type EncodedState = Option<(u32, Vec<(u32, u32)>)>;
    let (defaults, max_id, states) = b.parts();
    let mut pool = Pool::default();
    let mut encoded: Vec<EncodedState> = Vec::with_capacity(states.len());
    for s in states {
        match s {
            Some((name, props)) => {
                let n = pool.intern(name);
                let ps = props
                    .iter()
                    .map(|(k, v)| (pool.intern(k), pool.intern(v)))
                    .collect();
                encoded.push(Some((n, ps)));
            }
            None => encoded.push(None),
        }
    }
    let mut defs: Vec<(u32, i32)> = defaults
        .iter()
        .map(|(name, id)| (pool.intern(name), *id))
        .collect();
    defs.sort();
    w.i64(max_id);
    w.u32(pool.list.len() as u32);
    for s in &pool.list {
        w.str(s);
    }
    w.u32(encoded.len() as u32);
    for s in &encoded {
        match s {
            None => w.u8(0),
            Some((n, ps)) => {
                w.u8(1);
                w.u32(*n);
                w.u32(ps.len() as u32);
                for (k, v) in ps {
                    w.u32(*k);
                    w.u32(*v);
                }
            }
        }
    }
    w.u32(defs.len() as u32);
    for (n, id) in defs {
        w.u32(n);
        w.i32(id);
    }
}

fn w_items(w: &mut W, items: &[ItemInfo]) {
    w.u32(items.len() as u32);
    for i in items {
        w.u8(i.max_stack);
        w.opt(i.food, |w, f: Food| {
            w.i32(f.nutrition);
            w.f32(f.saturation);
            w.bool(f.can_always_eat);
            w.f32(f.seconds);
        });
        w.opt(i.drink, |w, d: Drink| {
            w.f32(d.seconds);
            w.i32(d.remainder);
        });
        w.u8(
            i.equip
                .and_then(|e| EQUIP.iter().position(|s| *s == e))
                .map_or(0, |p| p as u8 + 1),
        );
        w.i32(i.max_damage);
        w.opt(i.fuel, |w, f: Fuel| {
            for v in f.burn {
                w.i32(v);
            }
            for v in f.speed {
                w.f32(v);
            }
        });
        w.f32(i.armor);
        w.f32(i.toughness);
        w.f32(i.attack);
        w.f32(i.attack_speed);
    }
}

fn w_tools(w: &mut W, tools: &[Option<Tool>]) {
    w.u32(tools.len() as u32);
    for t in tools {
        match t {
            None => w.u8(0),
            Some(t) => {
                w.u8(1);
                w.f32(t.default_speed);
                w.i32(t.damage_per_block);
                w.u32(t.rules.len() as u32);
                for r in &t.rules {
                    w.set(&r.blocks);
                    w.opt(r.speed, |w, v: f32| w.f32(v));
                    w.opt(r.correct_for_drops, |w, v: bool| w.bool(v));
                }
            }
        }
    }
}

fn w_ingredient(w: &mut W, ing: &Ingredient) {
    let mut ids: Vec<i32> = ing.iter().copied().collect();
    ids.sort_unstable();
    w.u32(ids.len() as u32);
    for id in ids {
        w.i32(id);
    }
}

fn w_recipes(w: &mut W, r: &Recipes) {
    let (list, cooking, campfire) = r.parts();
    w.u32(list.len() as u32);
    for recipe in list {
        match recipe {
            Recipe::Shaped {
                width,
                height,
                cells,
                result,
            } => {
                w.u8(0);
                w.u32(*width as u32);
                w.u32(*height as u32);
                w.u32(cells.len() as u32);
                for c in cells {
                    w.opt(c.as_ref(), |w, i: &Ingredient| w_ingredient(w, i));
                }
                w.i32(result.0);
                w.i32(result.1);
            }
            Recipe::Shapeless {
                ingredients,
                result,
            } => {
                w.u8(1);
                w.u32(ingredients.len() as u32);
                for i in ingredients {
                    w_ingredient(w, i);
                }
                w.i32(result.0);
                w.i32(result.1);
            }
        }
    }
    w.u32(cooking.len() as u32);
    for c in cooking {
        w.u8(c.kind);
        w_ingredient(w, &c.ingredient);
        w.i32(c.result.0);
        w.i32(c.result.1);
        w.i32(c.time);
        w.f32(c.experience);
    }
    w.u32(campfire.len() as u32);
    for c in campfire {
        w.str(&c.id);
        w_ingredient(w, &c.ingredient);
        w.i32(c.result.0);
        w.i32(c.result.1);
        w.i32(c.time);
        w.f32(c.experience);
    }
}

fn w_level_value(w: &mut W, v: &LevelValue) {
    use LevelValue::*;
    match v {
        Constant(c) => {
            w.u8(0);
            w.f32(*c);
        }
        Linear { base, per } => {
            w.u8(1);
            w.f32(*base);
            w.f32(*per);
        }
        LevelsSquared { added } => {
            w.u8(2);
            w.f32(*added);
        }
        Fraction(n, d) => {
            w.u8(3);
            w_level_value(w, n);
            w_level_value(w, d);
        }
        Clamped { value, min, max } => {
            w.u8(4);
            w_level_value(w, value);
            w.f32(*min);
            w.f32(*max);
        }
    }
}

fn w_enchanting(w: &mut W, e: &Enchanting) {
    w.u32(e.defs.len() as u32);
    for d in &e.defs {
        w.i32(d.id);
        w.str(&d.name);
        for v in [d.max_level, d.weight, d.min_base, d.min_step, d.max_base, d.max_step] {
            w.i32(v);
        }
        w.set(&d.supported);
        w.set(&d.primary);
        w.set(&d.exclusive);
        w.bool(d.in_table);
        w.bool(d.curse);
        w.u32(d.slots.len() as u32);
        for s in &d.slots {
            w.str(s);
        }
        w.u32(d.attributes.len() as u32);
        for a in &d.attributes {
            w.str(&a.attribute);
            w.str(&a.id);
            w.i32(a.op);
            w_level_value(w, &a.amount);
        }
        w.u32(d.protection.len() as u32);
        for (req, value) in &d.protection {
            w.u32(req.len() as u32);
            for (tags, expected) in req {
                w.set(tags);
                w.bool(*expected);
            }
            w_level_value(w, value);
        }
    }
    let mut enchantable: Vec<(&String, &i32)> = e.enchantable.iter().collect();
    enchantable.sort();
    w.u32(enchantable.len() as u32);
    for (name, v) in enchantable {
        w.str(name);
        w.i32(*v);
    }
    w.set(&e.providers);
    w.set(&e.transmitters);
    w.u32(e.table.len() as u32);
    for i in &e.table {
        w.u32(*i as u32);
    }
}

// ------------------------------------------------------------------ чтение

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> R<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DataError> {
        let end = self
            .p
            .checked_add(n)
            .filter(|&e| e <= self.b.len())
            .ok_or_else(|| corrupt("unexpected end of boot image"))?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, DataError> {
        Ok(self.take(1)?[0])
    }

    fn bool(&mut self) -> Result<bool, DataError> {
        Ok(self.u8()? != 0)
    }

    fn u32(&mut self) -> Result<u32, DataError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, DataError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> Result<i64, DataError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, DataError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn str(&mut self) -> Result<String, DataError> {
        let n = self.u32()? as usize;
        let bytes = self.take(n)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|e| corrupt(&e.to_string()))
    }

    /// Число элементов: не больше, чем позволяет остаток файла.
    fn count(&mut self) -> Result<usize, DataError> {
        let n = self.u32()? as usize;
        if n > self.b.len() - self.p.min(self.b.len()) {
            return Err(corrupt("element count exceeds image size"));
        }
        Ok(n)
    }

    fn set(&mut self) -> Result<HashSet<String>, DataError> {
        let n = self.count()?;
        let mut out = HashSet::with_capacity(n);
        for _ in 0..n {
            out.insert(self.str()?);
        }
        Ok(out)
    }

    fn opt<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, DataError>) -> Result<Option<T>, DataError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(f(self)?)),
            _ => Err(corrupt("invalid option tag")),
        }
    }
}

fn corrupt(msg: &str) -> DataError {
    DataError::Format {
        file: FILE.to_owned(),
        msg: msg.to_owned(),
    }
}

fn decode(bytes: &[u8], world_version: i32) -> Result<Boot, DataError> {
    let started = std::time::Instant::now();
    let mut r = R::new(bytes);
    if r.u32()? != MAGIC {
        return Err(corrupt("not a boot image"));
    }
    if r.u32()? != SCHEMA {
        return Err(corrupt("boot image schema changed"));
    }
    if r.i32()? != world_version {
        return Err(corrupt("boot image belongs to another version"));
    }
    let packets = r_packets(&mut r)?;
    let builtin = r_builtin(&mut r)?;
    let registries = r_registries(&mut r)?;
    let blocks = r_blocks(&mut r)?;
    let sections = started.elapsed();
    let n = r.count()?;
    let mut light = Vec::with_capacity(n);
    for _ in 0..n {
        let e = r.u8()?;
        let d = r.u8()?;
        let s = r.bool()?;
        light.push((e, d, s));
    }
    let n = r.count()?;
    let mut hardness = Vec::with_capacity(n);
    for _ in 0..n {
        hardness.push(r.f32()?);
    }
    let n = r.count()?;
    let sturdy = r.take(n)?.to_vec();
    let n = r.count()?;
    let mut requires_tool = Vec::with_capacity(n);
    for _ in 0..n {
        requires_tool.push(r.bool()?);
    }
    let items = r_items(&mut r)?;
    let tools = r_tools(&mut r)?;
    let recipes = r_recipes(&mut r)?;
    let enchanting = r_enchanting(&mut r)?;
    let n = r.count()?;
    let mut enchantment_names = Vec::with_capacity(n);
    for _ in 0..n {
        enchantment_names.push(r.str()?);
    }
    let n = r.count()?;
    let mut enchantment_lines = Vec::with_capacity(n);
    for _ in 0..n {
        enchantment_lines.push(r.str()?);
    }
    let has_skylight = r.bool()?;
    let min_y = r.i32()?;
    let height = r.i32()?;
    tracing::debug!(
        "boot image sections: registries and blocks {} ms, survival {} ms",
        sections.as_millis(),
        started.elapsed().as_millis() - sections.as_millis()
    );    Ok(Boot {
        world_version,
        packets,
        builtin,
        registries,
        blocks,
        light,
        hardness,
        sturdy,
        requires_tool,
        items,
        tools,
        recipes,
        enchanting,
        enchantment_names,
        enchantment_lines,
        dimension: crate::world::DimensionType {
            min_y,
            height,
            has_skylight,
        },
    })
}

fn r_packets(r: &mut R<'_>) -> Result<PacketTable, DataError> {
    let n = r.count()?;
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        let state = state_from(r.u8()?)?;
        let dir = dir_from(r.u8()?)?;
        let name = r.str()?;
        let id = r.i32()?;
        entries.push((state, dir, name, id));
    }
    Ok(PacketTable::from_entries(entries))
}

fn r_builtin(r: &mut R<'_>) -> Result<Builtin, DataError> {
    let n = r.count()?;
    let mut out = Builtin::with_capacity(n);
    for _ in 0..n {
        let registry = r.str()?;
        let m = r.count()?;
        let mut entries = HashMap::with_capacity(m);
        for _ in 0..m {
            let entry = r.str()?;
            entries.insert(entry, r.i32()?);
        }
        out.insert(registry, entries);
    }
    Ok(out)
}

fn r_registries(r: &mut R<'_>) -> Result<RegistryData, DataError> {
    let n = r.count()?;
    let mut synchronized = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.str()?;
        let m = r.count()?;
        let mut entries = Vec::with_capacity(m);
        for _ in 0..m {
            entries.push(r.str()?);
        }
        synchronized.push(Registry { name, entries });
    }
    let n = r.count()?;
    let mut tags = Vec::with_capacity(n);
    for _ in 0..n {
        let registry = r.str()?;
        let m = r.count()?;
        let mut list = Vec::with_capacity(m);
        for _ in 0..m {
            let name = r.str()?;
            let k = r.count()?;
            let mut ids = Vec::with_capacity(k);
            for _ in 0..k {
                ids.push(r.i32()?);
            }
            list.push((name, ids));
        }
        tags.push(RegistryTags {
            registry,
            tags: list,
        });
    }
    Ok(RegistryData {
        synchronized,
        tags,
    })
}

fn r_blocks(r: &mut R<'_>) -> Result<Blocks, DataError> {
    let max_id = r.i64()?;
    let n = r.count()?;
    let mut pool = Vec::with_capacity(n);
    for _ in 0..n {
        pool.push(r.str()?);
    }
    let get = |i: u32| -> Result<&str, DataError> {
        pool.get(i as usize)
            .map(String::as_str)
            .ok_or_else(|| corrupt("block name out of range"))
    };
    let n = r.count()?;
    let mut states: Vec<Option<(String, Properties)>> = Vec::with_capacity(n);
    for _ in 0..n {
        if r.u8()? == 0 {
            states.push(None);
            continue;
        }
        let name = get(r.u32()?)?.to_owned();
        let m = r.count()?;
        let mut props = Vec::with_capacity(m);
        for _ in 0..m {
            let k = get(r.u32()?)?.to_owned();
            let v = get(r.u32()?)?.to_owned();
            props.push((k, v));
        }
        states.push(Some((name, props)));
    }
    let n = r.count()?;
    let mut defaults = HashMap::with_capacity(n);
    for _ in 0..n {
        let name = get(r.u32()?)?.to_owned();
        defaults.insert(name, r.i32()?);
    }
    Ok(Blocks::from_parts(defaults, max_id, states))
}

fn r_items(r: &mut R<'_>) -> Result<Vec<ItemInfo>, DataError> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let max_stack = r.u8()?;
        let food = r.opt(|r| {
            Ok(Food {
                nutrition: r.i32()?,
                saturation: r.f32()?,
                can_always_eat: r.bool()?,
                seconds: r.f32()?,
            })
        })?;
        let drink = r.opt(|r| {
            Ok(Drink {
                seconds: r.f32()?,
                remainder: r.i32()?,
            })
        })?;
        let equip = match r.u8()? {
            0 => None,
            i => EQUIP.get(i as usize - 1).copied(),
        };
        let max_damage = r.i32()?;
        let fuel = r.opt(|r| {
            let mut burn = [0; 3];
            for v in &mut burn {
                *v = r.i32()?;
            }
            let mut speed = [1.0; 3];
            for v in &mut speed {
                *v = r.f32()?;
            }
            Ok(Fuel { burn, speed })
        })?;
        out.push(ItemInfo {
            max_stack,
            food,
            drink,
            equip,
            max_damage,
            fuel,
            armor: r.f32()?,
            toughness: r.f32()?,
            attack: r.f32()?,
            attack_speed: r.f32()?,
        });
    }
    Ok(out)
}

fn r_tools(r: &mut R<'_>) -> Result<Vec<Option<Tool>>, DataError> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if r.u8()? == 0 {
            out.push(None);
            continue;
        }
        let default_speed = r.f32()?;
        let damage_per_block = r.i32()?;
        let m = r.count()?;
        let mut rules = Vec::with_capacity(m);
        for _ in 0..m {
            let blocks = Arc::new(r.set()?);
            let speed = r.opt(|r| r.f32())?;
            let correct_for_drops = r.opt(|r| r.bool())?;
            rules.push(ToolRule {
                blocks,
                speed,
                correct_for_drops,
            });
        }
        out.push(Some(Tool {
            default_speed,
            damage_per_block,
            rules,
        }));
    }
    Ok(out)
}

fn r_ingredient(r: &mut R<'_>) -> Result<Ingredient, DataError> {
    let n = r.count()?;
    let mut set = HashSet::with_capacity(n);
    for _ in 0..n {
        set.insert(r.i32()?);
    }
    Ok(Arc::new(set))
}

fn r_recipes(r: &mut R<'_>) -> Result<Recipes, DataError> {
    let n = r.count()?;
    let mut list = Vec::with_capacity(n);
    for _ in 0..n {
        match r.u8()? {
            0 => {
                let width = r.u32()? as usize;
                let height = r.u32()? as usize;
                let m = r.count()?;
                let mut cells = Vec::with_capacity(m);
                for _ in 0..m {
                    cells.push(r.opt(|r| r_ingredient(r))?);
                }
                let result = (r.i32()?, r.i32()?);
                list.push(Recipe::Shaped {
                    width,
                    height,
                    cells,
                    result,
                });
            }
            1 => {
                let m = r.count()?;
                let mut ingredients = Vec::with_capacity(m);
                for _ in 0..m {
                    ingredients.push(r_ingredient(r)?);
                }
                let result = (r.i32()?, r.i32()?);
                list.push(Recipe::Shapeless {
                    ingredients,
                    result,
                });
            }
            _ => return Err(corrupt("unknown recipe kind")),
        }
    }
    let n = r.count()?;
    let mut cooking = Vec::with_capacity(n);
    for _ in 0..n {
        cooking.push(Cooking {
            kind: r.u8()?,
            ingredient: r_ingredient(r)?,
            result: (r.i32()?, r.i32()?),
            time: r.i32()?,
            experience: r.f32()?,
        });
    }
    let n = r.count()?;
    let mut campfire = Vec::with_capacity(n);
    for _ in 0..n {
        campfire.push(CampfireCook {
            id: r.str()?,
            ingredient: r_ingredient(r)?,
            result: (r.i32()?, r.i32()?),
            time: r.i32()?,
            experience: r.f32()?,
        });
    }
    Ok(Recipes::from_parts(list, cooking, campfire))
}

fn r_level_value(r: &mut R<'_>) -> Result<LevelValue, DataError> {
    use LevelValue::*;
    Ok(match r.u8()? {
        0 => Constant(r.f32()?),
        1 => Linear {
            base: r.f32()?,
            per: r.f32()?,
        },
        2 => LevelsSquared { added: r.f32()? },
        3 => Fraction(
            Box::new(r_level_value(r)?),
            Box::new(r_level_value(r)?),
        ),
        4 => Clamped {
            value: Box::new(r_level_value(r)?),
            min: r.f32()?,
            max: r.f32()?,
        },
        _ => return Err(corrupt("unknown level value")),
    })
}

fn r_enchanting(r: &mut R<'_>) -> Result<Enchanting, DataError> {
    let n = r.count()?;
    let mut defs = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.i32()?;
        let name = r.str()?;
        let (max_level, weight, min_base, min_step, max_base, max_step) =
            (r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        let supported = r.set()?;
        let primary = r.set()?;
        let exclusive = r.set()?;
        let in_table = r.bool()?;
        let curse = r.bool()?;
        let m = r.count()?;
        let mut slots = Vec::with_capacity(m);
        for _ in 0..m {
            slots.push(r.str()?);
        }
        let m = r.count()?;
        let mut attributes = Vec::with_capacity(m);
        for _ in 0..m {
            attributes.push(AttrEffect {
                attribute: r.str()?,
                id: r.str()?,
                op: r.i32()?,
                amount: r_level_value(r)?,
            });
        }
        let m = r.count()?;
        let mut protection = Vec::with_capacity(m);
        for _ in 0..m {
            let k = r.count()?;
            let mut req = Vec::with_capacity(k);
            for _ in 0..k {
                req.push((r.set()?, r.bool()?));
            }
            protection.push((req, r_level_value(r)?));
        }
        defs.push(EnchantDef {
            id,
            name,
            max_level,
            weight,
            min_base,
            min_step,
            max_base,
            max_step,
            supported,
            primary,
            exclusive,
            in_table,
            curse,
            slots,
            attributes,
            protection,
        });
    }
    let n = r.count()?;
    let mut enchantable = HashMap::with_capacity(n);
    for _ in 0..n {
        let name = r.str()?;
        enchantable.insert(name, r.i32()?);
    }
    let providers = r.set()?;
    let transmitters = r.set()?;
    let n = r.count()?;
    let mut table = Vec::with_capacity(n);
    for _ in 0..n {
        table.push(r.u32()? as usize);
    }
    Ok(Enchanting {
        defs,
        enchantable,
        providers,
        transmitters,
        table,
    })
}

fn state_code(s: State) -> u8 {
    match s {
        State::Handshake => 0,
        State::Status => 1,
        State::Login => 2,
        State::Configuration => 3,
        State::Play => 4,
    }
}

fn state_from(c: u8) -> Result<State, DataError> {
    Ok(match c {
        0 => State::Handshake,
        1 => State::Status,
        2 => State::Login,
        3 => State::Configuration,
        4 => State::Play,
        _ => return Err(corrupt("unknown packet state")),
    })
}

fn dir_code(d: Direction) -> u8 {
    match d {
        Direction::Clientbound => 0,
        Direction::Serverbound => 1,
    }
}

fn dir_from(c: u8) -> Result<Direction, DataError> {
    Ok(match c {
        0 => Direction::Clientbound,
        1 => Direction::Serverbound,
        _ => return Err(corrupt("unknown packet direction")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::Builtin;

    fn sample() -> Boot {
        let blocks = Blocks::parse(&serde_json::json!({
            "minecraft:air": {"states": [{"id": 0, "default": true}]},
            "minecraft:stone": {"states": [{"id": 1, "default": true}]},
            "minecraft:oak_log": {"states": [
                {"id": 2, "default": true, "properties": {"axis": "y"}},
                {"id": 3, "properties": {"axis": "x"}}
            ]}
        }))
        .unwrap();
        let mut builtin = Builtin::new();
        builtin.insert(
            "item".into(),
            HashMap::from([("minecraft:stone".to_owned(), 1), ("minecraft:bread".to_owned(), 2)]),
        );
        let mut registries = RegistryData::default();
        registries.synchronized.push(Registry {
            name: "worldgen/biome".into(),
            entries: vec!["minecraft:plains".into()],
        });
        registries.tags.push(RegistryTags {
            registry: "item".into(),
            tags: vec![("minecraft:logs".into(), vec![2, 3])],
        });
        let items = vec![
            ItemInfo::default(),
            ItemInfo {
                max_stack: 64,
                ..Default::default()
            },
            ItemInfo {
                max_stack: 16,
                food: Some(Food {
                    nutrition: 5,
                    saturation: 6.0,
                    can_always_eat: false,
                    seconds: 1.6,
                }),
                drink: None,
                equip: Some("head"),
                max_damage: 165,
                fuel: Some(Fuel {
                    burn: [1600, 800, 800],
                    speed: [1.0, 2.0, 2.0],
                }),
                armor: 8.0,
                toughness: 2.0,
                attack: 5.0,
                attack_speed: -2.4,
            },
        ];
        let tools = vec![
            None,
            Some(Tool {
                default_speed: 1.0,
                damage_per_block: 1,
                rules: vec![ToolRule {
                    blocks: Arc::new(HashSet::from(["minecraft:stone".to_owned()])),
                    speed: Some(4.0),
                    correct_for_drops: Some(true),
                }],
            }),
        ];
        let recipes = Recipes::from_parts(
            vec![Recipe::Shaped {
                width: 1,
                height: 1,
                cells: vec![Some(Arc::new(HashSet::from([1])))],
                result: (2, 4),
            }],
            vec![Cooking {
                kind: 0,
                ingredient: Arc::new(HashSet::from([1])),
                result: (2, 1),
                time: 200,
                experience: 0.1,
            }],
            vec![CampfireCook {
                id: "minecraft:bread".into(),
                ingredient: Arc::new(HashSet::from([2])),
                result: (2, 1),
                time: 600,
                experience: 0.35,
            }],
        );
        let mut enchanting = Enchanting::default();
        enchanting.defs.push(EnchantDef {
            id: 0,
            name: "minecraft:sharpness".into(),
            max_level: 5,
            weight: 10,
            min_base: 1,
            min_step: 11,
            max_base: 21,
            max_step: 11,
            supported: HashSet::from(["minecraft:stone".to_owned()]),
            primary: HashSet::from(["minecraft:stone".to_owned()]),
            exclusive: HashSet::new(),
            in_table: true,
            curse: false,
            slots: vec!["mainhand".into()],
            attributes: vec![AttrEffect {
                attribute: "minecraft:attack_damage".into(),
                id: "minecraft:sharpness".into(),
                op: 0,
                amount: LevelValue::Linear {
                    base: 1.0,
                    per: 0.5,
                },
            }],
            protection: vec![(
                vec![(HashSet::from(["minecraft:is_fire".to_owned()]), true)],
                LevelValue::Clamped {
                    value: Box::new(LevelValue::Constant(2.0)),
                    min: 0.0,
                    max: 4.0,
                },
            )],
        });
        enchanting
            .enchantable
            .insert("minecraft:stone".into(), 5);
        enchanting.providers.insert("minecraft:bookshelf".into());
        enchanting.table.push(0);
        Boot {
            world_version: 5023,
            packets: PacketTable::from_entries(vec![(
                State::Play,
                Direction::Clientbound,
                "login".into(),
                0x2b,
            )]),
            builtin,
            registries,
            blocks,
            light: vec![(0, 15, true), (15, 0, false)],
            hardness: vec![-1.0, 1.5],
            sturdy: vec![0, 63],
            requires_tool: vec![false, true],
            items,
            tools,
            recipes,
            enchanting,
            enchantment_names: vec!["minecraft:sharpness".into()],
            enchantment_lines: vec!["minecraft:sharpness 5 10 1 1 11 21 11 0 0".into()],
            dimension: crate::world::DimensionType {
                min_y: -64,
                height: 384,
                has_skylight: true,
            },
        }
    }

    #[test]
    fn round_trip() {
        let boot = sample();
        let bytes = encode(&boot);
        let back = decode(&bytes, boot.world_version).expect("decode");
        assert_eq!(back.world_version, boot.world_version);
        assert_eq!(back.blocks, boot.blocks);
        assert_eq!(back.sturdy, boot.sturdy);
        assert_eq!(back.hardness, boot.hardness);
        assert_eq!(back.requires_tool, boot.requires_tool);
        assert_eq!(back.light, boot.light);
        assert_eq!(back.items, boot.items);
        assert_eq!(back.tools, boot.tools);
        assert_eq!(back.enchantment_names, boot.enchantment_names);
        assert_eq!(back.enchantment_lines, boot.enchantment_lines);
        assert_eq!(back.registries, boot.registries);
        assert_eq!(back.builtin, boot.builtin);
        assert_eq!(
            back.packets
                .id(State::Play, Direction::Clientbound, "login")
                .unwrap(),
            0x2b
        );
        assert_eq!(back.blocks.state_count(), boot.blocks.state_count());
        assert_eq!(
            back.blocks.state_id(
                "minecraft:oak_log",
                &[("axis".to_owned(), "x".to_owned())]
            ),
            Some(3)
        );
        assert_eq!(back.recipes.len(), boot.recipes.len());
        assert_eq!(back.recipes.find(&[Some(1)], 1), Some((2, 4)));
        assert_eq!(back.enchanting.defs.len(), 1);
        assert_eq!(back.enchanting.defs[0].max_level, 5);
        assert_eq!(back.enchanting.table, vec![0]);
        assert_eq!(back.dimension.height, 384);
    }

    #[test]
    fn other_world_version_is_rejected() {
        let bytes = encode(&sample());
        assert!(decode(&bytes, 4000).is_err());
    }

    #[test]
    fn truncated_image_is_an_error() {
        let bytes = encode(&sample());
        for cut in [0, 4, 12, bytes.len() / 2, bytes.len() - 1] {
            assert!(decode(&bytes[..cut], 5023).is_err(), "cut at {cut}");
        }
    }
}
