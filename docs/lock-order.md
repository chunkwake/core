# Lock-order аудит тик-пути

Сгенерировано `scripts/lock_audit.py`.

Захватов: 528, уникальных блокировок: 136.


## Порядок захвата (наблюдаемые рёбра)

| удерживается | берётся | раз |
| --- | --- | ---: |
| `?` | `state.evicted` | 1 |
| `?` | `state.regions` | 2 |
| `dim_state.chunk_frames` | `dim_state.dirty` | 3 |
| `dim_state.chunk_frames` | `dim_state.saved` | 1 |
| `dim_state.chunk_frames` | `sim.pending_beacons` | 1 |
| `dim_state.chunk_frames` | `sim.pending_campfires` | 1 |
| `dim_state.chunk_frames` | `sim.pending_conduits` | 1 |
| `dim_state.chunk_frames` | `sim.pending_jukeboxes` | 1 |
| `dim_state.chunk_frames` | `sim.populated` | 1 |
| `dim_state.edited` | `dim_state.chunk_frames` | 7 |
| `dim_state.edited` | `dim_state.dirty` | 6 |
| `dim_state.edited` | `dim_state.saved` | 1 |
| `dim_state.edited` | `sim.pending_beacons` | 1 |
| `dim_state.edited` | `sim.pending_campfires` | 1 |
| `dim_state.edited` | `sim.pending_conduits` | 1 |
| `dim_state.edited` | `sim.pending_jukeboxes` | 1 |
| `dim_state.edited` | `sim.populated` | 1 |
| `dim_state.edited` | `state.lent` | 1 |
| `dim_state.saved` | `sim.pending_beacons` | 1 |
| `dim_state.saved` | `sim.pending_campfires` | 1 |
| `dim_state.saved` | `sim.pending_conduits` | 1 |
| `dim_state.saved` | `sim.pending_jukeboxes` | 1 |
| `hub.players` | `sim.dropped` | 1 |
| `hub.players` | `sim.mobs` | 1 |
| `light_lock` | `dim_state.edited` | 1 |
| `light_lock` | `state.lent` | 2 |
| `live` | `hub.players` | 3 |
| `sim.arrows` | `hub.players` | 1 |
| `sim.dropped` | `sim.items_gone` | 1 |
| `sim.dropped` | `sim.items_merging` | 1 |
| `sim.dropped` | `sim.items_syncs` | 1 |
| `sim.dropped_by_chunk` | `sim.dropped` | 1 |
| `sim.entity_written` | `sim.entity_new` | 1 |
| `sim.entity_written` | `sim.mobs_loaded` | 1 |
| `sim.mobs` | `love_cause` | 2 |
| `sim.mobs` | `particle_types` | 1 |
| `sim.orbs` | `hub.players` | 1 |
| `sim.populated` | `sim.entity_written` | 1 |
| `sim.populated` | `sim.mobs_loaded` | 1 |
| `sim.simulated_key` | `hub.players` | 1 |
| `sim.torch_toggles` | `sim.torch_cool` | 1 |
| `state.dirty` | `state.sim.jukeboxes` | 1 |
| `state.edited` | `state.dirty` | 2 |
| `state.edited` | `state.saved` | 2 |
| `state.edited` | `state.sim.jukeboxes` | 1 |
| `state.generating` | `state.edited` | 1 |
| `state.saved` | `state.dirty` | 2 |
| `state.saved` | `state.sim.jukeboxes` | 1 |
| `weather_levels` | `hub.players` | 6 |
| `weather_levels` | `plugin_invs` | 1 |
| `weather_levels` | `sleepers` | 1 |
| `weather_levels` | `spawn_points` | 2 |
| `weather_levels` | `won_game` | 1 |
| `weather_levels` | `world_edit` | 1 |

## Канонический порядок (предложение)

`live` -> `sleepers` -> `player_saves` -> `hub.players` -> `sim.villagers` -> `sim.mobs_loaded` -> `sim.mobs` -> `dim_state.edited` -> `dim_state.chunk_frames` -> `dim_state.dirty` -> `world_edit`

### Нарушения порядка (берут более ранний лок, держа более поздний)

Нарушений нет.


## Циклы

Циклов в наблюдаемых рёбрах нет.


## По файлам


### `crates/net/src/play.rs` (423)

- `chunk_frame`: `dim_state.chunk_frames.lock()` → `?`
- `chunk_frame`: `dim_state.edited.read()` → `read`
- `chunk_frame`: `dim_state.edited.read()` → `world`
- `chunk_frame`: `dim_state.chunk_frames.lock()` → `cache`
- `materialize_void`: `dim_state.edited.read()` → `?`
- `materialize_void`: `dim_state.edited.write()` → `?`
- `store_pois`: `dim_state.pois.lock()` → `map`
- `note_poi`: `dim_state.pois.lock()` → `map`
- `locate_poi`: `dim_state.pois.lock()` → `map`
- `set_block`: `dim_state.edited.read()` → `?`
- `set_block`: `dim_state.edited.write()` → `world`
- `set_block`: `dim_state.dirty.lock()` → `?`
- `set_block`: `sim.beacons.lock()` → `?`
- `set_block`: `sim.pending_beacons.lock()` → `?`
- `set_block`: `sim.conduits.lock()` → `?`
- `set_block`: `sim.pending_conduits.lock()` → `?`
- `set_block`: `sim.sculk_sections.lock()` → `?`
- `set_block`: `dim_state.chunk_frames.lock()` → `?`
- `set_block`: `dim_state.chunk_frames.lock()` → `frames`
- `set_block`: `pending_blocks.lock()` → `?`
- `after_block_change`: `sim.redstone_ticks.lock()` → `?`
- `spawn_end_crystal`: `dragon_fight.lock()` → `fight`
- `try_respawn_dragon`: `dragon_fight.lock()` → `fight`
- `dragon_crystal_burst`: `particle_types.lock()` → `?`
- `tick_dragon_fight`: `dragon_fight.lock()` → `fight`
- `tick_dragon_fight`: `particle_types.lock()` → `?`
- `trigger_summoned_entity`: `hub.players.lock()` → `?`
- `trigger_summoned_entity`: `live.lock()` → `live`
- `set_blocks`: `dim_state.edited.read()` → `?`
- `set_blocks`: `dim_state.edited.write()` → `world`
- `set_blocks`: `dim_state.dirty.lock()` → `dirty`
- `set_blocks`: `dim_state.chunk_frames.lock()` → `frames`
- `set_blocks`: `dim_state.dirty.lock()` → `?`
- `set_blocks`: `pending_blocks.lock()` → `pending`
- `comparator_output`: `sim.comparators.lock()` → `?`
- `redstone_output`: `sim.open_blocks.lock()` → `viewers`
- `analog_at`: `sim.sculk_frequency.lock()` → `?`
- `set_commands`: `plugin_commands.lock()` → `?`
- `set_commands`: `hub.players.lock()` → `online`
- `set_particle_types`: `particle_types.lock()` → `?`
- `set_command_lang`: `command_lang.lock()` → `?`
- `command_text`: `command_lang.lock()` → `lang`
- `permissions_changed`: `hub.players.lock()` → `entity`
- `set_op`: `hub.players.lock()` → `name`
- `present_player`: `hub.players.lock()` → `?`
- `present_player`: `live.lock()` → `?`
- `player_state_shared`: `live.lock()` → `live`
- `player_state_shared`: `state_cache.lock()` → `?`
- `player_state_shared`: `state_cache.lock()` → `cache`
- `state_version`: `live.lock()` → `live`
- `player_control`: `controls.lock()` → `?`
- `player_control`: `live.lock()` → `?`
- `world_request`: `state.simulated.lock()` → `keys`
- `world_request`: `state.chunk_frames.lock()` → `?`
- `world_request`: `bukkit_values.lock()` → `?`
- `world_request`: `player_saves.lock()` → `?`
- `world_request`: `ticks.lock()` → `ticks`
- `world_request`: `sim.deferred_updates.lock()` → `?`
- `set_sneaking`: `live.lock()` → `?`
- `set_sneaking`: `state_cache.lock()` → `?`
- `publish_live`: `live.lock()` → `live_map`
- `online_by_name`: `hub.players.lock()` → `?`
- `push_control`: `live.lock()` → `?`
- `push_control`: `controls.lock()` → `?`
- `regions_loaded`: `dim_state.regions.lock()` → `?`
- `tick_weather_levels`: `weather_levels.lock()` → `levels`
- `mark_entity_changed`: `sim.entity_new.lock()` → `?`
- `ensure_region`: `s.regions.lock()` → `loaded`
- `ensure_region`: `s.evicted.lock()` → `?`
- `ensure_region`: `?.lock()` → `_guard`
- `ensure_region`: `state.regions.lock()` → `?`
- `ensure_region`: `state.evicted.lock()` → `?`
- `install_region`: `dim_state.edited.write()` → `world`
- `install_region`: `dim_state.chunk_frames.lock()` → `cache`
- `install_region`: `sim.populated.lock()` → `?`
- `install_region`: `dim_state.saved.lock()` → `saved`
- `install_region`: `sim.pending_jukeboxes.lock()` → `?`
- `install_region`: `sim.pending_campfires.lock()` → `?`
- `install_region`: `sim.pending_beacons.lock()` → `?`
- `install_region`: `sim.pending_conduits.lock()` → `?`
- `install_region`: `dim_state.edited.read()` → `world`
- `install_region`: `sim.pending_furnaces.lock()` → `?`
- `install_region`: `sim.hopper_ticks.lock()` → `queue`
- `generate_chunk`: `state.edited.read()` → `?`
- `generate_chunk`: `state.generating.lock()` → `busy`
- `drop`: `0.generating.lock()` → `?`
- `drop`: `dim_state.edited.write()` → `world`
- `drop`: `dim_state.chunk_frames.lock()` → `?`
- `drop`: `sim.fluid_ticks.lock()` → `queue`
- `spread_commit`: `dim_state.edited.read()` → `world`
- `trim_idle_generators`: `hub.players.lock()` → `occupied`
- `mem_stats`: `st.edited.read()` → `w`
- `mem_stats`: `st.chunk_frames.lock()` → `f`
- `mem_stats`: `st.pois.lock()` → `pois`
- `mem_stats`: `st.sim.mobs.lock()` → `mobs`
- `mem_stats`: `st.sim.dropped.lock()` → `dropped`
- `mem_stats`: `st.sim.populated.lock()` → `populated`
- `mem_stats`: `st.saved.lock()` → `saved`
- `mem_stats`: `st.sim.entity_written.lock()` → `entity_written`
- `mem_stats`: `st.sim.entity_new.lock()` → `entity_new`
- `unsaved_in`: `state.edited.read()` → `world`
- `unsaved_in`: `state.saved.lock()` → `saved`
- `unsaved_in`: `state.dirty.lock()` → `dirty`
- `unsaved_in`: `state.sim.jukeboxes.lock()` → `playing`
- `pending_player_saves`: `player_saves.lock()` → `?`
- `mark_player_saved`: `player_saves.lock()` → `pending`
- `evict_session`: `this.controls.lock()` → `online`
- `evict_session`: `this.hub.players.lock()` → `?`
- `evict_session`: `controls.lock()` → `?`
- `disconnect_all`: `controls.lock()` → `?`
- `snapshot`: `seen_credits.lock()` → `?`
- `snapshot`: `bukkit_values.lock()` → `?`
- `snapshot`: `hub.players.lock()` → `dim`
- `snapshot`: `spawn_points.lock()` → `?`
- `snapshot`: `player_saves.lock()` → `?`
- `mark_saved_in`: `state.edited.read()` → `world`
- `mark_saved_in`: `state.saved.lock()` → `saved`
- `mark_saved_in`: `state.dirty.lock()` → `dirty`
- `wait_lent`: `state.lent.lock()` → `?`
- `wait_lent`: `light_lock.lock()` → `?`
- `block_in`: `state.edited.read()` → `columns`
- `active_dim`: `active.lock()` → `?`
- `sleep_player`: `sleepers.lock()` → `sleepers`
- `sleep_player`: `hub.players.lock()` → `?`
- `wake_player`: `sleepers.lock()` → `head`
- `wake_player`: `sleepers.lock()` → `?`
- `wake_player`: `hub.players.lock()` → `?`
- `wake_bed_state`: `sleepers.lock()` → `uuids`
- `tick_sleep`: `sleepers.lock()` → `sleepers`
- `tick_sleep`: `hub.players.lock()` → `total`
- `bed_spawn`: `hub.players.lock()` → `players`
- `respawn_position`: `dim_state.edited.read()` → `columns`
- `refresh_simulated`: `hub.players.lock()` → `players`
- `refresh_simulated`: `sim.forced.lock()` → `forced`
- `refresh_simulated`: `sim.simulated_key.lock()` → `last`
- `refresh_simulated`: `dim_state.simulated.lock()` → `?`
- `ticking_check`: `dim_state.edited.read()` → `world`
- `take_ticking`: `queue.lock()` → `list`
- `simulated_set`: `dim_state.simulated.lock()` → `?`
- `simulated_chunks`: `dim_state.simulated.lock()` → `?`
- `set_simulated`: `dim_state.simulated.lock()` → `?`
- `set_simulated`: `sim.simulated_key.lock()` → `?`
- `light_ring`: `light_lock.lock()` → `_light`
- `light_ring`: `dim_state.edited.write()` → `world`
- `light_ring`: `state.lent.lock()` → `?`
- `commit_light`: `dim_state.edited.write()` → `world`
- `commit_light`: `dim_state.chunk_frames.lock()` → `frames`
- `commit_light`: `dim_state.dirty.lock()` → `dirty`
- `build_chunk`: `light_lock.lock()` → `_light`
- `run`: `player_saves.lock()` → `pending`
- `run`: `seen_credits.lock()` → `?`
- `run`: `bukkit_values.lock()` → `?`
- `run`: `spawn_points.lock()` → `?`
- `run`: `active.lock()` → `?`
- `run`: `hub.players.lock()` → `players`
- `run`: `controls.lock()` → `?`
- `run`: `controls.lock()` → `_`
- `run`: `live.lock()` → `?`
- `run`: `client_loading.lock()` → `?`
- `run`: `mob_hits.lock()` → `?`
- `run`: `hub.players.lock()` → `left_in`
- `run`: `hub.players.lock()` → `?`
- `spawn_frame`: `spawn_frames.lock()` → `?`
- `spawn_frame`: `spawn_frames.lock()` → `cache`
- `remove_frame`: `remove_frames.lock()` → `cache`
- `drop_item`: `sim.dropped.lock()` → `?`
- `spawn_orbs`: `sim.orbs.lock()` → `orbs`
- `spawn_orbs`: `sim.orbs.lock()` → `?`
- `summon_orb`: `sim.orbs.lock()` → `?`
- `track_orbs`: `sim.orbs.lock()` → `?`
- `orb_spawn`: `sim.orbs.lock()` → `orbs`
- `tick_orbs`: `sim.orbs.lock()` → `orbs`
- `tick_orbs`: `hub.players.lock()` → `?`
- `orb_near`: `sim.orbs.lock()` → `?`
- `take_orb`: `sim.orbs.lock()` → `orbs`
- `track_drops`: `sim.dropped.lock()` → `?`
- `drop_spawn`: `sim.dropped.lock()` → `dropped`
- `index_drop`: `sim.dropped_by_chunk.lock()` → `?`
- `rebuild_drop_index`: `sim.dropped.lock()` → `dropped`
- `rebuild_drop_index`: `sim.dropped_by_chunk.lock()` → `index`
- `drops_near`: `sim.dropped_by_chunk.lock()` → `index`
- `drops_near`: `sim.dropped.lock()` → `dropped`
- `arrow_candidates`: `sim.arrows.lock()` → `arrows`
- `pickup_arrows`: `sim.arrows.lock()` → `?`
- `pickup`: `sim.dropped.lock()` → `dropped`
- `tick_items`: `sim.dropped.lock()` → `dropped`
- `tick_items`: `sim.items_syncs.lock()` → `syncs`
- `tick_items`: `sim.items_gone.lock()` → `gone`
- `tick_items`: `sim.items_merging.lock()` → `merging`
- `tick_arrows`: `sim.arrows.lock()` → `?`
- `tick_arrows`: `hub.players.lock()` → `?`
- `tick_arrows`: `sim.mobs.lock()` → `mob_targets`
- `tick_arrows`: `sim.arrows.lock()` → `arrows`
- `tick_arrows`: `hub.players.lock()` → `players`
- `tick_arrows`: `sim.mobs.lock()` → `mobs`
- `tick_arrows`: `live.lock()` → `live`
- `splash_potion`: `hub.players.lock()` → `players`
- `throw_ender_eye`: `sim.eyes.lock()` → `?`
- `summon_eye`: `sim.eyes.lock()` → `?`
- `tick_eyes`: `sim.eyes.lock()` → `eyes`
- `boost_rocket`: `sim.arrows.lock()` → `?`
- `summon_arrow`: `sim.arrows.lock()` → `?`
- `throw_trident`: `hub.players.lock()` → `up`
- `throw_trident`: `hub.players.lock()` → `?`
- `summon_trident`: `sim.arrows.lock()` → `?`
- `launch`: `sim.arrows.lock()` → `?`
- `scan_portals`: `hub.players.lock()` → `?`
- `portal_at`: `portal_time.lock()` → `times`
- `portal_at`: `portal_cooldown.lock()` → `cd`
- `portal_at`: `portal_cooldown.lock()` → `?`
- `end_travel`: `seen_credits.lock()` → `?`
- `end_travel`: `won_game.lock()` → `?`
- `end_travel`: `spawn_points.lock()` → `?`
- `closest_portal`: `pois.lock()` → `map`
- `command_teleport`: `hub.players.lock()` → `here`
- `switch_player_dimension_turned`: `hub.players.lock()` → `look`
- `switch_player_dimension_look`: `hub.players.lock()` → `?`
- `switch_player_dimension_look`: `active.lock()` → `?`
- `track_nether_travel`: `nether_entry.lock()` → `entries`
- `channel_lightning`: `sim.mobs.lock()` → `?`
- `channel_lightning`: `sim.villagers.lock()` → `?`
- `channel_lightning`: `hub.players.lock()` → `?`
- `tick_random_blocks`: `dim_state.edited.read()` → `world`
- `tick_precipitation`: `dim_state.edited.read()` → `world`
- `thunder_strike`: `sim.lightning.lock()` → `?`
- `thunder_strike`: `hub.players.lock()` → `players`
- `tick_lightning`: `sim.lightning.lock()` → `flashes`
- `biome`: `plan.dim_state.edited.read()` → `world`
- `summon_tnt`: `sim.tnt.lock()` → `?`
- `spawn_primed_tnt`: `sim.tnt.lock()` → `?`
- `tick_tnt`: `sim.tnt.lock()` → `?`
- `tick_tnt`: `sim.tnt.lock()` → `entities`
- `explode_full`: `particle_types.lock()` → `?`
- `damage_tnt_targets`: `hub.players.lock()` → `players`
- `damage_tnt_targets`: `sim.mobs.lock()` → `mobs`
- `plugin_inv_request`: `plugin_invs.lock()` → `?`
- `plugin_inv_request`: `plugin_invs.lock()` → `invs`
- `plugin_inv_request`: `controls.lock()` → `?`
- `player_dim`: `hub.players.lock()` → `?`
- `player_at`: `hub.players.lock()` → `?`
- `moved`: `hub.players.lock()` → `players`
- `ticking_dims`: `hub.players.lock()` → `occupied`
- `ticking_dims`: `state.sim.forced.lock()` → `dims`
- `run_dim_phases`: `world_edit.lock()` → `_edit`
- `record_tick`: `ticks.lock()` → `ticks`
- `record_tick`: `debug_profile.lock()` → `?`
- `record_tick`: `profile_sink.lock()` → `?`
- `recompute_work`: `hub.players.lock()` → `busy`
- `busy`: `forced.lock()` → `?`
- `busy`: `mobs.lock()` → `?`
- `busy`: `minecarts.lock()` → `?`
- `busy`: `mobs_loaded.lock()` → `?`
- `busy`: `dropped.lock()` → `?`
- `busy`: `arrows.lock()` → `?`
- `busy`: `clouds.lock()` → `?`
- `busy`: `eyes.lock()` → `?`
- `busy`: `tnt.lock()` → `?`
- `busy`: `lightning.lock()` → `?`
- `busy`: `orbs.lock()` → `?`
- `busy`: `furnaces.lock()` → `?`
- `busy`: `pending_furnaces.lock()` → `?`
- `busy`: `campfires.lock()` → `?`
- `busy`: `pending_campfires.lock()` → `?`
- `busy`: `jukeboxes.lock()` → `?`
- `busy`: `pending_jukeboxes.lock()` → `?`
- `busy`: `crafters.lock()` → `?`
- `busy`: `hopper_ticks.lock()` → `?`
- `busy`: `redstone_ticks.lock()` → `?`
- `busy`: `fluid_ticks.lock()` → `?`
- `busy`: `deferred_updates.lock()` → `?`
- `busy`: `piston_moves.lock()` → `?`
- `busy`: `plates.lock()` → `?`
- `busy`: `torch_toggles.lock()` → `?`
- `spawn_tick`: `plan.ticks.lock()` → `ticks`
- `flush_blocks`: `pending_blocks.lock()` → `pending`
- `flush_moves`: `hub.players.lock()` → `players`
- `refresh_visible`: `hub.players.lock()` → `all`
- `session`: `weather_levels.lock()` → `levels`
- `session`: `hub.players.lock()` → `tab`
- `session`: `plugin_invs.lock()` → `menu`
- `session`: `hub.players.lock()` → `mode`
- `session`: `hub.players.lock()` → `sleeping`
- `session`: `hub.players.lock()` → `?`
- `session`: `hub.players.lock()` → `players`
- `session`: `won_game.lock()` → `?`
- `session`: `spawn_points.lock()` → `?`
- `session`: `sleepers.lock()` → `occupied`
- `session`: `world_edit.lock()` → `edit`
- `tick_player_triggers`: `hub.players.lock()` → `?`
- `tick_player_triggers`: `live.lock()` → `live`
- `clone_entity_item`: `sim.mobs.lock()` → `mobs`
- `clone_entity_item`: `sim.boats.lock()` → `boats`
- `mine`: `hub.players.lock()` → `entity`
- `break_block`: `live.lock()` → `held_shears`
- `break_block`: `world_edit.lock()` → `edit`
- `break_block`: `sim.comparators.lock()` → `?`
- `break_block`: `sim.hopper_ticks.lock()` → `?`
- `break_block`: `sim.furnaces.lock()` → `removed`
- `break_block`: `sim.campfires.lock()` → `?`
- `window_valid`: `plugin_invs.lock()` → `?`
- `mark_gliding`: `hub.players.lock()` → `?`
- `mark_burning`: `hub.players.lock()` → `?`
- `refresh_glide`: `hub.players.lock()` → `ground`
- `refresh_glide`: `hub.players.lock()` → `burning`
- `container_viewers`: `sim.open_blocks.lock()` → `map`
- `release_viewer`: `sim.open_blocks.lock()` → `?`
- `close_window`: `sim.furnaces.lock()` → `?`
- `window_slots`: `sim.furnaces.lock()` → `?`
- `set_furnace_slots`: `controls.lock()` → `controls`
- `save_brewing_slots`: `controls.lock()` → `?`
- `tick_brewing`: `dim_state.edited.read()` → `world`
- `tick_brewing`: `controls.lock()` → `?`
- `tick_redstone`: `sim.deferred_updates.lock()` → `deferred`
- `tick_redstone`: `sim.deferred_updates.lock()` → `?`
- `tick_redstone`: `sim.redstone_ticks.lock()` → `?`
- `enqueue_tick`: `sim.redstone_ticks.lock()` → `?`
- `scan_tripwires`: `hub.players.lock()` → `players`
- `scan_tripwires`: `sim.mobs.lock()` → `?`
- `scan_tripwires`: `sim.dropped.lock()` → `?`
- `scan_tripwires`: `live.lock()` → `live`
- `scan_plates`: `hub.players.lock()` → `?`
- `scan_plates`: `sim.mobs.lock()` → `?`
- `scan_plates`: `sim.dropped.lock()` → `?`
- `scan_plates`: `sim.plates.lock()` → `?`
- `update_plate`: `sim.plates.lock()` → `count`
- `eyeblossom_tick`: `particle_types.lock()` → `?`
- `scheduled_tick`: `sim.comparators.lock()` → `?`
- `scheduled_tick`: `sim.torch_toggles.lock()` → `toggles`
- `scheduled_tick`: `sim.torch_cool.lock()` → `cools`
- `redstone_update`: `sim.hopper_ticks.lock()` → `?`
- `check_piston`: `sim.piston_moves.lock()` → `front_moving`
- `check_piston`: `sim.piston_moves.lock()` → `?`
- `claim`: `sim.piston_moves.lock()` → `?`
- `land_now`: `sim.deferred_updates.lock()` → `?`
- `land_moves`: `sim.piston_moves.lock()` → `moves`
- `land_moves`: `sim.redstone_ticks.lock()` → `?`
- `cont_put`: `controls.lock()` → `controls`
- `tick_hoppers`: `sim.hopper_ticks.lock()` → `?`
- `hopper_step`: `sim.hopper_ticks.lock()` → `?`
- `hopper_suck`: `sim.dropped.lock()` → `dropped`
- `dispense_special`: `live.lock()` → `live`
- `dispense_special`: `hub.players.lock()` → `?`
- `wake_fluids`: `sim.fluid_ticks.lock()` → `?`
- `tick_fluids`: `sim.fluid_ticks.lock()` → `?`
- `campfire_room`: `sim.campfires.lock()` → `?`
- `campfire_place`: `sim.campfires.lock()` → `snap`
- `tick_campfires`: `sim.pending_campfires.lock()` → `pending`
- `tick_campfires`: `sim.campfires.lock()` → `?`
- `tick_campfire`: `sim.campfires.lock()` → `gone`
- `tick_campfire`: `sim.campfires.lock()` → `map`
- `tick_campfire`: `sim.campfires.lock()` → `snap`
- `tick_campfire`: `sim.campfires.lock()` → `?`
- `tick_furnaces`: `sim.pending_furnaces.lock()` → `pending`
- `tick_furnaces`: `sim.furnaces.lock()` → `map`
- `tick_furnaces`: `controls.lock()` → `controls`
- `save_window`: `controls.lock()` → `controls`
- `activate_sculk`: `sim.sculk_frequency.lock()` → `?`
- `tripwire_pressed`: `live.lock()` → `live`
- `tripwire_pressed`: `hub.players.lock()` → `?`
- `tripwire_pressed`: `sim.mobs.lock()` → `mob`
- `tripwire_pressed`: `sim.dropped.lock()` → `?`
- `notify_lectern`: `controls.lock()` → `?`
- `tick_copper_chests`: `dim_state.edited.read()` → `world`
- `block_entity`: `dim_state.edited.read()` → `world`
- `set_block_entity`: `dim_state.edited.read()` → `?`
- `set_block_entity`: `dim_state.edited.write()` → `world`
- `set_block_entity`: `dim_state.dirty.lock()` → `?`
- `set_block_entity`: `dim_state.chunk_frames.lock()` → `?`
- `container_items`: `plugin_invs.lock()` → `?`
- `save_container`: `plugin_invs.lock()` → `?`
- `set_game_mode`: `hub.players.lock()` → `?`
- `sync_effect_particles`: `particle_types.lock()` → `?`
- `live_motion_modifiers`: `live.lock()` → `?`
- `attribute_fallback`: `hub.players.lock()` → `?`
- `set_client_loading`: `client_loading.lock()` → `?`
- `client_loaded`: `client_loading.lock()` → `?`
- `respawn`: `spawn_points.lock()` → `?`
- `respawn`: `hub.players.lock()` → `?`
- `respawn`: `live.lock()` → `?`
- `respawn`: `active.lock()` → `?`
- `within_pick_range`: `slot.lock()` → `?`

### `crates/net/src/play/mobs.rs` (101)

- `set_mob_killer`: `sim.mobs.lock()` → `?`
- `riptide_spin`: `sim.mobs.lock()` → `mobs`
- `mob_interact_info`: `sim.mobs.lock()` → `mobs`
- `mob_packets`: `particle_types.lock()` → `?`
- `spawn_mob_final`: `sim.mobs.lock()` → `?`
- `track_mobs`: `sim.mobs.lock()` → `?`
- `mob_spawn`: `sim.mobs.lock()` → `mobs`
- `tick_mobs`: `sim.mobs_loaded.lock()` → `loaded`
- `tick_mobs`: `sim.mobs.lock()` → `?`
- `tick_mobs`: `live.lock()` → `live`
- `tick_mobs`: `hub.players.lock()` → `players`
- `tick_mobs`: `sim.villagers.lock()` → `villagers`
- `tick_mobs`: `sim.mobs.lock()` → `mobs`
- `tick_mobs`: `particle_types.lock()` → `?`
- `tick_mobs`: `hub.players.lock()` → `entity`
- `tick_mobs`: `dragon_fight.lock()` → `?`
- `tick_witch_heals`: `raids.lock()` → `?`
- `tick_witch_heals`: `sim.mobs.lock()` → `mobs`
- `mob_loot`: `live.lock()` → `killer_pos`
- `mob_loot`: `live.lock()` → `live`
- `melee_hit`: `hub.players.lock()` → `?`
- `mace_smash`: `sim.mobs.lock()` → `mobs`
- `stab_targets`: `sim.mobs.lock()` → `mobs`
- `piercing_attack`: `sim.mobs.lock()` → `mobs`
- `kinetic_strike`: `sim.mobs.lock()` → `mobs`
- `player_entity`: `hub.players.lock()` → `?`
- `attack_player`: `hub.players.lock()` → `players`
- `attack_player`: `hub.players.lock()` → `on_ground`
- `attack_player`: `hub.players.lock()` → `name`
- `attack_mob`: `hub.players.lock()` → `players`
- `attack_mob`: `sim.villagers.lock()` → `villager_pos`
- `attack_mob`: `sim.mobs.lock()` → `species`
- `attack_mob`: `sim.mobs.lock()` → `mobs`
- `mob_potion_effect`: `sim.mobs.lock()` → `mobs`
- `splash_mobs`: `sim.mobs.lock()` → `mobs`
- `sweep_attack`: `hub.players.lock()` → `?`
- `sweep_attack`: `sim.mobs.lock()` → `mobs`
- `sweep_attack`: `hub.players.lock()` → `players`
- `sweep_attack`: `particle_types.lock()` → `?`
- `interact_mob`: `sim.mobs.lock()` → `species`
- `interact_mob`: `sim.mobs.lock()` → `mobs`
- `interact_mob`: `love_cause.lock()` → `?`
- `dispense_shear`: `sim.mobs.lock()` → `mobs`
- `refresh_mob`: `sim.mobs.lock()` → `mobs`
- `tick_animals`: `sim.mobs.lock()` → `mobs`
- `break_mob_door`: `world_edit.lock()` → `_edit`
- `apply_mob_outs`: `sim.mobs.lock()` → `mobs`
- `apply_mob_outs`: `love_cause.lock()` → `causes`
- `apply_mob_outs`: `sim.mobs.lock()` → `pos`
- `apply_mob_outs`: `sim.mobs.lock()` → `mob`
- `apply_mob_outs`: `world_edit.lock()` → `_edit`
- `apply_mob_outs`: `sim.mobs.lock()` → `?`
- `load_mobs`: `sim.populated.lock()` → `populated`
- `load_mobs`: `sim.entity_written.lock()` → `written`
- `load_mobs`: `sim.mobs_loaded.lock()` → `?`
- `populate_chunk`: `sim.populated.lock()` → `?`
- `populate_chunk`: `dim_state.edited.read()` → `?`
- `populate_chunk`: `sim.entity_new.lock()` → `?`
- `chunk_biome`: `dim_state.edited.read()` → `world`
- `top_solid`: `dim_state.edited.read()` → `world`
- `unsaved_entities`: `sim.mobs.lock()` → `?`
- `unsaved_entities`: `sim.entity_written.lock()` → `?`
- `unsaved_entities`: `sim.entity_new.lock()` → `?`
- `mark_entities_saved`: `sim.entity_written.lock()` → `written`
- `mark_entities_saved`: `sim.entity_new.lock()` → `new`
- `mob_request`: `sim.mobs.lock()` → `mobs`
- `mob_request`: `sim.mobs.lock()` → `gone`
- `looking_at_entity`: `sim.mobs.lock()` → `mobs`
- `lightning_mobs`: `sim.mobs.lock()` → `mobs`
- `mob_shoot`: `sim.mobs.lock()` → `?`
- `mob_shoot`: `hub.players.lock()` → `?`
- `mob_shoot`: `sim.mobs.lock()` → `uuid`
- `mob_strike`: `mob_hits.lock()` → `hits`
- `mob_strike`: `hub.players.lock()` → `players`
- `tick_natural_spawn`: `hub.players.lock()` → `players`
- `tick_natural_spawn`: `sim.mobs.lock()` → `?`

### `crates/net/src/play/tracking.rs` (2)

- `publish_tracked`: `dim_state.tracked.lock()` → `?`
- `update_tracking`: `dim_state.tracked.lock()` → `snapshot`

### `crates/net/src/play/stream.rs` (2)

- `stream_enter`: `dim_state.edited.read()` → `world`
- `stream_enter`: `dim_state.chunk_frames.lock()` → `cache`

### `crates/net/src/mobs/mod.rs` (0)

