//! Метрики сервера для наблюдения без внешних агентов.
//!
//! Крейт ничего не знает о сети и мире: ему отдают замеры тика, счётчики и
//! загрузку процесса, а он считает окна TPS/MSPT, перцентили и собирает
//! снимок, который уходит в чат, в терминал и в Prometheus.
//!
//! Почему отдельно от `chunkwake-net`: замеры нужны и бинарнику (консоль,
//! HTTP), и лаборатории, а `net` — самый горячий крейт; тянуть в него
//! `sysinfo` ради сэмплера процесса незачем.

pub mod http;
pub mod live;
pub mod proc;
pub mod render;
pub mod snapshot;
pub mod tick;
pub mod web;

pub use live::{HistoryPoint, Live};
pub use proc::ProcessSampler;
pub use proc::{CoreLoad, ProcStats};
pub use snapshot::{Limits, NetCounters, Snapshot, TickReport, WorldCounters};
pub use tick::TickStats;
