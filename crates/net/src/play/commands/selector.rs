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
                return Err(ArgError::new("argument.entity.selector.unknown"))
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
                let inner = value
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .trim();
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
        let mut out: Vec<Target> = match self.kind {
            Kind::Self_ => {
                let source =
                    source.ok_or_else(|| ArgError::new("argument.entity.notfound.entity"))?;
                vec![source.clone()]
            }
            Kind::Named => targets
                .iter()
                .filter(|t| t.name.eq_ignore_ascii_case(&self.name))
                .cloned()
                .collect(),
            _ => targets.to_vec(),
        };
        out.retain(|t| {
            if player_only && t.game_mode.is_none() {
                return false;
            }
            if let Some(kind) = &self.kind_filter {
                let want = super::args::resource_name(kind);
                // Имена мобов в снимке уже с пространством: `minecraft:cow`.
                if !t.name.eq_ignore_ascii_case(&want) && !format!("minecraft:{}", t.name).eq(&want)
                {
                    // `type=` относится к типу сущности, а не к имени игрока.
                    if t.game_mode.is_some() && want != "minecraft:player" {
                        return false;
                    }
                }
            }
            if let Some((min, max)) = self.distance {
                let d = distance(t.pos, origin);
                if d < min || d > max {
                    return false;
                }
            }
            if let Some(volume) = self.volume {
                for index in 0..3 {
                    let delta = t.pos[index] - origin[index];
                    if delta < 0.0 || delta > volume[index] {
                        return false;
                    }
                }
            }
            if let Some(mode) = self.gamemode
                && t.game_mode != Some(mode)
            {
                return false;
            }
            if let Some(name) = &self.name_filter {
                match name.strip_prefix('!') {
                    Some(negated) => {
                        if t.name.eq_ignore_ascii_case(negated) {
                            return false;
                        }
                    }
                    None => {
                        // Ванила сравнивает имя как шаблон с `*`.
                        if !wildcard(name, &t.name) {
                            return false;
                        }
                    }
                }
            }
            if let Some((yaw, pitch)) = self.yaw.zip(self.pitch) {
                // Поле зрения: разница углов в пределах половины обзора.
                let dyaw = wrap_degrees(t.yaw - yaw).abs();
                let dpitch = (t.pitch - pitch).abs();
                if dyaw > 45.0 || dpitch > 45.0 {
                    return false;
                }
            }
            if !self.tags.is_empty() {
                let held = tags_of(t);
                for (tag, want) in &self.tags {
                    if held.iter().any(|h| h == tag) != *want {
                        return false;
                    }
                }
            }
            true
        });
        if self.kind == Kind::Self_ && out.is_empty() {
            return Err(ArgError::new("argument.entity.notfound.entity"));
        }
        // Сортировка и лимит.
        match self.sort {
            Sort::Nearest => out.sort_by(|a, b| {
                distance(a.pos, origin)
                    .total_cmp(&distance(b.pos, origin))
                    .then(a.entity_id.cmp(&b.entity_id))
            }),
            Sort::Furthest => out.sort_by(|a, b| {
                distance(b.pos, origin)
                    .total_cmp(&distance(a.pos, origin))
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
        } else if matches!(self.kind, Kind::Nearest | Kind::Random | Kind::Named) {
            out.truncate(1);
        }
        Ok(out)
    }

    /// Ожидается ровно одна цель: `@p`, `@r`, `@s`, имя, `limit=1`.
    pub fn expects_single(&self) -> bool {
        self.single
    }

    /// Есть ли отбор, кроме «все игроки»: `/tp @a` из консоли.
    pub fn is_broad(&self) -> bool {
        matches!(self.kind, Kind::AllPlayers | Kind::AllEntities)
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

/// Расстояние между точками.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
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
    let (pattern, name) = (pattern.to_lowercase(), name.to_lowercase());
    if !pattern.contains('*') {
        return pattern == name;
    }
    // Жадный поиск по частям между звёздочками.
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = name.as_str();
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 {
            if !rest.starts_with(part) {
                return false;
            }
            rest = &rest[part.len()..];
            continue;
        }
        let Some(found) = rest.find(part) else {
            return false;
        };
        rest = &rest[found + part.len()..];
    }
    // Хвост после последней звёздочки должен совпасть с концом имени.
    match parts.last() {
        Some(tail) if !tail.is_empty() => name.ends_with(tail),
        _ => true,
    }
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
            s.apply(Some(&me), &players(), |_| Vec::new()).unwrap().len(),
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
    }
}
