//! Сверка с эталоном ванили: `compare <generated-dir> <seed> <ref.txt>`, где ref —
//! вывод харнесса RefGen (строка `cx cz` + символы A/S/W/L на каждый блок).

use chunkwake_worldgen::{Fill, Generator};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = Instant::now();
    let g = Generator::load(a[1].as_ref(), a[2].parse().unwrap()).unwrap();
    println!("load {:?}", t.elapsed());
    let text = std::fs::read_to_string(&a[3]).unwrap();
    let (mut chunks, mut bad_chunks, mut bad_blocks) = (0, 0, 0);
    let t = Instant::now();
    for line in text.lines() {
        let mut it = line.split(' ');
        let cx: i32 = it.next().unwrap().parse().unwrap();
        let cz: i32 = it.next().unwrap().parse().unwrap();
        let want = it.next().unwrap().as_bytes();
        let got = g.fill(cx, cz);
        chunks += 1;
        let mut first = None;
        let mut bad = 0;
        for (i, f) in got.iter().enumerate() {
            let c = match f {
                Fill::Air => b'A',
                Fill::Solid => b'S',
                Fill::Water => b'W',
                Fill::Lava => b'L',
            };
            if c != want[i] {
                bad += 1;
                first.get_or_insert((i, c, want[i]));
            }
        }
        if bad > 0 {
            bad_chunks += 1;
            bad_blocks += bad;
            let (i, c, w) = first.unwrap();
            let (x, z, y) = (i % 16, i / 16 % 16, i / 256);
            println!(
                "chunk {cx} {cz}: {bad} blocks differ, first at x={x} y={} z={z}: got {} want {}",
                y as i32 + g.min_y,
                c as char,
                w as char
            );
        }
    }
    println!(
        "{chunks} chunks in {:?}: {bad_chunks} differ, {bad_blocks} blocks",
        t.elapsed()
    );
}
