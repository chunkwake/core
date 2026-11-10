//! Состояние игрока для плагинов: зеркало, которое сессия обновляет каждый тик,
//! и команды управления от плагинов (`Player.setHealth`, `getInventory().setItem`…).
//!
//! Формат обмена с JVM — текст: строки через `\n`, поля через пробел.

use crate::survival::{OFFHAND, SLOTS};
use chunkwake_protocol::inventory::{Slot, Stack};

/// Размер `PlayerInventory` Bukkit: хотбар, основной, броня, вторая рука.
pub const BUKKIT_SLOTS: usize = 41;

/// Индекс Bukkit (`PlayerInventory`) → слот окна игрока (`InventoryMenu`).
/// 0–8 хотбар, 9–35 основной, 36–39 ботинки..шлем, 40 вторая рука.
pub fn bukkit_to_window(i: usize) -> Option<usize> {
    match i {
        0..=8 => Some(36 + i),
        9..=35 => Some(i),
        36..=39 => Some(8 - (i - 36)),
        40 => Some(OFFHAND),
        _ => None,
    }
}

/// Снимок игрока для плагинов.
#[derive(Clone, Debug, PartialEq)]
pub struct Live {
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub game_mode: u8,
    pub selected: usize,
    pub slots: [Slot; SLOTS],
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub effects: Vec<crate::survival::Effect>,
    pub xp: crate::xp::Experience,
    /// Приседает (`Player.isSneaking`): меняется от `player_input`, без тика.
    pub sneaking: bool,
    /// Измерение игрока (`Player.getWorld`): меняется переходом и респавном.
    pub dimension: crate::play::Dimension,
}

impl Live {
    /// Отпечаток состояния: всё, что попадает в [`Self::render`].
    ///
    /// Кэш текста снимка сверяет отпечаток, а не счётчик изменений: счётчик
    /// надо поднимать в каждом месте, где правится зеркало, и одна забытая
    /// строчка молча отдала бы плагину устаревшее состояние. Отпечаток
    /// считается по самим полям, поэтому промахнуться невозможно.
    pub fn fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        self.health.to_bits().hash(&mut h);
        self.food.hash(&mut h);
        self.saturation.to_bits().hash(&mut h);
        self.exhaustion.to_bits().hash(&mut h);
        self.game_mode.hash(&mut h);
        self.selected.hash(&mut h);
        for v in self.pos {
            v.to_bits().hash(&mut h);
        }
        self.yaw.to_bits().hash(&mut h);
        self.pitch.to_bits().hash(&mut h);
        self.xp.level.hash(&mut h);
        self.xp.progress.to_bits().hash(&mut h);
        self.xp.total.hash(&mut h);
        self.sneaking.hash(&mut h);
        self.dimension.hash(&mut h);
        // Слоты: `Option<Stack>` — сравнивается всё, что печатается в строку.
        for s in &self.slots {
            match s {
                Some(s) => {
                    s.item.hash(&mut h);
                    s.count.hash(&mut h);
                    s.damage.hash(&mut h);
                    s.extra.hash(&mut h);
                }
                None => 0u8.hash(&mut h),
            }
        }
        for e in &self.effects {
            e.id.hash(&mut h);
            e.amplifier.hash(&mut h);
            e.duration.hash(&mut h);
            e.particles.hash(&mut h);
            e.ambient.hash(&mut h);
            e.show_icon.hash(&mut h);
        }
        h.finish()
    }

    /// Первая строка — `health food saturation exhaustion mode selected x y z yaw pitch
    /// уровень полоска всего_опыта приседает(0/1)`, затем
    /// [`BUKKIT_SLOTS`] строк `item count` (или `-`) в порядке Bukkit, затем эффекты
    /// строками `e имя усиление тики частицы фон значок` (0/1).
    pub fn render<'a>(
        &'a self,
        item_name: impl Fn(i32) -> Option<&'a str>,
        effect_name: impl Fn(i32) -> Option<&'a str>,
    ) -> String {
        let [x, y, z] = self.pos;
        let mut out = format!(
            "{} {} {} {} {} {} {x} {y} {z} {} {} {} {} {} {} {}",
            self.health,
            self.food,
            self.saturation,
            self.exhaustion,
            self.game_mode,
            self.selected,
            self.yaw,
            self.pitch,
            self.xp.level,
            self.xp.progress,
            self.xp.total,
            u8::from(self.sneaking),
            self.dimension.key()
        );
        for i in 0..BUKKIT_SLOTS {
            out.push('\n');
            let stack = bukkit_to_window(i).and_then(|w| self.slots[w]);
            match stack.and_then(|s| item_name(s.item).map(|n| (n, s))) {
                Some((name, s)) => out += &crate::stack_nbt::to_line(s, name),
                None => out.push('-'),
            }
        }
        for e in &self.effects {
            if let Some(name) = effect_name(e.id) {
                out += &format!(
                    "\ne {name} {} {} {} {} {}",
                    e.amplifier,
                    e.duration,
                    u8::from(e.particles),
                    u8::from(e.ambient),
                    u8::from(e.show_icon)
                );
            }
        }
        out
    }
}

/// Команда сессии игрока от плагина.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    /// Внутреннее: штатно завершить сессию при остановке сервера.
    Shutdown,
    Health(f32),
    Food(i32),
    Saturation(f32),
    Exhaustion(f32),
    /// Режим без события (плагин уже вызвал `PlayerGameModeChangeEvent`).
    GameMode(u8),
    /// Слот окна и стопка.
    Slot(usize, Slot),
    Selected(usize),
    /// Телепорт: позиция, поворот.
    Teleport([f64; 3], f32, f32),
    /// Переход в другое измерение: `respawn` клиенту и новый поток чанков.
    DimensionChange(crate::play::Dimension, [f64; 3], (f32, f32)),
    /// `ServerPlayer.showEndCredits`: титры при первом выходе из Края.
    Credits,
    /// Очистить инвентарь игрока (`/clear`).
    Clear,
    /// Снять до `max` штук предметов из списка (`/clear` с пределом или фильтром).
    /// `extra == 0` и `damage == None` — любые компоненты.
    ClearItem {
        /// `None` — любой предмет.
        items: Option<Vec<i32>>,
        extra: u32,
        damage: Option<i32>,
        max: i32,
    },
    /// Выдать предмет (`/give`): id, количество, компоненты и износ.
    Give {
        item: i32,
        count: i32,
        extra: u32,
        damage: i32,
    },
    /// Внутреннее: урон извне сессии (стрела из раздатчика).
    Hurt(f32, crate::survival::Cause),
    /// Мгновенное лечение от взрывного зелья с силой по расстоянию.
    Heal(f32),
    /// Внутреннее (не от плагинов): содержимое сундука изменил другой игрок.
    ContainerChanged([i32; 3]),
    /// Внутреннее: тик печи — свойства окна и сменились ли предметы.
    Furnace([i32; 3], [i32; 4], bool),
    Brewing([i32; 3], [i32; 2], bool),
    /// Наложить эффект. `command` — сессия вызовет событие с причиной COMMAND.
    Effect {
        effect: crate::survival::Effect,
        force: bool,
        command: bool,
    },
    /// Снять эффект (`None` — все). `command` — событие COMMAND.
    ClearEffect {
        id: Option<i32>,
        command: bool,
    },
    /// Опыт: `xp points|levels|level|setpoints|progress|total <число>`.
    Xp(crate::xp::XpChange),
    /// Открыть окно плагина: `open <id> <заголовок JSON>`.
    OpenPlugin(i32, String),
    /// Сообщение канала плагина клиенту: `payload <канал> <hex>`.
    Payload(String, Vec<u8>),
    /// Закрыть открытое окно (`closewin`).
    CloseWindow,
    /// Отключить игрока с причиной (JSON-компонент): `/kick`, бан, `Player.kick`.
    Kick(String),
    /// Переслать на другой сервер (`/transfer`).
    Transfer(String, u16),
}

impl Control {
    #[cfg(test)]
    fn parse_t(line: &str, item_id: impl Fn(&str) -> Option<i32>) -> Option<Self> {
        Self::parse(line, item_id, |n| (n == "minecraft:haste").then_some(2))
    }

    /// Разбор строки от JVM: `health 10`, `food 20`, `mode 1`,
    /// `slot <bukkit> <item> <count>` или `slot <bukkit> -`, `held 3`,
    /// `effect <имя> <усиление> <тики> <частицы> [фон] [значок] [force]`, `uneffect <имя>|*`.
    pub fn parse(
        line: &str,
        item_id: impl Fn(&str) -> Option<i32>,
        effect_id: impl Fn(&str) -> Option<i32>,
    ) -> Option<Self> {
        let mut it = line.split_whitespace();
        let f = |s: Option<&str>| {
            s.and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite())
        };
        Some(match it.next()? {
            "health" => Control::Health(f(it.next())?.max(0.0)),
            "food" => Control::Food(it.next()?.parse::<i32>().ok()?.clamp(0, 20)),
            "saturation" => Control::Saturation(f(it.next())?.max(0.0)),
            "exhaustion" => Control::Exhaustion(f(it.next())?.max(0.0)),
            "mode" => Control::GameMode(it.next()?.parse::<u8>().ok().filter(|&m| m <= 3)?),
            "tp" => {
                let mut n = || {
                    it.next()
                        .and_then(|s| s.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                };
                let pos = [n()?, n()?, n()?];
                Control::Teleport(pos, n()? as f32, n()? as f32)
            }
            "effect" => {
                let id = effect_id(it.next()?)?;
                let amplifier = it.next()?.parse::<u8>().ok()?;
                let duration = it
                    .next()?
                    .parse::<i32>()
                    .ok()
                    .filter(|&d| d == -1 || d > 0)?;
                let particles = it.next().is_none_or(|p| p != "0");
                let ambient = it.next().is_some_and(|p| p == "1");
                let show_icon = it.next().is_none_or(|p| p != "0");
                let force = it.next().is_some_and(|p| p == "1");
                Control::Effect {
                    effect: crate::survival::Effect {
                        id,
                        amplifier,
                        duration,
                        ambient,
                        particles,
                        show_icon,
                        age: 0,
                    },
                    force,
                    command: false,
                }
            }
            "xp" => {
                use crate::xp::XpChange as X;
                let kind = it.next()?;
                let v = it.next()?;
                let int = || v.parse::<i32>().ok();
                Control::Xp(match kind {
                    "points" => X::Points(int()?),
                    "levels" => X::Levels(int()?),
                    "level" => X::SetLevel(int()?),
                    "setpoints" => X::SetPoints(int()?),
                    "progress" => X::SetProgress(v.parse::<f32>().ok().filter(|p| p.is_finite())?),
                    "total" => X::SetTotal(int()?),
                    _ => return None,
                })
            }
            "uneffect" => {
                let id = match it.next()? {
                    "*" => None,
                    name => Some(effect_id(name)?),
                };
                Control::ClearEffect { id, command: false }
            }
            "open" => {
                let id = it.next()?.parse::<i32>().ok()?;
                let title = line
                    .trim_start()
                    .splitn(3, char::is_whitespace)
                    .nth(2)?
                    .trim();
                Control::OpenPlugin(id, title.to_owned())
            }
            "closewin" => Control::CloseWindow,
            // `Player.kick`: событие плагин уже вызвал, здесь только причина.
            "kick" => Control::Kick(
                line.trim_start()
                    .split_once(char::is_whitespace)
                    .map_or("", |(_, rest)| rest.trim())
                    .to_owned(),
            ),
            "payload" => {
                let channel = it.next()?.to_owned();
                let data = crate::payload::from_hex(it.next().unwrap_or(""))?;
                if data.len() > crate::payload::MAX_CLIENTBOUND {
                    return None;
                }
                Control::Payload(channel, data)
            }
            "held" => Control::Selected(it.next()?.parse::<usize>().ok().filter(|&s| s <= 8)?),
            "slot" => {
                let window = bukkit_to_window(it.next()?.parse().ok()?)?;
                // Остаток строки после номера — стопка (`-` — пусто).
                let rest = line
                    .trim_start()
                    .splitn(3, char::is_whitespace)
                    .nth(2)?
                    .trim();
                let stack = match rest {
                    "-" => None,
                    l => Some(crate::stack_nbt::from_line(l, &item_id)?),
                };
                Control::Slot(window, stack)
            }
            _ => return None,
        })
    }

    /// Применяет команду к зеркалу (чтобы плагин сразу видел своё изменение).
    pub fn apply(&self, live: &mut Live) {
        match *self {
            Control::Health(h) => live.health = h,
            Control::Heal(amount) => {
                live.health = (live.health + amount).min(crate::survival::MAX_HEALTH)
            }
            Control::Food(f) => live.food = f,
            Control::Saturation(s) => live.saturation = s,
            Control::Exhaustion(e) => live.exhaustion = e,
            Control::GameMode(m) => live.game_mode = m,
            Control::Slot(w, s) => live.slots[w] = s,
            Control::Selected(s) => live.selected = s,
            Control::Teleport(p, yaw, pitch) => {
                live.pos = p;
                live.yaw = yaw;
                live.pitch = pitch;
            }
            Control::DimensionChange(dim, p, (yaw, pitch)) => {
                live.dimension = dim;
                live.pos = p;
                live.yaw = yaw;
                live.pitch = pitch;
            }
            Control::Credits => {}
            Control::Effect { effect, force, .. } => {
                crate::survival::add_effect(&mut live.effects, effect, force);
            }
            Control::ClearEffect { id, .. } => {
                live.effects.retain(|e| id.is_some_and(|w| w != e.id))
            }
            Control::Clear => live.slots = [None; SLOTS],
            Control::ClearItem {
                ref items,
                extra,
                damage,
                max,
            } => {
                let _ = crate::survival::take_selected_slots(
                    &mut live.slots[1..],
                    items.as_deref(),
                    extra,
                    damage,
                    max,
                );
            }
            Control::Give {
                item,
                count,
                extra,
                damage,
            } => {
                let mut inv = crate::survival::Inventory {
                    slots: live.slots,
                    carried: None,
                    selected: live.selected,
                    state: 0,
                    drag: Default::default(),
                };
                let max = if damage > 0 { 1 } else { 64 };
                inv.add_components(item, count, max, extra, damage);
                live.slots = inv.slots;
            }
            Control::Xp(c) => {
                live.xp.apply(c);
            }
            Control::ContainerChanged(_)
            | Control::OpenPlugin(..)
            | Control::Payload(..)
            | Control::CloseWindow
            | Control::Kick(_)
            | Control::Transfer(..)
            | Control::Furnace(..)
            | Control::Brewing(..)
            | Control::Shutdown
            | Control::Hurt(..) => {}
        }
    }
}

/// События для плагинов (`PluginHooks::event`); номера совпадают с
/// `dev.chunkwake.bridge.Bridge.EVENT_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Event {
    /// `EntityDamageEvent`: `<CAUSE> <amount>` → `c` (отмена) или `d <amount>`.
    Damage = 1,
    /// `PlayerDeathEvent`: `<damage_type> <опыт>\n<message json>\n<item count>…` →
    /// `<keepInventory> <keepLevel> <опыт> <newLevel> <newExp> <newTotalExp>\n<message json или пусто>\n<item count>…`.
    Death = 2,
    /// `PlayerDropItemEvent`: `<item> <count>` → `c` или пусто.
    Drop = 3,
    /// `EntityPickupItemEvent`: `<item> <count> <remaining>` → `c` или пусто.
    Pickup = 4,
    /// `PlayerGameModeChangeEvent`: `<mode> <CAUSE>` → `c` или пусто.
    GameMode = 5,
    /// `AsyncChatEvent`/`AsyncPlayerChatEvent`: текст → `c` или JSON сообщения для всех.
    Chat = 6,
    /// `PlayerCommandPreprocessEvent`: строка без `/` → `c`, `m <новая строка>` или пусто.
    CommandPreprocess = 7,
    /// Таб-комплит команд плагинов: строка без `/` → варианты через `\n`.
    TabComplete = 8,
    /// `BlockPlaceEvent`: `x y z block ax ay az hand` → `c` или пусто.
    Place = 9,
    /// `PlayerMoveEvent`: `from(x y z yaw pitch) to(x y z yaw pitch)` →
    /// `c`, `t x y z yaw pitch` (новая цель) или пусто.
    Move = 10,
    /// `PlayerInteractEvent`: `ACTION x y z FACE hand` (`~` вместо координат без блока) → `c` или пусто.
    Interact = 11,
    /// `EntityPotionEffectEvent` от сервера (`/effect`, конец таймера):
    /// `CAUSE ACTION [ключ усиление тики фон частицы значок …]` → `c` или пусто.
    Potion = 12,
    /// `PrepareItemEnchantEvent`: `x y z полки\nпредмет\nцена чары уровень`×3 →
    /// `c`, пусто или три новые строки `цена чары уровень` (`-` — нет).
    PrepareEnchant = 13,
    /// `EnchantItemEvent`: `x y z цена кнопка подсказка уровень\nпредмет\nчары уровень…` →
    /// `c`, пусто или `цена\nчары уровень…`.
    Enchant = 14,
    /// `PlayerPickupExperienceEvent`: `величина x y z` → `c` или пусто.
    PickupXp = 15,
    /// `PlayerItemMendEvent`: `слот починка опыт предмет` → `c`, пусто или новая починка.
    Mend = 16,
    /// `PlayerExpChangeEvent`: `очки` → пусто или новые очки.
    ExpChange = 17,
    /// `BlockRedstoneEvent`: `x y z старый новый` → пусто или новая сила.
    Redstone = 18,
    /// `BlockPistonExtendEvent`/`BlockPistonRetractEvent`:
    /// `x y z грань 1|0 x,y,z;…` → `c` или пусто.
    Piston = 19,
    /// `InventoryMoveItemEvent`: `откуда x y z тип куда x y z тип 1|0 предмет` → `c`, пусто или `i предмет`.
    MoveItem = 20,
    /// `InventoryPickupItemEvent` (воронка берёт с земли): `x y z предмет` → `c` или пусто.
    HopperPickup = 21,
    /// `BlockDispenseEvent`: `x y z vx vy vz предмет` → `c` или пусто.
    Dispense = 22,
    /// `PlayerBucketEmptyEvent`/`PlayerBucketFillEvent`:
    /// `EMPTY|FILL x y z FACE ведро hand` (x y z — блок под взглядом) → `c` или пусто.
    Bucket = 23,
    /// `PlayerPickupArrowEvent`: `x y z` воткнутой стрелы → `c` или пусто.
    PickupArrow = 24,
    /// `EntityShootBowEvent`: `оружие снаряд сила рука тратить(1|0)` → `c`, пусто
    /// или `1`/`0` — плагин сменил `setConsumeItem`.
    ShootBow = 25,
    /// `InventoryClickEvent`: `тип окно размер слот кнопка режим\nтекущий\nкурсор`
    /// (окно — id инвентаря плагина или -1, стопки — строки или `-`) → `c` или пусто.
    InventoryClick = 26,
    /// `InventoryCloseEvent` окна плагина: `id` (без ответа).
    InventoryClose = 27,
    /// `EntityDamageByEntityEvent` (игрок бьёт моба): `id вид урон крит(1|0)` → `c`, пусто или `d урон`.
    MobDamage = 28,
    /// `EntityDeathEvent` моба: `id вид x y z опыт\nпредмет…` → пусто или `опыт\nпредмет…`.
    MobDeath = 29,
    /// `PlayerInteractEntityEvent`: `id вид рука` → `c` или пусто.
    InteractEntity = 30,
    /// `CreatureSpawnEvent`: `id вид x y z ПРИЧИНА` → `c` или пусто.
    CreatureSpawn = 31,
    /// `EntityBreedEvent`: `детёныш мать отец опыт` → `c`, пусто или новый опыт.
    Breed = 32,
    /// `PlayerShearEntityEvent`: `id рука` → `c` или пусто.
    Shear = 33,
    /// `CampfireStartEvent`: `x y z\nid опыт время\nрезультат\nисходная стопка` → время готовки.
    CampfireStart = 34,
    /// `BlockCookEvent`: `x y z\nid опыт время\nсырьё\nрезультат` → `c`, `e` (пустой результат),
    /// `i стопка` или пусто.
    Cook = 35,
    /// `PrepareGrindstoneEvent`: `x y z\nвход\nвход\nрезультат` → пусто, `e` или `i стопка`.
    PrepareGrindstone = 36,
    /// Сообщение канала плагина от клиента: `<канал> <hex>`, ответ не нужен.
    /// `minecraft:register` из configuration приходит этим же событием после входа.
    PluginMessage = 37,
    /// `AsyncPlayerPreLoginEvent` (и `PlayerPreLoginEvent`) после авторизации,
    /// вне потока плагинов: `имя uuid ip порт\nсвойство значение [подпись]…` →
    /// `k <json причины>` (отказ) или пусто.
    PreLogin = 38,
    /// `PlayerConnectionValidateLoginEvent` и `PlayerLoginEvent` после проверки
    /// бана/белого списка/мест:
    /// `имя uuid ip порт\n[KICK_BANNED|KICK_WHITELIST|KICK_FULL <json>]\nсвойство значение [подпись]…` →
    /// `a` (пустить), `k <json причины>` или пусто (решение ванили).
    Login = 39,
    /// `PlayerKickEvent`: `ПРИЧИНА\n<json причины>` → `c` (отмена), пусто или
    /// `<json причины>` (новая). Сообщение о выходе остаётся в Java до `quit`.
    Kick = 40,
    /// `PlayerRespawnEvent`: `x y z yaw pitch кровать(1|0) якорь(1|0) ПРИЧИНА` →
    /// пусто или новая точка `x y z yaw pitch`.
    Respawn = 41,
    /// `PlayerTeleportEvent` от сервера (команды, жемчуг):
    /// `ПРИЧИНА fx fy fz fyaw fpitch tx ty tz tyaw tpitch` → `c`, пусто или
    /// `t x y z yaw pitch` (новая цель).
    Teleport = 42,
    /// `PlayerItemHeldEvent`: `старый новый` слот хотбара → `c` или пусто.
    ItemHeld = 43,
    /// `PlayerToggleSneakEvent`: `1|0` → `c` или пусто.
    ToggleSneak = 44,
    /// `PlayerToggleSprintEvent`: `1|0` → `c` или пусто.
    ToggleSprint = 45,
    /// `PlayerChangedWorldEvent` после перехода: `<ключ прежнего мира>`, ответ не нужен.
    ChangedWorld = 46,
    /// `UncheckedSignChangeEvent` до проверок: `x y z FRONT|BACK` и четыре
    /// строки JSON-строками по строке → `c` или пусто.
    SignUnchecked = 47,
    /// `SignChangeEvent`: `x y z FRONT|BACK` и четыре компонента JSON по
    /// строке → `c`, пусто или четыре новых компонента.
    SignChange = 48,
    /// `PlayerOpenSignEvent` (и `PlayerSignOpenEvent`): `x y z FRONT|BACK ПРИЧИНА` → `c` или пусто.
    OpenSign = 49,
}

impl Event {
    /// Бит в маске слушателей (`Bridge.LISTEN_*`).
    pub fn listener_bit(self) -> u64 {
        4 << self as i32
    }
}

/// Стопки из строк `item count` (имена неизвестных предметов пропускаются).
pub fn parse_stacks<'a>(
    lines: impl Iterator<Item = &'a str>,
    item_id: impl Fn(&str) -> Option<i32>,
) -> Vec<Stack> {
    lines
        .filter_map(|l| crate::stack_nbt::from_line(l, &item_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(name: &str) -> Option<i32> {
        match name {
            "minecraft:stone" => Some(1),
            "minecraft:bread" => Some(2),
            _ => None,
        }
    }

    #[test]
    fn slot_mapping_matches_inventory_menu() {
        assert_eq!(bukkit_to_window(0), Some(36));
        assert_eq!(bukkit_to_window(8), Some(44));
        assert_eq!(bukkit_to_window(9), Some(9));
        assert_eq!(bukkit_to_window(35), Some(35));
        // Ботинки (36) → слот 8, шлем (39) → 5.
        assert_eq!(bukkit_to_window(36), Some(8));
        assert_eq!(bukkit_to_window(39), Some(5));
        assert_eq!(bukkit_to_window(40), Some(OFFHAND));
        assert_eq!(bukkit_to_window(41), None);
    }

    #[test]
    fn render_and_control() {
        let mut live = Live {
            health: 20.0,
            food: 20,
            saturation: 5.0,
            exhaustion: 0.0,
            game_mode: 0,
            selected: 0,
            slots: [None; SLOTS],
            pos: [1.0, 64.0, -2.5],
            yaw: 90.0,
            pitch: 0.0,
            effects: Vec::new(),
            xp: Default::default(),
            sneaking: false,
            dimension: crate::play::Dimension::Overworld,
        };
        let c = Control::parse_t("slot 0 minecraft:bread 3", ids).unwrap();
        assert_eq!(
            c,
            Control::Slot(
                36,
                Some(Stack {
                    item: 2,
                    count: 3,
                    damage: 0,
                    extra: 0
                })
            )
        );
        c.apply(&mut live);
        Control::parse_t("health 7.5", ids)
            .unwrap()
            .apply(&mut live);
        let text = live.render(
            |id| match id {
                2 => Some("minecraft:bread"),
                _ => None,
            },
            |_| None,
        );
        // Эффекты: наложить в зеркало, снять все.
        Control::parse_t("effect minecraft:haste 1 200 0", ids)
            .unwrap()
            .apply(&mut live);
        assert_eq!(
            live.effects,
            vec![crate::survival::Effect {
                id: 2,
                amplifier: 1,
                duration: 200,
                ambient: false,
                particles: false,
                show_icon: true,
                age: 0,
            }]
        );
        assert!(
            live.render(
                |id| match id {
                    2 => Some("minecraft:bread"),
                    _ => None,
                },
                |_| Some("minecraft:haste")
            )
            .ends_with(
                "
e minecraft:haste 1 200 0 0 1"
            )
        );
        Control::parse_t("uneffect *", ids)
            .unwrap()
            .apply(&mut live);
        assert!(live.effects.is_empty());
        assert_eq!(Control::parse_t("effect minecraft:nope 0 20", ids), None);
        let mut lines = text.lines();
        assert_eq!(
            lines.next(),
            Some("7.5 20 5 0 0 0 1 64 -2.5 90 0 0 0 0 0 minecraft:overworld"),
            "первая строка: присед (0/1), затем ключ измерения"
        );
        assert_eq!(lines.next(), Some("minecraft:bread 3"));
        assert_eq!(text.lines().count(), 1 + BUKKIT_SLOTS);
        assert_eq!(
            Control::parse_t("slot 3 -", ids),
            Some(Control::Slot(39, None))
        );
        assert_eq!(Control::parse_t("mode 9", ids), None);
        assert_eq!(
            Control::parse_t("tp 1 2.5 3 90 -10", ids),
            Some(Control::Teleport([1.0, 2.5, 3.0], 90.0, -10.0))
        );
        assert_eq!(Control::parse_t("tp 1 NaN 3 0 0", ids), None);
        assert_eq!(Control::parse_t("slot 0 minecraft:nope 1", ids), None);
    }

    #[test]
    fn stacks_from_lines() {
        let s = parse_stacks("minecraft:stone 5\nbad\nminecraft:nope 1\n".lines(), ids);
        assert_eq!(
            s,
            vec![Stack {
                item: 1,
                count: 5,
                damage: 0,
                extra: 0
            }]
        );
    }
}
