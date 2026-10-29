//! Пул генерации чанков.
//!
//! Счёт чанка (шум, биомы, свет, кодирование, сжатие) идёт на своих потоках, а не
//! в задаче сессии игрока: иначе тик делит с ним ядра и мир замирает. Потоки
//! работают с приоритетом ниже обычного, поэтому тик их вытесняет, а не наоборот.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Одна задача пула: готовая работа без параметров.
pub type Job = Box<dyn FnOnce() + Send + 'static>;

/// Очередь задач и потоки-исполнители. Живёт столько же, сколько мир.
pub struct GenPool {
    queue: Mutex<Sender<Job>>,
    /// Сколько задач ещё не выполнено: по нему видно, успевает ли пул.
    pending: AtomicUsize,
}

impl GenPool {
    /// `workers` потоков; вызывающий сам оставляет ядро тику.
    pub fn new(workers: usize) -> Arc<Self> {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let workers = workers.max(1);
        for i in 0..workers {
            let rx = rx.clone();
            std::thread::Builder::new()
                .name(format!("chunk-gen-{i}"))
                .spawn(move || {
                    crate::play::background_priority();
                    loop {
                        // Замок держится только на ожидании задачи, не на самой работе.
                        let job = {
                            let guard = rx.lock().unwrap();
                            guard.recv()
                        };
                        let Ok(job) = job else { return };
                        job();
                    }
                })
                .expect("spawn chunk generation thread");
        }
        Arc::new(Self {
            queue: Mutex::new(tx),
            pending: AtomicUsize::new(0),
        })
    }

    /// Ставит задачу в очередь. Порядок очереди — порядок отправки: ближние первыми.
    pub fn submit(&self, job: Job) {
        self.pending.fetch_add(1, Ordering::Relaxed);
        let tx = self.queue.lock().unwrap();
        if tx.send(job).is_err() {
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
