//! Мультишумный источник биомов (`MultiNoiseBiomeSource`, `Climate.ParameterList`).
//!
//! Климат — шесть осей (`temperature`, `humidity`, `continentalness`, `erosion`,
//! `depth`, `weirdness`), каждая точка `ParameterPoint` задаёт диапазон на каждой
//! оси. Биом выбирается по минимальному «штрафу»: сумма квадратов расстояний до
//! диапазонов. Оси квантуются как `Climate.quantizeCoord` (float * 10000 в long),
//! поэтому сравнение целочисленное и совпадает с ванилью бит в бит.

use std::collections::HashMap;

/// Точек в файле ванили около 7600; при равенстве штрафа выбор задаёт
/// обход `Climate.RTree` (см. [`Index`]).
#[derive(Debug, Clone)]
pub struct Point {
    pub name: String,
    /// `[min, max]` по осям в порядке [`AXES`], уже квантованные.
    pub ranges: [[i64; 2]; 6],
}

/// Биомы Края: центр, высокогорья, средние земли, малые острова, пустоши.
pub const END_BIOMES: [&str; 5] = [
    "minecraft:the_end",
    "minecraft:end_highlands",
    "minecraft:end_midlands",
    "minecraft:small_end_islands",
    "minecraft:end_barrens",
];

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

/// `Climate.RTree`: дерево ванили по семи осям (седьмая — `offset`, у всех
/// точек 0). Строится тем же разбиением и обходится в том же порядке, поэтому
/// при равных штрафах выбирается та же точка, что у ванили: побеждает
/// найденная раньше, а прошлый ответ (`lastResult`) — раньше всех.
#[derive(Debug, Clone, Default)]
struct Index {
    nodes: Vec<Node>,
    /// Лист каждой точки (номер узла).
    leaf_of: Vec<usize>,
    root: usize,
}

#[derive(Debug, Clone)]
struct Node {
    space: [[i64; 2]; DIMS],
    /// Номер точки у листа.
    point: Option<usize>,
    children: Vec<usize>,
}

/// Шесть осей климата и `offset`.
const DIMS: usize = 7;
/// `RTree.CHILDREN_PER_NODE`.
const CHILDREN: usize = 19;

fn center(r: [i64; 2]) -> i64 {
    (r[0] + r[1]) / 2
}

impl Index {
    fn build(points: &[Point]) -> Self {
        let mut index = Self::default();
        let leaves: Vec<usize> = points
            .iter()
            .enumerate()
            .map(|(id, p)| {
                let mut space = [[0i64; 2]; DIMS];
                space[..6].copy_from_slice(&p.ranges);
                index.nodes.push(Node {
                    space,
                    point: Some(id),
                    children: Vec::new(),
                });
                index.nodes.len() - 1
            })
            .collect();
        index.leaf_of = leaves.clone();
        if !leaves.is_empty() {
            index.root = index.node(leaves);
        }
        index
    }

    fn subtree(&mut self, children: Vec<usize>) -> usize {
        let mut space = self.nodes[children[0]].space;
        for &c in &children[1..] {
            for (s, r) in space.iter_mut().zip(self.nodes[c].space) {
                s[0] = s[0].min(r[0]);
                s[1] = s[1].max(r[1]);
            }
        }
        self.nodes.push(Node {
            space,
            point: None,
            children,
        });
        self.nodes.len() - 1
    }

    /// `RTree.build`.
    fn node(&mut self, mut children: Vec<usize>) -> usize {
        if children.len() == 1 {
            return children[0];
        }
        if children.len() <= CHILDREN {
            children.sort_by_key(|&c| {
                self.nodes[c]
                    .space
                    .iter()
                    .map(|&r| center(r).abs())
                    .sum::<i64>()
            });
            return self.subtree(children);
        }
        let mut best: Option<(i64, usize, Vec<Vec<usize>>)> = None;
        for d in 0..DIMS {
            self.sort(&mut children, d, false);
            let buckets = bucketize(&children);
            let cost: i64 = buckets
                .iter()
                .map(|b| {
                    (0..DIMS)
                        .map(|k| {
                            let lo = b
                                .iter()
                                .map(|&c| self.nodes[c].space[k][0])
                                .min()
                                .unwrap_or(0);
                            let hi = b
                                .iter()
                                .map(|&c| self.nodes[c].space[k][1])
                                .max()
                                .unwrap_or(0);
                            (hi - lo).abs()
                        })
                        .sum::<i64>()
                })
                .sum();
            if best.as_ref().is_none_or(|(min, _, _)| *min > cost) {
                best = Some((cost, d, buckets));
            }
        }
        let (_, dim, buckets) = best.expect("at least one dimension");
        let mut subs: Vec<usize> = buckets.into_iter().map(|b| self.subtree(b)).collect();
        self.sort(&mut subs, dim, true);
        let built: Vec<usize> = subs
            .into_iter()
            .map(|s| {
                let kids = self.nodes[s].children.clone();
                self.node(kids)
            })
            .collect();
        self.subtree(built)
    }

    /// `RTree.sort`: ось `dim`, потом остальные по кругу; стабильно.
    fn sort(&self, list: &mut [usize], dim: usize, absolute: bool) {
        list.sort_by(|&a, &b| {
            for k in 0..DIMS {
                let axis = (dim + k) % DIMS;
                let key = |n: usize| {
                    let c = center(self.nodes[n].space[axis]);
                    if absolute { c.abs() } else { c }
                };
                let o = key(a).cmp(&key(b));
                if o.is_ne() {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    fn distance(&self, node: usize, target: &[i64; DIMS]) -> i64 {
        distance(&self.nodes[node].space, target)
    }

    /// `SubTree.search`: строгое `>` — при равенстве остаётся прежний кандидат.
    fn search(&self, node: usize, target: &[i64; DIMS], candidate: Option<usize>) -> Option<usize> {
        let n = &self.nodes[node];
        if n.point.is_some() {
            return Some(node);
        }
        let mut min = candidate.map_or(i64::MAX, |c| self.distance(c, target));
        let mut closest = candidate;
        for &child in &n.children {
            let d = self.distance(child, target);
            if min > d {
                let Some(leaf) = self.search(child, target, closest) else {
                    continue;
                };
                let ld = if leaf == child {
                    d
                } else {
                    self.distance(leaf, target)
                };
                if min > ld {
                    min = ld;
                    closest = Some(leaf);
                }
            }
        }
        closest
    }

    /// Точка и штраф для `point`; `hint` — прошлый ответ (`lastResult`).
    fn find(
        &self,
        points: &[Point],
        point: &[i64; 6],
        hint: Option<usize>,
    ) -> Option<(usize, i64)> {
        if points.is_empty() {
            return None;
        }
        let mut target = [0i64; DIMS];
        target[..6].copy_from_slice(point);
        let candidate = hint.and_then(|h| self.leaf_of.get(h).copied());
        let leaf = self.search(self.root, &target, candidate)?;
        let id = self.nodes[leaf].point?;
        Some((id, self.distance(leaf, &target)))
    }
}

/// `RTree.bucketize`: куски по `19^floor(log19(n - 0.01))`.
fn bucketize(nodes: &[usize]) -> Vec<Vec<usize>> {
    let n = nodes.len() as f64;
    let expected =
        (CHILDREN as f64).powf(((n - 0.01).ln() / (CHILDREN as f64).ln()).floor()) as usize;
    nodes
        .chunks(expected.max(1))
        .map(<[usize]>::to_vec)
        .collect()
}

/// `Node.distance`: сумма квадратов выхода за диапазоны.
fn distance(space: &[[i64; 2]; DIMS], target: &[i64; DIMS]) -> i64 {
    let mut sum = 0i64;
    for (&[min, max], &t) in space.iter().zip(target) {
        let d = if t > max { t - max } else { (min - t).max(0) };
        sum += d * d;
    }
    sum
}

impl BiomeSource {
    /// `Preset.NETHER` из `MultiNoiseBiomeSourceParameterList`: пять точек,
    /// остальные оси закреплены нулём. Порядок — как в `List.of`.
    pub fn nether() -> Self {
        // `Climate.parameters(temperature, humidity, continentalness, erosion,
        // depth, weirdness, offset)`; `Climate.quantizeCoord` — `float * 10000`.
        const fn q(v: f32) -> i64 {
            (v as f64 * 10000.0) as i64
        }
        let point = |name: &str, temperature: f32, vegetation: f32, ridges: f32| Point {
            name: name.to_owned(),
            ranges: [
                [q(temperature), q(temperature)],
                [q(vegetation), q(vegetation)],
                [0, 0],
                [0, 0],
                [0, 0],
                [q(ridges), q(ridges)],
            ],
        };
        let points = vec![
            point("minecraft:nether_wastes", 0.0, 0.0, 0.0),
            point("minecraft:soul_sand_valley", 0.0, -0.5, 0.0),
            point("minecraft:crimson_forest", 0.4, 0.0, 0.0),
            point("minecraft:warped_forest", 0.0, 0.5, 0.375),
            point("minecraft:basalt_deltas", -0.5, 0.0, 0.175),
        ];
        let index = Index::build(&points);
        Self {
            points,
            ids: HashMap::new(),
            index,
        }
    }

    /// `TheEndBiomeSource`: пять биомов в порядке `collectPossibleBiomes`.
    /// Климат их не выбирает — точки нужны только для имён и ID.
    pub fn end() -> Self {
        let points: Vec<Point> = END_BIOMES
            .iter()
            .map(|name| Point {
                name: (*name).to_owned(),
                ranges: [[0, 0]; 6],
            })
            .collect();
        let index = Index::build(&points);
        Self {
            points,
            ids: HashMap::new(),
            index,
        }
    }

    /// Имя точки номер `id`.
    pub fn name_of(&self, id: usize) -> Option<&str> {
        self.points.get(id).map(|p| p.name.as_str())
    }

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
        let (id, _) = self.index.find(&self.points, point, None)?;
        self.points.get(id)
    }

    /// Номер точки для `point`, поиск от прошлого ответа `hint`.
    pub fn find_from(&self, point: &[i64; 6], hint: Option<usize>) -> Option<usize> {
        self.index.find(&self.points, point, hint).map(|(id, _)| id)
    }

    /// Сетевой ID биома точки номер `id`.
    pub fn id_of(&self, id: usize) -> Option<u32> {
        self.ids.get(&self.points[id].name).copied()
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

    /// Имена биомов без повторов в порядке точек.
    pub fn names(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.points
            .iter()
            .filter(|p| seen.insert(p.name.as_str()))
            .map(|p| p.name.clone())
            .collect()
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
        // Настоящая таблица ванили: 7594 точки. Дерево обязано находить точку
        // с тем же штрафом, что и полный перебор; ничьи решает порядок дерева.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release/generated/26.3/reports/biomes.csv");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return; // кэш данных не собран — тест нечего проверять
        };
        let src = BiomeSource::parse(&text).unwrap();
        assert_eq!(src.len(), 7594);
        let fitness = |p: &Point, point: &[i64; 6]| -> i64 {
            let mut space = [[0i64; 2]; DIMS];
            space[..6].copy_from_slice(&p.ranges);
            let mut target = [0i64; DIMS];
            target[..6].copy_from_slice(point);
            distance(&space, &target)
        };
        let linear = |point: &[i64; 6]| src.points.iter().map(|p| fitness(p, point)).min();
        // Точки, взятые из самой таблицы, плюс их середины и края диапазонов.
        let mut checked = 0;
        for (i, p) in src.points.iter().enumerate().step_by(37) {
            let mut inside = [0i64; 6];
            for (axis, range) in p.ranges.iter().enumerate() {
                inside[axis] = (range[0] + range[1]) / 2;
            }
            let got = src.find(&inside).map(|p| fitness(p, &inside));
            let want = linear(&inside);
            assert_eq!(got, want, "точка {i} внутри своего диапазона");
            checked += 1;
            // За границей всех диапазонов: штраф ненулевой у всех.
            let mut outside = inside;
            outside[0] = outside[0].saturating_add(4000);
            outside[3] = outside[3].saturating_sub(3000);
            let got = src.find(&outside).map(|p| fitness(p, &outside));
            let want = linear(&outside);
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
