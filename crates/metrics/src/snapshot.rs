//! ?????? ????????? ???????: ???, ???????, ???, ????.
//!
//! ???? ??? ??? ???? ????????????: `/top` ? ????, `chunkwake top` ?
//! ?????????, `/metrics` ? Prometheus ? `/snapshot` ??? ???????????. ???
//! ????? ? ???? ? ? ????????? ?? ??????????.

use serde::{Deserialize, Serialize};

pub use crate::proc::{CoreLoad, ProcStats};

/// ?????? ???? ?? ?????.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct TickReport {
    pub tps_now: f64,
    pub tps_1m: f64,
    pub tps_5m: f64,
    pub tps_15m: f64,
    pub mspt_now: f64,
    pub mspt_1m: f64,
    pub mspt_p50: f64,
    pub mspt_p95: f64,
    pub mspt_p99: f64,
    pub mspt_worst: f64,
    /// ???????? ???????? ???? (`/tick rate`).
    pub rate: f64,
    pub frozen: bool,
    pub sprint: bool,
    pub total: u64,
    pub late: u64,
}

/// ??? ?????? ? ????.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct WorldCounters {
    pub players: usize,
    pub max_players: usize,
    pub mobs: usize,
    pub dropped_items: usize,
    pub arrows: usize,
    pub orbs: usize,
    /// ????? ? ?????? ????????? ?????? ???????.
    pub simulated_chunks: usize,
    /// ???????, ????????? ???? (??????????? ? ???????????????).
    pub loaded_chunks: usize,
    /// ??????? ?????????? ??????: ????????, ????????, ???????, ????, ??????.
    pub redstone_queue: usize,
    pub fluid_queue: usize,
    pub hopper_queue: usize,
    pub furnaces: usize,
    pub campfires: usize,
    /// ????? ????????? ?????? ? ????.
    pub gen_pending: usize,
    /// ??????? ??????? ?????????.
    pub gen_workers: usize,
}

/// ????: ???????? ???? ? ????????.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct NetCounters {
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub packets_in: u64,
    pub packets_out: u64,
    /// ??????? ???? ???????, ??; `None` ? ????? ??? ?? ????.
    pub ping_avg: Option<f32>,
    pub ping_max: Option<f32>,
}

/// ?????????, ??????? ????? ? ??????.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Limits {
    /// ??????? MSPT: 50 ?? ?? ??? ??? 20 TPS.
    pub target_mspt: f64,
    pub view_distance: u8,
    pub simulation_distance: u8,
    pub compression_threshold: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            target_mspt: 50.0,
            view_distance: 8,
            simulation_distance: 6,
            compression_threshold: 256,
        }
    }
}

/// ?????? ?????? ?? ?????? ???????.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// ????? ?????? (UTC, RFC 3339).
    pub at: String,
    /// ??????? ???????? ??????, ???????.
    pub uptime: u64,
    pub tick: TickReport,
    pub proc: ProcStats,
    pub world: WorldCounters,
    pub net: NetCounters,
    pub limits: Limits,
    /// ???????? ??????? ? ?????? ????.
    pub motd: String,
    pub version: String,
}

impl Snapshot {
    /// ???????? ???? ? ????????? ??? ???????.
    pub fn core_bar(core: &CoreLoad) -> f32 {
        core.usage.clamp(0.0, 100.0)
    }

    /// ????????? ?????? ??????? ? ???.
    pub fn memory_used_gib(&self) -> f64 {
        self.proc.used_memory as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn memory_total_gib(&self) -> f64 {
        self.proc.total_memory as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    /// RSS ???????? ? ???.
    pub fn rss_mib(&self) -> f64 {
        self.proc.rss as f64 / (1024.0 * 1024.0)
    }

    /// ?????? ??????? ??? `1d 02:03`.
    pub fn uptime_text(&self) -> String {
        let days = self.uptime / 86_400;
        let hours = (self.uptime % 86_400) / 3_600;
        let minutes = (self.uptime % 3_600) / 60;
        let seconds = self.uptime % 60;
        if days > 0 {
            format!("{days}d {hours:02}:{minutes:02}:{seconds:02}")
        } else {
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        }
    }
}

/// ?????? `# HELP` ? `# TYPE` ??? ???????.
fn header(out: &mut String, name: &str, kind: &str, help: &str) {
    out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} {kind}\n"));
}

/// ????????? ??????? Prometheus ?? ??????.
pub fn prometheus(snapshot: &Snapshot) -> String {
    let mut out = String::with_capacity(4096);
    header(
        &mut out,
        "chunkwake_up",
        "gauge",
        "1 when the server is running",
    );
    out.push_str("chunkwake_up 1\n");

    header(
        &mut out,
        "chunkwake_tps",
        "gauge",
        "Ticks per second over a window",
    );
    out.push_str(&format!(
        "chunkwake_tps{{window=\"1m\"}} {}\n",
        snapshot.tick.tps_1m
    ));
    out.push_str(&format!(
        "chunkwake_tps{{window=\"5m\"}} {}\n",
        snapshot.tick.tps_5m
    ));
    out.push_str(&format!(
        "chunkwake_tps{{window=\"15m\"}} {}\n",
        snapshot.tick.tps_15m
    ));

    header(
        &mut out,
        "chunkwake_tick_rate",
        "gauge",
        "Configured tick rate",
    );
    out.push_str(&format!("chunkwake_tick_rate {}\n", snapshot.tick.rate));
    header(
        &mut out,
        "chunkwake_tick_frozen",
        "gauge",
        "1 when ticking is frozen",
    );
    out.push_str(&format!(
        "chunkwake_tick_frozen {}\n",
        u8::from(snapshot.tick.frozen)
    ));

    header(&mut out, "chunkwake_mspt", "gauge", "Milliseconds per tick");
    out.push_str(&format!(
        "chunkwake_mspt{{quantile=\"0.5\"}} {}\n",
        snapshot.tick.mspt_p50
    ));
    out.push_str(&format!(
        "chunkwake_mspt{{quantile=\"0.95\"}} {}\n",
        snapshot.tick.mspt_p95
    ));
    out.push_str(&format!(
        "chunkwake_mspt{{quantile=\"0.99\"}} {}\n",
        snapshot.tick.mspt_p99
    ));
    out.push_str(&format!(
        "chunkwake_mspt{{quantile=\"max\"}} {}\n",
        snapshot.tick.mspt_worst
    ));
    out.push_str(&format!("chunkwake_mspt_last {}\n", snapshot.tick.mspt_now));

    header(
        &mut out,
        "chunkwake_ticks_total",
        "counter",
        "Ticks since start",
    );
    out.push_str(&format!("chunkwake_ticks_total {}\n", snapshot.tick.total));
    header(
        &mut out,
        "chunkwake_late_ticks_total",
        "counter",
        "Ticks later than target",
    );
    out.push_str(&format!(
        "chunkwake_late_ticks_total {}\n",
        snapshot.tick.late
    ));

    header(
        &mut out,
        "chunkwake_process_cpu_percent",
        "gauge",
        "Process CPU, % of one core",
    );
    out.push_str(&format!(
        "chunkwake_process_cpu_percent {}\n",
        snapshot.proc.process_cpu
    ));
    header(
        &mut out,
        "chunkwake_process_rss_bytes",
        "gauge",
        "Resident memory of the process",
    );
    out.push_str(&format!(
        "chunkwake_process_rss_bytes {}\n",
        snapshot.proc.rss
    ));
    header(
        &mut out,
        "chunkwake_process_threads",
        "gauge",
        "Threads of the process",
    );
    out.push_str(&format!(
        "chunkwake_process_threads {}\n",
        snapshot.proc.threads
    ));
    header(
        &mut out,
        "chunkwake_system_cpu_percent",
        "gauge",
        "Average CPU load of the machine",
    );
    out.push_str(&format!(
        "chunkwake_system_cpu_percent {}\n",
        snapshot.proc.cpu_total
    ));
    header(
        &mut out,
        "chunkwake_system_memory_used_bytes",
        "gauge",
        "Used memory of the machine",
    );
    out.push_str(&format!(
        "chunkwake_system_memory_used_bytes {}\n",
        snapshot.proc.used_memory
    ));
    header(
        &mut out,
        "chunkwake_system_memory_total_bytes",
        "gauge",
        "Total memory of the machine",
    );
    out.push_str(&format!(
        "chunkwake_system_memory_total_bytes {}\n",
        snapshot.proc.total_memory
    ));
    header(
        &mut out,
        "chunkwake_cpu_core_percent",
        "gauge",
        "Load of one core",
    );
    for (index, core) in snapshot.proc.cores.iter().enumerate() {
        out.push_str(&format!(
            "chunkwake_cpu_core_percent{{core=\"{index}\"}} {}\n",
            core.usage
        ));
    }

    header(&mut out, "chunkwake_players", "gauge", "Players online");
    out.push_str(&format!("chunkwake_players {}\n", snapshot.world.players));
    header(&mut out, "chunkwake_entities", "gauge", "Live mobs");
    out.push_str(&format!("chunkwake_entities {}\n", snapshot.world.mobs));
    header(
        &mut out,
        "chunkwake_dropped_items",
        "gauge",
        "Items on the ground",
    );
    out.push_str(&format!(
        "chunkwake_dropped_items {}\n",
        snapshot.world.dropped_items
    ));
    header(
        &mut out,
        "chunkwake_chunks_simulated",
        "gauge",
        "Chunks in simulation distance",
    );
    out.push_str(&format!(
        "chunkwake_chunks_simulated {}\n",
        snapshot.world.simulated_chunks
    ));
    header(
        &mut out,
        "chunkwake_chunks_loaded",
        "gauge",
        "Chunks known to the world",
    );
    out.push_str(&format!(
        "chunkwake_chunks_loaded {}\n",
        snapshot.world.loaded_chunks
    ));
    header(
        &mut out,
        "chunkwake_chunkgen_pending",
        "gauge",
        "Queued chunk generation jobs",
    );
    out.push_str(&format!(
        "chunkwake_chunkgen_pending {}\n",
        snapshot.world.gen_pending
    ));

    header(
        &mut out,
        "chunkwake_network_bytes_total",
        "counter",
        "Bytes over play connections",
    );
    out.push_str(&format!(
        "chunkwake_network_bytes_total{{direction=\"in\"}} {}\n",
        snapshot.net.bytes_in
    ));
    out.push_str(&format!(
        "chunkwake_network_bytes_total{{direction=\"out\"}} {}\n",
        snapshot.net.bytes_out
    ));
    header(
        &mut out,
        "chunkwake_network_packets_total",
        "counter",
        "Packets over play connections",
    );
    out.push_str(&format!(
        "chunkwake_network_packets_total{{direction=\"in\"}} {}\n",
        snapshot.net.packets_in
    ));
    out.push_str(&format!(
        "chunkwake_network_packets_total{{direction=\"out\"}} {}\n",
        snapshot.net.packets_out
    ));
    if let Some(ping) = snapshot.net.ping_avg {
        header(
            &mut out,
            "chunkwake_ping_milliseconds",
            "gauge",
            "Average player ping",
        );
        out.push_str(&format!("chunkwake_ping_milliseconds {ping}\n"));
    }

    header(
        &mut out,
        "chunkwake_uptime_seconds",
        "counter",
        "Server uptime",
    );
    out.push_str(&format!("chunkwake_uptime_seconds {}\n", snapshot.uptime));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_formats() {
        let mut s = Snapshot {
            uptime: 3_723,
            ..Snapshot::default()
        };
        assert_eq!(s.uptime_text(), "01:02:03");
        s.uptime = 90_061;
        assert_eq!(s.uptime_text(), "1d 01:01:01");
    }

    #[test]
    fn memory_and_rss_units() {
        let mut s = Snapshot::default();
        s.proc.rss = 512 * 1024 * 1024;
        s.proc.total_memory = 16 * 1024 * 1024 * 1024;
        s.proc.used_memory = 8 * 1024 * 1024 * 1024;
        assert!((s.rss_mib() - 512.0).abs() < 0.01);
        assert!((s.memory_total_gib() - 16.0).abs() < 0.01);
        assert!((s.memory_used_gib() - 8.0).abs() < 0.01);
    }

    #[test]
    fn prometheus_has_windows_and_core_labels() {
        let mut s = Snapshot::default();
        s.tick.tps_1m = 19.8;
        s.tick.tps_5m = 19.9;
        s.tick.tps_15m = 20.0;
        s.tick.mspt_p99 = 4.5;
        s.proc.cores = vec![CoreLoad { usage: 12.0 }, CoreLoad { usage: 40.0 }];
        s.net.ping_avg = Some(21.0);
        let text = prometheus(&s);
        assert!(text.contains("chunkwake_tps{window=\"1m\"} 19.8"));
        assert!(text.contains("chunkwake_tps{window=\"15m\"} 20"));
        assert!(text.contains("chunkwake_mspt{quantile=\"0.99\"} 4.5"));
        assert!(text.contains("chunkwake_cpu_core_percent{core=\"1\"} 40"));
        assert!(text.contains("chunkwake_ping_milliseconds 21"));
        assert!(text.contains("# TYPE chunkwake_up gauge"));
    }
}
