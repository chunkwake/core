//! Пул потоков тика: сущности считаются параллельно, а их последствия
//! (пакеты, звуки, удары, выпадения) сливаются последовательно в порядке
//! обхода. Поэтому результат тика тот же, что у последовательного прохода:
//! каждый кусок пишет только в своих сущностей и в свой буфер, а мир в это
//! время только читается.
//!
//! Число потоков — `CHUNKWAKE_TICK_THREADS`, по умолчанию половина ядер (до 16):
//! вторая половина остаётся генерации, свету и сессиям.

use rayon::prelude::*;
use std::sync::OnceLock;

fn pool() -> &'static rayon::ThreadPool {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = std::env::var("CHUNKWAKE_TICK_THREADS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map_or(1, |n| n.get())
                    .div_ceil(2)
                    .clamp(1, 16)
            });
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("tick-{i}"))
            .build()
            .expect("пул потоков тика")
    })
}

/// Потоков в пуле тика.
pub fn threads() -> usize {
    pool().current_num_threads()
}

/// Регионный тик вместо кусков: `CHUNKWAKE_TICK_REGIONS=1` (по умолчанию
/// куски). Спрашивается один раз: тик идёт двадцать раз в секунду.
pub fn regions_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("CHUNKWAKE_TICK_REGIONS").as_deref() == Ok("1"))
}

/// Печать раскладки регионов: `CHUNKWAKE_TICK_DEBUG=1`.
fn tick_debug() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("CHUNKWAKE_TICK_DEBUG").as_deref() == Ok("1"))
}

/// Делит `items` на подряд идущие куски не короче `min_chunk`, считает `f`
/// по кускам параллельно и возвращает результаты в порядке кусков. Мало
/// элементов или один поток — один вызов `f` на текущем потоке, без пула.
pub fn chunks<T: Send, R: Send>(
    items: &mut [T],
    min_chunk: usize,
    f: impl Fn(&mut [T]) -> R + Sync,
) -> Vec<R> {
    let n = threads();
    if n <= 1 || items.len() < min_chunk.max(1) * 2 {
        return vec![f(items)];
    }
    // Кусков в 4 раза больше потоков: мобы неравны по цене (поиск пути), и
    // мелкие куски выравнивают нагрузку.
    let size = items.len().div_ceil(n * 4).max(min_chunk.max(1));
    pool().install(|| items.par_chunks_mut(size).map(&f).collect())
}

/// Сколько разных регионов при размере `1 << shift` чанков.
fn count_regions<T>(items: &[T], chunk: &impl Fn(&T) -> (i32, i32), shift: u32) -> usize {
    // FxHash: это горячий путь тика, SipHash на тысячах сущностей заметен.
    let mut seen = rustc_hash::FxHashSet::with_capacity_and_hasher(items.len(), Default::default());
    for it in items {
        let (cx, cz) = chunk(it);
        seen.insert((cx >> shift, cz >> shift));
    }
    seen.len()
}

/// Размер региона: обычно 8x8 чанков (shift 3), но если регионов меньше,
/// чем потоков пула, регион дробится до 4x4, 2x2, 1x1. Иначе пул простаивает
/// (A/B с двумя 8x8-регионами давал 2 задачи на 8 потоков и был медленнее
/// кусков). Внутри региона сущности независимы: удары и дроп между ними идут
/// через очередь, поэтому дробление не меняет результат тика.
fn pick_shift<T>(items: &[T], chunk: &impl Fn(&T) -> (i32, i32), threads: usize) -> (u32, usize) {
    let mut chosen = (3u32, 0usize);
    for shift in [3u32, 2, 1, 0] {
        let regions = count_regions(items, chunk, shift);
        chosen = (shift, regions);
        if threads <= 1 || regions >= threads {
            break;
        }
    }
    chosen
}

/// Делит `items` по регионам и считает `f` на каждом. Размер региона
/// адаптивный (см. [`pick_shift`]): при нехватке регионов он дробится, чтобы
/// задач было не меньше потоков, а при избытке соседние регионы
/// группируются в задачи (примерно вчетверо больше потоков).
///
/// Порядок регионов и элементов внутри них детерминирован (сортировка по
/// ключу), поэтому слияние результатов совпадает с последовательным обходом:
/// побочные эффекты (удары, дроп, звуки) применяются в стабильном порядке.
pub fn regions<T: Send, R: Send>(
    items: &mut [T],
    chunk: impl Fn(&T) -> (i32, i32) + Sync,
    f: impl Fn(&mut [T]) -> R + Sync,
) -> Vec<R> {
    if items.is_empty() {
        return Vec::new();
    }
    let n = threads();
    if n <= 1 {
        items.sort_by_key(|it| chunk(it));
        return vec![f(items)];
    }
    let (shift, _) = pick_shift(items, &chunk, n);
    let cell = |it: &T| {
        let (cx, cz) = chunk(it);
        (cx >> shift, cz >> shift)
    };
    items.sort_by_key(|it| cell(it));
    // Границы регионов в индексах элементов.
    let mut bounds = Vec::new();
    bounds.push(0usize);
    for i in 1..items.len() {
        if cell(&items[i]) != cell(&items[i - 1]) {
            bounds.push(i);
        }
    }
    bounds.push(items.len());
    let regions = bounds.len() - 1;
    if regions <= 1 {
        return vec![f(items)];
    }
    // Соседние регионы сливаются в задачу по бюджету сущностей: маленькие
    // регионы (пара мобов) не должны превращаться в 50 задач — настройка
    // задачи (with_dim, 16 буферов эффектов, rand) дороже самих мобов.
    // Регион при этом никогда не режется: задача = целые регионы.
    let target = n.max(1).min(regions);
    let budget = items.len().div_ceil(target).max(1);
    let mut slices = Vec::new();
    let mut rest = items;
    let mut region = 0usize;
    while region < regions {
        let start = bounds[region];
        let mut end_region = region;
        let mut count = 0usize;
        while end_region < regions && (count == 0 || count < budget) {
            count += bounds[end_region + 1] - bounds[end_region];
            end_region += 1;
        }
        let len = bounds[end_region] - start;
        let (head, tail) = rest.split_at_mut(len);
        slices.push(head);
        rest = tail;
        region = end_region;
    }
    if tick_debug() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CALLS: AtomicU64 = AtomicU64::new(0);
        if CALLS.fetch_add(1, Ordering::Relaxed).is_multiple_of(64) {
            eprintln!(
                "tick regions: threads={n} shift={shift} regions={regions} tasks={}",
                slices.len()
            );
        }
    }
    pool().install(|| slices.into_par_iter().map(&f).collect())
}

/// Параллельная карта строго по регионам: границы задач совпадают с
/// границами регионов (регионы не сливаются). Нужна блочным фазам
/// Folia-режима: воркер региона не трогает чужие регионы напрямую, а
/// складывает правки в очередь.
pub fn regions_strict<T: Send, K: Ord + Copy + Send, R: Send>(
    items: &mut [T],
    key: impl Fn(&T) -> K + Sync,
    f: impl Fn(&mut [T]) -> R + Sync,
) -> Vec<R> {
    if items.is_empty() {
        return Vec::new();
    }
    items.sort_by_key(|it| key(it));
    let mut bounds = vec![0usize];
    for i in 1..items.len() {
        if key(&items[i]) != key(&items[i - 1]) {
            bounds.push(i);
        }
    }
    bounds.push(items.len());
    let n = threads();
    if n <= 1 || bounds.len() <= 2 {
        return vec![f(items)];
    }
    let mut slices = Vec::new();
    let mut rest = items;
    for w in bounds.windows(2) {
        let (head, tail) = rest.split_at_mut(w[1] - w[0]);
        slices.push(head);
        rest = tail;
    }
    pool().install(|| slices.into_par_iter().map(&f).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn chunks_keep_order() {
        let mut v: Vec<u32> = (0..10_000).collect();
        let parts = super::chunks(&mut v, 16, |c| {
            for x in c.iter_mut() {
                *x *= 2;
            }
            c.to_vec()
        });
        let flat: Vec<u32> = parts.into_iter().flatten().collect();
        assert_eq!(flat, (0..10_000).map(|x| x * 2).collect::<Vec<_>>());
    }

    #[test]
    fn adaptive_shift_splits_when_few_regions() {
        // 16 чанков подряд = 2 региона 8x8. На 8 потоков регион дробится
        // до 4x4 (8 регионов), но не мельче.
        let v: Vec<(i32, i32)> = (0..16).map(|cx| (cx, 0)).collect();
        assert_eq!(super::pick_shift(&v, &|c: &(i32, i32)| *c, 8), (1, 8));
        // На 24 потока — до 2x2 (16 регионов): ровно столько, сколько нужно.
        assert_eq!(super::pick_shift(&v, &|c: &(i32, i32)| *c, 24), (0, 16));
        // Два потока — хватает одного 8x8-региона? Нет: их два, это >= 2,
        // поэтому остаётся 8x8.
        assert_eq!(super::pick_shift(&v, &|c: &(i32, i32)| *c, 2), (3, 2));
    }

    #[test]
    fn regions_merge_small_regions_into_few_tasks() {
        // 7 регионов по 100 сущностей: маленькие регионы сливаются так,
        // чтобы задач было не больше потоков пула и регион не резался.
        let mut v: Vec<(i32, i32)> = (0..700).map(|i| ((i / 100) * 8, 0)).collect();
        let parts = super::regions(&mut v, |c: &(i32, i32)| *c, |slice| slice.len());
        let n = super::threads();
        assert_eq!(parts.len(), n.clamp(1, 7));
        assert!(parts.iter().all(|len| len % 100 == 0), "регион разрезан");
    }

    #[test]
    fn regions_keep_order_and_merge_adjacent() {
        // Чанки 0..64 по x, по одному элементу: 8 регионов 8x8.
        let mut v: Vec<((i32, i32), u64)> = (0..64).map(|cx| ((cx, 0), cx as u64)).collect();
        let n = super::threads();
        let (shift, _) = super::pick_shift(&v, &|x: &((i32, i32), u64)| x.0, n);
        let parts = super::regions(
            &mut v,
            |(c, _)| *c,
            |slice| slice.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
        );
        let flat: Vec<u64> = parts.into_iter().flatten().collect();
        // Регионы идут по возрастанию ключа, внутри — исходный порядок.
        let mut expected: Vec<u64> = (0..64u64).collect();
        expected.sort_by_key(|cx| (*cx as i32 >> shift, 0));
        assert_eq!(flat, expected);
    }
}
