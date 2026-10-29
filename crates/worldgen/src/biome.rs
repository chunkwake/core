//! Мультишумный источник биомов (`MultiNoiseBiomeSource`, `Climate.ParameterList`).
//!
//! Климат — шесть осей (`temperature`, `humidity`, `continentalness`, `erosion`,
//! `depth`, `weirdness`), каждая точка `ParameterPoint` задаёт диапазон на каждой
//! оси. Биом выбирается по минимальному «штрафу»: сумма квадратов расстояний до
//! диапазонов. Оси квантуются как `Climate.quantizeCoord` (float * 10000 в long),
//! поэтому сравнение целочисленное и совпадает с ванилью бит в бит.

use std::collections::HashMap;

/// Точек в файле ванили около 7600; при равенстве штрафа побеждает первая,
/// как в `findValueBruteForce` (строгое `<`).
#[derive(Debug, Clone)]
pub struct Point {
    pub name: String,
    /// `[min, max]` по осям в порядке [`AXES`], уже квантованные.
    pub ranges: [[i64; 2]; 6],
}

/// Порядок осей в `Climate.TargetPoint`.
pub const AXES: [&str; 6] = [
    "temperature",
    "humidity",
    "continentalness",
    "erosion",
    "depth",
    "weirdness",
];

/// `Climate.quantizeCoord`: float * 10000 с отбрасыванием дробной части.
pub fn quantize(v: f32) -> i64 {
    (v * 10000.0) as i64
}

#[derive(Debug, Clone, Default)]
pub struct BiomeSource {
    points: Vec<Point>,
    /// Имя биома → сетевой ID; задаёт сервер из своего реестра.
    ids: HashMap<String, u32>,
    /// Поиск ближайшей точки с отсечением вместо перебора всех 7594.
    index: Index,
}

/// Узел k-d дерева по шести осям климата: лист хранит индексы точек,
/// внутренний узел — разделение по самой широкой оси.
///
/// Поиск ближайшей точки идёт с отсечением: для узла считается нижняя оценка
/// штрафа (сумма квадратов расстояний до его рамки), и узел пропускается, если
/// оценка уже не меньше найденной. Это то, что делает RTree у ванили; результат
/// тот же — минимальный штраф, при равенстве первая точка файла.
#[derive(Debug, Clone, Default)]
struct Index {
    /// Рамки узлов: по 6 пар `[min, max]`.
    boxes: Vec<[[i64; 2]; 6]>,
    /// Дети внутреннего узла; у листа пусто.
    children: Vec<[usize; 2]>,
    /// Точки листа (индексы в `BiomeSource::points`); у внутреннего пусто.
    leaves: Vec<Vec<usize>>,
    root: usize,
}

/// Лист до 16 точек: глубже дерево не нужно, перебор дешевле спуска.
const LEAF: usize = 16;

impl Index {
    fn build(points: &[Point]) -> Self {
        let mut index = Self::default();
        let all: Vec<usize> = (0..points.len()).collect();
        index.root = index.node(points, &all);
        index
    }

    fn bounds(points: &[Point], ids: &[usize]) -> [[i64; 2]; 6] {
        let mut b = [[i64::MAX, i64::MIN]; 6];
        for &id in ids {
            for (axis, range) in points[id].ranges.iter().enumerate() {
                b[axis][0] = b[axis][0].min(range[0]);
                b[axis][1] = b[axis][1].max(range[1]);
            }
        }
        b
    }

    fn node(&mut self, points: &[Point], ids: &[usize]) -> usize {
        let at = self.boxes.len();
        self.boxes.push(Self::bounds(points, ids));
        self.children.push([0, 0]);
        if ids.len() <= LEAF {
            self.leaves.push(ids.to_vec());
            return at;
        }
        self.leaves.push(Vec::new());
        // Самая широкая ось рамки — по ней и делим.
        let b = self.boxes[at];
        let axis = (0..6).max_by_key(|&a| b[a][1] - b[a][0]).unwrap_or(0);
        let mut sorted = ids.to_vec();
        // Центр диапазона точки — устойчивый порядок при равных рамках.
        sorted.sort_by_key(|&id| (points[id].ranges[axis][0] + points[id].ranges[axis][1]) / 2);
        let mid = sorted.len() / 2;
        let left = self.node(points, &sorted[..mid]);
        let right = self.node(points, &sorted[mid..]);
        self.children[at] = [left, right];
        at
    }

    /// Нижняя оценка штрафа: расстояние от точки до рамки узла.
    fn lower_bound(b: &[[i64; 2]; 6], point: &[i64; 6]) -> i64 {
        let mut sum = 0i64;
        for (axis, [min, max]) in b.iter().enumerate() {
            let d = if point[axis] > *max {
                point[axis] - max
            } else {
                (min - point[axis]).max(0)
            };
            sum += d * d;
        }
        sum
    }

    /// Минимум штрафа и лучшая точка. Обход в глубину с отсечением по оценке:
    /// узлы с оценкой не меньше текущего минимума не смотрятся.
    fn find(&self, points: &[Point], point: &[i64; 6]) -> Option<(usize, i64)> {
        let mut best: Option<(usize, i64)> = None;
        let mut stack = vec![self.root];
        while let Some(node) = stack.pop() {
            let bound = Self::lower_bound(&self.boxes[node], point);
            if best.is_some_and(|(_, b)| bound >= b) {
                continue;
            }
            let leaf = &self.leaves[node];
            if !leaf.is_empty() {
                for &id in leaf {
                    let fitness = fitness(&points[id], point);
                    if best.is_none_or(|(best_id, b)| fitness < b || (fitness == b && id < best_id))
                    {
                        best = Some((id, fitness));
                    }
                }
                continue;
            }
            let [left, right] = self.children[node];
            // Ближний узел обрабатываем первым: он даёт лучший минимум для отсечения.
            let (a, b) = (
                Self::lower_bound(&self.boxes[left], point),
                Self::lower_bound(&self.boxes[right], point),
            );
            if a <= b {
                stack.push(right);
                stack.push(left);
            } else {
                stack.push(left);
                stack.push(right);
            }
        }
        best
    }
}

/// `ParameterPoint.fitness`: сумма квадратов расстояний до диапазонов.
fn fitness(p: &Point, point: &[i64; 6]) -> i64 {
    let mut sum = 0i64;
    for (i, [min, max]) in p.ranges.iter().enumerate() {
        // `Parameter.distance`: только выход за диапазон, внутрь — ноль.
        let d = if point[i] > *max {
            point[i] - max
        } else {
            (min - point[i]).max(0)
        };
        sum += d * d;
    }
    sum
}

impl BiomeSource {
    /// `reports/biomes.csv`: `имя` и по паре `мин,макс` на каждую ось.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut points = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut f = line.split(',');
            let name = f.next().unwrap_or_default().to_owned();
            let mut ranges = [[0i64; 2]; 6];
            for axis in &mut ranges {
                let read = |v: Option<&str>| -> Result<i64, String> {
                    v.and_then(|v| v.trim().parse::<i64>().ok())
                        .ok_or_else(|| format!("biomes.csv line {}: bad number in {line:?}", i + 1))
                };
                axis[0] = read(f.next())?;
                axis[1] = read(f.next())?;
            }
            points.push(Point { name, ranges });
        }
        if points.is_empty() {
            return Err("biomes.csv: no parameter points".into());
        }
        let index = Index::build(&points);
        Ok(Self {
            points,
            ids: HashMap::new(),
            index,
        })
    }

    /// Привязка имён к сетевым ID реестра `worldgen/biome` этого сервера.
    pub fn bind(&mut self, ids: HashMap<String, u32>) {
        self.ids = ids;
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// `ParameterList.findValue`: минимум суммы квадратов расстояний.
    /// `point` уже квантован и лежит в порядке [`AXES`].
    pub fn find(&self, point: &[i64; 6]) -> Option<&Point> {
        let (id, _) = self.index.find(&self.points, point)?;
        self.points.get(id)
    }

    /// Сетевой ID биома для квантованной точки климата; без привязки — `None`.
    pub fn biome(&self, point: &[i64; 6]) -> Option<u32> {
        let p = self.find(point)?;
        self.ids.get(&p.name).copied()
    }

    /// Имя биома для точки — для сверки с ванилью.
    pub fn name(&self, point: &[i64; 6]) -> Option<&str> {
        self.find(point).map(|p| p.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "\
minecraft:plains,-10000,-4500,-10000,10000,-10000,10000,-10000,10000,0,0,-10000,10000,0
minecraft:desert,5500,10000,-10000,-6000,-10000,10000,-10000,10000,0,0,-10000,10000,0
";

    fn point(v: [i64; 6]) -> [i64; 6] {
        v
    }

    #[test]
    fn nearest_band_wins() {
        let src = BiomeSource::parse(CSV).unwrap();
        assert_eq!(src.len(), 2);
        // Холодно и сухо — пустыня по температуре выше 5500.
        let hot = point([8000, -9000, 0, 0, 0, 0]);
        assert_eq!(src.name(&hot), Some("minecraft:desert"));
        let cold = point([-8000, -9000, 0, 0, 0, 0]);
        assert_eq!(src.name(&cold), Some("minecraft:plains"));
    }

    #[test]
    fn inside_a_range_costs_nothing() {
        let src = BiomeSource::parse(CSV).unwrap();
        // Обе точки внутри своих диапазонов по температуре в 0..?
        // Пустыня от 5500, равнины до -4500: между ними зазор, побеждает ближняя.
        assert_eq!(
            src.name(&point([0, 0, 0, 0, 0, 0])),
            Some("minecraft:plains")
        );
        assert_eq!(
            src.name(&point([10000, 0, 0, 0, 0, 0])),
            Some("minecraft:desert")
        );
    }

    #[test]
    fn ties_keep_the_first_point() {
        let src = BiomeSource::parse(CSV).unwrap();
        // Ровно посередине зазора (500) обе точки одинаково далеки.
        assert_eq!(
            src.name(&point([500, 0, 0, 0, 0, 0])),
            Some("minecraft:plains")
        );
    }

    #[test]
    fn quantize_matches_vanilla_truncation() {
        assert_eq!(quantize(0.0), 0);
        assert_eq!(quantize(-0.1234), -1234);
        assert_eq!(quantize(-0.12349), -1234);
        assert_eq!(quantize(1.0), 10000);
    }

    #[test]
    fn index_matches_linear_scan_on_real_table() {
        // Настоящая таблица ванили: 7594 точки. Индекс обязан давать ту же
        // точку, что и полный перебор, иначе биомы разойдутся с ванилью.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release/generated/26.3/reports/biomes.csv");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return; // кэш данных не собран — тест нечего проверять
        };
        let src = BiomeSource::parse(&text).unwrap();
        assert_eq!(src.len(), 7594);
        let linear = |point: &[i64; 6]| -> usize {
            let mut best = (usize::MAX, i64::MAX);
            for (i, p) in src.points.iter().enumerate() {
                let f = fitness(p, point);
                if f < best.1 {
                    best = (i, f);
                }
            }
            best.0
        };
        // Точки, взятые из самой таблицы, плюс их середины и края диапазонов.
        let mut checked = 0;
        for (i, p) in src.points.iter().enumerate().step_by(37) {
            let mut inside = [0i64; 6];
            for (axis, range) in p.ranges.iter().enumerate() {
                inside[axis] = (range[0] + range[1]) / 2;
            }
            let got = src.find(&inside).map(|p| p as *const Point);
            let want = src.points.get(linear(&inside)).map(|p| p as *const Point);
            assert_eq!(got, want, "точка {i} внутри своего диапазона");
            checked += 1;
            // За границей всех диапазонов: штраф ненулевой у всех.
            let mut outside = inside;
            outside[0] = outside[0].saturating_add(4000);
            outside[3] = outside[3].saturating_sub(3000);
            let got = src.find(&outside).map(|p| p as *const Point);
            let want = src.points.get(linear(&outside)).map(|p| p as *const Point);
            assert_eq!(got, want, "точка {i} вне диапазонов");
            checked += 1;
        }
        assert!(checked > 100, "проверено всего {checked} точек");
    }

    #[test]
    fn biome_id_requires_binding() {
        let mut src = BiomeSource::parse(CSV).unwrap();
        assert_eq!(src.biome(&point([8000, 0, 0, 0, 0, 0])), None);
        src.bind(HashMap::from([("minecraft:desert".to_owned(), 7)]));
        assert_eq!(src.biome(&point([8000, 0, 0, 0, 0, 0])), Some(7));
        // Неизвестное имя не превращается в чужой биом.
        assert_eq!(src.biome(&point([-8000, 0, 0, 0, 0, 0])), None);
    }
}

/// `BiomeManager.obfuscateSeed`: SHA-256 от long в порядке little-endian
/// (Guava `hashLong`), первые 8 байт как little-endian (`HashCode.asLong`).
/// Это же значение уходит клиенту как `hashed_seed`.
pub fn zoom_seed(seed: i64) -> i64 {
    use sha2::Digest;
    let h = sha2::Sha256::digest(seed.to_le_bytes());
    i64::from_le_bytes(h[..8].try_into().unwrap())
}

/// `LinearCongruentialGenerator.next`.
fn lcg(r: i64, c: i64) -> i64 {
    r.wrapping_mul(
        r.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407),
    )
    .wrapping_add(c)
}

fn fiddle(r: i64) -> f64 {
    ((r >> 24).rem_euclid(1024) as f64 / 1024.0 - 0.5) * 0.9
}

fn fiddled_distance(seed: i64, x: i32, y: i32, z: i32, dx: f64, dy: f64, dz: f64) -> f64 {
    let mut r = lcg(seed, x as i64);
    r = lcg(r, y as i64);
    r = lcg(r, z as i64);
    r = lcg(r, x as i64);
    r = lcg(r, y as i64);
    r = lcg(r, z as i64);
    let fx = fiddle(r);
    r = lcg(r, seed);
    let fy = fiddle(r);
    r = lcg(r, seed);
    let fz = fiddle(r);
    (dz + fz) * (dz + fz) + (dy + fy) * (dy + fy) + (dx + fx) * (dx + fx)
}

/// `BiomeManager.getBiome`: квартовая клетка, чей биом достаётся блоку.
/// Граница биомов «дрожит» по сиду, поэтому соседняя клетка тоже возможна.
pub fn zoom(seed: i64, x: i32, y: i32, z: i32) -> (i32, i32, i32) {
    let (ax, ay, az) = (x - 2, y - 2, z - 2);
    let (px, py, pz) = (ax >> 2, ay >> 2, az >> 2);
    let (fx, fy, fz) = (
        (ax & 3) as f64 / 4.0,
        (ay & 3) as f64 / 4.0,
        (az & 3) as f64 / 4.0,
    );
    let mut best = 0;
    let mut min = f64::INFINITY;
    for i in 0..8 {
        let (xe, ye, ze) = (i & 4 == 0, i & 2 == 0, i & 1 == 0);
        let d = fiddled_distance(
            seed,
            if xe { px } else { px + 1 },
            if ye { py } else { py + 1 },
            if ze { pz } else { pz + 1 },
            if xe { fx } else { fx - 1.0 },
            if ye { fy } else { fy - 1.0 },
            if ze { fz } else { fz - 1.0 },
        );
        if min > d {
            best = i;
            min = d;
        }
    }
    (
        if best & 4 == 0 { px } else { px + 1 },
        if best & 2 == 0 { py } else { py + 1 },
        if best & 1 == 0 { pz } else { pz + 1 },
    )
}

#[cfg(test)]
mod zoom_tests {
    #[test]
    fn zoom_seed_matches_guava() {
        // Hashing.sha256().hashLong(seed).asLong() из Guava 33.6.
        assert_eq!(super::zoom_seed(12345), 293737985876514017);
        assert_eq!(super::zoom_seed(1), -6467378160175308932);
        assert_eq!(super::zoom_seed(-4172144997902289642), 2159143436479834350);
    }

    #[test]
    fn zoom_stays_next_to_the_block() {
        for (x, y, z) in [(0, 0, 0), (-7, 63, 130), (1000, -64, -1000)] {
            let (qx, qy, qz) = super::zoom(42, x, y, z);
            assert!(((x - 2) >> 2..=((x - 2) >> 2) + 1).contains(&qx));
            assert!(((y - 2) >> 2..=((y - 2) >> 2) + 1).contains(&qy));
            assert!(((z - 2) >> 2..=((z - 2) >> 2) + 1).contains(&qz));
        }
    }
}
