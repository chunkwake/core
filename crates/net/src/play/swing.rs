//! Взмах руки игрока (`ServerGamePacketListenerImpl.handlePunch`).
//!
//! Клиент шлёт пустой `punch` на каждый взмах атаки. Сервер берёт
//! `attack_animation` предмета в основной руке, проверяет `SwingState` и
//! рассылает `swing_animation` остальным игрокам; самому игроку не шлёт —
//! он уже махнул рукой у себя (`sendToTrackingPlayers`, не `AndSelf`).
//!
//! Успешный `use_item` (`Item.use`) тоже взмахивает рукой и через
//! `swing_use` сбрасывает заряд атаки (`swingAndResetAttackStrength`).

use super::*;
use crate::swing::{SwingState, modified_duration};

/// Длина тика сервера: сессия идёт не в такт тику мира, тики взмаха считаются по часам.
const TICK: Duration = Duration::from_millis(50);

impl PlayPlan {
    fn start_swing(
        &self,
        entity_id: i32,
        uuid: Uuid,
        held: Slot,
        vitals: &Vitals,
        swing: &mut (SwingState, Instant),
    ) -> bool {
        let (state, clock) = swing;
        let ticks = (clock.elapsed().as_millis() / TICK.as_millis()) as u64;
        state.advance(ticks);
        *clock += TICK * ticks as u32;

        let (kind, base) = held
            .and_then(|s| self.survival.items.get(s.item as usize))
            .map_or(chunkwake_data::world::DEFAULT_SWING, |i| i.swing);
        let amp = |name: &str| {
            self.survival
                .effects
                .get(name)
                .and_then(|&id| vitals.effect(id))
                .map(|e| e.amplifier)
        };
        let haste = amp("minecraft:haste").max(amp("minecraft:conduit_power"));
        let duration = modified_duration(i32::from(base), haste, amp("minecraft:mining_fatigue"));
        if !state.start_if_able(duration) {
            return false;
        }
        // Пакет несёт `SwingAnimation` предмета как есть: эффекты клиент учтёт сам.
        self.hub.send(
            Some(uuid),
            play::swing_animation(
                self.ids.swing_animation,
                entity_id,
                0,
                i32::from(kind),
                i32::from(base),
            ),
        );
        true
    }

    pub(super) fn punch(
        &self,
        entity_id: i32,
        uuid: Uuid,
        held: Slot,
        vitals: &Vitals,
        swing: &mut (SwingState, Instant),
    ) {
        self.start_swing(entity_id, uuid, held, vitals, swing);
    }

    /// `ServerPlayer.swingAndResetAttackStrength` от успешного `use_item`:
    /// взмах уходит зрителям, заряд сбрасывается только если взмах начался.
    pub(super) fn swing_use(
        &self,
        entity_id: i32,
        uuid: Uuid,
        held: Slot,
        vitals: &Vitals,
        swing: &mut (SwingState, Instant),
    ) -> bool {
        self.start_swing(entity_id, uuid, held, vitals, swing)
    }
}
