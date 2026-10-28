//! Мир: плоский overworld по `level.dat` / `server.properties` и параметры входа игрока.

mod level;

use crate::config::Properties;
use crate::storage::WorldStorage;
use chunkwake_data::packets::PacketTable;
use chunkwake_data::registries::RegistryData;
use chunkwake_data::world::{Blocks, DimensionType};
use chunkwake_net::play::{PlayPlan, SurvivalData, WorldSpec};
use chunkwake_net::survival::Difficulty;
use chunkwake_protocol::play::{Login, SpawnInfo};
use chunkwake_world::{ChunkColumn, bits_for};
use std::path::Path;

const VIEW_DISTANCE: i32 = 8;
const DIMENSION: &str = "overworld";

pub fn build_play_plan(
    data_dir: &Path,
    packets: &PacketTable,
    registries: &RegistryData,
    props: &Properties,
    world_dir: &Path,
) -> Result<(PlayPlan, WorldStorage), String> {
    let s = |e: chunkwake_data::DataError| e.to_string();
    let t = std::time::Instant::now();
    let blocks = Blocks::load(data_dir).map_err(s)?;
    tracing::debug!("blocks: {:?}", t.elapsed());
    let dim = DimensionType::load(data_dir, DIMENSION).map_err(s)?;
    let data_version = crate::config::read_world_version(data_dir)?;
    let level = level::load_or_create(
        world_dir,
        data_dir,
        &props.level_seed,
        &props.level_type,
        &props.generator_settings,
        data_version,
    )?;
    tracing::info!(
        "world: seed {}, flat {}",
        level.seed,
        level
            .flat
            .layers
            .iter()
            .map(|(b, h)| format!("{h}×{}", b.trim_start_matches("minecraft:")))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let biome = registries
        .network_id("worldgen/biome", &level.flat.biome)
        .or_else(|_| registries.network_id("worldgen/biome", "minecraft:plains"))
        .map_err(s)?;
    let biomes: Vec<String> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| r.entries.clone())
        .unwrap_or_default();
    let biome_count = biomes.len().max(1);

    let mut states = Vec::new();
    for (name, n) in &level.flat.layers {
        states.push((blocks.default_state(name).map_err(s)? as u32, *n));
    }
    let top = level.flat.layers.iter().map(|l| l.1).sum::<usize>() as f64;
    let light = std::sync::Arc::new(chunkwake_world::light::LightTable::new(
        chunkwake_data::world::load_light(data_dir).map_err(s)?,
    ));
    let mut chunk = ChunkColumn::flat(
        &states,
        dim.sections(),
        biome as u32,
        bits_for(blocks.state_count()),
        bits_for(biome_count),
        dim.has_skylight,
    );
    chunkwake_world::light::relight_column(&mut chunk, &light);
    tracing::debug!("light and template chunk: {:?}", t.elapsed());

    let dimension = format!("minecraft:{DIMENSION}");
    let login = Login {
        entity_id: 0,
        hardcore: false,
        dimensions: vec![dimension.clone()],
        max_players: props.max_players as i32,
        view_distance: VIEW_DISTANCE,
        simulation_distance: VIEW_DISTANCE,
        reduced_debug_info: false,
        show_death_screen: true,
        limited_crafting: false,
        spawn: SpawnInfo {
            dimension_type: registries
                .network_id("dimension_type", &dimension)
                .map_err(s)?,
            dimension,
            hashed_seed: level::hashed_seed(level.seed),
            game_mode: props.game_mode,
            previous_game_mode: None,
            is_debug: false,
            is_flat: true,
            portal_cooldown: 0,
            sea_level: 63,
        },
        online_mode: props.online_mode,
        enforces_secure_chat: false,
    };
    // Центр блока над верхним слоем; пустой мир — у уровня моря.
    let spawn = if level.flat.layers.iter().any(|(b, _)| b != "minecraft:air") {
        [8.5, dim.min_y as f64 + top, 8.5]
    } else {
        [8.5, 64.0, 8.5]
    };
    let player_type =
        chunkwake_data::registries::builtin_id(data_dir, "entity_type", "minecraft:player")
            .map_err(s)?;
    // Предмет ставит блок с тем же именем (stone → stone), кроме пыли, семян и ягод.
    let items = chunkwake_data::registries::builtin_entries(data_dir, "item").map_err(s)?;
    let mut item_blocks = vec![None; items.values().max().map_or(0, |&m| m as usize + 1)];
    for (name, id) in &items {
        if name != "minecraft:air" {
            let block = block_name_for_item(name);
            item_blocks[*id as usize] = blocks.default_state(block).ok().map(|s| s as u32);
        }
    }
    let world = WorldSpec {
        template: chunk.clone(),
        light: light.clone(),
        min_y: dim.min_y,
        item_blocks,
        spawn,
        player_type,
        block_names: std::sync::Arc::new(
            (0..blocks.state_count() as u32)
                .map(|id| {
                    blocks
                        .state(id)
                        .map_or_else(String::new, |(n, _)| n.to_owned())
                })
                .collect(),
        ),
        parsers: {
            let parser = |name| {
                chunkwake_data::registries::builtin_id(data_dir, "command_argument_type", name)
                    .map_err(s)
            };
            chunkwake_protocol::text::CommandParsers {
                string: parser("brigadier:string")?,
                integer: parser("brigadier:integer")?,
                item_stack: parser("minecraft:item_stack")?,
                resource: parser("minecraft:resource")?,
            }
        },
        survival: survival_data(data_dir, &blocks, &items, registries, props.difficulty)?,
    };
    let plan = PlayPlan::build(packets, login, world, props.compression_threshold).map_err(s)?;
    let storage = WorldStorage::new(
        world_dir,
        blocks,
        biomes,
        data_version,
        dim.min_y,
        chunk,
        light,
        items,
        chunkwake_data::registries::builtin_entries(data_dir, "mob_effect").map_err(s)?,
    );
    Ok((plan, storage))
}

/// Твёрдость и дроп блоков, размеры стопок, типы урона.
fn survival_data(
    data_dir: &Path,
    blocks: &Blocks,
    items: &std::collections::HashMap<String, i32>,
    registries: &RegistryData,
    difficulty: Difficulty,
) -> Result<SurvivalData, String> {
    let s = |e: chunkwake_data::DataError| e.to_string();
    let t = std::time::Instant::now();
    let loot = chunkwake_data::world::load_block_loot_cached(data_dir).map_err(s)?;
    tracing::debug!("loot tables: {:?}", t.elapsed());
    let damage =
        chunkwake_data::registries::builtin_id(data_dir, "data_component_type", "minecraft:damage")
            .map_err(s)?;
    chunkwake_protocol::inventory::DAMAGE_COMPONENT
        .store(damage, std::sync::atomic::Ordering::Relaxed);
    let enchantments;
    let enchantment_names;
    // Чары, имя и описание предметов: номера компонентов и сетевые ID чар.
    {
        use chunkwake_protocol::components as c;
        let component = |name| {
            chunkwake_data::registries::builtin_id(data_dir, "data_component_type", name).map_err(s)
        };
        let order = std::sync::atomic::Ordering::Relaxed;
        c::ENCHANTMENTS_COMPONENT.store(component("minecraft:enchantments")?, order);
        c::CUSTOM_NAME_COMPONENT.store(component("minecraft:custom_name")?, order);
        c::LORE_COMPONENT.store(component("minecraft:lore")?, order);
        c::STORED_ENCHANTMENTS_COMPONENT.store(component("minecraft:stored_enchantments")?, order);
        c::CUSTOM_DATA_COMPONENT.store(component("minecraft:custom_data")?, order);
        c::CHARGED_PROJECTILES_COMPONENT.store(component("minecraft:charged_projectiles")?, order);
        let names: Vec<String> = registries
            .synchronized
            .iter()
            .find(|r| r.name == "enchantment")
            .map(|r| r.entries.clone())
            .unwrap_or_default();
        enchantments = enchantment_lines(data_dir, &names);
        enchantment_names = names.clone();
        c::set_enchantments(names);
    }
    let _ = chunkwake_world::BLOCK_ENTITY_TYPES
        .set(chunkwake_data::registries::builtin_ids(data_dir, "block_entity_type").map_err(s)?);
    // Сундуки: свойства `facing`/`type`/`waterlogged` каждого состояния.
    let chests: std::collections::HashMap<u32, chunkwake_net::play::ChestState> =
        (0..blocks.state_count() as u32)
            .filter_map(|id| {
                let (name, props) = blocks.state(id)?;
                let trapped = match name {
                    "minecraft:chest" => false,
                    "minecraft:trapped_chest" => true,
                    _ => return None,
                };
                let get = |k: &str| props.iter().find(|(p, _)| p == k).map(|(_, v)| v.as_str());
                let facing = ["north", "east", "south", "west"]
                    .iter()
                    .position(|f| Some(*f) == get("facing"))? as u8;
                let kind = ["single", "left", "right"]
                    .iter()
                    .position(|t| Some(*t) == get("type"))? as u8;
                Some((
                    id,
                    chunkwake_net::play::ChestState {
                        trapped,
                        facing,
                        kind,
                        waterlogged: get("waterlogged") == Some("true"),
                    },
                ))
            })
            .collect();
    // Печи: состояние → то же с другим `lit`.
    let mut by_props: std::collections::HashMap<(String, Vec<(String, String)>), u32> =
        Default::default();
    for id in 0..blocks.state_count() as u32 {
        if let Some((name, props)) = blocks.state(id)
            && chunkwake_data::world::FURNACES.contains(&name)
        {
            by_props.insert((name.to_owned(), props.to_vec()), id);
        }
    }
    let furnace_lit = by_props
        .iter()
        .filter_map(|((name, props), &id)| {
            let flipped: Vec<(String, String)> = props
                .iter()
                .map(|(k, v)| {
                    let v = if k == "lit" {
                        if v == "true" { "false" } else { "true" }
                    } else {
                        v
                    };
                    (k.clone(), v.to_owned())
                })
                .collect();
            Some((id, *by_props.get(&(name.clone(), flipped))?))
        })
        .collect();
    let recipes = chunkwake_data::recipes::load_recipes(data_dir, items).map_err(s)?;
    tracing::debug!("recipes: {}", recipes.len());
    let menu = |name| chunkwake_data::registries::builtin_id(data_dir, "menu", name).map_err(s);
    let enchanting = chunkwake_data::enchanting::load(data_dir, &enchantment_names).map_err(s)?;
    tracing::info!(
        "enchanting: {} enchantments ({} on the table), {} enchantable items",
        enchanting.defs.len(),
        enchanting.defs.iter().filter(|d| d.in_table).count(),
        enchanting.enchantable.len()
    );
    Ok(SurvivalData {
        enchantments,
        recipes,
        menu_chest: menu("minecraft:generic_9x3")?,
        menu_double_chest: menu("minecraft:generic_9x6")?,
        menu_hopper: menu("minecraft:hopper")?,
        menu_3x3: menu("minecraft:generic_3x3")?,
        menu_rows: [
            menu("minecraft:generic_9x1")?,
            menu("minecraft:generic_9x2")?,
            menu("minecraft:generic_9x3")?,
            menu("minecraft:generic_9x4")?,
            menu("minecraft:generic_9x5")?,
            menu("minecraft:generic_9x6")?,
        ],
        chests: chests.clone(),
        chest_states: chests.iter().map(|(&id, &c)| (c, id)).collect(),
        menu_crafting: menu("minecraft:crafting")?,
        menu_enchantment: menu("minecraft:enchantment")?,
        menu_grindstone: menu("minecraft:grindstone")?,
        enchanting,
        menu_furnace: [
            menu("minecraft:furnace")?,
            menu("minecraft:blast_furnace")?,
            menu("minecraft:smoker")?,
        ],
        furnace_lit,
        effects: chunkwake_data::registries::builtin_entries(data_dir, "mob_effect").map_err(s)?,
        attributes: chunkwake_data::registries::builtin_entries(data_dir, "attribute")
            .map_err(s)?,
        sounds: chunkwake_data::registries::builtin_entries(data_dir, "sound_event").map_err(s)?,
        attribute_speed: chunkwake_data::registries::builtin_id(
            data_dir,
            "attribute",
            "minecraft:movement_speed",
        )
        .map_err(s)?,
        attribute_jump: chunkwake_data::registries::builtin_id(
            data_dir,
            "attribute",
            "minecraft:jump_strength",
        )
        .map_err(s)?,
        attribute_gravity: chunkwake_data::registries::builtin_id(
            data_dir,
            "attribute",
            "minecraft:gravity",
        )
        .map_err(s)?,
        placement: chunkwake_net::placement::Placement::new(
            (0..blocks.state_count() as u32)
                .map(|id| blocks.state(id).map(|(n, p)| (n.to_owned(), p.to_vec())))
                .collect(),
            chunkwake_data::world::load_sturdy(data_dir).map_err(s)?,
            chunkwake_data::world::read_json(
                &data_dir.join("data/minecraft/tags/block/walls.json"),
            )
            .ok()
            .and_then(|j| j["values"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        ),
        hardness: chunkwake_data::world::load_hardness(data_dir).map_err(s)?,
        requires_tool: chunkwake_data::world::load_requires_tool(data_dir).map_err(s)?,
        tools: {
            let t = std::time::Instant::now();
            let v = chunkwake_data::tools::load_tools_cached(data_dir, items).map_err(s)?;
            tracing::debug!("tools: {:?}", t.elapsed());
            v
        },
        block_names: (0..blocks.state_count() as u32)
            .map(|id| {
                blocks
                    .state(id)
                    .map_or(String::new(), |(n, _)| n.to_owned())
            })
            .collect(),
        block_types: chunkwake_data::registries::builtin_ids(data_dir, "block").map_err(s)?,
        block_defaults: (0..blocks.state_count() as u32)
            .filter_map(|id| blocks.state(id).map(|(n, _)| n.to_owned()))
            .filter_map(|n| blocks.default_state(&n).ok().map(|s| (n, s as u32)))
            .collect(),
        loot,
        items: {
            let t = std::time::Instant::now();
            let v = chunkwake_data::world::load_items_cached(data_dir, items).map_err(s)?;
            tracing::debug!("item components: {:?}", t.elapsed());
            v
        },
        damage_fall: registries
            .network_id("damage_type", "minecraft:fall")
            .map_err(s)?,
        damage_void: registries
            .network_id("damage_type", "minecraft:out_of_world")
            .map_err(s)?,
        damage_starve: registries
            .network_id("damage_type", "minecraft:starve")
            .map_err(s)?,
        damage_magic: registries
            .network_id("damage_type", "minecraft:magic")
            .map_err(s)?,
        damage_arrow: registries
            .network_id("damage_type", "minecraft:arrow")
            .map_err(s)?,
        snowball_entity_type: chunkwake_data::registries::builtin_id(
            data_dir,
            "entity_type",
            "minecraft:snowball",
        )
        .map_err(s)?,
        egg_entity_type: chunkwake_data::registries::builtin_id(
            data_dir,
            "entity_type",
            "minecraft:egg",
        )
        .map_err(s)?,
        arrow_entity_type: chunkwake_data::registries::builtin_id(
            data_dir,
            "entity_type",
            "minecraft:arrow",
        )
        .map_err(s)?,
        firework_entity_type: chunkwake_data::registries::builtin_id(
            data_dir,
            "entity_type",
            "minecraft:firework_rocket",
        )
        .map_err(s)?,
        damage_wither: registries
            .network_id("damage_type", "minecraft:wither")
            .map_err(s)?,
        difficulty: std::sync::atomic::AtomicU8::new(difficulty.id()),
        item_ids: items.clone(),
        potions: {
            let map = chunkwake_data::registries::builtin_entries(data_dir, "potion").map_err(s)?;
            let len = map
                .values()
                .copied()
                .max()
                .map_or(0, |id| (id.max(0) as usize) + 1);
            let mut names = vec![String::new(); len];
            for (name, id) in map {
                if let Some(slot) = names.get_mut(id as usize) {
                    *slot = name;
                }
            }
            names
        },
        potion_component: chunkwake_data::registries::builtin_id(
            data_dir,
            "data_component_type",
            "minecraft:potion_contents",
        )
        .map_err(s)?,
        item_entity_type: chunkwake_data::registries::builtin_id(
            data_dir,
            "entity_type",
            "minecraft:item",
        )
        .map_err(s)?,
        orb_entity_type: chunkwake_data::registries::builtin_id(
            data_dir,
            "entity_type",
            "minecraft:experience_orb",
        )
        .map_err(s)?,
        mob_entity_types: {
            let id =
                |n| chunkwake_data::registries::builtin_id(data_dir, "entity_type", n).map_err(s);
            [
                id("minecraft:cow")?,
                id("minecraft:pig")?,
                id("minecraft:sheep")?,
                id("minecraft:chicken")?,
            ]
        },
    })
}

/// Чары для плагинов: `имя макс_уровень вес цена_наковальни мин_база мин_шаг макс_база
/// макс_шаг сокровище проклятие` (из `data/minecraft/enchantment` и тегов).
fn enchantment_lines(data_dir: &Path, names: &[String]) -> Vec<String> {
    let base = data_dir.join("data/minecraft");
    let read = |p: std::path::PathBuf| {
        std::fs::read_to_string(p)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    };
    let tag = |t: &str| -> Vec<String> {
        read(base.join(format!("tags/enchantment/{t}.json")))
            .and_then(|v| v.get("values")?.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    };
    let (treasure, curse) = (tag("treasure"), tag("curse"));
    names
        .iter()
        .map(|name| {
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let v = read(base.join(format!("enchantment/{path}.json"))).unwrap_or_default();
            let n = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(1);
            let cost = |k: &str, f: &str| {
                v.get(k)
                    .and_then(|c| c.get(f))
                    .and_then(|x| x.as_i64())
                    .unwrap_or(0)
            };
            format!(
                "{name} {} {} {} {} {} {} {} {} {}",
                n("max_level"),
                n("weight"),
                n("anvil_cost"),
                cost("min_cost", "base"),
                cost("min_cost", "per_level_above_first"),
                cost("max_cost", "base"),
                cost("max_cost", "per_level_above_first"),
                u8::from(treasure.contains(name)),
                u8::from(curse.contains(name)),
            )
        })
        .collect()
}

/// Предмет, который ставит блок с другим именем (`ItemNameBlockItem`).
fn block_name_for_item(item: &str) -> &str {
    match item {
        "minecraft:redstone" => "minecraft:redstone_wire",
        "minecraft:string" => "minecraft:tripwire",
        "minecraft:wheat_seeds" => "minecraft:wheat",
        "minecraft:carrot" => "minecraft:carrots",
        "minecraft:potato" => "minecraft:potatoes",
        "minecraft:beetroot_seeds" => "minecraft:beetroots",
        "minecraft:melon_seeds" => "minecraft:melon_stem",
        "minecraft:pumpkin_seeds" => "minecraft:pumpkin_stem",
        "minecraft:torchflower_seeds" => "minecraft:torchflower_crop",
        "minecraft:pitcher_pod" => "minecraft:pitcher_crop",
        "minecraft:cocoa_beans" => "minecraft:cocoa",
        "minecraft:sweet_berries" => "minecraft:sweet_berry_bush",
        "minecraft:glow_berries" => "minecraft:cave_vines",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::block_name_for_item;

    #[test]
    fn redstone_item_places_wire() {
        // `default_state` вызывается уже с именем блока, поэтому id пыли хранит состояние провода.
        let cases = [
            ("minecraft:redstone", "minecraft:redstone_wire"),
            ("minecraft:string", "minecraft:tripwire"),
            ("minecraft:wheat_seeds", "minecraft:wheat"),
            ("minecraft:carrot", "minecraft:carrots"),
            ("minecraft:potato", "minecraft:potatoes"),
            ("minecraft:beetroot_seeds", "minecraft:beetroots"),
            ("minecraft:melon_seeds", "minecraft:melon_stem"),
            ("minecraft:pumpkin_seeds", "minecraft:pumpkin_stem"),
            ("minecraft:torchflower_seeds", "minecraft:torchflower_crop"),
            ("minecraft:pitcher_pod", "minecraft:pitcher_crop"),
            ("minecraft:cocoa_beans", "minecraft:cocoa"),
            ("minecraft:sweet_berries", "minecraft:sweet_berry_bush"),
            ("minecraft:glow_berries", "minecraft:cave_vines"),
            ("minecraft:stone", "minecraft:stone"),
            ("minecraft:repeater", "minecraft:repeater"),
            ("minecraft:stick", "minecraft:stick"),
        ];
        for (item, block) in cases {
            assert_eq!(block_name_for_item(item), block);
        }
    }
}
