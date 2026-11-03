//! Снимок нагрузки мира: сборка для `/top` и `chunkwake top`, рендер в чат.
//!
//! Числа берутся из тех же мест, что и тик: замеры [`chunkwake_metrics::TickStats`],
//! счётчики сети и очередей. Поэтому `/top` показывает ровно то, что влияет на
//! производительность, а не отдельную «декоративную» статистику.

use crate::play::PlayPlan;
use chunkwake_metrics::render::Section;
use chunkwake_metrics::{Snapshot, TickReport, WorldCounters};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Рендер снимка для чата; неизвестная секция — сообщение об ошибке.
pub fn render(snapshot: &Snapshot, token: Option<&str>) -> Value {
    let Some(section) = super::perf::section(token) else {
        return super::dispatch::error_component("Usage: /top [cpu|tick|mem|world|net]");
    };
    chunkwake_metrics::render::chat::render(snapshot, section)
}

/// Снимок мира как компонент чата (для `/perf start`).
pub fn render_snapshot(plan: &PlayPlan) -> Value {
    chunkwake_metrics::render::chat::render(&plan.load_snapshot(), Section::All)
}

impl PlayPlan {
    /// Снимок нагрузки: тик, процесс, мир, сеть.
    ///
    /// Загрузка процесса приходит из сэмплера сервера (`set_proc_stats`):
    /// `sysinfo` считает проценты между двумя опросами, поэтому опрашивать
    /// его из команды нельзя — первый вызов всегда дал бы нули.
    pub fn load_snapshot(&self) -> Snapshot {
        let tick = {
            let ticks = self.ticks.lock().unwrap();
            let last_minute = ticks.window(chunkwake_metrics::tick::PERCENTILE_TICKS);
            TickReport {
                tps_now: last_minute.map_or(ticks.rate(), |w| w.tps),
                tps_1m: ticks.tps_over(chunkwake_metrics::tick::PERCENTILE_TICKS),
                tps_5m: ticks.tps_over(chunkwake_metrics::tick::PERCENTILE_TICKS * 5),
                tps_15m: ticks.tps_over(chunkwake_metrics::tick::PERCENTILE_TICKS * 15),
                mspt_now: ticks.last_work(),
                mspt_1m: last_minute.map_or(0.0, |w| w.mspt_avg),
                mspt_p50: last_minute.map_or(0.0, |w| w.mspt_p50),
                mspt_p95: last_minute.map_or(0.0, |w| w.mspt_p95),
                mspt_p99: last_minute.map_or(0.0, |w| w.mspt_p99),
                mspt_worst: ticks.worst_work(),
                rate: ticks.rate(),
                frozen: ticks.frozen(),
                sprint: ticks.sprint_left() > 0,
                total: ticks.total(),
                late: ticks.late(),
            }
        };
        let pings = self.pings.lock().unwrap();
        let net = self.net_stats.snapshot(&pings);
        let world = WorldCounters {
            players: self.hub.players.lock().unwrap().len(),
            max_players: self.login.max_players.max(0) as usize,
            mobs: self.sim().mobs.lock().unwrap().len(),
            dropped_items: self.sim().dropped.lock().unwrap().len(),
            arrows: self.sim().arrows.lock().unwrap().len(),
            orbs: self.sim().orbs.lock().unwrap().len(),
            simulated_chunks: self.simulated_chunks(),
            loaded_chunks: self.loaded_columns(),
            redstone_queue: self.sim().redstone_ticks.lock().unwrap().len(),
            fluid_queue: self.sim().fluid_ticks.lock().unwrap().len(),
            hopper_queue: self.sim().hopper_ticks.lock().unwrap().len(),
            furnaces: self.sim().furnaces.lock().unwrap().len(),
            campfires: self.sim().campfires.lock().unwrap().len(),
            gen_pending: self.gen_pool.get().map_or(0, |pool| pool.pending()),
            gen_workers: self.gen_pool.get().map_or(0, |pool| pool.workers()),
        };
        Snapshot {
            at: timestamp(),
            uptime: self.started.elapsed().as_secs(),
            tick,
            proc: self.proc_stats.lock().unwrap().clone(),
            world,
            net,
            limits: chunkwake_metrics::Limits {
                target_mspt: 1000.0 / tick.rate.max(1.0),
                view_distance: self.login.view_distance.clamp(0, 255) as u8,
                simulation_distance: self.login.simulation_distance.clamp(0, 255) as u8,
                compression_threshold: self.compression_threshold,
            },
            motd: self.motd.clone(),
            version: self.version_name.clone(),
        }
    }

    /// Строка `/tps` и `/tick query` (формат разбирает `chunkwake-lab`).
    pub fn tick_meter_line(&self) -> String {
        self.ticks.lock().unwrap().line()
    }

    /// `None` — профилировщик уже запущен.
    pub fn start_debug_profile(&self) -> bool {
        let mut slot = self.debug_profile.lock().unwrap();
        if slot.is_some() {
            return false;
        }
        *slot = crate::play::DebugProfile::create(&self.debug_dir);
        let started = slot.is_some();
        drop(slot);
        self.set_profiling(started);
        started
    }

    /// Останавливает профиль и отдаёт путь к файлу.
    pub fn stop_debug_profile(&self) -> Option<std::path::PathBuf> {
        let path = self
            .debug_profile
            .lock()
            .unwrap()
            .take()
            .map(|profile| profile.path);
        if path.is_some() {
            self.set_profiling(false);
        }
        path
    }
}

/// Текущее время в секундах UTC.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    secs.to_string()
}

/// Секция снимка из `/top`.
pub(super) fn section(token: Option<&str>) -> Option<Section> {
    match token {
        None => Some(Section::All),
        Some(token) => Section::parse(token),
    }
}
