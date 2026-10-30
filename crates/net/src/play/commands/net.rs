//! Счётчики сети: байты и пакеты в обе стороны.
//!
//! Считаются атомиками без замков: горячий путь сессии не должен ждать
//! мьютекс ради метрики. Снимок читает их раз в секунду.

use std::sync::atomic::{AtomicU64, Ordering};

/// Накопительные счётчики play-соединений.
#[derive(Debug, Default)]
pub struct NetStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub packets_in: AtomicU64,
    pub packets_out: AtomicU64,
}

impl NetStats {
    pub fn add_in(&self, bytes: u64) {
        self.bytes_in.fetch_add(bytes, Ordering::Relaxed);
        self.packets_in.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_out(&self, bytes: u64, packets: u64) {
        self.bytes_out.fetch_add(bytes, Ordering::Relaxed);
        self.packets_out.fetch_add(packets, Ordering::Relaxed);
    }

    /// Снимок для [`chunkwake_metrics::NetCounters`].
    pub fn snapshot(
        &self,
        pings: &std::collections::HashMap<uuid::Uuid, f32>,
    ) -> chunkwake_metrics::NetCounters {
        let ping_avg = if pings.is_empty() {
            None
        } else {
            Some(pings.values().sum::<f32>() / pings.len() as f32)
        };
        let ping_max = pings.values().copied().fold(None, |acc: Option<f32>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        });
        chunkwake_metrics::NetCounters {
            bytes_in: self.bytes_in.load(Ordering::Relaxed),
            bytes_out: self.bytes_out.load(Ordering::Relaxed),
            packets_in: self.packets_in.load(Ordering::Relaxed),
            packets_out: self.packets_out.load(Ordering::Relaxed),
            ping_avg,
            ping_max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate() {
        let stats = NetStats::default();
        stats.add_in(100);
        stats.add_in(50);
        stats.add_out(400, 3);
        let snapshot = stats.snapshot(&std::collections::HashMap::new());
        assert_eq!(snapshot.bytes_in, 150);
        assert_eq!(snapshot.packets_in, 2);
        assert_eq!(snapshot.bytes_out, 400);
        assert_eq!(snapshot.packets_out, 3);
        assert_eq!(snapshot.ping_avg, None);
    }

    #[test]
    fn ping_average_and_max() {
        let stats = NetStats::default();
        let mut pings = std::collections::HashMap::new();
        pings.insert(uuid::Uuid::from_u128(1), 10.0);
        pings.insert(uuid::Uuid::from_u128(2), 30.0);
        let snapshot = stats.snapshot(&pings);
        assert_eq!(snapshot.ping_avg, Some(20.0));
        assert_eq!(snapshot.ping_max, Some(30.0));
    }
}
