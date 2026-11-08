//! Weather properties from the biome data pack.

use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BiomeWeather {
    pub temperature: f64,
    pub has_precipitation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

impl BiomeWeather {
    pub fn at(self, y: i32) -> Precipitation {
        if !self.has_precipitation {
            return Precipitation::None;
        }
        // Above Y=80 biome temperature falls by approximately 0.00125 per block.
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
        };
        let plains = BiomeWeather {
            temperature: 0.8,
            has_precipitation: true,
        };
        let mountains = BiomeWeather {
            temperature: 0.2,
            has_precipitation: true,
        };
        assert_eq!(desert.at(64), Precipitation::None);
        assert_eq!(plains.at(64), Precipitation::Rain);
        assert_eq!(mountains.at(64), Precipitation::Rain);
        assert_eq!(mountains.at(128), Precipitation::Snow);
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
