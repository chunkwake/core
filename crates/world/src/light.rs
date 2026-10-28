//! Свет: небесный и блочный, по ниббл-массиву на секцию (как `DataLayer`).
//!
//! Изменение блока пересчитывается инкрементально (снятие старого света BFS-ом
//! и распространение нового), стоимость — по размеру изменения, а не мира.
//! Полный пересчёт — только для столбца без сохранённого света.

use crate::ChunkColumn;
use std::collections::VecDeque;
use std::sync::Arc;

/// Свет секции: одно значение на все 4096 блоков или ниббл-массив за `Arc`.
#[derive(Clone, Debug, PartialEq)]
pub enum LightLayer {
    Uniform(u8),
    Full(Arc<[u8; 2048]>),
}

impl LightLayer {
    pub fn get(&self, i: usize) -> u8 {
        match self {
            LightLayer::Uniform(v) => *v,
            LightLayer::Full(a) => (a[i >> 1] >> ((i & 1) * 4)) & 15,
        }
    }

    pub fn set(&mut self, i: usize, v: u8) {
        if let LightLayer::Uniform(u) = *self {
            if u == v {
                return;
            }
            *self = LightLayer::Full(Arc::new([u | u << 4; 2048]));
        }
        let LightLayer::Full(a) = self else {
            unreachable!()
        };
        let a = Arc::make_mut(a);
        let shift = (i & 1) * 4;
        a[i >> 1] = (a[i >> 1] & !(15 << shift)) | v << shift;
    }

    /// Массив как в `DataLayer::getData` (2048 байт, младший ниббл — чётный индекс).
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            LightLayer::Uniform(v) => vec![v | v << 4; 2048],
            LightLayer::Full(a) => a.to_vec(),
        }
    }

    /// Из массива `DataLayer`; неверная длина — `None`.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let mut layer = LightLayer::Full(Arc::new(b.try_into().ok()?));
        layer.compact();
        Some(layer)
    }

    pub fn is_zero(&self) -> bool {
        match self {
            LightLayer::Uniform(v) => *v == 0,
            LightLayer::Full(a) => a.iter().all(|&b| b == 0),
        }
    }

    /// Однородный массив → одно значение.
    pub fn compact(&mut self) {
        if let LightLayer::Full(a) = self {
            let b = a[0];
            if b & 15 == b >> 4 && a.iter().all(|&x| x == b) {
                *self = LightLayer::Uniform(b & 15);
            }
        }
    }
}

/// Свойства света состояний блоков: излучение, затухание и
/// «небесный свет 15 проходит вниз без ослабления» (`propagatesSkylightDown`).
pub struct LightTable {
    /// Биты 0–3 — излучение, 4–7 — затухание.
    props: Vec<u8>,
    sky_down: Vec<bool>,
}

impl LightTable {
    /// Строки по id состояния: `(излучение, затухание, небо_вниз)`.
    pub fn new(rows: impl IntoIterator<Item = (u8, u8, bool)>) -> Self {
        let (props, sky_down) = rows
            .into_iter()
            .map(|(e, d, s)| (e.min(15) | d.min(15) << 4, s))
            .unzip();
        Self { props, sky_down }
    }

    /// Неизвестное состояние — непрозрачный блок без света.
    pub fn emission(&self, state: u32) -> u8 {
        self.props.get(state as usize).map_or(0, |p| p & 15)
    }

    pub fn dampening(&self, state: u32) -> u8 {
        self.props.get(state as usize).map_or(15, |p| p >> 4)
    }

    pub fn sky_down(&self, state: u32) -> bool {
        self.sky_down.get(state as usize).copied().unwrap_or(false)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Sky,
    Block,
}

/// Доступ к столбцам мира по координатам чанка. `column_mut` вызывается только
/// при реальном изменении света (столбец может быть создан по требованию).
pub trait LightAccess {
    fn column(&mut self, cx: i32, cz: i32) -> Option<&ChunkColumn>;
    fn column_mut(&mut self, cx: i32, cz: i32) -> Option<&mut ChunkColumn>;
}

impl ChunkColumn {
    pub fn light(&self, kind: Kind) -> &[LightLayer] {
        match kind {
            Kind::Sky => &self.sky_light,
            Kind::Block => &self.block_light,
        }
    }

    fn light_mut(&mut self, kind: Kind) -> &mut [LightLayer] {
        match kind {
            Kind::Sky => &mut self.sky_light,
            Kind::Block => &mut self.block_light,
        }
    }
}

const DOWN: usize = 0;
const DIRS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

fn index(p: [i32; 3]) -> usize {
    ((((p[1] & 15) << 8) | ((p[2] & 15) << 4) | (p[0] & 15)) as usize) & 4095
}

/// BFS по свету. Координаты блоков мировые по x/z, y — от низа мира (0..height).
pub struct Engine<'a, A: LightAccess> {
    access: &'a mut A,
    table: &'a LightTable,
    height: i32,
    inc: VecDeque<[i32; 3]>,
    dec: VecDeque<([i32; 3], u8)>,
}

impl<'a, A: LightAccess> Engine<'a, A> {
    pub fn new(access: &'a mut A, table: &'a LightTable, height: usize) -> Self {
        Self {
            access,
            table,
            height: height as i32,
            inc: VecDeque::new(),
            dec: VecDeque::new(),
        }
    }

    /// Свет в точке; `None` — столбца нет. Над миром небо = 15, под миром — 0.
    fn get(&mut self, kind: Kind, p: [i32; 3]) -> Option<u8> {
        if p[1] >= self.height {
            return Some(if kind == Kind::Sky { 15 } else { 0 });
        }
        if p[1] < 0 {
            return Some(0);
        }
        let c = self.access.column(p[0] >> 4, p[2] >> 4)?;
        Some(c.light(kind)[(p[1] >> 4) as usize].get(index(p)))
    }

    fn set(&mut self, kind: Kind, p: [i32; 3], v: u8) {
        if p[1] < 0 || p[1] >= self.height || self.get(kind, p) == Some(v) {
            return;
        }
        if let Some(c) = self.access.column_mut(p[0] >> 4, p[2] >> 4) {
            c.light_mut(kind)[(p[1] >> 4) as usize].set(index(p), v);
        }
    }

    /// Состояние блока; вне мира по высоте — `None`.
    fn state(&mut self, p: [i32; 3]) -> Option<u32> {
        if p[1] < 0 || p[1] >= self.height {
            return None;
        }
        let c = self.access.column(p[0] >> 4, p[2] >> 4)?;
        Some(c.sections[(p[1] >> 4) as usize].get(index(p)))
    }

    fn has_sky(&mut self, p: [i32; 3]) -> bool {
        self.access
            .column(p[0] >> 4, p[2] >> 4)
            .is_some_and(|c| c.has_skylight)
    }

    /// Блок в `p` уже заменён: пересчитать оба вида света вокруг него.
    pub fn block_changed(&mut self, p: [i32; 3]) {
        if self.has_sky(p) {
            self.relight(Kind::Sky, p);
        }
        self.relight(Kind::Block, p);
    }

    fn relight(&mut self, kind: Kind, p: [i32; 3]) {
        let Some(state) = self.state(p) else { return };
        let cur = self.get(kind, p).unwrap_or(0);
        if cur > 0 {
            self.set(kind, p, 0);
            self.dec.push_back((p, cur));
        }
        self.decrease(kind);
        if kind == Kind::Block {
            let e = self.table.emission(state);
            if e > 0 {
                self.set(kind, p, e);
                self.inc.push_back(p);
            }
        }
        // Свет соседей заново втекает в изменённый блок и дальше.
        for d in DIRS {
            let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            if self.get(kind, n).unwrap_or(0) > 0 {
                self.inc.push_back(n);
            }
        }
        self.increase(kind);
    }

    /// Снятие света, который шёл от уже снятых клеток; остальной — в очередь роста.
    fn decrease(&mut self, kind: Kind) {
        while let Some((q, lvl)) = self.dec.pop_front() {
            for (dir, d) in DIRS.iter().enumerate() {
                let n = [q[0] + d[0], q[1] + d[1], q[2] + d[2]];
                if n[1] < 0 || n[1] >= self.height {
                    if n[1] >= self.height && kind == Kind::Sky {
                        self.inc.push_back(n);
                    }
                    continue;
                }
                let Some(nl) = self.get(kind, n) else {
                    continue;
                };
                if nl == 0 {
                    continue;
                }
                let sky_column = kind == Kind::Sky && dir == DOWN && lvl == 15 && nl == 15;
                if nl < lvl || sky_column {
                    self.set(kind, n, 0);
                    self.dec.push_back((n, nl));
                    if kind == Kind::Block {
                        let e = self.state(n).map_or(0, |s| self.table.emission(s));
                        if e > 0 {
                            self.set(kind, n, e);
                            self.inc.push_back(n);
                        }
                    }
                } else {
                    self.inc.push_back(n);
                }
            }
        }
    }

    fn increase(&mut self, kind: Kind) {
        while let Some(q) = self.inc.pop_front() {
            let Some(lvl) = self.get(kind, q) else {
                continue;
            };
            if lvl <= 1 {
                continue;
            }
            for (dir, d) in DIRS.iter().enumerate() {
                let n = [q[0] + d[0], q[1] + d[1], q[2] + d[2]];
                let Some(state) = self.state(n) else { continue };
                let Some(nl) = self.get(kind, n) else {
                    continue;
                };
                let new = if kind == Kind::Sky
                    && dir == DOWN
                    && lvl == 15
                    && self.table.sky_down(state)
                {
                    15
                } else {
                    lvl.saturating_sub(self.table.dampening(state).max(1))
                };
                if new > nl {
                    self.set(kind, n, new);
                    self.inc.push_back(n);
                }
            }
        }
    }
}

/// Один столбец без соседей (полный пересчёт).
struct Single<'a>(&'a mut ChunkColumn);

impl LightAccess for Single<'_> {
    fn column(&mut self, cx: i32, cz: i32) -> Option<&ChunkColumn> {
        (cx == 0 && cz == 0).then_some(&*self.0)
    }
    fn column_mut(&mut self, cx: i32, cz: i32) -> Option<&mut ChunkColumn> {
        (cx == 0 && cz == 0).then_some(&mut *self.0)
    }
}

/// Полный свет столбца по его собственным блокам (соседи не учитываются).
pub fn relight_column(c: &mut ChunkColumn, table: &LightTable) {
    let height = c.height() as i32;
    let n = c.sections.len();
    c.sky_light = vec![LightLayer::Uniform(0); n];
    c.block_light = vec![LightLayer::Uniform(0); n];
    let has_sky = c.has_skylight;
    let mut access = Single(c);
    let mut e = Engine::new(&mut access, table, height as usize);

    if has_sky {
        // Небо сверху вниз до первого блока, который его задерживает;
        // в очередь — клетки на границе тени (дальше идёт BFS).
        let mut top = [[0i32; 16]; 16];
        for (x, row) in top.iter_mut().enumerate() {
            for (z, h) in row.iter_mut().enumerate() {
                let mut y = height;
                while y > 0 {
                    let s = e.state([x as i32, y - 1, z as i32]).unwrap_or(0);
                    if !table.sky_down(s) {
                        break;
                    }
                    y -= 1;
                }
                *h = y;
            }
        }
        // Секции целиком над рельефом — сразу 15, без поклеточной записи.
        let lowest = top.iter().flatten().copied().max().unwrap_or(0);
        let first_open = ((lowest + 15) / 16) as usize;
        for layer in &mut e.access.0.sky_light[first_open.min(n)..] {
            *layer = LightLayer::Uniform(15);
        }
        for x in 0..16i32 {
            for z in 0..16i32 {
                let h = top[x as usize][z as usize];
                for y in h..(first_open as i32 * 16).min(height) {
                    e.set(Kind::Sky, [x, y, z], 15);
                }
                // Граница: самая нижняя светлая клетка и клетки рядом с более
                // глубокой тенью у соседних столбцов.
                let mut edge = h;
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let (nx, nz) = (x + dx, z + dz);
                    if (0..16).contains(&nx) && (0..16).contains(&nz) {
                        edge = edge.max(top[nx as usize][nz as usize]);
                    }
                }
                for y in h..edge.min(height).max(h + 1) {
                    if y < height {
                        e.inc.push_back([x, y, z]);
                    }
                }
            }
        }
        e.increase(Kind::Sky);
    }

    for si in 0..n {
        let sec = &e.access.0.sections[si];
        if sec.is_uniform() && table.emission(sec.get(0)) == 0 {
            continue;
        }
        for i in 0..4096i32 {
            let p = [i & 15, si as i32 * 16 + (i >> 8), (i >> 4) & 15];
            let em = e.state(p).map_or(0, |s| table.emission(s));
            if em > 0 {
                e.set(Kind::Block, p, em);
                e.inc.push_back(p);
            }
        }
    }
    e.increase(Kind::Block);

    for l in c.sky_light.iter_mut().chain(c.block_light.iter_mut()) {
        l.compact();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0 = воздух, 1 = камень, 2 = факел (14), 3 = стекло.
    fn table() -> LightTable {
        LightTable::new([(0, 0, true), (0, 15, false), (14, 0, true), (0, 0, true)])
    }

    fn flat() -> ChunkColumn {
        // Камень 4 слоя, высота 4 секции.
        ChunkColumn::flat(&[(1, 4)], 4, 0, 2, 0, true)
    }

    #[test]
    fn sky_and_torch_in_one_column() {
        let t = table();
        let mut c = flat();
        relight_column(&mut c, &t);
        assert_eq!(c.light(Kind::Sky)[0].get(index([0, 4, 0])), 15);
        assert_eq!(c.light(Kind::Sky)[0].get(index([0, 3, 0])), 0);
        assert_eq!(c.light(Kind::Sky)[3], LightLayer::Uniform(15));

        c.set_block(5, 4, 5, 2);
        relight_column(&mut c, &t);
        assert_eq!(c.light(Kind::Block)[0].get(index([5, 4, 5])), 14);
        assert_eq!(c.light(Kind::Block)[0].get(index([5, 5, 7])), 11);
    }

    struct World(std::collections::HashMap<(i32, i32), ChunkColumn>);
    impl LightAccess for World {
        fn column(&mut self, cx: i32, cz: i32) -> Option<&ChunkColumn> {
            self.0.get(&(cx, cz))
        }
        fn column_mut(&mut self, cx: i32, cz: i32) -> Option<&mut ChunkColumn> {
            self.0.get_mut(&(cx, cz))
        }
    }

    fn light(w: &mut World, kind: Kind, p: [i32; 3]) -> u8 {
        w.0[&(p[0] >> 4, p[2] >> 4)].light(kind)[(p[1] >> 4) as usize].get(index(p))
    }

    #[test]
    fn incremental_matches_full_and_crosses_chunks() {
        let t = table();
        let mut base = flat();
        relight_column(&mut base, &t);
        let mut w = World(
            [(0, 0), (1, 0)]
                .into_iter()
                .map(|k| (k, base.clone()))
                .collect(),
        );

        // Факел у границы: свет переходит в соседний чанк.
        let torch = [15, 4, 8];
        w.0.get_mut(&(0, 0)).unwrap().set_block(15, 4, 8, 2);
        Engine::new(&mut w, &t, 64).block_changed(torch);
        assert_eq!(light(&mut w, Kind::Block, torch), 14);
        assert_eq!(light(&mut w, Kind::Block, [16, 4, 8]), 13);
        assert_eq!(light(&mut w, Kind::Block, [20, 4, 8]), 9);

        // Убрали факел — блочного света нет нигде.
        w.0.get_mut(&(0, 0)).unwrap().set_block(15, 4, 8, 0);
        Engine::new(&mut w, &t, 64).block_changed(torch);
        assert_eq!(light(&mut w, Kind::Block, [16, 4, 8]), 0);
        assert!(
            w.0.values()
                .all(|c| c.light(Kind::Block).iter().all(LightLayer::is_zero))
        );

        // Крыша над ямой: под ней тень, без крыши — снова небо.
        w.0.get_mut(&(0, 0)).unwrap().set_block(3, 3, 3, 0);
        Engine::new(&mut w, &t, 64).block_changed([3, 3, 3]);
        assert_eq!(light(&mut w, Kind::Sky, [3, 3, 3]), 15);
        w.0.get_mut(&(0, 0)).unwrap().set_block(3, 10, 3, 1);
        Engine::new(&mut w, &t, 64).block_changed([3, 10, 3]);
        assert_eq!(light(&mut w, Kind::Sky, [3, 9, 3]), 14);
        assert_eq!(light(&mut w, Kind::Sky, [3, 3, 3]), 13);
        w.0.get_mut(&(0, 0)).unwrap().set_block(3, 10, 3, 3);
        Engine::new(&mut w, &t, 64).block_changed([3, 10, 3]);
        assert_eq!(light(&mut w, Kind::Sky, [3, 3, 3]), 15);
    }

    /// Три фонаря (15) в закрытой пещере на стыке четырёх чанков, потом сломаны:
    /// блочного света не остаётся нигде.
    #[test]
    fn lanterns_removed_at_chunk_corner() {
        let t = LightTable::new([(0, 0, true), (0, 15, false), (15, 0, true)]);
        let mut base = ChunkColumn::flat(&[(1, 30)], 4, 0, 2, 0, true);
        relight_column(&mut base, &t);
        let keys = [(-1, -1), (-1, 0), (0, -1), (0, 0)];
        let mut w = World(keys.into_iter().map(|k| (k, base.clone())).collect());
        let put = |w: &mut World, p: [i32; 3], s: u32| {
            w.0.get_mut(&(p[0] >> 4, p[2] >> 4)).unwrap().set_block(
                (p[0] & 15) as usize,
                p[1] as usize,
                (p[2] & 15) as usize,
                s,
            );
            Engine::new(w, &t, 64).block_changed(p);
        };
        let cave: Vec<[i32; 3]> = (-3..3)
            .flat_map(|x| (-3..3).flat_map(move |z| (20..23).map(move |y| [x, y, z])))
            .collect();
        for &p in &cave {
            put(&mut w, p, 0);
        }
        let lanterns = [[-2, 20, -2], [1, 20, 1], [-1, 21, 0]];
        for p in lanterns {
            put(&mut w, p, 2);
        }
        assert_eq!(light(&mut w, Kind::Block, [0, 22, 0]), 13);
        for p in lanterns {
            put(&mut w, p, 0);
        }
        for &p in &cave {
            assert_eq!(light(&mut w, Kind::Block, p), 0, "{p:?}");
            assert_eq!(light(&mut w, Kind::Sky, p), 0, "{p:?}");
        }
    }

    /// Случайные раскопки, крыши и факелы: инкрементальный свет после каждого
    /// шага совпадает с полным пересчётом.
    #[test]
    fn random_edits_match_full_relight() {
        let t = table();
        let mut c = ChunkColumn::flat(&[(1, 24)], 4, 0, 2, 0, true);
        relight_column(&mut c, &t);
        let mut seed = 12345u64;
        let mut rnd = |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        for step in 0..3000 {
            let p = [rnd(16) as i32, rnd(30) as i32, rnd(16) as i32];
            let state = [0, 0, 1, 1, 2, 3][rnd(6) as usize];
            c.set_block(p[0] as usize, p[1] as usize, p[2] as usize, state);
            Engine::new(&mut Single(&mut c), &t, 64).block_changed(p);
            let mut full = c.clone();
            relight_column(&mut full, &t);
            for kind in [Kind::Sky, Kind::Block] {
                for y in 0..64 {
                    for i in 0..256 {
                        let q = [i & 15, y, i >> 4];
                        let (a, b) = (
                            c.light(kind)[(y >> 4) as usize].get(index(q)),
                            full.light(kind)[(y >> 4) as usize].get(index(q)),
                        );
                        assert_eq!(a, b, "step {step} {kind:?} at {q:?} after {state} at {p:?}");
                    }
                }
            }
        }
    }
}
