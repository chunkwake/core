//! Быстрый срез рельефа Нижнего мира: сколько твёрдого по столбцу.
//!
//! `nether_probe <данные> <seed>` — печатает для чанка (0,0) высоты первой
//! твёрдой клетки сверху и снизу по нескольким столбцам.

use chunkwake_worldgen::Fill;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::path::PathBuf::from(&a[1]);
    let seed: i64 = a[2].parse().expect("seed");
    let settings = a.get(3).map(String::as_str).unwrap_or("minecraft:nether");
    let g = chunkwake_worldgen::Generator::load_settings(&data, seed, None, settings).unwrap();
    println!(
        "settings {settings}: min_y {} height {} sea {}",
        g.min_y, g.height, g.sea_level
    );
    let fills = g.fill(0, 0);
    let h = g.height as usize;
    let mut solid = 0;
    let mut air = 0;
    for f in &fills {
        match f {
            Fill::Solid => solid += 1,
            Fill::Air => air += 1,
            _ => {}
        }
    }
    println!(
        "чанк (0,0): твёрдых {solid}, воздуха {air}, всего {}",
        fills.len()
    );
    for (x, z) in [(0usize, 0usize), (8, 8), (15, 15)] {
        let mut top = None;
        let mut bottom = None;
        for y in (0..h).rev() {
            if fills[(y * 16 + z) * 16 + x] != Fill::Air {
                if top.is_none() {
                    top = Some(y);
                }
                bottom = Some(y);
            }
        }
        println!("  x{x} z{z}: верх {top:?}, низ {bottom:?}");
    }
}
