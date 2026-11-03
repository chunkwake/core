//! Публичный фасад шаблонов структур для команд: `/place template`.
//!
//! Генератор держит разбор и постановку шаблонов внутри (`feature::template`).
//! Команде нужен только список «куда и что поставить» без процессоров,
//! маркеров и водной логики — его и отдаёт этот модуль.

use crate::feature::{self, Heightmap};
use crate::feature::{load_public, place_public, size_public};
use std::path::Path;

pub use crate::feature::Palette;

/// Куда поставить: имя блока и его свойства.
pub type BlockStates = [Option<(String, Vec<(String, String)>)>];

/// Один блок постройки: мировые координаты и состояние сервера.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub pos: [i32; 3],
    pub state: u32,
    /// У блока в шаблоне был NBT (сундук, табличка). Сами данные не переносятся.
    pub has_nbt: bool,
}

/// Поворот постройки (`Rotation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    None,
    Clockwise90,
    Clockwise180,
    CounterClockwise90,
}

impl Rotation {
    /// `none`, `clockwise_90`, `clockwise_180`, `counterclockwise_90`.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "clockwise_90" => Some(Self::Clockwise90),
            "clockwise_180" => Some(Self::Clockwise180),
            "counterclockwise_90" => Some(Self::CounterClockwise90),
            _ => None,
        }
    }
}

/// Зеркало (`Mirror`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mirror {
    None,
    LeftRight,
    FrontBack,
}

impl Mirror {
    /// `none`, `left_right`, `front_back`.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "left_right" => Some(Self::LeftRight),
            "front_back" => Some(Self::FrontBack),
            _ => None,
        }
    }
}

/// Ошибка постановки: нет файла, битый NBT, неизвестный блок.
pub struct TemplateError(pub String);

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Размер и блоки шаблона `minecraft:<path>` после поворота и зеркала.
///
/// `origin` — угол постройки в мире (он же точка `pos` из `/place template`).
/// `palette` — палитра генератора: по ней читается NBT шаблона.
pub fn template_blocks(
    data_dir: &Path,
    palette: &feature::Palette,
    id: &str,
    origin: [i32; 3],
    rotation: Rotation,
    mirror: Mirror,
    ignore_air: bool,
) -> Result<Vec<Placed>, TemplateError> {
    let template = load_public(data_dir, palette, id).map_err(TemplateError)?;
    Ok(place_public(
        palette, &template, origin, rotation, mirror, ignore_air,
    ))
}

/// Размер шаблона до поворота: `(ширина, высота, глубина)`.
pub fn template_size(
    data_dir: &Path,
    palette: &feature::Palette,
    id: &str,
) -> Result<[i32; 3], TemplateError> {
    size_public(data_dir, palette, id).map_err(TemplateError)
}

/// Уровень поверхностей для адаптера: воздух наверху, без записи в мир.
pub struct Air;

impl feature::Level for Air {
    fn min_y(&self) -> i32 {
        0
    }
    fn height(&self) -> i32 {
        0
    }
    fn get(&self, _x: i32, _y: i32, _z: i32) -> u32 {
        0
    }
    fn set(&mut self, _x: i32, _y: i32, _z: i32, _state: u32) {}
    fn can_write(&self, _x: i32, _z: i32) -> bool {
        false
    }
    fn surface(&self, _kind: Heightmap, _x: i32, _z: i32) -> i32 {
        0
    }
    fn sea_level(&self) -> i32 {
        63
    }
    fn biome(&self, _x: i32, _y: i32, _z: i32) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/place template minecraft:desert_well/well` даёт блоки, а
    /// несуществующий шаблон — пустой (`getOrCreate`).
    #[test]
    fn desert_well_has_blocks() {
        let Some(data) = std::env::var_os("CHUNKWAKE_DATA").map(std::path::PathBuf::from) else {
            return;
        };
        let blocks = chunkwake_data::world::Blocks::load(&data).unwrap();
        let states = blocks.states();
        let state = |n: &str| blocks.default_state(n).ok().map(|s| s as u32);
        let palette = feature::Palette::new(&data, &states, &state);
        let placed = template_blocks(
            &data,
            &palette,
            "minecraft:desert_well/well",
            [0, 64, 0],
            Rotation::None,
            Mirror::None,
            false,
        )
        .ok()
        .unwrap_or_default();
        assert!(!placed.is_empty());
    }
}
