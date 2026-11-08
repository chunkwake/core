//! Счёт живой кучи для поиска памяти (признак `alloc-count`).
//!
//! Обёртка над mimalloc: на каждое выделение и освобождение — атомарное
//! сложение по классу размера (степень двойки). Сравнение живых байт с
//! закоммиченной памятью процесса отвечает, где память: в данных или в
//! удержании аллокатором. В обычной сборке не участвует.

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicI64, Ordering};

pub struct Counting;

const CLASSES: usize = 48;

static BYTES: [AtomicI64; CLASSES] = [const { AtomicI64::new(0) }; CLASSES];
static COUNT: [AtomicI64; CLASSES] = [const { AtomicI64::new(0) }; CLASSES];

fn class(size: usize) -> usize {
    (usize::BITS - size.max(1).leading_zeros()) as usize % CLASSES
}

/// Выборка мест выделения: класс размера из `CHUNKWAKE_ALLOC_TRACE`
/// (например `13` — блоки 4–8 КиБ), каждый 64-й объект со стеком, пока жив.
static TRACE_CLASS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);
static SEEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
type Traces = std::collections::HashMap<usize, String>;
static LIVE: std::sync::Mutex<Option<Traces>> = std::sync::Mutex::new(None);

thread_local! {
    static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Включить выборку по классу размера из окружения.
pub fn init_trace() {
    if let Some(c) = std::env::var("CHUNKWAKE_ALLOC_TRACE")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        *LIVE.lock().unwrap() = Some(Traces::new());
        TRACE_CLASS.store(c, Ordering::Relaxed);
    }
}

fn traced(ptr: *mut u8, size: usize, alloc: bool) {
    if class(size) != TRACE_CLASS.load(Ordering::Relaxed) {
        return;
    }
    if INSIDE.with(|i| i.replace(true)) {
        return;
    }
    if alloc {
        if SEEN.fetch_add(1, Ordering::Relaxed) % 64 == 0 {
            let bt = std::backtrace::Backtrace::force_capture().to_string();
            // Только кадры нашего кода: короче и группируется лучше.
            let frames: Vec<&str> = bt
                .lines()
                .filter(|l| l.contains("chunkwake") && !l.contains("alloc_count"))
                .take(6)
                .map(str::trim)
                .collect();
            if let Ok(mut live) = LIVE.lock()
                && let Some(map) = live.as_mut()
            {
                map.insert(ptr as usize, frames.join(" <- "));
            }
        }
    } else if let Ok(mut live) = LIVE.lock()
        && let Some(map) = live.as_mut()
    {
        map.remove(&(ptr as usize));
    }
    INSIDE.with(|i| i.set(false));
}

/// Самые частые живые места выделения из выборки.
pub fn trace_report() -> String {
    let Ok(live) = LIVE.lock() else {
        return String::new();
    };
    let Some(map) = live.as_ref() else {
        return String::new();
    };
    let mut by: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for site in map.values() {
        *by.entry(site.as_str()).or_default() += 1;
    }
    let mut v: Vec<_> = by.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.iter()
        .take(8)
        .map(|(site, n)| format!("\n  x{} {site}", n * 64))
        .collect()
}

fn note(size: usize, sign: i64) {
    let c = class(size);
    BYTES[c].fetch_add(sign * size as i64, Ordering::Relaxed);
    COUNT[c].fetch_add(sign, Ordering::Relaxed);
}

// SAFETY: делегирует всё mimalloc, сам память не трогает.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { mimalloc::MiMalloc.alloc(layout) };
        if !p.is_null() {
            note(layout.size(), 1);
            traced(p, layout.size(), true);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { mimalloc::MiMalloc.alloc_zeroed(layout) };
        if !p.is_null() {
            note(layout.size(), 1);
            traced(p, layout.size(), true);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note(layout.size(), -1);
        traced(ptr, layout.size(), false);
        unsafe { mimalloc::MiMalloc.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { mimalloc::MiMalloc.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            note(layout.size(), -1);
            traced(ptr, layout.size(), false);
            note(new_size, 1);
            traced(p, new_size, true);
        }
        p
    }
}

/// Живые байты и объекты по классам размера: `≤2^k Б: N МБ в M объектах`.
pub fn report() -> String {
    let mut total = 0i64;
    let mut parts = Vec::new();
    for c in 0..CLASSES {
        let (b, n) = (
            BYTES[c].load(Ordering::Relaxed),
            COUNT[c].load(Ordering::Relaxed),
        );
        total += b;
        if b >= 4 << 20 {
            parts.push(format!("<{}: {} MB/{}", 1u64 << c, b >> 20, n));
        }
    }
    format!("live {} MB; {}", total >> 20, parts.join(", "))
}
