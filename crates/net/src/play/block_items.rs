//! Предметы над особыми блоками — общее для игрока и раздатчика:
//! мёд и соты из полного улья (`BeehiveBlock.useItemOn`, `dropHoneycomb`,
//! `resetHoneyLevel`), заряд якоря возрождения светокаменем
//! (`RespawnAnchorBlock.charge`), бутылка воды по земле — грязь
//! (`PotionItem.useOn`), и особые действия раздатчика, которых нет в
//! [`PlayPlan::dispense_special`]: стеклянная бутылка, светокамень, соты,
//! бутылка воды, ножницы по улью, ведро снега и пустое ведро
//! (`DispenseItemBehavior.bootStrap`: `$4`, `$9`, `$10`, `$12`, `$13`,
//! `ShearsDispenseItemBehavior`, `SolidBucketItem`).
//!
//! Нет: пчёл (улей только сбрасывает мёд — злить и выпускать некого),
//! шалкеров из раздатчика (у шалкера нет контейнера), черепа иссушителя и
//! тыквы — ритуалы голема и иссушителя (идут как броня на голову).

use super::*;
use crate::hoppers as hp;

/// `BeehiveBlock.MAX_HONEY_LEVELS`.
const HONEY_FULL: &str = "5";
/// `RespawnAnchorBlock.MAX_CHARGES`.
const ANCHOR_MAX: u8 = 4;
const ANCHOR: &str = "minecraft:respawn_anchor";
/// `LevelEvent.PARTICLES_DESTROY_BLOCK`.
const EVENT_DESTROY: i32 = 2001;

/// Исход особого действия раздатчика (`OptionalDispenseItemBehavior`):
/// звук `1000`/`1001` и выброс предмета обычным путём.
pub(super) enum Dispensed {
    /// Сделано: щелчок `1000` и дым.
    Done,
    /// Осечка: `1001` и дым, предмет остаётся.
    Fail,
    /// Обычный выброс (`DefaultDispenseItemBehavior.execute`).
    Drop,
    /// Выброс со звуком осечки: бутылке нечего набрать, а `execute` всё
    /// равно падает в `DefaultDispenseItemBehavior.execute`.
    DropFail,
    /// Выброс внутренним `DefaultDispenseItemBehavior.dispense` (свой
    /// щелчок и дым) и ещё раз щелчок и дым внешнего — не вода в бутылке.
    DropTwice,
}

impl PlayPlan {
    /// Полный улей или гнездо (`BlockTags.BEEHIVES`, `honey_level` 5).
    pub(super) fn full_hive(&self, pos: [i32; 3]) -> Option<u32> {
        let s = self.block(pos)?;
        let hives = self.survival.block_tags.get("minecraft:beehives")?;
        (hives.contains(self.state_name(s))
            && self.survival.placement.prop(s, "honey_level") == Some(HONEY_FULL))
        .then_some(s)
    }

    /// `BeehiveBlock.resetHoneyLevel`: `setBlockAndUpdate` с `honey_level` 0.
    fn reset_honey(&self, pos: [i32; 3], s: u32) {
        if let Some(empty) = self.survival.placement.with_prop(s, "honey_level", "0") {
            self.set_block(pos, empty);
            self.redstone_update(&crate::redstone::around(pos));
        }
    }

    /// `Block.popResource`: в пределах ±0.25 от центра блока, на 0.125 ниже.
    fn pop_resource(&self, pos: [i32; 3], stack: Stack) {
        let r: [f64; 3] = rand::random();
        let at = [
            pos[0] as f64 + 0.5 + r[0] * 0.5 - 0.25,
            pos[1] as f64 + 0.5 + r[1] * 0.5 - 0.25 - 0.125,
            pos[2] as f64 + 0.5 + r[2] * 0.5 - 0.25,
        ];
        let vel = items::pop_velocity(rand::random(), rand::random());
        self.drop_item(at, vel, stack, items::BLOCK_DELAY);
    }

    /// Ножницы по полному улью: соты из `harvest/beehive`, звук `shear`
    /// в `sound_at` (у игрока — в его ногах, у раздатчика — в улье).
    pub(super) fn shear_hive(&self, pos: [i32; 3], sound_at: [f64; 3]) -> bool {
        let Some(s) = self.full_hive(pos) else {
            return false;
        };
        self.play_sound(None, "minecraft:block.beehive.shear", crate::sounds::BLOCKS, sound_at, 1.0, 1.0);
        let none = |_: &str| None;
        let level = |_: &str| 0;
        let ctx = crate::loot::Ctx {
            prop: &none,
            block: "minecraft:harvest/beehive",
            tool: Some("minecraft:shears"),
            level: &level,
            entity: None,
            killed_by_player: false,
            smelt: &|_| None,
        };
        for (drop, count) in crate::loot::roll(&self.survival.loot, &ctx, &mut rand::thread_rng()) {
            if let Some(id) = self.item_id(&drop) {
                self.pop_resource(pos, Stack::new(id, count.min(self.max_stack(id).max(1))));
            }
        }
        self.reset_honey(pos, s);
        self.emit_vibration(pos.map(|v| v as f64 + 0.5), "minecraft:shear");
        true
    }

    /// Бутылка по полному улью: мёд сброшен. Звук наполнения — только от
    /// игрока (`playSound(player, …)`: ему самому не шлётся), раздатчик
    /// набирает молча.
    pub(super) fn bottle_hive(&self, pos: [i32; 3], by: Option<(Uuid, [f64; 3])>) -> bool {
        let Some(s) = self.full_hive(pos) else {
            return false;
        };
        if let Some((uuid, at)) = by {
            self.play_sound(Some(uuid), "minecraft:item.bottle.fill", crate::sounds::BLOCKS, at, 1.0, 1.0);
        }
        self.reset_honey(pos, s);
        true
    }

    /// Якорь возрождения в `pos` и его заряд.
    pub(super) fn anchor_charge(&self, pos: [i32; 3]) -> Option<(u32, u8)> {
        let s = self.block(pos)?;
        if self.state_name(s) != ANCHOR {
            return None;
        }
        let c = self.survival.placement.prop(s, "charges")?.parse().ok()?;
        Some((s, c))
    }

    /// `RespawnAnchorBlock.charge`: +1 заряд, звук в центре блока.
    pub(super) fn charge_anchor(&self, pos: [i32; 3]) -> bool {
        let Some((s, c)) = self.anchor_charge(pos).filter(|(_, c)| *c < ANCHOR_MAX) else {
            return false;
        };
        let Some(next) = self.survival.placement.with_prop(s, "charges", &(c + 1).to_string()) else {
            return false;
        };
        self.set_block(pos, next);
        self.redstone_update(&crate::redstone::around(pos));
        let center = pos.map(|v| v as f64 + 0.5);
        self.emit_vibration(center, "minecraft:block_change");
        self.play_sound(None, "minecraft:block.respawn_anchor.charge", crate::sounds::BLOCKS, center, 1.0, 1.0);
        true
    }

    /// Бутылка воды (`potion_contents` с `minecraft:water`).
    pub(super) fn is_water_bottle(&self, st: Stack) -> bool {
        if self.item_name(st.item) != "minecraft:potion" {
            return false;
        }
        let water = self.survival.potions.iter().position(|n| n == "minecraft:water");
        let extra = chunkwake_protocol::components::get(st.extra);
        let potion = extra
            .raw
            .as_deref()
            .and_then(|raw| crate::potions::read_contents(raw, self.survival.potion_component))
            .and_then(|c| c.potion);
        water.is_some_and(|w| potion == Some(w as i32))
    }

    fn water_bottle(&self) -> Option<Stack> {
        let item = self.item_id("minecraft:potion")?;
        let water = self.survival.potions.iter().position(|n| n == "minecraft:water")?;
        Some(self.brewed_stack(Stack::new(item, 1), item, water as i32))
    }

    /// Земля под водой из бутылки (`BlockTags.CONVERTIBLE_TO_MUD`) → грязь:
    /// пять брызг над блоком, звук опустошения в `sound_at` (раздатчик
    /// звучит у себя, `BlockSource.pos`). У игрока ещё и всплеск — его
    /// слышат все, и он сам (`playSound(null, …)`).
    pub(super) fn soak_to_mud(&self, pos: [i32; 3], sound_at: [i32; 3], player: bool) -> bool {
        let Some(s) = self.block(pos) else { return false };
        let convertible = self
            .survival
            .block_tags
            .get("minecraft:convertible_to_mud")
            .is_some_and(|t| t.contains(self.state_name(s)));
        let Some(&mud) = self.survival.block_defaults.get("minecraft:mud") else {
            return false;
        };
        if !convertible {
            return false;
        }
        if player {
            self.block_sound(None, "minecraft:entity.generic.splash", pos, 1.0, 1.0);
        }
        if let Some(splash) = self.particle_id("minecraft:splash") {
            for _ in 0..5 {
                let r: [f64; 2] = rand::random();
                let at = [pos[0] as f64 + r[0], (pos[1] + 1) as f64, pos[2] as f64 + r[1]];
                self.hub.send(
                    None,
                    play::level_particles(self.ids.level_particles, splash, false, at, [0.0; 3], 1.0, 1),
                );
            }
        }
        self.block_sound(None, "minecraft:item.bottle.empty", sound_at, 1.0, 1.0);
        self.emit_vibration(pos.map(|v| v as f64 + 0.5), "minecraft:fluid_place");
        self.set_block(pos, mud);
        self.redstone_update(&crate::redstone::around(pos));
        true
    }

    /// `DefaultDispenseItemBehavior.consumeWithRemainder`: последний предмет
    /// стопки сменяется остатком, иначе остаток кладётся в раздатчик
    /// (`DispenserBlockEntity.insertItem`), а не влезший вылетает с щелчком и дымом.
    fn dispense_remainder(&self, me: &mut Cont, i: usize, st: Stack, rest: Stack, disp: [i32; 3], facing: usize) {
        if st.count <= 1 {
            me.slots[i] = Some(rest);
            return;
        }
        me.slots[i] = Some(Stack { count: st.count - 1, ..st });
        let max = self.max_stack(rest.item);
        if hp::insert_all(&mut me.slots, rest, |_| max) > 0 {
            let (at, vel) = hp::dispense_motion(disp, crate::placement::step(facing), rand::random());
            self.drop_item(at, vel, rest, std::time::Duration::ZERO);
            self.hub.send(None, play::level_event(self.ids.level_event, hp::EVENT_DISPENSE, disp, 0, false));
            self.hub.send(None, play::level_event(self.ids.level_event, hp::EVENT_SMOKE, disp, facing as i32, false));
        }
    }

    /// Особые действия раздатчика из этого модуля; `None` — не наш предмет.
    pub(super) fn dispense_extra(
        &self,
        me: &mut Cont,
        i: usize,
        st: Stack,
        front: [i32; 3],
        facing: usize,
    ) -> Option<Dispensed> {
        use crate::fluids as fl;
        let disp = crate::redstone::step(front, facing ^ 1);
        let plain = |name: &str| self.item_id(name).map(|id| Stack::new(id, 1));
        let front_state = self.block(front);
        let empty = front_state.is_some_and(|s| {
            matches!(self.state_name(s), "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
        });
        Some(match self.item_name(st.item) {
            // `$9`: мёд из полного улья, иначе вода из любой воды перед собой.
            "minecraft:glass_bottle" => {
                let water = front_state
                    .and_then(|s| fl::state_fluid(&self.survival.placement, s))
                    .is_some_and(|f| f.kind == fl::Kind::Water);
                let rest = if self.bottle_hive(front, None) {
                    plain("minecraft:honey_bottle")
                } else if water {
                    self.water_bottle()
                } else {
                    return Some(Dispensed::DropFail);
                };
                self.emit_vibration(disp.map(|v| v as f64 + 0.5), "minecraft:fluid_pickup");
                if let Some(rest) = rest {
                    self.dispense_remainder(me, i, st, rest, disp, facing);
                }
                Dispensed::Done
            }
            // `$10`: якорь заряжается, полный — осечка, не якорь — выброс.
            "minecraft:glowstone" => match self.anchor_charge(front) {
                Some((_, c)) if c < ANCHOR_MAX => {
                    self.charge_anchor(front);
                    hp::take_one(&mut me.slots, i);
                    Dispensed::Done
                }
                Some(_) => Dispensed::Fail,
                None => Dispensed::Drop,
            },
            // `$12`: воск на медь (`HoneycombItem.getWaxed`).
            "minecraft:honeycomb" => {
                let s = front_state?;
                let name = self.state_name(s).to_owned();
                let Some(waxed) = self.survival.transforms.waxed.get(&name) else {
                    return Some(Dispensed::Drop);
                };
                let pl = &self.survival.placement;
                let mut next = *self.survival.block_defaults.get(waxed)?;
                for (k, v) in pl.props(s) {
                    next = pl.with_prop(next, k, v).unwrap_or(next);
                }
                self.put_transformed(front, &name, next);
                self.hub.send(
                    None,
                    play::level_event(self.ids.level_event, crate::transform::EVENT_WAX_ON, front, 0, false),
                );
                self.block_sound(None, "minecraft:item.honeycomb.wax_on", front, 1.0, 1.0);
                hp::take_one(&mut me.slots, i);
                Dispensed::Done
            }
            // `$13`: вода в землю перед собой; любое другое зелье — выброс.
            "minecraft:potion" => {
                if !self.is_water_bottle(st) || !self.soak_to_mud(front, disp, false) {
                    return Some(Dispensed::DropTwice);
                }
                if let Some(rest) = plain("minecraft:glass_bottle") {
                    self.dispense_remainder(me, i, st, rest, disp, facing);
                }
                Dispensed::Done
            }
            // `ShearsDispenseItemBehavior.tryShearBeehive` раньше овец.
            "minecraft:shears" => {
                if !self.shear_hive(front, front.map(|v| v as f64 + 0.5)) {
                    return None;
                }
                let mut s = st;
                me.slots[i] = (!self.wear_one(&mut s)).then_some(s);
                Dispensed::Done
            }
            // `SolidBucketItem.emptyContents`: только в пустоту.
            "minecraft:powder_snow_bucket" => {
                let Some(&snow) = self.survival.block_defaults.get("minecraft:powder_snow") else {
                    return Some(Dispensed::Drop);
                };
                if !empty {
                    return Some(Dispensed::DropTwice);
                }
                self.set_block(front, snow);
                self.emit_vibration(front.map(|v| v as f64 + 0.5), "minecraft:fluid_place");
                self.block_sound(None, "minecraft:item.bucket.empty_powder_snow", front, 1.0, 1.0);
                if let Some(rest) = plain("minecraft:bucket") {
                    self.dispense_remainder(me, i, st, rest, disp, facing);
                }
                Dispensed::Done
            }
            // `$3`, полное ведро: льёт [`PlayPlan::dispense_special`]; некуда —
            // выброс внутренним `dispense` (двойной щелчок).
            "minecraft:water_bucket" | "minecraft:lava_bucket" => {
                let s = front_state?;
                let kind = if self.item_name(st.item) == "minecraft:water_bucket" {
                    fl::Kind::Water
                } else {
                    fl::Kind::Lava
                };
                let here = fl::state_fluid(&self.survival.placement, s);
                let soak = kind == fl::Kind::Water && self.waterloggable(front);
                let open = empty || self.fluid_replaceable(front);
                let blocked = !open && here.is_none() && !soak;
                if blocked || here.is_some_and(|f| f.source && f.kind == kind) {
                    return Some(Dispensed::DropTwice);
                }
                return None;
            }
            // `$4`: `BucketPickup.pickupBlock` — источник, вода из заводнённого
            // блока, рыхлый снег. Без звука наполнения: его играет только
            // `BucketItem.use` игрока.
            "minecraft:bucket" => {
                let s = front_state?;
                let filled = if self.state_name(s) == "minecraft:powder_snow" {
                    self.set_block(front, AIR);
                    self.hub.send(
                        None,
                        play::level_event(self.ids.level_event, EVENT_DESTROY, front, s as i32, false),
                    );
                    "minecraft:powder_snow_bucket"
                } else {
                    let Some(f) = fl::state_fluid(&self.survival.placement, s).filter(|f| f.source) else {
                        return Some(Dispensed::Drop);
                    };
                    self.drain(front);
                    match f.kind {
                        fl::Kind::Water => "minecraft:water_bucket",
                        fl::Kind::Lava => "minecraft:lava_bucket",
                    }
                };
                self.emit_vibration(front.map(|v| v as f64 + 0.5), "minecraft:fluid_pickup");
                if let Some(rest) = plain(filled) {
                    self.dispense_remainder(me, i, st, rest, disp, facing);
                }
                Dispensed::Done
            }
            _ => return None,
        })
    }
}
