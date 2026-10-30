//! Компоненты предметов сверх износа: чары, имя, описание.
//!
//! Стопка остаётся `Copy`: набор компонентов хранится один раз в общей
//! таблице, а в [`crate::inventory::Stack::extra`] — его номер (0 — нет).

use crate::inventory::Stack;
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
    pub lore: Vec<String>,
    /// Сырой патч компонентов с провода, если в нём было что-то кроме известных
    /// (зелья и прочее). Пишется как есть, чтобы клиент не терял состав.
    pub raw: Option<Vec<u8>>,
    /// `custom_data`: сетевой NBT (безымянный корневой compound), в т. ч.
    /// `PublicBukkitValues` плагинов (`PersistentDataContainer`).
    pub custom_data: Option<Vec<u8>>,
    /// `minecraft:charged_projectiles`: чем заряжен арбалет.
    pub charged: Vec<Stack>,
    /// Хвост командного `item_stack`, который не лёг в известные поля
    /// (`minecraft:potion_contents` и прочее). В патч клиента не пишется:
    /// сырой `raw` заменил бы уже разобранные чары и имя.
    pub rest: Option<String>,
}

impl Extra {
    pub fn is_empty(&self) -> bool {
        self.enchantments.is_empty()
            && self.stored.is_empty()
            && self.custom_name.is_none()
            && self.lore.is_empty()
            && self.raw.is_none()
            && self.custom_data.is_none()
            && self.charged.is_empty()
            && self.rest.is_none()
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
