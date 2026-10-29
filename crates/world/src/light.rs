//! Свет: небесный и блочный, по ниббл-массиву на секцию (как `DataLayer`).
//!
//! Изменение блока пересчитывается инкрементально (снятие старого света BFS-ом
//! и распространение нового), стоимость — по размеру изменения, а не мира.
//! Полный пересчёт — только для столбца без сохранённого света.

use crate::ChunkColumn;
use std::collections::{HashMap, HashSet, VecDeque};
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
        let mut out = Vec::with_capacity(2048);
        self.write_bytes(&mut out);
        out
    }

    /// Те же 2048 байт, что у [`Self::bytes`], дописанные в `out`.
    pub fn write_bytes(&self, out: &mut Vec<u8>) {
        match self {
            LightLayer::Uniform(v) => {
                let n = out.len();
                out.resize(n + 2048, v | v << 4);
            }
            LightLayer::Full(a) => out.extend_from_slice(a.as_ref()),
        }
    }

    /// Те же байты со знаком, как NBT `ByteArray` света. Один буфер, без промежуточного `Vec<u8>`.
    pub fn write_i8(&self, out: &mut Vec<i8>) {
        match self {
            LightLayer::Uniform(v) => {
                let n = out.len();
                out.resize(n + 2048, (v | v << 4) as i8);
            }
            LightLayer::Full(a) => out.extend(a.iter().map(|&b| b as i8)),
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

    /// Излучение, затухание и проход неба вниз совпали — поле света от замены не меняется.
    pub fn same_light(&self, a: u32, b: u32) -> bool {
        self.emission(a) == self.emission(b)
            && self.dampening(a) == self.dampening(b)
            && self.sky_down(a) == self.sky_down(b)
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

    /// Состояние и свет клетки внутри мира. Вне высоты и без столбца — `None`.
    fn cell(&mut self, kind: Kind, p: [i32; 3]) -> Option<(u32, u8)> {
        if p[1] < 0 || p[1] >= self.height {
            return None;
        }
        let c = self.access.column(p[0] >> 4, p[2] >> 4)?;
        let si = (p[1] >> 4) as usize;
        let i = index(p);
        Some((c.sections[si].get(i), c.light(kind)[si].get(i)))
    }

    /// Запись ниббла, который уже отличается от текущего. Столбец создаётся только здесь.
    fn write_changed(&mut self, kind: Kind, p: [i32; 3], v: u8) {
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
                let Some((state, nl)) = self.cell(kind, n) else {
                    continue;
                };
                if nl == 0 {
                    continue;
                }
                let sky_column = kind == Kind::Sky && dir == DOWN && lvl == 15 && nl == 15;
                if nl < lvl || sky_column {
                    // Сначала ноль: остальной BFS должен увидеть пустую клетку до излучения.
                    self.write_changed(kind, n, 0);
                    self.dec.push_back((n, nl));
                    if kind == Kind::Block {
                        let e = self.table.emission(state);
                        if e > 0 {
                            self.write_changed(kind, n, e);
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
                let Some((state, nl)) = self.cell(kind, n) else {
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
                    self.write_changed(kind, n, new);
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
    let tops = has_sky.then(|| column_sky_top(c, table));
    let mut access = Single(c);
    let mut e = Engine::new(&mut access, table, height as usize);

    if let Some(tops) = tops {
        // Столбец один и лежит в чанке (0, 0): локальные координаты совпадают с мировыми.
        paint_sky(&mut e, &tops, 0, 0, height, n);
        e.increase(Kind::Sky);
    }

    for si in 0..n {
        let sec = &e.access.0.sections[si];
        if sec.is_uniform() && table.emission(sec.get(0)) == 0 {
            continue;
        }
        for i in 0..4096i32 {
            let state = e.access.0.sections[si].get(i as usize);
            let em = table.emission(state);
            if em > 0 {
                let p = [i & 15, si as i32 * 16 + (i >> 8), (i >> 4) & 15];
                e.set(Kind::Block, p, em);
                e.inc.push_back(p);
            }
        }
    }
    e.increase(Kind::Block);
    drop(e);
    drop(access);

    for l in c.sky_light.iter_mut().chain(c.block_light.iter_mut()) {
        l.compact();
    }
}

/// Высота неба по столбцам 16×16: y первой клетки, куда небо ещё доходит.
fn column_sky_top(col: &ChunkColumn, table: &LightTable) -> [[i32; 16]; 16] {
    let height = col.height() as i32;
    let mut top = [[0i32; 16]; 16];
    for (x, row) in top.iter_mut().enumerate() {
        for (z, h) in row.iter_mut().enumerate() {
            let mut y = height;
            let (x, z) = (x as i32, z as i32);
            while y > 0 {
                let si = ((y - 1) >> 4) as usize;
                let sec = &col.sections[si];
                let bottom = si as i32 * 16;
                if sec.is_uniform() {
                    // Вся секция одного состояния: либо пропустить, либо стоп на верхней клетке.
                    if table.sky_down(sec.get(0)) {
                        y = bottom;
                        continue;
                    }
                    break;
                }
                let mut blocked = false;
                while y > bottom {
                    if !table.sky_down(sec.get(index([x, y - 1, z]))) {
                        blocked = true;
                        break;
                    }
                    y -= 1;
                }
                if blocked {
                    break;
                }
            }
            *h = y;
        }
    }
    top
}

/// Небо чанка `(ox, oz)` и очередь роста. Координаты в очереди мировые.
fn paint_sky<A: LightAccess>(
    e: &mut Engine<A>,
    tops: &[[i32; 16]; 16],
    ox: i32,
    oz: i32,
    height: i32,
    sections: usize,
) {
    let lowest = tops.iter().flatten().copied().max().unwrap_or(0);
    let first_open = ((lowest + 15) / 16) as usize;
    if let Some(col) = e.access.column_mut(ox, oz) {
        for layer in &mut col.sky_light[first_open.min(sections)..] {
            *layer = LightLayer::Uniform(15);
        }
    }
    let base_x = ox * 16;
    let base_z = oz * 16;
    for x in 0..16i32 {
        for z in 0..16i32 {
            let h = tops[x as usize][z as usize];
            for y in h..(first_open as i32 * 16).min(height) {
                e.set(Kind::Sky, [base_x + x, y, base_z + z], 15);
            }
            // Граница: самая нижняя светлая клетка и клетки рядом с более
            // глубокой тенью у соседних столбцов.
            let mut edge = h;
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (nx, nz) = (x + dx, z + dz);
                if (0..16).contains(&nx) && (0..16).contains(&nz) {
                    edge = edge.max(tops[nx as usize][nz as usize]);
                }
            }
            for y in h..edge.min(height).max(h + 1) {
                if y < height {
                    e.inc.push_back([base_x + x, y, base_z + z]);
                }
            }
        }
    }
}

struct Area<'a> {
    columns: &'a mut HashMap<(i32, i32), ChunkColumn>,
    touched: &'a mut HashSet<(i32, i32)>,
}

impl LightAccess for Area<'_> {
    fn column(&mut self, cx: i32, cz: i32) -> Option<&ChunkColumn> {
        self.columns.get(&(cx, cz))
    }
    fn column_mut(&mut self, cx: i32, cz: i32) -> Option<&mut ChunkColumn> {
        self.touched.insert((cx, cz));
        self.columns.get_mut(&(cx, cz))
    }
}

/// Свет нового чанка `origin` и пролив через грань с соседями, которые уже лежат
/// в `columns`. Чужака нет — шаблон не подставляется. Соседей с нуля не гасим.
/// Возвращает чанки, где свет записан.
pub fn spread_chunk(
    columns: &mut HashMap<(i32, i32), ChunkColumn>,
    origin: (i32, i32),
    table: &LightTable,
) -> Vec<(i32, i32)> {
    let Some(col) = columns.get_mut(&origin) else {
        return Vec::new();
    };
    let height = col.height() as i32;
    let n = col.sections.len();
    let has_sky = col.has_skylight;
    col.sky_light = vec![LightLayer::Uniform(0); n];
    col.block_light = vec![LightLayer::Uniform(0); n];
    let tops = has_sky.then(|| column_sky_top(col, table));
    let mut touched = HashSet::from([origin]);
    let mut area = Area {
        columns,
        touched: &mut touched,
    };
    let mut e = Engine::new(&mut area, table, height as usize);
    if let Some(tops) = tops {
        paint_sky(&mut e, &tops, origin.0, origin.1, height, n);
        queue_faces(&mut e, Kind::Sky, origin, height);
        e.increase(Kind::Sky);
    }
    queue_emitters(&mut e, origin, table);
    queue_faces(&mut e, Kind::Block, origin, height);
    e.increase(Kind::Block);
    drop(e);
    drop(area);
    for pos in &touched {
        if let Some(c) = columns.get_mut(pos) {
            for l in c.sky_light.iter_mut().chain(c.block_light.iter_mut()) {
                l.compact();
            }
        }
    }
    touched.into_iter().collect()
}

/// Клетки общей грани соседних чанков, откуда свет может войти в новый.
fn queue_faces<A: LightAccess>(e: &mut Engine<A>, kind: Kind, origin: (i32, i32), height: i32) {
    let (cx, cz) = origin;
    let faces = [
        (cx - 1, cz, cx * 16 - 1, None),
        (cx + 1, cz, cx * 16 + 16, None),
        (cx, cz - 1, cz * 16 - 1, Some(())),
        (cx, cz + 1, cz * 16 + 16, Some(())),
    ];
    for (nx, nz, edge, along_x) in faces {
        if e.access.column(nx, nz).is_none() {
            continue;
        }
        for y in 0..height {
            for i in 0..16 {
                let (x, z) = if along_x.is_none() {
                    (edge, nz * 16 + i)
                } else {
                    (nx * 16 + i, edge)
                };
                if e.get(kind, [x, y, z]).unwrap_or(0) > 1 {
                    e.inc.push_back([x, y, z]);
                }
            }
        }
    }
}

fn queue_emitters<A: LightAccess>(e: &mut Engine<A>, origin: (i32, i32), table: &LightTable) {
    let Some(col) = e.access.column(origin.0, origin.1) else {
        return;
    };
    let mut lights = Vec::new();
    for si in 0..col.sections.len() {
        let sec = &col.sections[si];
        if sec.is_uniform() && table.emission(sec.get(0)) == 0 {
            continue;
        }
        for i in 0..4096i32 {
            let em = table.emission(sec.get(i as usize));
            if em > 0 {
                lights.push((
                    [
                        origin.0 * 16 + (i & 15),
                        si as i32 * 16 + (i >> 8),
                        origin.1 * 16 + ((i >> 4) & 15),
                    ],
                    em,
                ));
            }
        }
    }
    for (p, em) in lights {
        e.set(Kind::Block, p, em);
        e.inc.push_back(p);
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
    fn same_light_ignores_shape() {
        let t = table();
        assert!(t.same_light(0, 3));
        assert!(!t.same_light(0, 1));
        assert!(!t.same_light(0, 2));
        assert!(t.same_light(9, 9));
    }

    #[test]
    fn light_bytes_match_direct_write() {
        let uniform = LightLayer::Uniform(15);
        let mut buf = Vec::new();
        uniform.write_bytes(&mut buf);
        assert_eq!(buf, vec![0xff; 2048]);
        let mut full = LightLayer::Full(Arc::new([0; 2048]));
        full.set(3, 9);
        let mut buf = Vec::new();
        full.write_bytes(&mut buf);
        let mut expect = vec![0u8; 2048];
        expect[1] = 9 << 4;
        assert_eq!(buf, expect);
        let mut signed = Vec::new();
        full.write_i8(&mut signed);
        assert_eq!(signed, expect.iter().map(|&b| b as i8).collect::<Vec<_>>());
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

    #[test]
    fn spread_carries_sky_across_the_border() {
        let t = table();
        let mut open = ChunkColumn::flat(&[], 4, 0, 2, 0, true);
        relight_column(&mut open, &t);
        let mut cave = ChunkColumn::flat(&[(1, 64)], 4, 0, 2, 0, true);
        cave.set_block(0, 10, 8, 0);
        let mut map = HashMap::new();
        map.insert((0, 0), open);
        map.insert((1, 0), cave);
        spread_chunk(&mut map, (1, 0), &t);
        let got = map[&(1, 0)].light(Kind::Sky)[0].get(index([0, 10, 8]));
        assert!(got > 0, "sky {got} did not cross the border");
        assert_eq!(map[&(0, 0)].light(Kind::Sky)[0].get(index([15, 10, 8])), 15);
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
