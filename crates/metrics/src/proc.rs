//! Загрузка процесса и машины: ядра, RSS, потоки, память системы.
//!
//! `sysinfo` берёт числа из системных вызовов ОС (Windows: `GetSystemTimes`,
//! `GetProcessMemoryInfo`; Linux: `/proc`), поэтому своих unsafe-обёрток не
//! нужно, а крейт собирается на обеих целевых платформах.
//!
//! В `sysinfo` 0.40 конструкторы возвращают `Result`, а загрузка CPU — это
//! разница между двумя опросами, поэтому первый снимок всегда нулевой.

use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// Загрузка одного ядра.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct CoreLoad {
    /// Загрузка 0..100 %.
    pub usage: f32,
}

/// Снимок процесса и системы.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProcStats {
    /// Загрузка по ядрам, %.
    pub cores: Vec<CoreLoad>,
    /// Средняя загрузка по всем ядрам, %.
    pub cpu_total: f32,
    /// Процесс занимает столько процентов одного ядра (может быть > 100).
    pub process_cpu: f32,
    /// Резидентная память процесса, байты.
    pub rss: u64,
    /// Виртуальная память процесса, байты.
    pub vsz: u64,
    pub threads: usize,
    /// Сколько живёт процесс, секунды.
    pub uptime: u64,
    /// Память системы: всего и занято, байты.
    pub total_memory: u64,
    pub used_memory: u64,
    /// Модель процессора.
    pub cpu_brand: String,
    /// Частота, МГц.
    pub cpu_mhz: u64,
}

/// Опрашивает систему и свой процесс.
pub struct ProcessSampler {
    /// `None` — ОС не поддерживается: тогда отдаём только нули и не падаем.
    system: Option<System>,
    pid: Option<sysinfo::Pid>,
    brand: String,
}

impl ProcessSampler {
    pub fn new() -> Self {
        // Первый опрос нужен, чтобы система знала ядра и запомнила счётчики:
        // загрузку `sysinfo` считает как разницу с этим опросом.
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(sysinfo::CpuRefreshKind::everything())
                .with_memory(sysinfo::MemoryRefreshKind::everything()),
        )
        .or_else(|error| {
            tracing_free_log(&format!("metrics: cannot read system info: {error}"));
            System::new()
        })
        .map(|mut system| {
            system.refresh_cpu_usage();
            system
        })
        .ok();
        let brand = system
            .as_ref()
            .and_then(|system| system.cpus().first())
            .map(|cpu| cpu.brand().trim().to_owned())
            .unwrap_or_default();
        Self {
            system,
            pid: sysinfo::get_current_pid().ok(),
            brand,
        }
    }

    /// Опрос раз в секунду: возвращает снимок с уже посчитанной загрузкой.
    pub fn sample(&mut self) -> ProcStats {
        let Some(system) = self.system.as_mut() else {
            return ProcStats::default();
        };
        system.refresh_cpu_usage();
        system.refresh_memory();
        if let Some(pid) = self.pid {
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                ProcessRefreshKind::nothing()
                    .with_cpu()
                    .with_memory()
                    .with_tasks(),
            );
        }
        let cores: Vec<CoreLoad> = system
            .cpus()
            .iter()
            .map(|cpu| CoreLoad { usage: cpu.usage() })
            .collect();
        let cpu_total = if cores.is_empty() {
            0.0
        } else {
            cores.iter().map(|c| c.usage).sum::<f32>() / cores.len() as f32
        };
        let cpu_mhz = system.cpus().first().map_or(0, |c| c.frequency());
        let process = self.pid.and_then(|pid| system.process(pid));
        let (process_cpu, rss, vsz, threads, uptime) = match process {
            Some(p) => (
                p.cpu_usage(),
                p.memory(),
                p.virtual_memory(),
                // `tasks()` отдаёт дочерние задачи только на Linux; на Windows
                // числа потоков нет — 0 значит «неизвестно», а не «один».
                p.tasks().map_or(0, |set| set.len()),
                p.run_time(),
            ),
            None => (0.0, 0, 0, 0, 0),
        };
        ProcStats {
            cores,
            cpu_total,
            process_cpu,
            rss,
            vsz,
            threads,
            uptime,
            total_memory: system.total_memory(),
            used_memory: system.used_memory(),
            cpu_brand: self.brand.clone(),
            cpu_mhz,
        }
    }
}

impl Default for ProcessSampler {
    fn default() -> Self {
        Self::new()
    }
}

/// Сообщение об ошибке без зависимости от `tracing`.
fn tracing_free_log(message: &str) {
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_cores_and_memory() {
        let mut sampler = ProcessSampler::new();
        let stats = sampler.sample();
        assert!(!stats.cores.is_empty(), "хотя бы одно ядро");
        assert!(stats.total_memory > 0);
        assert!(stats.rss > 0, "процесс занимает память");
        assert!(stats.cpu_total <= 100.0);
    }
}
