//! Функции плотности 26.3 (`densityfunction/`): дерево из `worldgen/density_function`,
//! перезапись как в `DensityFunctionCompiler` (инлайн ссылок, общие кэши, срезы по
//! неиспользуемым осям) и сэмплеры с точечным и объёмным путём. Пути считают
//! по-разному, а кэши (`SamplerContext`) отдают точкам значения из объёма, поэтому
//! порядок вызовов повторяет ванильный.

use crate::noise::{self, NormalNoise, Stack, lerp, lerp3};
use crate::random::Positional;
use crate::volume::Volume;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

const AXIS_X: u8 = 1;
const AXIS_Y: u8 = 2;
const AXIS_Z: u8 = 4;
const ALL_AXES: u8 = 7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unary {
    Abs,
    Square,
    Cube,
    Sqrt,
    HalfNegative,
    QuarterNegative,
    Reciprocal,
    Negate,
    Squeeze,
    Log,
    Sign,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tiling {
    Clamp,
    Repeat,
    Mirrored,
}

/// Функция плотности до компиляции.
#[derive(Clone, Debug)]
pub enum Df {
    Const(f32),
    Ref(String),
    Noise {
        noise: String,
        xz: f64,
        y: f64,
        shift: Box<[Df; 3]>,
    },
    Shift(String),
    ShiftA(String),
    ShiftB(String),
    Blended {
        xz_scale: f64,
        y_scale: f64,
        xz_factor: f64,
        y_factor: f64,
        smear: f64,
    },
    Gradient {
        axis: usize,
        tiling: Tiling,
        from: i32,
        to: i32,
        from_value: f32,
        to_value: f32,
    },
    Unary(Unary, Box<Df>),
    Binary(Bin, Box<Df>, Box<Df>),
    Clamp(Box<Df>, f32, f32),
    RangeChoice(Box<[Df; 3]>, f32, f32),
    IntervalSelect(Box<Df>, Vec<f32>, Vec<Df>),
    Lerp(Box<[Df; 3]>),
    Cache(Box<Df>),
    Interpolated(Box<Df>, i32, i32),
    Slice(usize, i32, Box<Df>),
    FindTop(Box<[Df; 2]>, i32, i32),
    Spline(Box<SplineDf>),
    BlendAlpha,
    BlendOffset,
    Beardifier,
    BlendDensity(Box<Df>),
    /// Кэш после дедупликации (`PreparedCache`): номер ячейки и оси входа.
    Prepared(usize, u8),
}

#[derive(Clone, Debug)]
pub enum SplineDf {
    Const(f32),
    Multi {
        coord: Df,
        locs: Vec<f32>,
        values: Vec<SplineDf>,
        ders: Vec<f32>,
    },
}

fn id(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn axis(v: &Value) -> Result<usize, String> {
    match v.as_str() {
        Some("x") => Ok(0),
        Some("y") => Ok(1),
        Some("z") => Ok(2),
        _ => Err(format!("bad axis {v}")),
    }
}

fn f(v: &Value, key: &str) -> Result<f64, String> {
    v[key]
        .as_f64()
        .ok_or_else(|| format!("density function: no number {key}"))
}

fn i(v: &Value, key: &str) -> Result<i32, String> {
    v[key]
        .as_i64()
        .map(|x| x as i32)
        .ok_or_else(|| format!("density function: no integer {key}"))
}

impl Df {
    pub fn parse(v: &Value) -> Result<Df, String> {
        let p = |key: &str| Df::parse(&v[key]).map(Box::new);
        let pp = |key: &str| Df::parse(&v[key]);
        Ok(match v {
            Value::Number(n) => Df::Const(n.as_f64().unwrap_or(0.0) as f32),
            Value::String(s) => Df::Ref(id(s)),
            Value::Object(o) => {
                let ty = o
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or("density function without type")?;
                let ty = ty.strip_prefix("minecraft:").unwrap_or(ty);
                let unary = |u| Ok::<_, String>(Df::Unary(u, p("input")?));
                let binary = |b| Ok::<_, String>(Df::Binary(b, p("left")?, p("right")?));
                match ty {
                    "constant" => Df::Const(f(v, "argument")? as f32),
                    "noise" => Df::Noise {
                        noise: id(v["noise"].as_str().ok_or("noise: no id")?),
                        xz: f(v, "xz_scale")?,
                        y: f(v, "y_scale")?,
                        shift: Box::new([
                            opt(&v["shift_x"])?,
                            opt(&v["shift_y"])?,
                            opt(&v["shift_z"])?,
                        ]),
                    },
                    "shift" => Df::Shift(id(noise_arg(v)?)),
                    "shift_a" => Df::ShiftA(id(noise_arg(v)?)),
                    "shift_b" => Df::ShiftB(id(noise_arg(v)?)),
                    "old_blended_noise" => Df::Blended {
                        xz_scale: f(v, "xz_scale")?,
                        y_scale: f(v, "y_scale")?,
                        xz_factor: f(v, "xz_factor")?,
                        y_factor: f(v, "y_factor")?,
                        smear: f(v, "smear_scale_multiplier")?,
                    },
                    "gradient" => Df::Gradient {
                        axis: axis(&v["axis"])?,
                        tiling: match v["tiling"].as_str() {
                            None | Some("clamp_to_edge") => Tiling::Clamp,
                            Some("repeat") => Tiling::Repeat,
                            Some("mirrored_repeat") => Tiling::Mirrored,
                            Some(t) => return Err(format!("gradient: tiling {t}")),
                        },
                        from: i(v, "from_coordinate")?,
                        to: i(v, "to_coordinate")?,
                        from_value: f(v, "from_value")? as f32,
                        to_value: f(v, "to_value")? as f32,
                    },
                    "abs" => unary(Unary::Abs)?,
                    "square" => unary(Unary::Square)?,
                    "cube" => unary(Unary::Cube)?,
                    "sqrt" => unary(Unary::Sqrt)?,
                    "half_negative" => unary(Unary::HalfNegative)?,
                    "quarter_negative" => unary(Unary::QuarterNegative)?,
                    "reciprocal" => unary(Unary::Reciprocal)?,
                    "negate" => unary(Unary::Negate)?,
                    "squeeze" => unary(Unary::Squeeze)?,
                    "log" => unary(Unary::Log)?,
                    "sign" => unary(Unary::Sign)?,
                    "add" => binary(Bin::Add)?,
                    "sub" => binary(Bin::Sub)?,
                    "mul" => binary(Bin::Mul)?,
                    "div" => binary(Bin::Div)?,
                    "min" => binary(Bin::Min)?,
                    "max" => binary(Bin::Max)?,
                    "clamp" => Df::Clamp(p("input")?, f(v, "min")? as f32, f(v, "max")? as f32),
                    "range_choice" => Df::RangeChoice(
                        Box::new([pp("input")?, pp("when_in_range")?, pp("when_out_of_range")?]),
                        f(v, "min_inclusive")? as f32,
                        f(v, "max_exclusive")? as f32,
                    ),
                    "interval_select" => Df::IntervalSelect(
                        p("input")?,
                        v["thresholds"]
                            .as_array()
                            .ok_or("interval_select: no thresholds")?
                            .iter()
                            .map(|t| t.as_f64().unwrap_or(0.0) as f32)
                            .collect(),
                        v["functions"]
                            .as_array()
                            .ok_or("interval_select: no functions")?
                            .iter()
                            .map(Df::parse)
                            .collect::<Result<_, _>>()?,
                    ),
                    "lerp" => Df::Lerp(Box::new([pp("alpha")?, pp("first")?, pp("second")?])),
                    "cache" => Df::Cache(p("input")?),
                    "interpolated" => {
                        Df::Interpolated(p("input")?, i(v, "cell_size_xz")?, i(v, "cell_size_y")?)
                    }
                    "slice" => Df::Slice(axis(&v["axis"])?, i(v, "coordinate")?, p("input")?),
                    "find_top_surface" => Df::FindTop(
                        Box::new([pp("density")?, pp("upper_bound")?]),
                        i(v, "lower_bound")?,
                        i(v, "cell_height")?,
                    ),
                    "spline" => Df::Spline(Box::new(SplineDf::parse(&v["spline"])?)),
                    "blend_alpha" => Df::BlendAlpha,
                    "blend_offset" => Df::BlendOffset,
                    "beardifier" => Df::Beardifier,
                    "blend_density" => Df::BlendDensity(p("input")?),
                    t => return Err(format!("density function type {t} is not supported")),
                }
            }
            _ => return Err(format!("bad density function {v}")),
        })
    }

    fn is_zero(&self) -> bool {
        matches!(self, Df::Const(v) if v.to_bits() == 0)
    }

    fn constant(&self) -> Option<f32> {
        match self {
            Df::Const(v) => Some(*v),
            _ => None,
        }
    }

    /// `rewriteChildren`: те же дети в том же порядке.
    fn map_children(self, g: &mut dyn FnMut(Df) -> Df) -> Df {
        let b = |d: Box<Df>, g: &mut dyn FnMut(Df) -> Df| Box::new(g(*d));
        match self {
            Df::Noise {
                noise,
                xz,
                y,
                shift,
            } => {
                let [a, bb, c] = *shift;
                let (a, bb, c) = (g(a), g(bb), g(c));
                Df::Noise {
                    noise,
                    xz,
                    y,
                    shift: Box::new([a, bb, c]),
                }
            }
            Df::Unary(u, x) => Df::Unary(u, b(x, g)),
            Df::Binary(o, l, r) => {
                let l = b(l, g);
                Df::Binary(o, l, b(r, g))
            }
            Df::Clamp(x, lo, hi) => Df::Clamp(b(x, g), lo, hi),
            Df::RangeChoice(c, lo, hi) => {
                let [a, bb, cc] = *c;
                let (a, bb, cc) = (g(a), g(bb), g(cc));
                Df::RangeChoice(Box::new([a, bb, cc]), lo, hi)
            }
            Df::IntervalSelect(x, t, fs) => {
                let x = b(x, g);
                Df::IntervalSelect(x, t, fs.into_iter().map(&mut *g).collect())
            }
            Df::Lerp(c) => {
                let [a, bb, cc] = *c;
                let (a, bb, cc) = (g(a), g(bb), g(cc));
                Df::Lerp(Box::new([a, bb, cc]))
            }
            Df::Cache(x) => Df::Cache(b(x, g)),
            Df::Interpolated(x, a, c) => Df::Interpolated(b(x, g), a, c),
            Df::Slice(a, c, x) => Df::Slice(a, c, b(x, g)),
            Df::FindTop(c, lo, h) => {
                let [d, u] = *c;
                let (d, u) = (g(d), g(u));
                Df::FindTop(Box::new([d, u]), lo, h)
            }
            Df::Spline(s) => Df::Spline(Box::new(s.map_coords(g))),
            Df::BlendDensity(x) => Df::BlendDensity(b(x, g)),
            other => other,
        }
    }
}

fn opt(v: &Value) -> Result<Df, String> {
    if v.is_null() {
        Ok(Df::Const(0.0))
    } else {
        Df::parse(v)
    }
}

fn noise_arg(v: &Value) -> Result<&str, String> {
    v["noise"]
        .as_str()
        .or_else(|| v["argument"].as_str())
        .ok_or_else(|| "shift: no noise".to_owned())
}

impl SplineDf {
    fn parse(v: &Value) -> Result<SplineDf, String> {
        if let Some(n) = v.as_f64() {
            return Ok(SplineDf::Const(n as f32));
        }
        let points = v["points"].as_array().ok_or("spline: no points")?;
        let mut locs = Vec::new();
        let mut values = Vec::new();
        let mut ders = Vec::new();
        for p in points {
            locs.push(f(p, "location")? as f32);
            ders.push(f(p, "derivative")? as f32);
            values.push(SplineDf::parse(&p["value"])?);
        }
        Ok(SplineDf::Multi {
            coord: Df::parse(&v["coordinate"])?,
            locs,
            values,
            ders,
        })
    }

    fn map_coords(self, g: &mut dyn FnMut(Df) -> Df) -> SplineDf {
        match self {
            SplineDf::Const(c) => SplineDf::Const(c),
            SplineDf::Multi {
                coord,
                locs,
                values,
                ders,
            } => {
                let coord = g(coord);
                SplineDf::Multi {
                    coord,
                    locs,
                    values: values.into_iter().map(|s| s.map_coords(g)).collect(),
                    ders,
                }
            }
        }
    }

    fn for_each_coord(&self, g: &mut dyn FnMut(&Df)) {
        if let SplineDf::Multi { coord, values, .. } = self {
            g(coord);
            for v in values {
                v.for_each_coord(g);
            }
        }
    }
}

/// `domainAxes`: от каких координат зависит функция.
fn domain_axes(d: &Df) -> u8 {
    match d {
        Df::Const(_) => 0,
        Df::Ref(_) | Df::Cache(_) => unreachable!("domain axes before rewrite"),
        Df::Noise { xz, y, shift, .. } => {
            let mut a = ALL_AXES;
            if *y == 0.0 {
                a &= !AXIS_Y;
            }
            if *xz == 0.0 {
                a &= !(AXIS_X | AXIS_Z);
            }
            a | domain_axes(&shift[0]) | domain_axes(&shift[1]) | domain_axes(&shift[2])
        }
        Df::Shift(_) | Df::Blended { .. } | Df::Beardifier => ALL_AXES,
        Df::ShiftA(_) | Df::ShiftB(_) | Df::BlendAlpha | Df::BlendOffset => AXIS_X | AXIS_Z,
        Df::Gradient { axis, .. } => 1 << axis,
        Df::Unary(_, x) | Df::Clamp(x, ..) | Df::Interpolated(x, ..) | Df::BlendDensity(x) => {
            domain_axes(x)
        }
        Df::Binary(_, l, r) => domain_axes(l) | domain_axes(r),
        Df::RangeChoice(c, ..) => c.iter().map(domain_axes).fold(0, |a, b| a | b),
        Df::Lerp(c) => c.iter().map(domain_axes).fold(0, |a, b| a | b),
        Df::IntervalSelect(x, _, fs) => fs
            .iter()
            .map(domain_axes)
            .fold(domain_axes(x), |a, b| a | b),
        Df::Slice(a, _, x) => domain_axes(x) & !(1 << a),
        Df::FindTop(c, ..) => (domain_axes(&c[0]) | domain_axes(&c[1])) & !AXIS_Y,
        Df::Spline(s) => {
            let mut a = 0;
            s.for_each_coord(&mut |c| a |= domain_axes(c));
            a
        }
        Df::Prepared(_, a) => *a,
    }
}

/// `DfRewriteRule.SliceUniformAxes`.
fn slice_axes(d: Df, parent: u8) -> Df {
    if matches!(d, Df::Const(_) | Df::Gradient { .. }) {
        return d;
    }
    let axes = domain_axes(&d);
    if axes == parent {
        return d.map_children(&mut |c| slice_axes(c, parent));
    }
    let mut d = d.map_children(&mut |c| slice_axes(c, axes));
    let mut existing = 0;
    let mut cur = &d;
    while let Df::Slice(a, _, x) = cur {
        existing |= 1 << a;
        cur = x;
    }
    let removed = parent & !axes & !existing;
    for (bit, a) in [(AXIS_X, 0), (AXIS_Z, 2), (AXIS_Y, 1)] {
        if removed & bit != 0 {
            d = Df::Slice(a, 0, Box::new(d));
        }
    }
    d
}

// ---------------------------------------------------------------- компиляция

/// Сжатый сплайн: координаты — индексы в `S::Spline::coords`.
#[derive(Debug)]
pub enum Spl {
    Const(f32),
    Multi {
        coord: usize,
        locs: Vec<f32>,
        values: Vec<Spl>,
        ders: Vec<f32>,
    },
}

#[derive(Debug)]
pub enum Grad {
    Clamp { from: i32, min: i32, max: i32 },
    Repeat { from: i32, range: i32 },
    Mirrored { from: i32, range: i32 },
}

/// Скомпилированный сэмплер (`DensitySampler`).
#[derive(Debug)]
pub enum S {
    Const(f32),
    Noise(Arc<Stack>, f64, f64),
    ShiftedXz(Box<[S; 2]>, Arc<Stack>, f64, f64),
    ShiftedXyz(Box<[S; 3]>, Arc<Stack>, f64, f64),
    ShiftB(Arc<Stack>),
    ConstAdd(Box<S>, f32),
    ConstSub(f32, Box<S>),
    ConstMul(Box<S>, f32),
    ConstDiv(f32, Box<S>),
    ConstMin(Box<S>, f32),
    ConstMax(Box<S>, f32),
    Binary(Bin, Box<[S; 2]>),
    Unary(Unary, Box<S>),
    Clamp(Box<S>, f32, f32),
    RangeChoice(Box<[S; 3]>, f32, f32),
    IntervalSelect(Box<S>, Vec<f32>, Vec<S>),
    Lerp(Box<[S; 3]>),
    Gradient(usize, Grad, f32, f32),
    Interpolated(Box<S>, i32, i32),
    SliceX(Box<S>, i32),
    SliceY(Box<S>, i32),
    SliceZ(Box<S>, i32),
    SliceXz(Box<S>, i32, i32),
    FindTop(Box<[S; 2]>, i32, i32),
    Spline(Spl, Vec<S>),
    Cached(usize, Arc<S>),
}

/// Ячейка кэша (`SamplerContext.CacheCell`).
struct Cell {
    vol: Option<Volume>,
    buf: Vec<f32>,
    key: (i32, i32, i32),
    value: f32,
}

/// `SamplerContext` с включёнными кэшами: свой на каждый чанк.
#[derive(Default)]
pub struct Ctx {
    cells: Vec<Cell>,
}

impl Ctx {
    fn cell(&mut self, id: usize) -> &mut Cell {
        while self.cells.len() <= id {
            self.cells.push(Cell {
                vol: None,
                buf: Vec::new(),
                key: (0, 0, 0),
                value: f32::NAN,
            });
        }
        &mut self.cells[id]
    }
}

/// Сборщик сэмплеров мира (`DensityFunctionCompiler` + `RandomState`).
pub struct Compiler {
    registry: HashMap<String, Value>,
    noise_params: HashMap<String, NormalNoise>,
    world: Positional,
    noises: HashMap<String, Arc<Stack>>,
    prepared: HashMap<String, (usize, u8)>,
    next_id: usize,
    compiled: HashMap<String, Arc<S>>,
}

impl Compiler {
    pub fn new(
        registry: HashMap<String, Value>,
        noise_params: HashMap<String, NormalNoise>,
        world: Positional,
    ) -> Self {
        Self {
            registry,
            noise_params,
            world,
            noises: HashMap::new(),
            prepared: HashMap::new(),
            next_id: 0,
            compiled: HashMap::new(),
        }
    }

    pub fn world(&self) -> Positional {
        self.world
    }

    /// `getSampler`: один сэмплер на равную функцию.
    pub fn sampler(&mut self, d: &Df) -> Result<Arc<S>, String> {
        let key = format!("{d:?}");
        if let Some(s) = self.compiled.get(&key) {
            return Ok(s.clone());
        }
        let opt = self.optimize(d.clone())?;
        let s = Arc::new(self.compile(&opt)?);
        self.compiled.insert(key, s.clone());
        Ok(s)
    }

    fn optimize(&mut self, d: Df) -> Result<Df, String> {
        let d = self.inline_caches(d)?;
        Ok(slice_axes(d, ALL_AXES))
    }

    fn inline_caches(&mut self, d: Df) -> Result<Df, String> {
        let mut d = d;
        while let Df::Ref(name) = &d {
            let v = self
                .registry
                .get(name)
                .ok_or_else(|| format!("density function {name} not found"))?;
            d = Df::parse(&v.clone())?;
        }
        if let Df::Cache(input) = d {
            return self.prepare(*input);
        }
        let mut err = None;
        let out = d.map_children(&mut |c| match self.inline_caches(c) {
            Ok(c) => c,
            Err(e) => {
                err = Some(e);
                Df::Const(0.0)
            }
        });
        match err {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    fn prepare(&mut self, input: Df) -> Result<Df, String> {
        let key = format!("{input:?}");
        if let Some(&(id, axes)) = self.prepared.get(&key) {
            return Ok(Df::Prepared(id, axes));
        }
        let id = self.next_id;
        self.next_id += 1;
        let opt = self.optimize(input)?;
        let axes = domain_axes(&opt);
        let s = Arc::new(self.compile(&opt)?);
        self.compiled.insert(format!("#cache{id}"), s);
        self.prepared.insert(key, (id, axes));
        Ok(Df::Prepared(id, axes))
    }

    fn noise(&mut self, name: &str) -> Result<Arc<Stack>, String> {
        if let Some(n) = self.noises.get(name) {
            return Ok(n.clone());
        }
        let params = self
            .noise_params
            .get(name)
            .ok_or_else(|| format!("noise {name} not found"))?;
        let n = Arc::new(noise::instantiate(params, &self.world, name));
        self.noises.insert(name.to_owned(), n.clone());
        Ok(n)
    }

    fn compile(&mut self, d: &Df) -> Result<S, String> {
        Ok(match d {
            Df::Const(v) => S::Const(*v),
            Df::Ref(_) | Df::Cache(_) => unreachable!("compile before rewrite"),
            Df::Noise {
                noise,
                xz,
                y,
                shift,
            } => {
                let n = self.noise(noise)?;
                if shift.iter().all(Df::is_zero) {
                    S::Noise(n, *xz, *y)
                } else {
                    let sx = self.compile(&shift[0])?;
                    let sz = self.compile(&shift[2])?;
                    if shift[1].is_zero() {
                        S::ShiftedXz(Box::new([sx, sz]), n, *xz, *y)
                    } else {
                        let sy = self.compile(&shift[1])?;
                        S::ShiftedXyz(Box::new([sx, sy, sz]), n, *xz, *y)
                    }
                }
            }
            Df::Shift(n) => S::ConstMul(Box::new(S::Noise(self.noise(n)?, 0.25, 0.25)), 4.0),
            Df::ShiftA(n) => S::ConstMul(Box::new(S::Noise(self.noise(n)?, 0.25, 0.0)), 4.0),
            Df::ShiftB(n) => S::ShiftB(self.noise(n)?),
            Df::Blended {
                xz_scale,
                y_scale,
                xz_factor,
                y_factor,
                smear,
            } => {
                let mut r = self.world.from_hash_of("minecraft:terrain");
                let xz_m = 684.412 * xz_scale;
                let y_m = 684.412 * y_scale;
                let limit_smear = y_m * smear;
                let main_smear = limit_smear / y_factor;
                let limit = 0.99998474_f32 as f64;
                let min = noise::blended_fbm(&mut r, -15, limit_smear, limit);
                let max = noise::blended_fbm(&mut r, -15, limit_smear, limit);
                let main = noise::blended_fbm(&mut r, -7, main_smear, 12.75);
                let main = S::Noise(Arc::new(main), xz_m / xz_factor, y_m / y_factor);
                let choice = S::Clamp(Box::new(S::ConstAdd(Box::new(main), 0.5)), 0.0, 1.0);
                S::Lerp(Box::new([
                    choice,
                    S::Noise(Arc::new(min), xz_m, y_m),
                    S::Noise(Arc::new(max), xz_m, y_m),
                ]))
            }
            Df::Gradient {
                axis,
                tiling,
                from,
                to,
                from_value,
                to_value,
            } => {
                let range = to - from;
                let factor = (to_value - from_value) / range as f32;
                let g = match tiling {
                    Tiling::Clamp => Grad::Clamp {
                        from: *from,
                        min: (*from).min(*to),
                        max: (*from).max(*to),
                    },
                    Tiling::Repeat => Grad::Repeat { from: *from, range },
                    Tiling::Mirrored => Grad::Mirrored { from: *from, range },
                };
                S::Gradient(*axis, g, *from_value, factor)
            }
            Df::Unary(u, x) => S::Unary(*u, Box::new(self.compile(x)?)),
            Df::Binary(op, l, r) => {
                let (lc, rc) = (l.constant(), r.constant());
                let ls = self.compile(l)?;
                let rs = self.compile(r)?;
                match op {
                    Bin::Add => match (lc, rc) {
                        (Some(v), _) => S::ConstAdd(Box::new(rs), v),
                        (_, Some(v)) => S::ConstAdd(Box::new(ls), v),
                        _ => S::Binary(Bin::Add, Box::new([ls, rs])),
                    },
                    Bin::Sub => match (lc, rc) {
                        (Some(v), _) => S::ConstSub(v, Box::new(rs)),
                        (_, Some(v)) => S::ConstAdd(Box::new(ls), -v),
                        _ => S::Binary(Bin::Sub, Box::new([ls, rs])),
                    },
                    Bin::Mul => match (lc, rc) {
                        (Some(v), _) => S::ConstMul(Box::new(rs), v),
                        (_, Some(v)) => S::ConstMul(Box::new(ls), v),
                        _ => S::Binary(Bin::Mul, Box::new([ls, rs])),
                    },
                    Bin::Div => match (lc, rc) {
                        (Some(v), _) => S::ConstDiv(v, Box::new(rs)),
                        (_, Some(v)) => S::ConstMul(Box::new(ls), 1.0 / v),
                        _ => S::Binary(Bin::Div, Box::new([ls, rs])),
                    },
                    Bin::Min => match (lc, rc) {
                        (Some(v), _) => S::ConstMin(Box::new(rs), v),
                        (_, Some(v)) => S::ConstMin(Box::new(ls), v),
                        _ => S::Binary(Bin::Min, Box::new([ls, rs])),
                    },
                    Bin::Max => match (lc, rc) {
                        (Some(v), _) => S::ConstMax(Box::new(rs), v),
                        (_, Some(v)) => S::ConstMax(Box::new(ls), v),
                        _ => S::Binary(Bin::Max, Box::new([ls, rs])),
                    },
                }
            }
            Df::Clamp(x, lo, hi) => S::Clamp(Box::new(self.compile(x)?), *lo, *hi),
            Df::RangeChoice(ch, lo, hi) => S::RangeChoice(
                Box::new([
                    self.compile(&ch[0])?,
                    self.compile(&ch[1])?,
                    self.compile(&ch[2])?,
                ]),
                *lo,
                *hi,
            ),
            Df::IntervalSelect(x, t, fs) => {
                let x = Box::new(self.compile(x)?);
                let fs = fs
                    .iter()
                    .map(|f| self.compile(f))
                    .collect::<Result<_, _>>()?;
                S::IntervalSelect(x, t.clone(), fs)
            }
            Df::Lerp(ch) => S::Lerp(Box::new([
                self.compile(&ch[0])?,
                self.compile(&ch[1])?,
                self.compile(&ch[2])?,
            ])),
            Df::Interpolated(x, xz, y) => S::Interpolated(Box::new(self.compile(x)?), *xz, *y),
            Df::Slice(a, coord, x) => {
                if let Df::Slice(b, inner, xx) = &**x
                    && matches!((a, b), (0, 2) | (2, 0))
                {
                    let (sx, sz) = if *a == 0 {
                        (*coord, *inner)
                    } else {
                        (*inner, *coord)
                    };
                    S::SliceXz(Box::new(self.compile(xx)?), sx, sz)
                } else {
                    let s = Box::new(self.compile(x)?);
                    match a {
                        0 => S::SliceX(s, *coord),
                        1 => S::SliceY(s, *coord),
                        _ => S::SliceZ(s, *coord),
                    }
                }
            }
            Df::FindTop(ch, lo, h) => S::SliceY(
                Box::new(S::FindTop(
                    Box::new([self.compile(&ch[0])?, self.compile(&ch[1])?]),
                    *lo,
                    *h,
                )),
                0,
            ),
            Df::Spline(s) => {
                let mut keys: Vec<String> = Vec::new();
                let mut coords = Vec::new();
                let spl = self.compile_spline(s, &mut keys, &mut coords)?;
                S::Spline(spl, coords)
            }
            Df::BlendAlpha => S::Const(1.0),
            Df::BlendOffset | Df::Beardifier => S::Const(0.0),
            Df::BlendDensity(x) => self.compile(x)?,
            Df::Prepared(id, _) => {
                let input = self.compiled[&format!("#cache{id}")].clone();
                S::Cached(*id, input)
            }
        })
    }

    fn compile_spline(
        &mut self,
        s: &SplineDf,
        keys: &mut Vec<String>,
        coords: &mut Vec<S>,
    ) -> Result<Spl, String> {
        Ok(match s {
            SplineDf::Const(v) => Spl::Const(*v),
            SplineDf::Multi {
                coord,
                locs,
                values,
                ders,
            } => {
                let key = format!("{coord:?}");
                let index = match keys.iter().position(|k| *k == key) {
                    Some(i) => i,
                    None => {
                        keys.push(key);
                        coords.push(self.compile(coord)?);
                        coords.len() - 1
                    }
                };
                Spl::Multi {
                    coord: index,
                    locs: locs.clone(),
                    values: values
                        .iter()
                        .map(|v| self.compile_spline(v, keys, coords))
                        .collect::<Result<_, _>>()?,
                    ders: ders.clone(),
                }
            }
        })
    }
}

// ------------------------------------------------------------------ сэмплинг

/// `Math.min(float, float)`: NaN заразен, −0 меньше +0.
fn jmin(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        a
    } else if a == 0.0 && b == 0.0 && b.to_bits() != 0 {
        b
    } else if a <= b {
        a
    } else {
        b
    }
}

fn jmax(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        a
    } else if a == 0.0 && b == 0.0 && a.to_bits() != 0 {
        b
    } else if a >= b {
        a
    } else {
        b
    }
}

/// `Mth.clamp(float)`.
fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo { lo } else { jmin(v, hi) }
}

fn signum(x: f32) -> f32 {
    if x == 0.0 || x.is_nan() {
        x
    } else {
        1f32.copysign(x)
    }
}

fn unary(u: Unary, x: f32) -> f32 {
    match u {
        Unary::Abs => x.abs(),
        Unary::Square => x * x,
        Unary::Cube => x * x * x,
        Unary::Sqrt => (x as f64).sqrt() as f32,
        Unary::HalfNegative => {
            if x > 0.0 {
                x
            } else {
                x * 0.5
            }
        }
        Unary::QuarterNegative => {
            if x > 0.0 {
                x
            } else {
                x * 0.25
            }
        }
        Unary::Reciprocal => 1.0 / x,
        Unary::Negate => -x,
        Unary::Squeeze => {
            let c = clampf(x, -1.0, 1.0);
            c / 2.0 - c * c * c / 24.0
        }
        Unary::Log => (x as f64).ln() as f32,
        Unary::Sign => signum(x),
    }
}

fn binary(op: Bin, l: f32, r: f32) -> f32 {
    match op {
        Bin::Add => l + r,
        Bin::Sub => l + -r,
        Bin::Mul => l * r,
        Bin::Div => l / r,
        Bin::Min => {
            if r < l {
                r
            } else {
                l
            }
        }
        Bin::Max => {
            if r > l {
                r
            } else {
                l
            }
        }
    }
}

impl Grad {
    fn compute(&self, coord: i32, from_value: f32, factor: f32) -> f32 {
        match *self {
            Grad::Clamp { from, min, max } => {
                from_value + (coord.clamp(min, max) - from) as f32 * factor
            }
            Grad::Repeat { from, range } => {
                from_value + (coord - from).rem_euclid(range) as f32 * factor
            }
            Grad::Mirrored { from, range } => {
                let rel = coord - from;
                let tile = rel.div_euclid(range);
                let local = rel - tile * range;
                if tile & 1 == 0 {
                    from_value + local as f32 * factor
                } else {
                    from_value + (range - local) as f32 * factor
                }
            }
        }
    }
}

fn spline_eval(s: &Spl, coord: &mut dyn FnMut(usize) -> f32) -> f32 {
    match s {
        Spl::Const(v) => *v,
        Spl::Multi {
            coord: c,
            locs,
            values,
            ders,
        } => {
            let input = coord(*c);
            // `Mth.binarySearch(0, n, i -> input < locs[i]) - 1`
            let start = locs.partition_point(|&l| !(input < l)) as isize - 1;
            let last = locs.len() - 1;
            let extend = |value: f32, i: usize| {
                let d = ders[i];
                if d == 0.0 {
                    value
                } else {
                    value + d * (input - locs[i])
                }
            };
            if start < 0 {
                let v = spline_eval(&values[0], coord);
                extend(v, 0)
            } else if start as usize == last {
                let v = spline_eval(&values[last], coord);
                extend(v, last)
            } else {
                let s = start as usize;
                let (x1, x2) = (locs[s], locs[s + 1]);
                let t = (input - x1) / (x2 - x1);
                let (d1, d2) = (ders[s], ders[s + 1]);
                let y1 = spline_eval(&values[s], coord);
                let y2 = spline_eval(&values[s + 1], coord);
                let a = d1 * (x2 - x1) - (y2 - y1);
                let b = -d2 * (x2 - x1) + (y2 - y1);
                lerp(t, y1, y2) + t * (1.0 - t) * lerp(t, a, b)
            }
        }
    }
}

impl S {
    /// Значение по точке совпадает с объёмом бит в бит: в дереве нет
    /// интерполяции, кэшей, срезов и `shift_b`, которые считают объём иначе.
    pub fn pointwise(&self) -> bool {
        match self {
            S::Const(_) | S::Noise(..) | S::Gradient(..) => true,
            S::ConstAdd(s, _)
            | S::ConstSub(_, s)
            | S::ConstMul(s, _)
            | S::ConstDiv(_, s)
            | S::ConstMin(s, _)
            | S::ConstMax(s, _)
            | S::Unary(_, s)
            | S::Clamp(s, ..) => s.pointwise(),
            S::Binary(_, ch) => ch.iter().all(S::pointwise),
            S::RangeChoice(ch, ..) | S::Lerp(ch) => ch.iter().all(S::pointwise),
            _ => false,
        }
    }

    /// Диапазон `y`, вне которого плотность равна константе, и эта константа.
    ///
    /// `None` — плотность зависит от `y` везде, сэмплировать нужно всё.
    ///
    /// Жилы руды (`ore_vein`) считаются объёмом на весь чанк, но их плотность
    /// из данных — `range_choice` по `minecraft:y`: выше 50 блоков она равна
    /// −1.0. Считать там дерево незачем: это 7 мс из 20 на чанк.
    pub fn y_constant_outside(&self) -> Option<(i32, i32, f32)> {
        match self {
            // `minecraft:y` — градиент по y с шагом 1: констант нет.
            S::Gradient(axis, ..) if *axis == 1 => None,
            S::Const(v) => Some((i32::MIN, i32::MAX, *v)),
            S::Noise(..)
            | S::ShiftedXz(..)
            | S::ShiftedXyz(..)
            | S::ShiftB(..)
            | S::Spline(..)
            | S::FindTop(..)
            | S::Interpolated(..)
            | S::Cached(..) => None,
            // Ветки, которые не меняют `y`-зависимость.
            S::ConstAdd(s, _)
            | S::ConstSub(_, s)
            | S::ConstMul(s, _)
            | S::ConstDiv(_, s)
            | S::ConstMin(s, _)
            | S::ConstMax(s, _)
            | S::Unary(_, s)
            | S::Clamp(s, ..)
            | S::SliceX(s, _)
            | S::SliceZ(s, _) => s.y_constant_outside(),
            S::SliceXz(s, ..) => s.y_constant_outside(),
            // Срез по y фиксирует ось: значение постоянно вне одной точки,
            // но сэмплировать всё равно нужно только её — объёмом это
            // выражается «нигде», поэтому такой узел считаем переменным.
            S::SliceY(..) => None,
            // Вне диапазона выбирается ветка «когда вне». Она должна быть
            // постоянной **и** не зависеть от `y`: `Some` от
            // `y_constant_outside` у константы, у шума — `None`, поэтому
            // проверка ниже отсекает ветки, которые меняются по x/z.
            S::RangeChoice(ch, lo, hi) => match ch[2].y_constant_outside() {
                // `(MIN, MAX, v)` — настоящая константа; диапазон от ветки —
                // значит она сама зависит от y внутри своей полосы, и вне
                // общей полосы поведение уже не «константа везде».
                Some((i32::MIN, i32::MAX, v)) => {
                    // `hi` исключается: вне `[lo, hi)` значение равно `v`.
                    Some((lo.floor() as i32, hi.ceil() as i32, v))
                }
                _ => None,
            },
            _ => None,
        }
    }

    pub fn value(&self, ctx: &mut Ctx, x: i32, y: i32, z: i32) -> f32 {
        match self {
            S::Const(v) => *v,
            S::Noise(n, xz, ys) => n.get(x as f64 * xz, y as f64 * ys, z as f64 * xz),
            S::ShiftedXz(sh, n, xz, ys) => {
                let px = x as f64 * xz + sh[0].value(ctx, x, y, z) as f64;
                let py = y as f64 * ys;
                let pz = z as f64 * xz + sh[1].value(ctx, x, y, z) as f64;
                n.get(px, py, pz)
            }
            S::ShiftedXyz(sh, n, xz, ys) => {
                let px = x as f64 * xz + sh[0].value(ctx, x, y, z) as f64;
                let py = y as f64 * ys + sh[1].value(ctx, x, y, z) as f64;
                let pz = z as f64 * xz + sh[2].value(ctx, x, y, z) as f64;
                n.get(px, py, pz)
            }
            S::ShiftB(n) => n.get(z as f64 * 0.25, x as f64 * 0.25, 0.0) * 4.0,
            S::ConstAdd(s, v) => s.value(ctx, x, y, z) + v,
            S::ConstSub(v, s) => v - s.value(ctx, x, y, z),
            S::ConstMul(s, v) => s.value(ctx, x, y, z) * v,
            S::ConstDiv(v, s) => v / s.value(ctx, x, y, z),
            S::ConstMin(s, v) => jmin(s.value(ctx, x, y, z), *v),
            S::ConstMax(s, v) => jmax(s.value(ctx, x, y, z), *v),
            S::Binary(op, ch) => {
                let l = ch[0].value(ctx, x, y, z);
                match op {
                    Bin::Add => l + ch[1].value(ctx, x, y, z),
                    Bin::Sub => l - ch[1].value(ctx, x, y, z),
                    Bin::Mul if l == 0.0 => 0.0,
                    Bin::Mul => l * ch[1].value(ctx, x, y, z),
                    Bin::Div if l == 0.0 => 0.0,
                    Bin::Div => l / ch[1].value(ctx, x, y, z),
                    Bin::Min => jmin(l, ch[1].value(ctx, x, y, z)),
                    Bin::Max => jmax(l, ch[1].value(ctx, x, y, z)),
                }
            }
            S::Unary(u, s) => unary(*u, s.value(ctx, x, y, z)),
            S::Clamp(s, lo, hi) => clampf(s.value(ctx, x, y, z), *lo, *hi),
            S::RangeChoice(ch, lo, hi) => {
                let v = ch[0].value(ctx, x, y, z);
                if v >= *lo && v < *hi {
                    ch[1].value(ctx, x, y, z)
                } else {
                    ch[2].value(ctx, x, y, z)
                }
            }
            S::IntervalSelect(input, t, fs) => {
                let v = input.value(ctx, x, y, z);
                fs[select(t, fs.len(), v)].value(ctx, x, y, z)
            }
            S::Lerp(ch) => {
                let a = ch[0].value(ctx, x, y, z);
                if a == 0.0 {
                    ch[1].value(ctx, x, y, z)
                } else if a == 1.0 {
                    ch[2].value(ctx, x, y, z)
                } else {
                    let f = ch[1].value(ctx, x, y, z);
                    let s = ch[2].value(ctx, x, y, z);
                    lerp(a, f, s)
                }
            }
            S::Gradient(axis, g, from, factor) => g.compute([x, y, z][*axis], *from, *factor),
            S::Interpolated(input, cxz, cy) => {
                let xi = x.rem_euclid(*cxz);
                let yi = y.rem_euclid(*cy);
                let zi = z.rem_euclid(*cxz);
                if xi == 0 && yi == 0 && zi == 0 {
                    return input.value(ctx, x, y, z);
                }
                let v = Volume::new([2, 2, 2], [x - xi, y - yi, z - zi], [*cxz, *cy, *cxz]);
                // Стек вместо `vec![0f32; 8]`: вызов идёт на каждую клетку.
                let mut b = [0f32; 8];
                input.volume(ctx, &mut b, &v);
                lerp3(
                    xi as f32 / *cxz as f32,
                    yi as f32 / *cy as f32,
                    zi as f32 / *cxz as f32,
                    b[v.index(0, 0, 0)],
                    b[v.index(1, 0, 0)],
                    b[v.index(0, 1, 0)],
                    b[v.index(1, 1, 0)],
                    b[v.index(0, 0, 1)],
                    b[v.index(1, 0, 1)],
                    b[v.index(0, 1, 1)],
                    b[v.index(1, 1, 1)],
                )
            }
            S::SliceX(s, c) => s.value(ctx, *c, y, z),
            S::SliceY(s, c) => s.value(ctx, x, *c, z),
            S::SliceZ(s, c) => s.value(ctx, x, y, *c),
            S::SliceXz(s, cx, cz) => s.value(ctx, *cx, y, *cz),
            S::FindTop(ch, lo, h) => {
                let upper = ch[1].value(ctx, x, y, z);
                find_top(&ch[0], ctx, x, z, upper, *lo, *h)
            }
            S::Spline(spl, coords) => {
                let mut memo = vec![f32::NAN; coords.len()];
                spline_eval(spl, &mut |i| {
                    if memo[i].is_nan() {
                        memo[i] = coords[i].value(ctx, x, y, z);
                    }
                    memo[i]
                })
            }
            S::Cached(id, input) => {
                let cell = ctx.cell(*id);
                if cell.key == (x, y, z) && !cell.value.is_nan() {
                    return cell.value;
                }
                if let Some(vol) = cell.vol
                    && let Some(i) = vol.index_of(x, y, z)
                {
                    return cell.buf[i];
                }
                let v = input.value(ctx, x, y, z);
                let cell = ctx.cell(*id);
                cell.key = (x, y, z);
                cell.value = v;
                v
            }
        }
    }

    /// `sampleVolume`: значения всей решётки в `out` (длина `v.len()`).
    pub fn volume(&self, ctx: &mut Ctx, out: &mut [f32], v: &Volume) {
        let n = v.len();
        match self {
            S::Const(c) => out.fill(*c),
            S::Noise(noise, xz, ys) => {
                out.fill(0.0);
                noise.add_to_volume(out, v, *xz, *ys, 1.0);
            }
            S::ShiftedXz(sh, noise, xz, ys) => {
                sh[0].volume(ctx, out, v);
                let mut bz = vec![0f32; n];
                sh[1].volume(ctx, &mut bz, v);
                let mut i = 0;
                for z in 0..v.size[2] {
                    let base_z = v.block_z(z) as f64 * xz;
                    for x in 0..v.size[0] {
                        let base_x = v.block_x(x) as f64 * xz;
                        for y in 0..v.size[1] {
                            let px = base_x + out[i] as f64;
                            let py = v.block_y(y) as f64 * ys;
                            let pz = base_z + bz[i] as f64;
                            out[i] = noise.get(px, py, pz);
                            i += 1;
                        }
                    }
                }
            }
            S::ShiftedXyz(sh, noise, xz, ys) => {
                sh[0].volume(ctx, out, v);
                let mut by = vec![0f32; n];
                sh[1].volume(ctx, &mut by, v);
                let mut bz = vec![0f32; n];
                sh[2].volume(ctx, &mut bz, v);
                let mut i = 0;
                for z in 0..v.size[2] {
                    let base_z = v.block_z(z) as f64 * xz;
                    for x in 0..v.size[0] {
                        let base_x = v.block_x(x) as f64 * xz;
                        for y in 0..v.size[1] {
                            let px = base_x + out[i] as f64;
                            let py = v.block_y(y) as f64 * ys + by[i] as f64;
                            let pz = base_z + bz[i] as f64;
                            out[i] = noise.get(px, py, pz);
                            i += 1;
                        }
                    }
                }
            }
            S::ShiftB(noise) => {
                let t = Volume::new(
                    [v.size[2], v.size[0], 1],
                    [v.min[2], v.min[0], 0],
                    [v.step[2], v.step[0], 1],
                );
                let mut tb = vec![0f32; t.len()];
                noise.add_to_volume(&mut tb, &t, 0.25, 0.25, 4.0);
                let sy = v.size[1] as usize;
                for z in 0..v.size[2] {
                    for x in 0..v.size[0] {
                        let val = tb[t.index(z, x, 0)];
                        let at = v.index(x, 0, z);
                        out[at..at + sy].fill(val);
                    }
                }
            }
            S::ConstAdd(s, c) => {
                s.volume(ctx, out, v);
                out.iter_mut().for_each(|o| *o += c);
            }
            S::ConstSub(c, s) => {
                s.volume(ctx, out, v);
                out.iter_mut().for_each(|o| *o = c - *o);
            }
            S::ConstMul(s, c) => {
                s.volume(ctx, out, v);
                out.iter_mut().for_each(|o| *o *= c);
            }
            S::ConstDiv(c, s) => {
                s.volume(ctx, out, v);
                out.iter_mut().for_each(|o| *o = c / *o);
            }
            S::ConstMin(s, c) => {
                s.volume(ctx, out, v);
                out.iter_mut().for_each(|o| {
                    if *c < *o {
                        *o = *c
                    }
                });
            }
            S::ConstMax(s, c) => {
                s.volume(ctx, out, v);
                out.iter_mut().for_each(|o| {
                    if *c > *o {
                        *o = *c
                    }
                });
            }
            S::Binary(op, ch) => {
                ch[0].volume(ctx, out, v);
                let mut r = vec![0f32; n];
                ch[1].volume(ctx, &mut r, v);
                for (o, r) in out.iter_mut().zip(&r) {
                    *o = binary(*op, *o, *r);
                }
            }
            S::Unary(u, s) => {
                s.volume(ctx, out, v);
                out.iter_mut().for_each(|o| *o = unary(*u, *o));
            }
            S::Clamp(s, lo, hi) => {
                s.volume(ctx, out, v);
                out.iter_mut().for_each(|o| *o = clampf(*o, *lo, *hi));
            }
            S::RangeChoice(ch, lo, hi) => {
                ch[1].volume(ctx, out, v);
                let mut input = vec![0f32; n];
                ch[0].volume(ctx, &mut input, v);
                let mut other = vec![0f32; n];
                ch[2].volume(ctx, &mut other, v);
                for i in 0..n {
                    let x = input[i];
                    if !(x >= *lo) || !(x < *hi) {
                        out[i] = other[i];
                    }
                }
            }
            S::IntervalSelect(input, t, fs) => {
                input.volume(ctx, out, v);
                let bufs: Vec<Vec<f32>> = fs
                    .iter()
                    .map(|f| {
                        let mut b = vec![0f32; n];
                        f.volume(ctx, &mut b, v);
                        b
                    })
                    .collect();
                for i in 0..n {
                    out[i] = bufs[select(t, fs.len(), out[i])][i];
                }
            }
            S::Lerp(ch) => {
                ch[0].volume(ctx, out, v);
                let mut first = vec![0f32; n];
                ch[1].volume(ctx, &mut first, v);
                let mut second = vec![0f32; n];
                ch[2].volume(ctx, &mut second, v);
                for i in 0..n {
                    let a = out[i];
                    out[i] = if a == 0.0 {
                        first[i]
                    } else if a == 1.0 {
                        second[i]
                    } else {
                        lerp(a, first[i], second[i])
                    };
                }
            }
            S::Gradient(axis, g, from, factor) => {
                let mut i = 0;
                for z in 0..v.size[2] {
                    for x in 0..v.size[0] {
                        for y in 0..v.size[1] {
                            let c = [v.block_x(x), v.block_y(y), v.block_z(z)][*axis];
                            out[i] = g.compute(c, *from, *factor);
                            i += 1;
                        }
                    }
                }
            }
            S::Interpolated(input, cxz, cy) => interpolate(input, *cxz, *cy, ctx, out, v),
            S::SliceX(s, c) => {
                if v.size[0] == 1 && v.min[0] == *c {
                    return s.volume(ctx, out, v);
                }
                let iv = Volume::new([1, v.size[1], v.size[2]], [*c, v.min[1], v.min[2]], v.step);
                let mut b = vec![0f32; iv.len()];
                s.volume(ctx, &mut b, &iv);
                let mut i = 0;
                for z in 0..v.size[2] {
                    for _ in 0..v.size[0] {
                        for y in 0..v.size[1] {
                            out[i] = b[iv.index(0, y, z)];
                            i += 1;
                        }
                    }
                }
            }
            S::SliceY(s, c) => {
                if v.size[1] == 1 && v.min[1] == *c {
                    return s.volume(ctx, out, v);
                }
                let iv = Volume::new([v.size[0], 1, v.size[2]], [v.min[0], *c, v.min[2]], v.step);
                let mut b = vec![0f32; iv.len()];
                s.volume(ctx, &mut b, &iv);
                let sy = v.size[1] as usize;
                for z in 0..v.size[2] {
                    for x in 0..v.size[0] {
                        let at = v.index(x, 0, z);
                        out[at..at + sy].fill(b[iv.index(x, 0, z)]);
                    }
                }
            }
            S::SliceZ(s, c) => {
                if v.size[2] == 1 && v.min[2] == *c {
                    return s.volume(ctx, out, v);
                }
                let iv = Volume::new([v.size[0], v.size[1], 1], [v.min[0], v.min[1], *c], v.step);
                let mut b = vec![0f32; iv.len()];
                s.volume(ctx, &mut b, &iv);
                let mut i = 0;
                for _ in 0..v.size[2] {
                    for x in 0..v.size[0] {
                        for y in 0..v.size[1] {
                            out[i] = b[iv.index(x, y, 0)];
                            i += 1;
                        }
                    }
                }
            }
            S::SliceXz(s, cx, cz) => {
                if v.size[0] == 1 && v.size[2] == 1 && v.min[0] == *cx && v.min[2] == *cz {
                    return s.volume(ctx, out, v);
                }
                let iv = Volume::new([1, v.size[1], 1], [*cx, v.min[1], *cz], v.step);
                let mut b = vec![0f32; iv.len()];
                s.volume(ctx, &mut b, &iv);
                for y in 0..v.size[1] {
                    let val = b[iv.index(0, y, 0)];
                    let mut at = v.index(0, y, 0);
                    for _ in 0..v.size[0] * v.size[2] {
                        out[at] = val;
                        at += v.size[1] as usize;
                    }
                }
            }
            S::FindTop(ch, lo, h) => {
                assert_eq!(v.size[1], 1, "find_top_surface: sizeY must be 1");
                ch[1].volume(ctx, out, v);
                let mut i = 0;
                for z in 0..v.size[2] {
                    let bz = v.block_z(z);
                    for x in 0..v.size[0] {
                        let bx = v.block_x(x);
                        out[i] = find_top(&ch[0], ctx, bx, bz, out[i], *lo, *h);
                        i += 1;
                    }
                }
            }
            S::Spline(spl, coords) => {
                let mut bufs: Vec<Option<Vec<f32>>> = (0..coords.len()).map(|_| None).collect();
                for (i, o) in out.iter_mut().enumerate() {
                    *o = spline_eval(spl, &mut |c| {
                        if bufs[c].is_none() {
                            let mut b = vec![0f32; n];
                            coords[c].volume(ctx, &mut b, v);
                            bufs[c] = Some(b);
                        }
                        bufs[c].as_ref().unwrap()[i]
                    });
                }
            }
            S::Cached(id, input) => {
                let cell = ctx.cell(*id);
                if cell.vol != Some(*v) {
                    let mut buf = std::mem::take(&mut cell.buf);
                    cell.vol = None;
                    buf.clear();
                    buf.resize(n, 0.0);
                    input.volume(ctx, &mut buf, v);
                    let cell = ctx.cell(*id);
                    cell.buf = buf;
                    cell.vol = Some(*v);
                }
                out.copy_from_slice(&ctx.cell(*id).buf);
            }
        }
    }
}

fn select(t: &[f32], count: usize, v: f32) -> usize {
    t.iter().position(|&th| v < th).unwrap_or(count - 1)
}

fn find_top(density: &S, ctx: &mut Ctx, x: i32, z: i32, upper: f32, lo: i32, h: i32) -> f32 {
    let top = (upper / h as f32).floor() as i32 * h;
    if top <= lo {
        return lo as f32;
    }
    let mut y = top;
    while y >= lo {
        if density.value(ctx, x, y, z) > 0.0 {
            return y as f32;
        }
        y -= h;
    }
    lo as f32
}

/// `InterpolatedFunction.Sampler.sampleVolume`.
fn interpolate(input: &S, cxz: i32, cy: i32, ctx: &mut Ctx, out: &mut [f32], v: &Volume) {
    let aligned = (v.step[0] == cxz || v.size[0] == 1)
        && (v.step[1] == cy || v.size[1] == 1)
        && (v.step[2] == cxz || v.size[2] == 1)
        && v.min[0].rem_euclid(cxz) == 0
        && v.min[1].rem_euclid(cy) == 0
        && v.min[2].rem_euclid(cxz) == 0;
    if aligned {
        return input.volume(ctx, out, v);
    }
    if v.step == [1, 1, 1] {
        return interpolate_blocks(input, cxz, cy, ctx, out, v);
    }
    let bv = Volume::blocks(
        [
            v.size[0] * v.step[0],
            v.size[1] * v.step[1],
            v.size[2] * v.step[2],
        ],
        v.min,
    );
    let mut b = vec![0f32; bv.len()];
    interpolate_blocks(input, cxz, cy, ctx, &mut b, &bv);
    for z in 0..v.size[2] {
        for x in 0..v.size[0] {
            for y in 0..v.size[1] {
                out[v.index(x, y, z)] = b[bv.index(x * v.step[0], y * v.step[1], z * v.step[2])];
            }
        }
    }
}

fn interpolate_blocks(input: &S, cxz: i32, cy: i32, ctx: &mut Ctx, out: &mut [f32], v: &Volume) {
    let min_cell = [
        v.min[0].div_euclid(cxz),
        v.min[1].div_euclid(cy),
        v.min[2].div_euclid(cxz),
    ];
    let max = [v.max_block(0), v.max_block(1), v.max_block(2)];
    let max_cell = [
        max[0].div_euclid(cxz),
        max[1].div_euclid(cy),
        max[2].div_euclid(cxz),
    ];
    let count = [
        max_cell[0] - min_cell[0] + 1,
        max_cell[1] - min_cell[1] + 1,
        max_cell[2] - min_cell[2] + 1,
    ];
    let cs = [cxz, cy, cxz];
    let size = [0, 1, 2].map(|a| {
        if max[a].rem_euclid(cs[a]) == 0 {
            count[a]
        } else {
            count[a] + 1
        }
    });
    let cv = Volume::new(
        size,
        [min_cell[0] * cxz, min_cell[1] * cy, min_cell[2] * cxz],
        cs,
    );
    let mut cb = vec![0f32; cv.len()];
    input.volume(ctx, &mut cb, &cv);
    let inv_xz = 1.0 / cxz as f32;
    let inv_y = 1.0 / cy as f32;
    for cz in 0..count[2] {
        let nz = (cz + 1).min(cv.size[2] - 1);
        for cx in 0..count[0] {
            let nx = (cx + 1).min(cv.size[0] - 1);
            let mut v000 = cb[cv.index(cx, 0, cz)];
            let mut v100 = cb[cv.index(nx, 0, cz)];
            let mut v001 = cb[cv.index(cx, 0, nz)];
            let mut v101 = cb[cv.index(nx, 0, nz)];
            for cyi in 0..count[1] {
                let ny = (cyi + 1).min(cv.size[1] - 1);
                let v010 = cb[cv.index(cx, ny, cz)];
                let v110 = cb[cv.index(nx, ny, cz)];
                let v011 = cb[cv.index(cx, ny, nz)];
                let v111 = cb[cv.index(nx, ny, nz)];
                // fillCell
                let ox = cv.block_x(cx) - v.min[0];
                let oy = cv.block_y(cyi) - v.min[1];
                let oz = cv.block_z(cz) - v.min[2];
                let x0 = (-ox).max(0);
                let y0 = (-oy).max(0);
                let z0 = (-oz).max(0);
                let x1 = cxz.min(v.size[0] - ox) - 1;
                let y1 = cy.min(v.size[1] - oy) - 1;
                let z1 = cxz.min(v.size[2] - oz) - 1;
                for z in z0..=z1 {
                    let az = z as f32 * inv_xz;
                    let v00 = lerp(az, v000, v001);
                    let v01 = lerp(az, v010, v011);
                    let v10 = lerp(az, v100, v101);
                    let v11 = lerp(az, v110, v111);
                    for x in x0..=x1 {
                        let ax = x as f32 * inv_xz;
                        let a0 = lerp(ax, v00, v10);
                        let a1 = lerp(ax, v01, v11);
                        let step = (a1 - a0) * inv_y;
                        let mut val = a0 + step * y0 as f32;
                        let mut at = v.index(ox + x, oy + y0, oz + z);
                        for _ in y0..=y1 {
                            out[at] = val;
                            at += 1;
                            val += step;
                        }
                    }
                }
                v000 = v010;
                v100 = v110;
                v001 = v011;
                v101 = v111;
            }
        }
    }
}
