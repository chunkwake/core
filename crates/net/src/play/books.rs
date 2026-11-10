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
    /// (`resolveForItem`) и открыть её клиенту. `Some(новая стопка)`, если
    /// книга изменилась и слот надо переслать.
    pub(super) fn open_written_book(&self, stack: Stack) -> (bool, Option<Stack>) {
        let extra = components::get(stack.extra);
        let Some(book) = extra.book.as_ref().filter(|b| b.written) else {
            // Без `written_book_content` ваниль книгу не открывает.
            return (false, None);
        };
        if book.resolved {
            return (true, None);
        }
        // Компоненты с селекторами и счётом не разрешаются: только флаг.
        let mut e = (*extra).clone();
        if let Some(b) = e.book.as_mut() {
            b.resolved = true;
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
}
