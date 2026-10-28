//! Мир: плоский overworld (bedrock, 2×dirt, grass) и параметры входа игрока.

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
    let biome = registries
        .network_id("worldgen/biome", "minecraft:plains")
        .map_err(s)?;
    let biomes: Vec<String> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| r.entries.clone())
        .unwrap_or_default();
    let biome_count = biomes.len().max(1);

    let layers = [
        ("minecraft:bedrock", 1),
        ("minecraft:dirt", 2),
        ("minecraft:grass_block", 1),
    ];
    let mut states = Vec::new();
    for (name, n) in layers {
        states.push((blocks.default_state(name).map_err(s)? as u32, n));
    }
    let top = layers.iter().map(|l| l.1).sum::<usize>() as f64;
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
            hashed_seed: 0,
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
    // Центр блока над травой.
    let spawn = [8.5, dim.min_y as f64 + top, 8.5];
    let player_type =
        chunkwake_data::registries::builtin_id(data_dir, "entity_type", "minecraft:player")
            .map_err(s)?;
    // Предмет ставит блок с тем же именем (stone → stone); остальные предметы — ничего.
    let items = chunkwake_data::registries::builtin_entries(data_dir, "item").map_err(s)?;
    let mut item_blocks = vec![None; items.values().max().map_or(0, |&m| m as usize + 1)];
    for (name, id) in &items {
        if name != "minecraft:air" {
            item_blocks[*id as usize] = blocks.default_state(name).ok().map(|s| s as u32);
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
            }
        },
        survival: survival_data(data_dir, &blocks, &items, registries, props.difficulty)?,
    };
    let plan = PlayPlan::build(packets, login, world, props.compression_threshold).map_err(s)?;
    let data_version = crate::config::read_world_version(data_dir)?;
    let storage = WorldStorage::new(
        world_dir,
        blocks,
        biomes,
        data_version,
        dim.min_y,
        chunk,
        light,
        items,
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
    let drops_by_block = chunkwake_data::world::load_block_drops_cached(data_dir).map_err(s)?;
    tracing::debug!("loot tables: {:?}", t.elapsed());
    let drops = (0..blocks.state_count() as u32)
        .map(|id| {
            let (name, _) = blocks.state(id)?;
            items.get(drops_by_block.get(name)?).copied()
        })
        .collect();
    let damage = chunkwake_data::registries::builtin_id(data_dir, "data_component_type", "minecraft:damage")
        .map_err(s)?;
    chunkwake_protocol::inventory::DAMAGE_COMPONENT.store(damage, std::sync::atomic::Ordering::Relaxed);
    let _ = chunkwake_world::BLOCK_ENTITY_TYPES.set(
        chunkwake_data::registries::builtin_ids(data_dir, "block_entity_type").map_err(s)?,
    );
    // Сундуки: свойства `facing`/`type`/`waterlogged` каждого состояния.
    let chests: std::collections::HashMap<u32, chunkwake_net::play::ChestState> = (0..blocks.state_count() as u32)
        .filter_map(|id| {
            let (name, props) = blocks.state(id)?;
            let trapped = match name {
                "minecraft:chest" => false,
                "minecraft:trapped_chest" => true,
                _ => return None,
            };
            let get = |k: &str| props.iter().find(|(p, _)| p == k).map(|(_, v)| v.as_str());
            let facing = ["north", "east", "south", "west"].iter().position(|f| Some(*f) == get("facing"))? as u8;
            let kind = ["single", "left", "right"].iter().position(|t| Some(*t) == get("type"))? as u8;
            Some((id, chunkwake_net::play::ChestState { trapped, facing, kind, waterlogged: get("waterlogged") == Some("true") }))
        })
        .collect();
    let recipes = chunkwake_data::recipes::load_recipes(data_dir, items).map_err(s)?;
    tracing::debug!("recipes: {}", recipes.len());
    let menu = |name| chunkwake_data::registries::builtin_id(data_dir, "menu", name).map_err(s);
    Ok(SurvivalData {
        recipes,
        menu_chest: menu("minecraft:generic_9x3")?,
        menu_double_chest: menu("minecraft:generic_9x6")?,
        chests: chests.clone(),
        chest_states: chests.iter().map(|(&id, &c)| (c, id)).collect(),
        menu_crafting: menu("minecraft:crafting")?,
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
        block_defaults: (0..blocks.state_count() as u32)
            .filter_map(|id| blocks.state(id).map(|(n, _)| n.to_owned()))
            .filter_map(|n| blocks.default_state(&n).ok().map(|s| (n, s as u32)))
            .collect(),
        drops,
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
        difficulty,
        item_ids: items.clone(),
        item_entity_type: chunkwake_data::registries::builtin_id(
            data_dir,
            "entity_type",
            "minecraft:item",
        )
        .map_err(s)?,
    })
}
