//! Фичи Края: острова, шипы, платформа, шлюз и хорус.
//! Кристаллы Края не пишутся (сущности генерации), но огонь и бедрок под ними
//! и `nextFloat` их поворота — как у ванили.

use super::{Features, Level, Palette};
use crate::random::{Bits, Legacy, Worldgen};
use chunkwake_world::nbt::{Compound, Tag};
use std::sync::Arc;

type Pos = [i32; 3];

/// `WorldGenRegion.setBlock`: вне радиуса записи блок не ставится.
fn put(level: &mut dyn Level, [x, y, z]: Pos, state: u32) {
    if level.can_write(x, z) {
        level.set(x, y, z, state);
    }
}

fn named(pal: &Palette, name: &str) -> Result<u32, String> {
    pal.state(&serde_json::Value::String(name.into()))
}

/// Состояния блоков Края, нужные фичам.
#[derive(Debug)]
pub(crate) struct Blocks {
    air: u32,
    end_stone: u32,
    obsidian: u32,
    bedrock: u32,
    fire: u32,
    iron_bars: u32,
    gateway: u32,
    plant: u32,
    flower: u32,
    supports_chorus: Arc<Vec<bool>>,
}

impl Blocks {
    pub(crate) fn load(pal: &mut Palette) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            air: named(pal, "minecraft:air")?,
            end_stone: named(pal, "minecraft:end_stone")?,
            obsidian: named(pal, "minecraft:obsidian")?,
            bedrock: named(pal, "minecraft:bedrock")?,
            fire: named(pal, "minecraft:fire")?,
            iron_bars: named(pal, "minecraft:iron_bars")?,
            gateway: named(pal, "minecraft:end_gateway")?,
            plant: named(pal, "minecraft:chorus_plant")?,
            flower: named(pal, "minecraft:chorus_flower")?,
            supports_chorus: pal.tag("minecraft:supports_chorus_plant")?,
        }))
    }
}

/// `EndIslandFeature`: перевёрнутый конус из камня Края.
pub(crate) fn island(b: &Blocks, level: &mut dyn Level, r: &mut Worldgen, origin: Pos) {
    let mut size = r.next_int_bound(3) as f32 + 4.0;
    let mut y = 0;
    while size > 0.5 {
        let lo = (-size).floor() as i32;
        let hi = size.ceil() as i32;
        for x in lo..=hi {
            for z in lo..=hi {
                if ((x * x + z * z) as f32) <= (size + 1.0) * (size + 1.0) {
                    put(
                        level,
                        [origin[0] + x, origin[1] + y, origin[2] + z],
                        b.end_stone,
                    );
                }
            }
        }
        size -= r.next_int_bound(2) as f32 + 0.5;
        y -= 1;
    }
}

/// `EndPlatformFeature.createEndPlatform`: обсидиан 5×5 и воздух над ним.
pub(crate) fn platform(b: &Blocks, pal: &Palette, level: &mut dyn Level, origin: Pos) {
    for dz in -2..=2 {
        for dx in -2..=2 {
            for dy in -1..3 {
                let p = [origin[0] + dx, origin[1] + dy, origin[2] + dz];
                let want = if dy == -1 { b.obsidian } else { b.air };
                if !pal.same_block(level.get(p[0], p[1], p[2]), want) {
                    put(level, p, want);
                }
            }
        }
    }
}

/// `end_gateway`: точка выхода и `exact`.
#[derive(Clone, Debug)]
pub(crate) struct Gateway {
    exit: Option<Pos>,
    exact: bool,
}

impl Gateway {
    pub(crate) fn parse(v: &serde_json::Value) -> Result<Self, String> {
        let exit = match v.get("exit") {
            Some(e) if !e.is_null() => {
                let a = e.as_array().ok_or("end_gateway: exit")?;
                let c = |i: usize| a.get(i).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                Some([c(0), c(1), c(2)])
            }
            _ => None,
        };
        Ok(Self {
            exit,
            exact: v["exact"].as_bool().unwrap_or(false),
        })
    }

    /// `EndGatewayFeature.place`: шлюз в бедроковой оправе 3×5×3.
    pub(crate) fn place(&self, b: &Blocks, level: &mut dyn Level, origin: Pos) {
        // `betweenClosed`: x быстрее всех, затем y, затем z.
        for z in origin[2] - 1..=origin[2] + 1 {
            for y in origin[1] - 2..=origin[1] + 2 {
                for x in origin[0] - 1..=origin[0] + 1 {
                    let same_x = x == origin[0];
                    let same_y = y == origin[1];
                    let same_z = z == origin[2];
                    let end = (y - origin[1]).abs() == 2;
                    let state = if same_x && same_y && same_z {
                        put(level, [x, y, z], b.gateway);
                        if level.can_write(x, z) {
                            let mut nbt = Compound::new();
                            nbt.insert("Age".into(), Tag::Long(0));
                            if let Some(e) = self.exit {
                                nbt.insert("exit_portal".into(), Tag::IntArray(e.to_vec()));
                                if self.exact {
                                    nbt.insert("ExactTeleport".into(), Tag::Byte(1));
                                }
                            }
                            level.set_block_entity(x, y, z, "minecraft:end_gateway", nbt);
                        }
                        continue;
                    } else if same_y {
                        b.air
                    } else if end && same_x && same_z {
                        b.bedrock
                    } else if (same_x || same_z) && !end {
                        b.bedrock
                    } else {
                        b.air
                    };
                    put(level, [x, y, z], state);
                }
            }
        }
    }
}

/// `EndSpikeFeature.EndSpike`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spike {
    pub center_x: i32,
    pub center_z: i32,
    pub radius: i32,
    pub height: i32,
    pub guarded: bool,
}

/// `EndSpikeFeature.getSpikesForLevel`: десять шипов по кругу радиуса 42,
/// размеры перемешаны генератором от сида мира.
pub fn spikes_for_seed(seed: i64) -> Vec<Spike> {
    let key = Legacy::new(seed).next_long() & 65535;
    // `Util.toShuffledList(IntStream.range(0, 10), random)`.
    let mut r = Legacy::new(key);
    let mut sizes: Vec<i32> = (0..10).collect();
    let mut i = sizes.len();
    while i > 1 {
        let j = r.next_int_bound(i as i32) as usize;
        sizes.swap(i - 1, j);
        i -= 1;
    }
    (0..10)
        .map(|i| {
            let a = 2.0 * (-std::f64::consts::PI + (std::f64::consts::PI / 10.0) * i as f64);
            let size = sizes[i];
            Spike {
                center_x: (42.0 * a.cos()).floor() as i32,
                center_z: (42.0 * a.sin()).floor() as i32,
                radius: 2 + size / 3,
                height: 76 + size * 3,
                guarded: size == 1 || size == 2,
            }
        })
        .collect()
}

/// `end_spike`: свой список шипов или шипы мира.
#[derive(Clone, Debug)]
pub(crate) struct Spikes {
    spikes: Vec<Spike>,
}

impl Spikes {
    pub(crate) fn parse(v: &serde_json::Value) -> Result<Self, String> {
        let mut spikes = Vec::new();
        for s in v["spikes"].as_array().ok_or("end_spike: no spikes")? {
            let i = |k: &str| s[k].as_i64().unwrap_or(0) as i32;
            spikes.push(Spike {
                center_x: i("centerX"),
                center_z: i("centerZ"),
                radius: i("radius"),
                height: i("height"),
                guarded: s["guarded"].as_bool().unwrap_or(false),
            });
        }
        Ok(Self { spikes })
    }

    pub(crate) fn place(
        &self,
        f: &Features,
        b: &Blocks,
        level: &mut dyn Level,
        r: &mut Worldgen,
        origin: Pos,
    ) {
        let own;
        let spikes = if self.spikes.is_empty() {
            own = spikes_for_seed(f.seed);
            &own
        } else {
            &self.spikes
        };
        for s in spikes {
            if origin[0] >> 4 == s.center_x >> 4 && origin[2] >> 4 == s.center_z >> 4 {
                spike(&f.palette, b, level, r, s);
            }
        }
    }
}

/// `EndSpikeFeature.placeSpike`.
fn spike(pal: &Palette, b: &Blocks, level: &mut dyn Level, r: &mut Worldgen, s: &Spike) {
    let rad = s.radius;
    let limit = (rad * rad + 1) as i64;
    for z in s.center_z - rad..=s.center_z + rad {
        for y in level.min_y()..=s.height + 10 {
            for x in s.center_x - rad..=s.center_x + rad {
                let dx = (x - s.center_x) as i64;
                let dz = (z - s.center_z) as i64;
                if dx * dx + dz * dz <= limit && y < s.height {
                    put(level, [x, y, z], b.obsidian);
                } else if y > 65 {
                    put(level, [x, y, z], b.air);
                }
            }
        }
    }
    if s.guarded {
        for dx in -2..=2i32 {
            for dz in -2..=2i32 {
                for dy in 0..=3 {
                    let x_side = dx.abs() == 2;
                    let z_side = dz.abs() == 2;
                    let top = dy == 3;
                    if x_side || z_side || top {
                        let x_edge = dx == -2 || dx == 2 || top;
                        let z_edge = dz == -2 || dz == 2 || top;
                        let flag = |v: bool| if v { "true" } else { "false" };
                        let mut st = b.iron_bars;
                        for (k, v) in [
                            ("north", x_edge && dz != -2),
                            ("south", x_edge && dz != 2),
                            ("west", z_edge && dx != -2),
                            ("east", z_edge && dx != 2),
                        ] {
                            st = pal.with_prop(st, k, flag(v)).unwrap_or(st);
                        }
                        put(level, [s.center_x + dx, s.height + dy, s.center_z + dz], st);
                    }
                }
            }
        }
    }
    // Кристалл: поворот `nextFloat() * 360`, бедрок под ним и огонь на месте.
    r.next_float();
    put(level, [s.center_x, s.height, s.center_z], b.bedrock);
    put(level, [s.center_x, s.height + 1, s.center_z], b.fire);
}

/// `ChorusPlantFeature`: растение на камне Края.
pub(crate) fn chorus(
    pal: &Palette,
    b: &Blocks,
    level: &mut dyn Level,
    r: &mut Worldgen,
    origin: Pos,
) {
    let below = level.get(origin[0], origin[1] - 1, origin[2]);
    if !pal.is_air(level.get(origin[0], origin[1], origin[2]))
        || !Palette::flag(&b.supports_chorus, below)
    {
        return;
    }
    let c = Chorus { pal, b };
    c.set_plant(level, origin);
    c.grow(level, r, origin, origin, 8, 0);
}

/// `Direction.Plane.HORIZONTAL`: север, восток, юг, запад.
const HORIZONTAL: [Pos; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];

struct Chorus<'a> {
    pal: &'a Palette,
    b: &'a Blocks,
}

impl Chorus<'_> {
    fn empty(&self, level: &dyn Level, p: Pos) -> bool {
        self.pal.is_air(level.get(p[0], p[1], p[2]))
    }

    fn plant_or_flower(&self, level: &dyn Level, p: Pos) -> bool {
        let s = level.get(p[0], p[1], p[2]);
        self.pal.same_block(s, self.b.plant) || self.pal.same_block(s, self.b.flower)
    }

    /// `ChorusPlantBlock.getStateWithConnections` и `setBlock(.., 2)`.
    fn set_plant(&self, level: &mut dyn Level, p: Pos) {
        let at = |d: Pos| [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        let below = level.get(p[0], p[1] - 1, p[2]);
        let down = self.plant_or_flower(&*level, at([0, -1, 0]))
            || Palette::flag(&self.b.supports_chorus, below);
        let mut st = self.b.plant;
        let flag = |v: bool| if v { "true" } else { "false" };
        for (k, v) in [
            ("down", down),
            ("up", self.plant_or_flower(&*level, at([0, 1, 0]))),
            ("north", self.plant_or_flower(&*level, at([0, 0, -1]))),
            ("east", self.plant_or_flower(&*level, at([1, 0, 0]))),
            ("south", self.plant_or_flower(&*level, at([0, 0, 1]))),
            ("west", self.plant_or_flower(&*level, at([-1, 0, 0]))),
        ] {
            st = self.pal.with_prop(st, k, flag(v)).unwrap_or(st);
        }
        put(level, p, st);
    }

    fn all_neighbors_empty(&self, level: &dyn Level, p: Pos, ignore: Option<usize>) -> bool {
        HORIZONTAL
            .iter()
            .enumerate()
            .all(|(i, d)| Some(i) == ignore || self.empty(level, [p[0] + d[0], p[1], p[2] + d[2]]))
    }

    /// `ChorusFlowerBlock.growTreeRecursive`.
    fn grow(
        &self,
        level: &mut dyn Level,
        r: &mut Worldgen,
        current: Pos,
        start: Pos,
        spread: i32,
        depth: i32,
    ) {
        let mut height = r.next_int_bound(4) + 1;
        if depth == 0 {
            height += 1;
        }
        for i in 0..height {
            let target = [current[0], current[1] + i + 1, current[2]];
            if !self.all_neighbors_empty(&*level, target, None) {
                return;
            }
            self.set_plant(level, target);
            self.set_plant(level, [target[0], target[1] - 1, target[2]]);
        }
        let mut placed = false;
        if depth < 4 {
            let mut stems = r.next_int_bound(4);
            if depth == 0 {
                stems += 1;
            }
            for _ in 0..stems {
                let d = r.next_int_bound(4) as usize;
                let dir = HORIZONTAL[d];
                let target = [
                    current[0] + dir[0],
                    current[1] + height,
                    current[2] + dir[2],
                ];
                if (target[0] - start[0]).abs() < spread
                    && (target[2] - start[2]).abs() < spread
                    && self.empty(&*level, target)
                    && self.empty(&*level, [target[0], target[1] - 1, target[2]])
                    && self.all_neighbors_empty(&*level, target, Some((d + 2) % 4))
                {
                    placed = true;
                    self.set_plant(level, target);
                    self.set_plant(level, [target[0] - dir[0], target[1], target[2] - dir[2]]);
                    self.grow(level, r, target, start, spread, depth + 1);
                }
            }
        }
        if !placed {
            let flower = self
                .pal
                .with_prop(self.b.flower, "age", "5")
                .unwrap_or(self.b.flower);
            put(level, [current[0], current[1] + height, current[2]], flower);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spike_ring_matches_vanilla() {
        let xz: Vec<(i32, i32)> = spikes_for_seed(0)
            .iter()
            .map(|s| (s.center_x, s.center_z))
            .collect();
        assert_eq!(
            xz,
            vec![
                (42, 0),
                (33, 24),
                (12, 39),
                (-13, 39),
                (-34, 24),
                (-42, -1),
                (-34, -25),
                (-13, -40),
                (12, -40),
                (33, -25)
            ]
        );
    }
}
