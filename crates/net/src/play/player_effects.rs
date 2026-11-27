//! Пакеты, адресованные одному игроку (`Player.playSound`, заголовки,
//! action bar и `sendBlockChange`). Плагины зовут их через мост:
//! `player <uuid> sound|title|times|stopsound|block …`.

use super::PlayPlan;
use super::world_effects::parse_particle;
use chunkwake_protocol::play;
use uuid::Uuid;

impl PlayPlan {
    /// `None` — строка не про эффекты игрока; `Some(ответ)` — обработана.
    pub(super) fn player_effect_request(&self, line: &str) -> Option<String> {
        let rest = line.strip_prefix("player ")?;
        let (uuid, command) = rest.split_once(' ')?;
        let uuid = Uuid::parse_str(uuid).ok()?;
        if let Some(rest) = command.strip_prefix("particle ") {
            let mut tokens = rest.split_whitespace();
            let Some(request) = parse_particle(self, &mut tokens) else {
                return Some("0".into());
            };
            if tokens.next().is_some() {
                return Some("0".into());
            }
            let packet = self.particle_packet(&request);
            self.send_to(uuid, packet);
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("blocks ") {
            let Some(sections) = parse_block_changes(rest, |name| self.parse_state(name)) else {
                return Some("0".into());
            };
            let mut packets = Vec::with_capacity(sections.len());
            for (section, mut blocks) in sections {
                if blocks.len() > 1 {
                    blocks = super::section_order::vanilla_order(&blocks);
                }
                packets.push(play::section_blocks_update(
                    self.ids.section_blocks_update,
                    section,
                    &blocks,
                ));
            }
            for packet in packets {
                self.send_to(uuid, packet);
            }
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("effect ") {
            let Some((event, pos, data)) = parse_effect(rest) else {
                return Some("0".into());
            };
            self.send_to(
                uuid,
                play::level_event(self.ids.level_event, event, pos, data, false),
            );
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("sendmap ") {
            let Ok(id) = rest.trim().parse::<i32>() else {
                return Some("0".into());
            };
            return Some(if self.send_map(uuid, id) { "1" } else { "0" }.into());
        }
        if let Some(rest) = command.strip_prefix("rpackpush ") {
            let mut it = rest.split_whitespace();
            let (Some(pack), Some(url), Some(hash), Some(required), Some(prompt)) =
                (it.next(), it.next(), it.next(), it.next(), it.next())
            else {
                return Some("0".into());
            };
            if it.next().is_some() {
                return Some("0".into());
            }
            let Ok(pack) = Uuid::parse_str(pack) else {
                return Some("0".into());
            };
            let Some(url) = hex_utf8(url) else {
                return Some("0".into());
            };
            let hash = if hash == "-" { "" } else { hash };
            let required = required == "1";
            let prompt = if prompt == "-" {
                None
            } else {
                let Some(prompt) = hex_component(prompt) else {
                    return Some("0".into());
                };
                Some(prompt)
            };
            let packet = play::resource_pack_push(
                self.ids.resource_pack_push,
                pack,
                &url,
                hash,
                required,
                prompt.as_ref(),
            );
            self.send_to(uuid, packet);
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("rpackpop ") {
            let Ok(pack) = Uuid::parse_str(rest.trim()) else {
                return Some("0".into());
            };
            self.send_to(
                uuid,
                play::resource_pack_pop(self.ids.resource_pack_pop, Some(pack)),
            );
            return Some("1".into());
        }
        if command == "rpackpopall" {
            self.send_to(
                uuid,
                play::resource_pack_pop(self.ids.resource_pack_pop, None),
            );
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("hideplayer ") {
            let Ok(target) = Uuid::parse_str(rest.trim()) else {
                return Some("0".into());
            };
            let entity = self
                .hub
                .players
                .lock()
                .unwrap()
                .values()
                .find(|p| p.entry.uuid == target)
                .map(|p| p.entity_id);
            if let Some(id) = entity {
                self.send_to(uuid, play::remove_entities(self.ids.remove_entities, &[id]));
                return Some("1".into());
            }
            return Some("0".into());
        }
        if let Some(rest) = command.strip_prefix("showplayer ") {
            let Ok(target) = Uuid::parse_str(rest.trim()) else {
                return Some("0".into());
            };
            let player = self
                .hub
                .players
                .lock()
                .unwrap()
                .values()
                .find(|p| p.entry.uuid == target)
                .map(|p| (p.entity_id, p.pos, p.yaw, p.pitch));
            if let Some((id, pos, yaw, pitch)) = player {
                self.send_to(
                    uuid,
                    play::add_entity_moving(
                        self.ids.add_entity,
                        id,
                        target,
                        self.player_type,
                        pos,
                        [0.0; 3],
                        yaw,
                        pitch,
                        0,
                    ),
                );
                return Some("1".into());
            }
            return Some("0".into());
        }
        if let Some(rest) = command.strip_prefix("hideentity ") {
            let Ok(id) = rest.trim().parse::<i32>() else {
                return Some("0".into());
            };
            self.send_to(uuid, play::remove_entities(self.ids.remove_entities, &[id]));
            return Some("1".into());
        }
        if command.strip_prefix("showentity ").is_some() {
            // Не-игровые сущности пока не восстанавливаем: нужен add_entity с типом/позой.
            return Some("0".into());
        }
        if let Some(rest) = command.strip_prefix("tablist ") {
            let mut it = rest.split_whitespace();
            let (Some(header), Some(footer)) = (it.next(), it.next()) else {
                return Some("0".into());
            };
            if it.next().is_some() {
                return Some("0".into());
            }
            let (Some(header), Some(footer)) = (hex_component(header), hex_component(footer))
            else {
                return Some("0".into());
            };
            self.send_to(uuid, play::tab_list(self.ids.tab_list, &header, &footer));
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("title ") {
            let (part, json) = rest.split_once(' ')?;
            let Ok(component) = serde_json::from_str(json) else {
                return Some("0".into());
            };
            if !matches!(part, "title" | "subtitle" | "actionbar") {
                return Some("0".into());
            }
            self.send_title(uuid, part, component);
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("times ") {
            let mut it = rest.split_whitespace();
            let (Some(fade_in), Some(stay), Some(fade_out)) = (
                it.next().and_then(|v| v.parse::<i32>().ok()),
                it.next().and_then(|v| v.parse::<i32>().ok()),
                it.next().and_then(|v| v.parse::<i32>().ok()),
            ) else {
                return Some("0".into());
            };
            self.send_title_animation(uuid, fade_in, stay, fade_out);
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("cleartitle ") {
            let reset = rest.trim() == "1";
            self.send_clear_titles(uuid, reset);
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("soundent ") {
            let mut it = rest.split_whitespace();
            let (Some(entity), Some(name), Some(source)) = (it.next(), it.next(), it.next()) else {
                return Some("0".into());
            };
            let (Some(volume), Some(pitch), Some(seed)) = (
                it.next().and_then(|v| v.parse::<f32>().ok()),
                it.next().and_then(|v| v.parse::<f32>().ok()),
                it.next().and_then(|v| v.parse::<i64>().ok()),
            ) else {
                return Some("0".into());
            };
            let Ok(entity) = entity.parse::<i32>() else {
                return Some("0".into());
            };
            let Some(&sound) = self.survival.sounds.get(normalize_sound(name)) else {
                return Some("0".into());
            };
            let Some(source) = sound_source(source) else {
                return Some("0".into());
            };
            self.send_to(
                uuid,
                play::sound_entity(
                    self.ids.sound_entity,
                    sound,
                    source,
                    entity,
                    volume,
                    pitch,
                    seed,
                ),
            );
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("sound ") {
            let mut it = rest.split_whitespace();
            let (Some(name), Some(source)) = (it.next(), it.next()) else {
                return Some("0".into());
            };
            let (Some(x), Some(y), Some(z), Some(volume), Some(pitch)) = (
                it.next().and_then(|v| v.parse::<f64>().ok()),
                it.next().and_then(|v| v.parse::<f64>().ok()),
                it.next().and_then(|v| v.parse::<f64>().ok()),
                it.next().and_then(|v| v.parse::<f32>().ok()),
                it.next().and_then(|v| v.parse::<f32>().ok()),
            ) else {
                return Some("0".into());
            };
            let Some(&sound) = self.survival.sounds.get(normalize_sound(name)) else {
                return Some("0".into());
            };
            let Some(source) = sound_source(source) else {
                return Some("0".into());
            };
            self.send_to(
                uuid,
                play::sound(
                    self.ids.sound,
                    sound,
                    source,
                    [x, y, z],
                    volume,
                    pitch,
                    rand::random(),
                ),
            );
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("stopsound ") {
            let mut it = rest.split_whitespace();
            let (Some(name), Some(source)) = (it.next(), it.next()) else {
                return Some("0".into());
            };
            let sound = (name != "*")
                .then(|| normalize_sound(name))
                .and_then(|name| self.survival.sounds.get(name).copied());
            let source = (source != "*").then(|| sound_source(source)).flatten();
            self.stop_sound(uuid, sound.map(|_| name), source);
            return Some("1".into());
        }
        if let Some(rest) = command.strip_prefix("block ") {
            let mut it = rest.split_whitespace();
            let (Some(x), Some(y), Some(z), Some(state)) = (
                it.next().and_then(|v| v.parse::<i32>().ok()),
                it.next().and_then(|v| v.parse::<i32>().ok()),
                it.next().and_then(|v| v.parse::<i32>().ok()),
                it.next(),
            ) else {
                return Some("0".into());
            };
            let Some(state) = self.parse_state(state) else {
                return Some("0".into());
            };
            self.send_to(
                uuid,
                play::block_update(self.ids.block_update, [x, y, z], state as i32),
            );
            return Some("1".into());
        }
        if command == "inventory" {
            self.push_control(uuid, crate::live::Control::Inventory);
            return Some("1".into());
        }
        Some("0".into())
    }
}

/// `effect <level-event> <x> <y> <z> <data>`: `BlockPos.containing` для пакета.
fn parse_effect(rest: &str) -> Option<(i32, [i32; 3], i32)> {
    let mut it = rest.split_whitespace();
    let event = it.next()?.parse::<i32>().ok()?;
    let x = it.next()?.parse::<f64>().ok()?;
    let y = it.next()?.parse::<f64>().ok()?;
    let z = it.next()?.parse::<f64>().ok()?;
    let data = it.next()?.parse::<i32>().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((
        event,
        [x.floor() as i32, y.floor() as i32, z.floor() as i32],
        data,
    ))
}

/// Изменения блоков по секциям: `[sx, sy, sz]` → `(локальная позиция, состояние)`.
type SectionChanges = std::collections::BTreeMap<[i32; 3], Vec<(u16, u32)>>;

/// `blocks x y z state …` → секции и локальные позиции `x<<8|z<<4|y`.
/// Состояние разбирает вызывающий (реестр блоков).
fn parse_block_changes(rest: &str, state: impl Fn(&str) -> Option<u32>) -> Option<SectionChanges> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.is_empty() || !tokens.len().is_multiple_of(4) || tokens.len() / 4 > 4096 {
        return None;
    }
    let mut sections = SectionChanges::new();
    for chunk in tokens.chunks(4) {
        let x = chunk[0].parse::<i32>().ok()?;
        let y = chunk[1].parse::<i32>().ok()?;
        let z = chunk[2].parse::<i32>().ok()?;
        let id = state(chunk[3])?;
        let local = (((x & 15) << 8) | ((z & 15) << 4) | (y & 15)) as u16;
        sections
            .entry([x >> 4, y >> 4, z >> 4])
            .or_default()
            .push((local, id));
    }
    Some(sections)
}

/// UTF-8 строка из hex (URL набора ресурсов и подсказка в `rpackpush`).
fn hex_utf8(text: &str) -> Option<String> {
    if !text.len().is_multiple_of(2) || text.len() > 65536 {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().as_chunks::<2>().0 {
        let pair = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair, 16).ok()?);
    }
    String::from_utf8(bytes).ok()
}

/// JSON-компонент из hex-байтов (Java присылает UTF-8 Gson JSON).
fn hex_component(text: &str) -> Option<serde_json::Value> {
    if !text.len().is_multiple_of(2) || text.len() > 32768 {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().as_chunks::<2>().0 {
        let pair = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair, 16).ok()?);
    }
    serde_json::from_str(std::str::from_utf8(&bytes).ok()?).ok()
}

fn normalize_sound(name: &str) -> &str {
    name
}

fn sound_source(name: &str) -> Option<i32> {
    Some(match name.to_ascii_uppercase().as_str() {
        "MASTER" => 0,
        "MUSIC" => 1,
        "RECORDS" => 2,
        "WEATHER" => 3,
        "BLOCKS" => 4,
        "HOSTILE" => 5,
        "NEUTRAL" => 6,
        "PLAYERS" => 7,
        "AMBIENT" => 8,
        "VOICE" => 9,
        "UI" => 10,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{hex_utf8, parse_block_changes, parse_effect, sound_source};

    #[test]
    fn sound_categories_match_vanilla_ordinals() {
        assert_eq!(sound_source("players"), Some(7));
        assert_eq!(sound_source("BLOCKS"), Some(4));
        assert_eq!(sound_source("ui"), Some(10));
        assert_eq!(sound_source("unknown"), None);
    }

    /// `Player.playEffect`: id события, блок под координатой и data.
    #[test]
    fn effect_request_parses_and_floors_position() {
        assert_eq!(
            parse_effect("1003 1.5 64.0 -2.5 7"),
            Some((1003, [1, 64, -3], 7))
        );
        assert_eq!(
            parse_effect("2001 -0.5 64.9 -16.1 0"),
            Some((2001, [-1, 64, -17], 0))
        );
        assert_eq!(parse_effect("1003 1 2 3"), None);
        assert_eq!(parse_effect("x 1 2 3 0"), None);
        assert_eq!(parse_effect("1003 1 2 3 0 9"), None);
    }

    #[test]
    fn resource_pack_url_hex_round_trips() {
        let url = "https://example.test/pack.zip?x=1";
        let hex: String = url
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(hex_utf8(&hex).as_deref(), Some(url));
        assert_eq!(hex_utf8("abc"), None);
        assert_eq!(hex_utf8("zz"), None);
    }

    #[test]
    fn block_changes_group_by_section_and_pack_local_position() {
        let got = parse_block_changes("1 2 3 stone 2 2 3 dirt 17 2 3 stone", |name| match name {
            "stone" => Some(5),
            "dirt" => Some(6),
            _ => None,
        })
        .expect("изменения");
        assert_eq!(got.get(&[0, 0, 0]).unwrap(), &vec![(306, 5), (562, 6)]);
        assert_eq!(got.get(&[1, 0, 0]).unwrap(), &vec![(306, 5)]);
        assert!(parse_block_changes("1 2 3", |_| Some(1)).is_none());
        assert!(parse_block_changes("1 2 3 unknown", |_| None).is_none());
    }
}
