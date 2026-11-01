//! ANSI-таблица для терминала и консоли сервера.
//!
//! Один снимок без интерактива: `/top` в консоли, `chunkwake top` из CLI и
//! периодическая печать в лог. Цвета — обычные SGR; `--no-color` даёт
//! чистый текст для логов.

use crate::render::{Section, bar, load_color, tps_color};
use crate::snapshot::Snapshot;
use std::fmt::Write;

/// Цвета ANSI.
struct Paint {
    enabled: bool,
}

impl Paint {
    fn wrap(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    fn named(&self, name: &str, text: &str) -> String {
        let code = match name {
            "green" => "32",
            "yellow" => "33",
            "red" => "31",
            "gray" => "90",
            "white" => "97",
            "cyan" => "36",
            _ => "0",
        };
        self.wrap(code, text)
    }
}

/// Рисует снимок; `section` сужает вывод.
pub fn render(snapshot: &Snapshot, section: Section, color: bool) -> String {
    let paint = Paint { enabled: color };
    let mut out = String::with_capacity(2048);
    let title = format!(
        " {} · {} · up {} ",
        snapshot.motd,
        snapshot.version,
        snapshot.uptime_text()
    );
    let _ = writeln!(out, "{}", paint.wrap("1;36", &format!("{:=^72}", title)));

    if matches!(section, Section::All | Section::Tick) {
        tick_section(&mut out, snapshot, &paint);
    }
    if matches!(section, Section::All | Section::Cpu) {
        cpu_section(&mut out, snapshot, &paint);
    }
    if matches!(section, Section::All | Section::Memory) {
        memory_section(&mut out, snapshot, &paint);
    }
    if matches!(section, Section::All | Section::World) {
        world_section(&mut out, snapshot, &paint);
    }
    if matches!(section, Section::All | Section::Net) {
        net_section(&mut out, snapshot, &paint);
    }
    out
}

/// Профиль тика для терминала: понятные группы, от тяжёлой к лёгкой.
///
/// Показываем группы, а не 18 внутренних фаз: «хопперы и механизмы»
/// объясняет админу, что его тормозит, а «отложенные правки плагинов» —
/// нет. Точные фазы остаются в `/api/profile` и в подробном режиме дашборда.
///
/// Профиль приходит отдельно от снимка (его снимает тик, а не сэмплер),
/// поэтому функция принимает уже готовую последнюю секунду.
pub fn render_profile(bucket: &crate::profile::ProfileBucket, color: bool) -> String {
    let paint = Paint { enabled: color };
    let mut out = String::with_capacity(1024);
    heading(&mut out, &paint, "PROFILE");
    let total = bucket.total_per_tick();
    let mut shown = 0;
    for group in bucket.groups_by_cost() {
        let ms = bucket.group_per_tick(group);
        if ms <= 0.005 {
            continue;
        }
        let entry = &crate::profile::GROUPS[group];
        let share = if total > 0.0 { ms / total * 100.0 } else { 0.0 };
        let _ = writeln!(
            out,
            "  {:<22} {:>6.2} ms/тик {:>4.0}%  {}",
            entry.label,
            ms,
            share,
            paint.named(
                load_color(f64::from(share)),
                &bar(f64::from(share), 100.0, 18)
            )
        );
        shown += 1;
    }
    if shown == 0 {
        let _ = writeln!(out, "  тик почти пустой: подсистемы ничего не делают");
    }
    let _ = writeln!(
        out,
        "  {:<22} {:>6.2} ms/тик   ({} тиков в секунде)",
        "итого", total, bucket.ticks
    );
    out
}

fn heading(out: &mut String, paint: &Paint, name: &str) {
    let _ = writeln!(out, "{}", paint.wrap("1;36", &format!("── {name} ")));
}

fn tps_paint(paint: &Paint, tps: f64) -> String {
    paint.named(tps_color(tps), &format!("{tps:5.1}"))
}

fn tick_section(out: &mut String, s: &Snapshot, paint: &Paint) {
    let tick = &s.tick;
    heading(out, paint, "TICK");
    let state = if tick.frozen {
        paint.named("yellow", "frozen")
    } else if tick.sprint {
        paint.named("cyan", "sprinting")
    } else {
        paint.named("green", "running")
    };
    let _ = writeln!(
        out,
        "  rate  {:>6.0}/s  {}   ticks {}",
        tick.rate, state, tick.total
    );
    let _ = writeln!(
        out,
        "  TPS   now {}  1m {}  5m {}  15m {}",
        tps_paint(paint, tick.tps_now),
        tps_paint(paint, tick.tps_1m),
        tps_paint(paint, tick.tps_5m),
        tps_paint(paint, tick.tps_15m)
    );
    let _ = writeln!(
        out,
        "  MSPT  last {:6.2}  1m avg {:6.2}  p50 {:6.2}  p95 {:6.2}  p99 {:6.2}  max {:6.2}",
        tick.mspt_now, tick.mspt_1m, tick.mspt_p50, tick.mspt_p95, tick.mspt_p99, tick.mspt_worst
    );
    let budget = s.limits.target_mspt;
    let _ = writeln!(
        out,
        "  budget {:>5.1} ms  {}  {}%",
        budget,
        paint.named(
            if tick.mspt_1m < budget * 0.5 {
                "green"
            } else if tick.mspt_1m < budget {
                "yellow"
            } else {
                "red"
            },
            &bar(tick.mspt_1m, budget, 40)
        ),
        (tick.mspt_1m / budget * 100.0).round()
    );
    let _ = writeln!(
        out,
        "  late  {} ticks since start",
        paint.named(
            if tick.late == 0 { "green" } else { "yellow" },
            &tick.late.to_string()
        )
    );
}

fn cpu_section(out: &mut String, s: &Snapshot, paint: &Paint) {
    heading(out, paint, "CPU");
    let _ = writeln!(
        out,
        "  {} · {} cores · {} MHz",
        s.proc.cpu_brand,
        s.proc.cores.len(),
        s.proc.cpu_mhz
    );
    let _ = writeln!(
        out,
        "  system {}%  {}",
        paint.named(
            load_color(f64::from(s.proc.cpu_total)),
            &format!("{:5.1}", s.proc.cpu_total)
        ),
        paint.named(
            load_color(f64::from(s.proc.cpu_total)),
            &bar(f64::from(s.proc.cpu_total), 100.0, 40)
        )
    );
    // Загрузка процесса может быть больше 100 % — это сумма по потокам.
    let cores = s.proc.cores.len().max(1) as f32;
    let machine_share = (s.proc.process_cpu / cores).clamp(0.0, 100.0);
    let _ = writeln!(
        out,
        "  server {:>6.1}% of one core ({machine_share:.1}% of {cores} cores)  {}",
        s.proc.process_cpu,
        paint.named(
            load_color(f64::from(machine_share)),
            &bar(f64::from(machine_share), 100.0, 40)
        )
    );
    let per_row = 8;
    for chunk in s.proc.cores.chunks(per_row) {
        let mut line = String::from("  ");
        for core in chunk {
            let _ = write!(
                line,
                "{} ",
                paint.named(
                    load_color(f64::from(core.usage)),
                    &format!("{:>3.0}%", core.usage)
                )
            );
        }
        let _ = writeln!(out, "{line}");
    }
}

fn memory_section(out: &mut String, s: &Snapshot, paint: &Paint) {
    heading(out, paint, "MEMORY");
    let _ = writeln!(
        out,
        "  rss   {:>8.1} MiB   virtual {:>8.1} MiB{}",
        s.rss_mib(),
        s.proc.vsz as f64 / (1024.0 * 1024.0),
        // На Windows число потоков недоступно: не печатаем ложное «1».
        if s.proc.threads == 0 {
            String::new()
        } else {
            format!("   threads {}", s.proc.threads)
        }
    );
    let used = s.memory_used_gib();
    let total = s.memory_total_gib();
    let percent = if total > 0.0 {
        used / total * 100.0
    } else {
        0.0
    };
    let _ = writeln!(
        out,
        "  host  {:>6.2} / {:.2} GiB {}  {}%",
        used,
        total,
        paint.named(load_color(percent), &bar(used, total, 30)),
        percent.round()
    );
}

fn world_section(out: &mut String, s: &Snapshot, paint: &Paint) {
    let world = &s.world;
    heading(out, paint, "WORLD");
    let _ = writeln!(
        out,
        "  players {}/{}   mobs {}   items {}   arrows {}   orbs {}",
        world.players, world.max_players, world.mobs, world.dropped_items, world.arrows, world.orbs
    );
    let _ = writeln!(
        out,
        "  chunks  simulated {} of {}   view {} sim {}",
        world.simulated_chunks,
        world.loaded_chunks,
        s.limits.view_distance,
        s.limits.simulation_distance
    );
    let _ = writeln!(
        out,
        "  queues  redstone {}  fluid {}  hopper {}  furnace {}  campfire {}   gen {}/{}",
        world.redstone_queue,
        world.fluid_queue,
        world.hopper_queue,
        world.furnaces,
        world.campfires,
        world.gen_pending,
        world.gen_workers
    );
}

fn net_section(out: &mut String, s: &Snapshot, paint: &Paint) {
    let net = &s.net;
    heading(out, paint, "NET");
    let _ = writeln!(
        out,
        "  bytes   in {:.1} MiB  out {:.1} MiB",
        net.bytes_in as f64 / (1024.0 * 1024.0),
        net.bytes_out as f64 / (1024.0 * 1024.0)
    );
    let _ = writeln!(
        out,
        "  packets in {}  out {}",
        net.packets_in, net.packets_out
    );
    match (net.ping_avg, net.ping_max) {
        (Some(avg), Some(max)) => {
            let _ = writeln!(out, "  ping    avg {avg:.0} ms  max {max:.0} ms");
        }
        _ => {
            let _ = writeln!(out, "  ping    —");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Snapshot {
        let mut s = Snapshot {
            motd: "Test".into(),
            version: "26.3".into(),
            uptime: 3_725,
            ..Snapshot::default()
        };
        s.tick.tps_now = 20.0;
        s.tick.tps_1m = 19.9;
        s.tick.tps_5m = 19.8;
        s.tick.tps_15m = 19.7;
        s.tick.mspt_now = 2.5;
        s.tick.mspt_1m = 2.4;
        s.tick.mspt_p50 = 2.1;
        s.tick.mspt_p95 = 3.0;
        s.tick.mspt_p99 = 4.0;
        s.tick.mspt_worst = 12.0;
        s.tick.rate = 20.0;
        s.proc.cores = vec![
            crate::proc::CoreLoad { usage: 12.0 },
            crate::proc::CoreLoad { usage: 95.0 },
        ];
        s.proc.cpu_total = 53.5;
        s.proc.process_cpu = 41.0;
        s.proc.rss = 300 * 1024 * 1024;
        s.proc.total_memory = 16 * 1024 * 1024 * 1024;
        s.proc.used_memory = 8 * 1024 * 1024 * 1024;
        s.proc.cpu_brand = "Test CPU".into();
        s.world.players = 2;
        s.world.max_players = 20;
        s.world.mobs = 37;
        s.net.ping_avg = Some(21.0);
        s.net.ping_max = Some(40.0);
        s
    }

    #[test]
    fn plain_text_has_no_escape_codes() {
        let text = render(&sample(), Section::All, false);
        assert!(!text.contains('\x1b'));
        assert!(text.contains("Test · 26.3"));
        assert!(text.contains("up 01:02:05"));
        assert!(text.contains("19.9"));
        assert!(text.contains("players 2/20"));
    }

    #[test]
    fn colored_text_uses_sgr() {
        let text = render(&sample(), Section::Tick, true);
        assert!(text.contains("\x1b["));
        assert!(text.contains("TICK"));
        assert!(!text.contains("CPU"), "секция ограничена");
    }

    #[test]
    fn sections_are_independent() {
        let snapshot = sample();
        for (section, needle) in [
            (Section::Cpu, "CPU"),
            (Section::Memory, "MEMORY"),
            (Section::World, "WORLD"),
            (Section::Net, "NET"),
        ] {
            let text = render(&snapshot, section, false);
            assert!(text.contains(needle), "{needle}");
        }
    }

    #[test]
    fn tps_line_shows_all_windows() {
        let text = render(&sample(), Section::Tick, false);
        let line = text
            .lines()
            .find(|l| l.contains("TPS"))
            .expect("строка TPS");
        assert!(line.contains("now"));
        assert!(line.contains("1m"));
        assert!(line.contains("5m"));
        assert!(line.contains("15m"));
    }

    /// Профиль в терминале: группы идут от тяжёлой к лёгкой, время — на тик.
    #[test]
    fn profile_lists_heaviest_groups_first() {
        use crate::profile::{PH_CAMPFIRES, PH_MOBS, PH_ORBS, ProfileBucket};
        let mut bucket = ProfileBucket {
            at: 1,
            ticks: 10,
            ..ProfileBucket::default()
        };
        bucket.total[PH_MOBS] = 100.0; // 10 мс/тик — мобы
        bucket.total[PH_ORBS] = 40.0; // 4 мс/тик — предметы и снаряды
        bucket.total[PH_CAMPFIRES] = 10.0; // 1 мс/тик — механизмы
        let text = render_profile(&bucket, false);
        assert!(text.contains("PROFILE"));
        assert!(text.contains("10.00"), "{text}");
        // Человеческие названия групп, а не внутренние фазы.
        let mobs = text.find("Мобы и животные").expect("группа мобов");
        let items = text.find("Предметы и снаряды").expect("группа предметов");
        let machines = text.find("Хопперы и механизмы").expect("группа механизмов");
        assert!(mobs < items, "мобы дороже предметов");
        assert!(items < machines, "предметы дороже механизмов");
    }

    /// Пустой тик не печатает строк с нулями: это шум, а не данные.
    #[test]
    fn empty_profile_says_so() {
        use crate::profile::ProfileBucket;
        let bucket = ProfileBucket {
            at: 1,
            ticks: 20,
            ..ProfileBucket::default()
        };
        let text = render_profile(&bucket, false);
        assert!(text.contains("тик почти пустой"), "{text}");
        assert!(text.contains("итого"));
    }
}
