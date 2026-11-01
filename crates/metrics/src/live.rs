//! Живое состояние метрик: последний снимок, история и оповещение подписчиков.
//!
//! Сэмплер сервера раз в секунду кладёт сюда готовый [`Snapshot`]. Один и тот
//! же объект читают все потребители: `/metrics` для Prometheus, `/snapshot`
//! для `chunkwake top`, `/api/history` для графиков и SSE `/api/events` для
//! страницы в браузере. Отдельный буфер истории нужен потому, что снимок —
//! это «сейчас»: по нему нельзя нарисовать график за последние минуты, а
//! каждому браузеру держать свою историю — значит терять её при перезагрузке.

use crate::snapshot::Snapshot;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

/// Сколько секунд истории помним: 15 минут при опросе раз в секунду.
pub const HISTORY_SECONDS: usize = 900;

/// Точка истории: только то, что рисуется на графиках.
///
/// Полный [`Snapshot`] сюда не годится: в нём вектор ядер и поля вроде `motd`,
/// которые в 900 копиях заняли бы память впустую.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct HistoryPoint {
    /// Момент замера, секунды UTC.
    pub at: u64,
    pub tps: f32,
    pub mspt: f32,
    pub mspt_p95: f32,
    /// Загрузка машины, %.
    pub cpu_system: f32,
    /// Загрузка процесса, % одного ядра (может быть больше 100).
    pub cpu_process: f32,
    /// RSS процесса, МиБ.
    pub rss_mib: f32,
    /// Занятая память машины, ГиБ.
    pub host_used_gib: f32,
    pub players: u32,
    pub mobs: u32,
    /// Чанки в симуляции.
    pub chunks: u32,
    /// Байты и пакеты, накопленные с запуска: скорость считает клиент.
    pub bytes_in: u64,
    pub bytes_out: u64,
    /// Средний пинг, мс; `0` — игроков нет.
    pub ping: f32,
}

impl HistoryPoint {
    /// Точка из снимка: `None` — снимок ещё не публиковался.
    pub fn from_snapshot(snapshot: &Snapshot) -> Self {
        Self {
            at: snapshot.at.parse().unwrap_or(0),
            tps: snapshot.tick.tps_now as f32,
            mspt: snapshot.tick.mspt_now as f32,
            mspt_p95: snapshot.tick.mspt_p95 as f32,
            cpu_system: snapshot.proc.cpu_total,
            cpu_process: snapshot.proc.process_cpu,
            rss_mib: snapshot.rss_mib() as f32,
            host_used_gib: snapshot.memory_used_gib() as f32,
            players: snapshot.world.players as u32,
            mobs: snapshot.world.mobs as u32,
            chunks: snapshot.world.simulated_chunks as u32,
            bytes_in: snapshot.net.bytes_in,
            bytes_out: snapshot.net.bytes_out,
            ping: snapshot.net.ping_avg.unwrap_or(0.0),
        }
    }
}

/// Последний снимок, история и канал «появились новые данные».
///
/// Канал передаёт не данные, а сигнал: подписчики читают историю и снимок из
/// самого [`Live`], поэтому медленный браузер не копит очереди снимков, а
/// просто пропускает кадры (`Lagged`).
#[derive(Debug)]
pub struct Live {
    snapshot: Snapshot,
    published: bool,
    history: VecDeque<HistoryPoint>,
    profile: crate::profile::TickProfile,
    tx: broadcast::Sender<()>,
}

/// Состояние, общее для сэмплера и HTTP-сервера.
pub type Shared = Arc<RwLock<Live>>;

impl Default for Live {
    fn default() -> Self {
        Self::new()
    }
}

impl Live {
    pub fn new() -> Self {
        // Глубина небольшая: подписчик, который не успевает, должен получать
        // `Lagged`, а не отставать на минуты.
        let (tx, _) = broadcast::channel(8);
        Self {
            snapshot: Snapshot::default(),
            published: false,
            history: VecDeque::with_capacity(HISTORY_SECONDS),
            profile: crate::profile::TickProfile::new(),
            tx,
        }
    }

    /// Последний снимок; до первой публикации — значения по умолчанию.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.clone()
    }

    /// Была ли хоть одна публикация: `chunkwake top` и дашборд до старта
    /// сэмплера показывают нули и не должны выдавать их за живые данные.
    pub fn published(&self) -> bool {
        self.published
    }

    /// История от старых точек к новым.
    pub fn history(&self) -> Vec<HistoryPoint> {
        self.history.iter().copied().collect()
    }

    /// Профиль по фазам тика: закрытые секунды от старых к новым.
    pub fn profile(&self) -> Vec<crate::profile::ProfileBucket> {
        self.profile.history()
    }

    /// Последняя закрытая секунда профиля — её отдаёт поток SSE.
    pub fn latest_profile(&self) -> Option<crate::profile::ProfileBucket> {
        self.profile.latest()
    }

    /// Профиль приходит отдельно от снимка: его снимает тик, а не сэмплер,
    /// и в секунду попадает разное число тиков.
    pub fn publish_profile(&mut self, at: u64, marks: &[f32; crate::profile::PHASE_COUNT]) {
        self.profile.record(at, marks);
    }

    /// Кладёт новый снимок: добавляет точку истории и будит подписчиков.
    pub fn publish(&mut self, snapshot: Snapshot) {
        if self.history.len() == HISTORY_SECONDS {
            self.history.pop_front();
        }
        self.history
            .push_back(HistoryPoint::from_snapshot(&snapshot));
        self.snapshot = snapshot;
        self.published = true;
        // `send` без получателей — не ошибка: дашборд просто никто не открыл.
        let _ = self.tx.send(());
    }

    /// Подписка на обновления; вызывать до чтения снимка, чтобы не потерять
    /// публикацию между чтением и ожиданием.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_keeps_fifteen_minutes() {
        let mut live = Live::new();
        for at in 0..(HISTORY_SECONDS as u64 + 50) {
            live.publish(Snapshot {
                at: at.to_string(),
                ..Snapshot::default()
            });
        }
        let history = live.history();
        assert_eq!(history.len(), HISTORY_SECONDS);
        assert_eq!(history[0].at, 50, "старые точки вытеснены");
        assert_eq!(history.last().unwrap().at, HISTORY_SECONDS as u64 + 49);
    }

    #[test]
    fn point_copies_graph_fields() {
        let mut snapshot = Snapshot {
            at: "1700000000".into(),
            ..Snapshot::default()
        };
        snapshot.tick.tps_now = 19.5;
        snapshot.tick.mspt_now = 1.5;
        snapshot.tick.mspt_p95 = 4.0;
        snapshot.proc.cpu_total = 12.5;
        snapshot.proc.process_cpu = 80.0;
        snapshot.proc.rss = 256 * 1024 * 1024;
        snapshot.proc.used_memory = 4 * 1024 * 1024 * 1024;
        snapshot.world.players = 3;
        snapshot.world.mobs = 42;
        snapshot.world.simulated_chunks = 441;
        snapshot.net.bytes_in = 1000;
        snapshot.net.bytes_out = 2000;
        snapshot.net.ping_avg = Some(21.5);
        let point = HistoryPoint::from_snapshot(&snapshot);
        assert_eq!(point.at, 1_700_000_000);
        assert!((point.tps - 19.5).abs() < 1e-6);
        assert!((point.mspt_p95 - 4.0).abs() < 1e-6);
        assert!((point.rss_mib - 256.0).abs() < 1e-3);
        assert!((point.host_used_gib - 4.0).abs() < 1e-5);
        assert_eq!(point.players, 3);
        assert_eq!(point.chunks, 441);
        assert_eq!(point.bytes_out, 2000);
        assert!((point.ping - 21.5).abs() < 1e-6);
    }

    /// Игроков нет — пинг не «нулевая задержка», а отсутствие данных.
    #[test]
    fn missing_ping_is_zero() {
        let point = HistoryPoint::from_snapshot(&Snapshot::default());
        assert_eq!(point.ping, 0.0);
    }

    #[test]
    fn publish_notifies_subscribers() {
        let mut live = Live::new();
        let mut rx = live.subscribe();
        assert!(!live.published());
        live.publish(Snapshot::default());
        assert!(live.published());
        assert!(rx.try_recv().is_ok());
    }

    /// Профиль живёт рядом с историей, но публикуется независимо: тик
    /// присылает его сам, секундными корзинами.
    #[test]
    fn profile_is_collected_separately() {
        let mut live = Live::new();
        assert!(live.profile().is_empty());
        let marks = [0.25_f32; crate::profile::PHASE_COUNT];
        for _ in 0..5 {
            live.publish_profile(10, &marks);
        }
        live.publish_profile(11, &marks);
        let buckets = live.profile();
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].at, 10);
        assert_eq!(buckets[0].ticks, 5);
        // Снимок при этом остаётся нетронутым: профиль не часть Snapshot.
        assert!(!live.published());
    }
}
