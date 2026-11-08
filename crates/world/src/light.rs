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

    /// То же из `ByteArray` NBT: байты там знаковые, а раскладка нибблов та же.
    ///
    /// Ради этого раньше делали `iter().map(..).collect::<Vec<u8>>()` — на чанк
    /// два слоя по 2048 байт, то есть до 48 аллокаций на регион.
    pub fn from_i8(b: &[i8]) -> Option<Self> {
        if b.len() != 2048 {
            return None;
        }
        let mut bytes = [0u8; 2048];
        for (dst, &src) in bytes.iter_mut().zip(b) {
            *dst = src as u8;
        }
        let mut layer = LightLayer::Full(Arc::new(bytes));
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
        // Секций у столбца может быть меньше, чем высота движка: у измерения
        // своя высота (Ад — 128, оверворлд — 384), а столбец приходит из того
        // измерения, где он лежит. Без этой проверки был бы выход за границу.
        let section = c.light(kind).get((p[1] >> 4) as usize)?;
        Some(section.get(index(p)))
    }

    fn set(&mut self, kind: Kind, p: [i32; 3], v: u8) {
        if p[1] < 0 || p[1] >= self.height || self.get(kind, p) == Some(v) {
            return;
        }
        if let Some(c) = self.access.column_mut(p[0] >> 4, p[2] >> 4)
            && let Some(section) = c.light_mut(kind).get_mut((p[1] >> 4) as usize)
        {
            section.set(index(p), v);
        }
    }

    /// Запись, когда текущее значение уже известно вызывающему: `relight`
    /// только что прочитал его тем же `cell`, и второй поиск столбца лишний.
    fn put_known(&mut self, kind: Kind, p: [i32; 3], v: u8) {
        if let Some(c) = self.access.column_mut(p[0] >> 4, p[2] >> 4)
            && let Some(section) = c.light_mut(kind).get_mut((p[1] >> 4) as usize)
        {
            section.set(index(p), v);
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
        // Высота движка и число секций столбца могут разойтись: у измерения
        // своя высота. Тогда клетки просто нет, а не паника.
        let section = c.sections.get(si)?;
        let light = c.light(kind).get(si)?;
        Some((section.get(i), light.get(i)))
    }

    /// Запись ниббла, который уже отличается от текущего. Столбец создаётся только здесь.
    ///
    /// Оставлено для путей, где текущее значение неизвестно; BFS пишет через
    /// [`Self::put_known`], потому что уровень только что прочитан `cell`.
    #[allow(dead_code)]
    fn write_changed(&mut self, kind: Kind, p: [i32; 3], v: u8) {
        if let Some(c) = self.access.column_mut(p[0] >> 4, p[2] >> 4)
            && let Some(section) = c.light_mut(kind).get_mut((p[1] >> 4) as usize)
        {
            section.set(index(p), v);
        }
    }

    /// Состояние блока; вне мира по высоте — `None`.
    fn state(&mut self, p: [i32; 3]) -> Option<u32> {
        if p[1] < 0 || p[1] >= self.height {
            return None;
        }
        let c = self.access.column(p[0] >> 4, p[2] >> 4)?;
        // Секций может быть меньше высоты движка: у измерения своя высота.
        c.sections
            .get((p[1] >> 4) as usize)
            .map(|s| s.get(index(p)))
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

    /// Блоки в `ps` уже заменены: пересчёт одним проходом на все сразу.
    ///
    /// Гашение от всех изменённых клеток идёт одной волной, рост — тоже:
    /// для `/fill` это один BFS вместо BFS на каждый блок. Итог тот же, что
    /// у [`Self::block_changed`] по очереди (тест `batch_matches_sequential`).
    pub fn blocks_changed(&mut self, ps: &[[i32; 3]]) {
        let Some(&first) = ps.first() else { return };
        if self.has_sky(first) {
            self.relight_many(Kind::Sky, ps);
        }
        self.relight_many(Kind::Block, ps);
    }

    fn relight_many(&mut self, kind: Kind, ps: &[[i32; 3]]) {
        for &p in ps {
            if self.state(p).is_none() {
                continue;
            }
            let cur = self.get(kind, p).unwrap_or(0);
            if cur > 0 {
                self.set(kind, p, 0);
                self.dec.push_back((p, cur));
            }
        }
        self.decrease(kind);
        for &p in ps {
            let Some(state) = self.state(p) else { continue };
            if kind == Kind::Block {
                let e = self.table.emission(state);
                if e > 0 {
                    self.set(kind, p, e);
                    self.inc.push_back(p);
                }
            }
            for d in DIRS {
                let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
                if self.get(kind, n).unwrap_or(0) > 0 {
                    self.inc.push_back(n);
                }
            }
        }
        self.increase(kind);
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
                    // Уровень `nl` уже прочитан `cell` — второй поиск столбца не нужен.
                    self.put_known(kind, n, 0);
                    self.dec.push_back((n, nl));
                    if kind == Kind::Block {
                        let e = self.table.emission(state);
                        if e > 0 {
                            self.put_known(kind, n, e);
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
                    self.put_known(kind, n, new);
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
    // `e` держит `&mut access`, а тот — `&mut c`; ниже нужен сам `c`.
    // Оба типа не реализуют `Drop`, поэтому заимствования заканчиваются
    // здесь по NLL, и вызов `drop` только продлевал бы их.

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

/// Столбцы пачки в плотной сетке: BFS трогает столбец на каждой клетке, и
/// хэш-таблица на этом пути съедала большую часть времени света.
struct Area<'a> {
    x0: i32,
    z0: i32,
    w: i32,
    d: i32,
    cells: Vec<Option<(i32, i32, &'a mut ChunkColumn)>>,
    touched: Vec<bool>,
}

impl<'a> Area<'a> {
    fn new(
        columns: &'a mut HashMap<(i32, i32), ChunkColumn>,
        origins: impl IntoIterator<Item = (i32, i32)>,
    ) -> Self {
        let (mut x0, mut z0, mut x1, mut z1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for &(x, z) in columns.keys() {
            (x0, z0, x1, z1) = (x0.min(x), z0.min(z), x1.max(x), z1.max(z));
        }
        let (w, d) = if columns.is_empty() {
            (0, 0)
        } else {
            (x1 - x0 + 1, z1 - z0 + 1)
        };
        let mut cells: Vec<Option<(i32, i32, &mut ChunkColumn)>> =
            (0..w * d).map(|_| None).collect();
        for (&(x, z), c) in columns.iter_mut() {
            cells[((z - z0) * w + (x - x0)) as usize] = Some((x, z, c));
        }
        let mut area = Self {
            x0,
            z0,
            w,
            d,
            touched: vec![false; cells.len()],
            cells,
        };
        for (x, z) in origins {
            if let Some(i) = area.slot(x, z) {
                area.touched[i] = true;
            }
        }
        area
    }

    fn slot(&self, cx: i32, cz: i32) -> Option<usize> {
        let (x, z) = (cx - self.x0, cz - self.z0);
        // Чанк вне прямоугольника области — самый частый промах (BFS смотрит
        // за границу края мира): сравнения дешевле умножения и `Vec::get`.
        if x < 0 || z < 0 || x >= self.w || z >= self.d {
            return None;
        }
        Some((z * self.w + x) as usize)
    }

    /// Столбцы, где свет записан; их слои ужимаются обратно.
    fn finish(self) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        for (cell, touched) in self.cells.into_iter().zip(self.touched) {
            if let (Some((x, z, c)), true) = (cell, touched) {
                // `compact` только у полных слоёв: у `Uniform` ужимать нечего,
                // а проверок на столбец здесь сотни.
                for l in c.sky_light.iter_mut().chain(c.block_light.iter_mut()) {
                    if matches!(l, LightLayer::Full(_)) {
                        l.compact();
                    }
                }
                out.push((x, z));
            }
        }
        out
    }
}

impl LightAccess for Area<'_> {
    fn column(&mut self, cx: i32, cz: i32) -> Option<&ChunkColumn> {
        let i = self.slot(cx, cz)?;
        self.cells[i].as_ref().map(|c| &*c.2)
    }
    fn column_mut(&mut self, cx: i32, cz: i32) -> Option<&mut ChunkColumn> {
        let i = self.slot(cx, cz)?;
        let c = self.cells[i].as_mut()?;
        self.touched[i] = true;
        Some(&mut *c.2)
    }
}

/// Свет пачки новых чанков сразу: `origins` сбрасываются, небо и излучатели
/// всех сразу ставятся в очередь, и один пролив доводит свет до соседей.
///
/// Отличие от [`spread_chunk`] по одному чанку принципиально: если два
/// соседних новых чанка проливать по очереди, первый видит у второго нулевой
/// свет, а второй потом не пересчитывает первого — на границе остаётся тёмный
/// шов. Здесь оба видят друг друга до пролива, поэтому шва нет.
/// Возвращает чанки, где свет изменился (свои плюс затронутые соседи).
pub fn spread_wave(
    columns: &mut HashMap<(i32, i32), ChunkColumn>,
    origins: &HashSet<(i32, i32)>,
    table: &LightTable,
) -> Vec<(i32, i32)> {
    let mut prepared = Vec::new();
    for &pos in origins {
        let Some(col) = columns.get_mut(&pos) else {
            continue;
        };
        let n = col.sections.len();
        col.sky_light = vec![LightLayer::Uniform(0); n];
        col.block_light = vec![LightLayer::Uniform(0); n];
        let tops = col.has_skylight.then(|| column_sky_top(col, table));
        prepared.push((pos, tops, col.height() as i32, n));
    }
    if prepared.is_empty() {
        return Vec::new();
    }
    let height = prepared[0].2;
    let mut area = Area::new(columns, prepared.iter().map(|p| p.0));
    let mut e = Engine::new(&mut area, table, height as usize);
    // Небо всех новых чанков — до пролива: иначе первый в списке «не видит»
    // остальные и затеняет их грани.
    for (pos, tops, h, n) in &prepared {
        if let Some(tops) = tops {
            paint_sky(&mut e, tops, pos.0, pos.1, *h, *n);
        }
    }
    for (pos, tops, h, _) in &prepared {
        if tops.is_some() {
            queue_faces(&mut e, Kind::Sky, *pos, *h);
        }
    }
    e.increase(Kind::Sky);
    for (pos, _, _, _) in &prepared {
        queue_emitters(&mut e, *pos, table);
    }
    for (pos, _, h, _) in &prepared {
        queue_faces(&mut e, Kind::Block, *pos, *h);
    }
    e.increase(Kind::Block);
    drop(e);
    area.finish()
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
    let mut area = Area::new(columns, [origin]);
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
    area.finish()
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

    /// Два чанка с пещерой через общую границу и факелом у самой границы.
    /// Факел в чанке 0, его свет должен уйти в чанк 1.
    fn seam_setup(t: &LightTable) -> HashMap<(i32, i32), ChunkColumn> {
        let mut map = HashMap::new();
        for cx in 0..2i32 {
            let mut c = ChunkColumn::flat(&[(1, 64)], 4, 0, 2, 0, true);
            // Тоннель y=10..=11 вдоль z=8, пересекающий границу x=15/16.
            let xs: Vec<usize> = if cx == 0 {
                (12..16).collect()
            } else {
                (0..4).collect()
            };
            for x in xs {
                for y in 10..12 {
                    c.set_block(x, y, 8, 0);
                }
            }
            relight_column(&mut c, t);
            map.insert((cx, 0), c);
        }
        // Факел в дальнем конце тоннеля чанка 0.
        map.get_mut(&(0, 0)).unwrap().set_block(12, 10, 8, 2);
        relight_column(map.get_mut(&(0, 0)).unwrap(), t);
        map
    }

    /// Эталон: свет всех столбцов разом, как посчитанный с нуля.
    fn reference(
        map: &HashMap<(i32, i32), ChunkColumn>,
        t: &LightTable,
    ) -> HashMap<(i32, i32), ChunkColumn> {
        let mut ref_map = map.clone();
        for c in ref_map.values_mut() {
            let n = c.sections.len();
            c.sky_light = vec![LightLayer::Uniform(0); n];
            c.block_light = vec![LightLayer::Uniform(0); n];
        }
        let origins: HashSet<(i32, i32)> = ref_map.keys().copied().collect();
        spread_wave(&mut ref_map, &origins, t);
        ref_map
    }

    fn light_at(map: &HashMap<(i32, i32), ChunkColumn>, kind: Kind, p: [i32; 3]) -> u8 {
        map[&(p[0] >> 4, p[2] >> 4)].light(kind)[(p[1] >> 4) as usize].get(index(p))
    }

    /// Волна по пачке чанков совпадает с эталоном, и свет реально переходит
    /// границу — то есть соседи видят друг друга до пролива.
    #[test]
    fn wave_lights_neighbouring_chunks_without_a_seam() {
        let t = table();
        let mut map = seam_setup(&t);
        let want = reference(&map, &t);
        let origins: HashSet<(i32, i32)> = map.keys().copied().collect();
        spread_wave(&mut map, &origins, &t);
        for (pos, got) in &map {
            let exp = &want[pos];
            for kind in [Kind::Sky, Kind::Block] {
                for y in 0..got.height() as i32 {
                    for i in 0..256i32 {
                        let p = [pos.0 * 16 + (i & 15), y, pos.1 * 16 + (i >> 4)];
                        assert_eq!(
                            light_at(&map, kind, p),
                            light_at(&want, kind, p),
                            "{kind:?} at {p:?}"
                        );
                    }
                }
            }
            let _ = exp;
        }
        // Свет прошёл границу: иначе тест прошёл бы на нулях.
        assert_eq!(light_at(&map, Kind::Block, [12, 10, 8]), 14);
        assert_eq!(light_at(&map, Kind::Block, [15, 10, 8]), 11);
        assert_eq!(
            light_at(&map, Kind::Block, [16, 10, 8]),
            10,
            "block light did not cross the border"
        );
    }

    /// Настоящая причина тёмных швов: `spread_commit` записывал свет в соседа
    /// только если ревизия соседа не успела измениться. Если сосед посчитал
    /// свой свет, пока этот ещё не был освещён, запись отбрасывалась и не
    /// повторялась — на границе навсегда оставалась тень. Здесь этот отброс
    /// воспроизводится явно.
    #[test]
    fn dropped_write_leaves_a_dark_seam() {
        let t = table();
        let base = seam_setup(&t);
        let want = reference(&base, &t);

        // Чанк 1 светится, пока чанк 0 ещё тёмный.
        let mut race = base.clone();
        for pos in [(0, 0), (1, 0)] {
            let c = race.get_mut(&pos).unwrap();
            let n = c.sections.len();
            c.sky_light = vec![LightLayer::Uniform(0); n];
            c.block_light = vec![LightLayer::Uniform(0); n];
        }
        spread_chunk(&mut race, (1, 0), &t);
        // Чанк 0 светится после и пишет в соседа, но ревизия чанка 1 уже
        // изменилась, поэтому запись отбрасывается — как в `commit_light`.
        let touched = spread_chunk(&mut race, (0, 0), &t);
        assert!(
            touched.contains(&(1, 0)),
            "пролив должен был дойти до соседа"
        );
        let dropped = base.clone();
        race.insert((1, 0), dropped[&(1, 0)].clone());

        let seam = light_at(&race, Kind::Block, [16, 10, 8]);
        let proper = light_at(&want, Kind::Block, [16, 10, 8]);
        assert!(
            seam < proper,
            "отброшенная запись обязана дать тёмный шов: {seam} против {proper}"
        );
    }
    fn light(w: &mut World, kind: Kind, p: [i32; 3]) -> u8 {
        // Клетки выше столбца нет — как и в движке, отвечаем нулём.
        w.0[&(p[0] >> 4, p[2] >> 4)]
            .light(kind)
            .get((p[1] >> 4) as usize)
            .map_or(0, |section| section.get(index(p)))
    }

    /// Движок выше столбца: у измерения своя высота (Ад — 128, оверворлд —
    /// 384), и столбец может прийти из измерения поменьше. Именно так падало
    /// в BlueMap: `state` читал секцию 8 при длине 8.
    #[test]
    fn engine_taller_than_column_does_not_panic() {
        let t = table();
        // Столбец Ада: 8 секций, 128 блоков.
        let mut short = ChunkColumn::flat(&[(1, 128)], 8, 0, 4, 4, false);
        relight_column(&mut short, &t);
        assert_eq!(short.sections.len(), 8);
        let mut w = World(
            [(0, 0)]
                .into_iter()
                .map(|_| ((0, 0), short.clone()))
                .collect(),
        );
        // Движок знает высоту оверворлда (384) — вдвое больше секций столбца.
        let mut e = Engine::new(&mut w, &t, 384);
        // Клетка выше столбца: секции 8 нет, должен быть `None`, а не паника.
        assert!(e.state([8, 130, 8]).is_none());
        assert_eq!(e.get(Kind::Block, [8, 130, 8]), None);
        assert!(e.cell(Kind::Block, [8, 130, 8]).is_none());
        // А внутри столбца всё работает как обычно.
        assert!(e.state([8, 40, 8]).is_some());
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

    /// Пачка правок (как `/fill` и взрыв) пересчитанная одним проходом даёт
    /// тот же свет, что полный пересчёт столбца.
    #[test]
    fn batch_matches_full_relight() {
        let t = table();
        let mut c = ChunkColumn::flat(&[(1, 24)], 4, 0, 2, 0, true);
        relight_column(&mut c, &t);
        let mut seed = 777u64;
        let mut rnd = |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        for step in 0..300 {
            // Пачка: прямоугольник одним состоянием или россыпь разных.
            let mut batch = Vec::new();
            if rnd(2) == 0 {
                let state = [0, 1, 2, 3][rnd(4) as usize];
                let (x0, y0, z0) = (rnd(12) as i32, rnd(28) as i32, rnd(12) as i32);
                let (w, h, d) = (1 + rnd(5) as i32, 1 + rnd(5) as i32, 1 + rnd(5) as i32);
                for x in x0..x0 + w {
                    for y in y0..y0 + h {
                        for z in z0..z0 + d {
                            batch.push(([x, y, z], state));
                        }
                    }
                }
            } else {
                for _ in 0..20 {
                    let p = [rnd(16) as i32, rnd(32) as i32, rnd(16) as i32];
                    batch.push((p, [0, 0, 1, 1, 2, 3][rnd(6) as usize]));
                }
            }
            let mut changed = Vec::new();
            for &(p, state) in &batch {
                if c.set_block(p[0] as usize, p[1] as usize, p[2] as usize, state) != state {
                    changed.push(p);
                }
            }
            Engine::new(&mut Single(&mut c), &t, 64).blocks_changed(&changed);
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
                        assert_eq!(a, b, "step {step} {kind:?} at {q:?}");
                    }
                }
            }
        }
    }
}
