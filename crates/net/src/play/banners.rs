//! Знамёна: блок-сущность (`BannerBlockEntity`) с узорами и именем из
//! предмета, обратно в предмет при поломке (`copy_components` таблицы
//! лута) и метки на картах (`MapItem.useOn` → `toggleBanner`,
//! `checkBanners` при перерисовке).

use super::*;
use crate::maps::{MapBanner, MapData};

const BANNER: &str = "minecraft:banner";

/// Цвет знамени (номер `DyeColor`) по имени блока: `*_banner`, `*_wall_banner`.
pub(super) fn banner_color(block: &str) -> Option<u8> {
    let base = block.strip_prefix("minecraft:")?;
    let color = base.strip_suffix("_wall_banner").or_else(|| base.strip_suffix("_banner"))?;
    chunkwake_protocol::components::DYE_COLORS.iter().position(|&c| c == color).map(|i| i as u8)
}

impl PlayPlan {
    /// `BannerBlockEntity` с компонентами предмета (`applyImplicitComponents`):
    /// `patterns` и `CustomName`; клиентам — `block_entity_data`
    /// (`getUpdatePacket` знамени есть всегда).
    pub(super) fn place_banner(&self, pos: [i32; 3], item: Slot) {
        // `components` пишется всегда (`saveWithoutMetadata`), пустой — тоже.
        let mut nbt = Compound::from([("components".into(), Tag::Compound(Compound::new()))]);
        if let Some(s) = item {
            let e = chunkwake_protocol::components::get(s.extra);
            if !e.banner_patterns.is_empty() {
                nbt.insert("patterns".into(), crate::stack_nbt::banner_layers_to_tag(&e.banner_patterns));
            }
            if let Some(n) = &e.custom_name {
                nbt.insert("CustomName".into(), crate::stack_nbt::json_to_tag(&crate::stack_nbt::text_json(n)));
            }
        }
        self.set_block_entity(pos, Some(BlockEntity { id: BANNER.into(), nbt: nbt.clone() }));
        if let Some(&kind) = chunkwake_world::BLOCK_ENTITY_TYPES.get().and_then(|t| t.get(BANNER)) {
            let bytes = chunkwake_world::nbt::write_network(&nbt);
            self.hub.send(None, play::block_entity_data(self.ids.block_entity_data, pos, kind, &bytes));
        }
    }

    /// `copy_components` из таблицы лута знамени: узоры и имя блок-сущности
    /// ложатся на выпавшее знамя.
    pub(super) fn dress_banner_loot(&self, be: Option<&BlockEntity>, loot: Vec<Stack>) -> Vec<Stack> {
        let Some(be) = be.filter(|b| b.id == BANNER) else {
            return loot;
        };
        let patterns = be.nbt.get("patterns").map(crate::stack_nbt::banner_layers_from_tag).unwrap_or_default();
        let name = be.nbt.get("CustomName").map(|t| crate::stack_nbt::tag_to_json(t).to_string());
        if patterns.is_empty() && name.is_none() {
            return loot;
        }
        loot.into_iter()
            .map(|mut s| {
                if self.item_name(s.item).ends_with("_banner") {
                    let mut e = (*chunkwake_protocol::components::get(s.extra)).clone();
                    e.banner_patterns = patterns.clone();
                    if name.is_some() {
                        e.custom_name = name.clone();
                    }
                    s.extra = chunkwake_protocol::components::intern(e);
                }
                s
            })
            .collect()
    }

    /// `MapBanner.fromWorld`: знамя с блок-сущностью в этом месте.
    pub(super) fn map_banner_at(&self, pos: [i32; 3]) -> Option<MapBanner> {
        let color = banner_color(self.state_name(self.block(pos)?))?;
        let be = self.block_entity(pos).filter(|b| b.id == BANNER)?;
        let name = be.nbt.get("CustomName").map(|t| crate::stack_nbt::tag_to_json(t).to_string());
        Some(MapBanner { pos, color, name })
    }

    /// `checkBanners` для перерисованных столбцов: метка без своего знамени
    /// снимается. Зовётся без блокировки мира.
    pub(super) fn check_map_banners(&self, map: &mut MapData, corners: &[(i32, i32)]) {
        for b in map.banners_in(corners) {
            if self.map_banner_at(b.pos).as_ref() != Some(&b) {
                map.remove_banner(&b);
            }
        }
    }

    /// `MapItem.useOn` по знамени: метка ставится или снимается. `false` —
    /// `InteractionResult.FAIL` (знамя вне карты, предел значков).
    pub(super) fn use_map_on_banner(&self, map: Stack, pos: [i32; 3]) -> bool {
        let Some(id) = self.map_id_of(map) else {
            return true;
        };
        let Some(banner) = self.map_banner_at(pos) else {
            // Знамя без блок-сущности: `fromWorld` → `null`, отказ.
            return self.toggle_map_banner(id, None);
        };
        self.toggle_map_banner(id, Some(banner))
    }
}
