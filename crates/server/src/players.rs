//! Игроки на диске: `<level-name>/playerdata/<uuid>.dat` (gzip NBT, как у ванилы).

use chunkwake_net::Uuid;
use chunkwake_net::survival::{Effect, PlayerSave};
use chunkwake_protocol::inventory::Stack;
use chunkwake_world::nbt::{self, Compound, Tag};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Слоты окна игрока под броню и вторую руку → ключи `equipment` ванилы.
const EQUIPMENT: [(usize, &str); 5] = [
    (5, "head"),
    (6, "chest"),
    (7, "legs"),
    (8, "feet"),
    (45, "offhand"),
];
/// Окно: основной инвентарь 9..36, хотбар 36..45. У ванилы хотбар 0..9.
const HOTBAR: usize = 36;

pub struct PlayerFiles {
    dir: PathBuf,
    data_version: i32,
    item_names: Vec<String>,
    item_ids: HashMap<String, i32>,
    /// Реестр `mob_effect`: имя → ID.
    effects: HashMap<String, i32>,
}

impl PlayerFiles {
    pub fn new(
        world_dir: &Path,
        data_version: i32,
        item_ids: HashMap<String, i32>,
        effects: HashMap<String, i32>,
    ) -> Self {
        let mut item_names =
            vec![String::new(); item_ids.values().max().map_or(0, |&m| m as usize + 1)];
        for (name, &id) in &item_ids {
            item_names[id as usize] = name.clone();
        }
        Self {
            dir: world_dir.join("playerdata"),
            data_version,
            item_names,
            item_ids,
            effects,
        }
    }

    fn path(&self, uuid: Uuid) -> PathBuf {
        self.dir.join(format!("{}.dat", uuid.hyphenated()))
    }

    pub fn load(&self, uuid: Uuid) -> Result<Option<PlayerSave>, String> {
        let Some(root) = nbt::read_gzip_file(&self.path(uuid))? else {
            return Ok(None);
        };
        let mut s = from_nbt(&root, &self.item_ids);
        if let Some(s) = s.as_mut() {
            s.effects = effects_from(&root, &self.effects);
        }
        Ok(s)
    }

    pub fn save(&self, uuid: Uuid, s: &PlayerSave) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("create {}: {e}", self.dir.display()))?;
        let mut root = to_nbt(s, self.data_version, &self.item_names);
        let names: HashMap<i32, &String> = self.effects.iter().map(|(n, &id)| (id, n)).collect();
        let list: Vec<Tag> = s
            .effects
            .iter()
            .filter_map(|e| {
                Some(Tag::Compound(Compound::from([
                    ("id".into(), Tag::String((*names.get(&e.id)?).clone())),
                    ("amplifier".into(), Tag::Byte(e.amplifier as i8)),
                    ("duration".into(), Tag::Int(e.duration)),
                    ("ambient".into(), Tag::Byte(i8::from(e.ambient))),
                    ("show_particles".into(), Tag::Byte(i8::from(e.particles))),
                    ("show_icon".into(), Tag::Byte(i8::from(e.show_icon))),
                ])))
            })
            .collect();
        if !list.is_empty() {
            root.insert("active_effects".into(), Tag::List(list));
        }
        nbt::write_gzip_file(&self.path(uuid), &root)
    }
}

/// `active_effects` ванилы (`MobEffectInstance.CODEC`).
fn effects_from(root: &Compound, ids: &HashMap<String, i32>) -> Vec<Effect> {
    root.get("active_effects")
        .and_then(Tag::as_list)
        .unwrap_or(&[])
        .iter()
        .filter_map(|t| {
            let c = t.as_compound()?;
            let id = *ids.get(c.get("id")?.as_str()?)?;
            let int = |k: &str| c.get(k).and_then(Tag::as_i64);
            Some(Effect {
                id,
                amplifier: int("amplifier").unwrap_or(0).clamp(0, 255) as u8,
                duration: int("duration").unwrap_or(0) as i32,
                ambient: int("ambient").unwrap_or(0) != 0,
                particles: int("show_particles").unwrap_or(1) != 0,
                show_icon: int("show_icon").unwrap_or(1) != 0,
                age: 0,
            })
        })
        .filter(|e| e.duration != 0)
        .collect()
}

fn stack_nbt(s: Stack, names: &[String]) -> Option<Compound> {
    chunkwake_net::stack_nbt::to_nbt(s, names.get(s.item as usize)?)
}

fn stack_from(c: &Compound, ids: &HashMap<String, i32>) -> Option<Stack> {
    chunkwake_net::stack_nbt::from_nbt(c, |n| ids.get(n).copied())
}

pub fn to_nbt(s: &PlayerSave, data_version: i32, names: &[String]) -> Compound {
    let mut inventory = Vec::new();
    let mut equipment = Compound::new();
    for &(i, stack) in &s.slots {
        let Some(mut c) = stack_nbt(stack, names) else {
            continue;
        };
        if let Some(&(_, key)) = EQUIPMENT.iter().find(|(w, _)| *w == i) {
            equipment.insert(key.into(), Tag::Compound(c));
            continue;
        }
        let slot = match i {
            HOTBAR..45 => i - HOTBAR,
            9..HOTBAR => i,
            _ => continue,
        };
        c.insert("Slot".into(), Tag::Byte(slot as i8));
        inventory.push(Tag::Compound(c));
    }
    Compound::from([
        ("DataVersion".into(), Tag::Int(data_version)),
        (
            "Dimension".into(),
            Tag::String("minecraft:overworld".into()),
        ),
        (
            "Pos".into(),
            Tag::List(s.pos.iter().map(|&v| Tag::Double(v)).collect()),
        ),
        (
            "Rotation".into(),
            Tag::List(vec![Tag::Float(s.yaw), Tag::Float(s.pitch)]),
        ),
        ("Health".into(), Tag::Float(s.health)),
        ("foodLevel".into(), Tag::Int(s.food)),
        ("foodSaturationLevel".into(), Tag::Float(s.saturation)),
        ("foodExhaustionLevel".into(), Tag::Float(s.exhaustion)),
        ("SelectedItemSlot".into(), Tag::Int(s.selected as i32)),
        (
            "playerGameType".into(),
            Tag::Int(s.game_mode.unwrap_or(0) as i32),
        ),
        ("Inventory".into(), Tag::List(inventory)),
        ("equipment".into(), Tag::Compound(equipment)),
    ])
}

pub fn from_nbt(root: &Compound, ids: &HashMap<String, i32>) -> Option<PlayerSave> {
    let f = |k: &str| match root.get(k) {
        Some(Tag::Float(v)) => Some(*v),
        _ => None,
    };
    let pos = match root.get("Pos")?.as_list()? {
        [Tag::Double(x), Tag::Double(y), Tag::Double(z)] => [*x, *y, *z],
        _ => return None,
    };
    let (yaw, pitch) = match root.get("Rotation").and_then(Tag::as_list) {
        Some([Tag::Float(y), Tag::Float(p)]) => (*y, *p),
        _ => (0.0, 0.0),
    };
    let mut slots = Vec::new();
    for t in root.get("Inventory").and_then(Tag::as_list).unwrap_or(&[]) {
        let Some(c) = t.as_compound() else { continue };
        let Some(slot) = c.get("Slot").and_then(Tag::as_i64) else {
            continue;
        };
        let window = match slot {
            0..9 => slot as usize + HOTBAR,
            9..36 => slot as usize,
            _ => continue,
        };
        if let Some(s) = stack_from(c, ids) {
            slots.push((window, s));
        }
    }
    if let Some(eq) = root.get("equipment").and_then(Tag::as_compound) {
        for (window, key) in EQUIPMENT {
            if let Some(s) = eq
                .get(key)
                .and_then(Tag::as_compound)
                .and_then(|c| stack_from(c, ids))
            {
                slots.push((window, s));
            }
        }
    }
    slots.sort_by_key(|&(i, _)| i);
    Some(PlayerSave {
        pos,
        yaw,
        pitch,
        health: f("Health").unwrap_or(20.0),
        food: root.get("foodLevel").and_then(Tag::as_i64).unwrap_or(20) as i32,
        saturation: f("foodSaturationLevel").unwrap_or(5.0),
        exhaustion: f("foodExhaustionLevel").unwrap_or(0.0),
        selected: root
            .get("SelectedItemSlot")
            .and_then(Tag::as_i64)
            .unwrap_or(0)
            .clamp(0, 8) as usize,
        slots,
        game_mode: root
            .get("playerGameType")
            .and_then(Tag::as_i64)
            .filter(|m| (0..4).contains(m))
            .map(|m| m as u8),
        effects: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_vanilla_slots() {
        let ids = HashMap::from([
            ("minecraft:stone".to_string(), 1),
            ("minecraft:bread".to_string(), 2),
        ]);
        let names = vec![
            String::new(),
            "minecraft:stone".into(),
            "minecraft:bread".into(),
        ];
        let s = PlayerSave {
            pos: [1.5, 64.0, -3.25],
            yaw: 45.0,
            pitch: 10.0,
            health: 13.0,
            food: 17,
            saturation: 2.5,
            exhaustion: 1.25,
            selected: 3,
            game_mode: Some(1),
            effects: Vec::new(),
            slots: vec![
                (
                    5,
                    Stack {
                        item: 1,
                        count: 1,
                        damage: 0,
                        extra: 0,
                    },
                ),
                (
                    10,
                    Stack {
                        item: 1,
                        count: 64,
                        damage: 0,
                        extra: 0,
                    },
                ),
                (
                    36,
                    Stack {
                        item: 2,
                        count: 1,
                        damage: 7,
                        extra: 0,
                    },
                ),
                (
                    45,
                    Stack {
                        item: 2,
                        count: 1,
                        damage: 0,
                        extra: 0,
                    },
                ),
            ],
        };
        let root = to_nbt(&s, 4000, &names);
        // Хотбар у ванилы — слоты 0..9, вторая рука — в equipment.
        let inv = root["Inventory"].as_list().unwrap();
        let slot_of = |t: &Tag| t.as_compound().unwrap()["Slot"].as_i64().unwrap();
        assert_eq!(inv.iter().map(slot_of).collect::<Vec<_>>(), vec![10, 0]);
        assert!(
            root["equipment"]
                .as_compound()
                .unwrap()
                .contains_key("offhand")
        );
        let bytes = nbt::write_root(&root);
        let back = from_nbt(&nbt::read_root(&bytes).unwrap(), &ids).unwrap();
        assert_eq!(back, s);
    }
}
