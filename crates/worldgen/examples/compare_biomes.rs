//! Сверка биомов с ванилью: `compare_biomes <данные> <seed> <чанков_в_сторону>`.
//!
//! Пишет `biomes.points` — по строке на клетку 4×4×4: шесть значений климата
//! как hex-биты f32 в порядке `Climate.TargetPoint` (на них запускается
//! `ChunkwakeBiomes decide`), и `biomes.got` — имя биома, выбранное нашим
//! кодом: `cx cz секция клетка имя`.

use chunkwake_worldgen::Generator;
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::Path::new(&a[1]);
    let seed: i64 = a[2].parse().unwrap();
    let chunks: i32 = a.get(3).map_or(4, |v| v.parse().unwrap());
    let g = Generator::load(data, seed).unwrap();
    if !g.has_biomes() {
        eprintln!("no reports/biomes.csv: run the server once to rebuild the data cache");
        std::process::exit(1);
    }
    let mut points_out = std::io::BufWriter::new(std::fs::File::create("biomes.points").unwrap());
    let mut got_out = std::io::BufWriter::new(std::fs::File::create("biomes.got").unwrap());
    for cx in -chunks..chunks {
        for cz in -chunks..chunks {
            let Some(points) = g.climate(cx, cz) else {
                eprintln!("no climate for {cx} {cz}");
                std::process::exit(1);
            };
            for (i, point) in points.iter().enumerate() {
                for v in point {
                    write!(points_out, "{:x} ", v.to_bits()).unwrap();
                }
                writeln!(points_out).unwrap();
                writeln!(
                    got_out,
                    "{cx} {cz} {} {} {}",
                    i / 64,
                    i % 64,
                    g.biome_name(point).unwrap_or("?")
                )
                .unwrap();
            }
        }
    }
    println!("{} chunks", (2 * chunks) * (2 * chunks));
}
