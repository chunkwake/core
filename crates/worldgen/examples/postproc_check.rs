//! Сверка жидкостей «потечёт после генерации» с ванилью: прото-чанки
//! `server.jar` в статусе `terrain` хранят `PostProcessing` — клетки, которые
//! акифер пометил `markPosForPostProcessing`. Сравниваются с нашими
//! `Blocks::fluid_posts` того же чанка без декорации. Лишних у нас быть не
//! должно; «только у ванили» — ещё и метки фич и кусков структур соседей,
//! которые в 26.3 пишут в чанк, пока он в `terrain`.
//! `CHUNKWAKE_SHOW=cx,cz` — клетки и ссылки структур одного чанка.
//!
//! `postproc_check <данные> <каталог region> <seed>`

use chunkwake_world::anvil::RegionStore;
use chunkwake_world::nbt::Tag;
use chunkwake_worldgen::Generator;
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn main() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        return Err("usage: postproc_check <data> <region dir> <seed>".into());
    }
    let data = Path::new(&a[1]);
    let store = RegionStore::new(Path::new(&a[2]));
    let seed: i64 = a[3].parse().map_err(|e| format!("seed: {e}"))?;
    let registries =
        chunkwake_data::registries::RegistryData::load(data).map_err(|e| e.to_string())?;
    let biomes: HashMap<String, u32> = registries
        .synchronized
        .iter()
        .find(|r| r.name == "worldgen/biome")
        .map(|r| r.entries.iter().enumerate().map(|(i, n)| (n.clone(), i as u32)).collect())
        .unwrap_or_default();
    let mut noise = Generator::load_with_version(data, seed, None)?;
    noise.bind_biomes(biomes.clone());
    let min_y = noise.min_y;
    let blocks = chunkwake_data::world::Blocks::load(data).map_err(|e| e.to_string())?;
    let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
    let overworld =
        chunkwake_worldgen::pipeline::Overworld::new(std::sync::Arc::new(noise), &state, &biomes)?
            .with_features(data, &blocks.states(), &state, &biomes)?;

    let files = std::fs::read_dir(Path::new(&a[2])).map_err(|e| e.to_string())?;
    let (mut chunks, mut both, mut only_v, mut only_o) = (0, 0, 0, 0);
    for f in files.flatten() {
        let name = f.file_name().to_string_lossy().into_owned();
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() != 4 || parts[0] != "r" || parts[3] != "mca" {
            continue;
        }
        let (Ok(rx), Ok(rz)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>()) else {
            continue;
        };
        for ((cx, cz), root) in store.load_region(rx, rz)? {
            let status = root.get("Status").and_then(Tag::as_str).unwrap_or("");
            if status != "minecraft:terrain" {
                continue;
            }
            let Some(lists) = root.get("PostProcessing").and_then(Tag::as_list) else {
                continue;
            };
            let mut want = HashSet::new();
            for (i, list) in lists.iter().enumerate() {
                for t in list.as_list().unwrap_or(&[]) {
                    let Tag::Short(p) = t else { continue };
                    let p = *p as i32;
                    let sy = min_y / 16 + i as i32;
                    want.insert([cx * 16 + (p & 15), sy * 16 + (p >> 4 & 15), cz * 16 + (p >> 8 & 15)]);
                }
            }
            let ours = overworld.terrain_for_test(cx, cz);
            let got: HashSet<[i32; 3]> = ours.fluid_posts.iter().copied().collect();
            chunks += 1;
            let common = got.intersection(&want).count();
            both += common;
            only_v += want.len() - common;
            only_o += got.len() - common;
            if want.len() != common || got.len() != common {
                println!(
                    "  {cx},{cz} {status}: ваниль {} | наше {} | общих {common}",
                    want.len(),
                    got.len()
                );
                if std::env::var("CHUNKWAKE_SHOW").ok() == Some(format!("{cx},{cz}")) {
                    if let Some(Tag::Compound(s)) = root.get("structures") {
                        for (k, v) in s {
                            if let Tag::Compound(m) = v {
                                println!("    {k}: {:?}", m.keys().collect::<Vec<_>>());
                            }
                        }
                    }
                    let mut extra: Vec<_> = want.difference(&got).copied().collect();
                    extra.sort_by_key(|p| (p[1], p[2], p[0]));
                    for p in extra.iter().take(40) {
                        let i = ((p[1] - min_y) as usize * 16 + (p[2] & 15) as usize) * 16
                            + (p[0] & 15) as usize;
                        println!("    {p:?} наше {}", ours.states[i]);
                    }
                }
            }
        }
    }
    println!("чанков {chunks}: общих {both}, только у ванили {only_v}, только у нас {only_o}");
    Ok(())
}
