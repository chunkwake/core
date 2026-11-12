//! Поводки (`Leashable`, `LeadItem`, `LeashFenceKnotEntity`).
//!
//! Привязь живёт в [`crate::leash::Leash`] у моба или лодки: к игроку
//! (`LeadItem` по сущности), к узлу на заборе (`use_item_on` по забору или
//! по узлу). Каждый тик держатель ищется по UUID/блоку; дальше 12 блоков
//! поводок рвётся и падает предметом, ближе 6 минус ширины — тянет к
//! держателю (`AXIS_SPECIFIC_ELASTICITY`).

use super::*;
use crate::leash::{self, Leash, LeashHolder};

/// Узел на заборе (`LeashFenceKnotEntity`): виден клиентам как
/// `minecraft:leash_knot` в точке `y + 0.375`.
#[derive(Clone, Copy, Debug)]
pub struct Knot {
    pub pos: [f64; 3],
    pub block: [i32; 3],
    pub uuid: Uuid,
}

impl PlayPlan {
    fn knot_type(&self) -> Option<i32> {
        self.survival
            .entity_types
            .get("minecraft:leash_knot")
            .copied()
    }

    /// Найти узел в блоке или создать его (`getOrCreateKnot`).
    /// `true` — узел только что появился (нужно разослать спавн).
    pub(super) fn ensure_knot(&self, block: [i32; 3]) -> (i32, bool) {
        {
            let knots = self.sim().knots.lock().unwrap();
            if let Some((&id, _)) = knots.iter().find(|(_, k)| k.block == block) {
                return (id, false);
            }
        }
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        let pos = [
            block[0] as f64 + 0.5,
            block[1] as f64 + 0.375,
            block[2] as f64 + 0.5,
        ];
        let knot = Knot {
            pos,
            block,
            uuid: Uuid::from_u128(rand::random()),
        };
        self.sim().knots.lock().unwrap().insert(id, knot);
        if let Some(ty) = self.knot_type() {
            let range = self.tracking_range(ty);
            self.hub.spawn_tracked(
                Tracked { id, pos, range },
                [play::add_entity_moving(
                    self.ids.add_entity,
                    id,
                    knot.uuid,
                    ty,
                    pos,
                    [0.0; 3],
                    0.0,
                    0.0,
                    0,
                )],
            );
        }
        (id, true)
    }

    /// Блок узла по ID сущности.
    pub(super) fn knot_block(&self, id: i32) -> Option<[i32; 3]> {
        self.sim().knots.lock().unwrap().get(&id).map(|k| k.block)
    }

    pub(super) fn track_knots(&self, out: &mut Vec<Tracked>) {
        let Some(ty) = self.knot_type() else {
            return;
        };
        let range = self.tracking_range(ty);
        for (&id, k) in self.sim().knots.lock().unwrap().iter() {
            out.push(Tracked {
                id,
                pos: k.pos,
                range,
            });
        }
    }

    /// Пакеты спавна узла для нового зрителя.
    pub(super) fn knot_spawn(&self, id: i32) -> Option<Vec<Vec<u8>>> {
        let ty = self.knot_type()?;
        let knots = self.sim().knots.lock().unwrap();
        let k = knots.get(&id)?;
        Some(vec![play::add_entity_moving(
            self.ids.add_entity,
            id,
            k.uuid,
            ty,
            k.pos,
            [0.0; 3],
            0.0,
            0.0,
            0,
        )])
    }

    /// ID узла по блоку (для `set_entity_link`).
    pub(super) fn knot_id_at(&self, block: [i32; 3]) -> Option<i32> {
        self.sim()
            .knots
            .lock()
            .unwrap()
            .iter()
            .find(|(_, k)| k.block == block)
            .map(|(&id, _)| id)
    }

    fn remove_knot(&self, id: i32) {
        if self.sim().knots.lock().unwrap().remove(&id).is_some() {
            self.hub.remove_tracked(&[id]);
        }
    }

    /// ID держателя для `set_entity_link`: игрок, моб, лодка или узел.
    pub(super) fn leash_holder_id(&self, holder: LeashHolder) -> Option<i32> {
        match holder {
            LeashHolder::Entity(uuid) => {
                if let Some(p) = self.hub.players.lock().unwrap().get(&uuid) {
                    return Some(p.entity_id);
                }
                if let Some(id) = self
                    .sim()
                    .mobs
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|(_, e)| Uuid::from_u128(e.mob.uuid) == uuid)
                    .map(|(&id, _)| id)
                {
                    return Some(id);
                }
                self.sim()
                    .boats
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|(_, b)| b.uuid == uuid)
                    .map(|(&id, _)| id)
            }
            LeashHolder::Knot(block) => self.knot_id_at(block),
        }
    }

    /// `ServerEntity.sendPairingData`: поводок вместе со спавном сущности.
    pub(super) fn entity_leash_link(&self, id: i32) -> Option<Vec<u8>> {
        let leash = self
            .sim()
            .mobs
            .lock()
            .unwrap()
            .get(&id)
            .and_then(|e| e.mob.leash)
            .or_else(|| {
                self.sim()
                    .boats
                    .lock()
                    .unwrap()
                    .get(&id)
                    .and_then(|b| b.leash)
            })?;
        let holder = self.leash_holder_id(leash.holder)?;
        Some(play::set_entity_link(self.ids.set_entity_link, id, holder))
    }

    /// `LeadItem` по сущности: `Some(true)` — привязали (тратится предмет),
    /// `Some(false)` — отвязали/узел, `None` — не поводок.
    pub(super) fn leash_entity(
        &self,
        player: Uuid,
        player_id: i32,
        target: i32,
        creative: bool,
    ) -> Option<bool> {
        // Узел на заборе: привязать мобов игрока или снять с узла.
        if let Some(block) = self.knot_block(target) {
            let mut moved = Vec::new();
            {
                let mut mobs = self.sim().mobs.lock().unwrap();
                let ids: Vec<i32> = mobs.keys().copied().collect();
                for id in ids {
                    if let Some(e) = mobs.get_mut(&id)
                        && e.mob
                            .leash
                            .is_some_and(|l| l.holder == LeashHolder::Entity(player))
                    {
                        e.mob.leash = Some(Leash::to(LeashHolder::Knot(block)));
                        moved.push(id);
                    }
                }
            }
            {
                let mut boats = self.sim().boats.lock().unwrap();
                let ids: Vec<i32> = boats.keys().copied().collect();
                for id in ids {
                    if let Some(b) = boats.get_mut(&id)
                        && b.leash
                            .is_some_and(|l| l.holder == LeashHolder::Entity(player))
                    {
                        b.leash = Some(Leash::to(LeashHolder::Knot(block)));
                        moved.push(id);
                    }
                }
            }
            if moved.is_empty() && !self.sneaking(player) {
                // Пустая рука по узлу снимает мобов на игрока.
                for id in self.leashed_to_knot(block) {
                    let _ = self.leash_entity_force(id, player, player_id);
                }
                return Some(false);
            }
            let knot = self.ensure_knot(block).0;
            for id in moved {
                self.hub.send_about(
                    id,
                    play::set_entity_link(self.ids.set_entity_link, id, knot),
                );
            }
            self.play_sound(
                None,
                "minecraft:entity.lead.tied",
                crate::sounds::NEUTRAL,
                knot_pos(block),
                1.0,
                1.0,
            );
            return Some(false);
        }
        if self.sim().mobs.lock().unwrap().contains_key(&target) {
            return self.leash_mob(target, player, player_id, creative);
        }
        if self.sim().boats.lock().unwrap().contains_key(&target) {
            return self.leash_boat(target, player, player_id, creative);
        }
        None
    }

    /// `Entity.interact` для моба (`canBeLeashed`: не монстр).
    fn leash_mob(&self, target: i32, player: Uuid, player_id: i32, creative: bool) -> Option<bool> {
        let (pos, current) = {
            let mobs = self.sim().mobs.lock().unwrap();
            let e = mobs.get(&target)?;
            if !e.mob.alive() || e.mob.species.is_monster() {
                return None;
            }
            (e.mob.pos, e.mob.leash)
        };
        if let Some(l) = current {
            if l.holder == LeashHolder::Entity(player) {
                if let Some(e) = self.sim().mobs.lock().unwrap().get_mut(&target) {
                    e.mob.leash = None;
                }
                if !creative {
                    self.drop_lead(pos);
                }
                self.play_sound(
                    None,
                    "minecraft:entity.lead.untied",
                    crate::sounds::NEUTRAL,
                    pos,
                    1.0,
                    1.0,
                );
                self.hub.send_about(
                    target,
                    play::set_entity_link(self.ids.set_entity_link, target, 0),
                );
                return Some(false);
            }
            if self.is_player(l.holder) {
                return None;
            }
        }
        if let Some(e) = self.sim().mobs.lock().unwrap().get_mut(&target) {
            e.mob.leash = Some(Leash::to(LeashHolder::Entity(player)));
        }
        if current.is_some() && !creative {
            self.drop_lead(pos);
        }
        self.play_sound(
            None,
            "minecraft:entity.lead.tied",
            crate::sounds::NEUTRAL,
            pos,
            1.0,
            1.0,
        );
        self.hub.send_about(
            target,
            play::set_entity_link(self.ids.set_entity_link, target, player_id),
        );
        Some(true)
    }

    fn leash_boat(
        &self,
        target: i32,
        player: Uuid,
        player_id: i32,
        creative: bool,
    ) -> Option<bool> {
        let (pos, current) = {
            let boats = self.sim().boats.lock().unwrap();
            let b = boats.get(&target)?;
            (b.pos, b.leash)
        };
        if let Some(l) = current {
            if l.holder == LeashHolder::Entity(player) {
                if let Some(b) = self.sim().boats.lock().unwrap().get_mut(&target) {
                    b.leash = None;
                }
                if !creative {
                    self.drop_lead(pos);
                }
                self.play_sound(
                    None,
                    "minecraft:entity.lead.untied",
                    crate::sounds::NEUTRAL,
                    pos,
                    1.0,
                    1.0,
                );
                self.hub.send_about(
                    target,
                    play::set_entity_link(self.ids.set_entity_link, target, 0),
                );
                return Some(false);
            }
            if self.is_player(l.holder) {
                return None;
            }
        }
        if let Some(b) = self.sim().boats.lock().unwrap().get_mut(&target) {
            b.leash = Some(Leash::to(LeashHolder::Entity(player)));
        }
        if current.is_some() && !creative {
            self.drop_lead(pos);
        }
        self.play_sound(
            None,
            "minecraft:entity.lead.tied",
            crate::sounds::NEUTRAL,
            pos,
            1.0,
            1.0,
        );
        self.hub.send_about(
            target,
            play::set_entity_link(self.ids.set_entity_link, target, player_id),
        );
        Some(true)
    }

    fn is_player(&self, holder: LeashHolder) -> bool {
        matches!(holder, LeashHolder::Entity(u) if self.hub.players.lock().unwrap().contains_key(&u))
    }

    fn sneaking(&self, player: Uuid) -> bool {
        self.live
            .lock()
            .unwrap()
            .get(&player)
            .is_some_and(|l| l.sneaking)
    }

    fn drop_lead(&self, pos: [f64; 3]) {
        if !self.vars.flag("entity_drops") {
            return;
        }
        if let Some(item) = self.item_id("minecraft:lead") {
            self.drop_item(
                pos,
                items::pop_velocity(rand::random(), rand::random()),
                Stack::new(item, 1),
                items::BLOCK_DELAY,
            );
        }
    }

    /// Сущности, привязанные к узлу.
    fn leashed_to_knot(&self, block: [i32; 3]) -> Vec<i32> {
        let mut out = Vec::new();
        out.extend(
            self.sim()
                .mobs
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, e)| {
                    e.mob
                        .leash
                        .is_some_and(|l| l.holder == LeashHolder::Knot(block))
                })
                .map(|(&id, _)| id),
        );
        out.extend(
            self.sim()
                .boats
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, b)| {
                    b.leash
                        .is_some_and(|l| l.holder == LeashHolder::Knot(block))
                })
                .map(|(&id, _)| id),
        );
        out
    }

    fn leash_entity_force(&self, target: i32, player: Uuid, player_id: i32) -> Option<bool> {
        if self.sim().mobs.lock().unwrap().contains_key(&target) {
            let mut mobs = self.sim().mobs.lock().unwrap();
            let e = mobs.get_mut(&target)?;
            e.mob.leash = Some(Leash::to(LeashHolder::Entity(player)));
            drop(mobs);
        } else {
            let mut boats = self.sim().boats.lock().unwrap();
            let b = boats.get_mut(&target)?;
            b.leash = Some(Leash::to(LeashHolder::Entity(player)));
            drop(boats);
        }
        self.hub.send_about(
            target,
            play::set_entity_link(self.ids.set_entity_link, target, player_id),
        );
        Some(false)
    }

    /// `LeadItem.useOn` по забору: привязать мобов игрока к узлу в этом блоке.
    pub(super) fn leash_to_fence(&self, player: Uuid, player_id: i32, block: [i32; 3]) -> bool {
        let (knot, created) = self.ensure_knot(block);
        let center = [
            block[0] as f64 + 0.5,
            block[1] as f64 + 0.375,
            block[2] as f64 + 0.5,
        ];
        let mut attached = Vec::new();
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            let ids: Vec<i32> = mobs.keys().copied().collect();
            for id in ids {
                let Some(e) = mobs.get_mut(&id) else { continue };
                let (w, h) = e.mob.size();
                let c = crate::leash::center(e.mob.pos, h);
                let near = (c[0] - center[0]).powi(2)
                    + (c[1] - center[1]).powi(2)
                    + (c[2] - center[2]).powi(2)
                    <= 16.0 * 16.0;
                if near
                    && e.mob
                        .leash
                        .is_some_and(|l| l.holder == LeashHolder::Entity(player))
                {
                    let _ = w;
                    e.mob.leash = Some(Leash::to(LeashHolder::Knot(block)));
                    attached.push(id);
                }
            }
        }
        {
            let mut boats = self.sim().boats.lock().unwrap();
            for (&id, b) in boats.iter_mut() {
                let c = crate::leash::center(b.pos, crate::play::boats::HEIGHT);
                let near = (c[0] - center[0]).powi(2)
                    + (c[1] - center[1]).powi(2)
                    + (c[2] - center[2]).powi(2)
                    <= 16.0 * 16.0;
                if near
                    && b.leash
                        .is_some_and(|l| l.holder == LeashHolder::Entity(player))
                {
                    b.leash = Some(Leash::to(LeashHolder::Knot(block)));
                    attached.push(id);
                }
            }
        }
        if attached.is_empty() {
            if created {
                self.remove_knot(knot);
            }
            return false;
        }
        self.play_sound(
            None,
            "minecraft:entity.lead.tied",
            crate::sounds::NEUTRAL,
            center,
            1.0,
            1.0,
        );
        for id in attached {
            self.hub.send_about(
                id,
                play::set_entity_link(self.ids.set_entity_link, id, knot),
            );
        }
        let _ = player_id;
        true
    }

    /// `Leashable.tickLeash` для мобов и лодок.
    pub(super) fn tick_leashes(&self) {
        let simulated = self.simulated_set();
        let awake = |p: [f64; 3]| {
            simulated.is_empty()
                || simulated.contains(&((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4))
        };
        // Держатели-сущности: игроки, мобы, лодки.
        let mut holders: FastMap<Uuid, ([f64; 3], f64)> = self
            .hub
            .players
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.dimension == self.active_dim())
            .map(|p| (p.entry.uuid, (crate::leash::center(p.pos, 1.8), 0.6)))
            .collect();
        let mut leases: Vec<(i32, bool, Leash)> = Vec::new();
        {
            let mobs = self.sim().mobs.lock().unwrap();
            for (&id, e) in mobs.iter() {
                let (w, h) = e.mob.size();
                holders.insert(
                    Uuid::from_u128(e.mob.uuid),
                    (crate::leash::center(e.mob.pos, h), w),
                );
                if let Some(l) = e.mob.leash {
                    leases.push((id, true, l));
                }
            }
        }
        {
            let boats = self.sim().boats.lock().unwrap();
            for (&id, b) in boats.iter() {
                holders.insert(
                    b.uuid,
                    (crate::leash::center(b.pos, boats::HEIGHT), boats::WIDTH),
                );
                if let Some(l) = b.leash {
                    leases.push((id, false, l));
                }
            }
        }
        // Недостающие узлы создаём заранее и запоминаем блоки.
        let mut knot_ids: FastMap<[i32; 3], i32> = FastMap::default();
        let mut links: Vec<(i32, i32)> = Vec::new();
        let mut used: FastSet<[i32; 3]> = FastSet::default();
        for &(id, _, l) in &leases {
            if let LeashHolder::Knot(block) = l.holder {
                used.insert(block);
                let (knot, created) = self.ensure_knot(block);
                knot_ids.insert(block, knot);
                if created {
                    links.push((id, knot));
                }
            }
        }
        let mut drops = Vec::new();
        let mut breaks: Vec<[f64; 3]> = Vec::new();
        // Мобы.
        {
            let mut mobs = self.sim().mobs.lock().unwrap();
            for &(id, is_mob, mut l) in &leases {
                if !is_mob {
                    continue;
                }
                let Some(e) = mobs.get_mut(&id) else { continue };
                if e.mob.leash != Some(l) || !awake(e.mob.pos) {
                    continue;
                }
                let holder = match l.holder {
                    LeashHolder::Entity(u) => holders.get(&u).copied(),
                    LeashHolder::Knot(block) => knot_ids.get(&block).map(|_| {
                        used.insert(block);
                        (knot_pos(block), 0.5)
                    }),
                };
                let Some((hc, hw)) = holder else {
                    l.lost += 1;
                    if l.lost >= leash::LOST_TICKS {
                        e.mob.leash = None;
                        links.push((id, 0));
                        drops.push(e.mob.pos);
                    } else {
                        e.mob.leash = Some(l);
                    }
                    continue;
                };
                let (w, h) = e.mob.size();
                let ec = crate::leash::center(e.mob.pos, h);
                let dist = dist(ec, hc);
                if dist > leash::TOO_FAR {
                    e.mob.leash = None;
                    links.push((id, 0));
                    drops.push(e.mob.pos);
                    breaks.push(hc);
                } else {
                    e.mob.leash = Some(l);
                    if dist > leash::ELASTIC - hw - w
                        && let Some(p) = leash::pull(ec, hc)
                    {
                        e.mob.vel[0] += p[0];
                        e.mob.vel[1] += p[1];
                        e.mob.vel[2] += p[2];
                    }
                }
            }
        }
        // Лодки.
        {
            let mut boats = self.sim().boats.lock().unwrap();
            for &(id, is_mob, mut l) in &leases {
                if is_mob {
                    continue;
                }
                let Some(b) = boats.get_mut(&id) else {
                    continue;
                };
                if b.leash != Some(l) || !awake(b.pos) {
                    continue;
                }
                let holder = match l.holder {
                    LeashHolder::Entity(u) => holders.get(&u).copied(),
                    LeashHolder::Knot(block) => knot_ids.get(&block).map(|_| {
                        used.insert(block);
                        (knot_pos(block), 0.5)
                    }),
                };
                let Some((hc, hw)) = holder else {
                    l.lost += 1;
                    if l.lost >= leash::LOST_TICKS {
                        b.leash = None;
                        links.push((id, 0));
                        drops.push(b.pos);
                    } else {
                        b.leash = Some(l);
                    }
                    continue;
                };
                let ec = crate::leash::center(b.pos, boats::HEIGHT);
                let dist = dist(ec, hc);
                if dist > leash::TOO_FAR {
                    b.leash = None;
                    links.push((id, 0));
                    drops.push(b.pos);
                    breaks.push(hc);
                } else {
                    b.leash = Some(l);
                    if dist > leash::ELASTIC - hw - boats::WIDTH
                        && let Some(p) = leash::pull(ec, hc)
                    {
                        b.vel[0] += p[0];
                        b.vel[1] += p[1];
                        b.vel[2] += p[2];
                    }
                }
            }
        }
        for (id, holder) in links {
            self.hub.send_about(
                id,
                play::set_entity_link(self.ids.set_entity_link, id, holder),
            );
        }
        for pos in drops {
            self.drop_lead(pos);
        }
        for pos in breaks {
            self.play_sound(
                None,
                "minecraft:entity.lead.break",
                crate::sounds::NEUTRAL,
                pos,
                1.0,
                1.0,
            );
        }
        // Узлы без привязей убираем (`LeashFenceKnotEntity.notifyLeasheeRemoved`).
        let unused: Vec<i32> = self
            .sim()
            .knots
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, k)| !used.contains(&k.block))
            .map(|(&id, _)| id)
            .collect();
        for id in unused {
            self.remove_knot(id);
        }
    }
}

/// Позиция узла (`OFFSET_Y` 0.375).
fn knot_pos(block: [i32; 3]) -> [f64; 3] {
    [
        block[0] as f64 + 0.5,
        block[1] as f64 + 0.375,
        block[2] as f64 + 0.5,
    ]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
