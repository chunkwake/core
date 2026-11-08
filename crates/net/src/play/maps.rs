//! Карты в игре: пустая карта (`EmptyMapItem.use`), обновление в руке
//! (`MapItem.inventoryTick` → `update`), значки держателей
//! (`MapItemSavedData.tickCarriedBy`), пакеты и запись в
//! `data/minecraft/maps/<id>.dat` с `last_id.dat` (`MapIndex`).
//!
//! Отрисовка и данные — [`crate::maps`], знамёна — [`super::banners`].

use super::*;
use crate::maps::{MapData, MapFrame, Sample, mode_color};

/// `MapColor.FIRE`: цвет лавы на карте.
const LAVA_COLOR: u8 = 4;

#[derive(Default)]
pub(super) struct MapStore {
    maps: FastMap<i32, MapData>,
    /// Номера, которых нет ни в памяти, ни на диске.
    missing: FastSet<i32>,
    /// `MapIndex.lastMapId`: `None` — ещё не читали `last_id.dat`.
    last_id: Option<i32>,
    last_id_dirty: bool,
    /// Номера карт в инвентаре каждого игрока (с прошлого тика).
    carried: FastMap<Uuid, Vec<i32>>,
    /// Игровое время последнего звука стола картографа.
    last_sound: i64,
}

impl MapStore {
    fn carries(&self, player: Uuid, id: i32) -> bool {
        self.carried.get(&player).is_some_and(|v| v.contains(&id))
    }
}

impl PlayPlan {
    /// `hasMapInvisibilityItemEquipped`: предмет на голове из тега данных.
    fn map_icon_hidden(&self, head: Slot) -> bool {
        head.is_some_and(|s| {
            self.survival
                .item_tags
                .get("minecraft:map_invisibility_equipment")
                .is_some_and(|tag| tag.contains(self.item_name(s.item)))
        })
    }

    fn map_hidden_players(&self) -> FastSet<Uuid> {
        self.live
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(&uuid, live)| self.map_icon_hidden(live.slots[survival::HEAD]).then_some(uuid))
            .collect()
    }

    /// Каталог `data/minecraft/maps` мира (зовёт сервер при запуске).
    pub fn set_maps_dir(&self, dir: std::path::PathBuf) {
        let _ = self.maps_dir.set(dir);
    }

    /// Карта в памяти; при первом обращении читается с диска.
    fn with_map<R>(&self, store: &mut MapStore, id: i32, f: impl FnOnce(&mut MapData) -> R) -> Option<R> {
        if !store.maps.contains_key(&id) {
            if store.missing.contains(&id) {
                return None;
            }
            let loaded = self.maps_dir.get().and_then(|dir| {
                let root = chunkwake_world::nbt::read_gzip_file(&dir.join(format!("{id}.dat")))
                    .map_err(|e| tracing::warn!("map {id}: {e}"))
                    .ok()??;
                root.get("data").and_then(Tag::as_compound).and_then(MapData::from_nbt)
            });
            match loaded {
                Some(m) => {
                    store.maps.insert(id, m);
                }
                None => {
                    store.missing.insert(id);
                    return None;
                }
            }
        }
        store.maps.get_mut(&id).map(f)
    }

    /// `ServerLevel.getFreeMapId`: следующий номер после `last_id.dat`.
    fn free_map_id(&self, store: &mut MapStore) -> i32 {
        let last = *store.last_id.get_or_insert_with(|| {
            self.maps_dir
                .get()
                .and_then(|dir| chunkwake_world::nbt::read_gzip_file(&dir.join("last_id.dat")).ok()?)
                .and_then(|root| {
                    root.get("data")?
                        .as_compound()?
                        .get("map")?
                        .as_i64()
                })
                .map_or(-1, |v| v as i32)
        });
        let id = last + 1;
        store.last_id = Some(id);
        store.last_id_dirty = true;
        id
    }

    /// `MapItem.create`: новая карта с центром около `(x, z)`, её номер.
    pub(crate) fn create_map(&self, x: i32, z: i32, scale: u8, tracking: bool, unlimited: bool) -> i32 {
        let dim = self.active_dim().key();
        let mut store = self.maps.lock().unwrap();
        let id = self.free_map_id(&mut store);
        store.missing.remove(&id);
        store
            .maps
            .insert(id, MapData::fresh(x as f64, z as f64, scale, tracking, unlimited, dim));
        id
    }

    /// Новая карта из имеющейся (`scaled` / `locked` у стола картографа).
    pub(crate) fn derive_map(&self, from: i32, f: impl FnOnce(&MapData) -> MapData) -> Option<i32> {
        let mut store = self.maps.lock().unwrap();
        let data = self.with_map(&mut store, from, |m| f(m))?;
        let id = self.free_map_id(&mut store);
        store.missing.remove(&id);
        store.maps.insert(id, data);
        Some(id)
    }

    /// Масштаб и замок карты (для стола картографа).
    pub(crate) fn map_info(&self, id: i32) -> Option<(u8, bool)> {
        let mut store = self.maps.lock().unwrap();
        self.with_map(&mut store, id, |m| (m.scale, m.locked))
    }

    /// Стопка заполненной карты с номером.
    pub(crate) fn filled_map(&self, id: i32) -> Option<Stack> {
        let item = self.item_id("minecraft:filled_map")?;
        let mut s = Stack::new(item, 1);
        s.extra = chunkwake_protocol::components::intern(chunkwake_protocol::components::Extra {
            map_id: Some(id),
            ..Default::default()
        });
        Some(s)
    }

    /// Номер карты стопки, если это заполненная карта.
    pub(crate) fn map_id_of(&self, s: Stack) -> Option<i32> {
        if s.extra == 0 || self.item_name(s.item) != "minecraft:filled_map" {
            return None;
        }
        chunkwake_protocol::components::get(s.extra).map_id
    }

    /// `EmptyMapItem.use`: карта с центром у игрока, звук, стопка в руку или в инвентарь.
    pub(super) async fn use_empty_map(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        inv: &mut survival::Inventory,
        slot: usize,
        creative: bool,
    ) -> Result<(), NetError> {
        let Some((feet, yaw, pitch)) = self.player_at(uuid) else {
            return Ok(());
        };
        if !creative {
            inv.consume_at(slot);
        }
        self.player_sound(uuid, "minecraft:ui.cartography_table.take_result", 1.0, 1.0);
        let id = self.create_map(feet[0].floor() as i32, feet[2].floor() as i32, 0, true, false);
        let Some(map) = self.filled_map(id) else {
            return Ok(());
        };
        if inv.slots[slot].is_none() {
            inv.slots[slot] = Some(map);
            return self.send_slots(conn, inv, &[slot]).await;
        }
        let max = self.max_stack(map.item);
        let (mut changed, left) = inv.add_components(map.item, 1, max, map.extra, 0);
        if left > 0 {
            self.throw(feet, yaw, pitch, map);
        }
        if !creative {
            changed.push(slot);
        }
        self.send_slots(conn, inv, &changed).await
    }

    /// Жидкость каждого состояния: 0 — нет, 1 — вода, 2 — лава.
    fn map_fluid_table(&self) -> &[u8] {
        self.map_fluids.get_or_init(|| {
            let pl = &self.survival.placement;
            (0..self.survival.map_colors.len() as u32)
                .map(|s| match pl.name(s).unwrap_or("") {
                    "minecraft:lava" => 2,
                    "minecraft:water" | "minecraft:bubble_column" | "minecraft:kelp"
                    | "minecraft:kelp_plant" | "minecraft:seagrass" | "minecraft:tall_seagrass" => 1,
                    _ if pl.prop(s, "waterlogged") == Some("true") => 1,
                    _ => 0,
                })
                .collect()
        })
    }

    /// Квадрат `scale×scale` столбцов с углом `(x, z)` (тело `MapItem.update`).
    fn map_sample(&self, col: &ChunkColumn, x: i32, z: i32, scale: i32) -> Sample {
        let colors = &self.survival.map_colors;
        let fluids = self.map_fluid_table();
        let color_of = |s: u32| colors.get(s as usize).copied().unwrap_or(0);
        let fluid_of = |s: u32| fluids.get(s as usize).copied().unwrap_or(0);
        let min_y = self.min_y();
        let at = |lx: usize, y: usize, lz: usize| {
            col.sections[y / 16].get(((y % 16) * 16 + lz) * 16 + lx)
        };
        let mut seen = Vec::with_capacity((scale * scale) as usize);
        let (mut height, mut depth) = (0.0f64, 0i32);
        for dx in 0..scale {
            for dz in 0..scale {
                let (lx, lz) = (((x + dx) & 15) as usize, ((z + dz) & 15) as usize);
                // Сверху вниз до первого блока с цветом.
                let mut found = None;
                'down: for si in (0..col.sections.len()).rev() {
                    let section = &col.sections[si];
                    if section.uniform_state().is_some_and(|s| color_of(s) == 0) {
                        continue;
                    }
                    for ly in (0..16).rev() {
                        let s = section.get((ly * 16 + lz) * 16 + lx);
                        if color_of(s) != 0 {
                            found = Some((s, si * 16 + ly));
                            break 'down;
                        }
                    }
                }
                let (mut color, y) = match found {
                    Some((s, y)) => (color_of(s), y),
                    // Пустой столбец: `Blocks.BEDROCK` на дне (цвет камня).
                    None => (11, 0),
                };
                if let Some((s, _)) = found
                    && y > 0
                    && fluid_of(s) != 0
                {
                    let mut below = y as i32 - 1;
                    loop {
                        let b = at(lx, below as usize, lz);
                        below -= 1;
                        depth += 1;
                        if below <= 0 || fluid_of(b) == 0 {
                            break;
                        }
                    }
                    // `getCorrectStateForFluidBlock`: верх не держит — видна жидкость.
                    if !self.survival.placement.sturdy(s, 1) {
                        color = if fluid_of(s) == 2 { LAVA_COLOR } else { crate::maps::WATER };
                    }
                }
                height += (min_y + y as i32) as f64 / (scale * scale) as f64;
                seen.push(color);
            }
        }
        Sample {
            color: mode_color(seen),
            height,
            depth: depth / (scale * scale),
        }
    }

    /// Карты в инвентаре игрока за тик: держатели, значки, отрисовка в руке, пакеты.
    pub(super) async fn tick_maps(
        &self,
        conn: &mut Connection,
        uuid: Uuid,
        inv: &survival::Inventory,
    ) -> Result<(), NetError> {
        let mut ids: Vec<i32> = Vec::new();
        for s in inv.slots.iter().flatten() {
            if let Some(id) = self.map_id_of(*s)
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        let hands: Vec<i32> = [inv.held_slot(), survival::OFFHAND]
            .iter()
            .filter_map(|&i| inv.slots[i].and_then(|s| self.map_id_of(s)))
            .collect();
        {
            let mut store = self.maps.lock().unwrap();
            if ids.is_empty() {
                store.carried.remove(&uuid);
            } else {
                store.carried.insert(uuid, ids.clone());
            }
        }
        if ids.is_empty() {
            return Ok(());
        }
        // Держатели: имя, место и измерение игроков с картой.
        let me_dim = self.active_dim();
        let online: Vec<(Uuid, String, [f64; 3], f32, Dimension)> = {
            let players = self.hub.players.lock().unwrap();
            players
                .values()
                .map(|p| (p.entry.uuid, p.entry.name.clone(), p.pos, p.yaw, p.dimension))
                .collect()
        };
        let Some(&(_, _, my_pos, _, _)) = online.iter().find(|p| p.0 == uuid) else {
            return Ok(());
        };
        let mut hidden = self.map_hidden_players();
        if self.map_icon_hidden(inv.slots[survival::HEAD]) {
            hidden.insert(uuid);
        } else {
            hidden.remove(&uuid);
        }
        let now = self.game_time.load(Ordering::Relaxed);
        let mut packets = Vec::new();
        for id in ids {
            let mut store = self.maps.lock().unwrap();
            let carried: Vec<(Uuid, bool)> = online.iter().map(|p| (p.0, store.carries(p.0, id))).collect();
            let Some(dim_key) = self.with_map(&mut store, id, |m| m.dimension.clone()) else {
                continue;
            };
            let map_dim = Dimension::from_key(&dim_key);
            let nether = map_dim == Some(Dimension::Nether);
            let map = store.maps.get_mut(&id).expect("загружена выше");
            // `tickCarriedBy`.
            map.holder(uuid);
            let holders: Vec<Uuid> = map.holders.iter().map(|h| h.player).collect();
            for h in holders {
                let info = online.iter().find(|p| p.0 == h);
                let still = carried.iter().any(|&(p, c)| p == h && c);
                match info {
                    Some((_, name, pos, yaw, dim)) if still => {
                        if hidden.contains(&h) {
                            map.remove_decoration(name);
                        } else if map.tracking && Some(*dim) == map_dim {
                            map.put_player(name, pos[0], pos[2], *yaw, nether.then_some(now));
                        }
                    }
                    _ => {
                        map.holders.retain(|x| x.player != h);
                        if let Some((_, name, ..)) = info {
                            map.remove_decoration(name);
                        }
                    }
                }
            }
            // `inventoryTick`: рисует только карта в руке, в своём измерении.
            if hands.contains(&id) && !map.locked && map_dim == Some(me_dim) {
                let scale = 1i32 << map.scale;
                let world = self.dim_state().edited.read().unwrap();
                let flat = self.terrain().is_none();
                let ceiling = me_dim == Dimension::Nether;
                // Углы перерисованных квадратов: в них `checkBanners`.
                let mut corners = Vec::new();
                map.update(uuid, [my_pos[0], my_pos[2]], ceiling, |x, z| {
                    let col = match world.get(&(x >> 4, z >> 4)) {
                        Some(e) => &e.0,
                        None if flat => self.template(),
                        None => return None,
                    };
                    if ceiling {
                        return Some(Sample { color: 0, height: 0.0, depth: 0 });
                    }
                    corners.push((x, z));
                    Some(self.map_sample(col, x, z, scale))
                });
                drop(world);
                if !map.banners.is_empty() {
                    self.check_map_banners(map, &corners);
                }
            }
            if let Some(u) = map.next_update(uuid) {
                packets.push(crate::maps::encode(self.ids.map_item_data, id, &u));
            }
        }
        for p in packets {
            conn.send(&p).await?;
        }
        Ok(())
    }

    /// Карта в рамке раз в 10 тиков (`ServerEntity.sendChanges`): каждому
    /// игроку измерения — `tickCarriedBy` с рамкой и его пакет.
    pub(super) fn tick_framed_map(&self, id: i32, frame: MapFrame) {
        let me_dim = self.active_dim();
        let hidden = self.map_hidden_players();
        let online: Vec<(Uuid, String, Dimension)> = {
            let players = self.hub.players.lock().unwrap();
            players
                .values()
                .map(|p| (p.entry.uuid, p.entry.name.clone(), p.dimension))
                .collect()
        };
        let now = self.game_time.load(Ordering::Relaxed);
        let mut packets = Vec::new();
        {
            let mut store = self.maps.lock().unwrap();
            let carried: Vec<(Uuid, bool)> = online.iter().map(|p| (p.0, store.carries(p.0, id))).collect();
            if self.with_map(&mut store, id, |_| ()).is_none() {
                return;
            }
            let map = store.maps.get_mut(&id).expect("загружена выше");
            for (uuid, name, dim) in online.iter().filter(|p| p.2 == me_dim) {
                map.holder(*uuid);
                if hidden.contains(uuid) || !carried.iter().any(|&(p, c)| p == *uuid && c) {
                    map.remove_decoration(name);
                }
                // С рамкой держатели не проверяются на карту в инвентаре:
                // уходят только вышедшие из игры.
                let gone: Vec<Uuid> = map
                    .holders
                    .iter()
                    .map(|h| h.player)
                    .filter(|h| !online.iter().any(|p| p.0 == *h))
                    .collect();
                for h in gone {
                    map.holders.retain(|x| x.player != h);
                }
                map.tick_frame(frame, (*dim == Dimension::Nether).then_some(now));
                if let Some(u) = map.next_update(*uuid) {
                    packets.push((*uuid, crate::maps::encode(self.ids.map_item_data, id, &u)));
                }
            }
        }
        for (to, p) in packets {
            self.send_to(to, p);
        }
    }

    /// `toggleBanner` у карты `id`: карты нет — успех без дела, знамени
    /// (`banner` = `None`) нет — отказ.
    pub(super) fn toggle_map_banner(&self, id: i32, banner: Option<crate::maps::MapBanner>) -> bool {
        let mut store = self.maps.lock().unwrap();
        match self.with_map(&mut store, id, |m| banner.map(|b| m.toggle_banner(b))) {
            None => true,
            Some(r) => r.unwrap_or(false),
        }
    }

    /// `ItemFrame.removeFramedMap`: карта ушла из рамки.
    pub(super) fn map_removed_from_frame(&self, id: i32, pos: [i32; 3], entity_id: i32) {
        let mut store = self.maps.lock().unwrap();
        self.with_map(&mut store, id, |m| m.removed_from_frame(pos, entity_id));
    }

    /// `ItemFrame.interact`: проверка лимита значков до вставки карты.
    pub(super) fn map_has_too_many_decorations(&self, id: i32) -> bool {
        let mut store = self.maps.lock().unwrap();
        self.with_map(&mut store, id, |m| m.tracked_count_over_limit())
            .unwrap_or(false)
    }

    /// Запись изменённых карт и `last_id.dat` (из автосохранения).
    pub fn save_maps(&self, data_version: i32) -> Result<usize, String> {
        let Some(dir) = self.maps_dir.get() else {
            return Ok(0);
        };
        let (dirty, last): (Vec<(i32, Compound)>, Option<i32>) = {
            let mut store = self.maps.lock().unwrap();
            let dirty = store
                .maps
                .iter_mut()
                .filter(|(_, m)| m.dirty)
                .map(|(id, m)| {
                    m.dirty = false;
                    (*id, m.to_nbt())
                })
                .collect();
            let last = store.last_id_dirty.then_some(store.last_id).flatten();
            store.last_id_dirty = false;
            (dirty, last)
        };
        if dirty.is_empty() && last.is_none() {
            return Ok(0);
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let wrap = |data: Compound| {
            Compound::from([
                ("data".into(), Tag::Compound(data)),
                ("DataVersion".into(), Tag::Int(data_version)),
            ])
        };
        let mut failed = Vec::new();
        for (id, data) in &dirty {
            if let Err(e) = chunkwake_world::nbt::write_gzip_file(&dir.join(format!("{id}.dat")), &wrap(data.clone())) {
                failed.push(*id);
                tracing::warn!("map {id}: {e}");
            }
        }
        if let Some(last) = last {
            let data = Compound::from([("map".into(), Tag::Int(last))]);
            if let Err(e) = chunkwake_world::nbt::write_gzip_file(&dir.join("last_id.dat"), &wrap(data)) {
                self.maps.lock().unwrap().last_id_dirty = true;
                return Err(e);
            }
        }
        if !failed.is_empty() {
            let mut store = self.maps.lock().unwrap();
            for id in &failed {
                if let Some(m) = store.maps.get_mut(id) {
                    m.dirty = true;
                }
            }
            return Err(format!("{} maps failed", failed.len()));
        }
        Ok(dirty.len())
    }
}

impl PlayPlan {
    /// `CartographyTableMenu.setupResultSlot`: увеличить (бумага), запереть
    /// (стекло), скопировать (пустая карта).
    pub(super) fn cartography_result(&self, map: Slot, extra: Slot) -> Slot {
        let (map, extra) = (map?, extra?);
        let id = self.map_id_of(map)?;
        let (scale, locked) = self.map_info(id)?;
        let with_post = |p: u8| {
            let mut e = (*chunkwake_protocol::components::get(map.extra)).clone();
            e.map_post_processing = Some(p);
            Stack {
                count: 1,
                extra: chunkwake_protocol::components::intern(e),
                ..map
            }
        };
        match self.item_name(extra.item) {
            "minecraft:paper" if !locked && scale < crate::maps::MAX_SCALE => Some(with_post(1)),
            "minecraft:glass_pane" if !locked => Some(with_post(0)),
            "minecraft:map" => Some(Stack { count: 2, ..map }),
            _ => None,
        }
    }

    /// `MapItem.onCraftedPostProcess`: снять пометку стола и завести новую карту.
    pub(super) fn finish_map_processing(&self, s: &mut Stack) {
        let Some(id) = self.map_id_of(*s) else {
            return;
        };
        let mut e = (*chunkwake_protocol::components::get(s.extra)).clone();
        let Some(post) = e.map_post_processing.take() else {
            return;
        };
        let new = match post {
            1 => self.derive_map(id, MapData::scaled),
            _ => self.derive_map(id, MapData::locked_copy),
        };
        if let Some(new) = new {
            e.map_id = Some(new);
        }
        s.extra = chunkwake_protocol::components::intern(e);
    }

    /// Звук забранного результата, не чаще раза за тик (`lastSoundTime`).
    pub(super) fn cartography_sound(&self, pos: [i32; 3]) {
        let now = self.game_time.load(Ordering::Relaxed);
        {
            let mut store = self.maps.lock().unwrap();
            if store.last_sound == now {
                return;
            }
            store.last_sound = now;
        }
        self.block_sound(None, "minecraft:ui.cartography_table.take_result", pos, 1.0, 1.0);
    }
}
