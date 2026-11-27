//! Книги: `edit_book` от клиента (`ServerGamePacketListenerImpl.handleEditBook`
//! → `updateBookContents`/`signBook`) с проверками Paper (размер книги и не
//! чаще раза в секунду), `PlayerEditBookEvent` и открытие подписанной книги
//! (`WrittenBookItem.use` → `ServerPlayer.openItemGui`).

use super::*;
use chunkwake_protocol::components::{self, Book};
use chunkwake_protocol::play::EditBook;

pub(super) const WRITABLE_BOOK: &str = "minecraft:writable_book";
pub(super) const WRITTEN_BOOK: &str = "minecraft:written_book";

/// Paper `item-validation.book-size`: `page-max` 2560 байт и `total-multiplier` 0.98.
const PAGE_MAX: i64 = 2560;
const TOTAL_MULTIPLIER: f64 = 0.98;

/// Проверка размера книги из `handleEditBook` (Paper): суммарные байты UTF-8
/// против бюджета, который растёт с длиной каждой страницы.
pub(super) fn book_size_ok(pages: &[String]) -> bool {
    let multiplier = TOTAL_MULTIPLIER.clamp(0.3, 1.0);
    let mut total: i64 = 0;
    let mut allowed: i64 = PAGE_MAX;
    for page in pages {
        let bytes = page.len() as i64;
        total += bytes;
        // `String.length()` — в UTF-16.
        let length = page.encode_utf16().count();
        let multibytes = if bytes as usize != length {
            page.encode_utf16().filter(|&c| c > 127).count() as i64
        } else {
            0
        };
        allowed = (allowed as f64
            + PAGE_MAX as f64 * (length as f64 / 255.0).clamp(0.1, 1.0) * multiplier)
            as i64;
        if multibytes > 1 {
            allowed -= multibytes;
        }
    }
    total <= allowed
}

/// Слот пакета (`Inventory`: 0–8 хотбар, 40 вторая рука) → слот окна игрока.
pub(super) fn book_window_slot(slot: i32) -> Option<usize> {
    match slot {
        0..=8 => Some(36 + slot as usize),
        40 => Some(survival::OFFHAND),
        _ => None,
    }
}

/// Текстовый компонент `Component.literal(page)` в JSON.
fn literal(page: &str) -> String {
    serde_json::Value::String(page.to_owned()).to_string()
}

/// Простой текст компонента (для `separator`): строка, `text` или `extra`.
fn plain_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => items.iter().map(plain_text).collect(),
        serde_json::Value::Object(map) => map
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Источник NBT для `NbtContents` (`{"nbt": "...", "entity": "..."}`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NbtSource {
    /// Селектор или имя сущности.
    Entity(String),
    /// Абсолютные координаты блока `x y z`.
    Block(String),
}

/// Запрос `NbtContents` к компоненту страницы.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NbtQuery {
    pub path: String,
    pub source: NbtSource,
    /// `interpret`: строка пути сама является JSON-компонентом.
    pub interpret: bool,
    /// `separator` для нескольких значений (сейчас путь даёт одно).
    pub separator: Option<String>,
}

/// `ComponentUtils.resolve`: заменить `selector`, `score` и `nbt` на готовый
/// текст/компонент, включая `extra` и аргументы `translate` (`with`).
pub(super) fn resolve_page(
    json: &str,
    selector: &dyn Fn(&str, Option<&str>) -> String,
    score: &dyn Fn(&str, &str) -> Option<String>,
    nbt: &dyn Fn(&NbtQuery) -> Option<serde_json::Value>,
) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json) else {
        return json.to_owned();
    };
    resolve_component(&mut value, selector, score, nbt);
    value.to_string()
}

fn resolve_component(
    value: &mut serde_json::Value,
    selector: &dyn Fn(&str, Option<&str>) -> String,
    score: &dyn Fn(&str, &str) -> Option<String>,
    nbt: &dyn Fn(&NbtQuery) -> Option<serde_json::Value>,
) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(sel)) = map.get("selector") {
                let sel = sel.clone();
                let separator = map.get("separator").map(plain_text);
                let text = selector(&sel, separator.as_deref());
                map.remove("selector");
                map.remove("separator");
                map.insert("text".into(), serde_json::Value::String(text));
            } else if let Some(serde_json::Value::Object(contents)) = map.get("score") {
                let name = contents
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let objective = contents
                    .get("objective")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let text = if name.is_empty() || objective.is_empty() {
                    String::new()
                } else {
                    score(name, objective).unwrap_or_default()
                };
                map.remove("score");
                map.insert("text".into(), serde_json::Value::String(text));
            } else if let Some(serde_json::Value::String(path)) = map.get("nbt") {
                let path = path.clone();
                let source = match (map.get("entity"), map.get("block")) {
                    (Some(serde_json::Value::String(entity)), _) => {
                        Some(NbtSource::Entity(entity.clone()))
                    }
                    (_, Some(serde_json::Value::String(block))) => {
                        Some(NbtSource::Block(block.clone()))
                    }
                    _ => None,
                };
                if let Some(source) = source {
                    let interpret = map
                        .get("interpret")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let separator = map.get("separator").map(plain_text);
                    let query = NbtQuery {
                        path,
                        source,
                        interpret,
                        separator,
                    };
                    let resolved = nbt(&query);
                    for key in ["nbt", "entity", "block", "interpret", "separator"] {
                        map.remove(key);
                    }
                    match resolved {
                        // `interpret`: содержимое — готовый компонент.
                        Some(serde_json::Value::Object(contents)) => {
                            for (key, value) in contents {
                                map.insert(key, value);
                            }
                        }
                        Some(other) => {
                            map.insert("text".into(), other);
                        }
                        None => {
                            map.insert("text".into(), serde_json::Value::String(String::new()));
                        }
                    }
                }
            }
            for key in ["extra", "with"] {
                if let Some(child) = map.get_mut(key) {
                    resolve_component(child, selector, score, nbt);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                resolve_component(item, selector, score, nbt);
            }
        }
        _ => {}
    }
}

impl PlayPlan {
    /// `handleEditBook`: `Some(слот окна)`, если слот надо переслать клиенту.
    /// Слишком большая книга или правка чаще раза в секунду — кик, как в Paper.
    pub(super) async fn edit_book(
        &self,
        uuid: Uuid,
        author: &str,
        inv: &mut Inventory,
        e: EditBook,
        last_book_tick: &mut u64,
    ) -> Option<usize> {
        if !book_size_ok(&e.pages) {
            tracing::warn!(
                "{author} tried to send a book too large. Pages: {}",
                e.pages.len()
            );
            self.kick(uuid, &literal("Book too large!"), "ILLEGAL_ACTION");
            return None;
        }
        let now = self.server_ticks.load(Ordering::Relaxed);
        if *last_book_tick + 20 > now {
            self.kick(uuid, &literal("Book edited too quickly!"), "ILLEGAL_ACTION");
            return None;
        }
        *last_book_tick = now;
        let slot = book_window_slot(e.slot)?;
        let old = inv.slots[slot]?;
        let extra = components::get(old.extra);
        // `has(WRITABLE_BOOK_CONTENT)`: у черновика он из прототипа.
        let writable = self.item_id(WRITABLE_BOOK) == Some(old.item)
            || extra.book.as_ref().is_some_and(|b| !b.written);
        if !writable {
            return None;
        }
        let pages: Vec<components::Filterable> =
            e.pages.iter().map(|p| (p.clone(), None)).collect();
        let signing = e.title.is_some();
        let mut new_extra = (*extra).clone();
        // Сырой патч держал прежнюю книгу: новая пишется разобранной.
        new_extra.raw = None;
        let book = match e.title {
            // `signBook`: `transmuteCopy(WRITTEN_BOOK)` с подписью игрока.
            Some(title) => Book {
                written: true,
                pages: pages.iter().map(|(p, _)| (literal(p), None)).collect(),
                title: (title, None),
                author: author.to_owned(),
                generation: 0,
                resolved: true,
            },
            None => Book::writable(pages),
        };
        new_extra.book_pages = Some(book.page_count());
        new_extra.book = Some(book);
        let mut new = old;
        if signing {
            new.item = self.item_id(WRITTEN_BOOK)?;
        }
        new.extra = components::intern(new_extra);
        // `CraftEventFactory.handleEditBookEvent`: слот — только хотбар, иначе -1.
        let event_slot = if (0..=8).contains(&e.slot) {
            e.slot
        } else {
            -1
        };
        let reply = self
            .plugin_event(Event::EditBook, uuid, || {
                format!(
                    "{event_slot} {}\n{}\n{}",
                    u8::from(signing),
                    crate::stack_nbt::to_line(old, self.item_name(old.item)),
                    crate::stack_nbt::to_line(new, self.item_name(new.item)),
                )
            })
            .await;
        match reply.as_deref().map(str::trim) {
            // Отмена: книга прежняя, клиенту — настоящий слот.
            Some("c") => return Some(slot),
            // `s|w <стопка>`: плагин сменил мету или подпись.
            Some(r) if r.len() > 2 => {
                let (flag, line) = r.split_at(1);
                if let Some(meta) = crate::stack_nbt::from_line(line.trim(), |n| self.item_id(n)) {
                    let item = if flag == "s" {
                        self.item_id(WRITTEN_BOOK)
                    } else {
                        Some(old.item)
                    };
                    if let Some(item) = item {
                        new = Stack {
                            item,
                            count: old.count,
                            damage: meta.damage,
                            extra: meta.extra,
                        };
                    }
                }
            }
            _ => {}
        }
        inv.slots[slot] = Some(new);
        Some(slot)
    }

    /// `ServerPlayer.openItemGui`: у подписанной книги разрешить страницы
    /// (`resolveForItem`: селекторы и счёт) и открыть её клиенту.
    /// `Some(новая стопка)`, если книга изменилась и слот надо переслать.
    pub(super) fn open_written_book(
        &self,
        uuid: Uuid,
        name: &str,
        stack: Stack,
    ) -> (bool, Option<Stack>) {
        let extra = components::get(stack.extra);
        let Some(book) = extra.book.as_ref().filter(|b| b.written) else {
            // Без `written_book_content` ваниль книгу не открывает.
            return (false, None);
        };
        if book.resolved {
            return (true, None);
        }
        let source = crate::play::commands::dispatch::Source::player(uuid, name, 0);
        let ctx = crate::play::commands::Context::new(self);
        // `SelectorContents.resolve`: список имён через `separator` (по умолчанию ", ").
        let selector = |sel: &str, separator: Option<&str>| {
            let list = ctx.select_any(&source, sel, true).unwrap_or_default();
            let separator = separator.unwrap_or(", ");
            list.iter()
                .map(|target| target.name.as_str())
                .collect::<Vec<_>>()
                .join(separator)
        };
        // `ScoreContents.resolve`: селектор — первая цель, иначе имя как есть;
        // нет цели или счёта — пустой компонент.
        let score = |holder: &str, objective: &str| {
            let holder = if holder.starts_with('@') {
                ctx.select_any(&source, holder, true)
                    .ok()
                    .and_then(|list| list.into_iter().next())
                    .map_or_else(|| holder.to_owned(), |target| target.name)
            } else {
                holder.to_owned()
            };
            self.score(&holder, objective)
                .map(|value| value.to_string())
        };
        // `NbtContents.resolve`: данные сущности/блока по NBT-пути. `interpret`
        // разбирает строку как JSON-компонент, иначе — SNBT-текст.
        let nbt = |query: &NbtQuery| -> Option<serde_json::Value> {
            use crate::play::commands::snbt;
            let root = match &query.source {
                NbtSource::Entity(entity) => {
                    let target = ctx.select_first(&source, entity, true)?;
                    chunkwake_world::nbt::Tag::Compound(ctx.plan.entity_nbt(&target))
                }
                NbtSource::Block(pos) => {
                    let mut it = pos.split_whitespace();
                    let at = [
                        it.next()?.parse::<i32>().ok()?,
                        it.next()?.parse::<i32>().ok()?,
                        it.next()?.parse::<i32>().ok()?,
                    ];
                    if it.next().is_some() {
                        return None;
                    }
                    chunkwake_world::nbt::Tag::Compound(ctx.plan.block_nbt(at)?)
                }
            };
            let steps = snbt::parse_path(&query.path)?;
            let found = snbt::get_path(&root, &steps)?;
            if query.interpret {
                match found {
                    chunkwake_world::nbt::Tag::String(text) => serde_json::from_str(&text).ok(),
                    _ => None,
                }
            } else {
                Some(serde_json::Value::String(snbt::format_tag(&found)))
            }
        };
        let mut e = (*extra).clone();
        if let Some(book) = e.book.as_mut() {
            for (page, filtered) in &mut book.pages {
                let resolved = resolve_page(page, &selector, &score, &nbt);
                *page = resolved;
                if let Some(filtered) = filtered {
                    let resolved = resolve_page(filtered, &selector, &score, &nbt);
                    *filtered = resolved;
                }
            }
            book.resolved = true;
        }
        e.raw = None;
        (
            true,
            Some(Stack {
                extra: components::intern(e),
                ..stack
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_book_size_limit() {
        // Сто полных страниц ASCII по 1024 символа укладываются в бюджет.
        let full: Vec<String> = (0..100).map(|_| "a".repeat(1024)).collect();
        assert!(book_size_ok(&full));
        // Сто страниц по 1024 двухбайтовых символа — нет.
        let wide: Vec<String> = (0..100).map(|_| "ж".repeat(1024)).collect();
        assert!(!book_size_ok(&wide));
        assert!(book_size_ok(&[]));
    }

    #[test]
    fn book_slots_match_inventory() {
        assert_eq!(book_window_slot(0), Some(36));
        assert_eq!(book_window_slot(8), Some(44));
        assert_eq!(book_window_slot(40), Some(survival::OFFHAND));
        assert_eq!(book_window_slot(9), None);
    }

    #[test]
    fn page_resolves_selectors_and_scores() {
        let json = r#"{"text":"","extra":[{"selector":"@p","separator":" | "},{"score":{"name":"@a","objective":"kills"}},{"text":"!"}]}"#;
        let selector = |sel: &str, separator: Option<&str>| match sel {
            "@p" => ["Steve", "Alex"].join(separator.unwrap_or(", ")),
            _ => String::new(),
        };
        let score = |name: &str, objective: &str| match (name, objective) {
            ("@a", "kills") => Some("7".to_owned()),
            _ => None,
        };
        let value: serde_json::Value =
            serde_json::from_str(&resolve_page(json, &selector, &score, &|_| None)).unwrap();
        assert_eq!(value["extra"][0]["text"], "Steve | Alex");
        assert_eq!(value["extra"][1]["text"], "7");
        assert_eq!(value["extra"][2]["text"], "!");
        assert!(value["extra"][0].get("selector").is_none());
        assert!(value["extra"][1].get("score").is_none());
    }

    #[test]
    fn page_resolves_translate_arguments() {
        let json = r#"{"translate":"chat.type.text","with":[{"selector":"@p"},{"score":{"name":"Steve","objective":"deaths"}}]}"#;
        let selector = |_: &str, _: Option<&str>| "Steve".to_owned();
        let score = |name: &str, objective: &str| {
            (name == "Steve" && objective == "deaths").then(|| "0".to_owned())
        };
        let value: serde_json::Value =
            serde_json::from_str(&resolve_page(json, &selector, &score, &|_| None)).unwrap();
        assert_eq!(value["with"][0]["text"], "Steve");
        assert_eq!(value["with"][1]["text"], "0");
    }

    #[test]
    fn page_missing_score_and_plain_text_stay_usable() {
        let json = r#"{"score":{"name":"@p","objective":"missing"}}"#;
        let selector = |_: &str, _: Option<&str>| "Steve".to_owned();
        let score = |_: &str, _: &str| None;
        let value: serde_json::Value =
            serde_json::from_str(&resolve_page(json, &selector, &score, &|_| None)).unwrap();
        assert_eq!(value["text"], "");
        // Не компонент и битый JSON не трогаются.
        assert_eq!(
            resolve_page("plain text", &selector, &score, &|_| None),
            "plain text"
        );
        assert_eq!(resolve_page("{oops", &selector, &score, &|_| None), "{oops");
    }

    #[test]
    fn page_resolves_nbt_entity_and_interpreted_component() {
        // `{"nbt":"SelectedItem.id","entity":"@p"}` — строка выводится SNBT.
        let json = r#"{"text":"","extra":[{"nbt":"SelectedItem.id","entity":"@p"},{"nbt":"Display","entity":"@p","interpret":true}]}"#;
        let nbt = |query: &NbtQuery| match (query.path.as_str(), &query.source, query.interpret) {
            ("SelectedItem.id", NbtSource::Entity(sel), false) if sel == "@p" => {
                Some(serde_json::Value::String("\"minecraft:stone\"".into()))
            }
            ("Display", NbtSource::Entity(sel), true) if sel == "@p" => {
                Some(serde_json::json!({"text": "Hi", "color": "gold"}))
            }
            _ => None,
        };
        let selector = |_: &str, _: Option<&str>| "Steve".to_owned();
        let score = |_: &str, _: &str| None;
        let value: serde_json::Value =
            serde_json::from_str(&resolve_page(json, &selector, &score, &nbt)).unwrap();
        assert_eq!(value["extra"][0]["text"], "\"minecraft:stone\"");
        assert_eq!(value["extra"][1]["text"], "Hi");
        assert_eq!(value["extra"][1]["color"], "gold");
        assert!(value["extra"][0].get("nbt").is_none());
        assert!(value["extra"][1].get("interpret").is_none());
    }

    #[test]
    fn page_nbt_missing_value_is_empty_text() {
        let json = r#"{"nbt":"Absent","entity":"@p"}"#;
        let nbt = |_: &NbtQuery| None;
        let selector = |_: &str, _: Option<&str>| String::new();
        let score = |_: &str, _: &str| None;
        let value: serde_json::Value =
            serde_json::from_str(&resolve_page(json, &selector, &score, &nbt)).unwrap();
        assert_eq!(value["text"], "");
    }
}
