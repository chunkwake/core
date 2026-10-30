//! Разбор аргументов команд: координаты, селекторы-заготовки, числа, текст.
//!
//! Ванильный Brigadier разбирает аргументы по типам. Здесь разбор разделён:
//! дерево (для клиента и подсказок) объявляет тип аргумента, а обработчик
//! получает уже разрезанные токены и проверяет их этими функциями. Так одна
//! и та же команда работает и из чата, и из консоли, а неоднозначности
//! (`/tp a b c` — цель или координаты) решает обработчик, как в ваниле.

use chunkwake_protocol::text::Parser;

/// Ошибка разбора аргумента: ванильный ключ перевода и аргументы.
#[derive(Debug, Clone)]
pub struct ArgError {
    pub key: &'static str,
    pub with: Vec<serde_json::Value>,
}

impl ArgError {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            with: Vec::new(),
        }
    }

    pub fn with(key: &'static str, args: Vec<serde_json::Value>) -> Self {
        Self { key, with: args }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self {
            key: "",
            with: vec![serde_json::Value::String(text.into())],
        }
    }

    /// Компонент ошибки: ванильный `translate` или готовый текст.
    pub fn component(&self) -> serde_json::Value {
        if self.key.is_empty() {
            let text = self
                .with
                .first()
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            serde_json::json!({"text": text, "color": "red"})
        } else if self.with.is_empty() {
            serde_json::json!({"translate": self.key, "color": "red"})
        } else {
            serde_json::json!({"translate": self.key, "with": self.with, "color": "red"})
        }
    }
}

/// Токены остатка строки с учётом кавычек.
pub struct Input<'a> {
    rest: &'a str,
}

impl<'a> Input<'a> {
    pub fn new(rest: &'a str) -> Self {
        Self {
            rest: rest.trim_start(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    /// Остаток как есть (для greedy-аргументов вроде сообщения).
    pub fn rest(&self) -> &'a str {
        self.rest
    }

    /// Следующее слово без его изъятия: нужно, чтобы отличить число от слова.
    pub fn peek_word(&self) -> Option<&'a str> {
        let rest = self.rest.trim_start();
        if rest.is_empty() {
            return None;
        }
        if let Some(body) = rest.strip_prefix('"') {
            return closing_quote(body).map(|end| &body[..end]).or(Some(body));
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        Some(&rest[..end])
    }

    /// Следующее слово: до пробела или целиком строка в кавычках.
    pub fn word(&mut self) -> Option<&'a str> {
        self.rest = self.rest.trim_start();
        if self.rest.is_empty() {
            return None;
        }
        if let Some(body) = self.rest.strip_prefix('"') {
            // Строка в кавычках: ищем закрывающую, учитывая `\"`.
            if let Some(end) = closing_quote(body) {
                let word = &body[..end];
                self.rest = body[end + 1..].trim_start();
                return Some(word);
            }
            // Незакрытая кавычка: всё до конца — слово.
            self.rest = "";
            return Some(body);
        }
        let end = self
            .rest
            .find(char::is_whitespace)
            .unwrap_or(self.rest.len());
        let word = &self.rest[..end];
        self.rest = self.rest[end..].trim_start();
        Some(word)
    }

    /// Следующий токен до пробела вне кавычек, `[]` и `{}`.
    ///
    /// `item[minecraft:custom_name="a b"]` и `{Foo:1, Bar:"x y"}` — одно слово.
    pub fn balanced(&mut self) -> Option<&'a str> {
        self.rest = self.rest.trim_start();
        if self.rest.is_empty() {
            return None;
        }
        let bytes = self.rest.as_bytes();
        let mut i = 0;
        let mut curly = 0i32;
        let mut square = 0i32;
        let mut quote = 0u8;
        while i < bytes.len() {
            let c = bytes[i];
            if quote != 0 {
                if c == b'\\' {
                    i += 2;
                    continue;
                }
                if c == quote {
                    quote = 0;
                }
                i += 1;
                continue;
            }
            match c {
                b'"' | b'\'' => {
                    quote = c;
                    i += 1;
                }
                b'{' => {
                    curly += 1;
                    i += 1;
                }
                b'}' => {
                    curly -= 1;
                    i += 1;
                }
                b'[' => {
                    square += 1;
                    i += 1;
                }
                b']' => {
                    square -= 1;
                    i += 1;
                }
                c if c.is_ascii_whitespace() && curly <= 0 && square <= 0 => break,
                _ => i += 1,
            }
        }
        let word = &self.rest[..i];
        self.rest = self.rest[i..].trim_start();
        (!word.is_empty()).then_some(word)
    }

    /// Весь остаток одной строкой: внешние кавычки снимаются.
    ///
    /// Строка в кавычках может стоять в начале: тогда берётся только она
    /// (`"a b" c` → `a b`), как `greedyString` ванилы на первом слове.
    pub fn greedy(&mut self) -> &'a str {
        let rest = self.rest.trim();
        self.rest = "";
        if let Some(body) = rest.strip_prefix('"')
            && let Some(end) = closing_quote(body)
        {
            return &body[..end];
        }
        rest
    }

    /// Все оставшиеся токены.
    pub fn tokens(&mut self) -> Vec<&'a str> {
        let mut out = Vec::new();
        while let Some(word) = self.word() {
            out.push(word);
        }
        out
    }
}

/// Позиция закрывающей кавычки в `body` (без начальной), учитывая `\"`.
fn closing_quote(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Координата как её записал игрок.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Coord {
    /// Абсолютная.
    Abs(f64),
    /// `~1.5` — относительно базы.
    Rel(f64),
    /// `^1` — по локальным осям (влево, вверх, вперёд).
    Local(f64),
}

impl Coord {
    pub fn parse(token: &str) -> Option<Self> {
        if let Some(rest) = token.strip_prefix('~') {
            let value = if rest.is_empty() {
                0.0
            } else {
                rest.parse::<f64>().ok()?
            };
            return value.is_finite().then_some(Coord::Rel(value));
        }
        if let Some(rest) = token.strip_prefix('^') {
            let value = if rest.is_empty() {
                0.0
            } else {
                rest.parse::<f64>().ok()?
            };
            return value.is_finite().then_some(Coord::Local(value));
        }
        let value = token.parse::<f64>().ok()?;
        value.is_finite().then_some(Coord::Abs(value))
    }

    /// Есть ли в наборе локальные координаты: тогда нужны оси игрока.
    pub fn any_local(c: [Coord; 3]) -> bool {
        c.iter().any(|v| matches!(v, Coord::Local(_)))
    }
}

/// Оси локальных координат: влево, вверх, вперёд (как `Vec3` ванилы).
#[derive(Clone, Copy, Debug)]
pub struct Basis {
    left: [f64; 3],
    up: [f64; 3],
    forward: [f64; 3],
}

impl Basis {
    pub fn from_rotation(yaw: f32, pitch: f32) -> Self {
        let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        // В Minecraft yaw 0 — взгляд на юг (+Z), yaw 90 — на запад (−X).
        let forward = [-sy * cp, -sp, cy * cp];
        let left = [cy, 0.0, sy];
        let up = [
            forward[1] * left[2] - forward[2] * left[1],
            forward[2] * left[0] - forward[0] * left[2],
            forward[0] * left[1] - forward[1] * left[0],
        ];
        Self { left, up, forward }
    }

    /// Мировые координаты из записанных.
    ///
    /// Локальные оси (`^`) считаются от базы и поворота, а не от результата
    /// соседних компонент: ванила складывает их независимо.
    pub fn resolve(&self, c: [Coord; 3], base: [f64; 3]) -> [f64; 3] {
        let mut out = base;
        for (index, value) in c.iter().enumerate() {
            match *value {
                Coord::Abs(v) => out[index] = v,
                Coord::Rel(v) => out[index] += v,
                Coord::Local(_) => {}
            }
        }
        let axes = [self.left, self.up, self.forward];
        for (axis, value) in c.iter().enumerate() {
            let Coord::Local(v) = *value else { continue };
            for (index, component) in axes[axis].iter().enumerate() {
                out[index] += component * v;
            }
        }
        out
    }
}

/// `~`/`^`/абсолютные три координаты.
pub fn coords3(a: &str, b: &str, c: &str) -> Option<[Coord; 3]> {
    Some([Coord::parse(a)?, Coord::parse(b)?, Coord::parse(c)?])
}

/// Похоже ли слово на координату: так `/tp` отличает цель от позиции.
pub fn is_coord(token: &str) -> bool {
    matches!(
        token.as_bytes().first(),
        Some(b'~' | b'^' | b'+' | b'-' | b'0'..=b'9' | b'.')
    )
}

/// Блок-позиция: целые, `~` и `^`, с округлением вниз как у ванилы.
pub fn block_pos(c: [Coord; 3], base: [f64; 3], basis: Basis) -> [i32; 3] {
    let world = basis.resolve(c, base);
    [
        world[0].floor() as i32,
        world[1].floor() as i32,
        world[2].floor() as i32,
    ]
}

/// Углы поворота: число или `~`.
pub fn rotation(yaw: &str, pitch: &str, base: (f32, f32)) -> Option<(f32, f32)> {
    let one = |token: &str, base: f32| -> Option<f32> {
        if let Some(rest) = token.strip_prefix('~') {
            let value = if rest.is_empty() {
                0.0
            } else {
                rest.parse::<f32>().ok()?
            };
            Some(base + value)
        } else {
            let value = token.parse::<f32>().ok()?;
            Some(value)
        }
    };
    let yaw = one(yaw, base.0)?;
    let pitch = one(pitch, base.1)?;
    (yaw.is_finite() && pitch.is_finite()).then_some((yaw, pitch))
}

/// Целое в диапазоне.
pub fn int(token: &str, min: i32, max: i32) -> Option<i32> {
    token
        .parse::<i32>()
        .ok()
        .filter(|v| (min..=max).contains(v))
}

/// Время: число, число с `d`/`s`/`t` (дни, секунды, тики).
pub fn time_ticks(token: &str) -> Option<i64> {
    let (number, unit) = match token.strip_suffix(['d', 's', 't']) {
        Some(number) => (
            number,
            match token.as_bytes()[token.len() - 1] {
                b'd' => 24_000,
                b's' => 20,
                _ => 1,
            },
        ),
        None => (token, 1),
    };
    let value = if let Ok(v) = number.parse::<f32>() {
        v
    } else {
        number.parse::<i64>().ok()? as f32
    };
    let ticks = (value as f64) * f64::from(unit);
    ticks.is_finite().then_some(ticks as i64)
}

/// Время суток и именованные значения (`day`, `noon`, …).
pub fn daytime(token: &str) -> Option<i64> {
    let named = match token {
        "day" => Some(1000),
        "noon" => Some(6000),
        "night" => Some(13000),
        "midnight" => Some(18000),
        "sunrise" => Some(23000),
        "sunset" => Some(12000),
        _ => None,
    };
    named
        .or_else(|| time_ticks(token))
        .map(|t| t.rem_euclid(24_000))
}

/// Режим игры: имя или сокращение.
pub fn gamemode(token: &str) -> Option<u8> {
    Some(match token {
        "survival" | "s" | "0" => 0,
        "creative" | "c" | "1" => 1,
        "adventure" | "a" | "2" => 2,
        "spectator" | "sp" | "3" => 3,
        _ => return None,
    })
}

/// Имя записи ресурса: без пространства имён — `minecraft:`.
pub fn resource_name(token: &str) -> String {
    if token.contains(':') {
        token.to_owned()
    } else {
        format!("minecraft:{token}")
    }
}

/// Число с плавающей точкой в диапазоне.
pub fn float(token: &str, min: f64, max: f64) -> Option<f64> {
    token
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= min && *v <= max)
}

/// Сообщение: остаток строки, кавычки снимаются, `\"` разворачивается.
pub fn message(rest: &str) -> String {
    let rest = rest.trim();
    let body = rest
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(rest);
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Принимает ли тип аргумента такой токен: нужно для выбора ветки дерева.
pub fn accepts(parser: &Parser, token: &str) -> bool {
    match parser {
        Parser::Integer(..) => token.parse::<i32>().is_ok(),
        Parser::Long(..) => token.parse::<i64>().is_ok(),
        Parser::Double(..) | Parser::Float(..) => token.parse::<f64>().is_ok(),
        Parser::Bool => matches!(token, "true" | "false"),
        Parser::BlockPos | Parser::ColumnPos | Parser::Vec3 | Parser::Vec2 => {
            token.split_whitespace().all(|t| Coord::parse(t).is_some())
        }
        Parser::Angle | Parser::Time(_) => time_ticks(token).is_some() || is_coord(token),
        Parser::Gamemode => gamemode(token).is_some(),
        Parser::Entity { players_only, .. } => {
            token.starts_with('@')
                || (!token.starts_with('@') && !*players_only || !is_coord(token))
        }
        Parser::ScoreHolder { .. } => token == "*" || !is_coord(token),
        Parser::String(_) | Parser::GreedyStringAsk | Parser::Message | Parser::Component => true,
        Parser::ItemStack | Parser::ItemPredicate | Parser::ResourceLocation => !token.is_empty(),
        Parser::Rotation => {
            let mut it = token.split_whitespace();
            let a = it.next();
            let b = it.next();
            a.map(|a| Coord::parse(a).is_some() || a.parse::<f32>().is_ok())
                .unwrap_or(false)
                && b.is_none_or(|b| Coord::parse(b).is_some() || b.parse::<f32>().is_ok())
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_quotes() {
        let mut input = Input::new("  alpha \"beta gamma\" delta ");
        assert_eq!(input.word(), Some("alpha"));
        assert_eq!(input.word(), Some("beta gamma"));
        assert_eq!(input.word(), Some("delta"));
        assert_eq!(input.word(), None);
        assert!(input.is_empty());
    }

    #[test]
    fn greedy_takes_rest() {
        let mut input = Input::new("\"a b\" c");
        assert_eq!(input.greedy(), "a b");
        assert!(input.is_empty());
        let mut input = Input::new("raw  text  ");
        assert_eq!(input.greedy(), "raw  text");
    }

    #[test]
    fn coordinates_resolve_like_vanilla() {
        let basis = Basis::from_rotation(0.0, 0.0);
        let pos = coords3("~5", "~", "10").unwrap();
        assert_eq!(basis.resolve(pos, [100.0, 64.0, 0.0]), [105.0, 64.0, 10.0]);
        let pos = coords3("^1", "0", "0").unwrap();
        // Взгляд на юг (yaw 0): «влево» — это +X.
        let out = basis.resolve(pos, [0.0, 0.0, 0.0]);
        assert!((out[0] - 1.0).abs() < 1e-9 && out[2].abs() < 1e-9);
    }

    #[test]
    fn block_pos_floors() {
        let basis = Basis::from_rotation(0.0, 0.0);
        let c = coords3("1.9", "-0.1", "~2.5").unwrap();
        assert_eq!(block_pos(c, [0.0, 0.0, 0.0], basis), [1, -1, 2]);
    }

    #[test]
    fn time_units() {
        assert_eq!(time_ticks("20"), Some(20));
        assert_eq!(time_ticks("1s"), Some(20));
        assert_eq!(time_ticks("2d"), Some(48_000));
        assert_eq!(daytime("night"), Some(13000));
        assert_eq!(daytime("-6000"), Some(18000));
    }

    #[test]
    fn gamemodes_and_names() {
        assert_eq!(gamemode("sp"), Some(3));
        assert_eq!(gamemode("creative"), Some(1));
        assert_eq!(gamemode("bogus"), None);
        assert_eq!(resource_name("stone"), "minecraft:stone");
        assert_eq!(resource_name("mod:thing"), "mod:thing");
    }

    #[test]
    fn messages_drop_quotes_and_escapes() {
        assert_eq!(message(r#"  hello world  "#), "hello world");
        assert_eq!(message(r#""say \"hi\"""#), r#"say "hi""#);
    }
}
