//! Чат-версия снимка: цветной текстовый компонент для `/top`.
//!
//! В чате нет ANSI и нельзя положиться на ширину шрифта, поэтому каждая
//! строка — отдельный элемент `extra`, полоски рисуются `█`/`░` фиксированной
//! длины, а подробности уходят в `hoverEvent` — так экран не забивается.

use crate::render::{Section, bar, load_color, tps_color};
use crate::snapshot::Snapshot;
use serde_json::{Value, json};

/// Строка компонента: цветной текст.
#[allow(dead_code)]
fn line(color: &str, text: impl Into<String>) -> Value {
    json!({"text": text.into(), "color": color})
}

fn push(extra: &mut Vec<Value>, value: Value) {
    extra.push(value);
}

/// Заголовок секции.
fn heading(extra: &mut Vec<Value>, name: &str) {
    push(
        extra,
        json!({"text": format!("\n{name} "), "color": "dark_aqua", "bold": true}),
    );
}

/// Рисует снимок как компонент. `None`-секция означает «всё».
pub fn render(snapshot: &Snapshot, section: Section) -> Value {
    let mut extra: Vec<Value> = Vec::new();
    let title = format!(" {} · {} ", snapshot.motd, snapshot.version);
    push(
        &mut extra,
        json!({"text": title, "color": "aqua", "bold": true}),
    );
    push(
        &mut extra,
        json!({"text": format!("up {}\n", snapshot.uptime_text()), "color": "dark_gray"}),
    );

    if matches!(section, Section::All | Section::Tick) {
        heading(&mut extra, "TICK");
        push(&mut extra, Value::String("\n".into()));
        let t = &snapshot.tick;
        push(
            &mut extra,
            json!({
                "text": format!("TPS {:5.1} / {:5.1} / {:5.1} / {:5.1}",
                    t.tps_now, t.tps_1m, t.tps_5m, t.tps_15m),
                "color": tps_color(t.tps_1m),
                "hover_event": hover(&format!(
                    "now {:.2}\n1m {:.2}\n5m {:.2}\n15m {:.2}",
                    t.tps_now, t.tps_1m, t.tps_5m, t.tps_15m
                )),
            }),
        );
        push(
            &mut extra,
            json!({"text": "  now 1m 5m 15m", "color": "dark_gray"}),
        );
        push(&mut extra, Value::String("\n".into()));
        push(
            &mut extra,
            json!({
                "text": format!("MSPT {:5.2}  p50 {:5.2}  p99 {:5.2}  max {:5.2}",
                    t.mspt_1m, t.mspt_p50, t.mspt_p99, t.mspt_worst),
                "color": if t.mspt_1m < snapshot.limits.target_mspt * 0.5 {
                    "green"
                } else if t.mspt_1m < snapshot.limits.target_mspt {
                    "yellow"
                } else {
                    "red"
                },
                "hover_event": hover(&format!(
                    "last {:.2} ms\np50 {:.2}\np95 {:.2}\np99 {:.2}\nworst {:.2}\nlate {} ticks",
                    t.mspt_now, t.mspt_p50, t.mspt_p95, t.mspt_p99, t.mspt_worst, t.late
                )),
            }),
        );
        push(
            &mut extra,
            json!({"text": "  avg p50 p99 worst", "color": "dark_gray"}),
        );
    }

    if matches!(section, Section::All | Section::Cpu) {
        heading(&mut extra, "CPU");
        push(&mut extra, Value::String("\n".into()));
        let percent = f64::from(snapshot.proc.cpu_total);
        push(
            &mut extra,
            json!({
                "text": format!("{} system {:4.0}%", bar(percent, 100.0, 20), percent),
                "color": load_color(percent),
                "hover_event": hover(&format!(
                    "{} · {} cores · {} MHz\nserver {:.1}% of one core",
                    snapshot.proc.cpu_brand,
                    snapshot.proc.cores.len(),
                    snapshot.proc.cpu_mhz,
                    snapshot.proc.process_cpu
                )),
            }),
        );
        // По одной строке на восемь ядер: в чате длинная строка переносится.
        for chunk in snapshot.proc.cores.chunks(8) {
            let mut text = String::new();
            for core in chunk {
                text.push_str(&format!("{:>3.0} ", core.usage));
            }
            push(&mut extra, Value::String("\n".into()));
            push(&mut extra, json!({"text": text, "color": "gray"}));
        }
    }

    if matches!(section, Section::All | Section::Memory) {
        heading(&mut extra, "MEMORY");
        push(&mut extra, Value::String("\n".into()));
        push(
            &mut extra,
            json!({
                "text": format!("rss {:.1} MiB", snapshot.rss_mib()),
                "color": "aqua",
                "hover_event": hover(&format!(
                    "virtual {:.1} MiB\nthreads {}\nhost {:.2} / {:.2} GiB",
                    snapshot.proc.vsz as f64 / (1024.0 * 1024.0),
                    snapshot.proc.threads,
                    snapshot.memory_used_gib(),
                    snapshot.memory_total_gib()
                )),
            }),
        );
        let used = snapshot.memory_used_gib();
        let total = snapshot.memory_total_gib();
        let percent = if total > 0.0 {
            used / total * 100.0
        } else {
            0.0
        };
        push(
            &mut extra,
            json!({
                "text": format!("  host {}%", percent.round()),
                "color": load_color(percent),
                "hover_event": hover(&format!("{used:.2} / {total:.2} GiB")),
            }),
        );
    }

    if matches!(section, Section::All | Section::World) {
        heading(&mut extra, "WORLD");
        push(&mut extra, Value::String("\n".into()));
        let w = &snapshot.world;
        push(
            &mut extra,
            json!({
                "text": format!("players {}/{}  mobs {}  items {}",
                    w.players, w.max_players, w.mobs, w.dropped_items),
                "color": "white",
                "hover_event": hover(&format!(
                    "arrows {}\norbs {}\nsimulated chunks {} of {}",
                    w.arrows, w.orbs, w.simulated_chunks, w.loaded_chunks
                )),
            }),
        );
        push(
            &mut extra,
            json!({
                "text": format!("  chunks {}", w.simulated_chunks),
                "color": "gray",
                "hover_event": hover(&format!(
                    "redstone {}\nfluid {}\nhopper {}\nfurnace {}\ncampfire {}\ngen {}/{}",
                    w.redstone_queue, w.fluid_queue, w.hopper_queue,
                    w.furnaces, w.campfires, w.gen_pending, w.gen_workers
                )),
            }),
        );
    }

    if matches!(section, Section::All | Section::Net) {
        heading(&mut extra, "NET");
        push(&mut extra, Value::String("\n".into()));
        let net = &snapshot.net;
        let ping = match net.ping_avg {
            Some(avg) => format!("ping {avg:.0} ms"),
            None => "ping —".into(),
        };
        push(
            &mut extra,
            json!({
                "text": format!("{ping}"),
                "color": "white",
                "hover_event": hover(&format!(
                    "in {:.1} MiB / {} packets\nout {:.1} MiB / {} packets\nmax ping {:.0} ms",
                    net.bytes_in as f64 / (1024.0 * 1024.0),
                    net.packets_in,
                    net.bytes_out as f64 / (1024.0 * 1024.0),
                    net.packets_out,
                    net.ping_max.unwrap_or(0.0)
                )),
            }),
        );
    }

    json!({"text": "", "extra": extra})
}

/// Подсказка при наведении.
fn hover(text: &str) -> Value {
    json!({"action": "show_text", "contents": {"text": text, "color": "gray"}, "value": {"text": text, "color": "gray"}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Snapshot {
        let mut s = Snapshot::default();
        s.motd = "Test".into();
        s.version = "26.3".into();
        s.uptime = 90;
        s.tick.tps_1m = 19.9;
        s.tick.mspt_1m = 2.4;
        s.limits.target_mspt = 50.0;
        s.proc.cores = (0..12)
            .map(|i| crate::proc::CoreLoad {
                usage: i as f32 * 5.0,
            })
            .collect();
        s.proc.cpu_total = 27.5;
        s.proc.rss = 256 * 1024 * 1024;
        s.proc.total_memory = 16 * 1024 * 1024 * 1024;
        s.proc.used_memory = 4 * 1024 * 1024 * 1024;
        s.world.players = 1;
        s.world.simulated_chunks = 169;
        s.net.ping_avg = Some(18.0);
        s
    }

    #[test]
    fn component_has_sections_and_flattened_text() {
        let value = render(&sample(), Section::All);
        let flat = chunkwake_flat(&value);
        assert!(flat.contains("Test · 26.3"), "{flat}");
        assert!(flat.contains("TICK"));
        assert!(flat.contains("CPU"));
        assert!(flat.contains("MEMORY"));
        assert!(flat.contains("WORLD"));
        assert!(flat.contains("NET"));
        assert!(flat.contains("TPS"));
        assert!(flat.contains("rss 256.0 MiB"));
    }

    #[test]
    fn single_section_hides_others() {
        let flat = chunkwake_flat(&render(&sample(), Section::Tick));
        assert!(flat.contains("TICK"));
        assert!(!flat.contains("MEMORY"));
    }

    #[test]
    fn twelve_cores_split_into_two_lines() {
        let value = render(&sample(), Section::Cpu);
        let cores: Vec<&Value> = value["extra"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| {
                v["color"] == "gray"
                    && v["text"]
                        .as_str()
                        .is_some_and(|t| t.trim_end().split_whitespace().count() == 8)
            })
            .collect();
        assert_eq!(cores.len(), 1);
    }

    /// Склейка `text` и `extra` как её прочитает клиент.
    fn chunkwake_flat(value: &Value) -> String {
        let mut out = String::new();
        if let Some(text) = value["text"].as_str() {
            out.push_str(text);
        }
        if let Some(extra) = value["extra"].as_array() {
            for item in extra {
                out.push_str(&chunkwake_flat(item));
            }
        }
        out
    }
}
