//! chunkwake-bridge — Paper-плагины в JVM внутри процесса через Panama FFM.
//!
//! JVM поднимается invocation API (`JNI_CreateJavaVM`) — это единственный
//! способ создать её в нативном процессе; JNI используется только чтобы один
//! раз вызвать `Bridge.boot(tableAddress)`. Дальше обе стороны общаются через
//! таблицу слотов u64 ([`Slot`]): Java кладёт туда адреса upcall-стабов FFM,
//! Rust — адреса своих `extern "C"` функций, плюс флаги в общей памяти.
//!
//! Все вызовы в Java идут из одного потока плагинов — это «main thread» Paper.

mod jvm;

use chunkwake_net::play::{PlayPlan, PluginHooks};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock, Weak};
use tokio::sync::oneshot;
use uuid::Uuid;

/// Раскладка таблицы; совпадает с `dev.chunkwake.bridge.Bridge`.
#[derive(Clone, Copy)]
#[repr(usize)]
enum Slot {
    UpEnable = 0,
    UpTick = 1,
    UpJoin = 2,
    UpQuit = 3,
    UpBlockBreak = 4,
    UpCommand = 5,
    DownSendMessage = 6,
    DownBroadcast = 7,
    DownLog = 8,
    FlagHasTasks = 9,
    FlagListeners = 10,
    ArgPluginsDir = 11,
    ArgPluginsDirLen = 12,
    DownSetCommands = 13,
}
const SLOTS: usize = 14;

const LISTEN_BLOCK_BREAK: u64 = 4;

type Table = [AtomicU64; SLOTS];

/// Сервер для downcall-ов из Java (одна JVM на процесс).
static HOST: OnceLock<Weak<PlayPlan>> = OnceLock::new();

enum Call {
    Tick,
    Join(Uuid, String),
    Quit(Uuid),
    BlockBreak(Uuid, [i32; 3], String, oneshot::Sender<bool>),
    Command(Uuid, String, oneshot::Sender<bool>),
}

/// Мост к JVM: хуки для `PlayPlan`.
pub struct Bridge {
    tx: mpsc::Sender<Call>,
    table: &'static Table,
    /// Тик уже в очереди: медленный плагин не копит тики.
    tick_queued: Arc<AtomicBool>,
}

/// Где искать API: `CHUNKWAKE_PLUGIN_API`, `<server>/api`, `java/dist` (dev).
pub fn find_api_dir(server_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("CHUNKWAKE_PLUGIN_API").map(PathBuf::from),
        Some(server_dir.join("api")),
        Some(PathBuf::from("java/dist")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|d| d.join("chunkwake-api.jar").is_file())
}

impl Bridge {
    /// Поднимает JVM, загружает и включает плагины из `plugins_dir`.
    /// Блокирует до конца `onEnable` всех плагинов; возвращает мост и число плагинов.
    pub fn start(
        java: &Path,
        api_dir: &Path,
        plugins_dir: &Path,
        plan: &Arc<PlayPlan>,
    ) -> Result<(Arc<Self>, i32), String> {
        let _ = HOST.set(Arc::downgrade(plan));
        let jvm_lib = jvm::find_library(java)?;
        let class_path = jvm::class_path(api_dir)?;
        let table: &'static Table = Box::leak(Box::new(std::array::from_fn(|_| AtomicU64::new(0))));
        set(
            table,
            Slot::DownSendMessage,
            send_message as *const () as u64,
        );
        set(table, Slot::DownBroadcast, broadcast as *const () as u64);
        set(table, Slot::DownLog, log as *const () as u64);
        set(
            table,
            Slot::DownSetCommands,
            set_commands as *const () as u64,
        );
        let dir: &'static str =
            Box::leak(plugins_dir.to_string_lossy().into_owned().into_boxed_str());
        set(table, Slot::ArgPluginsDir, dir.as_ptr() as u64);
        set(table, Slot::ArgPluginsDirLen, dir.len() as u64);

        let (tx, rx) = mpsc::channel::<Call>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<i32, String>>();
        let tick_queued = Arc::new(AtomicBool::new(false));
        let queued = tick_queued.clone();
        std::thread::Builder::new()
            .name("plugins".into())
            .stack_size(16 << 20)
            .spawn(move || {
                let booted = jvm::boot(&jvm_lib, &class_path, table as *const Table as i64)
                    .and_then(|()| {
                        let n = unsafe { up::<extern "C" fn() -> i32>(table, Slot::UpEnable)() };
                        if n < 0 {
                            Err("plugin startup failed".into())
                        } else {
                            Ok(n)
                        }
                    });
                let ok = booted.is_ok();
                let _ = ready_tx.send(booted);
                if ok {
                    run(table, rx, &queued);
                }
            })
            .map_err(|e| e.to_string())?;
        let n = ready_rx
            .recv()
            .map_err(|_| "plugin thread died".to_owned())??;
        Ok((
            Arc::new(Self {
                tx,
                table,
                tick_queued,
            }),
            n,
        ))
    }
}

fn set(table: &Table, slot: Slot, v: u64) {
    table[slot as usize].store(v, Ordering::Release);
}

fn get(table: &Table, slot: Slot) -> u64 {
    table[slot as usize].load(Ordering::Acquire)
}

/// Адрес upcall-стаба из таблицы как функция.
///
/// # Safety
/// `F` должен совпадать с `FunctionDescriptor` в `Bridge.boot`.
unsafe fn up<F: Copy>(table: &Table, slot: Slot) -> F {
    let addr = get(table, slot) as usize;
    assert!(addr != 0, "upcall {} not installed", slot as usize);
    unsafe { std::mem::transmute_copy::<usize, F>(&addr) }
}

/// Цикл потока плагинов: события по порядку, как в main thread Paper.
fn run(table: &'static Table, rx: mpsc::Receiver<Call>, tick_queued: &AtomicBool) {
    type JoinFn = extern "C" fn(i64, i64, *const u8, i32);
    type QuitFn = extern "C" fn(i64, i64);
    type BreakFn = extern "C" fn(i64, i64, i32, i32, i32, *const u8, i32) -> i32;
    type CommandFn = extern "C" fn(i64, i64, *const u8, i32) -> i32;
    let (tick, join, quit, block_break, command) = unsafe {
        (
            up::<extern "C" fn()>(table, Slot::UpTick),
            up::<JoinFn>(table, Slot::UpJoin),
            up::<QuitFn>(table, Slot::UpQuit),
            up::<BreakFn>(table, Slot::UpBlockBreak),
            up::<CommandFn>(table, Slot::UpCommand),
        )
    };
    let id = |u: Uuid| {
        let (hi, lo) = u.as_u64_pair();
        (hi as i64, lo as i64)
    };
    for call in rx {
        match call {
            Call::Tick => {
                tick_queued.store(false, Ordering::Release);
                tick();
            }
            Call::Join(u, name) => {
                let (hi, lo) = id(u);
                join(hi, lo, name.as_ptr(), name.len() as i32);
            }
            Call::Quit(u) => {
                let (hi, lo) = id(u);
                quit(hi, lo);
            }
            Call::BlockBreak(u, [x, y, z], key, reply) => {
                let (hi, lo) = id(u);
                let cancelled = block_break(hi, lo, x, y, z, key.as_ptr(), key.len() as i32);
                let _ = reply.send(cancelled != 0);
            }
            Call::Command(u, line, reply) => {
                let (hi, lo) = id(u);
                let known = command(hi, lo, line.as_ptr(), line.len() as i32);
                let _ = reply.send(known != 0);
            }
        }
    }
}

impl PluginHooks for Bridge {
    fn join(&self, uuid: Uuid, name: &str) {
        let _ = self.tx.send(Call::Join(uuid, name.to_owned()));
    }

    fn quit(&self, uuid: Uuid) {
        let _ = self.tx.send(Call::Quit(uuid));
    }

    fn block_break(
        &self,
        uuid: Uuid,
        pos: [i32; 3],
        block: &str,
    ) -> Option<oneshot::Receiver<bool>> {
        if get(self.table, Slot::FlagListeners) & LISTEN_BLOCK_BREAK == 0 {
            return None;
        }
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Call::BlockBreak(uuid, pos, block.to_owned(), reply))
            .ok()?;
        Some(rx)
    }

    fn command(&self, uuid: Uuid, line: &str) -> oneshot::Receiver<bool> {
        let (reply, rx) = oneshot::channel();
        let _ = self.tx.send(Call::Command(uuid, line.to_owned(), reply));
        rx
    }

    fn tick(&self) {
        // Без задач планировщика граница не пересекается вовсе.
        if get(self.table, Slot::FlagHasTasks) != 0
            && !self.tick_queued.swap(true, Ordering::AcqRel)
        {
            let _ = self.tx.send(Call::Tick);
        }
    }
}

// --- downcalls из Java ---

/// # Safety
/// `ptr..ptr+len` — UTF-8 из Java (живёт на время вызова).
unsafe fn text<'a>(ptr: *const u8, len: i32) -> &'a str {
    if ptr.is_null() || len <= 0 {
        return "";
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("")
}

fn host() -> Option<Arc<PlayPlan>> {
    HOST.get().and_then(Weak::upgrade)
}

extern "C" fn send_message(hi: i64, lo: i64, ptr: *const u8, len: i32) {
    if let Some(plan) = host() {
        plan.send_json(Uuid::from_u64_pair(hi as u64, lo as u64), unsafe {
            text(ptr, len)
        });
    }
}

extern "C" fn broadcast(ptr: *const u8, len: i32) {
    if let Some(plan) = host() {
        plan.broadcast_json(unsafe { text(ptr, len) });
    }
}

extern "C" fn set_commands(ptr: *const u8, len: i32) {
    let names = unsafe { text(ptr, len) };
    if let Some(plan) = host() {
        plan.set_commands(
            names
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect(),
        );
    }
}

extern "C" fn log(level: i32, ptr: *const u8, len: i32) {
    let msg = unsafe { text(ptr, len) };
    match level {
        0 => tracing::error!("{msg}"),
        1 => tracing::warn!("{msg}"),
        2 => tracing::info!("{msg}"),
        _ => tracing::debug!("{msg}"),
    }
}
