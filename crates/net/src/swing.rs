//! Взмах руки 26.3: `LivingEntity.SwingState` и `getModifiedSwingDuration`.
//!
//! Клиент шлёт `punch` на каждый взмах атаки (удар по воздуху, по мобу,
//! ломка блока). Сервер (`ServerGamePacketListenerImpl.handlePunch`) берёт
//! `attack_animation` предмета в основной руке и, если взмах можно начать,
//! рассылает `swing_animation` всем, кто следит за игроком, кроме него самого.

/// Состояние взмаха: длительность текущего (с учётом эффектов) и прошедшие тики.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SwingState {
    current: Option<i32>,
    ticks: i32,
}

impl SwingState {
    /// `SwingState.startIfAble`: новый взмах не начинается, пока текущий не
    /// прошёл половину (`ticks <= duration / 2`, кроме самого первого тика
    /// `ticks == 0`, когда можно перезапустить).
    pub fn start_if_able(&mut self, duration: i32) -> bool {
        if let Some(cur) = self.current
            && self.ticks <= cur / 2
            && self.ticks > 0
        {
            return false;
        }
        self.current = Some(duration);
        self.ticks = 0;
        true
    }

    /// `SwingState.tick`: `ticks++`, и взмах заканчивается, когда прежнее
    /// значение уже больше длительности.
    pub fn tick(&mut self) {
        let Some(cur) = self.current else { return };
        let before = self.ticks;
        self.ticks += 1;
        if before > cur {
            self.current = None;
        }
    }

    /// Догоняет `n` пропущенных тиков (сессия игрока не идёт в такт тику мира).
    pub fn advance(&mut self, n: u64) {
        for _ in 0..n.min(64) {
            if self.current.is_none() {
                break;
            }
            self.tick();
        }
    }
}

/// `LivingEntity.getModifiedSwingDuration`: Спешка (или Сила источника)
/// укорачивает на `1 + amp`, иначе Утомление удлиняет на `(1 + amp) * 2`.
pub fn modified_duration(base: i32, haste: Option<u8>, fatigue: Option<u8>) -> i32 {
    if let Some(a) = haste {
        base - (1 + i32::from(a))
    } else if let Some(a) = fatigue {
        base + (1 + i32::from(a)) * 2
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swing_restarts_only_after_half() {
        let mut s = SwingState::default();
        assert!(s.start_if_able(6));
        // Тот же тик: ticks == 0 — можно перезапустить.
        assert!(s.start_if_able(6));
        s.tick();
        s.tick();
        s.tick();
        // ticks = 3 <= 6/2: рано.
        assert!(!s.start_if_able(6));
        s.tick();
        // ticks = 4 > 3: можно.
        assert!(s.start_if_able(6));
    }

    #[test]
    fn swing_ends_after_duration() {
        let mut s = SwingState::default();
        assert!(s.start_if_able(2));
        s.advance(3);
        assert_eq!(
            s,
            SwingState {
                current: Some(2),
                ticks: 3
            }
        );
        s.advance(1);
        assert_eq!(
            s,
            SwingState {
                current: None,
                ticks: 4
            }
        );
        assert!(s.start_if_able(2));
    }

    #[test]
    fn effects_change_duration_like_vanilla() {
        assert_eq!(modified_duration(6, None, None), 6);
        assert_eq!(modified_duration(6, Some(1), None), 4);
        assert_eq!(modified_duration(6, None, Some(0)), 8);
        // Спешка важнее Утомления.
        assert_eq!(modified_duration(6, Some(0), Some(2)), 5);
        assert_eq!(modified_duration(13, None, Some(2)), 19);
    }
}
