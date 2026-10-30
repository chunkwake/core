//! Пул генерации чанков.
//!
//! Счёт чанка (шум, биомы, свет, кодирование, сжатие) идёт на своих потоках, а не
//! в задаче сессии игрока: иначе тик делит с ним ядра и мир замирает. Потоки
//! работают с приоритетом ниже обычного, поэтому тик их вытесняет, а не наоборот.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Одна задача пула: готовая работа без параметров.
pub type Job = Box<dyn FnOnce() + Send + 'static>;

/// Очередь задач и потоки-исполнители. Живёт столько же, сколько мир.
///
/// Очередь — `crossbeam_channel` с несколькими получателями. Раньше стоял
/// `std::sync::mpsc` за общим `Mutex`, и замок держался **на время `recv()`**:
/// простаивающие потоки выстраивались в очередь на замке, а выдача задачи
/// сериализовалась через него. Здесь каждый поток забирает задачу сам.
pub struct GenPool {
    queue: crossbeam_channel::Sender<Job>,
    /// Сколько задач ещё не выполнено: по нему видно, успевает ли пул.
    pending: AtomicUsize,
    /// Сколько потоков заведено (`/top` показывает глубину пула).
    workers: usize,
}

impl GenPool {
    /// `workers` потоков; вызывающий сам оставляет ядро тику.
    pub fn new(workers: usize) -> Arc<Self> {
        let (tx, rx) = crossbeam_channel::unbounded::<Job>();
        let workers = workers.max(1);
        for i in 0..workers {
            let rx = rx.clone();
            std::thread::Builder::new()
                .name(format!("chunk-gen-{i}"))
                .spawn(move || {
                    crate::play::background_priority();
                    // Пока есть отправитель, `recv` ждёт задачу без замка.
                    while let Ok(job) = rx.recv() {
                        job();
                    }
                })
                .expect("spawn chunk generation thread");
        }
        Arc::new(Self {
            queue: tx,
            pending: AtomicUsize::new(0),
            workers,
        })
    }

    /// Сколько потоков генерации заведено.
    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Ставит задачу в очередь. Порядок очереди — порядок отправки: ближние первыми.
    pub fn submit(&self, job: Job) {
        self.pending.fetch_add(1, Ordering::Relaxed);
        if self.queue.send(job).is_err() {
            self.pending.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Задача выполнена (или отброшена): уменьшает счётчик незавершённых.
    pub fn done(&self) {
        self.pending.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }
}

/// Сколько потоков отдать генерации: все ядра, кроме одного под тик.
pub fn workers() -> usize {
    static CORES: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let cores = *CORES.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
    });
    cores.saturating_sub(1).max(1)
}
