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
    use super::sound_source;

    #[test]
    fn sound_categories_match_vanilla_ordinals() {
        assert_eq!(sound_source("players"), Some(7));
        assert_eq!(sound_source("BLOCKS"), Some(4));
        assert_eq!(sound_source("ui"), Some(10));
        assert_eq!(sound_source("unknown"), None);
    }
}
