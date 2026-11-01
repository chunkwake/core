//! Профиль тика: сколько миллисекунд уходит на каждую подсистему.
//!
//! Замеры ставятся в `tick_once` между вызовами подсистем: разница
//! `Instant` — это и есть «работа такой-то фазы». Так же считает spark, и
//! это не требует трогать внутренности подсистем, где пришлось бы
//! протаскивать профайлер через десятки вызовов.
//!
//! Хранение секундными корзинами, а не по тикам: 20 тиков в секунду дают
//! 18 000 замеров на фазу за 15 минут, а графику нужна та же секундная
//! сетка, что и остальным панелям. В корзине копятся сумма и максимум,
//! поэтому видно и среднюю нагрузку, и разовый всплеск.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Сколько секунд профиля помним: как и остальная история, 15 минут.
pub const PROFILE_SECONDS: usize = 900;

/// Индексы фаз. Порядок совпадает с [`PHASES`] и с порядком замеров в тике.
pub const PH_REFRESH: usize = 0;
pub const PH_ITEMS: usize = 1;
pub const PH_ARROWS: usize = 2;
pub const PH_MOBS: usize = 3;
pub const PH_ANIMALS: usize = 4;
pub const PH_ORBS: usize = 5;
pub const PH_FURNACES: usize = 6;
pub const PH_CAMPFIRES: usize = 7;
pub const PH_DEFERRED: usize = 8;
pub const PH_PLATES: usize = 9;
pub const PH_FLUIDS: usize = 10;
pub const PH_PISTONS: usize = 11;
pub const PH_HOPPERS: usize = 12;
pub const PH_SCHEDULED: usize = 13;
pub const PH_BLOCKS: usize = 14;
pub const PH_MOVES: usize = 15;
pub const PH_PLUGINS: usize = 16;
pub const PH_TIME: usize = 17;

/// Сколько фаз в профиле.
pub const PHASE_COUNT: usize = 18;

/// Ключи фаз: стабильные имена для JSON, CLI и подписей.
///
/// Ключи английские, потому что их видят скрипты и `--json`; подписи для
/// людей лежат в [`PHASE_LABELS`].
pub const PHASES: [&str; PHASE_COUNT] = [
    "refresh",
    "items",
    "arrows",
    "mobs",
    "animals",
    "orbs",
    "furnaces",
    "campfires",
    "deferred",
    "plates",
    "fluids",
    "pistons",
    "hoppers",
    "scheduled",
    "blocks",
    "moves",
    "plugins",
    "time",
];

/// Подписи фаз для человека.
///
/// Они нужны только в подробном режиме: по умолчанию дашборд показывает
/// [`GROUPS`], потому что «плиты» и «отложенные правки» — это внутренние
/// имена, а не ответ на вопрос «что тормозит тик».
pub const PHASE_LABELS: [&str; PHASE_COUNT] = [
    "чанки симуляции",
    "предметы",
    "стрелы",
    "мобы",
    "животные",
    "сферы опыта",
    "печи",
    "костры",
    "правки плагинов",
    "нажимные плиты",
    "жидкости",
    "поршни",
    "воронки",
    "редстоун",
    "отправка блоков",
    "отправка позиций",
    "плагины",
    "часы мира",
];

/// Понятная группа фаз: то, что видит админ, а не внутренние имена.
///
/// Разбивка на 18 фаз нужна, чтобы найти виновника, но читать её как
/// список «плиты, отложенные правки, рассылка блоков» невозможно: это
/// устройство кода, а не то, о чём думает владелец сервера. Группы
/// отвечают на его вопрос («мобы тормозят» или «механизмы»), а точные
/// фазы остаются в подробном режиме.
#[derive(Clone, Copy, Debug)]
pub struct PhaseGroup {
    /// Стабильный ключ для JSON.
    pub key: &'static str,
    /// Название для человека.
    pub label: &'static str,
    /// Пояснение в одну строку: что именно сюда входит.
    pub hint: &'static str,
    /// Фазы, из которых складывается группа.
    pub phases: &'static [usize],
}

/// Сколько групп в понятной разбивке.
pub const GROUP_COUNT: usize = 7;

/// Группы в порядке показа: от самого «дорогого» для сервера к служебному.
pub const GROUPS: [PhaseGroup; GROUP_COUNT] = [
    PhaseGroup {
        key: "mobs",
        label: "Мобы и животные",
        hint: "ИИ, движение, спавн",
        phases: &[PH_MOBS, PH_ANIMALS],
    },
    PhaseGroup {
        key: "machines",
        label: "Хопперы и механизмы",
        hint: "печи, костры, поршни, редстоун, жидкости",
        phases: &[
            PH_FURNACES,
            PH_CAMPFIRES,
            PH_HOPPERS,
            PH_SCHEDULED,
            PH_PLATES,
            PH_DEFERRED,
            PH_PISTONS,
            PH_FLUIDS,
        ],
    },
    PhaseGroup {
        key: "items",
        label: "Предметы и снаряды",
        hint: "дроп на земле, стрелы, опыт",
        phases: &[PH_ITEMS, PH_ARROWS, PH_ORBS],
    },
    PhaseGroup {
        key: "players",
        label: "Отправка игрокам",
        hint: "пакеты о блоках и позициях",
        phases: &[PH_BLOCKS, PH_MOVES],
    },
    PhaseGroup {
        key: "chunks",
        label: "Чанки в симуляции",
        hint: "пересчёт набора чанков рядом с игроками",
        phases: &[PH_REFRESH],
    },
    PhaseGroup {
        key: "plugins",
        label: "Плагины",
        hint: "код плагинов в тике",
        phases: &[PH_PLUGINS],
    },
    PhaseGroup {
        key: "other",
        label: "Служебное",
        hint: "часы мира и прочая мелочь",
        phases: &[PH_TIME],
    },
];

/// Секунда профиля: сумма и максимум по каждой фазе.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ProfileBucket {
    /// Начало секунды, unix-время.
    pub at: u64,
    /// Сколько тиков попало в секунду.
    pub ticks: u32,
    /// Суммарное время фазы за секунду, мс.
    pub total: [f32; PHASE_COUNT],
    /// Худший тик секунды по этой фазе, мс.
    pub max: [f32; PHASE_COUNT],
}

impl ProfileBucket {
    /// Сумма всех фаз за секунду, мс: это «сколько занял тик в сумме».
    pub fn grand_total(&self) -> f32 {
        self.total.iter().sum()
    }

    /// Среднее время на тик для фазы, мс.
    pub fn per_tick(&self, phase: usize) -> f32 {
        if self.ticks == 0 {
            0.0
        } else {
            self.total[phase] / self.ticks as f32
        }
    }

    /// Доля фазы в общей работе, 0..1.
    pub fn share(&self, phase: usize) -> f32 {
        let all = self.grand_total();
        if all <= 0.0 {
            0.0
        } else {
            self.total[phase] / all
        }
    }

    /// Время группы на тик, мс: сумма её фаз, делённая на число тиков.
    pub fn group_per_tick(&self, group: usize) -> f32 {
        let Some(entry) = GROUPS.get(group) else {
            return 0.0;
        };
        if self.ticks == 0 {
            return 0.0;
        }
        let sum: f32 = entry.phases.iter().map(|p| self.total[*p]).sum();
        sum / self.ticks as f32
    }

    /// Все группы по убыванию времени на тик: так их и показываем.
    ///
    /// Возвращает индексы [`GROUPS`], а не копии: подписи берёт вызывающий,
    /// чтобы не тащить строки через каждый кадр.
    pub fn groups_by_cost(&self) -> [usize; GROUP_COUNT] {
        let mut order = [0usize; GROUP_COUNT];
        for (slot, value) in order.iter_mut().enumerate() {
            *value = slot;
        }
        order.sort_by(|a, b| self.group_per_tick(*b).total_cmp(&self.group_per_tick(*a)));
        order
    }

    /// Сколько всего заняла секунда, мс на тик.
    pub fn total_per_tick(&self) -> f32 {
        if self.ticks == 0 {
            0.0
        } else {
            self.grand_total() / self.ticks as f32
        }
    }
}

/// Накопитель профиля: закрытые секунды и та, что ещё идёт.
#[derive(Debug)]
pub struct TickProfile {
    closed: VecDeque<ProfileBucket>,
    open: ProfileBucket,
}

impl Default for TickProfile {
    fn default() -> Self {
        Self::new()
    }
}

impl TickProfile {
    pub fn new() -> Self {
        Self {
            closed: VecDeque::with_capacity(PROFILE_SECONDS),
            open: ProfileBucket::default(),
        }
    }

    /// Записывает замер одного тика с отметкой времени в секундах.
    ///
    /// Смена секунды закрывает прошлую корзину и открывает новую. Тик,
    /// попавший ровно на границу, считается по своей секунде — так сумма
    /// по корзинам никогда не теряет тики.
    pub fn record(&mut self, at: u64, marks: &[f32; PHASE_COUNT]) {
        if self.open.ticks > 0 && self.open.at != at {
            if self.closed.len() == PROFILE_SECONDS {
                self.closed.pop_front();
            }
            self.closed.push_back(self.open);
            self.open = ProfileBucket {
                at,
                ..ProfileBucket::default()
            };
        }
        if self.open.ticks == 0 {
            self.open.at = at;
        }
        for (phase, &value) in marks.iter().enumerate() {
            let value = value.max(0.0);
            self.open.total[phase] += value;
            self.open.max[phase] = self.open.max[phase].max(value);
        }
        self.open.ticks += 1;
    }

    /// Закрытые секунды, от старых к новым.
    pub fn history(&self) -> Vec<ProfileBucket> {
        self.closed.iter().copied().collect()
    }

    /// Последняя закрытая секунда: её и отдаёт поток раз в секунду.
    pub fn latest(&self) -> Option<ProfileBucket> {
        self.closed.back().copied()
    }

    /// Сколько тиков в незакрытой корзине (для тестов и диагностики).
    pub fn open_ticks(&self) -> u32 {
        self.open.ticks
    }
}

/// Секундомер фаз тика.
///
/// Один `Instant` на тик и `lap` после каждой подсистемы: `lap` начисляет
/// прошедшее время названной фазе и запоминает новую точку. Так замер
/// стоит одно чтение часов на фазу, а не пара вокруг каждой подсистемы.
#[derive(Debug)]
pub struct Stopwatch {
    marks: [f32; PHASE_COUNT],
    last: std::time::Instant,
}

impl Stopwatch {
    pub fn start() -> Self {
        Self {
            marks: [0.0; PHASE_COUNT],
            last: std::time::Instant::now(),
        }
    }

    /// Начисляет прошедшее с прошлого `lap` время фазе `phase`, мс.
    pub fn lap(&mut self, phase: usize) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64() * 1000.0;
        self.last = now;
        if let Some(slot) = self.marks.get_mut(phase) {
            *slot += elapsed as f32;
        }
    }

    /// Замеры для [`TickProfile::record`].
    pub fn marks(&self) -> [f32; PHASE_COUNT] {
        self.marks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(value: f32) -> [f32; PHASE_COUNT] {
        [value; PHASE_COUNT]
    }

    #[test]
    fn ticks_of_one_second_fold_into_one_bucket() {
        let mut profile = TickProfile::new();
        for _ in 0..20 {
            profile.record(100, &marks(0.5));
        }
        // Секунда ещё не закрыта: показывать нечего, она копится.
        assert!(profile.latest().is_none());
        assert_eq!(profile.open_ticks(), 20);

        profile.record(101, &marks(0.5));
        let bucket = profile.latest().expect("секунда 100 закрыта");
        assert_eq!(bucket.at, 100);
        assert_eq!(bucket.ticks, 20);
        assert!((bucket.total[PH_MOBS] - 10.0).abs() < 1e-6, "20 × 0.5 мс");
        assert!((bucket.per_tick(PH_MOBS) - 0.5).abs() < 1e-6);
        assert!((bucket.max[PH_MOBS] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn max_keeps_the_worst_tick() {
        let mut profile = TickProfile::new();
        profile.record(5, &marks(0.2));
        profile.record(5, &marks(4.0));
        profile.record(6, &marks(0.0));
        let bucket = profile.latest().unwrap();
        assert!((bucket.total[PH_ITEMS] - 4.2).abs() < 1e-6);
        assert!((bucket.max[PH_ITEMS] - 4.0).abs() < 1e-6);
        assert!((bucket.per_tick(PH_ITEMS) - 2.1).abs() < 1e-6);
    }

    #[test]
    fn share_is_part_of_the_total_work() {
        let mut bucket = ProfileBucket {
            at: 1,
            ticks: 1,
            ..ProfileBucket::default()
        };
        bucket.total[PH_FLUIDS] = 3.0;
        bucket.total[PH_MOBS] = 1.0;
        assert!((bucket.grand_total() - 4.0).abs() < 1e-6);
        assert!((bucket.share(PH_FLUIDS) - 0.75).abs() < 1e-6);
        assert_eq!(bucket.per_tick(PH_FLUIDS), 3.0);
    }

    #[test]
    fn empty_bucket_divides_by_zero_safely() {
        let bucket = ProfileBucket::default();
        assert_eq!(bucket.per_tick(PH_MOBS), 0.0);
        assert_eq!(bucket.share(PH_MOBS), 0.0);
        assert_eq!(bucket.grand_total(), 0.0);
    }

    #[test]
    fn ring_keeps_fifteen_minutes() {
        let mut profile = TickProfile::new();
        // Последняя записанная секунда остаётся открытой (она ещё идёт),
        // поэтому закрытых корзин на одну меньше числа записанных.
        let total = PROFILE_SECONDS as u64 + 30;
        for at in 0..total {
            profile.record(at, &marks(1.0));
        }
        let history = profile.history();
        assert_eq!(history.len(), PROFILE_SECONDS);
        // Закрытых корзин 929, лишние 29 вытеснены — первая остаётся секундой 29.
        assert_eq!(history[0].at, 29, "старые секунды вытеснены");
        assert_eq!(
            history.last().unwrap().at,
            total - 2,
            "предпоследняя закрыта"
        );
        assert_eq!(profile.open_ticks(), 1, "последняя ещё копится");
    }

    /// Отрицательные замеры (часы ОС могут дать минус) не ломают сумму.
    #[test]
    fn negative_marks_are_clamped() {
        let mut profile = TickProfile::new();
        profile.record(1, &marks(-5.0));
        profile.record(2, &marks(1.0));
        let bucket = profile.history().first().copied().unwrap();
        assert_eq!(bucket.total[PH_TIME], 0.0);
    }

    /// Имена и подписи идут в одном порядке: иначе UI подпишет чужую фазу.
    #[test]
    fn phase_tables_agree() {
        assert_eq!(PHASES.len(), PHASE_COUNT);
        assert_eq!(PHASE_LABELS.len(), PHASE_COUNT);
        assert_eq!(PHASES[PH_FLUIDS], "fluids");
        assert_eq!(PHASES[PH_TIME], "time");
    }

    /// Каждая фаза входит ровно в одну группу: иначе время либо посчитается
    /// дважды, либо потеряется, и «итого» разойдётся с суммой групп.
    #[test]
    fn groups_cover_every_phase_once() {
        let mut seen = [0u8; PHASE_COUNT];
        for group in &GROUPS {
            for phase in group.phases {
                assert!(*phase < PHASE_COUNT, "{}: фаза {phase}", group.key);
                seen[*phase] += 1;
            }
        }
        for (phase, count) in seen.iter().enumerate() {
            assert_eq!(*count, 1, "фаза {} в {} группах", PHASES[phase], count);
        }
    }

    /// Группы считают время тика так же, как сумма фаз: расхождение значило
    /// бы, что на графике и в таблице разные числа.
    #[test]
    fn groups_add_up_to_the_same_total() {
        let mut bucket = ProfileBucket {
            at: 1,
            ticks: 20,
            ..ProfileBucket::default()
        };
        for (phase, value) in bucket.total.iter_mut().enumerate() {
            *value = (phase + 1) as f32;
        }
        let by_phases: f32 = bucket.total.iter().sum::<f32>() / 20.0;
        let by_groups: f32 = (0..GROUP_COUNT).map(|g| bucket.group_per_tick(g)).sum();
        assert!(
            (by_phases - by_groups).abs() < 1e-4,
            "{by_phases} vs {by_groups}"
        );
        assert!((bucket.total_per_tick() - by_phases).abs() < 1e-4);
    }

    /// Порядок групп — по убыванию стоимости, самая тяжёлая первой.
    #[test]
    fn groups_sort_by_cost() {
        let mut bucket = ProfileBucket {
            at: 1,
            ticks: 10,
            ..ProfileBucket::default()
        };
        bucket.total[PH_ORBS] = 50.0; // предметы
        bucket.total[PH_MOBS] = 10.0; // мобы
        let order = bucket.groups_by_cost();
        let first = &GROUPS[order[0]];
        assert_eq!(first.key, "items", "предметы дороже мобов");
        let costs: Vec<f32> = order.iter().map(|g| bucket.group_per_tick(*g)).collect();
        for pair in costs.windows(2) {
            assert!(pair[0] >= pair[1], "порядок по убыванию: {costs:?}");
        }
    }

    /// Секундомер начисляет время той фазе, которую назвали, и не теряет
    /// остаток: сумма всех `lap` равна полному времени работы.
    #[test]
    fn stopwatch_accumulates_per_phase() {
        let mut watch = Stopwatch::start();
        std::thread::sleep(std::time::Duration::from_millis(12));
        watch.lap(PH_MOBS);
        std::thread::sleep(std::time::Duration::from_millis(12));
        watch.lap(PH_FLUIDS);
        watch.lap(PH_TIME);
        let marks = watch.marks();
        assert!(marks[PH_MOBS] >= 10.0, "мобы: {}", marks[PH_MOBS]);
        assert!(marks[PH_FLUIDS] >= 10.0, "жидкости: {}", marks[PH_FLUIDS]);
        // Время между последним lap и вызовом `marks` не приписано никому.
        let total: f32 = marks.iter().sum();
        assert!((20.0..100.0).contains(&total), "сумма фаз: {total}");
    }
}
