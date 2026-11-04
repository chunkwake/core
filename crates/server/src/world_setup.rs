//! Мир: плоский overworld по `level.dat` / `server.properties` и параметры входа игрока.

mod level;

use crate::MC_VERSION;
use crate::config::Properties;
use crate::storage::WorldStorage;
use chunkwake_data::boot::Boot;
use chunkwake_data::registries::{Builtin, RegistryData};
use chunkwake_data::world::Blocks;
use chunkwake_net::play::{PlayPlan, SurvivalData, WorldSpec};
use chunkwake_net::survival::Difficulty;
use chunkwake_protocol::play::{Login, SpawnInfo};
use chunkwake_world::{ChunkColumn, bits_for};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

const DIMENSION: &str = "overworld";

/// `/locate`: ближайшая точка от блока. Функция нужна и биомам, и структурам.
type Locate = std::sync::Arc<dyn Fn(&str, [i32; 3]) -> Option<[i32; 3]> + Send + Sync>;

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
        level::Kind::Noise(noise) => {
            if let Some(biome) = &noise.biome {
                tracing::info!(
                    "world: seed {}, noise {}, biome {biome}",
                    level.seed,
                    noise.settings
                );
            } else {
                tracing::info!("world: seed {}, noise {}", level.seed, noise.settings);
            }
        }
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
        level::Kind::Noise(noise) => noise.biome.as_deref().unwrap_or("minecraft:plains"),
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
        level::Kind::Noise(_) => 0.0,
    };
    let t_light = std::time::Instant::now();
    let light = std::sync::Arc::new(chunkwake_world::light::LightTable::new(light_rows));
    tracing::debug!("light table {} ms", t_light.elapsed().as_millis());
    let t_chunk = std::time::Instant::now();
    let mut chunk = ChunkColumn::flat(
        &states,
        dim.sections(),
        biome as u32,
        bits_for(blocks.state_count()),
        bits_for(biome_count),
        dim.has_skylight,
    );
    tracing::debug!("template column {} ms", t_chunk.elapsed().as_millis());
    let t_relight = std::time::Instant::now();
    chunkwake_world::light::relight_column(&mut chunk, &light);
    tracing::debug!("template relight {} ms", t_relight.elapsed().as_millis());

    let block_bits = bits_for(blocks.state_count());
    let biome_bits = bits_for(biome_count);
    let poi_tags = chunkwake_net::play::load_poi_tags(data_dir);
    let structure_names = chunkwake_net::play::structure_names(data_dir);
    chunkwake_world::set_fluid_states(fluid_states(&blocks.states()));
    chunkwake_world::set_state_flags(state_flags(&blocks.states()));
    chunkwake_world::set_extra_flags(extra_flags(&blocks.states()));
    chunkwake_world::set_rare_flags(rare_flags(&blocks.states()));
    chunkwake_world::set_poi_bits(poi_bits(&blocks.states()));
    chunkwake_world::set_height_classes(height_classes(data_dir, &blocks.states())?);
    let (terrain, locate_biome, locate_structure, spawn_noise, sea_level, is_flat) = match &level
        .kind
    {
        level::Kind::Flat(_) => (None, None, None, None, 63, true),
        level::Kind::Noise(noise) => {
            let mut loaded = chunkwake_worldgen::Generator::load_settings(
                data_dir,
                level.seed,
                Some(data_version),
                &noise.settings,
            )?;
            if loaded.min_y != dim.min_y || loaded.height != dim.height {
                return Err(format!(
                    "noise settings y {} height {} do not match the overworld y {} height {}",
                    loaded.min_y, loaded.height, dim.min_y, dim.height
                ));
            }
            // Биомы выбирает климат (`MultiNoiseBiomeSource`), а не один plains.
            // `single_biome_surface` подменяет климат одним биомом.
            loaded.bind_biomes(biome_ids.clone());
            let fixed = noise.biome.is_some();
            if let Some(name) = &noise.biome {
                loaded.set_fixed_biome(name, &biome_ids)?;
            }
            let climate = loaded.has_biomes();
            let noise = std::sync::Arc::new(loaded);
            let sea = noise.sea_level;
            // `NoiseSpawnFinder.findSpawnPosition` даёт точку `(x, 0, z)`;
            // высоту берём по столбцу этой точки, как `ServerLevel`.
            let stand = noise
                .spawn_position()
                .map(|p| {
                    let cx = p[0].div_euclid(16);
                    let cz = p[2].div_euclid(16);
                    let y =
                        stand_on_solid(&noise.fill(cx, cz), noise.height, dim.min_y, p[0], p[2]);
                    [p[0] as f64 + 0.5, y, p[2] as f64 + 0.5]
                })
                .unwrap_or([8.5, 64.0, 8.5]);
            let biome_id = biome as u32;
            let sky = dim.has_skylight;
            // Шум, поверхность и карверы — один конвейер генератора.
            let state = |name: &str| blocks.default_state(name).ok().map(|s| s as u32);
            let overworld =
                chunkwake_worldgen::pipeline::Overworld::new(noise, &state, &biome_ids)?
                    .with_features(data_dir, &blocks.states(), &state, &biome_ids)?;
            if let Some((done, all)) = overworld.feature_coverage() {
                tracing::info!("worldgen features: {done} of {all} ported");
                for (name, why) in overworld.feature_failures().iter().take(5) {
                    tracing::warn!("worldgen feature {name}: {why}");
                }
            }
            if !overworld.has_surface() {
                tracing::warn!("no material_rule in the data cache: the world keeps bare stone");
            }
            // Свет считает пролив при отправке: здесь только блоки.
            // Тот же генератор отвечает `/locate biome`, не строя чанк.
            let overworld = std::sync::Arc::new(overworld);
            let locate_world = overworld.clone();
            let locate: Locate =
                std::sync::Arc::new(move |name, origin| locate_world.locate_biome(name, origin));
            let locate_struct_world = overworld.clone();
            let locate_structure: Locate = std::sync::Arc::new(move |name, origin| {
                locate_struct_world.locate_structure(name, origin)
            });
            let built: std::sync::Arc<dyn Fn(i32, i32) -> Option<ChunkColumn> + Send + Sync> =
                std::sync::Arc::new(move |cx, cz| {
                    // `None` — чанк уже отдан: генератор его не повторяет, и
                    // вызывающему нужен готовый столбец из мира, а не копия.
                    let chunk = overworld.chunk(cx, cz)?;
                    let mut column = match chunk.biomes {
                        Some(biomes) => ChunkColumn::from_blocks_with_biomes(
                            &chunk.states,
                            &biomes,
                            block_bits,
                            biome_bits,
                            sky,
                        ),
                        None => ChunkColumn::from_blocks(
                            &chunk.states,
                            biome_id,
                            block_bits,
                            biome_bits,
                            sky,
                        ),
                    };
                    // Лут структур: ванила кладёт в сундук `LootTable` и
                    // `LootTableSeed`, содержимое считается при открытии.
                    column.fluid_posts = chunk.fluid_posts.clone();
                    for ([x, y, z], id, nbt) in &chunk.block_entities {
                        column.block_entities.insert(
                            [*x, *y, *z],
                            chunkwake_world::BlockEntity {
                                id: id.clone(),
                                nbt: nbt.clone(),
                            },
                        );
                    }
                    Some(column)
                });
            if !fixed && !climate {
                tracing::warn!(
                    "reports/biomes.csv is missing: the noise world keeps a single biome. \
                     Delete the data cache or rebuild it to get real biomes"
                );
            }
            (
                Some(built),
                Some(locate),
                Some(locate_structure),
                Some(stand),
                sea,
                false,
            )
        }
    };

    let dimension = format!("minecraft:{DIMENSION}");

    // Ад и Край: у ванили они есть в любом пресете `world_preset`, плоский
    // мир и пустота тоже получают обычные `nether` и `end`.
    let nether_dim = build_dimension(
        data_dir,
        level.seed,
        data_version,
        &blocks,
        &biome_ids,
        registries
            .network_id("dimension_type", "minecraft:the_nether")
            .map_err(s)?,
        "minecraft:the_nether",
        "minecraft:nether",
        "minecraft:netherrack",
        "minecraft:lava",
        "minecraft:nether_wastes",
    )?;
    let end_dim = build_dimension(
        data_dir,
        level.seed,
        data_version,
        &blocks,
        &biome_ids,
        registries
            .network_id("dimension_type", "minecraft:the_end")
            .map_err(s)?,
        "minecraft:the_end",
        "minecraft:end",
        "minecraft:end_stone",
        "minecraft:air",
        "minecraft:the_end",
    )?;

    let login = Login {
        entity_id: 0,
        hardcore: props.hardcore,
        dimensions: {
            let mut list = vec![dimension.clone()];
            if nether_dim.is_some() {
                list.push("minecraft:the_nether".to_owned());
            }
            if end_dim.is_some() {
                list.push("minecraft:the_end".to_owned());
            }
            list
        },
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
    // Нормальный: точка `NoiseSpawnFinder` и высота её столбца.
    tracing::debug!("world: level and template {} ms", t.elapsed().as_millis());
    let void_platform = matches!(&level.kind, level::Kind::Flat(flat) if flat.void_platform());
    let spawn = if let Some(p) = spawn_noise {
        p
    } else if void_platform {
        // Ноги на блоке над булыжником `(8, minY+3, 8)`.
        [8.5, (dim.min_y + 4) as f64, 8.5]
    } else if let level::Kind::Flat(flat) = &level.kind
        && flat.layers.iter().any(|(b, _)| b != "minecraft:air")
    {
        [8.5, dim.min_y as f64 + top, 8.5]
    } else {
        [8.5, 64.0, 8.5]
    };
    let void_blocks = if void_platform {
        Some((
            blocks.default_state("minecraft:stone").map_err(s)? as u32,
            blocks.default_state("minecraft:cobblestone").map_err(s)? as u32,
        ))
    } else {
        None
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
        dimensions: {
            let mut list = vec![(
                chunkwake_net::play::Dimension::Overworld,
                chunkwake_net::play::DimSpec {
                    dimension_type: registries
                        .network_id("dimension_type", "minecraft:overworld")
                        .map_err(s)?,
                    template: chunk.clone(),
                    terrain,
                    locate_biome,
                    locate_structure,
                    void_platform: void_blocks,
                    min_y: dim.min_y,
                },
            )];
            if let Some(nether) = nether_dim {
                list.push((chunkwake_net::play::Dimension::Nether, nether));
            }
            if let Some(end) = end_dim {
                list.push((chunkwake_net::play::Dimension::End, end));
            }
            list
        },
        trees: std::sync::Arc::new(chunkwake_worldgen::feature::RuntimeTrees::load(
            data_dir,
            &blocks.states(),
            &|name| blocks.default_state(name).ok().map(|s| s as u32),
            dim.min_y,
            dim.height,
        )?),
        poi_tags,
        structure_names,
        light: light.clone(),
        item_blocks,
        spawn,
        player_type,
        block_names: block_names.clone(),
        parsers: chunkwake_protocol::text::CommandParsers::load(|name| {
            chunkwake_data::registries::builtin_id_of(&builtin, "command_argument_type", name)
                .map_err(s)
        })?,
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
    let mut plan = plan;
    // Командам нужны настройки сервера: правила, белый список и каталог `/reload`.
    // Каталог сервера — родитель каталога мира: там `ops.json` и `server.properties`.
    let server_dir = world_dir.parent().unwrap_or(world_dir);
    let version_protocol = crate::config::read_cached_version(data_dir).map_or(0, |v| v.protocol);
    plan.set_particle_types(
        chunkwake_data::registries::builtin_ids_of(&builtin, "particle_type").map_err(s)?,
    );
    plan.set_command_lang(command_language(data_dir));
    plan.configure(
        &chunkwake_net::play::commands::PlanConfig {
            level_name: &props.level_name,
            motd: &props.motd,
            seed: level.seed,
            version_name: MC_VERSION,
            version_protocol,
            server_dir,
            whitelist: props.white_list,
            pvp: props.pvp,
            allow_flight: props.allow_flight,
        },
        world_dir,
    );
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
            let (trapped, copper) = match name {
                "minecraft:chest" => (false, 0),
                "minecraft:trapped_chest" => (true, 0),
                "minecraft:copper_chest" => (false, 1),
                "minecraft:exposed_copper_chest" => (false, 2),
                "minecraft:weathered_copper_chest" => (false, 3),
                "minecraft:oxidized_copper_chest" => (false, 4),
                "minecraft:waxed_copper_chest" => (false, 5),
                "minecraft:waxed_exposed_copper_chest" => (false, 6),
                "minecraft:waxed_weathered_copper_chest" => (false, 7),
                "minecraft:waxed_oxidized_copper_chest" => (false, 8),
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
                    copper,
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
fn stand_on_solid(
    fill: &[chunkwake_worldgen::Fill],
    height: i32,
    min_y: i32,
    x: i32,
    z: i32,
) -> f64 {
    let lx = (x & 15) as usize;
    let lz = (z & 15) as usize;
    for y in (0..height).rev() {
        let i = (y as usize * 16 + lz) * 16 + lx;
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
        c::REPAIR_COST_COMPONENT.store(component("minecraft:repair_cost")?, order);
        c::TRIM_COMPONENT.store(component("minecraft:trim")?, order);
        c::BANNER_PATTERNS_COMPONENT.store(component("minecraft:banner_patterns")?, order);
        c::WRITTEN_BOOK_CONTENT_COMPONENT
            .store(component("minecraft:written_book_content")?, order);
        c::WRITABLE_BOOK_CONTENT_COMPONENT
            .store(component("minecraft:writable_book_content")?, order);
        c::set_banner_pattern_names(
            registries
                .synchronized
                .iter()
                .find(|r| r.name == "banner_pattern")
                .map_or(Vec::new(), |r| r.entries.clone()),
        );
        c::set_trim_names(
            registries
                .synchronized
                .iter()
                .find(|r| r.name == "trim_material")
                .map_or(Vec::new(), |r| r.entries.clone()),
            registries
                .synchronized
                .iter()
                .find(|r| r.name == "trim_pattern")
                .map_or(Vec::new(), |r| r.entries.clone()),
        );
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
    let banner_tags = registries
        .tags
        .iter()
        .find(|t| t.registry == "banner_pattern");
    let banner_tag = |name: &str| {
        banner_tags
            .and_then(|t| {
                t.tags
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, ids)| ids.clone())
            })
            .unwrap_or_default()
    };
    let loom_basic = banner_tag("minecraft:no_item_required");
    let mut loom_special = HashMap::new();
    for (name, &id) in items
        .iter()
        .filter(|(name, _)| name.ends_with("_banner_pattern"))
    {
        let path = data_dir
            .join("reports/minecraft/components/item")
            .join(format!("{}.json", name.trim_start_matches("minecraft:")));
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(report) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let Some(tag) = report
            .pointer("/components/minecraft:provides_banner_patterns")
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        if let Some(tag) = tag.strip_prefix('#') {
            let ids = banner_tag(tag);
            if !ids.is_empty() {
                loom_special.insert(id, ids);
            }
        }
    }
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
        menu_anvil: menu("minecraft:anvil")?,
        menu_smithing: menu("minecraft:smithing")?,
        menu_stonecutter: menu("minecraft:stonecutter")?,
        menu_loom: menu("minecraft:loom")?,
        menu_lectern: menu("minecraft:lectern")?,
        menu_brewing: menu("minecraft:brewing_stand")?,
        loom_basic,
        loom_special,
        anvil_repair: chunkwake_data::enchanting::repair_materials(data_dir, items),
        smithing: chunkwake_data::recipes::load_smithing(data_dir, items, registries).map_err(s)?,
        brewing: chunkwake_data::recipes::load_brewing(
            data_dir,
            items,
            &chunkwake_data::registries::builtin_entries_of(builtin, "potion").map_err(s)?,
        )
        .map_err(s)?,
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
        instruments: chunkwake_data::world::load_instruments(data_dir).map_err(s)?,
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
        damage_explosion: registries
            .network_id("damage_type", "minecraft:explosion")
            .map_err(s)?,
        damage_fire: registries
            .network_id("damage_type", "minecraft:on_fire")
            .map_err(s)?,
        damage_lava: registries
            .network_id("damage_type", "minecraft:lava")
            .map_err(s)?,
        damage_drown: registries
            .network_id("damage_type", "minecraft:drown")
            .map_err(s)?,
        damage_cactus: registries
            .network_id("damage_type", "minecraft:cactus")
            .map_err(s)?,
        damage_berry: registries
            .network_id("damage_type", "minecraft:sweet_berry_bush")
            .map_err(s)?,
        damage_freeze: registries
            .network_id("damage_type", "minecraft:freeze")
            .map_err(s)?,
        damage_magma: registries
            .network_id("damage_type", "minecraft:hot_floor")
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
        pearl_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:ender_pearl",
        )
        .map_err(s)?,
        eye_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:eye_of_ender",
        )
        .map_err(s)?,
        potion_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            // В ванили отдельного `minecraft:potion` нет: брошенное зелье —
            // это `minecraft:splash_potion` (устойчивое — `lingering_potion`).
            "minecraft:splash_potion",
        )
        .map_err(s)?,
        lightning_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:lightning_bolt",
        )
        .map_err(s)?,
        arrow_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:arrow",
        )
        .map_err(s)?,
        tnt_entity_type: chunkwake_data::registries::builtin_id_of(
            builtin,
            "entity_type",
            "minecraft:tnt",
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
        creature_spawns: creature_spawns(data_dir, registries),
        physics: chunkwake_data::world::load_physics(data_dir)
            .ok()
            .map(std::sync::Arc::new),
        item_tags: chunkwake_net::play::load_registry_tags(data_dir, "item"),
        block_tags: chunkwake_net::play::load_registry_tags(data_dir, "block"),
        biome_names: registries
            .synchronized
            .iter()
            .find(|r| r.name == "worldgen/biome")
            .map(|r| r.entries.clone())
            .unwrap_or_default(),
        weather_biomes: chunkwake_net::weather::load_biomes(
            data_dir,
            &registries
                .synchronized
                .iter()
                .find(|r| r.name == "worldgen/biome")
                .map(|r| r.entries.clone())
                .unwrap_or_default(),
        ),
        biome_tags: chunkwake_net::play::load_registry_tags(data_dir, "worldgen/biome"),
        data_dir: data_dir.to_owned(),
        block_states: blocks.states(),
    };
    Ok(survival)
}

/// Мирные мобы биомов для заселения новых чанков.
fn creature_spawns(
    data_dir: &Path,
    registries: &RegistryData,
) -> Vec<chunkwake_net::play::CreatureSpawns> {
    let names = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| r.entries.clone())
        .unwrap_or_default();
    names
        .iter()
        .map(|name| {
            let (ns, path) = name.split_once(':').unwrap_or(("minecraft", name));
            let file = data_dir.join("data").join(ns).join("worldgen/biome").join(format!("{path}.json"));
            let Some(v) = std::fs::read(&file)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            else {
                return Default::default();
            };
            let attrs = &v["attributes"];
            let chance = attrs["minecraft:gameplay/creature_world_gen_spawn_probability"]
                .as_f64()
                .unwrap_or(0.1) as f32;
            let list = &attrs["minecraft:gameplay/natural_mob_spawns"]["argument"]["spawns_by_category"]["creature"];
            let entries = list
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| {
                    let kind = e["type"].as_str()?.to_string();
                    let weight = e["weight"].as_u64()? as u32;
                    let c = &e["count"];
                    let (lo, hi) = match c.as_u64() {
                        Some(n) => (n as u32, n as u32),
                        None => (c["min_inclusive"].as_u64()? as u32, c["max_inclusive"].as_u64()? as u32),
                    };
                    Some((kind, weight, lo, hi))
                })
                .collect();
            chunkwake_net::play::CreatureSpawns { chance, entries }
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

/// Биты карт высот: 1 поверхность, 2 движение, 4 движение без листвы,
/// 8 океанское дно (без воды и листвы — нужно только в файле, не клиенту).
/// Листва не поднимает карты движения — иначе миникарта считает игрока в пещере.
fn height_classes(
    data_dir: &Path,
    states: &[Option<(String, chunkwake_data::world::Properties)>],
) -> Result<Vec<u8>, String> {
    let motion = block_tag(data_dir, "#minecraft:blocks_motion_in_heightmap")?;
    let bare = block_tag(data_dir, "#minecraft:blocks_motion_in_heightmap_no_leaves")?;
    let leaves = block_tag(data_dir, "#minecraft:leaves")?;
    const ALWAYS: &[&str] = &[
        "minecraft:water",
        "minecraft:lava",
        "minecraft:bubble_column",
        "minecraft:kelp",
        "minecraft:kelp_plant",
        "minecraft:seagrass",
        "minecraft:tall_seagrass",
    ];
    Ok(states
        .iter()
        .map(|s| {
            let Some((name, props)) = s else {
                return 0;
            };
            let fluid = ALWAYS.contains(&name.as_str())
                || props.iter().any(|(k, v)| k == "waterlogged" && v == "true");
            let mut bits = 0u8;
            let air = matches!(
                name.as_str(),
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
            );
            if !air {
                bits |= 1;
            }
            if motion.contains(name) || fluid {
                bits |= 2;
            }
            if bare.contains(name) || fluid {
                bits |= 4;
            }
            // `OCEAN_FLOOR`: всё, что не вода и не листва. Листва берётся из
            // тега `#minecraft:leaves`, потому что таблицу классов собирают
            // здесь же, а `props` про листву ничего не знает.
            if !air && !fluid && !leaves.contains(name) {
                bits |= chunkwake_world::OCEAN_FLOOR;
            }
            bits
        })
        .collect())
}

/// Тег блоков `data/<ns>/tags/block`, вместе с вложенными `#тегами`.
fn block_tag(root: &Path, spec: &str) -> Result<HashSet<String>, String> {
    fn resolve(
        root: &Path,
        spec: &str,
        memo: &mut HashMap<String, HashSet<String>>,
        visiting: &mut HashSet<String>,
    ) -> Result<HashSet<String>, String> {
        if let Some(hit) = memo.get(spec) {
            return Ok(hit.clone());
        }
        if !visiting.insert(spec.to_owned()) {
            return Ok(HashSet::new());
        }
        let mut out = HashSet::new();
        if let Some(tag) = spec.strip_prefix('#') {
            let tag = ns(tag);
            let (namespace, path) = tag.split_once(':').unwrap_or(("minecraft", tag.as_str()));
            let file = root.join(format!("data/{namespace}/tags/block/{path}.json"));
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{file:?}: {e}"))?;
            let json: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("{file:?}: {e}"))?;
            for v in json["values"].as_array().into_iter().flatten() {
                let id = v.as_str().or_else(|| v["id"].as_str()).unwrap_or("");
                if id.is_empty() {
                    continue;
                }
                if id.starts_with('#') {
                    out.extend(resolve(root, id, memo, visiting)?);
                } else {
                    out.insert(ns(id));
                }
            }
        } else {
            out.insert(ns(spec));
        }
        visiting.remove(spec);
        memo.insert(spec.to_owned(), out.clone());
        Ok(out)
    }
    resolve(root, spec, &mut HashMap::new(), &mut HashSet::new())
}

fn ns(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

/// Состояния с жидкостью: вода, лава, пузырьки, водоросли и всё затопленное.
fn fluid_states(states: &[Option<(String, chunkwake_data::world::Properties)>]) -> Vec<bool> {
    const ALWAYS: &[&str] = &[
        "minecraft:water",
        "minecraft:lava",
        "minecraft:bubble_column",
        "minecraft:kelp",
        "minecraft:kelp_plant",
        "minecraft:seagrass",
        "minecraft:tall_seagrass",
    ];
    states
        .iter()
        .map(|s| {
            s.as_ref().is_some_and(|(name, props)| {
                ALWAYS.contains(&name.as_str())
                    || props.iter().any(|(k, v)| k == "waterlogged" && v == "true")
            })
        })
        .collect()
}

/// Второй байт флагов состояния: плиты и воронка.
///
/// Отдельно от [`state_flags`], потому что первый байт читается в коллизиях
/// мобов и свете, и расширять его ради редких проверок не нужно.
fn extra_flags(states: &[Option<(String, chunkwake_data::world::Properties)>]) -> Vec<u8> {
    use chunkwake_world::extra as xf;
    states
        .iter()
        .map(|s| {
            let Some((name, _)) = s else { return 0 };
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let mut bits = 0u8;
            if path.ends_with("_pressure_plate") {
                bits |= xf::PLATE;
                if matches!(
                    path,
                    "light_weighted_pressure_plate" | "heavy_weighted_pressure_plate"
                ) {
                    bits |= xf::WEIGHTED_PLATE;
                } else if path.contains("stone") {
                    bits |= xf::LIVING_PLATE;
                }
            }
            if path == "hopper" {
                bits |= xf::HOPPER;
            }
            bits
        })
        .collect()
}

/// Третий байт флагов: то, чего не влезло во второй.
fn rare_flags(states: &[Option<(String, chunkwake_data::world::Properties)>]) -> Vec<u8> {
    use chunkwake_world::rare as rf;
    states
        .iter()
        .map(|s| {
            let Some((name, _)) = s else { return 0 };
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let mut bits = 0u8;
            if path == "tripwire" {
                bits |= rf::TRIPWIRE;
            }
            if path == "tripwire_hook" {
                bits |= rf::TRIPWIRE_HOOK;
            }
            bits
        })
        .collect()
}

/// Биты «состояние — точка интереса» по именам состояний.
///
/// Те же имена, что в `poi::of_block`, но считается один раз за старт:
/// скан нового чанка иначе сравнивал бы строки на каждом из 98 тыс. блоков.
fn poi_bits(states: &[Option<(String, chunkwake_data::world::Properties)>]) -> Vec<u16> {
    use chunkwake_net::play::poi_group;
    states
        .iter()
        .map(|s| s.as_ref().map_or(0, |(name, _)| poi_group(name)))
        .collect()
}

/// Флаги состояний для горячих проверок: воздух, твёрдость, наблюдатель,
/// трава, смываемое, пыль, факел, открываемое.
///
/// Считается один раз при сборке мира. Всё это тик спрашивает у блоков
/// миллионы раз за сессию (коллизии мобов, свет, наблюдатели, жидкости), и
/// сравнение имён там стоило дороже самого поиска блока.
fn state_flags(states: &[Option<(String, chunkwake_data::world::Properties)>]) -> Vec<u8> {
    use chunkwake_world::flags as f;
    states
        .iter()
        .map(|s| {
            let Some((name, _props)) = s else { return 0 };
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let mut bits = 0u8;
            let air = matches!(path, "air" | "cave_air" | "void_air");
            if air {
                bits |= f::AIR;
            }
            if matches!(
                path,
                "short_grass"
                    | "tall_grass"
                    | "fern"
                    | "large_fern"
                    | "dead_bush"
                    | "redstone_wire"
                    | "torch"
                    | "wall_torch"
                    | "redstone_torch"
                    | "redstone_wall_torch"
                    | "lever"
                    | "rail"
                    | "tripwire"
                    | "fire"
                    | "vine"
                    | "seagrass"
                    | "snow"
                    | "dandelion"
                    | "poppy"
                    | "wheat"
                    | "carrots"
                    | "potatoes"
                    | "beetroots"
            ) || path.ends_with("_sapling")
                || path.ends_with("_tulip")
                || path.ends_with("_button")
            {
                bits |= f::REPLACEABLE;
            }
            // Твёрдое для коллизий: не воздух, не жидкость и не смываемое.
            // Затопленный блок (ступени, забор) по-прежнему твёрдый — так же,
            // как в старой проверке по имени, где `fluid_of(name, None)` у
            // такого блока `None`.
            let fluid = matches!(path, "water" | "lava" | "bubble_column");
            if !air && !fluid && bits & f::REPLACEABLE == 0 {
                bits |= f::SOLID;
            }
            if path == "observer" {
                bits |= f::OBSERVER;
            }
            if path == "grass_block" {
                bits |= f::GRASS;
            }
            if path == "redstone_wire" {
                bits |= f::WIRE;
            }
            if matches!(path, "redstone_torch" | "redstone_wall_torch") {
                bits |= f::TORCH;
            }
            if path.ends_with("_door")
                || path.ends_with("_trapdoor")
                || path.ends_with("_fence_gate")
            {
                bits |= f::OPENABLE;
            }
            bits
        })
        .collect()
}

/// `en_us.json` для сообщений консоли: `assets/minecraft/lang` из каталога
/// данных (он же лежит во встроенном архиве). Нет файла — консоль оставляет
/// ключ, но пишет почему.
fn command_language(data_dir: &Path) -> HashMap<String, String> {
    let file = data_dir.join("assets/minecraft/lang/en_us.json");
    match std::fs::read_to_string(&file) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(lang) => lang,
            Err(error) => {
                tracing::warn!(
                    "console command errors stay as translation keys: {}: {error}",
                    file.display()
                );
                HashMap::new()
            }
        },
        Err(error) => {
            tracing::warn!(
                "console command errors stay as translation keys: {}: {error}",
                file.display()
            );
            HashMap::new()
        }
    }
}

/// Ад или Край: свой `noise_settings` и свой генератор с фичами и структурами.
///
/// Ключевое отличие от оверворлда — `new_for` со своим `default_block`
/// (`netherrack`, `end_stone`): карверы там не ставят траву с берёзой. Биомы —
/// свой источник измерения, `biomes.csv` оверворлда не используется.
#[allow(clippy::too_many_arguments)]
/// `dimension_type/<имя>.json`: полная высота измерения.
fn dimension_height(data_dir: &Path, key: &str) -> Option<i32> {
    let name = key.strip_prefix("minecraft:").unwrap_or(key);
    let path = data_dir.join(format!("data/minecraft/dimension_type/{name}.json"));
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    json["height"].as_i64().map(|h| h as i32)
}

fn build_dimension(
    data_dir: &Path,
    seed: i64,
    data_version: i32,
    blocks: &Blocks,
    biome_ids: &HashMap<String, u32>,
    dimension_type: i32,
    dimension_key: &str,
    settings: &str,
    default_block: &str,
    default_fluid: &str,
    fallback_biome: &str,
) -> Result<Option<chunkwake_net::play::DimSpec>, String> {
    let mut noise = match chunkwake_worldgen::Generator::load_settings(
        data_dir,
        seed,
        Some(data_version),
        settings,
    ) {
        Ok(noise) => noise,
        Err(e) => {
            tracing::warn!("{settings}: noise settings не собрались ({e}); измерение выключено");
            return Ok(None);
        }
    };
    noise.bind_biomes(biome_ids.clone());
    let sea_level = noise.sea_level;
    let min_y = noise.min_y;
    let height = noise.height;
    // Клиент ждёт секции на всю высоту `dimension_type` (у Ада и Края 256),
    // а шум заполняет только свои 128: выше — воздух, как у `ProtoChunk`.
    let dim_height = dimension_height(data_dir, dimension_key).unwrap_or(height).max(height);
    let state = |name: &str| blocks.default_state(name).ok().map(|s| s as u32);
    let air = state("minecraft:air").unwrap_or(0);
    let built = chunkwake_worldgen::pipeline::Overworld::new_for(
        std::sync::Arc::new(noise),
        &state,
        biome_ids,
        default_block,
        default_fluid,
    )
    .and_then(|w| w.with_features(data_dir, &blocks.states(), &state, biome_ids))
    .map_err(|e| format!("{settings} generator: {e}"))?;
    if let Some((done, all)) = built.feature_coverage() {
        tracing::info!("{settings}: фичи {done} из {all}");
        for (name, why) in built.feature_failures().iter().take(5) {
            tracing::warn!("{settings}: {name}: {why}");
        }
    }
    let built = std::sync::Arc::new(built);
    let block_bits = bits_for(blocks.state_count());
    let biome_bits = bits_for(biome_ids.len());
    let fallback = *biome_ids.get(fallback_biome).unwrap_or(&0);
    // Шаблон: основной блок измерения, без неба.
    let template = ChunkColumn::flat(
        &[(state(default_block).unwrap_or(0), height as usize)],
        (dim_height / 16) as usize,
        fallback,
        block_bits,
        biome_bits,
        false,
    );
    let world = built.clone();
    let terrain: chunkwake_net::play::TerrainFn = std::sync::Arc::new(move |cx, cz| {
        let mut chunk = world.chunk(cx, cz)?;
        if dim_height > height {
            chunk.states.resize(dim_height as usize * 256, air);
            if let Some(biomes) = &mut chunk.biomes {
                // Биомы над шумом берутся у того же источника.
                let generator = world.generator();
                for qy in height / 4..dim_height / 4 {
                    for qz in 0..4 {
                        for qx in 0..4 {
                            let id = generator
                                .biome_at(cx * 4 + qx, min_y / 4 + qy, cz * 4 + qz)
                                .unwrap_or(fallback);
                            biomes.push(id);
                        }
                    }
                }
            }
        }
        let mut column = match chunk.biomes {
            Some(biomes) => ChunkColumn::from_blocks_with_biomes(
                &chunk.states,
                &biomes,
                block_bits,
                biome_bits,
                false,
            ),
            None => ChunkColumn::from_blocks(&chunk.states, fallback, block_bits, biome_bits, false),
        };
        column.fluid_posts = chunk.fluid_posts.clone();
        for ([x, y, z], id, nbt) in &chunk.block_entities {
            column.block_entities.insert(
                [*x, *y, *z],
                chunkwake_world::BlockEntity {
                    id: id.clone(),
                    nbt: nbt.clone(),
                },
            );
        }
        Some(column)
    });
    let locate_world = built.clone();
    let locate_biome: Locate =
        std::sync::Arc::new(move |name, origin| locate_world.locate_biome(name, origin));
    let locate_struct_world = built;
    let locate_structure: Locate = std::sync::Arc::new(move |name, origin| {
        locate_struct_world.locate_structure(name, origin)
    });
    tracing::info!("{settings}: генератор готов (min_y {min_y}, высота {height}, море {sea_level})");
    Ok(Some(chunkwake_net::play::DimSpec {
        dimension_type,
        template,
        terrain: Some(terrain),
        locate_biome: Some(locate_biome),
        locate_structure: Some(locate_structure),
        void_platform: None,
        min_y,
    }))
}

#[cfg(test)]
mod tests {
    use super::{block_name_for_item, chest_states};

    #[test]
    fn all_copper_chest_variants_have_container_states() {
        let names = [
            "copper_chest",
            "exposed_copper_chest",
            "weathered_copper_chest",
            "oxidized_copper_chest",
            "waxed_copper_chest",
            "waxed_exposed_copper_chest",
            "waxed_weathered_copper_chest",
            "waxed_oxidized_copper_chest",
        ];
        let mut json = serde_json::Map::new();
        for (index, name) in names.iter().enumerate() {
            json.insert(
                format!("minecraft:{name}"),
                serde_json::json!({
                    "states": [{"id": index, "default": true, "properties": {
                        "facing": "north", "type": "single", "waterlogged": "false"
                    }}]
                }),
            );
        }
        let blocks =
            chunkwake_data::world::Blocks::parse(&serde_json::Value::Object(json)).unwrap();
        let states = chest_states(&blocks);
        for index in 0..8 {
            assert_eq!(states.get(&(index as u32)).unwrap().copper, index as u8 + 1);
        }
    }

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
