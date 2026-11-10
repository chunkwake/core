//! Предметы, инвентарь игрока, здоровье и респаун.

use crate::play::{SpawnInfo, write_spawn_info};
use crate::{Reader, Result, Writer};

/// Стопка предметов: ID в реестре `item`, количество и износ (`minecraft:damage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stack {
    pub item: i32,
    pub count: i32,
    pub damage: i32,
    /// Остальные компоненты ([`crate::components`]): 0 — нет.
    pub extra: u32,
}

impl Stack {
    pub fn new(item: i32, count: i32) -> Self {
        Self {
            item,
            count,
            damage: 0,
            extra: 0,
        }
    }

    /// Та же вещь (для складывания в стопку): тип, износ и компоненты.
    pub fn same_kind(&self, o: &Stack) -> bool {
        self.item == o.item && self.damage == o.damage && self.extra == o.extra
    }
}

/// Номер компонента `minecraft:damage` в реестре `data_component_type` (ставится при старте).
pub static DAMAGE_COMPONENT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Слот: `None` — пусто.
pub type Slot = Option<Stack>;

fn book_page_count(kind: i32, payload: &[u8]) -> Option<i32> {
    use crate::components::{
        WRITABLE_BOOK_CONTENT_COMPONENT, WRITTEN_BOOK_CONTENT_COMPONENT, id_of,
    };
    let mut r = Reader::new(payload);
    let count = if kind == id_of(&WRITABLE_BOOK_CONTENT_COMPONENT) {
        r.var_int().ok()?
    } else if kind == id_of(&WRITTEN_BOOK_CONTENT_COMPONENT) {
        r.string(32).ok()?;
        if r.bool().ok()? {
            r.string(32).ok()?;
        }
        r.string(32767).ok()?;
        r.var_int().ok()?;
        r.var_int().ok()?
    } else {
        return None;
    };
    (0..=100).contains(&count).then_some(count.max(1))
}

/// `Filterable.streamCodec`: значение и необязательная отфильтрованная версия.
fn read_filterable(
    r: &mut Reader,
    read: &dyn Fn(&mut Reader) -> Result<String>,
) -> Result<crate::components::Filterable> {
    let raw = read(r)?;
    let filtered = if r.bool()? { Some(read(r)?) } else { None };
    Ok((raw, filtered))
}

fn write_filterable(
    w: &mut Writer,
    v: &crate::components::Filterable,
    write: &dyn Fn(&mut Writer, &str),
) {
    write(w, &v.0);
    w.bool(v.1.is_some());
    if let Some(f) = &v.1 {
        write(w, f);
    }
}

/// Тело `writable_book_content` / `written_book_content` (`WritableBookContent`
/// и `WrittenBookContent.STREAM_CODEC`).
fn read_book(r: &mut Reader, written: bool) -> Result<crate::components::Book> {
    use crate::components::Book;
    let text = |r: &mut Reader| Ok(crate::text::read_component(r)?.to_string());
    if !written {
        let n = r.var_int()?;
        if !(0..=100).contains(&n) {
            return Err(crate::ProtocolError::BadLength(n));
        }
        let mut pages = Vec::with_capacity(n as usize);
        for _ in 0..n {
            pages.push(read_filterable(r, &|r| Ok(r.string(1024)?.to_owned()))?);
        }
        return Ok(Book::writable(pages));
    }
    let title = read_filterable(r, &|r| Ok(r.string(32)?.to_owned()))?;
    let author = r.string(32767)?.to_owned();
    let generation = r.var_int()?;
    let n = r.var_int()?;
    if !(0..=100).contains(&n) {
        return Err(crate::ProtocolError::BadLength(n));
    }
    let mut pages = Vec::with_capacity(n as usize);
    for _ in 0..n {
        pages.push(read_filterable(r, &text)?);
    }
    Ok(Book {
        written: true,
        pages,
        title,
        author,
        generation,
        resolved: r.bool()?,
    })
}

fn write_book(w: &mut Writer, b: &crate::components::Book) {
    let text = |w: &mut Writer, t: &str| {
        let v = serde_json::from_str(t).unwrap_or_else(|_| serde_json::Value::String(t.to_owned()));
        crate::text::write_component(w, &v);
    };
    let plain = |w: &mut Writer, t: &str| {
        w.string(t);
    };
    if b.written {
        write_filterable(w, &b.title, &plain);
        w.string(&b.author).var_int(b.generation);
    }
    w.var_int(b.pages.len() as i32);
    for p in &b.pages {
        write_filterable(w, p, if b.written { &text } else { &plain });
    }
    if b.written {
        w.bool(b.resolved);
    }
}

/// `ItemStack.OPTIONAL_STREAM_CODEC`: количество, затем ID и пустой патч компонентов.
pub fn write_stack(w: &mut Writer, s: Slot) {
    match s {
        Some(s) if s.count > 0 => {
            w.var_int(s.count).var_int(s.item);
            write_patch(w, &s);
        }
        _ => {
            w.var_int(0);
        }
    }
}

/// Патч компонентов стопки: добавленные (износ, чары, имя, описание), удалённых нет.
/// Если патч пришёл с неизвестным компонентом, он пишется байт в байт.
fn write_patch(w: &mut Writer, s: &Stack) {
    use crate::components::{
        CHARGED_PROJECTILES_COMPONENT, CUSTOM_NAME_COMPONENT, ENCHANTMENTS_COMPONENT,
        LORE_COMPONENT, STORED_ENCHANTMENTS_COMPONENT, get, id_of,
    };
    let e = get(s.extra);
    if let Some(raw) = &e.raw {
        let trim_id = id_of(&crate::components::TRIM_COMPONENT);
        let banner_id = id_of(&crate::components::BANNER_PATTERNS_COMPONENT);
        let trim = e.trim.filter(|_| trim_id >= 0);
        let banner =
            (!e.banner_patterns.is_empty() && banner_id >= 0).then_some(&e.banner_patterns);
        if trim.is_some() || banner.is_some() {
            let mut r = Reader::new(raw);
            if let (Ok(added), Ok(removed)) = (r.var_int(), r.var_int())
                && added >= 0
                && removed >= 0
            {
                let mut tail = raw.len() - r.remaining().len();
                let mut skipped = 0;
                // Серверные компоненты в сыром патче всегда впереди неизвестного хвоста.
                while skipped < added {
                    let mut probe = Reader::new(&raw[tail..]);
                    let Ok(kind) = probe.var_int() else { break };
                    if kind == trim_id && trim.is_some() {
                        if probe.var_int().is_err() || probe.var_int().is_err() {
                            break;
                        }
                    } else if kind == banner_id && banner.is_some() {
                        let Ok(n) = probe.var_int() else { break };
                        if !(0..=256).contains(&n) {
                            break;
                        }
                        let mut ok = true;
                        for _ in 0..n {
                            if probe.var_int().is_err() || probe.var_int().is_err() {
                                ok = false;
                                break;
                            }
                        }
                        if !ok {
                            break;
                        }
                    } else {
                        break;
                    }
                    tail = raw.len() - probe.remaining().len();
                    skipped += 1;
                }
                w.var_int(
                    added - skipped + i32::from(trim.is_some()) + i32::from(banner.is_some()),
                )
                .var_int(removed);
                if let Some((material, pattern)) = trim {
                    w.var_int(trim_id)
                        .var_int(material.saturating_add(1))
                        .var_int(pattern.saturating_add(1));
                }
                if let Some(layers) = banner {
                    w.var_int(banner_id).var_int(layers.len() as i32);
                    for &(pattern, color) in layers {
                        w.var_int(pattern.saturating_add(1)).var_int(color);
                    }
                }
                w.buf.extend_from_slice(&raw[tail..]);
                return;
            }
        }
        w.buf.extend_from_slice(raw);
        return;
    }
    let damage = id_of(&DAMAGE_COMPONENT);
    let (ench, name, lore) = (
        id_of(&ENCHANTMENTS_COMPONENT),
        id_of(&CUSTOM_NAME_COMPONENT),
        id_of(&LORE_COMPONENT),
    );
    let parts = [
        s.damage > 0 && damage >= 0,
        !e.enchantments.is_empty() && ench >= 0,
        e.custom_name.is_some() && name >= 0,
        !e.lore.is_empty() && lore >= 0,
        !e.stored.is_empty() && id_of(&STORED_ENCHANTMENTS_COMPONENT) >= 0,
        e.custom_data.is_some() && id_of(&crate::components::CUSTOM_DATA_COMPONENT) >= 0,
        !e.charged.is_empty() && id_of(&CHARGED_PROJECTILES_COMPONENT) >= 0,
        e.repair_cost > 0 && id_of(&crate::components::REPAIR_COST_COMPONENT) >= 0,
        e.trim.is_some() && id_of(&crate::components::TRIM_COMPONENT) >= 0,
        !e.banner_patterns.is_empty() && id_of(&crate::components::BANNER_PATTERNS_COMPONENT) >= 0,
        e.map_id.is_some() && id_of(&crate::components::MAP_ID_COMPONENT) >= 0,
        e.map_post_processing.is_some()
            && id_of(&crate::components::MAP_POST_PROCESSING_COMPONENT) >= 0,
        e.book.is_some() && book_id(e.book.as_ref()) >= 0,
    ];
    w.var_int(parts.iter().filter(|&&p| p).count() as i32)
        .var_int(0);
    if parts[0] {
        w.var_int(damage).var_int(s.damage);
    }
    if parts[1] {
        w.var_int(ench).var_int(e.enchantments.len() as i32);
        for &(id, lvl) in &e.enchantments {
            w.var_int(id).var_int(lvl);
        }
    }
    let json = |t: &str| {
        serde_json::from_str(t).unwrap_or_else(|_| serde_json::Value::String(t.to_owned()))
    };
    if let (true, Some(n)) = (parts[2], &e.custom_name) {
        w.var_int(name);
        crate::text::write_component(w, &json(n));
    }
    if parts[4] {
        w.var_int(id_of(&STORED_ENCHANTMENTS_COMPONENT))
            .var_int(e.stored.len() as i32);
        for &(id, lvl) in &e.stored {
            w.var_int(id).var_int(lvl);
        }
    }
    if parts[3] {
        w.var_int(lore).var_int(e.lore.len() as i32);
        for l in &e.lore {
            crate::text::write_component(w, &json(l));
        }
    }
    if let (true, Some(nbt)) = (parts[5], &e.custom_data) {
        w.var_int(id_of(&crate::components::CUSTOM_DATA_COMPONENT));
        w.buf.extend_from_slice(nbt);
    }
    if parts[6] {
        w.var_int(id_of(&CHARGED_PROJECTILES_COMPONENT))
            .var_int(e.charged.len() as i32);
        for st in &e.charged {
            write_stack(w, Some(*st));
        }
    }
    if parts[7] {
        w.var_int(id_of(&crate::components::REPAIR_COST_COMPONENT))
            .var_int(e.repair_cost);
    }
    if let (true, Some((material, pattern))) = (parts[8], e.trim) {
        w.var_int(id_of(&crate::components::TRIM_COMPONENT))
            .var_int(material.saturating_add(1))
            .var_int(pattern.saturating_add(1));
    }
    if parts[9] {
        w.var_int(id_of(&crate::components::BANNER_PATTERNS_COMPONENT))
            .var_int(e.banner_patterns.len() as i32);
        for &(pattern, color) in &e.banner_patterns {
            w.var_int(pattern.saturating_add(1)).var_int(color);
        }
    }
    if let (true, Some(id)) = (parts[10], e.map_id) {
        w.var_int(id_of(&crate::components::MAP_ID_COMPONENT))
            .var_int(id);
    }
    if let (true, Some(p)) = (parts[11], e.map_post_processing) {
        w.var_int(id_of(&crate::components::MAP_POST_PROCESSING_COMPONENT))
            .var_int(i32::from(p));
    }
    if let (true, Some(book)) = (parts[12], &e.book) {
        w.var_int(book_id(Some(book)));
        write_book(w, book);
    }
}

/// Номер компонента книги в реестре; -1 — нет книги или реестр не задан.
fn book_id(book: Option<&crate::components::Book>) -> i32 {
    use crate::components::{
        WRITABLE_BOOK_CONTENT_COMPONENT, WRITTEN_BOOK_CONTENT_COMPONENT, id_of,
    };
    match book {
        Some(b) if b.written => id_of(&WRITTEN_BOOK_CONTENT_COMPONENT),
        Some(_) => id_of(&WRITABLE_BOOK_CONTENT_COMPONENT),
        None => -1,
    }
}

/// Читает `ItemStack` с компонентами: износ, чары, имя и описание.
/// Неизвестный компонент (состав зелья и т. п.) сохраняется сырым патчем целиком.
/// Стопка с таким патчем должна быть последним полем пакета: хвост не разбирается.
pub fn read_stack(r: &mut Reader) -> Result<Slot> {
    read_stack_at(r, 0)
}

/// Предел вложенности стопок (`charged_projectiles` внутри `charged_projectiles`).
/// У ванили арбалет в арбалете не бывает; без предела клиент цепочкой в сотню
/// тысяч уровней переполнял стек, а `panic = "abort"` ронял сервер.
const MAX_STACK_DEPTH: u8 = 4;

fn read_stack_at(r: &mut Reader, depth: u8) -> Result<Slot> {
    if depth > MAX_STACK_DEPTH {
        return Err(crate::ProtocolError::BadLength(i32::from(depth)));
    }
    use crate::components::{
        CHARGED_PROJECTILES_COMPONENT, CUSTOM_NAME_COMPONENT, ENCHANTMENTS_COMPONENT, Extra,
        LORE_COMPONENT, STORED_ENCHANTMENTS_COMPONENT, id_of, intern,
    };
    let count = r.var_int()?;
    if count <= 0 {
        return Ok(None);
    }
    let item = r.var_int()?;
    let mut s = Stack::new(item, count);
    let mut e = Extra::default();
    let patch = r.remaining().to_vec();
    let added = r.var_int()?;
    let removed = r.var_int()?;
    let mut known = true;
    for _ in 0..added {
        let t = r.var_int()?;
        if t == id_of(&DAMAGE_COMPONENT) {
            s.damage = r.var_int()?.max(0);
        } else if t == id_of(&ENCHANTMENTS_COMPONENT) {
            for _ in 0..r.var_int()?.clamp(0, 256) {
                e.enchantments.push((r.var_int()?, r.var_int()?));
            }
        } else if t == id_of(&CUSTOM_NAME_COMPONENT) {
            e.custom_name = Some(crate::text::read_component(r)?.to_string());
        } else if t == id_of(&crate::components::REPAIR_COST_COMPONENT) {
            e.repair_cost = r.var_int()?.max(0);
        } else if t == id_of(&crate::components::MAP_ID_COMPONENT) {
            e.map_id = Some(r.var_int()?);
        } else if t == id_of(&crate::components::MAP_POST_PROCESSING_COMPONENT) {
            e.map_post_processing = Some(r.var_int()?.clamp(0, 1) as u8);
        } else if t == id_of(&crate::components::TRIM_COMPONENT) {
            let material = r.var_int()?;
            let pattern = r.var_int()?;
            if material > 0 && pattern > 0 {
                e.trim = Some((material - 1, pattern - 1));
            }
        } else if t == id_of(&crate::components::BANNER_PATTERNS_COMPONENT) {
            for _ in 0..r.var_int()?.clamp(0, 256) {
                let pattern = r.var_int()?;
                let color = r.var_int()?;
                if pattern > 0 && (0..16).contains(&color) {
                    e.banner_patterns.push((pattern - 1, color));
                }
            }
        } else if t == id_of(&crate::components::WRITTEN_BOOK_CONTENT_COMPONENT)
            || t == id_of(&crate::components::WRITABLE_BOOK_CONTENT_COMPONENT)
        {
            let written = t == id_of(&crate::components::WRITTEN_BOOK_CONTENT_COMPONENT);
            let start = r.remaining().to_vec();
            match read_book(r, written) {
                Ok(book) => {
                    e.book_pages = Some(book.page_count());
                    e.book = Some(book);
                }
                // Не разобрали — патч целиком сырым, как раньше.
                Err(_) => {
                    e.book_pages = book_page_count(t, &start);
                    known = false;
                    break;
                }
            }
        } else if t == id_of(&STORED_ENCHANTMENTS_COMPONENT) {
            for _ in 0..r.var_int()?.clamp(0, 256) {
                e.stored.push((r.var_int()?, r.var_int()?));
            }
        } else if t == id_of(&LORE_COMPONENT) {
            for _ in 0..r.var_int()?.clamp(0, 256) {
                e.lore.push(crate::text::read_component(r)?.to_string());
            }
        } else if id_of(&CHARGED_PROJECTILES_COMPONENT) >= 0
            && t == id_of(&CHARGED_PROJECTILES_COMPONENT)
        {
            for _ in 0..r.var_int()?.clamp(0, 16) {
                if let Some(st) = read_stack_at(r, depth + 1)? {
                    e.charged.push(st);
                }
            }
        } else {
            known = false;
            break;
        }
    }
    if known {
        for _ in 0..removed {
            r.var_int()?;
        }
    } else if !patch.is_empty() {
        e.raw = Some(patch);
    }
    s.extra = intern(e);
    Ok(Some(s))
}

/// `HashedStack`: есть ли предмет, ID, количество, хэши компонентов.
fn read_hashed_stack(r: &mut Reader) -> Result<Slot> {
    if !r.bool()? {
        return Ok(None);
    }
    let item = r.var_int()?;
    let count = r.var_int()?;
    // Добавленные компоненты: (тип, хэш int), удалённые: типы.
    for _ in 0..r.var_int()? {
        r.var_int()?;
        r.bytes(4)?;
    }
    for _ in 0..r.var_int()? {
        r.var_int()?;
    }
    Ok((count > 0).then_some(Stack::new(item, count)))
}

/// Окно инвентаря игрока.
pub const PLAYER_WINDOW: i32 = 0;

/// `ClientboundOpenScreenPacket`: номер окна, тип меню (`menu`) и заголовок.
pub fn open_screen(id: i32, window: i32, menu: i32, title: &serde_json::Value) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(window).var_int(menu);
    crate::text::write_component(&mut w, title);
    w.buf
}

/// `ClientboundContainerClosePacket`.
pub fn container_close(id: i32, window: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(window);
    w.buf
}

pub fn container_set_content(
    id: i32,
    window: i32,
    state: i32,
    slots: &[Slot],
    carried: Slot,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(window)
        .var_int(state)
        .var_int(slots.len() as i32);
    for s in slots {
        write_stack(&mut w, *s);
    }
    write_stack(&mut w, carried);
    w.buf
}

/// `ClientboundUpdateMobEffectPacket`: флаги 1 — фоновый, 2 — частицы, 4 — значок.
pub fn update_mob_effect(
    id: i32,
    entity: i32,
    effect: i32,
    amplifier: i32,
    duration: i32,
    flags: u8,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity)
        .var_int(effect)
        .var_int(amplifier)
        .var_int(duration)
        .u8(flags);
    w.buf
}

/// `ClientboundUpdateAttributesPacket` для одного атрибута: база и модификаторы
/// `(идентификатор, величина, операция)`; операции: 0 прибавить, 1 × базу, 2 × итог.
pub fn update_attribute(
    id: i32,
    entity: i32,
    attribute: i32,
    base: f64,
    modifiers: &[(String, f64, i32)],
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity)
        .var_int(1)
        .var_int(attribute)
        .f64(base);
    w.var_int(modifiers.len() as i32);
    for (name, amount, op) in modifiers {
        w.string(name).f64(*amount).var_int(*op);
    }
    w.buf
}

/// `ClientboundRemoveMobEffectPacket`.
pub fn remove_mob_effect(id: i32, entity: i32, effect: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity).var_int(effect);
    w.buf
}

/// `ClientboundContainerSetDataPacket`: свойство окна (прогресс печи).
pub fn container_set_data(id: i32, window: i32, property: i16, value: i16) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(window)
        .u16(property as u16)
        .u16(value as u16);
    w.buf
}

pub fn container_set_slot(id: i32, window: i32, state: i32, slot: i16, stack: Slot) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(window)
        .var_int(state)
        .u16(slot as u16);
    write_stack(&mut w, stack);
    w.buf
}

/// `ServerboundContainerClickPacket` после ID. Клиент присылает итог клика:
/// изменённые слоты и предмет на курсоре.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerClick {
    pub window: i32,
    pub state: i32,
    pub slot: i16,
    pub button: i8,
    /// `ContainerInput`: 0 PICKUP, 1 QUICK_MOVE, 2 SWAP, 3 CLONE, 4 THROW, 5 QUICK_CRAFT, 6 PICKUP_ALL.
    pub mode: i32,
    pub changed: Vec<(i16, Slot)>,
    pub carried: Slot,
}

/// Предел изменённых слотов в одном клике (как в ванилле).
const MAX_CHANGED: i32 = 128;

pub fn read_container_click(r: &mut Reader) -> Result<ContainerClick> {
    let window = r.var_int()?;
    let state = r.var_int()?;
    let slot = r.u16()? as i16;
    let button = r.u8()? as i8;
    let mode = r.var_int()?;
    let n = r.var_int()?;
    if !(0..=MAX_CHANGED).contains(&n) {
        return Err(crate::ProtocolError::BadLength(n));
    }
    let mut changed = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let s = r.u16()? as i16;
        changed.push((s, read_hashed_stack(r)?));
    }
    let carried = read_hashed_stack(r)?;
    Ok(ContainerClick {
        window,
        state,
        slot,
        button,
        mode,
        changed,
        carried,
    })
}

/// `ServerboundSetCreativeModeSlotPacket`: слот и стопка.
///
/// В отличие от клиентских пакетов, здесь `DataComponentPatch.DELIMITED_STREAM_CODEC`:
/// у каждого добавленного компонента перед данными стоит длина. Её снимаем, иначе
/// при отдаче инвентаря клиент читает длину как состав зелья и показывает пустое.
pub fn read_creative_slot(r: &mut Reader) -> Result<(i16, Slot)> {
    let slot = r.u16()? as i16;
    Ok((slot, read_untrusted_stack(r)?))
}

/// Стопка из креатива: патч с длиной у каждого компонента → обычный патч для ответа клиенту.
fn read_untrusted_stack(r: &mut Reader) -> Result<Slot> {
    use crate::components::{
        CHARGED_PROJECTILES_COMPONENT, CUSTOM_NAME_COMPONENT, ENCHANTMENTS_COMPONENT, Extra,
        LORE_COMPONENT, STORED_ENCHANTMENTS_COMPONENT, id_of, intern,
    };
    let count = r.var_int()?;
    if count <= 0 {
        return Ok(None);
    }
    let item = r.var_int()?;
    let mut s = Stack::new(item, count);
    let mut e = Extra::default();
    let added = r.var_int()?;
    let removed = r.var_int()?;
    if !(0..=256).contains(&added) || !(0..=256).contains(&removed) {
        return Err(crate::ProtocolError::BadLength(added));
    }
    let mut patch = Writer::new();
    let mut known = true;
    let mut raw_added = 0;
    for _ in 0..added {
        let kind = r.var_int()?;
        let len = r.var_int()?;
        if !(0..=1 << 20).contains(&len) {
            return Err(crate::ProtocolError::BadLength(len));
        }
        let payload = r.bytes(len as usize)?;
        if kind != id_of(&crate::components::TRIM_COMPONENT)
            && kind != id_of(&crate::components::BANNER_PATTERNS_COMPONENT)
        {
            raw_added += 1;
            patch.var_int(kind);
            patch.buf.extend_from_slice(payload);
        }
        let mut body = Reader::new(payload);
        if kind == id_of(&DAMAGE_COMPONENT)
            && let Ok(d) = body.var_int()
        {
            s.damage = d.max(0);
        } else if kind == id_of(&ENCHANTMENTS_COMPONENT)
            && let Ok(n) = body.var_int()
        {
            for _ in 0..n.clamp(0, 256) {
                let Ok(id) = body.var_int() else { break };
                let Ok(lvl) = body.var_int() else { break };
                e.enchantments.push((id, lvl));
            }
        } else if kind == id_of(&STORED_ENCHANTMENTS_COMPONENT)
            && let Ok(n) = body.var_int()
        {
            for _ in 0..n.clamp(0, 256) {
                let Ok(id) = body.var_int() else { break };
                let Ok(lvl) = body.var_int() else { break };
                e.stored.push((id, lvl));
            }
        } else if kind == id_of(&CUSTOM_NAME_COMPONENT)
            && let Ok(text) = crate::text::read_component(&mut body)
        {
            e.custom_name = Some(text.to_string());
        } else if kind == id_of(&crate::components::REPAIR_COST_COMPONENT)
            && let Ok(cost) = body.var_int()
        {
            e.repair_cost = cost.max(0);
        } else if kind == id_of(&crate::components::MAP_ID_COMPONENT)
            && let Ok(id) = body.var_int()
        {
            e.map_id = Some(id);
        } else if kind == id_of(&crate::components::MAP_POST_PROCESSING_COMPONENT)
            && let Ok(p) = body.var_int()
        {
            e.map_post_processing = Some(p.clamp(0, 1) as u8);
        } else if kind == id_of(&crate::components::TRIM_COMPONENT)
            && let (Ok(material), Ok(pattern)) = (body.var_int(), body.var_int())
        {
            if material > 0 && pattern > 0 {
                e.trim = Some((material - 1, pattern - 1));
            }
        } else if kind == id_of(&crate::components::BANNER_PATTERNS_COMPONENT)
            && let Ok(n) = body.var_int()
        {
            for _ in 0..n.clamp(0, 256) {
                let (Ok(pattern), Ok(color)) = (body.var_int(), body.var_int()) else {
                    break;
                };
                if pattern > 0 && (0..16).contains(&color) {
                    e.banner_patterns.push((pattern - 1, color));
                }
            }
        } else if kind == id_of(&crate::components::WRITTEN_BOOK_CONTENT_COMPONENT)
            || kind == id_of(&crate::components::WRITABLE_BOOK_CONTENT_COMPONENT)
        {
            e.book_pages = book_page_count(kind, payload);
            known = false;
        } else if kind == id_of(&LORE_COMPONENT)
            && let Ok(n) = body.var_int()
        {
            for _ in 0..n.clamp(0, 256) {
                let Ok(line) = crate::text::read_component(&mut body) else {
                    break;
                };
                e.lore.push(line.to_string());
            }
        } else if id_of(&CHARGED_PROJECTILES_COMPONENT) >= 0
            && kind == id_of(&CHARGED_PROJECTILES_COMPONENT)
            && let Ok(n) = body.var_int()
        {
            for _ in 0..n.clamp(0, 16) {
                if let Ok(Some(st)) = read_stack_at(&mut body, 1) {
                    e.charged.push(st);
                }
            }
        } else {
            known = false;
        }
    }
    for _ in 0..removed {
        patch.var_int(r.var_int()?);
    }
    if !known || removed != 0 {
        let mut raw = Writer::new();
        raw.var_int(raw_added).var_int(removed);
        raw.buf.extend_from_slice(&patch.buf);
        e.raw = Some(raw.buf);
    }
    s.extra = intern(e);
    Ok(Some(s))
}

pub fn set_held_slot(id: i32, slot: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(slot);
    w.buf
}

/// `ClientboundSetExperiencePacket`: полоска, уровень, всего очков.
pub fn set_experience(id: i32, progress: f32, level: i32, total: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).f32(progress).var_int(level).var_int(total);
    w.buf
}

pub fn set_health(id: i32, health: f32, food: i32, saturation: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).f32(health).var_int(food).f32(saturation);
    w.buf
}

/// `ClientboundRespawnPacket`: то же описание мира, что при входе, и что сохранить.
pub fn respawn(id: i32, spawn: &SpawnInfo, keep: u8) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    write_spawn_info(&mut w, spawn);
    w.u8(keep);
    w.buf
}

/// `ServerboundClientCommandPacket.Action::PERFORM_RESPAWN`.
pub const PERFORM_RESPAWN: i32 = 0;

/// `ClientboundDamageEventPacket` без источника-сущности и позиции.
/// `damage_type` — сетевой ID в реестре `damage_type`.
pub fn damage_event(id: i32, entity_id: i32, damage_type: i32) -> Vec<u8> {
    let mut w = Writer::new();
    // Holder реестра: ID + 1; ID сущностей-источников: 0 — нет.
    w.var_int(id)
        .var_int(entity_id)
        .var_int(damage_type + 1)
        .var_int(0)
        .var_int(0)
        .bool(false);
    w.buf
}

/// `ClientboundEntityEventPacket`: 3 — смерть живой сущности.
pub fn entity_event(id: i32, entity_id: i32, event: u8) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).i32(entity_id).u8(event);
    w.buf
}

/// `set_entity_data`: общие флаги сущности (индекс 0, сериализатор `BYTE` = 0).
/// Бит 1 — присед, бит 3 — бег, бит 7 — полёт на элитрах.
pub fn set_entity_flags(id: i32, entity_id: i32, flags: u8) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .u8(0)
        .var_int(0)
        .u8(flags)
        .u8(0xff);
    w.buf
}

/// Модель игрока (`Avatar`): основная рука (15, `HUMANOID_ARM` = 42) и
/// видимые части скина (16, `BYTE` = 0).
/// Пишутся только поля `Some`: как грязные значения `SynchedEntityData`.
pub fn set_player_model(
    id: i32,
    entity_id: i32,
    main_hand: Option<u8>,
    skin_parts: Option<u8>,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    if let Some(hand) = main_hand {
        w.u8(15).var_int(42).var_int(i32::from(hand));
    }
    if let Some(skin) = skin_parts {
        w.u8(16).var_int(0).u8(skin);
    }
    w.u8(0xff);
    w.buf
}

/// Общие метаданные сущности: запас воздуха (1) и тики замерзания (7).
pub fn set_entity_air(id: i32, entity_id: i32, air: i32) -> Vec<u8> {
    set_entity_int(id, entity_id, 1, air)
}

pub fn set_entity_frozen(id: i32, entity_id: i32, ticks: i32) -> Vec<u8> {
    set_entity_int(id, entity_id, 7, ticks)
}

fn set_entity_int(id: i32, entity_id: i32, index: u8, value: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .u8(index)
        .var_int(1)
        .var_int(value)
        .u8(0xff);
    w.buf
}

/// Поза игрока и позиция кровати в `set_entity_data` (Entity.DATA_POSE и
/// LivingEntity.SLEEPING_POS_ID). Пустая позиция возвращает позу стоя.
pub fn set_entity_sleeping(id: i32, entity_id: i32, bed: Option<[i32; 3]>) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .u8(6)
        .var_int(20)
        .var_int(if bed.is_some() { 2 } else { 0 })
        .u8(14)
        .var_int(11)
        .bool(bed.is_some());
    if let Some(pos) = bed {
        w.i64(crate::play::pack_block_pos(pos));
    }
    w.u8(0xff);
    w.buf
}

/// `set_entity_data` предмета-сущности: `ItemEntity.DATA_ITEM` (индекс 8,
/// сериализатор `ITEM_STACK` = 7).
pub fn set_entity_item(id: i32, entity_id: i32, stack: Slot) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id).u8(8).var_int(7);
    write_stack(&mut w, stack);
    w.u8(0xff);
    w.buf
}

/// Величина сферы опыта (`ExperienceOrb.DATA_VALUE`, индекс 8, int).
pub fn set_orb_value(id: i32, entity_id: i32, value: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .u8(8)
        .var_int(1)
        .var_int(value)
        .u8(0xff);
    w.buf
}

/// `FireworkRocketEntity.DATA_ATTACHED_TO_TARGET` (индекс 9, сериализатор
/// `OPTIONAL_UNSIGNED_INT` = 19: `id + 1`, 0 — пусто). Клиент сам ведёт
/// ракету за сущностью и сам разгоняет своего игрока на элитрах.
pub fn set_firework_attached(id: i32, entity_id: i32, attached: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .u8(9)
        .var_int(19)
        .var_int(attached + 1)
        .u8(0xff);
    w.buf
}

/// `PrimedTnt.DATA_FUSE_ID`: оставшиеся тики фитиля (индекс 8, int).
pub fn set_tnt_fuse(id: i32, entity_id: i32, fuse: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .u8(8)
        .var_int(1)
        .var_int(fuse)
        .u8(0xff);
    w.buf
}

/// Данные лодки (`VehicleEntity`/`AbstractBoat`): время удара (8, int),
/// его сторона (9, int), накопленный урон (10, float) и вёсла (11, 12, boolean).
pub fn boat_data(
    id: i32,
    entity_id: i32,
    hurt: i32,
    hurt_dir: i32,
    damage: f32,
    paddles: (bool, bool),
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    w.u8(8).var_int(1).var_int(hurt);
    w.u8(9).var_int(1).var_int(hurt_dir);
    w.u8(10).var_int(3).f32(damage);
    w.u8(11).var_int(8).bool(paddles.0);
    w.u8(12).var_int(8).bool(paddles.1);
    w.u8(0xff);
    w.buf
}

/// Удар по транспорту (`VehicleEntity`): время удара (8), сторона (9), урон (10).
pub fn boat_data_hurt(id: i32, entity_id: i32, hurt: i32, hurt_dir: i32, damage: f32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    w.u8(8).var_int(1).var_int(hurt);
    w.u8(9).var_int(1).var_int(hurt_dir);
    w.u8(10).var_int(3).f32(damage);
    w.u8(0xff);
    w.buf
}

/// Данные рамки (`HangingEntity`/`ItemFrame`): невидимость (флаги 0, бит 0x20),
/// сторона (8, `DIRECTION` = 12, 3D-номер), предмет (9, `ITEM_STACK`) и
/// поворот предмета (10, int). Пишутся только переданные поля — как
/// `packDirty`/`getNonDefaultValues` у ванили.
pub fn item_frame_data(
    id: i32,
    entity_id: i32,
    facing: Option<i32>,
    item: Option<Slot>,
    rotation: Option<i32>,
    invisible: Option<bool>,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id);
    if let Some(v) = invisible {
        w.u8(0).var_int(0).u8(if v { 0x20 } else { 0 });
    }
    if let Some(v) = facing {
        w.u8(8).var_int(12).var_int(v);
    }
    if let Some(v) = item {
        w.u8(9).var_int(7);
        write_stack(&mut w, v);
    }
    if let Some(v) = rotation {
        w.u8(10).var_int(1).var_int(v);
    }
    w.u8(0xff);
    w.buf
}

/// `MinecartFurnace.DATA_ID_FUEL` (индекс 13, boolean): печь горит.
pub fn furnace_cart_fuel(id: i32, entity_id: i32, lit: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(entity_id)
        .u8(13)
        .var_int(8)
        .bool(lit)
        .u8(0xff);
    w.buf
}

/// `take_item_entity`: анимация подбора (предмет летит к игроку).
pub fn take_item_entity(id: i32, item_entity: i32, collector: i32, amount: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(item_entity)
        .var_int(collector)
        .var_int(amount);
    w.buf
}

pub const ENTITY_EVENT_DEATH: u8 = 3;
/// Сломался предмет в основной руке (`EquipmentSlot.MAINHAND`).
pub const ENTITY_EVENT_BREAK_MAINHAND: u8 = 47;
/// Сломался предмет во второй руке.
pub const ENTITY_EVENT_BREAK_OFFHAND: u8 = 48;
/// Уровень прав игрока для клиента: 24 + уровень (0..=4).
pub const ENTITY_EVENT_PERMISSION_LEVEL_0: u8 = 24;
/// Использование предмета закончено (еда съедена).
pub const ENTITY_EVENT_USE_ITEM_COMPLETE: u8 = 9;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleeping_metadata_has_pose_and_optional_bed_position() {
        let bed = [12, 64, -3];
        let packet = set_entity_sleeping(42, 7, Some(bed));
        let mut r = Reader::new(&packet);
        assert_eq!(r.var_int().unwrap(), 42);
        assert_eq!(r.var_int().unwrap(), 7);
        assert_eq!(r.u8().unwrap(), 6);
        assert_eq!(r.var_int().unwrap(), 20);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.u8().unwrap(), 14);
        assert_eq!(r.var_int().unwrap(), 11);
        assert!(r.bool().unwrap());
        assert_eq!(r.i64().unwrap(), crate::play::pack_block_pos(bed));
        assert_eq!(r.u8().unwrap(), 0xff);
        assert!(r.remaining().is_empty());

        let packet = set_entity_sleeping(42, 7, None);
        let mut r = Reader::new(&packet);
        assert_eq!(r.var_int().unwrap(), 42);
        assert_eq!(r.var_int().unwrap(), 7);
        assert_eq!(r.u8().unwrap(), 6);
        assert_eq!(r.var_int().unwrap(), 20);
        assert_eq!(r.var_int().unwrap(), 0);
        assert_eq!(r.u8().unwrap(), 14);
        assert_eq!(r.var_int().unwrap(), 11);
        assert!(!r.bool().unwrap());
        assert_eq!(r.u8().unwrap(), 0xff);
        assert!(r.remaining().is_empty());
    }

    #[test]
    fn stack_roundtrip() {
        let mut w = Writer::new();
        write_stack(
            &mut w,
            Some(Stack {
                item: 300,
                count: 64,
                damage: 0,
                extra: 0,
            }),
        );
        write_stack(&mut w, None);
        let mut r = Reader::new(&w.buf);
        assert_eq!(read_stack(&mut r).unwrap(), Some(Stack::new(300, 64)));
        assert_eq!(read_stack(&mut r).unwrap(), None);
        assert!(r.remaining().is_empty());
    }

    /// Вложенность глубже предела — ошибка разбора, а не рекурсия до переполнения стека.
    #[test]
    fn nested_stack_depth_is_capped() {
        let mut w = Writer::new();
        write_stack(&mut w, Some(Stack::new(300, 1)));
        assert!(read_stack_at(&mut Reader::new(&w.buf), MAX_STACK_DEPTH).is_ok());
        assert!(read_stack_at(&mut Reader::new(&w.buf), MAX_STACK_DEPTH + 1).is_err());
    }

    #[test]
    fn unknown_component_patch_roundtrips() {
        let mut w = Writer::new();
        w.var_int(1)
            .var_int(10)
            .var_int(1)
            .var_int(0)
            .var_int(42)
            .var_int(7);
        let original = w.buf.clone();
        let stack = read_stack(&mut Reader::new(&original)).unwrap();
        let mut out = Writer::new();
        write_stack(&mut out, stack);
        assert_eq!(out.buf, original);
    }

    #[test]
    fn repair_cost_component_roundtrips() {
        use std::sync::atomic::Ordering;
        crate::components::REPAIR_COST_COMPONENT.store(123, Ordering::Relaxed);
        let stack = Stack {
            extra: crate::components::intern(crate::components::Extra {
                repair_cost: 7,
                ..Default::default()
            }),
            ..Stack::new(10, 1)
        };
        let mut w = Writer::new();
        write_stack(&mut w, Some(stack));
        assert_eq!(read_stack(&mut Reader::new(&w.buf)).unwrap(), Some(stack));
    }

    #[test]
    fn trim_component_roundtrips() {
        use std::sync::atomic::Ordering;
        crate::components::TRIM_COMPONENT.store(124, Ordering::Relaxed);
        let stack = Stack {
            extra: crate::components::intern(crate::components::Extra {
                trim: Some((2, 5)),
                ..Default::default()
            }),
            ..Stack::new(10, 1)
        };
        let mut w = Writer::new();
        write_stack(&mut w, Some(stack));
        assert_eq!(read_stack(&mut Reader::new(&w.buf)).unwrap(), Some(stack));
    }

    #[test]
    fn banner_patterns_component_roundtrips() {
        use std::sync::atomic::Ordering;
        crate::components::BANNER_PATTERNS_COMPONENT.store(125, Ordering::Relaxed);
        let stack = Stack {
            extra: crate::components::intern(crate::components::Extra {
                banner_patterns: vec![(2, 14), (5, 0)],
                ..Default::default()
            }),
            ..Stack::new(10, 1)
        };
        let mut w = Writer::new();
        write_stack(&mut w, Some(stack));
        assert_eq!(read_stack(&mut Reader::new(&w.buf)).unwrap(), Some(stack));
    }

    #[test]
    fn book_page_counts_decode_from_component_payloads() {
        use std::sync::atomic::Ordering;
        crate::components::WRITABLE_BOOK_CONTENT_COMPONENT.store(126, Ordering::Relaxed);
        crate::components::WRITTEN_BOOK_CONTENT_COMPONENT.store(127, Ordering::Relaxed);
        let mut writable = Writer::new();
        writable.var_int(3);
        assert_eq!(super::book_page_count(126, &writable.buf), Some(3));
        let mut written = Writer::new();
        written
            .string("Title")
            .bool(false)
            .string("Author")
            .var_int(0)
            .var_int(5);
        assert_eq!(super::book_page_count(127, &written.buf), Some(5));
    }

    #[test]
    fn written_book_component_roundtrips() {
        use std::sync::atomic::Ordering;
        crate::components::WRITABLE_BOOK_CONTENT_COMPONENT.store(126, Ordering::Relaxed);
        crate::components::WRITTEN_BOOK_CONTENT_COMPONENT.store(127, Ordering::Relaxed);
        for written in [false, true] {
            let page = |t: &str| {
                if written {
                    format!("{{\"text\":\"{t}\"}}")
                } else {
                    t.to_owned()
                }
            };
            let book = crate::components::Book {
                written,
                pages: vec![(page("one"), None), (page("two"), Some(page("t*o")))],
                title: if written {
                    ("T".into(), None)
                } else {
                    Default::default()
                },
                author: if written {
                    "Steve".into()
                } else {
                    String::new()
                },
                generation: i32::from(written),
                resolved: written,
            };
            let stack = Stack {
                extra: crate::components::intern(crate::components::Extra {
                    book_pages: Some(2),
                    book: Some(book),
                    ..Default::default()
                }),
                ..Stack::new(10, 1)
            };
            let mut w = Writer::new();
            write_stack(&mut w, Some(stack));
            assert_eq!(read_stack(&mut Reader::new(&w.buf)).unwrap(), Some(stack));
        }
    }

    #[test]
    fn trim_overlays_unknown_component_without_losing_it() {
        use std::sync::atomic::Ordering;
        crate::components::TRIM_COMPONENT.store(124, Ordering::Relaxed);
        let original = vec![1, 0, 42, 7];
        let stack = Stack {
            extra: crate::components::intern(crate::components::Extra {
                trim: Some((2, 5)),
                raw: Some(original),
                ..Default::default()
            }),
            ..Stack::new(10, 1)
        };
        let mut w = Writer::new();
        write_stack(&mut w, Some(stack));
        let mut r = Reader::new(&w.buf);
        assert_eq!(r.var_int().unwrap(), 1);
        assert_eq!(r.var_int().unwrap(), 10);
        assert_eq!(r.var_int().unwrap(), 2);
        assert_eq!(r.var_int().unwrap(), 0);
        assert_eq!(r.var_int().unwrap(), 124);
        assert_eq!((r.var_int().unwrap(), r.var_int().unwrap()), (3, 6));
        assert_eq!(r.remaining(), &[42, 7]);
    }

    #[test]
    fn creative_trim_keeps_unknown_patch_once() {
        use std::sync::atomic::Ordering;
        crate::components::TRIM_COMPONENT.store(124, Ordering::Relaxed);
        let mut packet = Writer::new();
        packet
            .u16(36)
            .var_int(1)
            .var_int(10)
            .var_int(2)
            .var_int(0)
            .var_int(42)
            .var_int(1)
            .u8(7)
            .var_int(124)
            .var_int(2)
            .var_int(3)
            .var_int(6);
        let (_, stack) = read_creative_slot(&mut Reader::new(&packet.buf)).unwrap();
        assert_eq!(
            crate::components::get(stack.unwrap().extra).trim,
            Some((2, 5))
        );
        let mut out = Writer::new();
        write_stack(&mut out, stack);
        assert_eq!(out.buf, vec![1, 10, 2, 0, 124, 3, 6, 42, 7]);
    }

    #[test]
    fn creative_slot_drops_component_length_prefix() {
        let mut w = Writer::new();
        // Слот 36, зелье ×1, один компонент типа 53 длиной 3 байта.
        w.u16(36)
            .var_int(1)
            .var_int(10)
            .var_int(1)
            .var_int(0)
            .var_int(53)
            .var_int(3);
        w.buf.extend_from_slice(&[1, 2, 0]);
        let (slot, stack) = read_creative_slot(&mut Reader::new(&w.buf)).unwrap();
        assert_eq!(slot, 36);
        let mut out = Writer::new();
        write_stack(&mut out, stack);
        let mut expect = Writer::new();
        expect
            .var_int(1)
            .var_int(10)
            .var_int(1)
            .var_int(0)
            .var_int(53);
        expect.buf.extend_from_slice(&[1, 2, 0]);
        assert_eq!(out.buf, expect.buf);
    }

    #[test]
    fn click_with_hashed_components() {
        let mut w = Writer::new();
        w.var_int(0).var_int(5).u16(36).u8(0).var_int(0).var_int(2);
        // Слот 36: пусто; слот 9: камень ×3 с одним хэшем компонента.
        w.u16(36).bool(false);
        w.u16(9)
            .bool(true)
            .var_int(1)
            .var_int(3)
            .var_int(1)
            .var_int(7)
            .i32(-1)
            .var_int(0);
        // На курсоре: земля ×1.
        w.bool(true).var_int(28).var_int(1).var_int(0).var_int(0);
        let c = read_container_click(&mut Reader::new(&w.buf)).unwrap();
        assert_eq!(c.state, 5);
        assert_eq!(
            c.changed,
            vec![
                (36, None),
                (
                    9,
                    Some(Stack {
                        item: 1,
                        count: 3,
                        damage: 0,
                        extra: 0
                    })
                )
            ]
        );
        assert_eq!(
            c.carried,
            Some(Stack {
                item: 28,
                count: 1,
                damage: 0,
                extra: 0
            })
        );
    }

    #[test]
    fn lp_vec3_matches_vanilla() {
        let enc = |v| {
            let mut w = Writer::new();
            crate::play::lp_vec3(&mut w, v);
            w.buf
        };
        assert_eq!(enc([0.0; 3]), [0]);
        // Масштаб 1, оси (0.5, 0, -0.5): 24575, 16383, 8192 (round((c*0.5+0.5)*32766)).
        let p: i64 = 1 | 24575 << 3 | 16383 << 18 | 8192 << 33;
        let mut want = vec![p as u8, (p >> 8) as u8];
        want.extend(((p >> 16) as u32).to_be_bytes());
        assert_eq!(enc([0.5, 0.0, -0.5]), want);
        // Масштаб 5 не влезает в 2 бита: продолжение varint(1).
        assert_eq!(enc([5.0, 0.0, 0.0])[0] & 7, 4 | 1);
        assert_eq!(*enc([5.0, 0.0, 0.0]).last().unwrap(), 1);
    }
}
