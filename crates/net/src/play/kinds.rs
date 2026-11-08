//! Виды состояний блоков по номеру — для горячих путей (`set_block`,
//! случайные и отложенные тики), где сравнение имён строк стоило дороже
//! самой работы. Таблица строится один раз из имён и свойств.
//!
//! Здесь же листва по `LeavesBlock`: `updateShape` ставит тик через 1,
//! `tick` пересчитывает `distance` по шести соседям, случайный тик есть
//! только у распадающейся (`distance` 7 без `persistent`).

use super::PlayPlan;

/// Огонь и огонь душ.
pub(super) const FIRE: u16 = 1;
/// Любая кровать (`*_bed`).
pub(super) const BED: u16 = 1 << 1;
/// Блок со свойством `distance` листвы (`LeavesBlock.DISTANCE`).
pub(super) const LEAVES: u16 = 1 << 2;
/// `#prevents_nearby_leaf_decay`: для листвы на расстоянии 0.
pub(super) const LEAF_ANCHOR: u16 = 1 << 3;
/// Листва, которая распадается: `distance` 7 и `persistent=false`.
pub(super) const DECAYING: u16 = 1 << 4;
/// Окисляемая медь (`WeatheringCopper`).
pub(super) const WEATHERING: u16 = 1 << 5;
/// Лава.
pub(super) const LAVA: u16 = 1 << 6;
/// Воздух любого вида (`minecraft:air`, `cave_air`, `void_air`).
pub(super) const AIR: u16 = 1 << 7;

impl PlayPlan {
    /// Флаги видов всех состояний.
    pub(super) fn kinds(&self) -> &[u16] {
        self.state_kinds.get_or_init(|| {
            let pl = &self.survival.placement;
            let anchors = self
                .survival
                .block_tags
                .get("minecraft:prevents_nearby_leaf_decay")
                .or_else(|| self.survival.block_tags.get("minecraft:logs"));
            (0..self.survival.block_names.len() as u32)
                .map(|state| {
                    let name = self.state_name(state);
                    let mut k = 0;
                    if crate::fire::is_fire(name) {
                        k |= FIRE;
                    }
                    if name.ends_with("_bed") {
                        k |= BED;
                    }
                    if name.ends_with("_leaves") && pl.prop(state, "distance").is_some() {
                        k |= LEAVES;
                        if pl.prop(state, "distance") == Some("7")
                            && pl.prop(state, "persistent") == Some("false")
                        {
                            k |= DECAYING;
                        }
                    }
                    if anchors.is_some_and(|set| set.contains(name)) {
                        k |= LEAF_ANCHOR;
                    }
                    if self.survival.transforms.weathering.contains_key(name) {
                        k |= WEATHERING;
                    }
                    if name == "minecraft:lava" {
                        k |= LAVA;
                    }
                    if name.ends_with(":air") {
                        k |= AIR;
                    }
                    k
                })
                .collect()
        })
    }

    /// Есть ли у состояния хоть один из флагов `f`.
    #[inline]
    pub(super) fn is_kind(&self, state: u32, f: u16) -> bool {
        self.kinds().get(state as usize).is_some_and(|k| k & f != 0)
    }

    /// `LeavesBlock.getDistanceAt`: якорь — 0, листва — своё `distance`, прочее — 7.
    fn leaf_distance_of(&self, state: u32) -> u8 {
        if self.is_kind(state, LEAF_ANCHOR) {
            return 0;
        }
        if self.is_kind(state, LEAVES) {
            return self
                .survival
                .placement
                .prop(state, "distance")
                .and_then(|d| d.parse().ok())
                .unwrap_or(7);
        }
        7
    }

    /// `LeavesBlock.updateShape` у листвы `leaves` рядом со сменившимся
    /// блоком `changed`: тик через 1, если расстояние могло поменяться.
    pub(super) fn leaves_shape(&self, leaves_pos: [i32; 3], leaves: u32, changed: u32) {
        let i = self.leaf_distance_of(changed) + 1;
        if i != 1 || self.leaf_distance_of(leaves) != i {
            self.schedule(leaves_pos, 1);
        }
    }

    /// `LeavesBlock.tick`: `distance` = min по шести соседям + 1 (до 7).
    pub(super) fn leaves_tick(&self, pos: [i32; 3], state: u32) {
        let mut d = 7u8;
        for dir in 0..6 {
            let n = crate::redstone::step(pos, dir);
            let s = self.block(n).unwrap_or(0);
            d = d.min(self.leaf_distance_of(s) + 1);
            if d == 1 {
                break;
            }
        }
        if let Some(next) = self
            .survival
            .placement
            .with_prop(state, "distance", &d.to_string())
            && next != state
        {
            self.set_block(pos, next);
        }
    }

    /// `LeavesBlock.randomTick`: распадающаяся листва роняет добычу и
    /// исчезает (`removeBlock` — без частиц разрушения).
    pub(super) fn leaves_decay(&self, pos: [i32; 3], state: u32) {
        let at = [
            pos[0] as f64 + 0.5,
            pos[1] as f64 + 0.25,
            pos[2] as f64 + 0.5,
        ];
        self.set_block(pos, 0);
        for st in self.block_loot(state, None) {
            let vel = crate::items::pop_velocity(rand::random(), rand::random());
            self.drop_item(at, vel, st, crate::items::BLOCK_DELAY);
        }
    }
}
