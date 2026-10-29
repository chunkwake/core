//! Мир: плоский overworld по `level.dat` / `server.properties` и параметры входа игрока.

mod level;

use crate::config::Properties;
use crate::storage::WorldStorage;
use chunkwake_data::boot::Boot;
use chunkwake_data::registries::{Builtin, RegistryData};
use chunkwake_data::world::Blocks;
use chunkwake_net::play::{PlayPlan, SurvivalData, WorldSpec};
use chunkwake_net::survival::Difficulty;
use chunkwake_protocol::play::{Login, SpawnInfo};
use chunkwake_world::{ChunkColumn, bits_for};
use std::path::Path;
use std::sync::Arc;

const DIMENSION: &str = "overworld";

/// Части образа, которые нужны только выживанию.
struct Loaded {
    hardness: Vec<f32>,
    sturdy: Vec<u8>,
    requires_tool: Vec<bool>,
    item_info: Vec<chunkwake_data::world::ItemInfo>,
    tools: Vec<Option<chunkwake_data::tools::Tool>>,
    recipes: chunkwake_data::recipes::Recipes,
    enchanting: chunkwake_data::enchanting::Enchanting,
    enchantment_names: Vec<String>,
    enchantment_lines: Vec<String>,
}

pub fn build_play_plan(
    data_dir: &Path,
    boot: Boot,
    props: &Properties,
    world_dir: &Path,
) -> Result<(PlayPlan, WorldStorage), String> {
    let s = |e: chunkwake_data::DataError| e.to_string();
    let t = std::time::Instant::now();
    let Boot {
        world_version,
        packets,
        builtin,
        registries,
        blocks,
        dimension: dim,
        light: light_rows,
        items: item_info,
        tools,
        recipes,
        enchanting,
        enchantment_names,
        enchantment_lines,
        hardness,
        sturdy,
        requires_tool,
        ..
    } = boot;
    let data_version = world_version;
    let level = level::load_or_create(
        world_dir,
        data_dir,
        &props.level_seed,
        &props.level_type,
        &props.generator_settings,
        data_version,
    )?;
    match &level.kind {
        level::Kind::Noise => tracing::info!("world: seed {}, noise", level.seed),
        level::Kind::Flat(flat) => tracing::info!(
            "world: seed {}, flat {}",
            level.seed,
            flat.layers
                .iter()
                .map(|(b, h)| format!("{h}×{}", b.trim_start_matches("minecraft:")))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
    let biome_name = match &level.kind {
        level::Kind::Flat(flat) => flat.biome.as_str(),
        level::Kind::Noise => "minecraft:plains",
    };
    let biome = registries
        .network_id("worldgen/biome", biome_name)
        .or_else(|_| registries.network_id("worldgen/biome", "minecraft:plains"))
        .map_err(s)?;
    let biomes: Vec<String> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| r.entries.clone())
        .unwrap_or_default();
    let biome_count = biomes.len().max(1);
    // Имя → сетевой ID: климат выбирает биом по имени, сервер по своему реестру.
    let biome_ids: std::collections::HashMap<String, u32> = biomes
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i as u32))
        .collect();

    let mut states = Vec::new();
    if let level::Kind::Flat(flat) = &level.kind {
        for (name, n) in &flat.layers {
            states.push((blocks.default_state(name).map_err(s)? as u32, *n));
        }
    }
    let top = match &level.kind {
        level::Kind::Flat(flat) => flat.layers.iter().map(|l| l.1).sum::<usize>() as f64,
        level::Kind::Noise => 0.0,
    };
    let light = std::sync::Arc::new(chunkwake_world::light::LightTable::new(light_rows));
    let mut chunk = ChunkColumn::flat(
        &states,
        dim.sections(),
        biome as u32,
        bits_for(blocks.state_count()),
        bits_for(biome_count),
        dim.has_skylight,
    );
    chunkwake_world::light::relight_column(&mut chunk, &light);

    let block_bits = bits_for(blocks.state_count());
    let biome_bits = bits_for(biome_count);
    let (terrain, spawn_noise, sea_level, is_flat) = match &level.kind {
        level::Kind::Flat(_) => (None, None, 63, true),
        level::Kind::Noise => {
            let mut loaded = chunkwake_worldgen::Generator::load_with_version(
                data_dir,
                level.seed,
                Some(data_version),
            )?;
            if loaded.min_y != dim.min_y || loaded.height != dim.height {
                return Err(format!(
                    "noise settings y {} height {} do not match the overworld y {} height {}",
                    loaded.min_y, loaded.height, dim.min_y, dim.height
                ));
            }
            // Биомы выбирает климат (`MultiNoiseBiomeSource`), а не один plains.
            loaded.bind_biomes(biome_ids.clone());
            let climate = loaded.has_biomes();
            let noise = std::sync::Arc::new(loaded);
            let sea = noise.sea_level;
            let stand = stand_on_solid(&noise.fill(0, 0), noise.height, dim.min_y);
            let biome_id = biome as u32;
            let sky = dim.has_skylight;
            // Шум, поверхность и карверы — один конвейер генератора.
            let state = |name: &str| blocks.default_state(name).ok().map(|s| s as u32);
            let overworld = chunkwake_worldgen::pipeline::Overworld::new(noise, &state, &biome_ids)?;
            if !overworld.has_surface() {
                tracing::warn!("no material_rule in the data cache: the world keeps bare stone");
            }
            // Свет считает пролив при отправке: здесь только блоки.
            let built: std::sync::Arc<dyn Fn(i32, i32) -> ChunkColumn + Send + Sync> =
                std::sync::Arc::new(move |cx, cz| {
                    let chunk = overworld.chunk(cx, cz);
                    match chunk.biomes {
                        Some(biomes) => ChunkColumn::from_blocks_with_biomes(
                            &chunk.states, &biomes, block_bits, biome_bits, sky,
                        ),
                        None => ChunkColumn::from_blocks(
                            &chunk.states, biome_id, block_bits, biome_bits, sky,
                        ),
                    }
                });
            if !climate {
                tracing::warn!(
                    "reports/biomes.csv is missing: the noise world keeps a single biome. \
                     Delete the data cache or rebuild it to get real biomes"
                );
            }
            (Some(built), Some(stand), sea, false)
        }
    };

    let dimension = format!("minecraft:{DIMENSION}");
    let login = Login {
        entity_id: 0,
        hardcore: false,
        dimensions: vec![dimension.clone()],
        max_players: props.max_players as i32,
        view_distance: props.view_distance as i32,
        // За пределами прорисовки чанков нет, поэтому симуляция не больше неё.
        simulation_distance: props.simulation_distance.min(props.view_distance) as i32,
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
            is_flat,
            portal_cooldown: 0,
            sea_level,
        },
        online_mode: props.online_mode,
        enforces_secure_chat: false,
    };
    // Плоский мир: центр блока над верхним слоем, пустой — у уровня моря.
    // Нормальный: верхний твёрдый блок столбца 8,8 чанка 0,0.
    tracing::debug!("world: level and template {} ms", t.elapsed().as_millis());
    let spawn = if let Some(y) = spawn_noise {
        [8.5, y, 8.5]
    } else if let level::Kind::Flat(flat) = &level.kind
        && flat.layers.iter().any(|(b, _)| b != "minecraft:air")
    {
        [8.5, dim.min_y as f64 + top, 8.5]
    } else {
        [8.5, 64.0, 8.5]
    };
    let player_type =
        chunkwake_data::registries::builtin_id_of(&builtin, "entity_type", "minecraft:player")
            .map_err(s)?;
    // Предмет ставит блок с тем же именем (stone → stone), кроме пыли, семян и ягод.
    let items = chunkwake_data::registries::builtin_entries_of(&builtin, "item").map_err(s)?;
    let mut item_blocks = vec![None; items.values().max().map_or(0, |&m| m as usize + 1)];
    for (name, id) in &items {
        if name != "minecraft:air" {
            let block = block_name_for_item(name);
            item_blocks[*id as usize] = blocks.default_state(block).ok().map(|s| s as u32);
        }
    }
    // Имя состояния нужно и миру, и правилам инструментов — один общий список.
    let block_names = Arc::new(state_names(&blocks));
    let loaded = Loaded {
        hardness,
        sturdy,
        requires_tool,
        item_info,
        tools,
        recipes,
        enchanting,
        enchantment_names,
        enchantment_lines,
    };
    let world = WorldSpec {
        template: chunk.clone(),
        terrain,
        light: light.clone(),
        min_y: dim.min_y,
        item_blocks,
        spawn,
        player_type,
        block_names: block_names.clone(),
        parsers: {
            let parser = |name| {
                chunkwake_data::registries::builtin_id_of(&builtin, "command_argument_type", name)
                    .map_err(s)
            };
            chunkwake_protocol::text::CommandParsers {
                string: parser("brigadier:string")?,
                integer: parser("brigadier:integer")?,
                item_stack: parser("minecraft:item_stack")?,
                resource: parser("minecraft:resource")?,
            }
        },
        survival: survival_data(
            data_dir,
            &blocks,
            &block_names,
            &items,
            &builtin,
            &registries,
            loaded,
            props.difficulty,
        )?,
    };
    let plan = PlayPlan::build(&packets, login, world, props.compression_threshold).map_err(s)?;
    tracing::debug!("world: play plan {} ms", t.elapsed().as_millis());
    let storage = WorldStorage::new(
        world_dir,
        blocks,
        biomes,
        data_version,
        dim.min_y,
        chunk,
        light,
        items,
        chunkwake_data::registries::builtin_entries_of(&builtin, "mob_effect").map_err(s)?,
    );
    Ok((plan, storage))
}

/// Имя блока по каждому состоянию: пустая строка у пропущенных ID.
fn state_names(blocks: &Blocks) -> Vec<String> {
    (0..blocks.state_count() as u32)
        .map(|id| {
            blocks
                .state(id)
                .map_or_else(String::new, |(n, _)| n.to_owned())
        })
        .collect()
}

/// Состояния сундуков: по состоянию свойства `facing`, `type` и `waterlogged`.
fn chest_states(
    blocks: &Blocks,
) -> std::collections::HashMap<u32, chunkwake_net::play::ChestState> {
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
        .collect()
}

/// Состояние печи → то же с другим `lit`.
fn furnace_states(blocks: &Blocks) -> std::collections::HashMap<u32, u32> {
    let mut by_props: std::collections::HashMap<(String, Vec<(String, String)>), u32> =
        Default::default();
    for id in 0..blocks.state_count() as u32 {
        if let Some((name, props)) = blocks.state(id)
            && chunkwake_data::world::FURNACES.contains(&name)
        {
            by_props.insert((name.to_owned(), props.to_vec()), id);
        }
    }
    by_props
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
        .collect()
}

/// Ноги игрока на блоке над верхним камнем столбца 8,8. Нет камня — уровень моря.
fn stand_on_solid(fill: &[chunkwake_worldgen::Fill], height: i32, min_y: i32) -> f64 {
    for y in (0..height).rev() {
        let i = ((y * 16 + 8) * 16 + 8) as usize;
        if fill.get(i) == Some(&chunkwake_worldgen::Fill::Solid) {
            return (min_y + y + 1) as f64;
        }
    }
    64.0
}

/// Твёрдость и дроп блоков, размеры стопок, типы урона.
#[allow(clippy::too_many_arguments)]
fn survival_data(
    data_dir: &Path,
    blocks: &Blocks,
    block_names: &Arc<Vec<String>>,
    items: &std::collections::HashMap<String, i32>,
    builtin: &Builtin,
    registries: &RegistryData,
    loaded: Loaded,
    difficulty: Difficulty,
) -> Result<SurvivalData, String> {
    let s = |e: chunkwake_data::DataError| e.to_string();
    // Таблицы лута разбираются в фоне: приём игроков их не ждёт.
    let loot = chunkwake_data::world::LootSource::load_in_background(data_dir);
    let damage = chunkwake_data::registries::builtin_id_of(
        builtin,
        "data_component_type",
        "minecraft:damage",
    )
    .map_err(s)?;
    chunkwake_protocol::inventory::DAMAGE_COMPONENT
        .store(damage, std::sync::atomic::Ordering::Relaxed);
    // Чары, имя и описание предметов: номера компонентов и сетевые ID чар.
    {
        use chunkwake_protocol::components as c;
        let component = |name| {
            chunkwake_data::registries::builtin_id_of(builtin, "data_component_type", name)
                .map_err(s)
        };
        let order = std::sync::atomic::Ordering::Relaxed;
        c::ENCHANTMENTS_COMPONENT.store(component("minecraft:enchantments")?, order);
        c::CUSTOM_NAME_COMPONENT.store(component("minecraft:custom_name")?, order);
        c::LORE_COMPONENT.store(component("minecraft:lore")?, order);
        c::STORED_ENCHANTMENTS_COMPONENT.store(component("minecraft:stored_enchantments")?, order);
        c::CUSTOM_DATA_COMPONENT.store(component("minecraft:custom_data")?, order);
        c::CHARGED_PROJECTILES_COMPONENT.store(component("minecraft:charged_projectiles")?, order);
        c::set_enchantments(loaded.enchantment_names.clone());
    }
    let _ = chunkwake_world::BLOCK_ENTITY_TYPES
        .set(chunkwake_data::registries::builtin_ids_of(builtin, "block_entity_type").map_err(s)?);
    // Сундуки: свойства `facing`/`type`/`waterlogged` каждого состояния.
    let chests = chest_states(blocks);
    let furnace_lit = furnace_states(blocks);
    let recipes = loaded.recipes;
    tracing::info!(
        "enchanting: {} enchantments ({} on the table), {} enchantable items",
        loaded.enchanting.defs.len(),
        loaded.enchanting.defs.iter().filter(|d| d.in_table).count(),
        loaded.enchanting.enchantable.len()
    );
    let menu = |name| chunkwake_data::registries::builtin_id_of(builtin, "menu", name).map_err(s);
    let survival = SurvivalData {
        enchantments: loaded.enchantment_lines,
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
        enchanting: loaded.enchanting,
        menu_furnace: [
            menu("minecraft:furnace")?,
            menu("minecraft:blast_furnace")?,
            menu("minecraft:smoker")?,
        ],
        furnace_lit,
        effects: chunkwake_data::registries::builtin_entries_of(builtin, "mob_effect")
            .map_err(s)?,
        attributes: chunkwake_data::registries::builtin_entries_of(builtin, "attribute")
            .map_err(s)?,
        sounds: chunkwake_data::registries::builtin_entries_of(builtin, "sound_event")
            .map_err(s)?,
        attribute_speed: chunkwake_data::registries::builtin_id_of(
            builtin,
            "attribute",
            "minecraft:movement_speed",
        )
        .map_err(s)?,
        attribute_jump: chunkwake_data::registries::builtin_id_of(
            builtin,
            "attribute",
            "minecraft:jump_strength",
        )
        .map_err(s)?,
        attribute_gravity: chunkwake_data::registries::builtin_id_of(
            builtin,
            "attribute",
            "minecraft:gravity",
        )
        .map_err(s)?,
        placement: chunkwake_net::placement::Placement::with_states(
            blocks.states(),
            loaded.sturdy,
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
        hardness: loaded.hardness,
        requires_tool: loaded.requires_tool,
        tools: loaded.tools,
        block_names: block_names.clone(),
        block_types: chunkwake_data::registries::builtin_ids_of(builtin, "block").map_err(s)?,
        block_defaults: blocks.defaults(),
        loot,
        items: loaded.item_info,
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
        snowball_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:snowball",
        )
        .map_err(s)?,
        egg_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:egg",
        )
        .map_err(s)?,
        arrow_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:arrow",
        )
        .map_err(s)?,
        firework_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
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
            let map =
                chunkwake_data::registries::builtin_entries_of(builtin, "potion").map_err(s)?;
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
        potion_component: chunkwake_data::registries::builtin_id_of(
            builtin,
            "data_component_type",
            "minecraft:potion_contents",
        )
        .map_err(s)?,
        item_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:item",
        )
        .map_err(s)?,
        orb_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:experience_orb",
        )
        .map_err(s)?,
        mob_entity_types: {
            let id =
                |n| chunkwake_data::registries::builtin_id_of(builtin, "entity_type", n).map_err(s);
            [
                id("minecraft:cow")?,
                id("minecraft:pig")?,
                id("minecraft:sheep")?,
                id("minecraft:chicken")?,
            ]
        },
    };
    Ok(survival)
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
