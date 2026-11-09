//! Ванильные селекторы целей: `@s`, `@p`, `@r`, `@a`, `@e` с аргументами.
//!
//! Раньше цели умели только `@s`/`@a`/имя, а `@p` и `@r` не различались.
//! Селектор здесь разбирается в фильтр и применяется к снимку целей: мир
//! отдаёт список, модуль отбирает и сортирует — так его можно тестировать
//! без сервера.

use super::args::ArgError;

/// Кого видит команда: игрок или сущность.
#[derive(Clone, Debug)]
pub struct Target {
    pub uuid: uuid::Uuid,
    pub name: String,
    pub entity_id: i32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// `None` — моб или предмет, а не игрок.
    pub game_mode: Option<u8>,
    /// Метки сущности (`Tags`); у игроков пусто.
    pub tags: Vec<String>,
}

/// Вид селектора.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `@s` — тот, кто выполнил команду.
    Self_,
    /// `@p` — ближайший игрок.
    Nearest,
    /// `@r` — случайный игрок.
    Random,
    /// `@a` — все игроки.
    AllPlayers,
    /// `@e` — все сущности.
    AllEntities,
    /// Имя игрока.
    Named,
}

/// Порядок выдачи нескольких целей.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Arbitrary,
    Nearest,
    Furthest,
    Random,
}

/// Разобранный селектор.
#[derive(Clone, Debug)]
pub struct Selector {
    pub kind: Kind,
    pub name: String,
    /// Только одна цель (у `@p`, `@r` и явного `limit=1`).
    pub single: bool,
    /// Разрешены не-игроки (`@e` и `@a[distance=…]` в ваниле — нет).
    pub entities: bool,
    pub sort: Sort,
    pub limit: Option<usize>,
    /// Начало отсчёта для `distance`, `dx/dy/dz`, `sort` и относительных координат.
    pub origin: Option<[f64; 3]>,
    pub yaw: Option<f32>,
    pub pitch: Option<f32>,
    pub distance: Option<(f64, f64)>,
    pub level: Option<(i32, i32)>,
    pub volume: Option<[f64; 3]>,
    pub kind_filter: Option<String>,
    pub gamemode: Option<u8>,
    pub name_filter: Option<String>,
    pub tags: Vec<(String, bool)>,
    pub scores: Vec<(String, Option<(i32, i32)>)>,
}

impl Selector {
    /// Разбор токена селектора или имени игрока.
    pub fn parse(token: &str) -> Result<Self, ArgError> {
        let (kind, body) = match token.split_once('[') {
            Some((head, tail)) => {
                let args = tail
                    .strip_suffix(']')
                    .ok_or_else(|| ArgError::new("argument.entity.options.unterminated"))?;
                (head, Some(args))
            }
            None => (token, None),
        };
        let kind = match kind {
            "@s" => Kind::Self_,
            "@p" => Kind::Nearest,
            "@r" => Kind::Random,
            "@a" => Kind::AllPlayers,
            "@e" => Kind::AllEntities,
            other if other.starts_with('@') => {
                return Err(ArgError::new("argument.entity.selector.unknown"));
            }
            _ => Kind::Named,
        };
        let single = matches!(kind, Kind::Nearest | Kind::Random | Kind::Named);
        let mut selector = Self {
            kind,
            name: if kind == Kind::Named {
                token.to_owned()
            } else {
                String::new()
            },
            single,
            entities: kind == Kind::AllEntities,
            sort: if matches!(kind, Kind::Nearest | Kind::Named) {
                Sort::Nearest
            } else {
                Sort::Arbitrary
            },
            limit: None,
            origin: None,
            yaw: None,
            pitch: None,
            distance: None,
            level: None,
            volume: None,
            kind_filter: None,
            gamemode: None,
            name_filter: None,
            tags: Vec::new(),
            scores: Vec::new(),
        };
        let Some(args) = body else {
            return Ok(selector);
        };
        for pair in args.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| ArgError::new("argument.entity.options.missing"))?;
            selector.apply_option(key.trim(), value.trim())?;
        }
        Ok(selector)
    }

    fn apply_option(&mut self, key: &str, value: &str) -> Result<(), ArgError> {
        let negated = key.starts_with('!');
        let key = key.trim_start_matches('!');
        match key {
            "type" => {
                if negated {
                    return Err(ArgError::new("argument.entity.options.type.invalid"));
                }
                self.kind_filter = Some(value.to_owned());
                self.entities = true;
            }
            "name" => {
                let name = value.trim_matches('"');
                if negated {
                    self.name_filter = Some(format!("!{name}"));
                } else {
                    self.name_filter = Some(name.to_owned());
                }
            }
            "distance" | "level" => {
                let range = range_of(value)?;
                if key == "distance" {
                    self.distance = Some(range);
                } else {
                    self.level = Some((
                        range.0.max(i32::MIN as f64) as i32,
                        range.1.min(i32::MAX as f64) as i32,
                    ));
                }
            }
            "limit" => {
                let limit = value
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| ArgError::new("argument.entity.options.limit.invalid"))?;
                self.limit = Some(limit);
                self.single = limit == 1;
            }
            "sort" => {
                self.sort = match value {
                    "nearest" => Sort::Nearest,
                    "furthest" => Sort::Furthest,
                    "random" => Sort::Random,
                    "arbitrary" => Sort::Arbitrary,
                    _ => return Err(ArgError::new("argument.entity.options.sort.invalid")),
                }
            }
            "x" | "y" | "z" => {
                let v: f64 = value
                    .parse()
                    .map_err(|_| ArgError::new("argument.pos.invalid"))?;
                let origin = self.origin.get_or_insert([0.0; 3]);
                origin[match key {
                    "x" => 0,
                    "y" => 1,
                    _ => 2,
                }] = v;
            }
            "dx" | "dy" | "dz" => {
                let v: f64 = value
                    .parse()
                    .map_err(|_| ArgError::new("argument.pos.invalid"))?;
                let volume = self.volume.get_or_insert([0.0; 3]);
                volume[match key {
                    "dx" => 0,
                    "dy" => 1,
                    _ => 2,
                }] = v;
            }
            "rot" => {
                let mut it = value.split_whitespace();
                let yaw = it
                    .next()
                    .and_then(|v| v.parse::<f32>().ok())
                    .ok_or_else(|| ArgError::new("argument.rotation.invalid"))?;
                let pitch = it
                    .next()
                    .and_then(|v| v.parse::<f32>().ok())
                    .ok_or_else(|| ArgError::new("argument.rotation.invalid"))?;
                self.yaw = Some(yaw);
                self.pitch = Some(pitch);
            }
            "gamemode" => {
                let mode = super::args::gamemode(value)
                    .ok_or_else(|| ArgError::new("argument.gamemode.invalid"))?;
                self.gamemode = Some(mode);
                if self.kind == Kind::AllEntities {
                    self.entities = false;
                }
            }
            "tag" => self.tags.push((value.to_owned(), !negated)),
            "scores" => {
                // `scores={objective=1..5,other=..3}` — вложенные скобки.
                let inner = value.trim_start_matches('{').trim_end_matches('}').trim();
                for pair in inner.split(',').filter(|p| !p.trim().is_empty()) {
                    let (objective, range) = pair
                        .split_once('=')
                        .ok_or_else(|| ArgError::new("argument.scoreboard.invalid"))?;
                    let range = if range.trim().is_empty() {
                        None
                    } else {
                        Some(range_of(range)?)
                    };
                    self.scores.push((
                        objective.trim().to_owned(),
                        range.map(|r| (r.0 as i32, r.1 as i32)),
                    ));
                }
            }
            other => {
                return Err(ArgError::with(
                    "argument.entity.options.unknown",
                    vec![serde_json::Value::String(other.to_owned())],
                ));
            }
        }
        Ok(())
    }

    /// Отбор и сортировка по снимку целей.
    ///
    /// `source` — кто выполнил команду (для `@s` и относительного начала
    /// координат); `filter` возвращает метки и счёт целей, которых нет в снимке.
    pub fn apply(
        &self,
        source: Option<&Target>,
        targets: &[Target],
        tags_of: impl Fn(&Target) -> Vec<String>,
    ) -> Result<Vec<Target>, ArgError> {
        let origin = self
            .origin
            .or_else(|| source.map(|s| s.pos))
            .unwrap_or([0.0; 3]);
        let player_only = matches!(
            self.kind,
            Kind::Self_ | Kind::Nearest | Kind::Random | Kind::AllPlayers
        );
        // `type=` и `name=` разбираются в фильтре один раз, а не на каждую цель:
        // иначе на каждый `@e` приходилось `format!` и `to_lowercase`.
        let kind = self.kind_filter.as_deref().map(TypeFilter::new);
        let name_filter = self.name_filter.as_deref().map(NameFilter::new);
        // Метки есть только у целей, которых нет в снимке: у остальных они уже там.
        let needs_tags = !self.tags.is_empty();

        if self.kind == Kind::Self_ {
            let source = source.ok_or_else(|| ArgError::new("argument.entity.notfound.entity"))?;
            if !self.matches(
                source,
                origin,
                player_only,
                kind.as_ref(),
                name_filter.as_ref(),
                &tags_of,
            ) {
                return Err(ArgError::new("argument.entity.notfound.entity"));
            }
            return Ok(vec![source.clone()]);
        }

        // Снимок перебирается без клонирования: клонируются только совпавшие.
        let candidates: Box<dyn Iterator<Item = &Target> + '_> = match self.kind {
            Kind::Named => Box::new(
                targets
                    .iter()
                    .filter(|t| t.name.eq_ignore_ascii_case(&self.name)),
            ),
            _ => Box::new(targets.iter()),
        };
        let mut out: Vec<Target> = candidates
            .filter(|t| {
                self.matches(
                    t,
                    origin,
                    player_only,
                    kind.as_ref(),
                    name_filter.as_ref(),
                    &tags_of,
                )
            })
            .cloned()
            .collect();
        let _ = needs_tags;

        // Сортировка и лимит.
        let one = match self.limit {
            Some(limit) => limit == 1,
            None => matches!(self.kind, Kind::Nearest | Kind::Random | Kind::Named),
        };
        // Для одной цели сортировать весь список незачем: выбираем минимум.
        if one {
            let pick = match self.sort {
                Sort::Nearest => out
                    .iter()
                    .min_by(|a, b| {
                        squared(a.pos, origin)
                            .total_cmp(&squared(b.pos, origin))
                            .then(a.entity_id.cmp(&b.entity_id))
                    })
                    .cloned(),
                Sort::Furthest => out
                    .iter()
                    .max_by(|a, b| {
                        squared(a.pos, origin)
                            .total_cmp(&squared(b.pos, origin))
                            .then(b.entity_id.cmp(&a.entity_id))
                    })
                    .cloned(),
                Sort::Random => {
                    use rand::Rng;
                    let pick = if out.is_empty() {
                        None
                    } else {
                        Some(rand::thread_rng().gen_range(0..out.len()))
                    };
                    pick.map(|i| out.swap_remove(i))
                }
                Sort::Arbitrary => out.first().cloned(),
            };
            return Ok(pick.into_iter().collect());
        }
        match self.sort {
            Sort::Nearest => out.sort_by(|a, b| {
                squared(a.pos, origin)
                    .total_cmp(&squared(b.pos, origin))
                    .then(a.entity_id.cmp(&b.entity_id))
            }),
            Sort::Furthest => out.sort_by(|a, b| {
                squared(b.pos, origin)
                    .total_cmp(&squared(a.pos, origin))
                    .then(a.entity_id.cmp(&b.entity_id))
            }),
            Sort::Random => {
                use rand::seq::SliceRandom;
                out.shuffle(&mut rand::thread_rng());
            }
            Sort::Arbitrary => {}
        }
        if let Some(limit) = self.limit {
            out.truncate(limit);
        }
        Ok(out)
    }

    /// Подходит ли цель под условия селектора.    #[allow(clippy::too_many_arguments)]
    fn matches(
        &self,
        target: &Target,
        origin: [f64; 3],
        player_only: bool,
        kind: Option<&TypeFilter>,
        name: Option<&NameFilter>,
        tags_of: &impl Fn(&Target) -> Vec<String>,
    ) -> bool {
        if player_only && target.game_mode.is_none() {
            return false;
        }
        if let Some(kind) = kind
            && !kind.matches(target)
        {
            return false;
        }
        if let Some((min, max)) = self.distance {
            // Сравнение по квадрату: корень нужен только для ответа, не для отбора.
            let d = squared(target.pos, origin);
            let lo = if min <= 0.0 { 0.0 } else { min * min };
            let hi = if max.is_finite() {
                max * max
            } else {
                f64::INFINITY
            };
            if d < lo || d > hi {
                return false;
            }
        }
        if let Some(volume) = self.volume {
            for index in 0..3 {
                let delta = target.pos[index] - origin[index];
                if delta < 0.0 || delta > volume[index] {
                    return false;
                }
            }
        }
        if let Some(mode) = self.gamemode
            && target.game_mode != Some(mode)
        {
            return false;
        }
        if let Some(name) = name
            && !name.matches(&target.name)
        {
            return false;
        }
        if let Some((yaw, pitch)) = self.yaw.zip(self.pitch) {
            // Поле зрения: разница углов в пределах половины обзора.
            let dyaw = wrap_degrees(target.yaw - yaw).abs();
            let dpitch = (target.pitch - pitch).abs();
            if dyaw > 45.0 || dpitch > 45.0 {
                return false;
            }
        }
        if !self.tags.is_empty() {
            let held = tags_of(target);
            for (tag, want) in &self.tags {
                if held.iter().any(|h| h == tag) != *want {
                    return false;
                }
            }
        }
        true
    }

    /// Ожидается ровно одна цель: `@p`, `@r`, `@s`, имя, `limit=1`.
    pub fn expects_single(&self) -> bool {
        self.single
    }

    /// Кому хватит одной цели: `@p`/`@r`/имя или явный `limit=1`.    ///
    /// Обработчикам вроде `/waypoint` нужен один игрок, но снимок мира им
    /// не нужен: вместо перебора всех целей достаточно взять первого.
    pub fn wants_one(&self) -> bool {
        self.single || self.limit == Some(1)
    }

    /// Есть ли отбор, кроме «все игроки»: `/tp @a` из консоли.
    pub fn is_broad(&self) -> bool {
        matches!(self.kind, Kind::AllPlayers | Kind::AllEntities)
    }

    /// Вид селектора (`@s` и `@p` нужны вызывающему без полного снимка).
    pub fn kind(&self) -> Kind {
        self.kind
    }
}

/// Диапазон `5`, `1..5`, `..5`, `5..`.
fn range_of(token: &str) -> Result<(f64, f64), ArgError> {
    let invalid = || ArgError::new("argument.range.invalid");
    let parse = |s: &str| s.trim().parse::<f64>().map_err(|_| invalid());
    match token.split_once("..") {
        Some((min, max)) => {
            let min = if min.trim().is_empty() {
                f64::NEG_INFINITY
            } else {
                parse(min)?
            };
            let max = if max.trim().is_empty() {
                f64::INFINITY
            } else {
                parse(max)?
            };
            Ok((min, max))
        }
        None => {
            let v = parse(token)?;
            Ok((v, v))
        }
    }
}

/// Квадрат расстояния: для отбора и сортировки корень не нужен.
fn squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    dx * dx + dy * dy + dz * dz
}

/// `type=` разобранный один раз: полное имя и его короткая форма.
struct TypeFilter {
    /// `type=!…`: все, кроме этого типа.
    negated: bool,
    want: String,
    /// `cow` совпадает и с целью `minecraft:cow`, и с целью `cow`.
    short: String,
}

impl TypeFilter {
    fn new(raw: &str) -> Self {
        let negated = raw.starts_with('!');
        let want = super::args::resource_name(raw.trim_start_matches('!'));
        let short = want
            .strip_prefix("minecraft:")
            .map_or_else(|| want.clone(), str::to_owned);
        Self {
            negated,
            want,
            short,
        }
    }

    /// Игрок — это `type=player`; у моба имя цели и есть его тип.
    fn matches(&self, target: &Target) -> bool {
        let hit = if target.game_mode.is_some() {
            self.short == "player"
        } else {
            // Имена мобов в снимке уже с пространством: `minecraft:cow`.
            target.name.eq_ignore_ascii_case(&self.want)
                || target.name.eq_ignore_ascii_case(&self.short)
        };
        hit != self.negated
    }
}

/// `name=` разобранный один раз: точное сравнение или шаблон с `*`.
enum NameFilter {
    Exact(String),
    Negated(String),
    Pattern(String),
}

impl NameFilter {
    fn new(raw: &str) -> Self {
        if let Some(negated) = raw.strip_prefix('!') {
            return Self::Negated(negated.to_owned());
        }
        if raw.contains('*') {
            return Self::Pattern(raw.to_ascii_lowercase());
        }
        Self::Exact(raw.to_owned())
    }

    fn matches(&self, name: &str) -> bool {
        match self {
            Self::Exact(want) => name.eq_ignore_ascii_case(want),
            Self::Negated(want) => !name.eq_ignore_ascii_case(want),
            Self::Pattern(pattern) => wildcard_lower(pattern, name),
        }
    }
}

/// Угол в пределах `(-180, 180]`.
fn wrap_degrees(deg: f32) -> f32 {
    let mut deg = deg % 360.0;
    if deg > 180.0 {
        deg -= 360.0;
    } else if deg <= -180.0 {
        deg += 360.0;
    }
    deg
}

/// Шаблон имени с `*` (звёздочек может быть несколько).
pub fn wildcard(pattern: &str, name: &str) -> bool {
    if pattern.contains('*') {
        // Шаблон приводится к нижнему регистру один раз, имя — по буквам ниже.
        return wildcard_lower(&pattern.to_ascii_lowercase(), name);
    }
    pattern.eq_ignore_ascii_case(name)
}

/// То же, но шаблон уже в нижнем регистре: имя не аллоцируется.
///
/// Семантика ванильная: первая часть шаблона якорится к началу имени,
/// средние ищутся где угодно, хвост после последней звёздочки — в конце.
fn wildcard_lower(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return pattern.eq_ignore_ascii_case(name);
    }
    let mut parts = pattern.split('*');
    let tail = parts.next_back().unwrap_or("");
    let mut rest = name;
    for (index, part) in parts.enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 {
            if !starts_with_ignore_case(rest, part) {
                return false;
            }
            rest = &rest[part.len()..];
            continue;
        }
        let Some(at) = find_ignore_case(rest, part) else {
            return false;
        };
        rest = &rest[at + part.len()..];
    }
    tail.is_empty() || ends_with_ignore_case(name, tail)
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.as_bytes()
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
}

fn ends_with_ignore_case(text: &str, suffix: &str) -> bool {
    text.len()
        .checked_sub(suffix.len())
        .and_then(|at| text.as_bytes().get(at..))
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix.as_bytes()))
}

fn find_ignore_case(text: &str, part: &str) -> Option<usize> {
    if part.is_empty() {
        return Some(0);
    }
    let haystack = text.as_bytes();
    let needle = part.as_bytes();
    if needle.len() > haystack.len() {
        return None;
    }
    let first = needle[0].to_ascii_lowercase();
    for start in 0..=haystack.len() - needle.len() {
        if haystack[start].to_ascii_lowercase() != first {
            continue;
        }
        if haystack[start..start + needle.len()].eq_ignore_ascii_case(needle) {
            return Some(start);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str, pos: [f64; 3], game_mode: Option<u8>) -> Target {
        Target {
            uuid: uuid::Uuid::nil(),
            name: name.into(),
            entity_id: 1,
            pos,
            yaw: 0.0,
            pitch: 0.0,
            game_mode,
            tags: Vec::new(),
        }
    }

    fn players() -> Vec<Target> {
        vec![
            target("Alice", [0.0, 0.0, 0.0], Some(0)),
            target("Bob", [10.0, 0.0, 0.0], Some(1)),
            target("Carol", [100.0, 0.0, 0.0], Some(0)),
        ]
    }

    #[test]
    fn default_kinds() {
        let all = players();
        let s = Selector::parse("@a").unwrap();
        assert_eq!(s.apply(None, &all, |_| Vec::new()).unwrap().len(), 3);
        let p = Selector::parse("@p").unwrap();
        let one = p.apply(None, &all, |_| Vec::new()).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].name, "Alice");
        let r = Selector::parse("@r").unwrap();
        assert_eq!(r.apply(None, &all, |_| Vec::new()).unwrap().len(), 1);
        let named = Selector::parse("bob").unwrap();
        let one = named.apply(None, &all, |_| Vec::new()).unwrap();
        assert_eq!(one[0].name, "Bob");
    }

    #[test]
    fn at_self_requires_source() {
        let s = Selector::parse("@s").unwrap();
        assert!(s.apply(None, &players(), |_| Vec::new()).is_err());
        let me = target("Alice", [0.0; 3], Some(0));
        assert_eq!(
            s.apply(Some(&me), &players(), |_| Vec::new())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn distance_and_limit() {
        let all = players();
        let s = Selector::parse("@a[distance=..20]").unwrap();
        assert_eq!(s.apply(None, &all, |_| Vec::new()).unwrap().len(), 2);
        let s = Selector::parse("@a[limit=2]").unwrap();
        assert_eq!(s.apply(None, &all, |_| Vec::new()).unwrap().len(), 2);
        let s = Selector::parse("@a[gamemode=creative]").unwrap();
        let out = s.apply(None, &all, |_| Vec::new()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "Bob");
    }

    #[test]
    fn sort_nearest_from_origin() {
        let all = players();
        let s = Selector::parse("@a[sort=furthest,limit=1]").unwrap();
        let out = s.apply(None, &all, |_| Vec::new()).unwrap();
        assert_eq!(out[0].name, "Carol");
    }

    #[test]
    fn tags_and_names() {
        let s = Selector::parse("@e[tag=zombie]").unwrap();
        let mut mob = target("minecraft:zombie", [0.0; 3], None);
        mob.tags = vec!["zombie".into()];
        let out = s
            .apply(None, std::slice::from_ref(&mob), |t| t.tags.clone())
            .unwrap();
        assert_eq!(out.len(), 1);
        let s = Selector::parse("@a[name=A*]").unwrap();
        assert_eq!(
            s.apply(None, &players(), |_| Vec::new()).unwrap()[0].name,
            "Alice"
        );
        let s = Selector::parse("@a[name=!Alice]").unwrap();
        assert_eq!(s.apply(None, &players(), |_| Vec::new()).unwrap().len(), 2);
    }

    #[test]
    fn bad_selectors() {
        assert!(Selector::parse("@q").is_err());
        assert!(Selector::parse("@a[limit=0]").is_err());
        assert!(Selector::parse("@a[sort=sideways]").is_err());
        assert!(Selector::parse("@a[distance=x]").is_err());
        assert!(Selector::parse("@a[unknown=1]").is_err());
    }

    #[test]
    fn wildcard_matching() {
        assert!(wildcard("a*", "Alice"));
        assert!(wildcard("*e", "Alice"));
        assert!(wildcard("*li*", "Alice"));
        assert!(!wildcard("b*", "Alice"));
        // Первая часть якорится к началу, средняя ищется где угодно.
        assert!(wildcard("Al*ce", "Alice"));
        assert!(!wildcard("li*ce", "Alice"));
        assert!(wildcard("A*L*E", "Alice"));
        assert!(wildcard("*", "Alice"));
        assert!(wildcard("alice", "ALICE"));
        assert!(!wildcard("*z", "Alice"));
    }

    #[test]
    fn distance_boundaries_are_inclusive() {
        let all = players();
        // Ровно 10 блоков от начала координат: граница входит в диапазон.
        let s = Selector::parse("@a[distance=10]").unwrap();
        let out = s.apply(None, &all, |_| Vec::new()).unwrap();
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].name, "Bob");
        let s = Selector::parse("@a[distance=0..10]").unwrap();
        assert_eq!(s.apply(None, &all, |_| Vec::new()).unwrap().len(), 2);
    }

    #[test]
    fn single_pick_matches_sorting() {
        let all = players();
        for query in ["@a[sort=nearest,limit=1]", "@a[sort=furthest,limit=1]"] {
            let sorted = Selector::parse(query).unwrap();
            let one = sorted.apply(None, &all, |_| Vec::new()).unwrap();
            assert_eq!(one.len(), 1, "{query}");
        }
        // `@e[type=player]` видит игроков, `type=minecraft:cow` над снимком игроков пуст.
        let s = Selector::parse("@e[type=player]").unwrap();
        assert_eq!(s.apply(None, &all, |_| Vec::new()).unwrap().len(), 3);
        let s = Selector::parse("@e[type=minecraft:cow]").unwrap();
        assert!(s.apply(None, &all, |_| Vec::new()).unwrap().is_empty());
        // `type=!…` — все, кроме: игроков нет, коровы нет — все трое.
        let s = Selector::parse("@e[type=!minecraft:player]").unwrap();
        assert!(s.apply(None, &all, |_| Vec::new()).unwrap().is_empty());
        let s = Selector::parse("@e[type=!cow]").unwrap();
        assert_eq!(s.apply(None, &all, |_| Vec::new()).unwrap().len(), 3);
    }
}
