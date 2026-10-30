//! Замеры тика: MSPT по окнам, TPS, перцентили и худшие тики.
//!
//! Раньше `TickMeter` держал 100 замеров (5 секунд) и печатал одну строку:
//! по ней нельзя было отличить разовый провал от устойчивой деградации.
//! Здесь кольцевой буфер на 15 минут, поэтому видно 1/5/15 минут как в `htop`.

use std::collections::VecDeque;

    /// Сколько тиков помним: 20 в секунду × 15 минут.
pub const WINDOW_TICKS: usize = 20 * 60 * 15;

/// Окно для перцентилей: минута.
pub const PERCENTILE_TICKS: usize = 20 * 60;

/// Целевая частота тика.
pub const TARGET_TPS: f64 = 20.0;

/// Замер одного тика.
#[derive(Clone, Copy, Debug)]
struct Sample {
    /// Работа самого тика, мс.
    work: f64,
    /// Полный интервал между тиками, мс (включая сон).
    gap: f64,
}

/// Накопитель замеров тика.
#[derive(Debug)]
pub struct TickStats {
    samples: VecDeque<Sample>,
    /// Тиков всего с запуска.
    total: u64,
    /// Момент предыдущего тика: по нему считается интервал.
    last_at: Option<std::time::Instant>,
    /// Тиков, которые не уложились в интервал: TPS ниже целевого.
    late: u64,
    /// Худший тик за всё время.
    worst_work: f64,
    worst_gap: f64,
    /// Последний интервал между тиками, мс.
    last_gap: f64,
    /// Заданная скорость тика (20 в секунду — ванила).
    rate: f64,
    /// Тик остановлен (`/tick freeze`).
    frozen: bool,
    /// Сколько тиков осталось выполнить вручную (`/tick step`).
    steps_left: u64,
    /// `sprint`: ускоренный прогон и сколько тиков осталось.
    sprint_left: u64,
    /// Приблизительное время сна между тиками, мс: 1000 / rate.
    interval: f64,
}

impl Default for TickStats {
    fn default() -> Self {
        Self::new()
    }
}

impl TickStats {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(WINDOW_TICKS),
            total: 0,
            last_at: None,
            late: 0,
            worst_work: 0.0,
            worst_gap: 0.0,
            last_gap: 0.0,
            rate: TARGET_TPS,
            frozen: false,
            steps_left: 0,
            sprint_left: 0,
            interval: 1000.0 / TARGET_TPS,
        }
    }

    /// Записывает тик: время работы и интервал от предыдущего.
    pub fn record(&mut self, work_ms: f64, gap_ms: f64) {
        if self.samples.len() == WINDOW_TICKS {
            self.samples.pop_front();
        }
        let work = work_ms.max(0.0);
        let gap = gap_ms.max(0.0);
        self.samples.push_back(Sample { work, gap });
        self.total += 1;
        self.worst_work = self.worst_work.max(work);
        self.worst_gap = self.worst_gap.max(gap);
        if gap > 0.0 {
            self.last_gap = gap;
        }
        // Опоздание — тик дольше целевого интервала (с запасом 10 % на
        // пробуждение таймера ОС), но не ускоренный прогон.
        if self.sprint_left == 0 && gap > self.interval * 1.10 {
            self.late += 1;
        }
    }

    /// Интервал с прошлого тика, мс: `0` в первый раз.
    ///
    /// Отметка времени живёт здесь, а не у мира: иначе `TickStats` пришлось
    /// бы знать, кто его вызывает, а миру — тащить ещё один `Instant`.
    pub fn last_interval_ms(&mut self, now: std::time::Instant) -> f64 {
        let gap = self
            .last_at
            .map_or(0.0, |prev| now.duration_since(prev).as_secs_f64() * 1000.0);
        self.last_at = Some(now);
        gap
    }

    /// Заданная скорость тика, тиков в секунду.
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Текущий целевой интервал между тиками, мс.
    pub fn interval(&self) -> f64 {
        self.interval
    }

    /// Меняет скорость: 20 — ванила, больше — ускорение (лаборатория).
    pub fn set_rate(&mut self, rate: f64) {
        if rate.is_finite() && rate >= 1.0 {
            self.rate = rate;
            self.interval = 1000.0 / rate;
        }
    }

    pub fn frozen(&self) -> bool {
        self.frozen
    }

    pub fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
        if frozen {
            self.sprint_left = 0;
        }
    }

    /// Просит выполнить `ticks` тиков вручную (на замороженном сервере).
    pub fn request_steps(&mut self, ticks: u64) {
        self.steps_left = self.steps_left.saturating_add(ticks);
        self.frozen = true;
    }

    /// Забирает один шаг, если он запрошен; `true` — тик надо выполнить.
    pub fn take_step(&mut self) -> bool {
        if self.steps_left == 0 {
            return false;
        }
        self.steps_left -= 1;
        true
    }

    pub fn steps_pending(&self) -> u64 {
        self.steps_left
    }

    pub fn start_sprint(&mut self, ticks: u64) {
        self.sprint_left = ticks;
    }

    /// Уменьшает счётчик ускоренного прогона и сообщает, идёт ли он.
    pub fn tick_sprint(&mut self) -> Option<u64> {
        if self.sprint_left == 0 {
            return None;
        }
        self.sprint_left -= 1;
        Some(self.sprint_left)
    }

    pub fn sprint_left(&self) -> u64 {
        self.sprint_left
    }

    pub fn stop_sprint(&mut self) -> Option<u64> {
        if self.sprint_left == 0 {
            return None;
        }
        let ran = self.sprint_left;
        self.sprint_left = 0;
        Some(ran)
    }

    /// Тиков с запуска.
    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn late(&self) -> u64 {
        self.late
    }

    pub fn worst_work(&self) -> f64 {
        self.worst_work
    }

    pub fn worst_gap(&self) -> f64 {
        self.worst_gap
    }

    /// Последний MSPT (работа последнего тика).
    pub fn last_work(&self) -> f64 {
        self.samples.back().map_or(0.0, |s| s.work)
    }

    /// Снимок окна `ticks` назад; `None` — замеров нет.
    pub fn window(&self, ticks: usize) -> Option<WindowStats> {
        let skip = self.samples.len().saturating_sub(ticks);
        let slice: Vec<&Sample> = self.samples.iter().skip(skip).collect();
        if slice.is_empty() {
            return None;
        }
        let mut work: Vec<f64> = slice.iter().map(|s| s.work).collect();
        let gaps: Vec<f64> = slice.iter().map(|s| s.gap).filter(|g| *g > 0.0).collect();
        let avg = |values: &[f64]| {
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            }
        };
        let tps = if gaps.is_empty() {
            self.rate
        } else {
            1000.0 / avg(&gaps)
        };
        work.sort_by(f64::total_cmp);
        Some(WindowStats {
            samples: slice.len(),
            mspt_avg: avg(&work),
            mspt_p50: percentile(&work, 50.0),
            mspt_p95: percentile(&work, 95.0),
            mspt_p99: percentile(&work, 99.0),
            mspt_max: work.last().copied().unwrap_or(0.0),
            tps,
        })
    }

    /// Строка `/tps` и `/tick query`.
    ///
    /// Формат `TPS: … MSPT: … interval: … ms` сохранён: его разбирает
    /// `chunkwake-lab` при замерах.
    pub fn line(&self) -> String {
        let Some(window) = self.window(PERCENTILE_TICKS) else {
            return format!(
                "TPS: {:.1} MSPT: 0.0 interval: {:.1} ms",
                self.rate, self.interval
            );
        };
        let interval = if self.last_gap > 0.0 {
            self.last_gap
        } else {
            self.interval
        };
        format!(
            "TPS: {:.1} MSPT: {:.1} interval: {interval:.1} ms",
            window.tps, window.mspt_avg
        )
    }
}

/// Статистика окна.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowStats {
    pub samples: usize,
    pub mspt_avg: f64,
    pub mspt_p50: f64,
    pub mspt_p95: f64,
    pub mspt_p99: f64,
    pub mspt_max: f64,
    pub tps: f64,
}

/// Перцентиль по отсортированному списку (ближайший ранг, как в spark).
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[index.saturating_sub(1).min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_reports_target() {
        let stats = TickStats::new();
        assert_eq!(stats.line(), "TPS: 20.0 MSPT: 0.0 interval: 50.0 ms");
        assert!(stats.window(20).is_none());
    }

    #[test]
    fn windows_average_and_tps() {
        let mut stats = TickStats::new();
        for _ in 0..1200 {
            stats.record(5.0, 50.0);
        }
        let window = stats.window(PERCENTILE_TICKS).unwrap();
        assert_eq!(window.samples, 1200);
        assert!((window.tps - 20.0).abs() < 1e-9);
        assert!((window.mspt_avg - 5.0).abs() < 1e-9);
        assert_eq!(stats.line(), "TPS: 20.0 MSPT: 5.0 interval: 50.0 ms");
    }

    /// Дрожание таймера ОС не считается опозданием: интервалы чередуются
    /// (47 и 63 мс) при верной средней частоте.
    #[test]
    fn timer_jitter_is_not_late() {
        let mut stats = TickStats::new();
        for i in 0..200 {
            stats.record(0.05, if i % 2 == 0 { 52.0 } else { 55.0 });
        }
        assert_eq!(stats.late(), 0, "запас 10 % покрывает пробуждение таймера");
    }

    /// Устойчивый провал виден: тики идут по 100 мс — это TPS 10.
    #[test]
    fn sustained_slowdown_is_late() {
        let mut stats = TickStats::new();
        for _ in 0..40 {
            stats.record(80.0, 100.0);
        }
        assert_eq!(stats.late(), 40);
        let window = stats.window(40).unwrap();
        assert!((window.tps - 10.0).abs() < 1e-9);
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut stats = TickStats::new();
        for i in 1..=100 {
            stats.record(f64::from(i), 50.0);
        }
        let window = stats.window(100).unwrap();
        assert_eq!(window.mspt_p50, 50.0);
        assert_eq!(window.mspt_p95, 95.0);
        assert_eq!(window.mspt_p99, 99.0);
        assert_eq!(window.mspt_max, 100.0);
    }

    #[test]
    fn ring_buffer_keeps_fifteen_minutes() {
        let mut stats = TickStats::new();
        for i in 0..(WINDOW_TICKS + 500) {
            stats.record(f64::from(i as u32 % 7), 50.0);
        }
        assert_eq!(stats.samples.len(), WINDOW_TICKS);
        assert_eq!(stats.total(), WINDOW_TICKS as u64 + 500);
    }

    #[test]
    fn freeze_and_steps() {
        let mut stats = TickStats::new();
        assert!(!stats.take_step());
        stats.request_steps(3);
        assert!(stats.frozen());
        assert!(stats.take_step());
        assert!(stats.take_step());
        assert!(stats.take_step());
        assert!(!stats.take_step());
    }

    #[test]
    fn sprint_counts_down() {
        let mut stats = TickStats::new();
        stats.start_sprint(2);
        assert_eq!(stats.tick_sprint(), Some(1));
        assert_eq!(stats.tick_sprint(), Some(0));
        assert_eq!(stats.tick_sprint(), None);
        stats.start_sprint(5);
        assert_eq!(stats.stop_sprint(), Some(5));
        assert_eq!(stats.stop_sprint(), None);
    }

    #[test]
    fn faster_rate_still_reports_above_twenty() {
        let mut stats = TickStats::new();
        stats.set_rate(40.0);
        stats.record(1.0, 25.0);
        assert_eq!(stats.line(), "TPS: 40.0 MSPT: 1.0 interval: 25.0 ms");
    }
}
