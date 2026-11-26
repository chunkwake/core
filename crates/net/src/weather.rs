//! Weather properties from the biome data pack.

use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BiomeWeather {
    pub temperature: f64,
    pub has_precipitation: bool,
    /// `temperature_modifier: frozen` — `Biome.TemperatureModifier.FROZEN`.
    pub frozen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

impl BiomeWeather {
    /// `Biome.getHeightAdjustedTemperature` в точке: высотный шум и
    /// `temperature_modifier: frozen` из 26.3.
    pub fn temperature_at(self, pos: [i32; 3], sea_level: i32) -> f32 {
        chunkwake_worldgen::feature::height_adjusted_temperature(
            self.temperature as f32,
            self.frozen,
            pos,
            sea_level,
        )
    }

    /// `Biome.coldEnoughToSnow` в точке.
    pub fn cold_enough_to_snow(self, pos: [i32; 3], sea_level: i32) -> bool {
        let t = self.temperature_at(pos, sea_level);
        // Как `!(t >= 0.15)`: NaN тоже считается снегом.
        !matches!(
            t.partial_cmp(&0.15),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        )
    }

    /// `Biome.getPrecipitationAt(pos, seaLevel)`.
    pub fn precipitation_at(self, pos: [i32; 3], sea_level: i32) -> Precipitation {
        if !self.has_precipitation {
            return Precipitation::None;
        }
        if self.cold_enough_to_snow(pos, sea_level) {
            Precipitation::Snow
        } else {
            Precipitation::Rain
        }
    }

    /// Приблизительный вариант по высоте без координат (оставлен для
    /// старых тестов; в тике погоды используется [`Self::precipitation_at`]).
    pub fn at(self, y: i32) -> Precipitation {
        if !self.has_precipitation {
            return Precipitation::None;
        }
        let temperature = self.temperature - (y - 80).max(0) as f64 * 0.00125;
        if temperature < 0.15 {
            Precipitation::Snow
        } else {
            Precipitation::Rain
        }
    }
}

pub fn load_biomes(data_dir: &Path, names: &[String]) -> Vec<BiomeWeather> {
    names
        .iter()
        .map(|name| {
            let path = name.strip_prefix("minecraft:").unwrap_or(name);
            let file = data_dir
                .join("data/minecraft/worldgen/biome")
                .join(format!("{path}.json"));
            let Ok(bytes) = std::fs::read(file) else {
                return BiomeWeather::default();
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return BiomeWeather::default();
            };
            BiomeWeather {
                temperature: value["temperature"].as_f64().unwrap_or(0.5),
                has_precipitation: value["has_precipitation"].as_bool().unwrap_or(false),
                frozen: value["temperature_modifier"].as_str() == Some("frozen"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biome_precipitation_rules() {
        let desert = BiomeWeather {
            temperature: 2.0,
            has_precipitation: false,
            frozen: false,
        };
        let plains = BiomeWeather {
            temperature: 0.8,
            has_precipitation: true,
            frozen: false,
        };
        let mountains = BiomeWeather {
            temperature: 0.2,
            has_precipitation: true,
            frozen: false,
        };
        assert_eq!(desert.at(64), Precipitation::None);
        assert_eq!(plains.at(64), Precipitation::Rain);
        assert_eq!(mountains.at(64), Precipitation::Rain);
        assert_eq!(mountains.at(128), Precipitation::Snow);
    }

    #[test]
    fn precipitation_uses_position_and_height() {
        let plains = BiomeWeather {
            temperature: 0.8,
            has_precipitation: true,
            frozen: false,
        };
        let cold = BiomeWeather {
            temperature: 0.1,
            has_precipitation: true,
            frozen: false,
        };
        assert_eq!(plains.precipitation_at([0, 64, 0], 64), Precipitation::Rain);
        assert_eq!(cold.precipitation_at([0, 64, 0], 64), Precipitation::Snow);
        assert_eq!(
            BiomeWeather {
                has_precipitation: false,
                ..plains
            }
            .precipitation_at([0, 64, 0], 64),
            Precipitation::None
        );
    }

    #[test]
    fn loads_26_3_biomes() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let names = [
            "minecraft:desert",
            "minecraft:plains",
            "minecraft:snowy_plains",
        ]
        .map(str::to_owned);
        let biomes = load_biomes(&data, &names);
        assert_eq!(biomes[0].at(64), Precipitation::None);
        assert_eq!(biomes[1].at(64), Precipitation::Rain);
        assert_eq!(biomes[2].at(64), Precipitation::Snow);
    }
}
