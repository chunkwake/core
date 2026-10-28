//! Предметы, инвентарь игрока, здоровье и респаун.

use crate::play::{SpawnInfo, write_spawn_info};
use crate::{Reader, Result, Writer};

/// Стопка предметов: ID в реестре `item`, количество и износ (`minecraft:damage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: i32,
    pub count: i32,
    pub damage: i32,
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
            let damage = DAMAGE_COMPONENT.load(std::sync::atomic::Ordering::Relaxed);
            if s.damage > 0 && damage >= 0 {
                w.var_int(1).var_int(0).var_int(damage).var_int(s.damage);
            } else {
                w.var_int(0).var_int(0);
            }
        }
        _ => {
            w.var_int(0);
        }
    }
}

/// Читает `ItemStack` (с компонентами) и возвращает тип и количество.
/// Компоненты пропускаются вместе с остатком пакета, поэтому стопка с
/// компонентами допустима только последней.
pub fn read_stack(r: &mut Reader) -> Result<Slot> {
    let count = r.var_int()?;
    if count <= 0 {
        return Ok(None);
    }
    let item = r.var_int()?;
    // Патч: добавленные (тип + значение), удалённые (типы). Без добавленных
    // пропускаем целиком; значения компонентов разбирать не умеем.
    if r.var_int()? == 0 {
        for _ in 0..r.var_int()? {
            r.var_int()?;
        }
    }
    Ok(Some(Stack { item, count, damage: 0 }))
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
    Ok((count > 0).then_some(Stack { item, count, damage: 0 }))
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
pub fn read_creative_slot(r: &mut Reader) -> Result<(i16, Slot)> {
    let slot = r.u16()? as i16;
    Ok((slot, read_stack(r)?))
}

pub fn set_held_slot(id: i32, slot: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(slot);
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

/// `set_entity_data` предмета-сущности: `ItemEntity.DATA_ITEM` (индекс 8,
/// сериализатор `ITEM_STACK` = 7).
pub fn set_entity_item(id: i32, entity_id: i32, stack: Slot) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id).var_int(entity_id).u8(8).var_int(7);
    write_stack(&mut w, stack);
    w.u8(0xff);
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
            }),
        );
        write_stack(&mut w, None);
        let mut r = Reader::new(&w.buf);
        assert_eq!(
            read_stack(&mut r).unwrap(),
            Some(Stack {
                item: 300,
                count: 64
            ,
                damage: 0})
        );
        assert_eq!(read_stack(&mut r).unwrap(), None);
        assert!(r.remaining().is_empty());
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
            vec![(36, None), (9, Some(Stack { item: 1, count: 3 , damage: 0}))]
        );
        assert_eq!(c.carried, Some(Stack { item: 28, count: 1 , damage: 0}));
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
