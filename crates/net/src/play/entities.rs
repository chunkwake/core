//! Немобовые сущности для Bukkit API (`World.spawnEntity`, `getEntities`).
//!
//! Плагины видят только те виды, которые уже симулирует Rust: стрелы,
//! трезубцы, ТНТ, сферы опыта, облака зелья, глаза Края, лодки/плоты и
//! вагонетки. Запросы идут через `PlayPlan::world_request` с префиксом
//! `entity `; формат общий для всех видов:
//!
//! - `entity spawn <kind> <x> <y> <z> <yaw>` → `<id> <uuid>`;
//! - `entity list <after-id>` → строки `<id> <kind> <uuid> <x> <y> <z> <yaw> <pitch>`;
//! - `entity get <id>` → `<kind> <uuid> <x> <y> <z> <yaw> <pitch>` или пусто;
//! - `entity remove <id>` → `1`/`0` (через [`PlayPlan::remove_entity`]).
//!
//! `yaw` при спавне принимается для единообразия с `mob spawn`, но хелперы
//! `summon_*` создают сущность без поворота; `entity get` отдаёт фактический
//! поворот (у стрел и глаз — из скорости). UUID выводится из ID
//! детерминированно, как в селекторах команд: у лодок и вагонеток он не
//! совпадает с UUID, разосланным клиентам при спавне.

use super::*;

/// Стабильный UUID сущности плагина: тот же вывод, что `entity_uuid` в
/// `commands/access.rs` и `execute.rs`.
fn plugin_uuid(id: i32) -> Uuid {
    Uuid::from_u128(u128::from(id as u32).rotate_left(32))
}

/// Немобовая сущность в виде, который получает Java.
struct PluginEntity {
    id: i32,
    kind: String,
    pos: [f64; 3],
    yaw: f32,
    pitch: f32,
}

impl PluginEntity {
    /// Строка `entity list`/`entity get`.
    fn line(&self) -> String {
        let [x, y, z] = self.pos;
        format!(
            "{} {} {} {x} {y} {z} {} {}\n",
            self.id,
            self.kind,
            plugin_uuid(self.id),
            self.yaw,
            self.pitch
        )
    }
}

impl PlayPlan {
    /// Запросы плагинов к немобовым сущностям: `spawn`, `list`, `get`, `remove`.
    pub(super) fn entity_request(&self, line: &str) -> String {
        let f: Vec<&str> = line.split(' ').collect();
        let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
        match f.first().copied() {
            Some("spawn") => {
                let (Some(kind), Some(x), Some(y), Some(z)) =
                    (f.get(1).copied(), num(2), num(3), num(4))
                else {
                    return String::new();
                };
                // Хелперы уже рассылают пакеты появления и индексируют сущность
                // в своём измерении; `yaw` пока не применяется (см. модуль).
                let _yaw = num(5).unwrap_or(0.0);
                let id = match kind {
                    "minecraft:arrow" => self.summon_arrow([x, y, z], [0.0; 3]),
                    "minecraft:trident" => self.summon_trident([x, y, z], None),
                    "minecraft:tnt" => self.summon_tnt([x, y, z], crate::tnt::FUSE),
                    "minecraft:experience_orb" => self.summon_orb([x, y, z], 1),
                    "minecraft:area_effect_cloud" => self.summon_cloud([x, y, z], None),
                    "minecraft:eye_of_ender" => self.summon_eye([x, y, z], [0.0; 3], None),
                    _ if kind.ends_with("_boat") || kind.ends_with("_raft") => {
                        self.summon_boat(kind, [x, y, z])
                    }
                    _ => self.summon_minecart(kind, [x, y, z], None),
                };
                id.map_or_else(String::new, |id| format!("{id} {}", plugin_uuid(id)))
            }
            Some("spawnarrow") => {
                let (Some(kind), Some(x), Some(y), Some(z), Some(vx), Some(vy), Some(vz)) = (
                    f.get(1).copied(),
                    num(2),
                    num(3),
                    num(4),
                    num(5),
                    num(6),
                    num(7),
                ) else {
                    return String::new();
                };
                let id = match kind {
                    "minecraft:trident" => self.summon_trident_vel([x, y, z], [vx, vy, vz]),
                    "minecraft:arrow" => self.summon_arrow([x, y, z], [vx, vy, vz]),
                    _ => None,
                };
                id.map_or_else(String::new, |id| format!("{id} {}", plugin_uuid(id)))
            }
            // `list после`: сущности с ID больше `после` по возрастанию, кусок до 7 КБ.
            Some("velocity") => {
                let Some(id) = f.get(1).and_then(|v| v.parse::<i32>().ok()) else {
                    return String::new();
                };
                self.plugin_velocity(id)
                    .map_or_else(String::new, |v| format!("{} {} {}", v[0], v[1], v[2]))
            }
            Some("setvelocity") => {
                let (Some(id), Some(vx), Some(vy), Some(vz)) = (
                    f.get(1).and_then(|v| v.parse::<i32>().ok()),
                    num(2),
                    num(3),
                    num(4),
                ) else {
                    return "0".into();
                };
                u8::from(self.set_plugin_velocity(id, [vx, vy, vz])).to_string()
            }
            Some("list") => {
                let after = f
                    .get(1)
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(i32::MIN);
                let mut entities = self.plugin_entities();
                entities.sort_unstable_by_key(|e| e.id);
                let mut out = String::new();
                for e in entities.into_iter().filter(|e| e.id > after) {
                    let line = e.line();
                    if out.len() + line.len() > 7000 {
                        break;
                    }
                    out.push_str(&line);
                }
                out
            }
            Some("get") => {
                let Some(id) = f.get(1).and_then(|v| v.parse::<i32>().ok()) else {
                    return String::new();
                };
                self.plugin_entity(id)
                    .map_or_else(String::new, |e| e.line().trim_end().to_owned())
            }
            Some("remove") => {
                let Some(id) = f.get(1).and_then(|v| v.parse::<i32>().ok()) else {
                    return "0".into();
                };
                u8::from(self.remove_entity(id)).to_string()
            }
            _ => String::new(),
        }
    }

    /// Все поддерживаемые немобовые сущности, по одной карте за раз.
    fn plugin_entities(&self) -> Vec<PluginEntity> {
        let mut out = Vec::new();
        {
            let arrows = self.sim().arrows.lock().unwrap();
            for (&id, arrow) in arrows.iter() {
                let kind = arrow.entity_name();
                if !matches!(kind, "minecraft:arrow" | "minecraft:trident") {
                    continue;
                }
                let (yaw, pitch) = crate::arrows::rotation(arrow.vel);
                out.push(PluginEntity {
                    id,
                    kind: kind.to_owned(),
                    pos: arrow.pos,
                    yaw,
                    pitch,
                });
            }
        }
        {
            let boats = self.sim().boats.lock().unwrap();
            for (&id, boat) in boats.iter() {
                out.push(PluginEntity {
                    id,
                    kind: boat.kind.clone(),
                    pos: boat.pos,
                    yaw: boat.yaw,
                    pitch: 0.0,
                });
            }
        }
        {
            let carts = self.sim().minecarts.lock().unwrap();
            for (&id, entry) in carts.iter() {
                out.push(PluginEntity {
                    id,
                    kind: entry.variant_name().to_owned(),
                    pos: entry.cart.pos,
                    yaw: entry.cart.yaw,
                    pitch: 0.0,
                });
            }
        }
        {
            let tnt = self.sim().tnt.lock().unwrap();
            for (&id, primed) in tnt.iter() {
                out.push(PluginEntity {
                    id,
                    kind: "minecraft:tnt".into(),
                    pos: primed.pos,
                    yaw: 0.0,
                    pitch: 0.0,
                });
            }
        }
        {
            let orbs = self.sim().orbs.lock().unwrap();
            for (&id, orb) in orbs.iter() {
                out.push(PluginEntity {
                    id,
                    kind: "minecraft:experience_orb".into(),
                    pos: orb.pos,
                    yaw: 0.0,
                    pitch: 0.0,
                });
            }
        }
        {
            let clouds = self.sim().clouds.lock().unwrap();
            for (&id, cloud) in clouds.iter() {
                out.push(PluginEntity {
                    id,
                    kind: "minecraft:area_effect_cloud".into(),
                    pos: cloud.pos,
                    yaw: 0.0,
                    pitch: 0.0,
                });
            }
        }
        {
            let eyes = self.sim().eyes.lock().unwrap();
            for (&id, eye) in eyes.iter() {
                let (yaw, pitch) = crate::arrows::rotation(eye.vel);
                out.push(PluginEntity {
                    id,
                    kind: "minecraft:eye_of_ender".into(),
                    pos: eye.pos,
                    yaw,
                    pitch,
                });
            }
        }
        out
    }

    /// Одна поддерживаемая сущность по ID: без сборки всего списка.
    /// Скорость поддерживаемой сущности (`Entity.getVelocity`).
    fn plugin_velocity(&self, id: i32) -> Option<[f64; 3]> {
        {
            let arrows = self.sim().arrows.lock().unwrap();
            if let Some(a) = arrows.get(&id)
                && matches!(a.entity_name(), "minecraft:arrow" | "minecraft:trident")
            {
                return Some(a.vel);
            }
        }
        if let Some(b) = self.sim().boats.lock().unwrap().get(&id) {
            return Some(b.vel);
        }
        if let Some(c) = self.sim().minecarts.lock().unwrap().get(&id) {
            return Some(c.cart.vel);
        }
        if let Some(t) = self.sim().tnt.lock().unwrap().get(&id) {
            return Some(t.vel);
        }
        if let Some(o) = self.sim().orbs.lock().unwrap().get(&id) {
            return Some(o.vel);
        }
        if let Some(e) = self.sim().eyes.lock().unwrap().get(&id) {
            return Some(e.vel);
        }
        if let Some(m) = self.sim().mobs.lock().unwrap().get(&id) {
            return Some(m.mob.vel);
        }
        None
    }

    /// Задать скорость и разослать `set_entity_motion`.
    fn set_plugin_velocity(&self, id: i32, vel: [f64; 3]) -> bool {
        let mut found = false;
        {
            let mut arrows = self.sim().arrows.lock().unwrap();
            if let Some(a) = arrows.get_mut(&id)
                && matches!(a.entity_name(), "minecraft:arrow" | "minecraft:trident")
            {
                a.vel = vel;
                found = true;
            }
        }
        if !found && let Some(b) = self.sim().boats.lock().unwrap().get_mut(&id) {
            b.vel = vel;
            found = true;
        }
        if !found && let Some(c) = self.sim().minecarts.lock().unwrap().get_mut(&id) {
            c.cart.vel = vel;
            found = true;
        }
        if !found && let Some(t) = self.sim().tnt.lock().unwrap().get_mut(&id) {
            t.vel = vel;
            found = true;
        }
        if !found && let Some(o) = self.sim().orbs.lock().unwrap().get_mut(&id) {
            o.vel = vel;
            found = true;
        }
        if !found && let Some(e) = self.sim().eyes.lock().unwrap().get_mut(&id) {
            e.vel = vel;
            found = true;
        }
        if !found && let Some(m) = self.sim().mobs.lock().unwrap().get_mut(&id) {
            m.mob.vel = vel;
            found = true;
        }
        if found {
            self.hub.send(
                None,
                play::set_entity_motion(self.ids.set_entity_motion, id, vel),
            );
            self.mark_work();
        }
        found
    }

    fn plugin_entity(&self, id: i32) -> Option<PluginEntity> {
        if let Some(arrow) = self.sim().arrows.lock().unwrap().get(&id) {
            let kind = arrow.entity_name();
            if !matches!(kind, "minecraft:arrow" | "minecraft:trident") {
                return None;
            }
            let (yaw, pitch) = crate::arrows::rotation(arrow.vel);
            return Some(PluginEntity {
                id,
                kind: kind.to_owned(),
                pos: arrow.pos,
                yaw,
                pitch,
            });
        }
        if let Some(boat) = self.sim().boats.lock().unwrap().get(&id) {
            return Some(PluginEntity {
                id,
                kind: boat.kind.clone(),
                pos: boat.pos,
                yaw: boat.yaw,
                pitch: 0.0,
            });
        }
        if let Some(entry) = self.sim().minecarts.lock().unwrap().get(&id) {
            return Some(PluginEntity {
                id,
                kind: entry.variant_name().to_owned(),
                pos: entry.cart.pos,
                yaw: entry.cart.yaw,
                pitch: 0.0,
            });
        }
        if let Some(primed) = self.sim().tnt.lock().unwrap().get(&id) {
            return Some(PluginEntity {
                id,
                kind: "minecraft:tnt".into(),
                pos: primed.pos,
                yaw: 0.0,
                pitch: 0.0,
            });
        }
        if let Some(orb) = self.sim().orbs.lock().unwrap().get(&id) {
            return Some(PluginEntity {
                id,
                kind: "minecraft:experience_orb".into(),
                pos: orb.pos,
                yaw: 0.0,
                pitch: 0.0,
            });
        }
        if let Some(cloud) = self.sim().clouds.lock().unwrap().get(&id) {
            return Some(PluginEntity {
                id,
                kind: "minecraft:area_effect_cloud".into(),
                pos: cloud.pos,
                yaw: 0.0,
                pitch: 0.0,
            });
        }
        if let Some(eye) = self.sim().eyes.lock().unwrap().get(&id) {
            let (yaw, pitch) = crate::arrows::rotation(eye.vel);
            return Some(PluginEntity {
                id,
                kind: "minecraft:eye_of_ender".into(),
                pos: eye.pos,
                yaw,
                pitch,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_uuid_is_stable_and_unique_by_id() {
        assert_eq!(plugin_uuid(1), plugin_uuid(1));
        assert_ne!(plugin_uuid(1), plugin_uuid(2));
        assert_eq!(
            plugin_uuid(1).to_string(),
            "00000000-0000-0000-0000-000100000000"
        );
    }
}
