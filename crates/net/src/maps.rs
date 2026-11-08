//! Карты (`MapItemSavedData`, `MapItem.update`): данные карты, отрисовка
//! местности по столбцам, значки игроков и заплатки для клиента.
//!
//! Мир сюда не входит: столбец описывает [`Sample`], его даёт вызывающий.

use chunkwake_world::nbt::{Compound, Tag};
use uuid::Uuid;

/// Сторона карты в пикселях.
pub const SIZE: usize = 128;
/// Наибольший масштаб (`1 << 4` блоков на пиксель).
pub const MAX_SCALE: u8 = 4;

/// `MapColor.NONE`, `DIRT`, `STONE`, `WATER`.
const NONE: u8 = 0;
const DIRT: u8 = 10;
const STONE: u8 = 11;
pub const WATER: u8 = 12;

/// Типы `MapDecorationType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoKind {
    Player,
    Frame,
    PlayerOffMap,
    PlayerOffLimits,
    /// Знамя цвета `DyeColor` с этим номером (`banner_white` … `banner_black`).
    Banner(u8),
}

impl DecoKind {
    /// Номер в реестре `map_decoration_type`.
    pub fn id(self) -> i32 {
        match self {
            DecoKind::Player => 0,
            DecoKind::Frame => 1,
            DecoKind::PlayerOffMap => 6,
            DecoKind::PlayerOffLimits => 7,
            DecoKind::Banner(c) => 10 + c as i32,
        }
    }
}

/// Значок на карте: `x`, `z` — от −128 до 127, `rot` — шестнадцатые доли
/// оборота, `name` — JSON-текст подписи (имя знамени).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoration {
    pub kind: DecoKind,
    pub x: i8,
    pub z: i8,
    pub rot: i8,
    pub name: Option<String>,
}

/// Знамя на карте (`MapBanner`): место, цвет (`DyeColor`) и имя (JSON-текст).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapBanner {
    pub pos: [i32; 3],
    pub color: u8,
    pub name: Option<String>,
}

impl MapBanner {
    /// `MapBanner.getId`.
    pub fn id(&self) -> String {
        format!("banner-{},{},{}", self.pos[0], self.pos[1], self.pos[2])
    }
}

/// `isTrackedCountOverLimit(256)`: все наши типы значков считаются
/// (`trackCount` у игрока, рамки и знамён).
const TRACKED_LIMIT: usize = 256;

/// Столбец (или квадрат `scale×scale` столбцов) под пикселем.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Самый частый цвет квадрата.
    pub color: u8,
    /// Сумма высот / `scale²`.
    pub height: f64,
    /// Глубина жидкости / `scale²` (целочисленно).
    pub depth: i32,
}

/// Игрок, у которого карта в инвентаре (`HoldingPlayer`).
#[derive(Clone, Debug)]
pub struct Holder {
    pub player: Uuid,
    pub step: i32,
    dirty_data: bool,
    min: [i32; 2],
    max: [i32; 2],
    dirty_decorations: bool,
    tick: i32,
}

impl Holder {
    fn new(player: Uuid) -> Self {
        Self {
            player,
            step: 0,
            dirty_data: true,
            min: [0, 0],
            max: [127, 127],
            dirty_decorations: true,
            tick: 0,
        }
    }

    fn mark_colors(&mut self, x: i32, z: i32) {
        if self.dirty_data {
            self.min = [self.min[0].min(x), self.min[1].min(z)];
            self.max = [self.max[0].max(x), self.max[1].max(z)];
        } else {
            self.dirty_data = true;
            self.min = [x, z];
            self.max = [x, z];
        }
    }
}

/// Прямоугольник пикселей для `ClientboundMapItemDataPacket`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub x: u8,
    pub z: u8,
    pub width: u8,
    pub height: u8,
    pub colors: Vec<u8>,
}

/// Содержимое пакета карты для одного игрока.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub scale: u8,
    pub locked: bool,
    pub decorations: Option<Vec<Decoration>>,
    pub patch: Option<Patch>,
}

/// Рамка с картой (`MapFrame`): ключ — `frame-x,y,z`, значок — `frame-<entity_id>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapFrame {
    pub pos: [i32; 3],
    pub rotation: i32,
    pub entity_id: i32,
}

impl MapFrame {
    /// `MapFrame.frameId`.
    pub fn id(&self) -> String {
        format!("frame-{},{},{}", self.pos[0], self.pos[1], self.pos[2])
    }
}

/// `MapItemSavedData.getFrameKey`.
pub fn frame_key(entity_id: i32) -> String {
    format!("frame-{entity_id}")
}

/// Одна карта (`data/minecraft/maps/<id>.dat`).
#[derive(Clone, Debug)]
pub struct MapData {
    pub scale: u8,
    pub center: [i32; 2],
    pub dimension: String,
    pub tracking: bool,
    pub unlimited: bool,
    pub locked: bool,
    pub colors: Vec<u8>,
    pub holders: Vec<Holder>,
    /// Значки по ключу (имя игрока, `frame-<id>`), в порядке добавления (`LinkedHashMap`).
    pub decorations: Vec<(String, Decoration)>,
    /// Рамки с этой картой (`frameMarkers`), по порядку добавления.
    pub frames: Vec<MapFrame>,
    /// Отмеченные знамёна (`bannerMarkers`), по порядку добавления.
    pub banners: Vec<MapBanner>,
    /// Изменилась с последнего сохранения.
    pub dirty: bool,
}

/// `MapItemSavedData.createFresh`: центр — середина сетки шагом `128·2^scale`.
pub fn center_for(x: f64, z: f64, scale: u8) -> [i32; 2] {
    let i = 128 * (1i32 << scale);
    let snap = |v: f64| ((v + 64.0) / i as f64).floor() as i32 * i + i / 2 - 64;
    [snap(x), snap(z)]
}

impl MapData {
    pub fn fresh(
        x: f64,
        z: f64,
        scale: u8,
        tracking: bool,
        unlimited: bool,
        dimension: &str,
    ) -> Self {
        let scale = scale.min(MAX_SCALE);
        Self {
            scale,
            center: center_for(x, z, scale),
            dimension: dimension.to_owned(),
            tracking,
            unlimited,
            locked: false,
            colors: vec![0; SIZE * SIZE],
            holders: Vec::new(),
            decorations: Vec::new(),
            frames: Vec::new(),
            banners: Vec::new(),
            dirty: true,
        }
    }

    /// `scaled()`: новая карта на шаг крупнее с тем же центром, пустая.
    pub fn scaled(&self) -> Self {
        Self::fresh(
            self.center[0] as f64,
            self.center[1] as f64,
            (self.scale + 1).min(MAX_SCALE),
            self.tracking,
            self.unlimited,
            &self.dimension,
        )
    }

    /// `locked()`: копия пикселей и значков, больше не обновляется.
    pub fn locked_copy(&self) -> Self {
        Self {
            locked: true,
            holders: Vec::new(),
            // `locked()` переносит значки и баннеры, но не метки рамок.
            frames: Vec::new(),
            dirty: true,
            ..self.clone()
        }
    }

    pub fn holder(&mut self, player: Uuid) -> &mut Holder {
        let i = match self.holders.iter().position(|h| h.player == player) {
            Some(i) => i,
            None => {
                self.holders.push(Holder::new(player));
                self.holders.len() - 1
            }
        };
        &mut self.holders[i]
    }

    /// `updateColor`: пиксель поменялся — грязный прямоугольник у всех держателей.
    pub fn update_color(&mut self, x: i32, z: i32, color: u8) -> bool {
        let i = x as usize + z as usize * SIZE;
        if self.colors[i] == color {
            return false;
        }
        self.colors[i] = color;
        self.dirty = true;
        for h in &mut self.holders {
            h.mark_colors(x, z);
        }
        true
    }

    fn decorations_dirty(&mut self) {
        for h in &mut self.holders {
            h.dirty_decorations = true;
        }
    }

    pub fn remove_decoration(&mut self, key: &str) {
        if let Some(i) = self.decorations.iter().position(|(k, _)| k == key) {
            self.decorations.remove(i);
            self.decorations_dirty();
        }
    }

    /// `isTrackedCountOverLimit`: рамка не принимает новую карту, когда
    /// все 256 отслеживаемых значков уже заняты.
    pub fn tracked_count_over_limit(&self) -> bool {
        self.decorations.len() >= TRACKED_LIMIT
    }

    /// `addDecoration` для игрока: `game_time` — только в Аду (вертящаяся стрелка).
    pub fn put_player(&mut self, key: &str, x: f64, z: f64, yaw: f32, nether_time: Option<i64>) {
        self.put_decoration(DecoKind::Player, key, x, z, yaw as f64, nether_time);
    }

    /// `addDecoration` (`calculateDecorationLocationAndType`): игрок вне карты —
    /// стрелка у края или точка за пределами; прочий значок вне карты остаётся
    /// у края только на карте без границ.
    pub fn put_decoration(
        &mut self,
        kind: DecoKind,
        key: &str,
        x: f64,
        z: f64,
        yaw: f64,
        nether_time: Option<i64>,
    ) {
        self.put_named(kind, key, x, z, yaw, nether_time, None);
    }

    /// `addDecoration` с подписью.
    #[allow(clippy::too_many_arguments)]
    fn put_named(
        &mut self,
        kind: DecoKind,
        key: &str,
        x: f64,
        z: f64,
        yaw: f64,
        nether_time: Option<i64>,
        name: Option<String>,
    ) {
        let s = (1i32 << self.scale) as f32;
        let fx = (x - self.center[0] as f64) as f32 / s;
        let fz = (z - self.center[1] as f64) as f32 / s;
        let kind = if inside(fx, fz) || (kind != DecoKind::Player && self.unlimited) {
            kind
        } else if kind != DecoKind::Player {
            self.remove_decoration(key);
            return;
        } else if fx.abs() < 320.0 && fz.abs() < 320.0 {
            DecoKind::PlayerOffMap
        } else if self.unlimited {
            DecoKind::PlayerOffLimits
        } else {
            self.remove_decoration(key);
            return;
        };
        let rot = rotation(yaw, nether_time);
        let deco = Decoration {
            kind,
            x: clamp_coord(fx),
            z: clamp_coord(fz),
            rot,
            name,
        };
        match self.decorations.iter_mut().find(|(k, _)| k == key) {
            Some((_, old)) if *old == deco => {}
            Some((_, old)) => {
                *old = deco;
                self.decorations_dirty();
            }
            None => {
                self.decorations.push((key.to_owned(), deco));
                self.decorations_dirty();
            }
        }
    }

    /// Ветка рамки в `tickCarriedBy`: значок `frame-<id>` в блоке рамки,
    /// поворот — сторона рамки × 90. Прежняя рамка в том же блоке с другим
    /// номером сущности теряет свой значок.
    pub fn tick_frame(&mut self, frame: MapFrame, nether_time: Option<i64>) {
        if !self.tracking {
            return;
        }
        let id = frame.id();
        if let Some(old) = self.frames.iter().find(|f| f.id() == id).copied()
            && old.entity_id != frame.entity_id
        {
            self.remove_decoration(&frame_key(old.entity_id));
        }
        self.put_decoration(
            DecoKind::Frame,
            &frame_key(frame.entity_id),
            frame.pos[0] as f64,
            frame.pos[2] as f64,
            frame.rotation as f64,
            nether_time,
        );
        match self.frames.iter_mut().find(|f| f.id() == id) {
            Some(old) if *old == frame => {}
            Some(old) => {
                *old = frame;
                self.dirty = true;
            }
            None => {
                self.frames.push(frame);
                self.dirty = true;
            }
        }
    }

    /// Метка знамени и её значок без проверок: чтение с диска и последняя
    /// ветка `toggleBanner`.
    fn add_banner(&mut self, b: MapBanner) {
        let id = b.id();
        self.put_named(
            DecoKind::Banner(b.color),
            &id,
            b.pos[0] as f64 + 0.5,
            b.pos[2] as f64 + 0.5,
            180.0,
            None,
            b.name.clone(),
        );
        match self.banners.iter_mut().find(|o| o.id() == id) {
            Some(old) => *old = b,
            None => self.banners.push(b),
        }
    }

    /// `toggleBanner`: то же знамя снимается, новое (или изменившееся)
    /// отмечается. `false` — знамя вне карты или значков слишком много.
    pub fn toggle_banner(&mut self, b: MapBanner) -> bool {
        let s = (1i32 << self.scale) as f64;
        let dx = (b.pos[0] as f64 + 0.5 - self.center[0] as f64) / s;
        let dz = (b.pos[2] as f64 + 0.5 - self.center[1] as f64) / s;
        if !(-63.0..=63.0).contains(&dx) || !(-63.0..=63.0).contains(&dz) {
            return false;
        }
        let id = b.id();
        if let Some(i) = self.banners.iter().position(|o| *o == b) {
            self.banners.remove(i);
            self.remove_decoration(&id);
            self.dirty = true;
            return true;
        }
        if self.tracked_count_over_limit() {
            return false;
        }
        self.add_banner(b);
        self.dirty = true;
        true
    }

    /// Метки знамён в квадратах `scale×scale` с этими углами — их сверяет
    /// `checkBanners` по каждому столбцу перерисованного квадрата.
    pub fn banners_in(&self, corners: &[(i32, i32)]) -> Vec<MapBanner> {
        let s = 1i32 << self.scale;
        self.banners
            .iter()
            .filter(|b| corners.contains(&(b.pos[0].div_euclid(s) * s, b.pos[2].div_euclid(s) * s)))
            .cloned()
            .collect()
    }

    /// `checkBanners`: знамени на месте нет или оно другое — метка снимается.
    pub fn remove_banner(&mut self, b: &MapBanner) {
        let id = b.id();
        self.banners.retain(|o| o.id() != id);
        self.remove_decoration(&id);
        self.dirty = true;
    }

    /// `removedFromFrame`: рамка лишилась карты или сломана.
    pub fn removed_from_frame(&mut self, pos: [i32; 3], entity_id: i32) {
        self.remove_decoration(&frame_key(entity_id));
        let id = format!("frame-{},{},{}", pos[0], pos[1], pos[2]);
        self.frames.retain(|f| f.id() != id);
        self.dirty = true;
    }

    /// `HoldingPlayer.nextUpdatePacket`: заплатка и (раз в 5 вызовов) значки.
    pub fn next_update(&mut self, player: Uuid) -> Option<Update> {
        let (scale, locked) = (self.scale, self.locked);
        let decorations: Vec<Decoration> =
            self.decorations.iter().map(|(_, d)| d.clone()).collect();
        let colors = &self.colors;
        let h = self.holders.iter_mut().find(|h| h.player == player)?;
        let patch = if h.dirty_data {
            h.dirty_data = false;
            let w = h.max[0] + 1 - h.min[0];
            let ht = h.max[1] + 1 - h.min[1];
            let mut out = vec![0u8; (w * ht) as usize];
            for dx in 0..w {
                for dz in 0..ht {
                    out[(dx + dz * w) as usize] =
                        colors[(h.min[0] + dx) as usize + (h.min[1] + dz) as usize * SIZE];
                }
            }
            Some(Patch {
                x: h.min[0] as u8,
                z: h.min[1] as u8,
                width: w as u8,
                height: ht as u8,
                colors: out,
            })
        } else {
            None
        };
        let decorations = if h.dirty_decorations && {
            let t = h.tick;
            h.tick += 1;
            t % 5 == 0
        } {
            h.dirty_decorations = false;
            Some(decorations)
        } else {
            None
        };
        (patch.is_some() || decorations.is_some()).then_some(Update {
            scale,
            locked,
            decorations,
            patch,
        })
    }

    /// `MapItem.update`: столбцы около игрока, раз в 16 тиков каждый
    /// (или подряд, пока соседний слева менялся).
    ///
    /// `sample(x, z)` — квадрат `scale×scale` с углом в мировых `(x, z)`;
    /// `None` — чанк не загружен. В измерении с потолком цвет берётся из
    /// хэша координат, но чанк всё равно должен быть.
    pub fn update(
        &mut self,
        player: Uuid,
        pos: [f64; 2],
        ceiling: bool,
        mut sample: impl FnMut(i32, i32) -> Option<Sample>,
    ) {
        let scale = 1i32 << self.scale;
        let [cx, cz] = self.center;
        let px = (pos[0] - cx as f64).floor() as i32 / scale + 64;
        let pz = (pos[1] - cz as f64).floor() as i32 / scale + 64;
        let mut r = 128 / scale;
        if ceiling {
            r /= 2;
        }
        let step = {
            let h = self.holder(player);
            h.step += 1;
            h.step
        };
        let mut changed = false;
        for x in (px - r + 1)..(px + r) {
            if (x & 15) != (step & 15) && !changed {
                continue;
            }
            changed = false;
            let mut prev = 0.0f64;
            for z in (pz - r - 1)..(pz + r) {
                if x < 0 || z < -1 || x >= 128 || z >= 128 {
                    continue;
                }
                let dist = (x - px) * (x - px) + (z - pz) * (z - pz);
                let edge = dist > (r - 2) * (r - 2);
                let wx = (cx / scale + x - 64) * scale;
                let wz = (cz / scale + z - 64) * scale;
                let Some(mut s) = sample(wx, wz) else {
                    continue;
                };
                if ceiling {
                    s = ceiling_sample(wx, wz);
                }
                let parity = ((x + z) & 1) as f64;
                let brightness = if s.color == WATER {
                    let f = s.depth as f64 * 0.1 + parity * 0.2;
                    if f < 0.5 {
                        2
                    } else if f > 0.9 {
                        0
                    } else {
                        1
                    }
                } else {
                    let f = (s.height - prev) * 4.0 / (scale + 4) as f64 + (parity - 0.5) * 0.4;
                    if f > 0.6 {
                        2
                    } else if f < -0.6 {
                        0
                    } else {
                        1
                    }
                };
                prev = s.height;
                if z < 0 || dist >= r * r || (edge && (x + z) & 1 == 0) {
                    continue;
                }
                changed |= self.update_color(x, z, packed(s.color, brightness));
            }
        }
    }

    /// `data` из `<id>.dat` (без `DataVersion`).
    pub fn to_nbt(&self) -> Compound {
        let mut c = Compound::new();
        c.insert("dimension".into(), Tag::String(self.dimension.clone()));
        c.insert("xCenter".into(), Tag::Int(self.center[0]));
        c.insert("zCenter".into(), Tag::Int(self.center[1]));
        c.insert("scale".into(), Tag::Byte(self.scale as i8));
        c.insert(
            "colors".into(),
            Tag::ByteArray(self.colors.iter().map(|&b| b as i8).collect()),
        );
        c.insert("trackingPosition".into(), Tag::Byte(self.tracking as i8));
        c.insert("unlimitedTracking".into(), Tag::Byte(self.unlimited as i8));
        c.insert("locked".into(), Tag::Byte(self.locked as i8));
        c.insert(
            "banners".into(),
            Tag::List(
                self.banners
                    .iter()
                    .map(|b| {
                        let mut m = Compound::new();
                        m.insert("pos".into(), Tag::IntArray(b.pos.to_vec()));
                        // `lenientOptionalFieldOf("color", WHITE)`: белый не пишется.
                        if b.color != 0
                            && let Some(c) =
                                chunkwake_protocol::components::DYE_COLORS.get(b.color as usize)
                        {
                            m.insert("color".into(), Tag::String((*c).to_owned()));
                        }
                        if let Some(n) = &b.name {
                            m.insert(
                                "name".into(),
                                crate::stack_nbt::json_to_tag(&crate::stack_nbt::text_json(n)),
                            );
                        }
                        Tag::Compound(m)
                    })
                    .collect(),
            ),
        );
        c.insert(
            "frames".into(),
            Tag::List(
                self.frames
                    .iter()
                    .map(|f| {
                        let mut m = Compound::new();
                        m.insert("pos".into(), Tag::IntArray(f.pos.to_vec()));
                        m.insert("rotation".into(), Tag::Int(f.rotation));
                        m.insert("entity_id".into(), Tag::Int(f.entity_id));
                        Tag::Compound(m)
                    })
                    .collect(),
            ),
        );
        c
    }

    pub fn from_nbt(c: &Compound) -> Option<Self> {
        let int = |k: &str| c.get(k).and_then(Tag::as_i64);
        let flag = |k: &str, d: bool| int(k).map_or(d, |v| v != 0);
        let mut colors = match c.get("colors") {
            Some(Tag::ByteArray(b)) => b.iter().map(|&v| v as u8).collect(),
            _ => vec![0; SIZE * SIZE],
        };
        colors.resize(SIZE * SIZE, 0);
        let frames: Vec<MapFrame> = match c.get("frames") {
            Some(Tag::List(l)) => l
                .iter()
                .filter_map(|t| {
                    let m = t.as_compound()?;
                    let pos = match m.get("pos") {
                        Some(Tag::IntArray(p)) if p.len() == 3 => [p[0], p[1], p[2]],
                        _ => return None,
                    };
                    Some(MapFrame {
                        pos,
                        rotation: m.get("rotation").and_then(Tag::as_i64)? as i32,
                        entity_id: m.get("entity_id").and_then(Tag::as_i64)? as i32,
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        let banners: Vec<MapBanner> = match c.get("banners") {
            Some(Tag::List(l)) => l
                .iter()
                .filter_map(|t| {
                    let m = t.as_compound()?;
                    let pos = match m.get("pos") {
                        Some(Tag::IntArray(p)) if p.len() == 3 => [p[0], p[1], p[2]],
                        _ => return None,
                    };
                    let color = match m.get("color") {
                        Some(Tag::String(s)) => chunkwake_protocol::components::DYE_COLORS
                            .iter()
                            .position(|c| c == s)?
                            as u8,
                        _ => 0,
                    };
                    let name = m
                        .get("name")
                        .map(|t| crate::stack_nbt::tag_to_json(t).to_string());
                    Some(MapBanner { pos, color, name })
                })
                .collect(),
            _ => Vec::new(),
        };
        let mut map = Self {
            scale: (int("scale").unwrap_or(0).clamp(0, MAX_SCALE as i64)) as u8,
            center: [int("xCenter")? as i32, int("zCenter")? as i32],
            dimension: match c.get("dimension") {
                Some(Tag::String(s)) => s.clone(),
                _ => "minecraft:overworld".into(),
            },
            tracking: flag("trackingPosition", true),
            unlimited: flag("unlimitedTracking", false),
            locked: flag("locked", false),
            colors,
            holders: Vec::new(),
            decorations: Vec::new(),
            frames: Vec::new(),
            banners: Vec::new(),
            dirty: false,
        };
        // `banners` читаются раньше рамок.
        for b in banners {
            map.add_banner(b);
        }
        // Как при чтении у ванили: метки рамок и их значки (без вращения в Аду).
        for f in frames {
            map.put_decoration(
                DecoKind::Frame,
                &frame_key(f.entity_id),
                f.pos[0] as f64,
                f.pos[2] as f64,
                f.rotation as f64,
                None,
            );
            if !map.frames.iter().any(|g| g.id() == f.id()) {
                map.frames.push(f);
            }
        }
        Some(map)
    }
}

/// `MapColor.getPackedId`: номер цвета × 4 + яркость (LOW 0, NORMAL 1, HIGH 2).
pub fn packed(color: u8, brightness: u8) -> u8 {
    color.wrapping_mul(4).wrapping_add(brightness)
}

/// Потолочное измерение: земля или камень по хэшу координат.
fn ceiling_sample(x: i32, z: i32) -> Sample {
    let mut h = x.wrapping_add(z.wrapping_mul(231871));
    h = h
        .wrapping_mul(h)
        .wrapping_mul(31287121)
        .wrapping_add(h.wrapping_mul(11));
    Sample {
        color: if (h >> 20) & 1 == 0 { DIRT } else { STONE },
        height: 100.0,
        depth: 0,
    }
}

/// Самый частый цвет; при равенстве — встреченный раньше
/// (`Multisets.copyHighestCountFirst` над `LinkedHashMultiset`).
pub fn mode_color(colors: impl IntoIterator<Item = u8>) -> u8 {
    let mut counts: Vec<(u8, u32)> = Vec::new();
    for c in colors {
        match counts.iter_mut().find(|(k, _)| *k == c) {
            Some((_, n)) => *n += 1,
            None => counts.push((c, 1)),
        }
    }
    let mut best = (NONE, 0);
    for &(c, n) in &counts {
        if n > best.1 {
            best = (c, n);
        }
    }
    best.0
}

fn inside(x: f32, z: f32) -> bool {
    (-63.0..=63.0).contains(&x) && (-63.0..=63.0).contains(&z)
}

fn clamp_coord(v: f32) -> i8 {
    if v <= -63.0 {
        -128
    } else if v >= 63.0 {
        127
    } else {
        ((v * 2.0) as f64 + 0.5) as i32 as i8
    }
}

/// `calculateRotation`: в Аду стрелка крутится от времени, иначе — по взгляду.
fn rotation(yaw: f64, nether_time: Option<i64>) -> i8 {
    if let Some(t) = nether_time {
        let i = (t / 10) as i32;
        return ((i
            .wrapping_mul(i)
            .wrapping_mul(34187121)
            .wrapping_add(i.wrapping_mul(121))
            >> 15)
            & 15) as i8;
    }
    let d = if yaw < 0.0 { yaw - 8.0 } else { yaw + 8.0 };
    (d * 16.0 / 360.0) as i32 as i8
}

/// `ClientboundMapItemDataPacket`.
pub fn encode(packet_id: i32, map_id: i32, u: &Update) -> Vec<u8> {
    let mut w = chunkwake_protocol::Writer::new();
    w.var_int(packet_id)
        .var_int(map_id)
        .u8(u.scale)
        .bool(u.locked);
    match &u.decorations {
        Some(list) => {
            w.bool(true).var_int(list.len() as i32);
            for d in list {
                w.var_int(d.kind.id())
                    .u8(d.x as u8)
                    .u8(d.z as u8)
                    .u8((d.rot & 15) as u8);
                match &d.name {
                    Some(n) => {
                        w.bool(true);
                        chunkwake_protocol::text::write_component(
                            &mut w,
                            &crate::stack_nbt::text_json(n),
                        );
                    }
                    None => {
                        w.bool(false);
                    }
                }
            }
        }
        None => {
            w.bool(false);
        }
    }
    match &u.patch {
        Some(p) => {
            w.u8(p.width)
                .u8(p.height)
                .u8(p.x)
                .u8(p.z)
                .byte_array(&p.colors);
        }
        None => {
            w.u8(0);
        }
    }
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_snaps_to_grid() {
        assert_eq!(center_for(0.0, 0.0, 0), [0, 0]);
        assert_eq!(center_for(-65.0, 63.0, 0), [-128, 0]);
        assert_eq!(center_for(100.0, -200.0, 0), [128, -256]);
        assert_eq!(center_for(0.0, 0.0, 1), [64, 64]);
        assert_eq!(center_for(300.0, 0.0, 2), [192, 192]);
    }

    #[test]
    fn scaled_map_keeps_center_cell() {
        let m = MapData::fresh(10.0, 10.0, 0, true, false, "minecraft:overworld");
        let s = m.scaled();
        assert_eq!(s.scale, 1);
        assert_eq!(s.center, [64, 64]);
        assert_eq!(s.scaled().center, [192, 192]);
        assert!(s.colors.iter().all(|&c| c == 0));
    }

    #[test]
    fn first_update_draws_a_column_band_and_sends_full_patch() {
        let me = Uuid::from_u128(1);
        let mut m = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        let mut calls = 0;
        // Ровная трава на высоте 64.
        m.update(me, [0.5, 0.5], false, |_, _| {
            calls += 1;
            Some(Sample {
                color: 1,
                height: 64.0,
                depth: 0,
            })
        });
        // step = 1: первый столбец x ≡ 1 (mod 16), дальше подряд, пока
        // предыдущий менялся, — на чистой карте до конца.
        assert!(calls > 0);
        let drawn: Vec<usize> = (0..SIZE)
            .filter(|&x| m.colors[x + 64 * SIZE] != 0)
            .collect();
        assert_eq!(drawn, (1..SIZE).collect::<Vec<_>>());
        // Второй проход: всё уже нарисовано — только столбцы x ≡ 2.
        calls = 0;
        m.update(me, [0.5, 0.5], false, |_, _| {
            calls += 1;
            Some(Sample {
                color: 1,
                height: 64.0,
                depth: 0,
            })
        });
        assert_eq!(calls, 8 * 129);
        // Ровно: яркость NORMAL или по шахматке.
        let c = m.colors[1 + 64 * SIZE];
        assert!(c == packed(1, 1) || c == packed(1, 0) || c == packed(1, 2));
        let u = m.next_update(me).unwrap();
        let p = u.patch.unwrap();
        assert_eq!((p.width, p.height), (128, 128));
        assert!(u.decorations.is_some());
        // Без изменений второй пакет пуст.
        assert!(m.next_update(me).is_none());
    }

    #[test]
    fn slope_brightens_and_water_depth_darkens() {
        let me = Uuid::from_u128(2);
        let mut m = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        // Подъём на блок за пиксель к югу — светлее (HIGH).
        m.update(me, [0.5, 0.5], false, |_, z| {
            Some(Sample {
                color: 1,
                height: z as f64,
                depth: 0,
            })
        });
        assert_eq!(m.colors[1 + 64 * SIZE], packed(1, 2));
        let mut w = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        w.update(me, [0.5, 0.5], false, |_, _| {
            Some(Sample {
                color: WATER,
                height: 60.0,
                depth: 10,
            })
        });
        assert_eq!(w.colors[1 + 64 * SIZE], packed(WATER, 0));
        let mut s = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        s.update(me, [0.5, 0.5], false, |_, _| {
            Some(Sample {
                color: WATER,
                height: 60.0,
                depth: 1,
            })
        });
        assert_eq!(s.colors[1 + 64 * SIZE], packed(WATER, 2));
    }

    #[test]
    fn unloaded_chunks_stay_blank() {
        let me = Uuid::from_u128(3);
        let mut m = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        m.update(me, [0.5, 0.5], false, |_, _| None);
        assert!(m.colors.iter().all(|&c| c == 0));
    }

    #[test]
    fn player_marker_inside_and_off_map() {
        let mut m = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        m.put_player("a", 0.0, 0.0, 0.0, None);
        assert_eq!(
            m.decorations[0].1,
            Decoration {
                kind: DecoKind::Player,
                x: 0,
                z: 0,
                rot: 0,
                name: None
            }
        );
        m.put_player("a", 200.0, 0.0, 90.0, None);
        assert_eq!(m.decorations[0].1.kind, DecoKind::PlayerOffMap);
        assert_eq!(m.decorations[0].1.x, 127);
        assert_eq!(m.decorations[0].1.rot, 4);
        m.put_player("a", 1000.0, 0.0, 0.0, None);
        assert!(m.decorations.is_empty());
    }

    #[test]
    fn nbt_round_trip() {
        let mut m = MapData::fresh(500.0, -500.0, 2, true, true, "minecraft:the_nether");
        m.colors[5] = 77;
        let back = MapData::from_nbt(&m.to_nbt()).unwrap();
        assert_eq!(back.center, m.center);
        assert_eq!(back.scale, 2);
        assert!(back.unlimited && back.tracking && !back.locked);
        assert_eq!(back.colors[5], 77);
        assert_eq!(back.dimension, "minecraft:the_nether");
    }

    #[test]
    fn frame_marker_survives_save_and_moves_with_new_entity_id() {
        let mut m = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        let f = MapFrame {
            pos: [10, 70, -6],
            rotation: 90,
            entity_id: 5,
        };
        m.tick_frame(f, None);
        let deco = |m: &MapData, k: &str| {
            m.decorations
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, d)| d.clone())
        };
        let d = deco(&m, "frame-5").expect("значок рамки");
        assert_eq!((d.kind, d.x, d.z, d.rot), (DecoKind::Frame, 20, -11, 4));
        // После перезапуска у рамки другой номер: старый значок уходит.
        let mut back = MapData::from_nbt(&m.to_nbt()).unwrap();
        assert_eq!(back.frames, vec![f]);
        assert!(deco(&back, "frame-5").is_some());
        back.tick_frame(MapFrame { entity_id: 9, ..f }, None);
        assert!(deco(&back, "frame-5").is_none() && deco(&back, "frame-9").is_some());
        back.removed_from_frame(f.pos, 9);
        assert!(back.frames.is_empty() && back.decorations.is_empty());
        // Вне карты значок рамки снимается, на безграничной — остаётся у края.
        m.put_decoration(DecoKind::Frame, "far", 500.0, 0.0, 0.0, None);
        assert!(deco(&m, "far").is_none());
        m.unlimited = true;
        m.put_decoration(DecoKind::Frame, "far", 500.0, 0.0, 0.0, None);
        assert_eq!(deco(&m, "far").map(|d| d.x), Some(127));
    }

    #[test]
    fn frame_limit_counts_existing_map_decorations() {
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        for id in 0..TRACKED_LIMIT {
            map.put_decoration(DecoKind::Frame, &format!("frame-{id}"), 0.0, 0.0, 0.0, None);
        }
        assert!(map.tracked_count_over_limit());
        map.remove_decoration("frame-0");
        assert!(!map.tracked_count_over_limit());
    }

    #[test]
    fn mode_prefers_first_on_tie() {
        assert_eq!(mode_color([3, 4, 4, 3]), 3);
        assert_eq!(mode_color([3, 4, 4]), 4);
    }

    /// `toggleBanner`: второй клик снимает метку; метки переживают
    /// сохранение, белый цвет не пишется; имя уходит в пакет.
    #[test]
    fn banner_toggle_and_save() {
        let mut m = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        let red = MapBanner {
            pos: [10, 70, -6],
            color: 14,
            name: Some(r#"{"text":"База"}"#.into()),
        };
        assert!(m.toggle_banner(red.clone()));
        let d = m.decorations[0].1.clone();
        assert_eq!((d.kind.id(), d.x, d.z, d.rot), (24, 21, -10, 8));
        assert_eq!(d.name.as_deref(), red.name.as_deref());
        let white = MapBanner {
            pos: [1, 64, 1],
            color: 0,
            name: None,
        };
        assert!(m.toggle_banner(white.clone()));
        let nbt = m.to_nbt();
        let back = MapData::from_nbt(&nbt).unwrap();
        assert_eq!(back.banners, vec![red.clone(), white.clone()]);
        assert_eq!(back.decorations.len(), 2);
        let Some(Tag::List(l)) = nbt.get("banners") else {
            panic!()
        };
        assert!(l[1].as_compound().unwrap().get("color").is_none());
        let u = Update {
            scale: 0,
            locked: false,
            decorations: Some(vec![d]),
            patch: None,
        };
        assert!(encode(1, 0, &u).len() > 10);
        // Повторный клик по тому же знамени снимает метку.
        assert!(m.toggle_banner(red.clone()));
        assert_eq!(m.banners, vec![white.clone()]);
        assert_eq!(m.banners_in(&[(1, 1)]), vec![white]);
        // Дальше 63 пикселей от центра — отказ.
        assert!(!m.toggle_banner(MapBanner {
            pos: [200, 64, 0],
            color: 0,
            name: None
        }));
    }

    #[test]
    fn packet_layout() {
        let u = Update {
            scale: 1,
            locked: false,
            decorations: Some(vec![Decoration {
                kind: DecoKind::Player,
                x: 1,
                z: -1,
                rot: 3,
                name: None,
            }]),
            patch: Some(Patch {
                x: 2,
                z: 3,
                width: 1,
                height: 1,
                colors: vec![9],
            }),
        };
        let b = encode(52, 7, &u);
        assert_eq!(
            b,
            vec![52, 7, 1, 0, 1, 1, 0, 1, 255, 3, 0, 1, 1, 2, 3, 1, 9]
        );
    }
}
