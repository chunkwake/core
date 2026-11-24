//! Очередь отложенных тиков блоков (`LevelTicks`): по позиции — один срок,
//! наступившие вынимаются по порядку сроков без прохода по всей очереди.

use std::collections::{BTreeMap, HashMap};

/// Отложенные тики одного чанка: позиция и срок.
type Parked = Vec<([i32; 3], u64)>;

#[derive(Default)]
pub struct TickQueue {
    /// Срок тика по позиции — истина; в `by_time` могут остаться устаревшие записи.
    at: HashMap<[i32; 3], u64>,
    by_time: BTreeMap<u64, Vec<[i32; 3]>>,
    /// Наступившие тики чанков, которые сейчас не тикают (`LevelChunkTicks`
    /// выгруженного или нетикающего чанка у ванили): лежат по чанку и не
    /// перебираются каждый тик, а возвращаются целиком [`Self::unpark`].
    parked: HashMap<(i32, i32), Parked>,
}

impl TickQueue {
    pub fn is_empty(&self) -> bool {
        self.at.is_empty()
    }

    /// Сколько позиций ждёт своего срока.
    pub fn len(&self) -> usize {
        self.at.len()
    }

    pub fn contains(&self, pos: [i32; 3]) -> bool {
        self.at.contains_key(&pos)
    }

    /// Все запланированные тики: `(позиция, абсолютный тик)`.
    pub fn iter(&self) -> impl Iterator<Item = ([i32; 3], u64)> + '_ {
        self.at.iter().map(|(&pos, &at)| (pos, at))
    }

    /// Снимает запланированный тик: блок сломали, воронку убрали. Устаревшая
    /// запись в `by_time` безвредна — её отсеет `take_due*` по `at`.
    pub fn remove(&mut self, pos: [i32; 3]) {
        self.at.remove(&pos);
    }

    /// Ставит тик; уже стоящий меняет срок только при `replace`.
    pub fn schedule(&mut self, pos: [i32; 3], at: u64, replace: bool) {
        match self.at.get(&pos) {
            Some(_) if !replace => return,
            Some(&old) if old == at => return,
            _ => {}
        }
        self.at.insert(pos, at);
        self.by_time.entry(at).or_default().push(pos);
    }

    /// Наступившие к `now` тики: по сроку, внутри срока — в порядке постановки.
    pub fn take_due(&mut self, now: u64) -> Vec<[i32; 3]> {
        let mut due = Vec::new();
        while let Some(entry) = self.by_time.first_entry() {
            if *entry.key() > now {
                break;
            }
            let (t, list) = entry.remove_entry();
            for p in list {
                if self.at.get(&p) == Some(&t) {
                    self.at.remove(&p);
                    due.push(p);
                }
            }
        }
        due
    }

    /// То же, но берёт только позиции, прошедшие `include`; остальные
    /// откладываются по своему чанку и ждут [`Self::unpark`]. Нужно, когда
    /// часть мира не тикает: отложенная работа за пределами симуляции не
    /// должна ни пропадать, ни перебираться заново на каждом тике.
    pub fn take_due_in(
        &mut self,
        now: u64,
        mut include: impl FnMut([i32; 3]) -> bool,
    ) -> Vec<[i32; 3]> {
        let mut due = Vec::new();
        while let Some(entry) = self.by_time.first_entry() {
            if *entry.key() > now {
                break;
            }
            let (t, list) = entry.remove_entry();
            for p in list {
                if self.at.get(&p) != Some(&t) {
                    continue; // устаревшая запись
                }
                if include(p) {
                    self.at.remove(&p);
                    due.push(p);
                } else {
                    self.parked
                        .entry((p[0] >> 4, p[2] >> 4))
                        .or_default()
                        .push((p, t));
                }
            }
        }
        due
    }

    /// Возвращает в очередь отложенные тики чанков, прошедших `ticking`: чанк
    /// снова тикает, и его наступившие тики идут в ближайший `take_due*`.
    pub fn unpark(&mut self, mut ticking: impl FnMut((i32, i32)) -> bool) {
        if self.parked.is_empty() {
            return;
        }
        let ready: Vec<(i32, i32)> = self
            .parked
            .keys()
            .copied()
            .filter(|&c| ticking(c))
            .collect();
        for c in ready {
            for (p, t) in self.parked.remove(&c).unwrap_or_default() {
                // Снятый или перенесённый тик здесь уже устарел.
                if self.at.get(&p) == Some(&t) {
                    self.by_time.entry(t).or_default().push(p);
                }
            }
        }
    }

    /// Сколько чанков держат отложенные тики.
    pub fn parked_chunks(&self) -> usize {
        self.parked.len()
    }
}

/// Порядок блочных фаз: регион за регионом (8x8 чанков, как в регионном
/// тике сущностей), внутри региона — исходный порядок (сортировка
/// стабильная). Это снимок расписаний (`redstone_ticks`, `fluid_ticks`,
/// `deferred_updates`) по регионам: фаза обходит мир целыми регионами, а не
/// вперемешку, и на этот порядок дальше садятся регионные воркеры. Прямые
/// обновления соседей через границу региона остаются немедленными, как в
/// ванили; отложенные межрегиональные правки идут обычными очередями.
pub fn region_order(due: Vec<[i32; 3]>) -> Vec<[i32; 3]> {
    let mut out = due;
    out.sort_by_key(|p| ((p[0] >> 4) >> 3, (p[2] >> 4) >> 3));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_dedup_and_replace() {
        let mut q = TickQueue::default();
        q.schedule([0, 0, 0], 5, false);
        q.schedule([1, 0, 0], 3, false);
        q.schedule([0, 0, 0], 1, false); // уже стоит — не меняется
        q.schedule([2, 0, 0], 4, false);
        q.schedule([2, 0, 0], 9, true); // перенос
        assert!(q.take_due(2).is_empty());
        assert_eq!(q.take_due(5), vec![[1, 0, 0], [0, 0, 0]]);
        assert!(q.contains([2, 0, 0]));
        assert_eq!(q.take_due(9), vec![[2, 0, 0]]);
        assert!(q.is_empty());
    }

    #[test]
    fn take_due_in_keeps_outside_ticks_for_later() {
        let mut q = TickQueue::default();
        q.schedule([0, 0, 0], 1, false);
        q.schedule([100, 0, 0], 1, false);
        q.schedule([100, 0, 0], 1, false); // повтор не плодит записей
        q.schedule([0, 0, 0], 2, false);
        // Снаружи симуляции — только чанк (0, 0).
        let inside = |p: [i32; 3]| p[0] >> 4 == 0;
        assert_eq!(q.take_due_in(5, inside), vec![[0, 0, 0]]);
        // Дальняя позиция не пропала: её тик ждёт, пока чанк начнёт тикать.
        assert!(q.contains([100, 0, 0]));
        assert_eq!(q.parked_chunks(), 1);
        assert!(q.take_due_in(5, |_| true).is_empty());
        q.unpark(|c| c == (6, 0));
        assert_eq!(q.parked_chunks(), 0);
        assert_eq!(q.take_due_in(5, |_| true), vec![[100, 0, 0]]);
        assert!(q.is_empty());
    }

    #[test]
    fn parked_tick_removed_or_moved_is_not_revived() {
        let mut q = TickQueue::default();
        q.schedule([100, 0, 0], 1, false);
        q.schedule([101, 0, 0], 1, false);
        assert!(q.take_due_in(5, |_| false).is_empty());
        q.remove([100, 0, 0]);
        q.schedule([101, 0, 0], 8, true);
        q.unpark(|_| true);
        assert!(q.take_due(5).is_empty());
        assert_eq!(q.take_due(8), vec![[101, 0, 0]]);
    }

    #[test]
    fn region_order_groups_regions_but_keeps_queue_order_inside() {
        // Чанк 100 -> регион 0 (6>>3), чанк 0 -> регион 0, чанк 12 -> регион 1.
        let due = vec![[100, 0, 0], [10, 0, 0], [200, 0, 0], [20, 0, 0]];
        let sorted = super::region_order(due);
        // Сначала весь регион 0 (в исходном порядке 100, 10, 20), затем регион 1.
        assert_eq!(
            sorted,
            vec![[100, 0, 0], [10, 0, 0], [20, 0, 0], [200, 0, 0]]
        );
    }
}
