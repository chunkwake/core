//! Порядок позиций в `section_blocks_update`, как у ванили.
//!
//! `ChunkHolder` копит сменённые позиции секции в `ShortOpenHashSet`
//! (fastutil), а пакет перечисляет их обходом этого набора: сначала ключ 0,
//! затем ячейки таблицы сверху вниз. Повторная запись позиции место не
//! меняет — состояние берётся на момент рассылки, то есть последнее.

/// `HashCommon.mix`.
fn mix(k: u16) -> u32 {
    let h = (k as i16 as i32 as u32).wrapping_mul(0x9E37_79B9);
    h ^ (h >> 16)
}

/// `HashCommon.arraySize` с коэффициентом заполнения 0.75.
fn array_size(expected: usize) -> usize {
    (expected * 4).div_ceil(3).next_power_of_two().max(2)
}

/// Изменения секции `(локальная позиция, состояние)` в порядке записи →
/// по одному на позицию с последним состоянием, в порядке ванили.
pub(super) fn vanilla_order(blocks: &[(u16, u32)]) -> Vec<(u16, u32)> {
    let mut last = vec![u32::MAX; 4096];
    let mut n = 32;
    let mut table: Vec<u16> = vec![0; n];
    let mut has_zero = false;
    let mut size = 0;
    let mut max_fill = 24;
    let place = |table: &mut [u16], k: u16| {
        let mask = table.len() - 1;
        let mut pos = mix(k) as usize & mask;
        while table[pos] != 0 {
            pos = (pos + 1) & mask;
        }
        table[pos] = k;
    };
    for &(k, state) in blocks {
        let fresh = last[k as usize & 4095] == u32::MAX;
        last[k as usize & 4095] = state;
        if !fresh {
            continue;
        }
        if k == 0 {
            has_zero = true;
        } else {
            place(&mut table, k);
        }
        // `if (size++ >= maxFill) rehash(arraySize(size + 1, f))`.
        let grow = size >= max_fill;
        size += 1;
        if grow {
            // `rehash` переносит ключи обходом старой таблицы сверху вниз.
            n = array_size(size + 1);
            let old = std::mem::replace(&mut table, vec![0; n]);
            for &k in old.iter().rev().filter(|k| **k != 0) {
                place(&mut table, k);
            }
            max_fill = ((n * 3).div_ceil(4)).min(n - 1);
        }
    }
    has_zero
        .then_some(0u16)
        .into_iter()
        .chain(table.iter().rev().copied().filter(|k| *k != 0))
        .map(|k| (k, last[k as usize]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_in_one_slot_come_out_reversed() {
        // Руда (9,200,10) и блок над ней: оба в ячейке 7, второй уходит в 8,
        // обход сверху вниз отдаёт его первым — как пишет ваниль.
        let ore = 9 << 8 | 10 << 4 | 8;
        let above = ore + 1;
        let got = vanilla_order(&[(ore, 1), (above, 2), (ore, 3)]);
        assert_eq!(got, vec![(above, 2), (ore, 3)]);
    }

    #[test]
    fn zero_first_and_every_position_once_after_growth() {
        let keys: Vec<(u16, u32)> = (0..300u16).rev().map(|k| (k * 13 % 4096, k as u32)).collect();
        let got = vanilla_order(&keys);
        assert_eq!(got.len(), 300);
        assert_eq!(got[0].0, 0);
        let mut sorted: Vec<u16> = got.iter().map(|g| g.0).collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 300);
    }
}
