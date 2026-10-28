//! Очередь отложенных тиков блоков (`LevelTicks`): по позиции — один срок,
//! наступившие вынимаются по порядку сроков без прохода по всей очереди.

use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub struct TickQueue {
    /// Срок тика по позиции — истина; в `by_time` могут остаться устаревшие записи.
    at: HashMap<[i32; 3], u64>,
    by_time: BTreeMap<u64, Vec<[i32; 3]>>,
}

impl TickQueue {
    pub fn is_empty(&self) -> bool {
        self.at.is_empty()
    }

    pub fn contains(&self, pos: [i32; 3]) -> bool {
        self.at.contains_key(&pos)
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
}
