//! Картины (`Painting`, `HangingEntity`, `PaintingVariant`).
//!
//! Картина висит на стене: точка привязки — блок перед опорой, направление —
//! сторона клика. Вариант выбирается из `painting_variant` по размеру: сначала
//! отбрасываются не влезающие, затем берётся самый большой и случайный из них.
//! Вариант передаётся клиенту в метаданных как direct holder (размер, asset id),
//! поэтому реестр вариантов клиенту не нужен.

use super::*;
use crate::placement::step;

/// `Painting.DEPTH`.
const DEPTH: f64 = 0.0625;
/// `BlockAttachedEntity.CHECK_INTERVAL`.
const CHECK_INTERVAL: i32 = 100;

#[derive(Clone, Debug)]
pub(crate) struct PaintingVariant {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub asset: String,
}

/// Загружает варианты из `data/minecraft/painting_variant/*.json`.
pub(crate) fn load_variants(data_dir: &std::path::Path) -> Vec<PaintingVariant> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(data_dir.join("data/minecraft/painting_variant")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let width = json
                .get("width")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(1)
                .clamp(1, 16) as i32;
            let height = json
                .get("height")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(1)
                .clamp(1, 16) as i32;
            let asset = json
                .get("asset_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("minecraft:{stem}"));
            out.push(PaintingVariant {
                name: format!("minecraft:{stem}"),
                width,
                height,
                asset,
            });
        }
    }
    if out.is_empty() {
        out.push(PaintingVariant {
            name: "minecraft:kebab".into(),
            width: 1,
            height: 1,
            asset: "minecraft:kebab".into(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub(crate) struct Painting {
    pub uuid: Uuid,
    /// Блок перед опорой (`block_pos`).
    pub pos: [i32; 3],
    /// 3D-направление от опоры к картине (2..=5).
    pub facing: usize,
    pub variant: PaintingVariant,
    pub custom_name: Option<Tag>,
    since_check: i32,
}

impl Painting {
    fn new(uuid: Uuid, pos: [i32; 3], facing: usize, variant: PaintingVariant) -> Self {
        Self {
            uuid,
            pos,
            facing,
            variant,
            custom_name: None,
            since_check: 0,
        }
    }

    fn boxed(&self) -> [[f64; 3]; 2] {
        painting_box(
            self.pos,
            self.facing,
            self.variant.width,
            self.variant.height,
        )
    }

    fn center(&self) -> [f64; 3] {
        let b = self.boxed();
        std::array::from_fn(|i| (b[0][i] + b[1][i]) / 2.0)
    }

    /// `getYRot`/`getXRot` (`HangingEntity.setDirection`).
    fn angles(&self) -> (f32, f32) {
        (two_d(self.facing) as f32 * 90.0, 0.0)
    }
}

/// `Direction.get2DDataValue`: юг 0, запад 1, север 2, восток 3.
fn two_d(facing: usize) -> i32 {
    match facing {
        3 => 0,
        4 => 1,
        2 => 2,
        5 => 3,
        _ => 0,
    }
}

/// 2D-направление из NBT -> 3D id.
fn from_two_d(id: i64) -> usize {
    match id {
        0 => 3, // south
        1 => 4, // west
        2 => 2, // north
        _ => 5, // east
    }
}

/// `Direction.getCounterClockWise` для горизонтальных направлений.
fn counter_clockwise(facing: usize) -> usize {
    match facing {
        2 => 4,
        3 => 5,
        4 => 3,
        5 => 2,
        _ => facing,
    }
}

/// `Vec3.atCenterOf(pos).relative(dir, -0.46875)`.
fn wall_center(pos: [i32; 3], facing: usize) -> [f64; 3] {
    let d = step(facing);
    std::array::from_fn(|i| pos[i] as f64 + 0.5 - d[i] as f64 * 0.46875)
}

/// `Painting.calculateBoundingBox`.
fn painting_box(pos: [i32; 3], facing: usize, width: i32, height: i32) -> [[f64; 3]; 2] {
    let attached = wall_center(pos, facing);
    let left = step(counter_clockwise(facing));
    let h = if width % 2 == 0 { 0.5 } else { 0.0 };
    let v = if height % 2 == 0 { 0.5 } else { 0.0 };
    let center = [
        attached[0] + left[0] as f64 * h,
        attached[1] + left[1] as f64 * h + v,
        attached[2] + left[2] as f64 * h,
    ];
    let (x, z) = if facing == 4 || facing == 5 {
        (DEPTH, width as f64)
    } else {
        (width as f64, DEPTH)
    };
    let size = [x, height as f64, z];
    [
        std::array::from_fn(|i| center[i] - size[i] / 2.0),
        std::array::from_fn(|i| center[i] + size[i] / 2.0),
    ]
}

fn intersects(a: [[f64; 3]; 2], b: [[f64; 3]; 2]) -> bool {
    (0..3).all(|i| a[0][i] < b[1][i] && a[1][i] > b[0][i])
}

/// Целые блоки, пересекающиеся с коробкой.
fn blocks_in(b: [[f64; 3]; 2]) -> Vec<[i32; 3]> {
    let lo: [i32; 3] = std::array::from_fn(|i| b[0][i].floor() as i32);
    let hi: [i32; 3] = std::array::from_fn(|i| b[1][i].ceil() as i32 - 1);
    let mut out = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                out.push([x, y, z]);
            }
        }
    }
    out
}

impl PlayPlan {
    fn add_painting(&self, p: Painting) -> i32 {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let packets = self.painting_packets(id, &p);
        let ty = self
            .survival
            .entity_types
            .get("minecraft:painting")
            .copied();
        let tracked = Tracked {
            id,
            pos: p.center(),
            range: ty.map_or(16, |t| self.tracking_range(t)),
        };
        self.sim().paintings.lock().unwrap().insert(id, p);
        self.hub.spawn_tracked(tracked, packets);
        id
    }

    fn painting_packets(&self, id: i32, p: &Painting) -> Vec<Vec<u8>> {
        let Some(&ty) = self.survival.entity_types.get("minecraft:painting") else {
            return Vec::new();
        };
        let center = p.center();
        let (yaw, pitch) = p.angles();
        let mut out = vec![play::add_entity_moving(
            self.ids.add_entity,
            id,
            p.uuid,
            ty,
            center,
            [0.0; 3],
            yaw,
            pitch,
            p.facing as i32,
        )];
        // `Painting.DATA_PAINTING_VARIANT_ID`: индекс 9, сериализатор 34.
        let mut w = Writer::new();
        w.var_int(self.ids.set_entity_data)
            .var_int(id)
            .u8(9)
            .var_int(34);
        // `ByteBufCodecs.holder`: 0 — direct holder, дальше `DIRECT_STREAM_CODEC`.
        w.var_int(0)
            .var_int(p.variant.width)
            .var_int(p.variant.height)
            .string(&p.variant.asset)
            .bool(false)
            .bool(false)
            .u8(0xff);
        out.push(w.buf);
        out
    }

    pub(super) fn is_painting(&self, id: i32) -> bool {
        self.sim().paintings.lock().unwrap().contains_key(&id)
    }

    pub(super) fn painting_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let paintings = self.sim().paintings.lock().unwrap();
        Some(self.painting_packets(id, paintings.get(&id)?))
    }

    pub(super) fn track_paintings(&self, out: &mut Vec<Tracked>) {
        for (&id, p) in self.sim().paintings.lock().unwrap().iter() {
            let ty = self
                .survival
                .entity_types
                .get("minecraft:painting")
                .copied();
            out.push(Tracked {
                id,
                pos: p.center(),
                range: ty.map_or(16, |t| self.tracking_range(t)),
            });
        }
    }

    /// Может ли картина `p` (или кандидат) висеть на этом месте.
    fn painting_survives(&self, skip: Option<i32>, p: &Painting) -> bool {
        let b = p.boxed();
        // `hasLevelCollision(getPopBox())`: ни один блок в объёме не мешает.
        for q in blocks_in(b) {
            let Some(state) = self.block(q) else {
                return false;
            };
            if let Some(phys) = self.survival.physics.as_deref() {
                let base = q.map(|v| v as f64);
                for s in phys.boxes_of(state) {
                    let block = [
                        [base[0] + s[0], base[1] + s[1], base[2] + s[2]],
                        [base[0] + s[3], base[1] + s[4], base[2] + s[5]],
                    ];
                    if intersects(b, block) {
                        return false;
                    }
                }
            } else if self.survival.placement.solid(state) {
                return false;
            }
        }
        // `calculateSupportBox`: за картиной сплошная опора.
        let d = step(p.facing);
        let support = [
            [
                b[0][0] - d[0] as f64 * 0.5,
                b[0][1] - d[1] as f64 * 0.5,
                b[0][2] - d[2] as f64 * 0.5,
            ],
            [
                b[1][0] - d[0] as f64 * 0.5,
                b[1][1] - d[1] as f64 * 0.5,
                b[1][2] - d[2] as f64 * 0.5,
            ],
        ];
        for q in blocks_in(support) {
            let Some(state) = self.block(q) else {
                return false;
            };
            let name = self.state_name(state);
            let diode = matches!(name, "minecraft:repeater" | "minecraft:comparator");
            if !self.survival.placement.solid(state) && !diode {
                return false;
            }
        }
        // `canCoexist`: картина не пересекается с другой в том же направлении.
        let paintings = self.sim().paintings.lock().unwrap();
        !paintings.iter().any(|(&other, o)| {
            Some(other) != skip && o.facing == p.facing && intersects(b, o.boxed())
        })
    }

    /// `Painting.create`: выбрать самый большой вариант, который влезает.
    pub(super) fn place_painting(
        &self,
        uuid: Uuid,
        clicked: [i32; 3],
        face: i32,
        adventure: bool,
    ) -> bool {
        if !(2..=5).contains(&face) {
            return false;
        }
        let d = step(face as usize);
        let pos = [clicked[0] + d[0], clicked[1] + d[1], clicked[2] + d[2]];
        if self.block(pos).is_none() || adventure || self.spawn_blocked(uuid, pos) {
            return false;
        }
        let facing = face as usize;
        let mut best: Vec<&PaintingVariant> = Vec::new();
        let mut area = 0;
        for variant in &self.painting_variants {
            let p = Painting::new(Uuid::nil(), pos, facing, variant.clone());
            if !self.painting_survives(None, &p) {
                continue;
            }
            let a = variant.width * variant.height;
            if a > area {
                area = a;
                best.clear();
                best.push(variant);
            } else if a == area {
                best.push(variant);
            }
        }
        if best.is_empty() {
            return false;
        }
        let variant = best[rand::random::<usize>() % best.len()].clone();
        let p = Painting::new(Uuid::from_u128(rand::random()), pos, facing, variant);
        self.play_sound(
            None,
            "minecraft:entity.painting.place",
            crate::sounds::NEUTRAL,
            p.center(),
            1.0,
            1.0,
        );
        self.add_painting(p);
        true
    }

    /// Удар по картине: проверить игрока и снять.
    pub(super) fn attack_painting(&self, uuid: Uuid, target: i32, creative: bool) -> Option<()> {
        let feet = self.player_at(uuid)?.0;
        let (pos, center) = {
            let paintings = self.sim().paintings.lock().unwrap();
            let p = paintings.get(&target)?;
            (p.pos, p.center())
        };
        let d: f64 = (0..3).map(|i| (center[i] - feet[i]).powi(2)).sum();
        if d > 36.0 || self.spawn_blocked(uuid, pos) {
            return Some(());
        }
        self.break_painting(target, creative);
        Some(())
    }

    /// Снять картину без игрока (опора пропала, взрыв).
    fn break_painting(&self, target: i32, creative: bool) {
        let Some(mut p) = self.sim().paintings.lock().unwrap().remove(&target) else {
            return;
        };
        let center = p.center();
        self.hub.remove_tracked(&[target]);
        self.play_sound(
            None,
            "minecraft:entity.painting.break",
            crate::sounds::NEUTRAL,
            center,
            1.0,
            1.0,
        );
        if !creative
            && self.vars.flag("entity_drops")
            && let Some(item) = self.item_id("minecraft:painting")
        {
            let mut stack = Stack::new(item, 1);
            if let Some(name) = p.custom_name.take() {
                stack.extra =
                    chunkwake_protocol::components::intern(chunkwake_protocol::components::Extra {
                        custom_name: Some(crate::stack_nbt::tag_to_json(&name).to_string()),
                        ..Default::default()
                    });
            }
            let d = step(p.facing);
            let at = [
                center[0] + d[0] as f64 * 0.15,
                center[1],
                center[2] + d[2] as f64 * 0.15,
            ];
            let vel = items::pop_velocity(rand::random(), rand::random());
            self.drop_item(at, vel, stack, items::BLOCK_DELAY);
        }
    }

    pub(super) fn pick_painting(&self, id: i32) -> Option<(Stack, [[f64; 3]; 2])> {
        let paintings = self.sim().paintings.lock().unwrap();
        let p = paintings.get(&id)?;
        let item = self.item_id("minecraft:painting")?;
        Some((Stack::new(item, 1), p.boxed()))
    }

    pub(super) fn explode_paintings(&self, center: [f64; 3], power: f32) {
        let reach = power as f64 * 2.0;
        let hit: Vec<i32> = self
            .sim()
            .paintings
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, p)| {
                let c = p.center();
                (0..3).map(|i| (c[i] - center[i]).powi(2)).sum::<f64>() <= reach * reach
            })
            .map(|(&id, _)| id)
            .collect();
        for id in hit {
            self.break_painting(id, false);
        }
    }

    /// Раз в 100 тиков проверяет опору, как `HangingEntity`.
    pub(super) fn tick_paintings(&self) {
        if self.sim().paintings.lock().unwrap().is_empty() {
            return;
        }
        let simulated = self.simulated_set();
        let mut check = Vec::new();
        {
            let mut paintings = self.sim().paintings.lock().unwrap();
            for (&id, p) in paintings.iter_mut() {
                if !simulated.is_empty() && !simulated.contains(&(p.pos[0] >> 4, p.pos[2] >> 4)) {
                    continue;
                }
                p.since_check += 1;
                if p.since_check >= CHECK_INTERVAL {
                    p.since_check = 0;
                    check.push(id);
                }
            }
        }
        for id in check {
            let alive = self
                .sim()
                .paintings
                .lock()
                .unwrap()
                .get(&id)
                .is_some_and(|p| self.painting_survives(Some(id), p));
            if !alive {
                self.break_painting(id, false);
            }
        }
    }

    pub(super) fn load_painting(&self, c: &Compound) -> bool {
        if c.get("id").and_then(Tag::as_str) != Some("minecraft:painting") {
            return false;
        }
        let pos = match c.get("block_pos") {
            Some(Tag::IntArray(p)) if p.len() == 3 => [p[0], p[1], p[2]],
            _ => match c.get("Pos") {
                Some(Tag::List(l)) if l.len() == 3 => {
                    let v: Vec<f64> = l
                        .iter()
                        .map(|t| match t {
                            Tag::Double(v) => *v,
                            Tag::Float(v) => *v as f64,
                            _ => 0.0,
                        })
                        .collect();
                    [
                        v[0].floor() as i32,
                        v[1].floor() as i32,
                        v[2].floor() as i32,
                    ]
                }
                _ => return false,
            },
        };
        let facing = c
            .get("facing")
            .and_then(Tag::as_i64)
            .map(from_two_d)
            .unwrap_or(3);
        let name = c.get("variant").and_then(Tag::as_str);
        let Some(variant) = self
            .painting_variants
            .iter()
            .find(|v| Some(v.name.as_str()) == name)
            .or_else(|| self.painting_variants.first())
            .cloned()
        else {
            return false;
        };
        let mut uuid = Uuid::from_u128(rand::random());
        if let Some(Tag::IntArray(u)) = c.get("UUID")
            && u.len() == 4
        {
            uuid = Uuid::from_u128(u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128));
        }
        let mut p = Painting::new(uuid, pos, facing, variant);
        p.custom_name = c.get("CustomName").cloned();
        self.add_painting(p);
        true
    }

    pub(super) fn paintings_nbt(&self) -> Vec<((i32, i32), Compound)> {
        self.sim()
            .paintings
            .lock()
            .unwrap()
            .values()
            .map(|p| {
                let mut c = Compound::new();
                let center = p.center();
                let (yaw, pitch) = p.angles();
                c.insert("id".into(), Tag::String("minecraft:painting".into()));
                c.insert(
                    "Pos".into(),
                    Tag::List(center.iter().map(|v| Tag::Double(*v)).collect()),
                );
                c.insert("Motion".into(), Tag::List(vec![Tag::Double(0.0); 3]));
                c.insert(
                    "Rotation".into(),
                    Tag::List(vec![Tag::Float(yaw), Tag::Float(pitch)]),
                );
                let u = p.uuid.as_u128();
                c.insert(
                    "UUID".into(),
                    Tag::IntArray(
                        (0..4)
                            .rev()
                            .map(|i| (u >> (32 * i)) as u32 as i32)
                            .collect(),
                    ),
                );
                c.insert("block_pos".into(), Tag::IntArray(p.pos.to_vec()));
                c.insert("facing".into(), Tag::Byte(two_d(p.facing) as i8));
                c.insert("variant".into(), Tag::String(p.variant.name.clone()));
                if let Some(n) = &p.custom_name {
                    c.insert("CustomName".into(), n.clone());
                }
                let chunk = (
                    (center[0].floor() as i32) >> 4,
                    (center[2].floor() as i32) >> 4,
                );
                (chunk, c)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_sizes_shift_by_half_a_block() {
        // 1x1: центр ровно у стены; 2x2: сдвиг на полблока влево и вверх.
        let one = painting_box([0, 64, 0], 2, 1, 1);
        let two = painting_box([0, 64, 0], 2, 2, 2);
        assert!((one[0][1] + one[1][1]) / 2.0 - 64.5 < 0.001);
        assert!((two[0][1] + two[1][1]) / 2.0 - 65.0 < 0.001);
        assert!((two[1][2] - two[0][2]) - 1.0 < 0.001);
    }
}
