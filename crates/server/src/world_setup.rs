//! Мир M2: плоский overworld (bedrock, 2×dirt, grass) и параметры входа игрока.

use crate::config::Properties;
use chunkwake_data::packets::PacketTable;
use chunkwake_data::registries::RegistryData;
use chunkwake_data::world::{Blocks, DimensionType};
use chunkwake_net::play::PlayPlan;
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
) -> Result<PlayPlan, String> {
    let s = |e: chunkwake_data::DataError| e.to_string();
    let blocks = Blocks::load(data_dir).map_err(s)?;
    let dim = DimensionType::load(data_dir, DIMENSION).map_err(s)?;
    let biome = registries
        .network_id("worldgen/biome", "minecraft:plains")
        .map_err(s)?;
    let biome_count = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map_or(1, |r| r.entries.len());

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
    let chunk = ChunkColumn::flat(
        &states,
        dim.sections(),
        biome as u32,
        bits_for(blocks.state_count()),
        bits_for(biome_count),
        dim.has_skylight,
    );

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
            game_mode: 1, // creative: пока нет ни урона, ни голода
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
    PlayPlan::build(
        packets,
        login,
        chunk.encode_body(),
        spawn,
        player_type,
        props.compression_threshold,
    )
    .map_err(s)
}
