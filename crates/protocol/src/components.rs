//! Компоненты предметов сверх износа: чары, имя, описание.
//!
//! Стопка остаётся `Copy`: набор компонентов хранится один раз в общей
//! таблице, а в [`crate::inventory::Stack::extra`] — его номер (0 — нет).

use crate::inventory::{Slot, Stack};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// Компоненты одной стопки. Имя и строки описания — текст в JSON (Adventure).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Extra {
    /// `(сетевой ID чара, уровень)` в порядке добавления.
    pub enchantments: Vec<(i32, i32)>,
    /// Чары, хранимые зачарованной книгой (`stored_enchantments`).
    pub stored: Vec<(i32, i32)>,
    pub custom_name: Option<String>,
    /// `minecraft:repair_cost`: штраф за предыдущие работы на наковальне.
    pub repair_cost: i32,
    /// `minecraft:trim`: сетевые ID материала и узора.
    pub trim: Option<(i32, i32)>,
    /// Слои знамени: сетевой ID узора и ID цвета.
    pub banner_patterns: Vec<(i32, i32)>,
    /// Число страниц книги (для лектора): по [`Extra::book`] или по сырому патчу.
    pub book_pages: Option<i32>,
    /// `writable_book_content` или `written_book_content`.
    pub book: Option<Book>,
    pub lore: Vec<String>,
    /// Сырой патч компонентов с провода, если в нём было что-то кроме известных
    /// (зелья и прочее). Пишется как есть, чтобы клиент не терял состав.
    pub raw: Option<Vec<u8>>,
    /// `custom_data`: сетевой NBT (безымянный корневой compound), в т. ч.
    /// `PublicBukkitValues` плагинов (`PersistentDataContainer`).
    pub custom_data: Option<Vec<u8>>,
    /// `minecraft:charged_projectiles`: чем заряжен арбалет.
    pub charged: Vec<Stack>,
    /// `minecraft:container`: содержимое шалкер-ящика по слотам. Шалкеры
    /// в шалкеры не кладутся, поэтому вложенность ограничена одним уровнем.
    pub container: Vec<Slot>,
    /// Хвост командного `item_stack`, который не лёг в известные поля
    /// (`minecraft:potion_contents` и прочее). В патч клиента не пишется:
    /// сырой `raw` заменил бы уже разобранные чары и имя.
    pub rest: Option<String>,
    /// `minecraft:map_id`: номер карты (`data/minecraft/maps/<id>.dat`).
    pub map_id: Option<i32>,
    /// `minecraft:map_post_processing` у результата стола картографа:
    /// 0 — `LOCK`, 1 — `SCALE`. Снимается, как только стопку забрали.
    pub map_post_processing: Option<u8>,
}

/// Строка с необязательной отфильтрованной версией (`Filterable<T>`).
pub type Filterable = (String, Option<String>);

/// Содержимое книги. У черновой (`writable_book_content`) страницы — простой
/// текст до 1024 символов, у подписанной (`written_book_content`) — текстовые
/// компоненты в JSON, плюс название, автор, поколение и флаг `resolved`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Book {
    pub written: bool,
    pub pages: Vec<Filterable>,
    pub title: Filterable,
    pub author: String,
    /// 0 — оригинал, 1 — копия, 2 — копия копии, 3 — потрёпанная.
    pub generation: i32,
    /// Компоненты страниц уже разрешены (`WrittenBookContent.resolveForItem`).
    pub resolved: bool,
}

impl Book {
    /// Черновик с этими страницами.
    pub fn writable(pages: Vec<Filterable>) -> Self {
        Book {
            pages,
            ..Book::default()
        }
    }

    /// Страниц для лектора: как у сырого патча, не меньше одной.
    pub fn page_count(&self) -> i32 {
        (self.pages.len().min(100) as i32).max(1)
    }
}

impl Extra {
    pub fn is_empty(&self) -> bool {
        self.enchantments.is_empty()
            && self.stored.is_empty()
            && self.custom_name.is_none()
            && self.repair_cost == 0
            && self.trim.is_none()
            && self.banner_patterns.is_empty()
            && self.book_pages.is_none()
            && self.book.is_none()
            && self.lore.is_empty()
            && self.raw.is_none()
            && self.custom_data.is_none()
            && self.charged.is_empty()
            && self.container.is_empty()
            && self.rest.is_none()
            && self.map_id.is_none()
            && self.map_post_processing.is_none()
    }

    pub fn level(&self, enchantment: i32) -> i32 {
        self.enchantments
            .iter()
            .find(|e| e.0 == enchantment)
            .map_or(0, |e| e.1)
    }
}

#[derive(Default)]
struct Table {
    list: Vec<Arc<Extra>>,
    index: HashMap<Arc<Extra>, u32>,
}

fn table() -> &'static RwLock<Table> {
    static T: OnceLock<RwLock<Table>> = OnceLock::new();
    T.get_or_init(Default::default)
}

/// Номер набора компонентов (одинаковые наборы — один номер); пустой — 0.
pub fn intern(mut e: Extra) -> u32 {
    if e.is_empty() {
        return 0;
    }
    // Один вид текста (ключи по порядку), чтобы одинаковые предметы складывались.
    let canon = |t: &mut String| {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
            *t = v.to_string();
        }
    };
    e.custom_name.iter_mut().for_each(canon);
    e.lore.iter_mut().for_each(canon);
    if let Some(book) = e.book.as_mut().filter(|b| b.written) {
        for (raw, filtered) in &mut book.pages {
            canon(raw);
            filtered.iter_mut().for_each(canon);
        }
    }
    e.enchantments.sort_unstable();
    e.stored.sort_unstable();
    if let Some(&id) = table().read().unwrap().index.get(&e) {
        return id;
    }
    let mut t = table().write().unwrap();
    if let Some(&id) = t.index.get(&e) {
        return id;
    }
    let e = Arc::new(e);
    t.list.push(e.clone());
    let id = t.list.len() as u32;
    t.index.insert(e, id);
    id
}

/// Набор по номеру; для 0 — пустой.
pub fn get(id: u32) -> Arc<Extra> {
    if id == 0 {
        return Arc::default();
    }
    table()
        .read()
        .unwrap()
        .list
        .get(id as usize - 1)
        .cloned()
        .unwrap_or_default()
}

/// Номера компонентов в реестре `data_component_type` (ставятся при старте; -1 — нет).
pub static ENCHANTMENTS_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static CUSTOM_NAME_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static LORE_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static STORED_ENCHANTMENTS_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static CUSTOM_DATA_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static CHARGED_PROJECTILES_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static CONTAINER_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static REPAIR_COST_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static TRIM_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static BANNER_PATTERNS_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static WRITTEN_BOOK_CONTENT_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static WRITABLE_BOOK_CONTENT_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static FIREWORKS_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static MAP_ID_COMPONENT: AtomicI32 = AtomicI32::new(-1);
pub static MAP_POST_PROCESSING_COMPONENT: AtomicI32 = AtomicI32::new(-1);

/// `minecraft:fireworks` ракеты из сырого патча: `(flight_duration, число
/// взрывов)`. Скрафченная ракета несёт в патче только этот компонент, и он
/// первый; за неизвестным компонентом длина тела неизвестна — тогда `None`,
/// и действует прототип предмета (полёт 1, без взрывов).
pub fn fireworks(extra: u32) -> Option<(i32, usize)> {
    let e = get(extra);
    let raw = e.raw.as_deref()?;
    let mut r = crate::Reader::new(raw);
    let added = r.var_int().ok()?;
    let _removed = r.var_int().ok()?;
    if added < 1 || r.var_int().ok()? != id_of(&FIREWORKS_COMPONENT) {
        return None;
    }
    let flight = r.var_int().ok()?;
    let explosions = r.var_int().ok()?.max(0) as usize;
    Some((flight, explosions))
}

static TRIM_MATERIAL_NAMES: OnceLock<Vec<String>> = OnceLock::new();
static TRIM_PATTERN_NAMES: OnceLock<Vec<String>> = OnceLock::new();
static BANNER_PATTERN_NAMES: OnceLock<Vec<String>> = OnceLock::new();

pub fn set_banner_pattern_names(patterns: Vec<String>) {
    let _ = BANNER_PATTERN_NAMES.set(patterns);
}

pub fn banner_pattern_name(id: i32) -> Option<&'static str> {
    BANNER_PATTERN_NAMES
        .get()?
        .get(id as usize)
        .map(String::as_str)
}

pub fn banner_pattern_id(name: &str) -> Option<i32> {
    Some(BANNER_PATTERN_NAMES.get()?.iter().position(|n| n == name)? as i32)
}

pub const DYE_COLORS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

pub fn set_trim_names(materials: Vec<String>, patterns: Vec<String>) {
    let _ = TRIM_MATERIAL_NAMES.set(materials);
    let _ = TRIM_PATTERN_NAMES.set(patterns);
}

pub fn trim_names(material: i32, pattern: i32) -> Option<(&'static str, &'static str)> {
    Some((
        TRIM_MATERIAL_NAMES.get()?.get(material as usize)?.as_str(),
        TRIM_PATTERN_NAMES.get()?.get(pattern as usize)?.as_str(),
    ))
}

pub fn trim_ids(material: &str, pattern: &str) -> Option<(i32, i32)> {
    Some((
        TRIM_MATERIAL_NAMES
            .get()?
            .iter()
            .position(|n| n == material)? as i32,
        TRIM_PATTERN_NAMES
            .get()?
            .iter()
            .position(|n| n == pattern)? as i32,
    ))
}

pub(crate) fn id_of(c: &AtomicI32) -> i32 {
    c.load(Ordering::Relaxed)
}

/// Чары по сетевому ID (порядок синхронизируемого реестра `enchantment`).
static ENCHANTMENT_NAMES: OnceLock<Vec<String>> = OnceLock::new();

pub fn set_enchantments(names: Vec<String>) {
    let _ = ENCHANTMENT_NAMES.set(names);
}

pub fn enchantment_name(id: i32) -> Option<&'static str> {
    ENCHANTMENT_NAMES
        .get()?
        .get(id as usize)
        .map(String::as_str)
}

/// Уровень чар `name` у стопки с компонентами `extra` (0 — нет).
pub fn stack_level(extra: u32, name: &str) -> i32 {
    if extra == 0 {
        return 0;
    }
    enchantment_id(name).map_or(0, |id| get(extra).level(id))
}

pub fn enchantment_id(name: &str) -> Option<i32> {
    ENCHANTMENT_NAMES
        .get()?
        .iter()
        .position(|n| n == name)
        .map(|i| i as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_dedupes() {
        let e = Extra {
            enchantments: vec![(3, 5)],
            ..Default::default()
        };
        let a = intern(e.clone());
        assert_ne!(a, 0);
        assert_eq!(intern(e), a);
        assert_eq!(get(a).level(3), 5);
        assert_eq!(intern(Extra::default()), 0);
    }
}
