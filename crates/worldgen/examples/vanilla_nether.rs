//! Стенд Нижнего мира: генерация шумом `minecraft:nether` и побайтовая сверка
//! с регионами ванильного `server.jar`.
//!
//! `vanilla_nether <данные> <каталог> <seed> <радиус>`
//!
//! Каталог — мир, созданный ванилью (`dimensions/minecraft/the_nether/region`).
//! `CHUNKWAKE_ORIGIN=cx,cz` и `CHUNKWAKE_NO_FEATURES=1` работают как в
//! `vanilla_regions`. `CHUNKWAKE_DIM=end` сверяет Край (`the_end`).

use chunkwake_world::ChunkColumn;
use chunkwake_world::anvil::{self, Format, Names, RegionStore};
use chunkwake_world::nbt::Compound;
use chunkwake_worldgen::Generator;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 {
        eprintln!("usage: vanilla_nether <data> <work> <seed> <radius>");
        std::process::exit(2);
    }
    let data = PathBuf::from(&a[1]);
    let dir = PathBuf::from(&a[2]);
    let seed: i64 = a[3].parse().expect("seed");
    let radius: i32 = a[4].parse().expect("radius");
    let regions = dir.join(format!("world/dimensions/minecraft/{}/region", dim().0));
    let regions = if regions.is_dir() {
        regions
    } else {
        // Старый формат: `DIM-1/region`.
        let legacy = dir.join("world/DIM-1/region");
        if legacy.is_dir() { legacy } else { regions }
    };
    if let Err(e) = compare(&regions, &data, seed, radius) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// Каталог измерения, настройки шума, блок и жидкость по умолчанию.
fn dim() -> (&'static str, &'static str, &'static str, &'static str) {
    if std::env::var("CHUNKWAKE_DIM").as_deref() == Ok("end") {
        (
            "the_end",
            "minecraft:end",
            "minecraft:end_stone",
            "minecraft:air",
        )
    } else {
        (
            "the_nether",
            "minecraft:nether",
            "minecraft:netherrack",
            "minecraft:lava",
        )
    }
}

fn origin() -> (i32, i32) {
    let Ok(raw) = std::env::var("CHUNKWAKE_ORIGIN") else {
        return (0, 0);
    };
    let Some((x, z)) = raw.split_once(',') else {
        panic!("CHUNKWAKE_ORIGIN must be cx,cz");
    };
    (
        x.trim().parse().expect("CHUNKWAKE_ORIGIN cx"),
        z.trim().parse().expect("CHUNKWAKE_ORIGIN cz"),
    )
}

fn chunk_square(radius: i32) -> Vec<(i32, i32)> {
    let (ox, oz) = origin();
    let mut out = (ox - radius..ox + radius)
        .flat_map(|cx| (oz - radius..oz + radius).map(move |cz| (cx, cz)))
        .collect::<Vec<_>>();
    // `CHUNKWAKE_REVERSE=1`: обратный порядок запросов — проверка, сколько
    // расхождений даёт сам порядок декорации соседей.
    if std::env::var_os("CHUNKWAKE_REVERSE").is_some() {
        out.reverse();
    }
    // `CHUNKWAKE_ORDER=z`: z снаружи; `ring`: от центра кольцами.
    match std::env::var("CHUNKWAKE_ORDER").as_deref() {
        Ok("z") => out.sort_by_key(|&(x, z)| (z, x)),
        Ok("ring") => out.sort_by_key(|&(x, z)| ((x - ox).abs().max((z - oz).abs()), x, z)),
        _ => {}
    }
    out
}

fn version(data: &Path) -> Result<i32, String> {
    let text = std::fs::read_to_string(data.join("version.json"))
        .map_err(|e| format!("version.json: {e}"))?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    json["world_version"]
        .as_i64()
        .map(|v| v as i32)
        .ok_or_else(|| "no world_version".to_owned())
}

fn compare(regions: &Path, data: &Path, seed: i64, radius: i32) -> Result<(), String> {
    let registries =
        chunkwake_data::registries::RegistryData::load(data).map_err(|e| e.to_string())?;
    let biomes: HashMap<String, u32> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| {
            r.entries
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), i as u32))
                .collect()
        })
        .unwrap_or_default();
    let mut noise = Generator::load_settings(data, seed, None, dim().1)?;
    noise.bind_biomes(biomes.clone());
    let (min_y, height) = (noise.min_y, noise.height);
    println!(
        "нижний мир: min_y {min_y}, height {height}, море {}",
        noise.sea_level
    );
    let blocks = chunkwake_data::world::Blocks::load(data).map_err(|e| e.to_string())?;
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let world = chunkwake_worldgen::pipeline::Overworld::new_for(
        std::sync::Arc::new(noise),
        &state,
        &biomes,
        dim().2,
        dim().3,
    )?;
    let world = if std::env::var_os("CHUNKWAKE_NO_FEATURES").is_some() {
        world
    } else {
        world.with_features(data, &blocks.states(), &state, &biomes)?
    };
    if let Some((done, all)) = world.feature_coverage() {
        println!("фичи: перенесено {done} из {all}");
    }
    if let Ok(names) = std::env::var("CHUNKWAKE_LOCATE") {
        // `имя` или `имя@x:z` — откуда искать.
        for item in names.split(',') {
            let (name, at) = item.split_once('@').unwrap_or((item, "0:0"));
            let (x, z) = at.split_once(':').unwrap_or(("0", "0"));
            let origin = [x.parse().unwrap_or(0), 64, z.parse().unwrap_or(0)];
            println!(
                "{name} от {origin:?}: {:?}",
                world.locate_structure(name, origin)
            );
        }
    }
    let names = FileNames {
        blocks: &blocks,
        biomes: &biomes,
    };
    // `CHUNKWAKE_VS=<каталог регионов>`: вместо нашей генерации — другой
    // прогон ванили, чтобы мерить её собственный разброс.
    let other = std::env::var("CHUNKWAKE_VS").ok().map(RegionStore::new);
    let store = RegionStore::new(regions);
    let mut chunks: HashMap<(i32, i32), Compound> = HashMap::new();
    let mut seen = HashSet::new();
    for (cx, cz) in chunk_square(radius) {
        let key = (cx >> 5, cz >> 5);
        if seen.insert(key) {
            for (pos, root) in store.load_region(key.0, key.1)? {
                chunks.insert(pos, root);
            }
        }
    }
    if std::env::var_os("CHUNKWAKE_STARTS").is_some() {
        let mut list: Vec<_> = chunks.iter().collect();
        list.sort_by_key(|(k, _)| **k);
        for ((cx, cz), root) in list {
            let Some(starts) = root
                .get("structures")
                .and_then(|t| t.as_compound())
                .and_then(|c| c.get("starts"))
                .and_then(|t| t.as_compound())
            else {
                continue;
            };
            if let Some(refs) = root
                .get("structures")
                .and_then(|t| t.as_compound())
                .and_then(|c| c.get("References"))
                .and_then(|t| t.as_compound())
            {
                for (name, list) in refs {
                    let at: Vec<(i32, i32)> = list
                        .as_long_array()
                        .unwrap_or(&[])
                        .iter()
                        .map(|v| (*v as i32, (*v >> 32) as i32))
                        .collect();
                    if !at.is_empty() && (cx.abs() < 3 && cz.abs() < 3) {
                        println!("ссылка {name} из ({cx},{cz}) на {at:?}");
                    }
                }
            }
            for (name, start) in starts {
                let pieces = start
                    .as_compound()
                    .and_then(|c| c.get("Children"))
                    .and_then(|t| t.as_list())
                    .unwrap_or(&[]);
                if start
                    .as_compound()
                    .and_then(|c| c.get("id"))
                    .and_then(|t| t.as_str())
                    == Some("INVALID")
                {
                    continue;
                }
                let first = pieces
                    .first()
                    .and_then(|p| p.as_compound())
                    .and_then(|p| p.get("BB"))
                    .and_then(|t| t.as_int_array())
                    .map(|b| b.to_vec());
                println!(
                    "старт {name} в ({cx},{cz}): кусков {}, первый {:?}",
                    pieces.len(),
                    first
                );
                if std::env::var("CHUNKWAKE_PIECES").ok() == Some(format!("{cx},{cz}")) {
                    for (i, p) in pieces.iter().enumerate() {
                        let p = p.as_compound();
                        let bb = p
                            .and_then(|p| p.get("BB"))
                            .and_then(|t| t.as_int_array())
                            .map(|b| b.to_vec());
                        let rot = p
                            .and_then(|p| p.get("rotation"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("");
                        let el = p
                            .and_then(|p| p.get("pool_element"))
                            .and_then(|t| t.as_compound())
                            .and_then(|e| e.get("location"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("");
                        println!("ванильный кусок {i}: {bb:?} {rot} {el}");
                    }
                }
            }
        }
    }
    let format = Format {
        data_version: version(data)?,
        min_section: min_y / 16,
    };
    let empty = vec![0u32; height as usize * 256];
    let template = ChunkColumn::from_blocks(&empty, 0, 15, 7, false);
    let props_on = std::env::var_os("CHUNKWAKE_PROPS").is_some();
    let short = |id: u32| {
        names.block_name(id).map_or("?".to_owned(), |(n, props)| {
            let base = n.trim_start_matches("minecraft:");
            if !props_on || props.is_empty() {
                return base.to_owned();
            }
            let text: Vec<_> = props.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("{base}[{}]", text.join(","))
        })
    };
    let (mut checked, mut bad, mut flows) = (0, 0usize, 0usize);
    let mut got: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    let mut pairs: HashMap<(String, String), usize> = HashMap::new();
    let mut biome_pairs: HashMap<(String, String), usize> = HashMap::new();
    for (cx, cz) in chunk_square(radius) {
        let Some(nbt) = chunks.get(&(cx, cz)) else {
            continue;
        };
        let (want, _) = anvil::chunk_from_nbt(nbt, &template, format, &names)?;
        let Some(mut ours) = world.chunk(cx, cz) else {
            continue;
        };
        if let Some(other) = &other {
            let Some(nbt) = other
                .load_region(cx >> 5, cz >> 5)?
                .into_iter()
                .find(|(p, _)| *p == (cx, cz))
                .map(|(_, r)| r)
            else {
                continue;
            };
            let (col, _) = anvil::chunk_from_nbt(&nbt, &template, format, &names)?;
            for y in 0..height as usize {
                for z in 0..16 {
                    for x in 0..16 {
                        ours.states[(y * 16 + z) * 16 + x] = col.block(x, y, z);
                    }
                }
            }
        }
        got.insert((cx, cz), ours.states.clone());
        checked += 1;
        // Биомы по клеткам 4×4×4.
        if let Some(cells) = &ours.biomes {
            for (i, &b) in cells.iter().enumerate() {
                let (qy, qz, qx) = (i / 16, (i / 4) % 4, i % 4);
                let Some(sec) = want.sections.get(qy / 4) else {
                    continue;
                };
                let v = sec.biome_at(qx, qy % 4, qz);
                if v != b {
                    let n = |id| BiomeNames(&biomes).name(id);
                    *biome_pairs.entry((n(b), n(v))).or_insert(0) += 1;
                    if std::env::var_os("CHUNKWAKE_BIOME_WHERE").is_some() {
                        println!("    биом {cx},{cz} q{qx},{qy},{qz}: {} | {}", n(b), n(v));
                    }
                }
            }
        }
        for y in 0..height as usize {
            for z in 0..16 {
                for x in 0..16 {
                    let a = short(ours.states[(y * 16 + z) * 16 + x]);
                    let b = short(want.block(x, y, z));
                    // Текущая жидкость — тики живого сервера, не генерация.
                    let flowing =
                        names
                            .block_name(want.block(x, y, z))
                            .is_some_and(|(_, props)| {
                                props.iter().any(|(k, v)| k == "level" && v != "0")
                            });
                    if a != b && flowing && a == "air" {
                        flows += 1;
                        continue;
                    }
                    // `NyliumBlock.randomTick`: под непрозрачным блоком нилий
                    // живого сервера становится незерраком.
                    if a.ends_with("_nylium") && b == "netherrack" && y + 1 < height as usize {
                        let above = short(want.block(x, y + 1, z));
                        if above != "air" && above != "cave_air" {
                            flows += 1;
                            continue;
                        }
                    }
                    if a != b {
                        bad += 1;
                        if let Ok(w) = std::env::var("CHUNKWAKE_WHERE")
                            && w == format!("{a},{b}")
                        {
                            let wy = y as i32 + min_y;
                            println!("    {} {wy} {}", cx * 16 + x as i32, cz * 16 + z as i32);
                        }
                        *pairs.entry((a, b)).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    // `CHUNKWAKE_DUMP=x,y0,y1,z0,z1[,x1]`: срезы по x, наше | ваниль.
    if let Ok(raw) = std::env::var("CHUNKWAKE_DUMP") {
        let v: Vec<i32> = raw.split(',').map(|t| t.trim().parse().unwrap()).collect();
        let (x0, y0, y1, z0, z1) = (v[0], v[1], v[2], v[3], v[4]);
        let x1 = v.get(5).copied().unwrap_or(x0);
        let glyph = |name: String| match name.as_str() {
            "netherrack" => 'N',
            "magma_block" => 'M',
            "air" => '.',
            "cave_air" => ',',
            "lava" => 'L',
            "obsidian" => 'O',
            "crying_obsidian" => 'C',
            "gravel" => 'G',
            "blackstone" => 'B',
            "red_mushroom" => 'r',
            "brown_mushroom" => 'u',
            "glowstone" => 'g',
            "basalt" => 'Z',
            "nether_quartz_ore" => 'q',
            "nether_gold_ore" => 'o',
            "nether_bricks" => '#',
            "crimson_nylium" => 'n',
            "warped_nylium" => 'w',
            "warped_wart_block" => 'X',
            "warped_stem" => 'T',
            "warped_roots" => 'r',
            "warped_fungus" => 'f',
            "nether_sprouts" => 's',
            "twisting_vines" | "twisting_vines_plant" => 't',
            "crimson_roots" => 'R',
            "crimson_stem" => 'S',
            "nether_wart_block" => 'W',
            "shroomlight" => 'l',
            "crimson_fungus" => 'F',
            "weeping_vines" | "weeping_vines_plant" => 'v',
            n if n.contains("blackstone") => 'b',
            n => {
                eprintln!("? = {n}");
                '?'
            }
        };
        for x in x0..=x1 {
            println!("x={x}");
            for y in (y0..=y1).rev() {
                let mut a = String::new();
                let mut b = String::new();
                for z in z0..=z1 {
                    let (cx, cz) = (x.div_euclid(16), z.div_euclid(16));
                    let (lx, lz) = (x.rem_euclid(16) as usize, z.rem_euclid(16) as usize);
                    let ry = (y - min_y) as usize;
                    let ours = got.get(&(cx, cz));
                    a.push(ours.map_or(' ', |c| glyph(short(c[(ry * 16 + lz) * 16 + lx]))));
                    let theirs = chunks
                        .get(&(cx, cz))
                        .and_then(|nbt| anvil::chunk_from_nbt(nbt, &template, format, &names).ok());
                    b.push(theirs.map_or(' ', |(c, _)| glyph(short(c.block(lx, ry, lz)))));
                }
                println!("{y:>4} {a} | {b}");
            }
        }
    }
    let mut top: Vec<_> = pairs.into_iter().collect();
    top.sort_by_key(|e| std::cmp::Reverse(e.1));
    let grep = std::env::var("CHUNKWAKE_GREP").ok();
    let shown = top.iter().filter(|((a, b), _)| {
        grep.as_deref()
            .is_none_or(|g| a.contains(g) || b.contains(g))
    });
    for ((a, b), n) in shown.take(if grep.is_some() { usize::MAX } else { 30 }) {
        println!("  {n:>6}  наше {a:<22} ваниль {b}");
    }
    println!("тики сервера (жидкости, нилий) {flows}");
    println!("блоков расходится {bad} в {checked} чанках");
    let mut bp: Vec<_> = biome_pairs.into_iter().collect();
    bp.sort_by(|a, b| b.1.cmp(&a.1));
    for ((a, b), n) in bp.iter().take(10) {
        println!("    {n:6}  биом наш {a:22} ваниль {b}");
    }
    println!(
        "клеток биомов расходится {}",
        bp.iter().map(|p| p.1).sum::<usize>()
    );
    Ok(())
}

struct FileNames<'a> {
    blocks: &'a chunkwake_data::world::Blocks,
    biomes: &'a HashMap<String, u32>,
}

impl Names for FileNames<'_> {
    fn block_name(&self, id: u32) -> Option<(&str, &[(String, String)])> {
        self.blocks.state(id)
    }
    fn block_id(&self, name: &str, props: &[(String, String)]) -> Option<u32> {
        self.blocks.state_id(name, props)
    }
    fn block_default(&self, name: &str) -> Option<u32> {
        self.blocks.default_state(name).ok().map(|s| s as u32)
    }
    fn biome_name(&self, id: u32) -> Option<&str> {
        self.biomes
            .iter()
            .find(|(_, v)| **v == id)
            .map(|(k, _)| k.as_str())
    }
    fn biome_id(&self, name: &str) -> Option<u32> {
        self.biomes.get(name).copied()
    }
}

struct BiomeNames<'a>(&'a HashMap<String, u32>);

impl BiomeNames<'_> {
    fn name(&self, id: u32) -> String {
        self.0
            .iter()
            .find(|(_, v)| **v == id)
            .map_or("?".into(), |(k, _)| {
                k.trim_start_matches("minecraft:").to_owned()
            })
    }
}
