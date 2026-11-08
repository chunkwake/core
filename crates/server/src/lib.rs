//! Библиотека сервера: сборка мира и план игры, доступная бенчмаркам.
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

/// Аллокатор процесса.
///
/// Горячий путь тика — тысячи коротких `Vec` и `HashMap` за тик (пакеты,
/// списки движений, буферы форм мобов). Системный аллокатор Windows держит
/// очередь на каждый поток, но освобождение чужого блока идёт через общий
/// замок; mimalloc раздаёт арены по потокам и освобождает чужие блоки без
/// общей блокировки, а кэш мелких размеров совпадает с профилем движка
/// (десятки — сотни байт).
///
/// Объявлен в библиотеке, а не в `main.rs`, чтобы бенчмарки мерили тот же
/// аллокатор, что и сервер.
#[cfg(not(feature = "alloc-count"))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// С признаком `alloc-count` — mimalloc со счётом живой кучи.
#[cfg(feature = "alloc-count")]
pub mod alloc_count;
#[cfg(feature = "alloc-count")]
#[global_allocator]
static ALLOC: alloc_count::Counting = alloc_count::Counting;

/// Версия Minecraft, под которую собран сервер (`version.json` ванильного jar).
pub const MC_VERSION: &str = "26.3";
