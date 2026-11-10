//! Дыры в океане в сохранённых чанках: столбцы, где на уровне моря воздух,
//! а рядом вода. Печатает столбец сверху вниз до первого твёрдого блока.
//!
//! `hole_scan <каталог region> <x> <z> <радиус в чанках>`
use chunkwake_world::anvil::RegionStore;
use chunkwake_world::nbt::{Compound, Tag};
use std::collections::HashMap;

/// Имена блоков чанка: (y секции) → 4096 имён.
fn sections(root: &Compound) -> HashMap<i32, Vec<String>> {
    let mut out = HashMap::new();
    let Some(Tag::List(list)) = root.get("sections") else {
        return out;
    };
    for s in list {
        let Tag::Compound(s) = s else { continue };
        let Some(Tag::Byte(y)) = s.get("Y") else {
            continue;
        };
        let Some(Tag::Compound(bs)) = s.get("block_states") else {
            continue;
        };
        let Some(Tag::List(palette)) = bs.get("palette") else {
            continue;
        };
        let names: Vec<String> = palette
            .iter()
            .map(|p| match p {
                Tag::String(n) => n.clone(),
                Tag::Compound(c) => match c.get("id").or(c.get("Name")) {
                    Some(Tag::String(n)) => match c.get("properties") {
                        Some(Tag::Compound(p))
                            if n.ends_with("water")
                                && matches!(p.get("level"), Some(Tag::String(l)) if l != "0") =>
                        {
                            format!("{n}_flowing")
                        }
                        _ => n.clone(),
                    },
                    _ => "?".into(),
                },
                _ => "?".into(),
            })
            .map(|n: String| {
                if n.contains(':') {
                    n
                } else {
                    format!("minecraft:{n}")
                }
            })
            .collect();
        let blocks = match bs.get("data") {
            Some(Tag::LongArray(data)) => {
                let bits = (usize::BITS - (names.len() - 1).leading_zeros()).max(4) as usize;
                let per = 64 / bits;
                (0..4096)
                    .map(|i| {
                        let word = data[i / per] as u64;
                        let idx = (word >> ((i % per) * bits)) & ((1 << bits) - 1);
                        names.get(idx as usize).cloned().unwrap_or("?".into())
                    })
                    .collect()
            }
            _ => vec![names[0].clone(); 4096],
        };
        out.insert(*y as i32, blocks);
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store = RegionStore::new(&a[1]);
    let (bx, bz): (i32, i32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let r: i32 = a[4].parse().unwrap();
    let (ccx, ccz) = (bx >> 4, bz >> 4);
    let mut chunks: HashMap<(i32, i32), HashMap<i32, Vec<String>>> = HashMap::new();
    let mut regions = std::collections::HashSet::new();
    for cx in ccx - r - 1..=ccx + r + 1 {
        for cz in ccz - r - 1..=ccz + r + 1 {
            regions.insert((cx >> 5, cz >> 5));
        }
    }
    for (rx, rz) in regions {
        for ((cx, cz), root) in store.load_region(rx, rz).unwrap_or_default() {
            chunks.insert((cx, cz), sections(&root));
        }
    }
    let block = |x: i32, y: i32, z: i32| -> Option<&str> {
        let c = chunks.get(&(x >> 4, z >> 4))?;
        let s = c.get(&(y >> 4))?;
        Some(&s[(((y & 15) * 16 + (z & 15)) * 16 + (x & 15)) as usize])
    };
    println!("{} chunk(s) loaded", chunks.len());
    let mut flowing: std::collections::BTreeMap<(i32, i32), (i32, i32, usize)> = Default::default();
    for cx in ccx - r..=ccx + r {
        for cz in ccz - r..=ccz + r {
            for x in cx * 16..cx * 16 + 16 {
                for z in cz * 16..cz * 16 + 16 {
                    for y in -64..120 {
                        if block(x, y, z).is_some_and(|b| b.ends_with("_flowing")) {
                            let e = flowing.entry((x & !7, z & !7)).or_insert((y, y, 0));
                            e.0 = e.0.min(y);
                            e.1 = e.1.max(y);
                            e.2 += 1;
                        }
                    }
                }
            }
        }
    }
    for ((x, z), (lo, hi, n)) in &flowing {
        println!("flowing water near {x} {z}: {n} block(s), y {lo}..{hi}");
    }
    for y in (40..=90).rev() {
        if let Some(b) = block(bx, y, bz)
            && b != "minecraft:air"
        {
            println!("player column {y}: {b}");
        }
    }
    let seas: Vec<i32> = std::env::var("SEA")
        .ok()
        .map(|s| s.split(',').map(|v| v.parse().unwrap()).collect())
        .unwrap_or(vec![62]);
    for sea in seas {
        let mut holes = 0;
        for cx in ccx - r..=ccx + r {
            for cz in ccz - r..=ccz + r {
                if !chunks.contains_key(&(cx, cz)) {
                    continue;
                }
                for x in cx * 16..cx * 16 + 16 {
                    for z in cz * 16..cz * 16 + 16 {
                        if block(x, sea, z) != Some("minecraft:air") {
                            continue;
                        }
                        let wet = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                            .iter()
                            .any(|(dx, dz)| block(x + dx, sea, z + dz) == Some("minecraft:water"));
                        if !wet {
                            continue;
                        }
                        holes += 1;
                        if holes <= 12 {
                            let mut col = Vec::new();
                            let mut y = sea;
                            while y > -64 {
                                let b = block(x, y, z).unwrap_or("-");
                                col.push(format!("{y}:{}", b.trim_start_matches("minecraft:")));
                                if !matches!(
                                    b,
                                    "minecraft:air" | "minecraft:water" | "minecraft:snow"
                                ) {
                                    break;
                                }
                                y -= 1;
                            }
                            // Свернуть повторы.
                            let mut short: Vec<String> = Vec::new();
                            for c in col {
                                let name = c.split(':').nth(1).unwrap().to_string();
                                if short
                                    .last()
                                    .is_some_and(|l| l.ends_with(&format!(":{name}")))
                                    && !short.last().unwrap().contains("..")
                                {
                                    let last = short.pop().unwrap();
                                    short.push(format!("{}..{c}", last.split(':').next().unwrap()));
                                } else if short
                                    .last()
                                    .is_some_and(|l| l.ends_with(&format!(":{name}")))
                                {
                                    let last = short.pop().unwrap();
                                    short
                                        .push(format!("{}..{c}", last.split("..").next().unwrap()));
                                } else {
                                    short.push(c);
                                }
                            }
                            println!("hole at {x} {z}: {}", short.join(" "));
                        }
                    }
                }
            }
        }
        println!("{holes} hole column(s) at y={sea} within {r} chunk(s) of {bx} {bz}");
    }
}
