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
}

/// Читает `ItemStack` с компонентами: износ, чары, имя и описание.
/// Неизвестный компонент (состав зелья и т. п.) сохраняется сырым патчем целиком.
/// Стопка с таким патчем должна быть последним полем пакета: хвост не разбирается.
pub fn read_stack(r: &mut Reader) -> Result<Slot> {
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
                if let Some(st) = read_stack(r)? {
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
    patch.var_int(added).var_int(removed);
    for _ in 0..added {
        let kind = r.var_int()?;
        let len = r.var_int()?;
        if !(0..=1 << 20).contains(&len) {
            return Err(crate::ProtocolError::BadLength(len));
        }
        let payload = r.bytes(len as usize)?;
        patch.var_int(kind);
        patch.buf.extend_from_slice(payload);
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
                if let Ok(Some(st)) = read_stack(&mut body) {
                    e.charged.push(st);
                }
            }
        }
    }
    for _ in 0..removed {
        patch.var_int(r.var_int()?);
    }
    if added != 0 || removed != 0 {
        e.raw = Some(patch.buf);
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
