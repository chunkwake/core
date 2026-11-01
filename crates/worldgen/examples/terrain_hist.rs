//! Срез рельефа: доля твёрдого по высоте в одном чанке.
//!
//! `terrain_hist <данные> <seed> <settings> [cx] [cz]`

use chunkwake_worldgen::Fill;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse().expect("seed");
    let settings = a[3].as_str();
    let cx: i32 = a.get(4).map_or(0, |v| v.parse().expect("cx"));
    let cz: i32 = a.get(5).map_or(0, |v| v.parse().expect("cz"));
    let g = chunkwake_worldgen::Generator::load_settings(&data, seed, None, settings).unwrap();
    let fills = g.fill(cx, cz);
    let h = g.height as usize;
    println!("{settings} чанк ({cx},{cz}) min_y {}", g.min_y);
    for y in 0..h {
        let mut solid = 0;
        let mut liquid = 0;
        for z in 0..16 {
            for x in 0..16 {
                match fills[(y * 16 + z) * 16 + x] {
                    Fill::Solid => solid += 1,
                    Fill::Air => {}
                    _ => liquid += 1,
                }
            }
        }
        if y % 8 == 0 || solid == 0 && y % 32 == 0 {
            println!(
                "  y={:<3} твёрдых {solid:>3} жидкость {liquid:>3}",
                y as i32 + g.min_y
            );
        }
    }
}
