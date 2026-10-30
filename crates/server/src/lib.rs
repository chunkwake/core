//! Библиотека сервера: сборка мира и плана игры, доступная бенчмаркам.
//!
//! Бинарник `chunkwake` использует те же модули; здесь они открыты, чтобы
//! `cargo bench` мог построить настоящий `PlayPlan` и мерить тик, а не
//! подделку. Всё, что не нужно снаружи, остаётся приватным.

pub mod config;
pub mod players;
pub mod storage;
pub mod world_setup;

pub use config::Properties;
pub use world_setup::build_play_plan;

/// Версия Minecraft, под которую собран сервер (`version.json` ванильного jar).
pub const MC_VERSION: &str = "26.3";
