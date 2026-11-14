//! Мобы 26.3: мирные (`Cow`, `Pig`, `Sheep`, `Chicken`) и враждебные (`Zombie`).
//! Физика `LivingEntity.travel`, коллизия с блоками, `Mob.serverAiStep` с
//! селекторами целей ([`goals`]) и поиском пути ([`path`]), урон с
//! отбрасыванием и смерть.
//!
//! Здесь же правила естественного спавна (`NaturalSpawner`), света для
//! монстров, горения на солнце и исчезновения (`Mob.checkDespawn`): они не
//! трогают мир напрямую, а получают его через замыкания и проверяются тестами.

pub mod cube;
pub mod goals;
pub mod path;

use goals::{CellInfo, Kin, LookControl, Out, Selector};
use path::{MoveControl, Navigation};

use chunkwake_protocol::inventory::Stack;

/// `EquipmentSlot` 26.3: имя в NBT и id в `set_equipment`.
/// Порядок — как `EntityEquipment`/`DropChances` в сохранении.
pub const EQUIPMENT_SLOTS: [(&str, u8); 6] = [
    ("mainhand", 0),
    ("offhand", 5),
    ("feet", 1),
    ("legs", 2),
    ("chest", 3),
    ("head", 4),
];
/// `DropChances.DEFAULT_EQUIPMENT_DROP_CHANCE`.
pub const DEFAULT_DROP_CHANCE: f32 = 0.085;
/// `Mob.getEquipmentForSlot`: материал брони 0..=5 по частям
/// (шлем, нагрудник, штаны, ботинки).
pub const ARMOR_NAMES: [[&str; 6]; 4] = [
    [
        "minecraft:leather_helmet",
        "minecraft:copper_helmet",
        "minecraft:golden_helmet",
        "minecraft:chainmail_helmet",
        "minecraft:iron_helmet",
        "minecraft:diamond_helmet",
    ],
    [
        "minecraft:leather_chestplate",
        "minecraft:copper_chestplate",
        "minecraft:golden_chestplate",
        "minecraft:chainmail_chestplate",
        "minecraft:iron_chestplate",
        "minecraft:diamond_chestplate",
    ],
    [
        "minecraft:leather_leggings",
        "minecraft:copper_leggings",
        "minecraft:golden_leggings",
        "minecraft:chainmail_leggings",
        "minecraft:iron_leggings",
        "minecraft:diamond_leggings",
    ],
    [
        "minecraft:leather_boots",
        "minecraft:copper_boots",
        "minecraft:golden_boots",
        "minecraft:chainmail_boots",
        "minecraft:iron_boots",
        "minecraft:diamond_boots",
    ],
];

/// `Mob.populateDefaultEquipmentSlots` + `Zombie`/`AbstractSkeleton`:
/// слот [`EQUIPMENT_SLOTS`] и имя предмета. `rng` — `nextFloat()`.
pub fn random_equipment(
    species: Species,
    difficulty: u8,
    special: f32,
    rng: &mut dyn FnMut() -> f32,
) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    if rng() < 0.15 * special {
        let mut armor_type = (rng() * 3.0) as usize % 3;
        for _ in 0..3 {
            if rng() < 0.1087 {
                armor_type += 1;
            }
        }
        let partial = if difficulty == 3 { 0.1 } else { 0.25 };
        let mut first = true;
        // `EQUIPMENT_POPULATION_ORDER`: HEAD, CHEST, LEGS, FEET.
        for (n, slot) in [5usize, 4, 3, 2].into_iter().enumerate() {
            if !first && rng() < partial {
                break;
            }
            first = false;
            if let Some(name) = ARMOR_NAMES[n].get(armor_type) {
                out.push((slot, *name));
            }
        }
    }
    match species {
        Species::Zombie | Species::Drowned => {
            let chance = if difficulty == 3 { 0.05 } else { 0.01 };
            if rng() < chance {
                let name = match (rng() * 6.0) as u32 % 6 {
                    0 => "minecraft:iron_sword",
                    1 => "minecraft:iron_spear",
                    _ => "minecraft:iron_shovel",
                };
                out.push((0, name));
            }
        }
        Species::Skeleton => out.push((0, "minecraft:bow")),
        _ => {}
    }
    out
}

/// Гравитация и сопротивление по вертикали (`LivingEntity.travel`).
pub const GRAVITY: f64 = 0.08;
pub const AIR_DRAG: f64 = 0.98;
/// Трение по горизонтали: блок 0.6 × 0.91 на земле, 0.91 в воздухе.
pub const GROUND_FRICTION: f64 = 0.6 * 0.91;
pub const AIR_FRICTION: f64 = 0.91;
/// Ускорение в воздухе (`getFlyingSpeed`).
pub const AIR_ACCEL: f64 = 0.02;
/// Прыжок (`getJumpPower`).
pub const JUMP: f64 = 0.42;

/// `LivingEntity.getJumpPower` без `blockJumpFactor`: `0.42` плюс
/// `getJumpBoostPower` = `0.1 × (уровень + 1)`.
pub fn jump_power(jump_boost: Option<u8>) -> f64 {
    JUMP + jump_boost.map_or(0.0, |a| 0.1 * (f64::from(a) + 1.0))
}

/// `SAFE_FALL_DISTANCE`: база плюс `+1` за уровень Прыгучести
/// (`MobEffects.JUMP_BOOST`).
pub fn safe_fall_distance(base: f32, jump_boost: Option<u8>) -> f32 {
    base + jump_boost.map_or(0.0, |a| f32::from(a) + 1.0)
}
/// Неуязвимость после удара (`invulnerableTime`); урон проходит при > 10, только больший.
pub const INVULNERABLE: u8 = 20;
/// Анимация смерти до исчезновения (`tickDeath`).
pub const DEATH_TICKS: u16 = 20;
/// Отбрасывание от удара игрока (`knockback`).
pub const KNOCKBACK: f64 = 0.4;
/// Детёныш растёт 20 минут (`AgeableMob.BABY_START_AGE`).
pub const BABY_AGE: i32 = -24000;
/// Цель забывается, если её не видно столько тиков (`TargetGoal.unseenMemoryTicks`).
pub const UNSEEN_MEMORY: u32 = 60;
/// Пауза между ударами (`MeleeAttackGoal.resetAttackCooldown`: 20 тиков).
pub const ATTACK_INTERVAL: u32 = 20;
/// Досягаемость удара (`Mob.DEFAULT_ATTACK_REACH = sqrt(2.04) - 0.6`): на
/// столько раздувается рамка моба по горизонтали.
pub const ATTACK_REACH: f64 = 0.828_285_686_438_656_6;
/// Рост и ширина игрока и высота его глаз (`EntityType.PLAYER`).
pub const PLAYER_SIZE: (f64, f64) = (0.6, 1.8);
pub const PLAYER_EYE: f64 = 1.62;
/// `MobCategory.despawnDistance` и `noDespawnDistance`.
pub const DESPAWN_DISTANCE: f64 = 128.0;
pub const NO_DESPAWN_DISTANCE: f64 = 32.0;
/// Ближе 24 блоков к игроку спавна нет (`NaturalSpawner.isRightDistanceToPlayerAndSpawnPoint`).
pub const MIN_SPAWN_DISTANCE: f64 = 24.0;
/// Делитель лимита (`NaturalSpawner.MAGIC_NUMBER` = 17²).
pub const MOB_CAP_CHUNKS: u32 = 289;
/// Существа (`creature`) спавнятся раз в 400 тиков (`spawnFriendlies`).
pub const CREATURE_SPAWN_PERIOD: u64 = 400;
/// Без дела монстр исчезает не раньше (`Mob.checkDespawn`: `noActionTime > 600`).
pub const IDLE_DESPAWN_TICKS: u32 = 600;

/// `MobCategory`: лимит на 289 чанков и исчезает ли моб вдали от игроков.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Creature,
    Monster,
}

impl Category {
    pub const ALL: [Category; 2] = [Category::Monster, Category::Creature];

    /// `MobCategory.getMaxInstancesPerChunk`.
    pub fn max(self) -> u32 {
        match self {
            Category::Monster => 70,
            Category::Creature => 10,
        }
    }

    /// `MobCategory.isPersistent` наоборот: существа не исчезают.
    pub fn despawns(self) -> bool {
        self == Category::Monster
    }

    /// Ключ в `spawns_by_category` биома.
    pub fn key(self) -> &'static str {
        match self {
            Category::Monster => "monster",
            Category::Creature => "creature",
        }
    }
}

/// Лимит категории (`NaturalSpawner.SpawnState.canSpawnForCategoryGlobal`):
/// `max × чанки / 289`.
pub fn mob_cap(category: Category, spawnable_chunks: usize) -> usize {
    category.max() as usize * spawnable_chunks / MOB_CAP_CHUNKS as usize
}

/// Чанков для лимита (`DistanceManager.getNaturalSpawnChunkCount`): счётчик
/// `naturalSpawnChunkCounter` — `FixedPlayerDistanceChunkTracker` радиуса 8,
/// то есть квадрат 17×17 вокруг чанка каждого игрока, без повторов. От
/// дальности симуляции не зависит: один игрок всегда даёт 289 чанков и полный
/// лимит, хотя спавн идёт только в чанках симуляции.
pub fn spawn_cap_chunks(players: &[(i32, i32)]) -> usize {
    let mut chunks = std::collections::HashSet::new();
    for &(cx, cz) in players {
        for dx in -8..=8 {
            for dz in -8..=8 {
                chunks.insert((cx + dx, cz + dz));
            }
        }
    }
    chunks.len()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Species {
    Cow,
    Pig,
    Sheep,
    Chicken,
    Zombie,
    Creeper,
    Skeleton,
    Spider,
    Slime,
    Drowned,
    Piglin,
    Wolf,
    Cod,
    IronGolem,
    Phantom,
    Pillager,
    Wither,
    SnowGolem,
    CopperGolem,
    EnderDragon,
    EndCrystal,
    Parrot,
}

/// Всё о виде в одном месте. Встроенные значения — запасные: при старте
/// сервер накрывает измеримые (размер, глаза, атрибуты, категорию) выгрузкой
/// `reports/entities.csv` ([`install_species_data`]). Поведенческие
/// константы целей (паника, приманка) в выгрузку не попадают и живут здесь.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpeciesInfo {
    pub name: &'static str,
    pub category: Category,
    /// Ширина и высота взрослого (`EntityType.sized`).
    pub size: (f64, f64),
    /// `EntityDimensions.eyeHeight` взрослого.
    pub eye_height: f64,
    pub max_health: f32,
    /// Атрибут `movement_speed`.
    pub speed: f64,
    /// Атрибут `follow_range`: дальность поиска цели и узлов пути.
    pub follow_range: f64,
    /// Атрибут `attack_damage` (у животных его нет — 0).
    pub attack_damage: f32,
    pub armor: f64,
    /// Атрибут `step_height`.
    pub step_height: f64,
    /// `safe_fall_distance`: с какой высоты урон ещё не начисляется.
    pub safe_fall: f32,
    pub fire_immune: bool,
    /// Множитель скорости `PanicGoal`.
    pub panic_speed: f64,
    /// Множитель скорости `TemptGoal`.
    pub tempt_speed: f64,
    /// Множитель скорости `FollowParentGoal`.
    pub follow_parent_speed: f64,
    /// Горит на солнце (`isSunSensitive`).
    pub burns_in_sun: bool,
    /// Громкость звуков (`getSoundVolume`).
    pub volume: f32,
    /// Префикс звуков: `.ambient`, `.hurt`, `.death`, `.step`.
    /// Подаёт голос (`getAmbientSound` не `null`): у крипера его нет.
    pub ambient: bool,
    pub sound: &'static str,
    /// Таблица лута (`entities/<вид>`).
    pub loot: &'static str,
}

const fn base(name: &'static str, sound: &'static str, loot: &'static str) -> SpeciesInfo {
    SpeciesInfo {
        name,
        category: Category::Creature,
        size: (0.6, 1.8),
        eye_height: 1.8 * 0.85,
        max_health: 20.0,
        speed: 0.25,
        follow_range: 16.0,
        attack_damage: 0.0,
        armor: 0.0,
        step_height: 0.6,
        safe_fall: 3.0,
        fire_immune: false,
        panic_speed: 1.0,
        tempt_speed: 1.0,
        follow_parent_speed: 1.1,
        burns_in_sun: false,
        volume: 1.0,
        ambient: true,
        sound,
        loot,
    }
}

/// Встроенная таблица в порядке [`Species::ALL`]: числа из `createAttributes`
/// и `EntityType` 26.3.
const BUILTIN: [SpeciesInfo; 22] = [
    SpeciesInfo {
        size: (0.9, 1.4),
        eye_height: 1.3,
        max_health: 10.0,
        speed: 0.2,
        panic_speed: 2.0,
        tempt_speed: 1.25,
        follow_parent_speed: 1.25,
        volume: 0.4,
        ..base(
            "minecraft:cow",
            "minecraft:entity.cow",
            "minecraft:entities/cow",
        )
    },
    SpeciesInfo {
        size: (0.9, 0.9),
        eye_height: 0.765,
        max_health: 10.0,
        panic_speed: 1.25,
        tempt_speed: 1.2,
        ..base(
            "minecraft:pig",
            "minecraft:entity.pig",
            "minecraft:entities/pig",
        )
    },
    SpeciesInfo {
        size: (0.9, 1.3),
        eye_height: 1.235,
        max_health: 8.0,
        speed: 0.23,
        panic_speed: 1.25,
        tempt_speed: 1.1,
        ..base(
            "minecraft:sheep",
            "minecraft:entity.sheep",
            "minecraft:entities/sheep",
        )
    },
    SpeciesInfo {
        size: (0.4, 0.7),
        eye_height: 0.644,
        max_health: 4.0,
        panic_speed: 1.4,
        ..base(
            "minecraft:chicken",
            "minecraft:entity.chicken",
            "minecraft:entities/chicken",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (0.6, 1.95),
        eye_height: 1.74,
        speed: 0.23,
        follow_range: 35.0,
        attack_damage: 3.0,
        armor: 2.0,
        burns_in_sun: true,
        ..base(
            "minecraft:zombie",
            "minecraft:entity.zombie",
            "minecraft:entities/zombie",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (0.6, 1.7),
        eye_height: 1.7 * 0.85,
        attack_damage: 2.0,
        ambient: false,
        ..base(
            "minecraft:creeper",
            "minecraft:entity.creeper",
            "minecraft:entities/creeper",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (0.6, 1.99),
        eye_height: 1.74,
        attack_damage: 2.0,
        burns_in_sun: true,
        ..base(
            "minecraft:skeleton",
            "minecraft:entity.skeleton",
            "minecraft:entities/skeleton",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (1.4, 0.9),
        eye_height: 0.65,
        max_health: 16.0,
        speed: 0.3,
        attack_damage: 2.0,
        ..base(
            "minecraft:spider",
            "minecraft:entity.spider",
            "minecraft:entities/spider",
        )
    },
    // `AbstractCubeMob`: числа размера 1; дальше всё множит `Mob::cube_size`.
    SpeciesInfo {
        category: Category::Monster,
        size: (0.52, 0.52),
        eye_height: 0.325,
        max_health: 1.0,
        speed: 0.3,
        attack_damage: 1.0,
        volume: 0.4,
        ambient: false,
        ..base(
            "minecraft:slime",
            "minecraft:entity.slime",
            "minecraft:entities/slime",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (0.6, 1.95),
        eye_height: 1.74,
        speed: 0.23,
        follow_range: 35.0,
        attack_damage: 3.0,
        armor: 2.0,
        burns_in_sun: false,
        ..base(
            "minecraft:drowned",
            "minecraft:entity.drowned",
            "minecraft:entities/drowned",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (0.6, 1.95),
        eye_height: 1.79,
        max_health: 16.0,
        speed: 0.35,
        follow_range: 16.0,
        attack_damage: 0.0,
        ..base(
            "minecraft:piglin",
            "minecraft:entity.piglin",
            "minecraft:entities/piglin",
        )
    },
    SpeciesInfo {
        size: (0.6, 0.85),
        eye_height: 0.68,
        max_health: 8.0,
        speed: 0.3,
        follow_range: 16.0,
        attack_damage: 3.0,
        panic_speed: 1.5,
        ..base(
            "minecraft:wolf",
            "minecraft:entity.wolf",
            "minecraft:entities/wolf",
        )
    },
    SpeciesInfo {
        size: (0.5, 0.3),
        eye_height: 0.195,
        max_health: 3.0,
        speed: 0.0,
        follow_range: 0.0,
        ..base(
            "minecraft:cod",
            "minecraft:entity.cod",
            "minecraft:entities/cod",
        )
    },
    SpeciesInfo {
        size: (1.4, 2.7),
        eye_height: 2.295,
        max_health: 100.0,
        speed: 0.25,
        follow_range: 16.0,
        attack_damage: 15.0,
        step_height: 1.0,
        ..base(
            "minecraft:iron_golem",
            "minecraft:entity.iron_golem",
            "minecraft:entities/iron_golem",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (0.9, 0.5),
        eye_height: 0.175,
        max_health: 20.0,
        speed: 0.7,
        follow_range: 16.0,
        attack_damage: 2.0,
        ..base(
            "minecraft:phantom",
            "minecraft:entity.phantom",
            "minecraft:entities/phantom",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (0.6, 1.95),
        eye_height: 1.6575,
        max_health: 24.0,
        speed: 0.35,
        follow_range: 32.0,
        attack_damage: 5.0,
        ..base(
            "minecraft:pillager",
            "minecraft:entity.pillager",
            "minecraft:entities/pillager",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (0.9, 3.5),
        eye_height: 2.9750001,
        max_health: 300.0,
        speed: 0.6,
        follow_range: 40.0,
        attack_damage: 2.0,
        armor: 4.0,
        fire_immune: true,
        ..base(
            "minecraft:wither",
            "minecraft:entity.wither",
            "minecraft:entities/wither",
        )
    },
    SpeciesInfo {
        size: (0.7, 1.9),
        eye_height: 1.7,
        max_health: 4.0,
        speed: 0.2,
        follow_range: 16.0,
        ..base(
            "minecraft:snow_golem",
            "minecraft:entity.snow_golem",
            "minecraft:entities/snow_golem",
        )
    },
    SpeciesInfo {
        size: (0.49, 0.98),
        eye_height: 0.8125,
        max_health: 12.0,
        speed: 0.2,
        follow_range: 16.0,
        step_height: 1.0,
        ..base(
            "minecraft:copper_golem",
            "minecraft:entity.copper_golem",
            "minecraft:entities/copper_golem",
        )
    },
    SpeciesInfo {
        category: Category::Monster,
        size: (16.0, 8.0),
        eye_height: 6.8,
        max_health: 200.0,
        speed: 0.7,
        follow_range: 16.0,
        fire_immune: true,
        ..base(
            "minecraft:ender_dragon",
            "minecraft:entity.ender_dragon",
            "minecraft:entities/ender_dragon",
        )
    },
    SpeciesInfo {
        size: (2.0, 2.0),
        eye_height: 1.7,
        max_health: 5.0,
        fire_immune: true,
        ..base(
            "minecraft:end_crystal",
            "minecraft:entity.end_crystal",
            "minecraft:entities/end_crystal",
        )
    },
    SpeciesInfo {
        size: (0.5, 0.9),
        eye_height: 0.54,
        max_health: 6.0,
        speed: 0.2,
        follow_range: 16.0,
        attack_damage: 3.0,
        ..base(
            "minecraft:parrot",
            "minecraft:entity.parrot",
            "minecraft:entities/parrot",
        )
    },
];

static INSTALLED: std::sync::OnceLock<Vec<SpeciesInfo>> = std::sync::OnceLock::new();

/// Накрыть встроенную таблицу выгрузкой `server.jar`. Вызывается один раз
/// при старте; без выгрузки (тесты) действуют встроенные значения.
pub fn install_species_data(types: &[chunkwake_data::world::EntityTypeInfo]) {
    let _ = INSTALLED.set(BUILTIN.iter().map(|b| merge(b, types)).collect());
}

/// Встроенные данные вида, накрытые строкой выгрузки с тем же именем.
fn merge(b: &SpeciesInfo, types: &[chunkwake_data::world::EntityTypeInfo]) -> SpeciesInfo {
    let Some(t) = types.iter().find(|t| t.name == b.name) else {
        return *b;
    };
    let mut s = *b;
    s.size = (f64::from(t.width), f64::from(t.height));
    s.eye_height = f64::from(t.eye_height);
    s.fire_immune = t.fire_immune;
    s.category = match t.category.as_str() {
        "monster" => Category::Monster,
        _ => Category::Creature,
    };
    if let Some(a) = t.attributes {
        s.max_health = a.max_health as f32;
        s.speed = a.movement_speed;
        s.follow_range = a.follow_range;
        s.attack_damage = a.attack_damage.unwrap_or(0.0) as f32;
        s.armor = a.armor;
        s.step_height = a.step_height;
        s.safe_fall = a.safe_fall_distance as f32;
    }
    s
}

impl Species {
    pub const ALL: [Species; 22] = [
        Species::Cow,
        Species::Pig,
        Species::Sheep,
        Species::Chicken,
        Species::Zombie,
        Species::Creeper,
        Species::Skeleton,
        Species::Spider,
        Species::Slime,
        Species::Drowned,
        Species::Piglin,
        Species::Wolf,
        Species::Cod,
        Species::IronGolem,
        Species::Phantom,
        Species::Pillager,
        Species::Wither,
        Species::SnowGolem,
        Species::CopperGolem,
        Species::EnderDragon,
        Species::EndCrystal,
        Species::Parrot,
    ];

    /// Данные вида: выгрузка, если она установлена, иначе встроенные.
    pub fn info(self) -> &'static SpeciesInfo {
        let i = self as usize;
        INSTALLED.get().map_or(&BUILTIN[i], |t| &t[i])
    }

    pub fn name(self) -> &'static str {
        BUILTIN[self as usize].name
    }

    /// Категория спавна (`EntityType.getCategory`).
    pub fn category(self) -> Category {
        self.info().category
    }

    pub fn is_monster(self) -> bool {
        self.category() == Category::Monster
    }

    /// Животное: растёт, любит, плодится (`Animal`). Детёныш зомби не растёт.
    pub fn is_animal(self) -> bool {
        !self.is_monster()
    }

    /// `EntityTypeTags.UNDEAD`: лечение вредит, урон лечит.
    pub fn is_undead(self) -> bool {
        matches!(self, Species::Zombie | Species::Skeleton | Species::Drowned)
    }

    pub fn attack_damage(self) -> f32 {
        self.info().attack_damage
    }

    pub fn follow_range(self) -> f64 {
        self.info().follow_range
    }

    pub fn burns_in_sun(self) -> bool {
        self.info().burns_in_sun
    }

    pub fn eye_height(self) -> f64 {
        self.info().eye_height
    }

    /// Опыт за убийство игроком: у животных 1–3 (`Animal.getBaseExperienceReward`),
    /// у монстров 5 (`Monster.xpReward`), у детёныша зомби ×2.5
    /// (`Zombie.getBaseExperienceReward`).
    pub fn xp(self, baby: bool, r: f64) -> i32 {
        if !self.is_monster() {
            1 + (r * 3.0).floor().min(2.0) as i32
        } else if baby {
            (5.0 * 2.5) as i32
        } else {
            5
        }
    }

    /// Имя без пространства (`cow`) — для плагинов.
    pub fn short(self) -> &'static str {
        &self.name()[10..]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Self::ALL.into_iter().find(|s| s.short() == name)
    }

    pub fn size(self) -> (f64, f64) {
        self.info().size
    }

    pub fn max_health(self) -> f32 {
        self.info().max_health
    }

    pub fn speed(self) -> f64 {
        self.info().speed
    }

    pub fn safe_fall(self) -> f32 {
        self.info().safe_fall
    }

    pub fn panic_speed(self) -> f64 {
        self.info().panic_speed
    }

    pub fn tempt_speed(self) -> f64 {
        self.info().tempt_speed
    }

    /// Еда (`isFood`: теги `cow_food`, `pig_food`, `sheep_food`, `chicken_food`).
    pub fn is_food(self, item: &str) -> bool {
        let item = item.strip_prefix("minecraft:").unwrap_or(item);
        match self {
            Species::Cow | Species::Sheep => item == "wheat",
            Species::Pig => matches!(item, "carrot" | "potato" | "beetroot"),
            Species::Chicken => matches!(
                item,
                "wheat_seeds"
                    | "melon_seeds"
                    | "pumpkin_seeds"
                    | "beetroot_seeds"
                    | "torchflower_seeds"
                    | "pitcher_pod"
            ),
            _ => false,
        }
    }

    pub fn sound(self) -> &'static str {
        self.info().sound
    }

    pub fn volume(self) -> f32 {
        self.info().volume
    }

    pub fn loot(self) -> &'static str {
        self.info().loot
    }
}

/// `WeatheringCopper.WeatherState`: стадии окисления медного голема.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeatherState {
    #[default]
    Unaffected,
    Exposed,
    Weathered,
    Oxidized,
}

impl WeatherState {
    pub fn name(self) -> &'static str {
        match self {
            WeatherState::Unaffected => "unaffected",
            WeatherState::Exposed => "exposed",
            WeatherState::Weathered => "weathered",
            WeatherState::Oxidized => "oxidized",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "unaffected" => WeatherState::Unaffected,
            "exposed" => WeatherState::Exposed,
            "weathered" => WeatherState::Weathered,
            "oxidized" => WeatherState::Oxidized,
            _ => return None,
        })
    }

    /// `WeatheringCopper.WeatherState` id для `set_entity_data`.
    pub fn id(self) -> i32 {
        match self {
            WeatherState::Unaffected => 0,
            WeatherState::Exposed => 1,
            WeatherState::Weathered => 2,
            WeatherState::Oxidized => 3,
        }
    }

    pub fn next(self) -> Self {
        match self {
            WeatherState::Unaffected => WeatherState::Exposed,
            WeatherState::Exposed => WeatherState::Weathered,
            WeatherState::Weathered | WeatherState::Oxidized => WeatherState::Oxidized,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mob {
    pub species: Species,
    pub uuid: u128,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub head_yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub on_ground: bool,
    /// Остаток неуязвимости и последний прошедший урон.
    pub invulnerable: u8,
    pub last_damage: f32,
    /// > 0 — умирает (тики анимации).
    pub death_time: u16,
    /// Возраст: < 0 — детёныш; > 0 — пауза после размножения.
    pub age: i32,
    /// Режим любви (`inLove`), тики.
    pub love: i32,
    pub sheared: bool,
    /// Цвет шерсти (`DyeColor` id).
    pub color: u8,
    /// `goalSelector` и `targetSelector`.
    pub goals: Selector,
    pub targets: Selector,
    /// `tickCount`.
    pub tick_count: u32,
    /// `LookControl`.
    pub look_ctl: LookControl,
    /// `Mob.isAggressive` (зомби поднимает руки).
    pub aggressive: bool,
    /// `lastHurtByMob` и его `tickCount` (держится 100 тиков).
    pub last_hurt_by: Option<(u128, u32)>,
    /// `lastDamageSource`: `tickCount` удара и входит ли он в `panic_causes`.
    pub last_damage_source: Option<(u32, bool)>,
    /// События тика для сервера ([`Out`]). Сбрасываются в начале тика.
    pub out: Vec<Out>,
    /// Курица: тиков до яйца.
    pub egg_time: i32,
    /// Пользовательское имя (JSON) от плагинов.
    pub custom_name: Option<String>,
    /// Цель монстра — UUID игрока (`Mob.getTarget`).
    pub target: Option<u128>,
    /// Удар этого тика: кого бить. Забирает и сбрасывает сервер.
    pub strike: Option<u128>,
    /// Слизень: все игроки, которых он коснулся за тик (`playerTouch`).
    pub touch_strikes: Vec<u128>,
    /// Тики без дела (`noActionTime`) — для исчезновения.
    pub no_action: u32,
    /// Не исчезает (`persistenceRequired`): с яйца, из команды, от плагина.
    pub persistent: bool,
    /// `Zombie.canBreakDoors`: ломает деревянные двери (`BreakDoorGoal`).
    pub can_break_doors: bool,
    /// `Mob.isNoAi`: без целей, навигации и управления движением, но с
    /// физикой, гравитацией и получением урона.
    pub no_ai: bool,
    /// Поводок (`Leashable`): держатель-сущность или узел на заборе.
    pub leash: Option<crate::leash::Leash>,
    /// `Entity.horizontalCollision` прошлого хода: условие `DoorInteractGoal`.
    pub horizontal_collision: bool,
    /// `TameableAnimal.isTame` (`Tame` в NBT); владелец - UUID игрока.
    pub tamed: bool,
    pub owner: Option<u128>,
    /// Навигация по пути (`GroundPathNavigation`).
    pub nav: Navigation,
    /// `MoveControl`.
    pub move_ctl: MoveControl,
    /// `LivingEntity.speed` (ставит `MoveControl`) и ход вперёд `zza`.
    pub move_speed: f64,
    pub zza: f64,
    /// `JumpControl.jump` этого тика и `noJumpDelay`.
    pub jump: bool,
    pub no_jump_delay: u8,
    /// Ход вбок `xxa` (`MoveControl.strafe`).
    pub xxa: f64,
    /// Паук лезет по стене (`Spider.isClimbing`, бит 1 `DATA_FLAGS_ID`).
    pub climbing: bool,
    /// Горит (`isOnFire`): ставит сервер.
    pub on_fire: bool,
    /// `active_effects`: зелья и эффекты на мобе.
    pub effects: Vec<crate::survival::Effect>,
    /// `EntityEquipment`: mainhand, offhand, feet, legs, chest, head.
    pub equipment: [Option<Stack>; 6],
    /// `EquipmentSlot.BODY` (`DataComponents.BODY_ARMOR`): волчья броня.
    pub body_armor: Option<Stack>,
    /// `Wolf.isInSittingPose` (`DATA_FLAGS_ID`, бит 0x01).
    pub sitting: bool,
    /// `Entity.isNoGravity` (`NoGravity`): кристаллы/визер/дракон не падают.
    pub no_gravity: bool,
    /// `CopperGolem.getWeatherState` (`weather_state`): стадия окисления.
    pub weather_state: WeatherState,
    /// `CopperGolem.nextWeatheringTick` (`next_weather_age`): -1 - не задан,
    /// -2 - воск (не окисляется), иначе тик следующей стадии.
    pub next_weather_age: i64,
    /// Счётчик тиков жизни (для `next_weather_age`, в NBT не пишется).
    pub ticks: u64,
    /// `CopperGolem`: на этом тике голем стал статуей (удаляется из мира).
    pub statue: bool,
    /// `EnderDragon`: кулдаун контактного урона (тиков).
    pub dragon_cooldown: u8,
    /// `DropChances` по тем же слотам; > 1 — предмет сохраняется всегда.
    pub drop_chances: [f32; 6],
    /// Множитель скорости от `Speed`/`Slowness` (пересчитывается в тике).
    pub effect_speed: f64,
    /// Штраф урона от `Weakness`.
    pub effect_weakness: f32,
    /// Крипер: `swell`, `swellDir`, `maxSwell`, `explosionRadius`, заряжен, подожжён.
    pub swell: i32,
    pub swell_dir: i32,
    pub max_swell: i32,
    pub explosion_radius: i32,
    pub powered: bool,
    pub ignited: bool,
    /// Скелет: тики натяжения лука (`getTicksUsingItem`); `None` — не тянет.
    pub using_item: Option<i32>,
    /// Слизень: размер (`AbstractCubeMob.ID_SIZE`, 1..127), прошлое
    /// касание земли (`wasOnGround`) и `CubeMobMoveControl`.
    pub cube_size: i32,
    pub was_on_ground: bool,
    pub cube: cube::CubeMove,
}

impl Mob {
    pub fn new(species: Species, pos: [f64; 3], uuid: u128) -> Self {
        let (goals, targets) = goals::for_species(species);
        Self {
            species,
            uuid,
            pos,
            vel: [0.0; 3],
            yaw: 0.0,
            head_yaw: 0.0,
            pitch: 0.0,
            health: species.max_health(),
            on_ground: false,
            invulnerable: 0,
            last_damage: 0.0,
            death_time: 0,
            age: 0,
            love: 0,
            sheared: false,
            color: 0,
            goals,
            targets,
            tick_count: 0,
            look_ctl: LookControl::default(),
            aggressive: false,
            last_hurt_by: None,
            last_damage_source: None,
            out: Vec::new(),
            egg_time: 6000,
            custom_name: None,
            target: None,
            strike: None,
            touch_strikes: Vec::new(),
            no_action: 0,
            persistent: false,
            can_break_doors: false,
            no_ai: false,
            leash: None,
            horizontal_collision: false,
            tamed: false,
            owner: None,
            no_gravity: false,
            weather_state: WeatherState::Unaffected,
            next_weather_age: -1,
            ticks: 0,
            statue: false,
            dragon_cooldown: 0,
            nav: Navigation::new(species == Species::Spider),
            move_ctl: MoveControl::default(),
            move_speed: 0.0,
            zza: 0.0,
            jump: false,
            no_jump_delay: 0,
            xxa: 0.0,
            climbing: false,
            on_fire: false,
            effects: Vec::new(),
            equipment: [None; 6],
            body_armor: None,
            sitting: false,
            drop_chances: [DEFAULT_DROP_CHANCE; 6],
            effect_speed: 1.0,
            effect_weakness: 0.0,
            swell: 0,
            swell_dir: -1,
            max_swell: 30,
            explosion_radius: 3,
            powered: false,
            ignited: false,
            using_item: None,
            cube_size: 1,
            was_on_ground: false,
            cube: cube::CubeMove::default(),
        }
    }

    /// Скорость с поправкой детёныша зомби (`Zombie.SPEED_MODIFIER_BABY`: +50 % базы).
    pub fn speed(&self) -> f64 {
        let base = if self.species == Species::Slime {
            cube::speed(self.cube_size)
        } else if matches!(self.species, Species::Zombie | Species::Drowned) && self.is_baby() {
            self.species.speed() * 1.5
        } else {
            self.species.speed()
        };
        base * self.effect_speed
    }

    /// Высота глаз над ногами с учётом детёныша.
    pub fn eye_height(&self) -> f64 {
        let eye = self.species.eye_height();
        if self.species == Species::Slime {
            return eye * f64::from(self.cube_size);
        }
        if self.is_baby() { eye * 0.5 } else { eye }
    }

    pub fn eye(&self) -> [f64; 3] {
        [self.pos[0], self.pos[1] + self.eye_height(), self.pos[2]]
    }

    pub fn is_baby(&self) -> bool {
        self.age < 0
    }

    /// `Mob.setSpeed`: скорость и ход вперёд.
    pub fn set_speed(&mut self, s: f32) {
        self.move_speed = f64::from(s);
        self.zza = f64::from(s);
    }

    /// Бежит ли в панике (`PanicGoal.isRunning`, `Mob.isPanicking`).
    pub fn panicking(&self) -> bool {
        self.goals
            .running(|g| matches!(g, goals::Goal::Panic { .. }))
    }

    /// Сородич для соседей по тику ([`goals::Kin`]).
    pub fn kin(&self, id: i32) -> Kin {
        Kin {
            id,
            species: self.species,
            pos: self.pos,
            age: self.age,
            in_love: self.love > 0,
            panicking: self.panicking(),
        }
    }

    pub fn alive(&self) -> bool {
        self.death_time == 0 && self.health > 0.0
    }

    /// Ширина и высота с учётом детёныша (`getScale` 0.5).
    pub fn size(&self) -> (f64, f64) {
        let (w, h) = self.species.size();
        if self.species == Species::Slime {
            let k = f64::from(self.cube_size);
            return (w * k, h * k);
        }
        if self.is_baby() {
            (w * 0.5, h * 0.5)
        } else {
            (w, h)
        }
    }

    /// Атрибут `max_health`: у слизня — квадрат размера (`setSize`).
    pub fn max_health(&self) -> f32 {
        if self.species == Species::Slime {
            (self.cube_size * self.cube_size) as f32
        } else {
            self.species.max_health()
        }
    }

    /// Атрибут `attack_damage`: у слизня - размер.
    pub fn attack_damage(&self) -> f32 {
        let base = if self.species == Species::Slime {
            self.cube_size as f32
        } else {
            self.species.attack_damage()
        };
        (base - self.effect_weakness).max(0.0)
    }

    /// `LivingEntity.setItemSlot` по индексу [`EQUIPMENT_SLOTS`].
    pub fn set_item(&mut self, slot: usize, item: Option<Stack>) {
        if let Some(s) = self.equipment.get_mut(slot) {
            *s = item.filter(|s| s.count > 0);
        }
    }

    pub fn item(&self, slot: usize) -> Option<Stack> {
        self.equipment.get(slot).copied().flatten()
    }

    /// `DropChances.byEquipment`.
    pub fn drop_chance(&self, slot: usize) -> f32 {
        self.drop_chances
            .get(slot)
            .copied()
            .unwrap_or(DEFAULT_DROP_CHANCE)
    }

    /// `LivingEntity.addEffect`: сильнее — заменяет, равный — продлевает.
    pub fn add_effect(&mut self, effect: crate::survival::Effect) {
        if let Some(old) = self.effects.iter_mut().find(|e| e.id == effect.id) {
            if effect.amplifier > old.amplifier
                || (effect.amplifier == old.amplifier && effect.duration > old.duration)
            {
                *old = effect;
            }
        } else {
            self.effects.push(effect);
        }
    }

    pub fn has_effect(&self, id: i32) -> bool {
        self.effects.iter().any(|e| e.id == id)
    }

    /// `LivingEntity.tickEffects`: лечение, яд, иссушение и истечение срока.
    pub fn tick_effects(
        &mut self,
        kinds: crate::survival::EffectKinds,
    ) -> crate::survival::EffectTick {
        use crate::survival::effect_due;
        let mut out = crate::survival::EffectTick::default();
        let mut gone = Vec::new();
        for e in &mut self.effects {
            if kinds.regeneration == Some(e.id) && effect_due(e.duration, e.age, 50, e.amplifier) {
                out.heal += 1.0;
            }
            if kinds.poison == Some(e.id) && effect_due(e.duration, e.age, 25, e.amplifier) {
                out.poison = true;
            }
            if kinds.wither == Some(e.id) && effect_due(e.duration, e.age, 40, e.amplifier) {
                out.wither = true;
            }
            if e.duration < 0 {
                e.age = e.age.wrapping_add(1);
            } else if e.duration > 0 {
                if e.duration == 1 {
                    gone.push(*e);
                }
                e.duration -= 1;
            }
        }
        if !gone.is_empty() {
            self.effects.retain(|e| e.duration != 0);
        }
        // Атрибуты движения и урона: `add_multiplied_total` +20%/−15%,
        // `Weakness` — `add_value` −4 за уровень.
        let mut factor = 1.0;
        let mut weak = 0.0f32;
        for e in &self.effects {
            let level = f64::from(e.amplifier) + 1.0;
            if kinds.speed == Some(e.id) {
                factor += 0.2 * level;
            }
            if kinds.slowness == Some(e.id) {
                factor -= 0.15 * level;
            }
            if kinds.weakness == Some(e.id) {
                weak = weak.max(4.0 * level as f32);
            }
        }
        self.effect_speed = factor.max(0.0);
        self.effect_weakness = weak;
        out.expired = gone;
        out
    }

    /// Опыт за убийство игроком: у слизня `xpReward` = размер.
    pub fn xp(&self, r: f64) -> i32 {
        if self.species == Species::Slime {
            self.cube_size
        } else {
            self.species.xp(self.is_baby(), r)
        }
    }

    /// `getSoundVolume`: у слизня 0.4 × размер.
    pub fn volume(&self) -> f32 {
        if self.species == Species::Slime {
            0.4 * self.cube_size as f32
        } else {
            self.species.volume()
        }
    }

    /// Звук `kind` (`hurt`, `death`): у маленького слизня — `<kind>_small`.
    pub fn sound(&self, kind: &str) -> String {
        if self.species == Species::CopperGolem {
            // `CopperGolemOxidationLevels`: у weathered/oxidized свои звуки.
            let base = match self.weather_state {
                WeatherState::Weathered => "minecraft:entity.copper_golem_weathered",
                WeatherState::Oxidized => "minecraft:entity.copper_golem_oxidized",
                _ => "minecraft:entity.copper_golem",
            };
            return format!("{base}.{kind}");
        }
        if self.species == Species::Slime && self.cube_size <= 1 {
            format!("{}.{kind}_small", self.species.sound())
        } else {
            format!("{}.{kind}", self.species.sound())
        }
    }

    /// `AbstractCubeMob.setSize`: размер 1..127, здоровье не выше нового
    /// максимума, `heal` — полное.
    pub fn set_size(&mut self, size: i32, heal: bool) {
        self.cube_size = size.clamp(1, 127);
        let max = self.max_health();
        self.health = if heal { max } else { self.health.min(max) };
    }

    /// Попадает ли луч в рамку моба (для подбора цели по ближайшему).
    pub fn contains(&self, p: [f64; 3]) -> bool {
        let (w, h) = self.size();
        (p[0] - self.pos[0]).abs() <= w / 2.0
            && (p[2] - self.pos[2]).abs() <= w / 2.0
            && p[1] >= self.pos[1]
            && p[1] <= self.pos[1] + h
    }
}

/// Мир для моба: коробки коллизии блока (`getCollisionShape`, в долях блока
/// `x0 y0 z0 x1 y1 z1`) и вода ли в нём.
///
/// У замыкания формы два независимых времени жизни: `'s` — сама ссылка на
/// замыкание, `'r` — возвращаемый срез. Слить их в одно нельзя: замыкание
/// обычно возвращает `&'static` (готовые константы форм), и тогда требование
/// «ссылка живёт столько же» запрещало бы брать замыкание из локальной
/// переменной.
pub struct World<'a, 's, 'r> {
    pub shape: &'s dyn Fn([i32; 3]) -> &'r [[f64; 6]],
    pub water: &'a dyn Fn([i32; 3]) -> bool,
    /// Клетки для поиска пути; `None` — по форме и воде ([`path::cell`]).
    pub cell: Option<&'a dyn Fn([i32; 3]) -> path::Cell>,
    /// Трава и яркость клетки для прогулок и еды овцы; `None` — яркость 0.5
    /// и травы нет.
    pub info: Option<&'a dyn Fn([i32; 3]) -> CellInfo>,
    /// Нижняя граница мира.
    pub min_y: i32,
}

/// Полный куб.
pub const FULL: &[[f64; 6]] = &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];

/// `maxUpStep` сухопутных мобов.
const STEP: f64 = 0.6;

/// Окружение для ИИ.
///
/// `players` — все игроки мира (на них смотрят), `prey` — те, кого можно
/// атаковать (не в творческом и не наблюдатели), `tempters` — ноги игроков с
/// едой этого вида в руке (отфильтрованы сервером раз на вид за тик, а не на
/// моба), `kin` — животные рядом для пар и детёнышей, `sight` — прямая
/// видимость между двумя точками (`Sensing.hasLineOfSight`, луч `COLLIDER`).
pub struct Senses<'a> {
    pub players: &'a [Prey],
    pub rand: &'a mut dyn FnMut() -> f64,
    pub prey: &'a [Prey],
    pub tempters: &'a [[f64; 3]],
    pub kin: &'a [Kin],
    pub sight: &'a dyn Fn([f64; 3], [f64; 3]) -> bool,
    /// Сложность (`Difficulty.getId`): глубина падения с целью.
    pub difficulty: u8,
    /// Id сущности: чётность полного прохода селекторов.
    pub entity_id: i32,
    /// `Level.isBrightOutside`: день без грозы (`skyDarken < 4`).
    pub bright_outside: bool,
    /// `GameRules.MOB_GRIEFING`: ломание дверей.
    pub mob_griefing: bool,
    /// Уровень `minecraft:jump_boost` из `LivingEntity.getJumpBoostPower`.
    pub jump_boost: Option<u8>,
}

/// Игрок, которого монстр может выбрать целью.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prey {
    pub uuid: u128,
    /// Ноги.
    pub pos: [f64; 3],
}

impl Prey {
    pub fn eye(&self) -> [f64; 3] {
        [self.pos[0], self.pos[1] + PLAYER_EYE, self.pos[2]]
    }
}

/// Видно всё: мирным мобам луч не нужен.
pub fn always_visible(_: [f64; 3], _: [f64; 3]) -> bool {
    true
}

/// Рамка моба, раздутая на досягаемость, задевает рамку игрока
/// (`Mob.isWithinMeleeAttackRange`: `getAttackBoundingBox().intersects(getHitbox())`).
pub fn in_reach(m: &Mob, p: &Prey) -> bool {
    let (w, h) = m.size();
    let half = w / 2.0 + ATTACK_REACH;
    let ph = PLAYER_SIZE.0 / 2.0;
    (m.pos[0] - p.pos[0]).abs() < half + ph
        && (m.pos[2] - p.pos[2]).abs() < half + ph
        && m.pos[1] < p.pos[1] + PLAYER_SIZE.1
        && p.pos[1] < m.pos[1] + h
}

const EPS: f64 = 1e-7;

/// Коробки блоков, задевающих область `lo..hi`, в мировых координатах. Снизу
/// берётся лишний слой: забор и стена выше своего блока.
fn colliders(lo: [f64; 3], hi: [f64; 3], w: &World) -> Vec<[f64; 6]> {
    let mut out = Vec::new();
    let r = |v: f64| (v - EPS).floor() as i32;
    for x in r(lo[0])..=r(hi[0] + 2.0 * EPS) {
        for y in r(lo[1]) - 1..=r(hi[1] + 2.0 * EPS) {
            for z in r(lo[2])..=r(hi[2] + 2.0 * EPS) {
                for b in (w.shape)([x, y, z]) {
                    let a = [
                        b[0] + x as f64,
                        b[1] + y as f64,
                        b[2] + z as f64,
                        b[3] + x as f64,
                        b[4] + y as f64,
                        b[5] + z as f64,
                    ];
                    if a[3] > lo[0]
                        && a[0] < hi[0]
                        && a[4] > lo[1]
                        && a[1] < hi[1]
                        && a[5] > lo[2]
                        && a[2] < hi[2]
                    {
                        out.push(a);
                    }
                }
            }
        }
    }
    out
}

/// `Shapes.collide`: сколько коробка `lo..hi` пройдёт по оси `axis` на `d`.
fn collide(lo: [f64; 3], hi: [f64; 3], axis: usize, mut d: f64, boxes: &[[f64; 6]]) -> f64 {
    if d == 0.0 {
        return 0.0;
    }
    let (o1, o2) = ((axis + 1) % 3, (axis + 2) % 3);
    for b in boxes {
        let overlaps = |o: usize| b[o + 3] - EPS > lo[o] && b[o] + EPS < hi[o];
        if !overlaps(o1) || !overlaps(o2) {
            continue;
        }
        if d > 0.0 && b[axis] >= hi[axis] - EPS {
            d = d.min(b[axis] - hi[axis]);
        } else if d < 0.0 && b[axis + 3] <= lo[axis] + EPS {
            d = d.max(b[axis + 3] - lo[axis]);
        }
    }
    d
}

/// `Entity.collideWithShapes`: оси по `Direction.axisStepOrder` — Y, затем
/// большая из X и Z.
fn collide_all(lo: [f64; 3], hi: [f64; 3], mv: [f64; 3], boxes: &[[f64; 6]]) -> [f64; 3] {
    let order = if mv[0].abs() < mv[2].abs() {
        [1, 2, 0]
    } else {
        [1, 0, 2]
    };
    let mut done = [0.0; 3];
    for axis in order {
        let l = [lo[0] + done[0], lo[1] + done[1], lo[2] + done[2]];
        let h = [hi[0] + done[0], hi[1] + done[1], hi[2] + done[2]];
        done[axis] = collide(l, h, axis, mv[axis], boxes);
    }
    done
}

/// `Entity.collide` с подъёмом на ступень `step`.
fn collide_move(
    pos: [f64; 3],
    half: f64,
    h: f64,
    mv: [f64; 3],
    on_ground: bool,
    step: f64,
    w: &World,
) -> [f64; 3] {
    let lo = [pos[0] - half, pos[1], pos[2] - half];
    let hi = [pos[0] + half, pos[1] + h, pos[2] + half];
    let span = |a: [f64; 3], b: [f64; 3], m: [f64; 3]| {
        let l = [
            a[0] + m[0].min(0.0),
            a[1] + m[1].min(0.0),
            a[2] + m[2].min(0.0),
        ];
        let u = [
            b[0] + m[0].max(0.0),
            b[1] + m[1].max(0.0),
            b[2] + m[2].max(0.0),
        ];
        (l, u)
    };
    let (l, u) = span(lo, hi, mv);
    let got = collide_all(lo, hi, mv, &colliders(l, u, w));
    let landed = got[1] != mv[1] && mv[1] < 0.0;
    if step <= 0.0 || !(landed || on_ground) || (got[0] == mv[0] && got[2] == mv[2]) {
        return got;
    }
    let shift = if landed { got[1] } else { 0.0 };
    let glo = [lo[0], lo[1] + shift, lo[2]];
    let ghi = [hi[0], hi[1] + shift, hi[2]];
    let (mut l, u) = span(glo, ghi, [mv[0], step, mv[2]]);
    if !landed {
        l[1] -= 1.0e-5f32 as f64;
    }
    let boxes = colliders(l, u, w);
    // `collectCandidateStepUpHeights`: верхние и нижние грани коробок от пола.
    let skip = got[1] as f32;
    let mut heights: Vec<f32> = Vec::new();
    for b in &boxes {
        for y in [b[1], b[4]] {
            let rel = (y - glo[1]) as f32;
            if rel >= 0.0 && rel != skip && rel as f64 <= step && !heights.contains(&rel) {
                heights.push(rel);
            }
        }
    }
    heights.sort_by(f32::total_cmp);
    let flat = got[0] * got[0] + got[2] * got[2];
    for h in heights {
        let up = collide_all(glo, ghi, [mv[0], h as f64, mv[2]], &boxes);
        if up[0] * up[0] + up[2] * up[2] > flat {
            return [up[0], up[1] + shift, up[2]];
        }
    }
    got
}

/// `LivingEntity.getFrictionInfluencedSpeed`: на обычном блоке (`friction` 0.6,
/// `friction_modifier` 1.0 → `f = 0.6`) это `speed * 0.216 / f³`, то есть `speed`.
/// На скользком блоке (лёд, `f` больше 0.6) скорость выше.
fn friction_speed(speed: f64, friction: f64) -> f64 {
    if friction > 0.6 {
        speed * 0.21600002 / (friction * friction * friction)
    } else {
        speed
    }
}

fn in_water(m: &Mob, w: &World) -> bool {
    (w.water)([
        m.pos[0].floor() as i32,
        (m.pos[1] + 0.1).floor() as i32,
        m.pos[2].floor() as i32,
    ])
}

/// `Entity.isInLiquid`: вода или лава в клетке ног. Для `CubeMobFloatGoal` и
/// `CubeMobRandomDirectionGoal` ваниль проверяет именно жидкость, а не только воду.
fn in_liquid(m: &Mob, w: &World) -> bool {
    if in_water(m, w) {
        return true;
    }
    let p = [
        m.pos[0].floor() as i32,
        (m.pos[1] + 0.1).floor() as i32,
        m.pos[2].floor() as i32,
    ];
    w.cell.is_some_and(|f| f(p).kind == path::PathType::Lava)
}

/// Один тик движения (`LivingEntity.travel` + `Entity.move`): `forward` — ускорение вдоль `yaw`.
/// Возвращает, упёрся ли моб по горизонтали.
pub fn travel(m: &mut Mob, forward: f64, w: &World) -> bool {
    let water = in_water(m, w);
    // `moveRelative(amount, (xxa, 0, zza))`: в воде 0.02; на земле
    // `getFrictionInfluencedSpeed` — скорость моба (`Mob.setSpeed` ставит и
    // `zza`, поэтому на земле ускорение — скорость в квадрате: корова с
    // `movement_speed` 0.2 разгоняется на 0.04 за тик); в воздухе 0.02.
    let amount = if water {
        0.02
    } else if m.on_ground {
        friction_speed(m.move_speed, GROUND_FRICTION)
    } else {
        AIR_ACCEL
    };
    let (mut ix, mut iz) = (m.xxa, forward);
    let l2 = ix * ix + iz * iz;
    if l2 >= 1.0e-7 {
        if l2 > 1.0 {
            let l = l2.sqrt();
            ix /= l;
            iz /= l;
        }
        ix *= amount;
        iz *= amount;
        // `Entity.getInputVector`: угол во float и табличные `Mth.sin`/`Mth.cos`.
        let yaw = f64::from(m.yaw * 0.017453292f32);
        let (sin, cos) = (
            f64::from(chunkwake_worldgen::carver::sin(yaw)),
            f64::from(chunkwake_worldgen::carver::cos(yaw)),
        );
        m.vel[0] += ix * cos - iz * sin;
        m.vel[2] += iz * cos + ix * sin;
    }
    // `handleOnClimbable`: паук на стене не разгоняется и не падает быстро.
    let climb = m.species == Species::Spider && m.climbing;
    if climb && !water {
        m.vel[0] = m.vel[0].clamp(-0.15, 0.15);
        m.vel[2] = m.vel[2].clamp(-0.15, 0.15);
        m.vel[1] = m.vel[1].max(-0.15);
    }

    let (wd, h) = m.size();
    let half = wd / 2.0;
    let want = m.vel;
    let [dx, dy, dz] = collide_move(m.pos, half, h, want, m.on_ground, STEP, w);
    m.pos = [m.pos[0] + dx, m.pos[1] + dy, m.pos[2] + dz];
    m.on_ground = want[1] < 0.0 && dy != want[1];
    let blocked = (dx - want[0]).abs() > EPS || (dz - want[2]).abs() > EPS;
    if (dy - want[1]).abs() > EPS {
        m.vel[1] = 0.0;
    }
    if (dx - want[0]).abs() > EPS {
        m.vel[0] = 0.0;
    }
    if (dz - want[2]).abs() > EPS {
        m.vel[2] = 0.0;
    }
    // Лезет вверх, пока упирается (или прыгает).
    if climb && (blocked || (m.jump && !water)) {
        m.vel[1] = 0.2;
    }
    if water {
        // Всплывают сами только с `FloatGoal` (прыжок в жидкости, `tick`).
        m.vel = [m.vel[0] * 0.8, m.vel[1] * 0.8 - 0.02, m.vel[2] * 0.8];
    } else {
        let f = if m.on_ground {
            GROUND_FRICTION
        } else {
            AIR_FRICTION
        };
        if !m.no_gravity {
            m.vel[1] = (m.vel[1] - GRAVITY) * AIR_DRAG;
        }
        // Курица планирует (`Chicken.aiStep`).
        if m.species == Species::Chicken && !m.on_ground && m.vel[1] < 0.0 {
            m.vel[1] *= 0.6;
        }
        m.vel[0] *= f;
        m.vel[2] *= f;
    }
    blocked
}

fn wrap(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

pub fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Моб для поиска пути.
pub fn walker(m: &Mob, w: &World, difficulty: u8, in_water: bool) -> path::Walker {
    let (width, height) = m.size();
    let animal = m.species.is_animal();
    path::Walker {
        pos: m.pos,
        width,
        height,
        on_ground: m.on_ground,
        in_water,
        step: m.species.info().step_height as f32,
        max_fall: path::max_fall_distance(m.target.is_some(), m.health, m.max_health(), difficulty),
        speed: m.move_speed as f32,
        // `FloatGoal` ставит `setCanFloat(true)`: его нет у зомби и скелета.
        can_float: !matches!(m.species, Species::Zombie | Species::Skeleton),
        can_open_doors: m.can_break_doors,
        can_pass_doors: true,
        can_walk_over_fences: false,
        malus: if animal { path::ANIMAL_MALUS } else { &[] },
        min_y: w.min_y,
    }
}

/// `Mob.serverAiStep`: селекторы → навигация → `MoveControl` → `LookControl`.
/// `WitherBoss.customServerAiStep`: полёт на высоте ~5 над целью и черепа
/// раз в 40 тиков (раз в 20 во второй половине здоровья).
/// Период стрельбы визера: 40 тиков, 20 во второй половине здоровья.
fn wither_period(health: f32, max: f32) -> u32 {
    if health * 2.0 <= max { 20 } else { 40 }
}

/// `EnderDragon.customServerAiStep`: полёт по кругу вокруг портала
/// (`[0.5, 80, 0.5]`, радиус 24) с рывками к ближайшему игроку.
fn dragon_ai_step(m: &mut Mob, s: &mut Senses) {
    m.no_gravity = true;
    if m.dragon_cooldown > 0 {
        m.dragon_cooldown -= 1;
    }
    const CENTER: [f64; 3] = [0.5, 80.0, 0.5];
    const RADIUS: f64 = 24.0;
    let target = m
        .target
        .and_then(|t| s.players.iter().find(|p| p.uuid == t).copied())
        .or_else(|| {
            s.players
                .iter()
                .copied()
                .filter(|p| dist2(p.pos, m.pos) <= 64.0 * 64.0)
                .min_by(|a, b| dist2(a.pos, m.pos).total_cmp(&dist2(b.pos, m.pos)))
        });
    if let Some(p) = target {
        m.target = Some(p.uuid);
    }
    // Рывок к цели на 40 тиков каждые 240 тиков.
    let charging = target.is_some() && m.tick_count % 240 < 40;
    let goal = if charging {
        let p = target.unwrap();
        [p.pos[0], p.pos[1] + 2.0, p.pos[2]]
    } else {
        let angle = f64::from(m.tick_count) * 0.008 + f64::from(m.uuid as u32) * 0.5;
        [
            CENTER[0] + angle.cos() * RADIUS,
            CENTER[1],
            CENTER[2] + angle.sin() * RADIUS,
        ]
    };
    let dx = goal[0] - m.pos[0];
    let dy = goal[1] - m.pos[1];
    let dz = goal[2] - m.pos[2];
    let horizontal = (dx * dx + dz * dz).sqrt();
    m.yaw = (path::atan2(dz, dx) * 57.295_776) as f32 - 90.0;
    m.head_yaw = m.yaw;
    m.pitch = -(path::atan2(dy, horizontal.max(1e-4)) * 57.295_776) as f32;
    // Дракон летит заданной скоростью (воздушное ускорение `travel` мало).
    let speed = if charging { 0.45 } else { 0.3 };
    if horizontal > 1.0 {
        m.vel[0] = dx / horizontal * speed;
        m.vel[2] = dz / horizontal * speed;
    } else {
        m.vel[0] = 0.0;
        m.vel[2] = 0.0;
    }
    m.vel[1] = (dy * 0.1).clamp(-0.3, 0.3);
    m.zza = 0.0;
    m.xxa = 0.0;
    // Дыхание по цели в 24 блоках раз в 100 тиков.
    if m.tick_count.is_multiple_of(100)
        && let Some(p) = s
            .players
            .iter()
            .copied()
            .filter(|p| dist2(p.pos, m.pos) <= 24.0 * 24.0)
            .min_by(|a, b| dist2(a.pos, m.pos).total_cmp(&dist2(b.pos, m.pos)))
    {
        m.out.push(Out::DragonBreath { target: p.uuid });
    }
}

fn wither_ai_step(m: &mut Mob, s: &mut Senses) {
    m.no_gravity = true;
    let target = m
        .target
        .and_then(|t| s.players.iter().find(|p| p.uuid == t).copied())
        .or_else(|| {
            s.players
                .iter()
                .copied()
                .filter(|p| dist2(p.pos, m.pos) <= 40.0 * 40.0)
                .min_by(|a, b| dist2(a.pos, m.pos).total_cmp(&dist2(b.pos, m.pos)))
        });
    let Some(p) = target else {
        m.zza = 0.0;
        m.xxa = 0.0;
        m.vel[1] *= 0.98;
        return;
    };
    m.target = Some(p.uuid);
    let dx = p.pos[0] - m.pos[0];
    let dz = p.pos[2] - m.pos[2];
    let horizontal = (dx * dx + dz * dz).sqrt();
    m.yaw = (path::atan2(dz, dx) * 57.295_776) as f32 - 90.0;
    m.head_yaw = m.yaw;
    let dy = p.pos[1] + 5.0 - m.pos[1];
    m.pitch = -(path::atan2(dy, horizontal.max(1e-4)) * 57.295_776) as f32;
    m.vel[1] = (dy * 0.08).clamp(-0.2, 0.2);
    m.zza = if horizontal > 4.0 { 1.0 } else { 0.0 };
    m.xxa = 0.0;
    let period = wither_period(m.health, m.max_health());
    if m.tick_count.is_multiple_of(period) {
        m.out.push(Out::ShootSkull { target: p.uuid });
    }
}

fn server_ai_step(m: &mut Mob, w: &World, s: &mut Senses) {
    // `Mob.isEffectiveAi`: без целей, навигации и LookControl.
    if m.no_ai {
        return;
    }
    // `Wolf` в позе сидения (`isOrderedToSit`): цели и навигация стоят.
    if m.sitting {
        m.nav.stop();
        m.move_ctl.op = path::MoveOp::Wait;
        m.move_ctl.speed = 0.0;
        m.move_ctl.strafe_fwd = 0.0;
        m.move_ctl.strafe_right = 0.0;
        m.zza = 0.0;
        m.xxa = 0.0;
        goals::look_tick(m);
        return;
    }
    if m.species == Species::Wither {
        wither_ai_step(m, s);
        return;
    }
    if m.species == Species::EnderDragon {
        dragon_ai_step(m, s);
        return;
    }
    let mut goals = std::mem::take(&mut m.goals);
    let mut targets = std::mem::take(&mut m.targets);
    {
        // Полный проход — на первом тике и через тик (со сдвигом на id), иначе
        // только цели с `requiresUpdateEveryTick`.
        let full = m.tick_count <= 1 || (i64::from(m.tick_count) + i64::from(s.entity_id)) % 2 == 0;
        let mut c = goals::Ctx { w, s };
        if full {
            targets.tick(m, &mut c);
            goals.tick(m, &mut c);
        } else {
            targets.tick_running(false, m, &mut c);
            goals.tick_running(false, m, &mut c);
        }
    }
    m.goals = goals;
    m.targets = targets;
    let water = in_water(m, w);
    let walk = walker(m, w, s.difficulty, water);
    if let Some((to, speed)) = m.nav.tick(w, &walk, m.species.follow_range()) {
        m.move_ctl.set_wanted(to, speed);
    }
    if m.species == Species::Slime {
        // `CubeMobMoveControl` сам ставит голову и тело; `LookControl` не трогает.
        cube::move_control(m, s.rand);
    } else {
        path::move_control(m, w, water);
        goals::look_tick(m);
    }
}

/// Тик моба (`LivingEntity.tick` → `aiStep`). Возвращает `true`, когда моб исчез.
/// `CopperGolem.updateWeathering`: смена стадии раз в 504000..=552000 тиков;
/// `-2` — воск (стоит), `-1` — пересчитать таймер заново.
pub fn copper_golem_weathering(m: &mut Mob) {
    if m.species != Species::CopperGolem {
        return;
    }
    m.ticks = m.ticks.saturating_add(1);
    let delta = || 504_000 + (rand::random::<u32>() % 48_001) as i64;
    match m.next_weather_age {
        -2 => {}
        -1 => m.next_weather_age = m.ticks as i64 + delta(),
        next if m.ticks as i64 >= next && m.weather_state != WeatherState::Oxidized => {
            m.weather_state = m.weather_state.next();
            m.next_weather_age = if m.weather_state == WeatherState::Oxidized {
                0
            } else {
                next + delta()
            };
        }
        _ => {}
    }
    // `CopperGolem`: полностью окисленный и не вощёный голем с шансом
    // 0.58 % за тик превращается в статую (случайная поза).
    if m.weather_state == WeatherState::Oxidized
        && m.next_weather_age != -2
        && rand::random::<f32>() < 0.0058
    {
        m.statue = true;
    }
}

pub fn tick(m: &mut Mob, w: &World, s: &mut Senses) -> bool {
    if m.death_time > 0 {
        m.death_time += 1;
        m.xxa = 0.0;
        let _ = travel(m, 0.0, w);
        return m.death_time > DEATH_TICKS;
    }
    copper_golem_weathering(m);
    m.out.clear();
    if m.species == Species::Creeper && creeper_tick(m) {
        return true;
    }
    m.tick_count = m.tick_count.wrapping_add(1);
    m.invulnerable = m.invulnerable.saturating_sub(1);
    m.strike = None;
    m.touch_strikes.clear();
    // `LivingEntity.baseTick`: обидчик забывается через 100 тиков.
    if m.last_hurt_by
        .is_some_and(|(_, t)| m.tick_count.wrapping_sub(t) > 100)
    {
        m.last_hurt_by = None;
    }
    // `Mob.serverAiStep`: `++noActionTime`; сбрасывают удар и игрок рядом.
    m.no_action = m.no_action.saturating_add(1);
    if m.species.is_animal() {
        if m.age < 0 {
            m.age += 1;
        } else if m.age > 0 {
            m.age -= 1;
        }
        m.love = (m.love - 1).max(0);
    }
    // `LivingEntity.updateUsingItem`: лук натягивается.
    if let Some(t) = &mut m.using_item {
        *t += 1;
    }
    // `LivingEntity.aiStep`: задержка прыжка тает до ИИ.
    m.no_jump_delay = m.no_jump_delay.saturating_sub(1);
    // `applyInput`: ход затухает до ИИ (`WAIT` гасит только `zza`).
    m.xxa = f64::from(m.xxa as f32 * 0.98);
    m.zza = f64::from(m.zza as f32 * 0.98);
    server_ai_step(m, w, s);
    // Прыжок: в воде — всплытие (`jumpInLiquid`, +0.04), с земли — не чаще
    // раза в 10 тиков подряд.
    let water = in_water(m, w);
    if m.jump {
        if water && (!m.on_ground || goals::deep_water(m, w)) {
            m.vel[1] += 0.04;
        } else if m.on_ground && m.no_jump_delay == 0 {
            m.vel[1] = m.vel[1].max(jump_power(s.jump_boost));
            m.no_jump_delay = 10;
        }
    } else {
        m.no_jump_delay = 0;
    }
    let blocked = travel(m, m.zza, w);
    m.horizontal_collision = blocked;
    m.jump = false;
    // `Spider.tick`: лезет, пока упёрся.
    if m.species == Species::Spider {
        m.climbing = blocked;
    }
    if m.species == Species::Slime {
        cube::after_tick(m);
        // `Player.aiStep` → `Slime.playerTouch`: игрок, чья рамка (раздутая
        // на 1, 0.5, 1) задевает слизня, получает удар, если тот в досягаемости
        // и на виду (`dealDamage`).
        if cube::deals_damage(m) {
            let (wd, h) = m.size();
            let touched: Vec<u128> = s
                .prey
                .iter()
                .filter(|p| {
                    let pw = PLAYER_SIZE.0 / 2.0 + 1.0;
                    (p.pos[0] - m.pos[0]).abs() < pw + wd / 2.0
                        && (p.pos[2] - m.pos[2]).abs() < pw + wd / 2.0
                        && p.pos[1] - 0.5 < m.pos[1] + h
                        && p.pos[1] + PLAYER_SIZE.1 + 0.5 > m.pos[1]
                        && in_reach(m, p)
                        && (s.sight)(m.eye(), p.eye())
                })
                .map(|p| p.uuid)
                .collect();
            m.touch_strikes.extend(touched);
        }
    }
    false
}

/// `Creeper.tick` до `super.tick`: набухание и взрыв. `true` — взорвался.
fn creeper_tick(m: &mut Mob) -> bool {
    if m.ignited {
        m.swell_dir = 1;
    }
    if m.swell_dir > 0 && m.swell == 0 {
        m.out.push(Out::Primed);
    }
    m.swell = (m.swell + m.swell_dir).max(0);
    if m.swell >= m.max_swell {
        m.swell = m.max_swell;
        let k = if m.powered { 2.0 } else { 1.0 };
        m.out.push(Out::Explode(m.explosion_radius as f32 * k));
        return true;
    }
    false
}

/// Источник урона (`DamageSource`): откуда и кто.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Source {
    /// Точка источника: отбрасывание от неё. Урон с точкой (удар, стрела,
    /// взрыв) пугает животных (`#panic_causes`), урон среды без точки — нет.
    pub from: Option<[f64; 3]>,
    /// Обидчик (`lastHurtByMob`) — UUID игрока: его берёт целью `HurtByTargetGoal`.
    pub attacker: Option<u128>,
}

impl Source {
    pub fn at(from: [f64; 3], attacker: Option<u128>) -> Self {
        Self {
            from: Some(from),
            attacker,
        }
    }
}

/// Удар по мобу (`LivingEntity.hurt`): неуязвимость, отбрасывание, память об
/// обидчике и источнике (для `HurtByTargetGoal` и `PanicGoal`).
/// `None` — урон не прошёл; `Some(true)` — моб умер.
/// `LivingEntity.causeFallDamage`: урон только за высоту сверх безопасной.
pub fn fall_damage(fall: f64, safe: f32) -> f32 {
    ((fall - f64::from(safe)).ceil()).max(0.0) as f32
}

/// `LivingEntity.knockback(power, xd, zd)`: отбрасывание по единичному
/// направлению `(xd, zd)`; вверх — только стоя на земле.
pub fn knockback(m: &mut Mob, power: f64, xd: f64, zd: f64) {
    if power <= 0.0 {
        return;
    }
    let len = (xd * xd + zd * zd).sqrt();
    if len <= 1e-4 {
        return;
    }
    m.vel[0] = m.vel[0] / 2.0 - xd / len * power;
    m.vel[2] = m.vel[2] / 2.0 - zd / len * power;
    if m.on_ground {
        m.vel[1] = (m.vel[1] / 2.0 + power).min(0.4);
    }
}

pub fn hurt(m: &mut Mob, amount: f32, src: Source, knockback_power: f64) -> Option<bool> {
    if !m.alive() {
        return None;
    }
    let dealt = if m.invulnerable > INVULNERABLE / 2 {
        if amount <= m.last_damage {
            return None;
        }
        amount - m.last_damage
    } else {
        m.invulnerable = INVULNERABLE;
        amount
    };
    m.last_damage = amount;
    m.health = (m.health - dealt).max(0.0);
    m.no_action = 0;
    m.last_damage_source = Some((m.tick_count, src.from.is_some()));
    if let Some(a) = src.attacker {
        m.last_hurt_by = Some((a, m.tick_count));
    }
    if let Some(from) = src.from {
        knockback(m, knockback_power, from[0] - m.pos[0], from[2] - m.pos[2]);
    }
    if m.health <= 0.0 {
        m.death_time = 1;
        return Some(true);
    }
    Some(false)
}

// --- Свет, солнце и исчезновение -------------------------------------------

/// Ключевые кадры `gameplay/sky_light_level` в `timeline/day.json`: множитель
/// к 15, линейно между кадрами, по кругу с периодом 24000.
const SKY_LIGHT_KEYFRAMES: [(f32, f32); 4] = [
    (133.0, 1.0),
    (11867.0, 1.0),
    (13670.0, 0.26666668),
    (22330.0, 0.26666668),
];

/// `KeyframeTrack` с линейным переходом: значение в тике `t` периода 24000.
fn sky_light_factor(t: f32) -> f32 {
    let n = SKY_LIGHT_KEYFRAMES.len();
    // Последний кадр, не позже `t`; до первого — последний прошлого периода.
    let i = SKY_LIGHT_KEYFRAMES
        .iter()
        .rposition(|k| k.0 <= t)
        .unwrap_or(n - 1);
    let (t0, v0) = SKY_LIGHT_KEYFRAMES[i];
    let (t1, v1) = SKY_LIGHT_KEYFRAMES[(i + 1) % n];
    let span = (t1 - t0).rem_euclid(24000.0);
    let x = (t - t0).rem_euclid(24000.0) / span;
    v0 + (v1 - v0) * x
}

/// Затемнение неба (`Level.updateSkyBrightness`): `(int)(15 − SKY_LIGHT_LEVEL)`.
/// Уровень — 15, умноженное на таймлайн дня, затем слои погоды
/// (`WeatherAttributes`): дождь без грозы тянет к 4 с долей 0.3125, гроза — с
/// долей 0.52734375. `day` — время суток, дождь и гроза — 0..1.
pub fn sky_darken(day: f32, rain: f32, thunder: f32) -> i32 {
    let mut level = 15.0 * sky_light_factor(day.rem_euclid(24000.0));
    let lerp = |a: f32, from: f32, to: f32| from + a * (to - from);
    let rain_only = rain - thunder;
    if rain_only > 0.0 {
        level = lerp(rain_only, level, lerp(0.3125, level, 4.0));
    }
    if thunder > 0.0 {
        level = lerp(thunder, level, lerp(0.52734375, level, 4.0));
    }
    (15.0 - level) as i32
}

/// `getMaxLocalRawBrightness`: блочный свет или небесный минус затемнение.
pub fn raw_brightness(sky: u8, block: u8, darken: i32) -> u8 {
    block.max((i32::from(sky) - darken).clamp(0, 15) as u8)
}

/// `LightTexture.getBrightness` при `ambient_light` 0: `f / (4 − 3f)`, `f = уровень / 15`.
pub fn brightness(level: u8) -> f32 {
    let f = f32::from(level.min(15)) / 15.0;
    f / (4.0 - 3.0 * f)
}

/// `Mob.isSunBurnTick`: светло снаружи (`skyDarken < 4`), яркость у глаз
/// больше 0.5, бросок `nextFloat × 30 < (яркость − 0.4) × 2`, не мокрый и
/// видит небо. `raw` — сырая яркость у глаз, `r` — `nextFloat`.
pub fn sun_burn_tick(raw: u8, darken: i32, sees_sky: bool, wet: bool, r: f64) -> bool {
    if darken >= 4 || wet || !sees_sky {
        return false;
    }
    let br = brightness(raw);
    br > 0.5 && (r as f32) * 30.0 < (br - 0.4) * 2.0
}

/// Световые правила спавна монстров измерения (`DimensionType`):
/// предел блочного света и диапазон `monster_spawn_light_level`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonsterLight {
    pub block_limit: u8,
    pub level: (u8, u8),
}

impl MonsterLight {
    /// Оверворлд и Край: блочный свет 0, уровень `uniform(0, 7)`.
    pub const OVERWORLD: Self = Self {
        block_limit: 0,
        level: (0, 7),
    };
    /// Ад: блочный свет до 15 не мешает, уровень 7.
    pub const NETHER: Self = Self {
        block_limit: 15,
        level: (7, 7),
    };
}

/// `Monster.isDarkEnoughToSpawn`: небесный свет больше `nextInt(32)` — нет;
/// блочный выше предела — нет; иначе сырая яркость (в грозу затемнение 10)
/// не выше случайного уровня из диапазона.
pub fn dark_enough(
    sky: u8,
    block: u8,
    darken: i32,
    thundering: bool,
    rules: MonsterLight,
    rand: &mut dyn FnMut() -> f64,
) -> bool {
    let roll = (rand() * 32.0).floor() as i32;
    if i32::from(sky) > roll {
        return false;
    }
    if rules.block_limit < 15 && block > rules.block_limit {
        return false;
    }
    let raw = raw_brightness(sky, block, if thundering { 10 } else { darken });
    let (lo, hi) = rules.level;
    let sample = lo + ((rand() * f64::from(hi - lo + 1)).floor() as u8).min(hi - lo);
    raw <= sample
}

/// `Animal.isBrightEnoughToSpawn`: `getRawBrightness(pos, 0) > 8`.
pub fn bright_enough(sky: u8, block: u8) -> bool {
    raw_brightness(sky, block, 0) > 8
}

/// `Mob.checkDespawn`. `nearest` — квадрат расстояния до ближайшего игрока
/// (`None` — игроков нет, тогда ничего не делаем). `true` — убрать моба.
pub fn check_despawn(m: &mut Mob, nearest: Option<f64>, peaceful: bool, r: f64) -> bool {
    if peaceful && m.species.is_monster() {
        return true;
    }
    if m.persistent || m.custom_name.is_some() || !m.species.category().despawns() {
        m.no_action = 0;
        return false;
    }
    let Some(d2) = nearest else {
        return false;
    };
    if d2 > DESPAWN_DISTANCE * DESPAWN_DISTANCE {
        return true;
    }
    let far = d2 > NO_DESPAWN_DISTANCE * NO_DESPAWN_DISTANCE;
    if m.no_action > IDLE_DESPAWN_TICKS && far && r < 1.0 / 800.0 {
        return true;
    }
    if !far {
        m.no_action = 0;
    }
    false
}

// --- Естественный спавн ----------------------------------------------------

/// Строка `spawns_by_category` биома: вид (`None` — его пока нет в сервере),
/// вес и размер группы.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnEntry {
    pub species: Option<Species>,
    pub weight: u32,
    pub min: u32,
    pub max: u32,
}

/// Взвешенный выбор (`WeightedList.getRandom`). Неизвестный вид остаётся в
/// весах: его доля просто не спавнится, а доля зомби остаётся ванильной.
pub fn pick_entry(entries: &[SpawnEntry], r: f64) -> Option<SpawnEntry> {
    let total: u32 = entries.iter().map(|e| e.weight).sum();
    if total == 0 {
        return None;
    }
    let mut pick = ((r * f64::from(total)).floor() as u32).min(total - 1);
    for e in entries {
        if pick < e.weight {
            return Some(*e);
        }
        pick -= e.weight;
    }
    None
}

/// Максимум мобов за один заход в чанк (`Mob.getMaxSpawnClusterSize`).
pub const MAX_CLUSTER: u32 = 4;

/// `NaturalSpawner.spawnCategoryForPosition`: три группы от точки `start`,
/// в каждой до `ceil(4·r)` шагов со сдвигом `nextInt(6) − nextInt(6)` по X и Z;
/// вид выбирается на первом подходящем шаге, и тогда число шагов становится
/// размером группы. Всего не больше [`MAX_CLUSTER`].
///
/// `nearest(pos)` — квадрат расстояния до ближайшего игрока от центра клетки
/// (`None` — игроков нет), `fits(вид, pos, d2)` — все проверки места: дальность
/// от игроков и точки спавна, опора, свободное место, свет. Возвращает, кого
/// и где поставить.
pub fn spawn_packs(
    start: [i32; 3],
    entries: &[SpawnEntry],
    nearest: &mut dyn FnMut([i32; 3]) -> Option<f64>,
    fits: &mut dyn FnMut(Species, [i32; 3], f64) -> bool,
    rand: &mut dyn FnMut() -> f64,
) -> Vec<(Species, [i32; 3])> {
    let mut out = Vec::new();
    let next_int = |rand: &mut dyn FnMut() -> f64, n: u32| {
        ((rand() * f64::from(n)).floor() as i32).min(n as i32 - 1)
    };
    for _ in 0..3 {
        let (mut x, mut z) = (start[0], start[2]);
        let mut chosen: Option<SpawnEntry> = None;
        let mut steps = (rand() * 4.0).ceil() as u32;
        let mut q = 0;
        while q < steps {
            q += 1;
            x += next_int(rand, 6) - next_int(rand, 6);
            z += next_int(rand, 6) - next_int(rand, 6);
            let pos = [x, start[1], z];
            let Some(d2) = nearest(pos) else {
                continue;
            };
            // `isRightDistanceToPlayerAndSpawnPoint`.
            if d2 <= MIN_SPAWN_DISTANCE * MIN_SPAWN_DISTANCE {
                continue;
            }
            let entry = match chosen {
                Some(e) => e,
                None => {
                    let Some(e) = pick_entry(entries, rand()) else {
                        break;
                    };
                    chosen = Some(e);
                    steps = e.min + next_int(rand, 1 + e.max.saturating_sub(e.min)).max(0) as u32;
                    e
                }
            };
            // Вид, которого в сервере нет: группа пропадает целиком.
            let Some(species) = entry.species else {
                break;
            };
            // `isValidSpawnPostitionForType`: исчезающие не дальше 128 блоков.
            if species.category().despawns() && d2 > DESPAWN_DISTANCE * DESPAWN_DISTANCE {
                continue;
            }
            if !fits(species, pos, d2) {
                continue;
            }
            out.push((species, pos));
            if out.len() as u32 >= MAX_CLUSTER {
                return out;
            }
        }
    }
    out
}

/// Пусто ли место под рамку моба `w × h` с ногами в `feet` (`Level.noCollision`
/// для `EntityType.getSpawnAABB`). `shape` — коробки блока в долях клетки.
pub fn box_free<'r>(
    feet: [f64; 3],
    w: f64,
    h: f64,
    shape: &dyn Fn([i32; 3]) -> &'r [[f64; 6]],
) -> bool {
    let half = w / 2.0;
    let lo = [feet[0] - half, feet[1], feet[2] - half];
    let hi = [feet[0] + half, feet[1] + h, feet[2] + half];
    let cell = |v: f64| v.floor() as i32;
    for x in cell(lo[0])..=cell(hi[0] - EPS) {
        for y in cell(lo[1]) - 1..=cell(hi[1] - EPS) {
            for z in cell(lo[2])..=cell(hi[2] - EPS) {
                for b in shape([x, y, z]) {
                    if b[3] + f64::from(x) > lo[0] + EPS
                        && b[0] + f64::from(x) < hi[0] - EPS
                        && b[4] + f64::from(y) > lo[1] + EPS
                        && b[1] + f64::from(y) < hi[1] - EPS
                        && b[5] + f64::from(z) > lo[2] + EPS
                        && b[2] + f64::from(z) < hi[2] - EPS
                    {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Верхняя грань блока прочна целиком (`isFaceSturdy(UP)`): есть коробка,
/// закрывающая всю клетку по X и Z и доходящая до верха.
pub fn sturdy_top(boxes: &[[f64; 6]]) -> bool {
    boxes.iter().any(|b| {
        b[0] <= EPS && b[2] <= EPS && b[3] >= 1.0 - EPS && b[5] >= 1.0 - EPS && b[4] >= 1.0 - EPS
    })
}

/// Полный куб коллизии (`isCollisionShapeFullBlock`).
pub fn full_block(boxes: &[[f64; 6]]) -> bool {
    boxes.iter().any(|b| {
        b[0] <= EPS
            && b[1] <= EPS
            && b[2] <= EPS
            && b[3] >= 1.0 - EPS
            && b[4] >= 1.0 - EPS
            && b[5] >= 1.0 - EPS
    })
}

/// Блоки, на которых не спавнится никто из наших мобов (`isValidSpawn` =
/// `never` или только для особых видов: лёд — белые медведи, листва —
/// оцелоты и попугаи, магма — огнестойкие).
pub fn spawn_floor_forbidden(name: &str) -> bool {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    matches!(
        n,
        "bedrock" | "barrier" | "magma_block" | "ice" | "packed_ice" | "blue_ice" | "frosted_ice"
    ) || n.ends_with("glass")
        || n.ends_with("leaves")
}

/// Опасные для обычного моба блоки в месте спавна (`EntityType.isBlockDangerous`)
/// и запрещающие спавн внутри (`#prevents_mob_spawning_inside` — рельсы).
pub fn spawn_cell_forbidden(name: &str) -> bool {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    matches!(
        n,
        "fire"
            | "soul_fire"
            | "lava"
            | "magma_block"
            | "sweet_berry_bush"
            | "wither_rose"
            | "powder_snow"
            | "cactus"
    ) || n.ends_with("campfire")
        || n.ends_with("rail")
}

#[cfg(test)]
mod tests {
    #[test]
    fn copper_golem_weathering_advances_stages() {
        let mut m = Mob::new(Species::CopperGolem, [0.0; 3], 1);
        // Первый тик назначает таймер в диапазоне ванили.
        copper_golem_weathering(&mut m);
        assert!(
            (504_000..=552_000).contains(&m.next_weather_age),
            "таймер вне диапазона: {}",
            m.next_weather_age
        );
        // Досрочная смена стадии (в NBT пришло `next_weather_age`).
        m.next_weather_age = 1;
        m.ticks = 2;
        copper_golem_weathering(&mut m);
        assert_eq!(m.weather_state, WeatherState::Exposed);
        m.next_weather_age = 1;
        copper_golem_weathering(&mut m);
        assert_eq!(m.weather_state, WeatherState::Weathered);
        m.next_weather_age = 1;
        copper_golem_weathering(&mut m);
        assert_eq!(m.weather_state, WeatherState::Oxidized);
        // Воск (`-2`) останавливает окисление.
        m.next_weather_age = -2;
        for _ in 0..5 {
            copper_golem_weathering(&mut m);
        }
        assert_eq!(m.weather_state, WeatherState::Oxidized);
        // Снятие воска (`-1`) снова включает таймер.
        m.next_weather_age = -1;
        copper_golem_weathering(&mut m);
        assert!(m.next_weather_age >= 504_000);
    }

    #[test]
    fn wither_shoots_faster_in_second_phase() {
        assert_eq!(wither_period(300.0, 300.0), 40);
        assert_eq!(wither_period(151.0, 300.0), 40);
        assert_eq!(wither_period(150.0, 300.0), 20);
        assert_eq!(wither_period(1.0, 300.0), 20);
    }

    #[test]
    fn dragon_flies_without_gravity() {
        let mut m = Mob::new(Species::EnderDragon, [0.0; 3], 1);
        let mut rnd = || 0.5;
        let sight = |_: [f64; 3], _: [f64; 3]| true;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &sight,
            difficulty: 2,
            entity_id: 1,
            bright_outside: true,
            mob_griefing: true,
            jump_boost: None,
        };
        dragon_ai_step(&mut m, &mut s);
        assert!(m.no_gravity);
        assert!(m.vel[1].abs() <= 0.3);
    }

    #[test]
    fn wither_flies_without_gravity() {
        let mut m = Mob::new(Species::Wither, [0.0; 3], 1);
        let mut rnd = || 0.5;
        let sight = |_: [f64; 3], _: [f64; 3]| true;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &sight,
            difficulty: 2,
            entity_id: 1,
            bright_outside: true,
            mob_griefing: true,
            jump_boost: None,
        };
        wither_ai_step(&mut m, &mut s);
        assert!(m.no_gravity);
    }

    #[test]
    fn weather_state_ids_match_vanilla() {
        assert_eq!(WeatherState::Unaffected.id(), 0);
        assert_eq!(WeatherState::Exposed.id(), 1);
        assert_eq!(WeatherState::Weathered.id(), 2);
        assert_eq!(WeatherState::Oxidized.id(), 3);
    }

    #[test]
    fn oxidized_golem_turns_into_statue_but_waxed_does_not() {
        let mut m = Mob::new(Species::CopperGolem, [0.0; 3], 1);
        m.weather_state = WeatherState::Oxidized;
        m.next_weather_age = 0;
        for _ in 0..20_000 {
            copper_golem_weathering(&mut m);
            if m.statue {
                break;
            }
        }
        assert!(m.statue, "окисленный голем не стал статуей");

        let mut waxed = Mob::new(Species::CopperGolem, [0.0; 3], 2);
        waxed.weather_state = WeatherState::Oxidized;
        waxed.next_weather_age = -2;
        for _ in 0..20_000 {
            copper_golem_weathering(&mut waxed);
        }
        assert!(!waxed.statue, "вощёный голем стал статуей");
    }

    #[test]
    fn copper_golem_sounds_follow_oxidation() {
        let mut m = Mob::new(Species::CopperGolem, [0.0; 3], 1);
        assert_eq!(m.sound("hurt"), "minecraft:entity.copper_golem.hurt");
        m.weather_state = WeatherState::Exposed;
        assert_eq!(m.sound("death"), "minecraft:entity.copper_golem.death");
        m.weather_state = WeatherState::Weathered;
        assert_eq!(
            m.sound("hurt"),
            "minecraft:entity.copper_golem_weathered.hurt"
        );
        m.weather_state = WeatherState::Oxidized;
        assert_eq!(
            m.sound("death"),
            "minecraft:entity.copper_golem_oxidized.death"
        );
    }

    use super::*;

    #[test]
    fn equipment_population_matches_vanilla() {
        // rng = 0: броня есть (0 < 0.15), все три апгрейда типа дают 3
        // (кольчуга), после шлема `partial` стоп; оружие зомби — меч.
        let mut zero = || 0.0f32;
        let gear = random_equipment(Species::Zombie, 2, 1.0, &mut zero);
        assert!(gear.contains(&(5, "minecraft:chainmail_helmet")));
        assert!(!gear.iter().any(|(slot, _)| *slot == 4));
        assert!(gear.contains(&(0, "minecraft:iron_sword")));

        // rng = 0.99: ни брони, ни оружия; скелет всё равно с луком.
        let mut high = || 0.99f32;
        assert!(random_equipment(Species::Zombie, 2, 1.0, &mut high).is_empty());
        assert_eq!(
            random_equipment(Species::Skeleton, 0, 0.0, &mut high),
            vec![(0, "minecraft:bow")]
        );

        // Тип 1 (медь) и полный комплект: броня не прерывается.
        let seq = [0.1f32, 0.5, 0.9, 0.9, 0.9, 0.9, 0.9, 0.9, 0.9];
        let mut i = 0;
        let mut rng = || {
            let v = seq[i % seq.len()];
            i += 1;
            v
        };
        let gear = random_equipment(Species::Zombie, 2, 1.0, &mut rng);
        for (slot, piece) in [(5usize, 0usize), (4, 1), (3, 2), (2, 3)] {
            assert!(gear.contains(&(slot, ARMOR_NAMES[piece][1])));
        }
        assert!(!gear.iter().any(|(slot, _)| *slot == 0));
    }

    /// Форма блока для тестового мира.
    type ShapeFn<'a> = Box<dyn Fn([i32; 3]) -> &'static [[f64; 6]] + 'a>;
    /// Вода в блоке для тестового мира.
    type WaterFn = Box<dyn Fn([i32; 3]) -> bool>;

    fn floor(p: [i32; 3]) -> bool {
        p[1] < 64
    }

    /// Мир-пол без воды: пара владеемых замыканий под `World`.
    fn flat_world(f: &dyn Fn([i32; 3]) -> bool) -> (ShapeFn<'_>, WaterFn) {
        let shape = move |p: [i32; 3]| if f(p) { FULL } else { &[] };
        (Box::new(shape), Box::new(|_: [i32; 3]| false))
    }

    fn settle(m: &mut Mob, w: &World) {
        for _ in 0..60 {
            travel(m, 0.0, w);
        }
    }

    /// Без целей, путь к `to` (`navigation.moveTo`): проверка ходьбы отдельно от ИИ.
    fn walk_to(m: &mut Mob, w: &World, to: [f64; 3], speed: f64) {
        m.goals = Selector::default();
        m.targets = Selector::default();
        let walk = walker(m, w, 2, false);
        let found = m.nav.path_to(w, &walk, to, 1, m.species.follow_range());
        assert!(m.nav.move_to(w, &walk, found, speed), "нет пути к {to:?}");
    }

    /// Детерминированный ГСЧ (LCG) для целей со случайными точками.
    fn lcg(seed: u64) -> impl FnMut() -> f64 {
        let mut s = seed;
        move || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Скорость ходьбы закреплена на ванильном значении.
    ///
    /// Сверено с байткодом 26.3. `travelInAir` передаёт в `moveRelative`
    /// `getFrictionInfluencedSpeed(f)`, а `input.zza` у моба равен его скорости
    /// (`Mob.setSpeed` вызывает `setZza(f)`). На обычном блоке
    /// `f = computeModifiedFriction(0.6, friction_modifier=1.0) = 0.6`, поэтому
    /// `getFrictionInfluencedSpeed = getSpeed() * 0.216 / f³ = getSpeed()`.
    /// Итоговое ускорение за тик — `speed²`, установившаяся скорость —
    /// `speed² / (1 - 0.6 * 0.91)`: для коровы (0.2) это 0.0881 блока за тик.
    #[test]
    fn slime_player_touch_hits_every_overlapping_player() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Slime, [8.5, 64.0, 8.5], 1);
        m.set_size(4, true);
        m.on_ground = true;
        let prey = [
            Prey {
                uuid: 7,
                pos: [8.5, 64.0, 8.5],
            },
            Prey {
                uuid: 8,
                pos: [9.5, 64.0, 8.5],
            },
        ];
        let sight = |_: [f64; 3], _: [f64; 3]| true;
        let mut rnd = || 0.5;
        let mut s = Senses {
            players: &prey,
            rand: &mut rnd,
            prey: &prey,
            tempters: &[],
            kin: &[],
            sight: &sight,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        tick(&mut m, &w, &mut s);
        assert_eq!(m.touch_strikes, vec![7, 8]);
    }

    #[test]
    fn lava_counts_as_liquid_for_cube_mobs() {
        let shape = |_: [i32; 3]| FULL;
        let water = |_: [i32; 3]| false;
        let cell = |_: [i32; 3]| path::Cell {
            kind: path::PathType::Lava,
            flags: 0,
        };
        let w = World {
            shape: &shape,
            water: &water,
            cell: Some(&cell),
            info: None,
            min_y: -64,
        };
        let m = Mob::new(Species::Slime, [0.5, 64.0, 0.5], 1);
        assert!(in_liquid(&m, &w));
    }

    #[test]
    fn walk_speed_matches_vanilla() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Cow, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        // Путь не длиннее `follow_range` (16): цель в его пределах.
        walk_to(&mut m, &w, [15.5, 64.0, 0.5], 1.0);
        let mut rnd = || 0.5;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        // Первые тики — разгон от нуля (за 30 тиков скорость сходится до 1e-8).
        for _ in 0..30 {
            tick(&mut m, &w, &mut s);
        }
        let start = m.pos;
        let ticks = 100u32;
        for _ in 0..ticks {
            tick(&mut m, &w, &mut s);
        }
        let per_tick = (m.pos[0] - start[0]) / ticks as f64;
        let speed = Species::Cow.speed();
        let expected = friction_speed(speed, 0.6) * speed / (1.0 - GROUND_FRICTION);
        assert!(
            (per_tick - expected).abs() < 1e-4,
            "cow walks {per_tick:.5} blocks/tick, vanilla gives {expected:.5}"
        );
        // И это заметно медленнее прежней ошибочной формулы (speed/(1-slip)).
        assert!(per_tick < 0.09, "cow still too fast: {per_tick:.5}");
    }

    #[test]
    fn falls_and_lands() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Cow, [0.5, 70.0, 0.5], 1);
        settle(&mut m, &w);
        assert!((m.pos[1] - 64.0).abs() < 1e-6, "{:?}", m.pos);
        assert!(m.on_ground);
    }

    #[test]
    fn chicken_glides() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut c = Mob::new(Species::Chicken, [0.5, 80.0, 0.5], 1);
        let mut p = Mob::new(Species::Pig, [0.5, 80.0, 0.5], 2);
        for _ in 0..10 {
            travel(&mut c, 0.0, &w);
            travel(&mut p, 0.0, &w);
        }
        assert!(c.pos[1] > p.pos[1] + 1.0, "{} {}", c.pos[1], p.pos[1]);
    }

    /// Пол y < 64 и неполный блок `top` в столбце x = 3 на y = 64.
    fn ledge(top: &'static [[f64; 6]]) -> impl Fn([i32; 3]) -> &'static [[f64; 6]] {
        move |p| {
            if p[1] < 64 {
                FULL
            } else if p[1] == 64 && p[0] == 3 {
                top
            } else {
                &[]
            }
        }
    }

    const SLAB: &[[f64; 6]] = &[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]];
    const PATH: &[[f64; 6]] = &[[0.0, 0.0, 0.0, 1.0, 0.9375, 1.0]];

    #[test]
    fn rests_on_partial_blocks() {
        for (top, y) in [(SLAB, 64.5), (PATH, 64.9375)] {
            let w = World {
                shape: &ledge(top),
                water: &|_| false,
                cell: None,
                info: None,
                min_y: -64,
            };
            let mut m = Mob::new(Species::Cow, [3.5, 70.0, 0.5], 1);
            settle(&mut m, &w);
            assert!((m.pos[1] - y).abs() < 1e-9, "{:?} on {top:?}", m.pos);
            assert!(m.on_ground);
        }
    }

    #[test]
    fn steps_onto_slab_without_jump() {
        let w = World {
            shape: &ledge(SLAB),
            water: &|_| false,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Pig, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        m.set_speed(0.25);
        let mut highest: f64 = 64.0;
        for _ in 0..60 {
            m.yaw = -90.0;
            travel(&mut m, 0.25, &w);
            highest = highest.max(m.pos[1]);
            if m.pos[0] > 3.6 {
                break;
            }
        }
        assert!(m.pos[0] > 3.0, "stuck at {:?}", m.pos);
        assert!(
            (highest - 64.5).abs() < 1e-9,
            "climbed to {highest}, not a slab step"
        );
    }

    #[test]
    fn walks_through_flowers() {
        let w = World {
            shape: &ledge(&[]),
            water: &|_| false,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Cow, [3.5, 70.0, 0.5], 1);
        settle(&mut m, &w);
        assert!((m.pos[1] - 64.0).abs() < 1e-9, "{:?}", m.pos);
    }

    #[test]
    fn wall_stops_and_jumps() {
        // Стена из одного блока на x = 3: моб упирается, прыгает и забирается.
        let wall = |p: [i32; 3]| p[1] < 64 || (p[0] == 3 && p[1] == 64);
        let (shape, water) = flat_world(&wall);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Pig, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        walk_to(&mut m, &w, [10.5, 64.0, 0.5], 1.0);
        let mut rnd = || 0.9;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        let mut max_x: f64 = 0.0;
        for _ in 0..200 {
            tick(&mut m, &w, &mut s);
            max_x = max_x.max(m.pos[0]);
            // Внутри стены не бывает.
            assert!(
                !(m.pos[0] + 0.45 > 3.0 + EPS && m.pos[0] - 0.45 < 4.0 && m.pos[1] < 65.0 - EPS),
                "{:?}",
                m.pos
            );
        }
        assert!(max_x > 4.0, "{max_x}");
    }

    #[test]
    fn panics_away_from_attacker() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Cow, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        let hit = Source::at([-2.0, 64.0, 0.5], None);
        assert_eq!(hurt(&mut m, 1.0, hit, KNOCKBACK), Some(false));
        // Повторный такой же удар в неуязвимости не проходит.
        assert_eq!(hurt(&mut m, 1.0, hit, KNOCKBACK), None);
        // `PanicGoal` бежит в случайную точку `DefaultRandomPos.getPos(5, 4)`, не
        // обязательно прочь от обидчика: важно, что бежит и с `panic_speed`.
        let mut rnd = lcg(7);
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        let start = m.pos;
        let mut ran = false;
        for _ in 0..40 {
            tick(&mut m, &w, &mut s);
            ran |= m.panicking() && m.nav.speed == Species::Cow.panic_speed();
        }
        assert!(ran, "не запаниковала");
        assert!(dist2(m.pos, start) > 4.0, "{:?}", m.pos);
        assert_eq!(m.health, 9.0);
    }

    #[test]
    fn dies_and_despawns() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Chicken, [0.5, 64.0, 0.5], 1);
        assert_eq!(hurt(&mut m, 10.0, Source::default(), 0.0), Some(true));
        let mut rnd = || 0.5;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        let gone = (0..30).position(|_| tick(&mut m, &w, &mut s));
        assert_eq!(gone, Some(19));
    }

    #[test]
    fn follows_food() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Sheep, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        let food = [[0.5, 64.0, 8.5]];
        let mut rnd = || 0.99;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &food,
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        for _ in 0..100 {
            tick(&mut m, &w, &mut s);
        }
        assert!(m.pos[2] > 5.0 && m.pos[2] < 7.0, "{:?}", m.pos);
    }

    /// Строка выгрузки накрывает измеримое, поведение остаётся встроенным.
    #[test]
    fn vanilla_data_overrides_builtin() {
        let row = |line: &str| {
            let f: Vec<&str> = line.split(';').collect();
            let num = |i: usize| f[i].parse::<f64>().ok();
            chunkwake_data::world::EntityTypeInfo {
                name: f[0].into(),
                tracking_range: 8,
                update_interval: 3,
                width: f[3].parse().unwrap(),
                height: f[4].parse().unwrap(),
                category: f[5].into(),
                fire_immune: false,
                eye_height: f[7].parse().unwrap(),
                attributes: Some(chunkwake_data::world::DefaultAttributes {
                    max_health: num(8).unwrap(),
                    movement_speed: num(9).unwrap(),
                    follow_range: num(10).unwrap(),
                    attack_damage: num(11),
                    armor: num(12).unwrap(),
                    knockback_resistance: num(13).unwrap(),
                    step_height: num(14).unwrap(),
                    safe_fall_distance: num(15).unwrap(),
                }),
            }
        };
        let types = [row(
            "minecraft:zombie;8;3;0.6;1.95;monster;false;1.74;20.0;0.23000000417232513;35.0;3.0;2.0;0.0;0.6;3.0",
        )];
        let z = merge(&BUILTIN[Species::Zombie as usize], &types);
        assert_eq!(z.speed, 0.23000000417232513);
        assert_eq!((z.follow_range, z.attack_damage, z.armor), (35.0, 3.0, 2.0));
        assert!(z.burns_in_sun && z.category == Category::Monster);
        // Нет строки — встроенные значения как есть.
        assert_eq!(merge(&BUILTIN[0], &types), BUILTIN[0]);
        // Встроенные совпадают с ванилью до f32.
        assert_eq!(
            BUILTIN[Species::Zombie as usize].speed as f32,
            z.speed as f32
        );
    }

    #[test]
    fn effects_merge_tick_and_expire() {
        use crate::survival::{Effect, EffectKinds};
        let mut m = Mob::new(Species::Cow, [0.0; 3], 1);
        m.add_effect(Effect::plain(1, 0, 100));
        // Слабже и короче — не заменяет.
        m.add_effect(Effect::plain(1, 0, 50));
        assert_eq!(m.effects[0].duration, 100);
        // Сильнее — заменяет целиком.
        m.add_effect(Effect::plain(1, 1, 40));
        assert_eq!((m.effects[0].amplifier, m.effects[0].duration), (1, 40));

        let kinds = EffectKinds {
            poison: Some(2),
            ..EffectKinds::default()
        };
        m.add_effect(Effect::plain(2, 0, 25));
        assert!(m.tick_effects(kinds).poison);
        assert_eq!(m.effects[1].duration, 24);
        for _ in 0..24 {
            m.tick_effects(kinds);
        }
        assert!(!m.has_effect(2), "яд должен истечь: {:?}", m.effects);
    }

    #[test]
    fn effects_change_speed_and_damage() {
        use crate::survival::{Effect, EffectKinds};
        let mut m = Mob::new(Species::Zombie, [0.0; 3], 1);
        let base = m.speed();
        let damage = m.attack_damage();
        assert_eq!(damage, 3.0);
        m.add_effect(Effect::plain(10, 0, 100));
        m.add_effect(Effect::plain(12, 0, 100));
        let kinds = EffectKinds {
            speed: Some(10),
            weakness: Some(12),
            ..EffectKinds::default()
        };
        m.tick_effects(kinds);
        assert!((m.speed() - base * 1.2).abs() < 1e-9);
        assert_eq!(m.attack_damage(), 0.0);
        // Замедление складывается с ускорением: 1 + 0.2 − 0.15.
        m.add_effect(Effect::plain(11, 0, 100));
        let kinds = EffectKinds {
            speed: Some(10),
            slowness: Some(11),
            weakness: Some(12),
            ..EffectKinds::default()
        };
        m.tick_effects(kinds);
        assert!((m.speed() - base * 1.05).abs() < 1e-9);
    }

    #[test]
    fn jump_boost_raises_jump_and_safe_fall() {
        assert!((jump_power(None) - 0.42).abs() < 1e-9);
        assert!((jump_power(Some(0)) - 0.52).abs() < 1e-9);
        assert!((jump_power(Some(2)) - 0.72).abs() < 1e-9);
        assert_eq!(safe_fall_distance(3.0, None), 3.0);
        assert_eq!(safe_fall_distance(3.0, Some(0)), 4.0);
        assert_eq!(safe_fall_distance(3.0, Some(4)), 8.0);
        assert_eq!(fall_damage(10.0, safe_fall_distance(3.0, Some(3))), 3.0);
    }

    #[test]
    fn fall_damage_starts_above_safe_distance() {
        assert_eq!(fall_damage(3.0, 3.0), 0.0);
        assert_eq!(fall_damage(3.1, 3.0), 1.0);
        assert_eq!(fall_damage(10.0, 3.0), 7.0);
        assert_eq!(fall_damage(0.0, 3.0), 0.0);
        // Данные `DefaultAttributes` доходят до вида.
        assert_eq!(Species::Cow.safe_fall(), 3.0);
    }

    #[test]
    fn species_names() {
        assert_eq!(Species::from_name("minecraft:cow"), Some(Species::Cow));
        assert_eq!(Species::from_name("sheep"), Some(Species::Sheep));
        assert!(Species::Pig.is_food("minecraft:carrot"));
    }

    /// Зомби на полу y = 64 и игрок в `at`; `sight` — видимость.
    fn zombie_run(
        at: [f64; 3],
        sight: &dyn Fn([f64; 3], [f64; 3]) -> bool,
        ticks: usize,
    ) -> (Mob, Vec<usize>) {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Zombie, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        let prey = [Prey { uuid: 7, pos: at }];
        let players = prey;
        // 0.05: выбор цели (< 0.1) проходит, прогулка (< 1/120) — нет.
        let mut rnd = || 0.05;
        let mut s = Senses {
            players: &players,
            rand: &mut rnd,
            prey: &prey,
            tempters: &[],
            kin: &[],
            sight,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        let mut strikes = Vec::new();
        for t in 0..ticks {
            tick(&mut m, &w, &mut s);
            if m.strike == Some(7) {
                strikes.push(t);
            }
        }
        (m, strikes)
    }

    #[test]
    fn zombie_chases_and_strikes_every_second() {
        let (m, strikes) = zombie_run([8.5, 64.0, 0.5], &always_visible, 200);
        assert_eq!(m.target, Some(7));
        assert!(m.pos[0] > 6.0 && m.pos[0] < 8.5, "{:?}", m.pos);
        assert!(strikes.len() >= 4, "{strikes:?}");
        // Дойдя до игрока, зомби завершает путь: `MeleeAttackGoal` встаёт
        // (`canContinueToUse` — `!navigation.isDone()`) и через 20 тиков проверки
        // `canUse` запускается снова, а `start` обнуляет `ticksUntilNextAttack`.
        // Поэтому второй удар раньше (как в байткоде), дальше — ровно раз в 20.
        assert!(
            strikes[1] - strikes[0] <= ATTACK_INTERVAL as usize,
            "{strikes:?}"
        );
        for pair in strikes[1..].windows(2) {
            assert_eq!(pair[1] - pair[0], ATTACK_INTERVAL as usize, "{strikes:?}");
        }
    }

    /// Стена в два блока между зомби и игроком: раньше зомби упирался в неё,
    /// теперь обходит по пути и бьёт.
    #[test]
    fn zombie_walks_around_wall() {
        let wall = |p: [i32; 3]| p[1] < 64 || (p[0] == 3 && (-3..=3).contains(&p[2]) && p[1] < 66);
        let (shape, water) = flat_world(&wall);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Zombie, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        let at = [6.5, 64.0, 0.5];
        let prey = [Prey { uuid: 7, pos: at }];
        let players = prey;
        let mut rnd = || 0.05;
        let mut s = Senses {
            players: &players,
            rand: &mut rnd,
            prey: &prey,
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        let mut hit = None;
        for t in 0..400 {
            tick(&mut m, &w, &mut s);
            assert!(
                !(m.pos[0] + 0.3 > 3.0 && m.pos[0] - 0.3 < 4.0 && m.pos[2].abs() < 3.6),
                "inside the wall at {:?}",
                m.pos
            );
            if m.strike == Some(7) {
                hit = Some(t);
                break;
            }
        }
        assert!(hit.is_some(), "never reached the player: {:?}", m.pos);
    }

    /// Загон из `mob_path_probe`: забор-«П» (передняя стенка z−4, боковые x±3
    /// до z−8), зомби в нём, игрок снаружи. Ваниль 26.3 проходит клетки
    /// (0,−6) (1,−7) (2,−8) (2,−9) (3,−9) (4,−9) (4,−8)…(4,−3) (3,−2) (2,−1)
    /// (1,0) и подходит на 1.5 блока за 131 тик движения.
    #[test]
    fn zombie_leaves_fence_pen_like_vanilla() {
        let pen =
            |x: i32, z: i32| (z == -4 && x.abs() <= 3) || (x.abs() == 3 && (-8..=-5).contains(&z));
        const POST: [f64; 6] = [0.375, 0.0, 0.375, 0.625, 1.5, 0.625];
        let shapes: std::collections::HashMap<[i32; 3], Vec<[f64; 6]>> = (-3..=3)
            .flat_map(|x| (-8..=-4).map(move |z| [x, 64, z]))
            .filter(|p| pen(p[0], p[2]))
            .map(|p| {
                let mut v = vec![POST];
                let arm = |dx: i32, dz: i32| pen(p[0] + dx, p[2] + dz);
                if arm(1, 0) {
                    v.push([0.625, 0.0, 0.375, 1.0, 1.5, 0.625]);
                }
                if arm(-1, 0) {
                    v.push([0.0, 0.0, 0.375, 0.375, 1.5, 0.625]);
                }
                if arm(0, 1) {
                    v.push([0.375, 0.0, 0.625, 0.625, 1.5, 1.0]);
                }
                if arm(0, -1) {
                    v.push([0.375, 0.0, 0.0, 0.625, 1.5, 0.375]);
                }
                (p, v)
            })
            .collect();
        let shape = |p: [i32; 3]| -> &[[f64; 6]] {
            if p[1] < 64 {
                FULL
            } else {
                shapes.get(&p).map_or(&[], |v| v.as_slice())
            }
        };
        let cell = |p: [i32; 3]| {
            if p[1] < 64 {
                path::Cell {
                    kind: path::PathType::Blocked,
                    flags: path::Cell::SOLID,
                }
            } else if shapes.contains_key(&p) {
                path::Cell {
                    kind: path::PathType::Fence,
                    flags: path::Cell::SOLID | path::Cell::FENCE,
                }
            } else {
                path::Cell::AIR_CELL
            }
        };
        let w = World {
            shape: &shape,
            water: &|_| false,
            cell: Some(&cell),
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Zombie, [0.5, 64.0, -5.5], 1);
        m.on_ground = true;
        // После `/summon` зомби несколько тиков стоит: гравитация успевает
        // прижать его к полу, и первый шаг идёт уже с земли.
        settle(&mut m, &w);
        let at = [0.5, 64.0, 0.5];
        let prey = [Prey { uuid: 7, pos: at }];
        let players = prey;
        let mut rnd = || 0.05;
        let mut s = Senses {
            players: &players,
            rand: &mut rnd,
            prey: &prey,
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        // `mob_path_probe` на `server.jar`: (тик движения, x, z) относительно
        // игрока по шаговым дельтам `move_entity` (трекер шлёт раз в 3 тика).
        const VANILLA: &[(i32, f64, f64)] = &[
            (1, 0.0530, -6.0000),
            (3, 0.1895, -6.0984),
            (5, 0.3396, -6.2493),
            (6, 0.4158, -6.3323),
            (9, 0.6453, -6.5906),
            (12, 0.8833, -6.8455),
            (15, 1.1255, -7.0974),
            (18, 1.3687, -7.3486),
            (21, 1.6118, -7.5996),
            (24, 1.7520, -7.9087),
            (27, 1.8391, -8.2444),
            (30, 1.9143, -8.5854),
            (33, 2.1584, -8.7749),
            (36, 2.4810, -8.8806),
            (39, 2.8218, -8.9421),
            (42, 3.1697, -8.9663),
            (45, 3.5186, -8.9792),
            (48, 3.7771, -8.8542),
            (51, 3.8918, -8.5503),
            (54, 3.9492, -8.2114),
            (57, 3.9707, -7.8640),
            (60, 3.9792, -7.5149),
            (63, 3.9829, -7.1655),
            (66, 3.9854, -6.8159),
            (69, 3.9885, -6.4666),
            (72, 3.9905, -6.1169),
            (75, 3.9919, -5.7676),
            (78, 3.9939, -5.4180),
            (81, 3.9946, -5.0684),
            (84, 3.9954, -4.7188),
            (87, 3.9966, -4.3694),
            (90, 3.9971, -4.0198),
            (93, 3.9976, -3.6702),
            (96, 3.9678, -3.3301),
            (99, 3.7988, -3.0359),
            (102, 3.5925, -2.7563),
            (105, 3.3784, -2.4805),
            (108, 3.1536, -2.2134),
            (111, 2.9153, -1.9578),
            (114, 2.6743, -1.7046),
            (117, 2.4326, -1.4519),
            (120, 2.1895, -1.2009),
            (123, 1.9438, -0.9521),
            (126, 1.6980, -0.7039),
            (129, 1.4519, -0.4556),
        ];
        let start = m.pos;
        let mut moving = 0;
        let mut arrived = None;
        let mut worst = (0.0f64, 0);
        for _ in 0..400 {
            tick(&mut m, &w, &mut s);
            if m.pos == start && moving == 0 {
                continue;
            }
            moving += 1;
            assert_eq!(m.pos[1], 64.0, "сошёл с пола на тике {moving}");
            if let Some(&(_, x, z)) = VANILLA.iter().find(|v| v.0 == moving) {
                let d = (m.pos[0] - 0.5 - x).hypot(m.pos[2] - 0.5 - z);
                if d > worst.0 {
                    worst = (d, moving);
                }
            }
            if (m.pos[0] - at[0]).hypot(m.pos[2] - at[2]) < 1.5 {
                arrived = Some(moving);
                break;
            }
        }
        // Остаток в сотые доли блока набегает на повороте у узла (4,−9);
        // его не видно по 3-тиковым пакетам ванили, а клетки и время те же.
        assert!(
            worst.0 < 0.02,
            "отклонение {:.4} на тике {}",
            worst.0,
            worst.1
        );
        let t = arrived.expect("не дошёл");
        assert!((t - 131).abs() <= 2, "{t} тиков против 131 в ванили");
    }

    #[test]
    fn zombie_needs_sight_and_range() {
        let (m, strikes) = zombie_run([8.5, 64.0, 0.5], &|_, _| false, 100);
        assert_eq!(m.target, None);
        assert!(strikes.is_empty());
        // Дальше 35 блоков не замечает.
        let (m, _) = zombie_run([40.5, 64.0, 0.5], &always_visible, 100);
        assert_eq!(m.target, None);
        // Игрока ищут `getNearestPlayer` без рамки по высоте: на 8 блоков выше
        // в дальности и на виду — цель (прежняя рамка ±4 была не из ванили).
        let (m, _) = zombie_run([3.5, 72.0, 0.5], &always_visible, 20);
        assert_eq!(m.target, Some(7));
    }

    #[test]
    fn zombie_forgets_unseen_target() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Zombie, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        let prey = [Prey {
            uuid: 7,
            pos: [20.5, 64.0, 0.5],
        }];
        let visible = std::cell::Cell::new(true);
        let sight = |_: [f64; 3], _: [f64; 3]| visible.get();
        let mut rnd = || 0.05;
        let mut s = Senses {
            players: &prey,
            rand: &mut rnd,
            prey: &prey,
            tempters: &[],
            kin: &[],
            sight: &sight,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        tick(&mut m, &w, &mut s);
        assert_eq!(m.target, Some(7));
        // `TargetGoal.canContinueToUse` считает невидимые полные проходы
        // селектора (раз в 2 тика) до `reducedTickDelay(60)` = 30.
        visible.set(false);
        for _ in 0..UNSEEN_MEMORY {
            tick(&mut m, &w, &mut s);
        }
        assert_eq!(m.target, Some(7));
        for _ in 0..3 {
            tick(&mut m, &w, &mut s);
        }
        assert_eq!(m.target, None);
    }

    /// `HurtByTargetGoal`: обидчика берёт целью без взгляда и дальности поиска,
    /// зовёт своих (`alertOthers`); творческого (нет в `prey`) — нет.
    #[test]
    fn zombie_targets_attacker_and_alerts() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let player = Prey {
            uuid: 7,
            pos: [10.5, 64.0, 0.5],
        };
        for (prey, want) in [(vec![player], Some(7)), (vec![], None)] {
            let mut m = Mob::new(Species::Zombie, [0.5, 64.0, 0.5], 1);
            m.on_ground = true;
            let mut rnd = || 0.9;
            let mut s = Senses {
                players: &[player],
                rand: &mut rnd,
                prey: &prey,
                tempters: &[],
                kin: &[],
                sight: &|_, _| false,
                difficulty: 2,
                entity_id: 1,
                bright_outside: false,
                mob_griefing: false,
                jump_boost: None,
            };
            tick(&mut m, &w, &mut s);
            assert_eq!(m.target, None);
            hurt(&mut m, 1.0, Source::at(player.pos, Some(7)), KNOCKBACK);
            tick(&mut m, &w, &mut s);
            let mut alerted = m.out.contains(&Out::Alert(7));
            tick(&mut m, &w, &mut s);
            alerted |= m.out.contains(&Out::Alert(7));
            assert_eq!(m.target, want);
            assert_eq!(alerted, want.is_some());
        }
    }

    #[test]
    fn monsters_do_not_panic_and_babies_stay_babies() {
        let mut z = Mob::new(Species::Zombie, [0.5, 64.0, 0.5], 1);
        z.age = -1;
        hurt(&mut z, 1.0, Source::at([2.0, 64.0, 0.5], None), KNOCKBACK);
        assert!(z.vel[0] < 0.0, "knockback {:?}", z.vel);
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut rnd = || 0.9;
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        for _ in 0..10 {
            tick(&mut z, &w, &mut s);
        }
        assert!(!z.panicking());
        assert!(z.is_baby());
        assert!((z.speed() - 0.345).abs() < 1e-9);
        assert_eq!(Species::Zombie.xp(true, 0.0), 12);
        assert_eq!(Species::Zombie.xp(false, 0.0), 5);
        assert_eq!(Species::Cow.xp(false, 0.999), 3);
    }

    #[test]
    fn sky_darkens_at_night_and_in_rain() {
        // Полдень и полночь.
        assert_eq!(sky_darken(6000.0, 0.0, 0.0), 0);
        assert_eq!(sky_darken(18000.0, 0.0, 0.0), 11);
        // Рассвет нового мира (тик 0): таймлайн уже почти поднялся до 1.
        assert_eq!(sky_darken(0.0, 0.0, 0.0), 0);
        assert_eq!(sky_darken(3.0, 0.0, 0.0), 0);
        // Середина заката: 15 × (1 − 0.733 × 1133 / 1803) = 8.09.
        assert_eq!(sky_darken(13000.0, 0.0, 0.0), 6);
        // Дождь и гроза днём.
        assert_eq!(sky_darken(6000.0, 1.0, 0.0), 3);
        assert_eq!(sky_darken(6000.0, 1.0, 1.0), 5);
    }

    #[test]
    fn zombies_burn_only_in_bright_sun() {
        assert!(sun_burn_tick(15, 0, true, false, 0.0));
        // Бросок: (1.0 − 0.4) × 2 / 30 = 4 % на тик.
        assert!(!sun_burn_tick(15, 0, true, false, 0.05));
        assert!(!sun_burn_tick(15, 4, true, false, 0.0));
        assert!(!sun_burn_tick(15, 0, false, false, 0.0));
        assert!(!sun_burn_tick(15, 0, true, true, 0.0));
        // Яркость выше 0.5 — с уровня 12: во `float` 12/15 даёт 0.50000001,
        // как и в ванили (`LightTexture.getBrightness`).
        assert!(!sun_burn_tick(11, 0, true, false, 0.0));
        assert!(sun_burn_tick(12, 0, true, false, 0.0));
    }

    #[test]
    fn monster_light_rules() {
        let mut high = || 0.999;
        let mut low = || 0.0;
        // Днём под небом — нет: сырая яркость 15.
        assert!(!dark_enough(
            15,
            0,
            0,
            false,
            MonsterLight::OVERWORLD,
            &mut high
        ));
        // Ночью под небом: 15 − 11 = 4 ≤ 7 при высоком броске.
        assert!(dark_enough(
            15,
            0,
            11,
            false,
            MonsterLight::OVERWORLD,
            &mut high
        ));
        // Небесный свет 15 больше `nextInt(32)` = 0.
        assert!(!dark_enough(
            15,
            0,
            11,
            false,
            MonsterLight::OVERWORLD,
            &mut low
        ));
        // В пещере темно; факел (блочный свет 1+) запрещает.
        assert!(dark_enough(
            0,
            0,
            0,
            false,
            MonsterLight::OVERWORLD,
            &mut low
        ));
        assert!(!dark_enough(
            0,
            1,
            0,
            false,
            MonsterLight::OVERWORLD,
            &mut high
        ));
        // В Аду блочный свет до 7 не мешает.
        assert!(dark_enough(0, 7, 0, false, MonsterLight::NETHER, &mut low));
        assert!(!dark_enough(0, 8, 0, false, MonsterLight::NETHER, &mut low));
        assert!(bright_enough(15, 0));
        assert!(!bright_enough(0, 8));
    }

    #[test]
    fn despawn_rules() {
        let mut z = Mob::new(Species::Zombie, [0.0; 3], 1);
        assert!(check_despawn(&mut z, Some(129.0 * 129.0), false, 0.5));
        assert!(!check_despawn(&mut z, None, false, 0.0));
        z.no_action = 700;
        assert!(check_despawn(&mut z, Some(40.0 * 40.0), false, 0.0));
        assert!(!check_despawn(&mut z, Some(40.0 * 40.0), false, 0.5));
        assert!(!check_despawn(&mut z, Some(10.0 * 10.0), false, 0.0));
        assert_eq!(z.no_action, 0);
        assert!(check_despawn(&mut z, Some(1.0), true, 0.5));
        let mut named = Mob::new(Species::Zombie, [0.0; 3], 1);
        named.custom_name = Some("Боб".into());
        assert!(!check_despawn(&mut named, Some(1e9), false, 0.0));
        let mut cow = Mob::new(Species::Cow, [0.0; 3], 1);
        assert!(!check_despawn(&mut cow, Some(1e9), true, 0.0));
    }

    #[test]
    fn spawn_packs_cap_and_unknown_species() {
        let zombie = [SpawnEntry {
            species: Some(Species::Zombie),
            weight: 95,
            min: 4,
            max: 4,
        }];
        let mut rnd = || 0.99;
        let got = spawn_packs(
            [8, 64, 8],
            &zombie,
            &mut |_| Some(1000.0),
            &mut |_, _, _| true,
            &mut rnd,
        );
        assert_eq!(got.len(), MAX_CLUSTER as usize);
        assert!(got.iter().all(|(s, p)| *s == Species::Zombie && p[1] == 64));
        // Рядом с игроком (ближе 24) — никого.
        let got = spawn_packs(
            [8, 64, 8],
            &zombie,
            &mut |_| Some(100.0),
            &mut |_, _, _| true,
            &mut rnd,
        );
        assert!(got.is_empty());
        // Только неизвестный вид — никого, и место даже не проверяется.
        let other = [SpawnEntry {
            species: None,
            weight: 100,
            min: 4,
            max: 4,
        }];
        let got = spawn_packs(
            [8, 64, 8],
            &other,
            &mut |_| Some(1000.0),
            &mut |_, _, _| panic!("unknown species checked"),
            &mut rnd,
        );
        assert!(got.is_empty());
        assert_eq!(mob_cap(Category::Monster, 289), 70);
        assert_eq!(mob_cap(Category::Creature, 578), 20);
        assert_eq!(spawn_cap_chunks(&[(3, -2)]), 289);
        assert_eq!(mob_cap(Category::Monster, spawn_cap_chunks(&[(3, -2)])), 70);
        // Соседние игроки делят квадраты: 17 × 18, а не 2 × 289.
        assert_eq!(spawn_cap_chunks(&[(0, 0), (1, 0)]), 306);
        assert_eq!(spawn_cap_chunks(&[]), 0);
    }

    #[test]
    fn pick_entry_keeps_weights() {
        let e = [
            SpawnEntry {
                species: Some(Species::Zombie),
                weight: 95,
                min: 4,
                max: 4,
            },
            SpawnEntry {
                species: None,
                weight: 100,
                min: 4,
                max: 4,
            },
        ];
        assert_eq!(pick_entry(&e, 0.0).unwrap().species, Some(Species::Zombie));
        assert_eq!(
            pick_entry(&e, 94.0 / 195.0).unwrap().species,
            Some(Species::Zombie)
        );
        assert_eq!(pick_entry(&e, 96.0 / 195.0).unwrap().species, None);
        assert_eq!(pick_entry(&e, 0.99999).unwrap().species, None);
    }

    #[test]
    fn spawn_space_checks() {
        let ground = |p: [i32; 3]| if p[1] < 64 { FULL } else { &[] as &[[f64; 6]] };
        assert!(box_free([0.5, 64.0, 0.5], 0.6, 1.95, &ground));
        assert!(!box_free([0.5, 63.5, 0.5], 0.6, 1.95, &ground));
        let low_ceiling = |p: [i32; 3]| {
            if p[1] < 64 || p[1] == 65 {
                FULL
            } else {
                &[] as &[[f64; 6]]
            }
        };
        assert!(!box_free([0.5, 64.0, 0.5], 0.6, 1.95, &low_ceiling));
        assert!(sturdy_top(FULL));
        assert!(!sturdy_top(&[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]));
        assert!(full_block(FULL));
        assert!(!full_block(&[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]));
        assert!(spawn_floor_forbidden("minecraft:oak_leaves"));
        assert!(spawn_floor_forbidden("minecraft:white_stained_glass"));
        assert!(!spawn_floor_forbidden("minecraft:stone"));
        assert!(spawn_cell_forbidden("minecraft:powered_rail"));
        assert!(!spawn_cell_forbidden("minecraft:short_grass"));
    }

    #[test]
    fn zombie_name_round_trip() {
        assert_eq!(
            Species::from_name("minecraft:zombie"),
            Some(Species::Zombie)
        );
        assert_eq!(Species::Zombie.short(), "zombie");
        assert!(Species::Zombie.is_monster());
    }

    /// Прогон монстра `species` на мире `solid` против игрока в `at`.
    /// Возвращает моба и события тиков (`Out`) с номерами тиков.
    #[allow(clippy::too_many_arguments)]
    fn monster_run(
        species: Species,
        from: [f64; 3],
        at: [f64; 3],
        solid: &dyn Fn([i32; 3]) -> bool,
        brightness: f32,
        difficulty: u8,
        rnd: &mut dyn FnMut() -> f64,
        ticks: usize,
    ) -> (Mob, Vec<(usize, Out)>) {
        let (shape, water) = flat_world(solid);
        let info = move |_: [i32; 3]| CellInfo {
            brightness,
            ..CellInfo::default()
        };
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: Some(&info),
            min_y: -64,
        };
        let mut m = Mob::new(species, from, 1);
        m.on_ground = true;
        let prey = [Prey { uuid: 7, pos: at }];
        let mut s = Senses {
            players: &prey,
            rand: rnd,
            prey: &prey,
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        let mut outs = Vec::new();
        for t in 0..ticks {
            let gone = tick(&mut m, &w, &mut s);
            outs.extend(m.out.iter().map(|o| (t, *o)));
            if gone {
                break;
            }
        }
        (m, outs)
    }

    /// `SwellGoal`: в двух блоках крипер встаёт, шипит один раз и через
    /// `maxSwell` = 30 тиков набухания взрывается с мощностью 3.
    #[test]
    fn creeper_swells_and_explodes() {
        let mut rnd = || 0.05;
        let (m, outs) = monster_run(
            Species::Creeper,
            [0.5, 64.0, 0.5],
            [2.5, 64.0, 0.5],
            &floor,
            0.0,
            2,
            &mut rnd,
            100,
        );
        let primed: Vec<usize> = outs
            .iter()
            .filter(|(_, o)| *o == Out::Primed)
            .map(|(t, _)| *t)
            .collect();
        let boom: Vec<(usize, Out)> = outs
            .iter()
            .filter(|(_, o)| matches!(o, Out::Explode(_)))
            .copied()
            .collect();
        assert_eq!(primed.len(), 1, "{outs:?}");
        assert_eq!(boom.len(), 1, "{outs:?}");
        // Набухание считается в `Creeper.tick` до ИИ: шипение на тике после
        // `SwellGoal.tick`, взрыв ещё через 29.
        assert_eq!(boom[0].0 - primed[0], 29, "{outs:?}");
        assert_eq!(boom[0].1, Out::Explode(3.0));
        assert_eq!(m.swell, 30);
    }

    /// Без цели крипер сдувается; заряженный бахает вдвое сильнее.
    #[test]
    fn creeper_deflates_and_powered_doubles() {
        let mut m = Mob::new(Species::Creeper, [0.5, 64.0, 0.5], 1);
        m.swell = 10;
        let mut rnd = || 0.99;
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut s = Senses {
            players: &[],
            rand: &mut rnd,
            prey: &[],
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        for _ in 0..12 {
            assert!(!tick(&mut m, &w, &mut s));
        }
        assert_eq!(m.swell, 0);
        let mut c = Mob::new(Species::Creeper, [0.5, 64.0, 0.5], 1);
        c.powered = true;
        c.ignited = true;
        c.swell = 29;
        assert!(creeper_tick(&mut c));
        assert_eq!(c.out, vec![Out::Explode(6.0)]);
    }

    /// `RangedBowAttackGoal`: натяжение 20 тиков, затем пауза 40 (20 на сложной).
    #[test]
    fn skeleton_shoots_at_bow_interval() {
        for (difficulty, period) in [(2u8, 60usize), (3, 40)] {
            let mut rnd = lcg(5);
            let (m, outs) = monster_run(
                Species::Skeleton,
                [0.5, 64.0, 0.5],
                [8.5, 64.0, 0.5],
                &floor,
                0.0,
                difficulty,
                &mut rnd,
                300,
            );
            let shots: Vec<usize> = outs
                .iter()
                .filter_map(|(t, o)| match o {
                    Out::Shoot { target: 7, power } => {
                        assert_eq!(*power, 1.0);
                        Some(*t)
                    }
                    _ => None,
                })
                .collect();
            assert!(shots.len() >= 3, "{shots:?}");
            for pair in shots.windows(2) {
                assert_eq!(pair[1] - pair[0], period, "{difficulty}: {shots:?}");
            }
            assert!(m.aggressive);
        }
        assert_eq!(goals::bow_power(20), 1.0);
        assert!((goals::bow_power(10) - 0.41666666).abs() < 1e-6);
    }

    /// Видя цель 20 тиков в 15 блоках, скелет встаёт и кружит боком.
    #[test]
    fn skeleton_strafes_in_range() {
        let mut rnd = lcg(9);
        let (m, _) = monster_run(
            Species::Skeleton,
            [0.5, 64.0, 0.5],
            [8.5, 64.0, 0.5],
            &floor,
            0.0,
            2,
            &mut rnd,
            60,
        );
        assert!(m.nav.is_done());
        assert!(m.xxa.abs() > 0.1, "xxa = {}", m.xxa);
        assert_eq!(m.pos[1], 64.0);
    }

    /// Паук, упёршись в стену, лезет вверх (`onClimbable`: 0.2 вверх) и
    /// забирается на неё.
    #[test]
    fn spider_climbs_wall() {
        let wall = |p: [i32; 3]| p[1] < 64 || (p[0] >= 3 && p[1] < 68);
        let mut rnd = lcg(3);
        let (m, _) = monster_run(
            Species::Spider,
            [0.5, 64.0, 0.5],
            [6.5, 68.0, 0.5],
            &wall,
            0.0,
            2,
            &mut rnd,
            200,
        );
        assert!(m.pos[1] >= 68.0, "{:?}", m.pos);
        assert!(m.pos[0] > 3.0, "{:?}", m.pos);
    }

    /// Днём (яркость ≥ 0.5) паук не берёт цель, ночью — берёт.
    #[test]
    fn spider_neutral_in_light() {
        for (light, hunts) in [(1.0, false), (0.0, true)] {
            let mut rnd = || 0.05;
            let (m, _) = monster_run(
                Species::Spider,
                [0.5, 64.0, 0.5],
                [6.5, 64.0, 0.5],
                &floor,
                light,
                2,
                &mut rnd,
                20,
            );
            assert_eq!(m.target.is_some(), hunts, "яркость {light}");
        }
    }

    /// `LeapAtTargetGoal`: в 2–4 блоках от цели паук прыгает на 0.4 вверх и к ней.
    #[test]
    fn spider_leaps_at_target() {
        let (shape, water) = flat_world(&floor);
        let w = World {
            shape: &*shape,
            water: &*water,
            cell: None,
            info: None,
            min_y: -64,
        };
        let mut m = Mob::new(Species::Spider, [0.5, 64.0, 0.5], 1);
        m.on_ground = true;
        m.target = Some(7);
        let prey = [Prey {
            uuid: 7,
            pos: [3.5, 64.0, 0.5],
        }];
        let mut rnd = || 0.0;
        let mut s = Senses {
            players: &prey,
            rand: &mut rnd,
            prey: &prey,
            tempters: &[],
            kin: &[],
            sight: &always_visible,
            difficulty: 2,
            entity_id: 1,
            bright_outside: false,
            mob_griefing: false,
            jump_boost: None,
        };
        let y0 = m.pos[1];
        tick(&mut m, &w, &mut s);
        assert!(m.pos[1] > y0 + 0.3, "{:?}", m.pos);
        assert!(m.vel[0] > 0.2, "{:?}", m.vel);
    }
}
