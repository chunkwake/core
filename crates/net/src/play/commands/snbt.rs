//! SNBT и путь NBT (`Foo.Bar[0]`) для `/data` и `execute store`.

use chunkwake_world::nbt::{Compound, Tag};

/// Шаг пути: ключ compound, индекс списка или все элементы (`[]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(i32),
    All,
}

/// Почему запись в путь не удалась.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    Missing,
    Type,
    Path,
}

/// Режим `/data modify`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modify {
    Set,
    Merge,
    Append,
    Prepend,
    Insert(i32),
}

struct P<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> P<'a> {
    fn new(s: &'a str) -> Self {
        Self { s, i: 0 }
    }

    fn eof(&self) -> bool {
        self.i >= self.s.len()
    }

    fn skip(&mut self) {
        while let Some(c) = self.peek() {
            if !c.is_whitespace() {
                break;
            }
            self.bump();
        }
    }

    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, want: char) -> Option<()> {
        self.skip();
        (self.bump() == Some(want)).then_some(())
    }

    fn eat_if(&mut self, want: char) -> bool {
        self.skip();
        if self.peek() == Some(want) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn quoted(&mut self) -> Option<String> {
        let q = self.bump()?;
        let mut out = String::new();
        while let Some(c) = self.bump() {
            if c == q {
                return Some(out);
            }
            if c == '\\' {
                match self.bump()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    other => out.push(other),
                }
            } else {
                out.push(c);
            }
        }
        None
    }

    fn value(&mut self) -> Option<Tag> {
        self.skip();
        match self.peek()? {
            '{' => self.compound(),
            '[' => self.list(),
            '"' | '\'' => Some(Tag::String(self.quoted()?)),
            _ => self.scalar(),
        }
    }

    fn compound(&mut self) -> Option<Tag> {
        self.eat('{')?;
        let mut c = Compound::new();
        loop {
            self.skip();
            if self.eat_if('}') {
                break;
            }
            let key = self.key()?;
            self.eat(':')?;
            c.insert(key, self.value()?);
            self.skip();
            if self.eat_if(',') {
                continue;
            }
            self.eat('}')?;
            break;
        }
        Some(Tag::Compound(c))
    }

    fn key(&mut self) -> Option<String> {
        self.skip();
        if matches!(self.peek(), Some('"' | '\'')) {
            return self.quoted();
        }
        let start = self.i;
        while let Some(c) = self.peek() {
            if c == ':' || c == '}' || c.is_whitespace() {
                break;
            }
            self.bump();
        }
        let key = &self.s[start..self.i];
        (!key.is_empty()).then(|| key.to_owned())
    }

    fn list(&mut self) -> Option<Tag> {
        self.eat('[')?;
        self.skip();
        if let Some(kind) = self.peek().filter(|c| matches!(c, 'B' | 'I' | 'L')) {
            let save = self.i;
            self.bump();
            self.skip();
            if self.peek() == Some(';') {
                self.bump();
                return self.typed(kind);
            }
            self.i = save;
        }
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.eat_if(']') {
                break;
            }
            items.push(self.value()?);
            self.skip();
            if self.eat_if(',') {
                continue;
            }
            self.eat(']')?;
            break;
        }
        Some(Tag::List(items))
    }

    fn typed(&mut self, kind: char) -> Option<Tag> {
        let mut nums = Vec::new();
        loop {
            self.skip();
            if self.eat_if(']') {
                break;
            }
            let token = self.raw_token()?;
            let n = number(&number_tag(&token)?)? as i64;
            nums.push(n);
            self.skip();
            if self.eat_if(',') {
                continue;
            }
            self.eat(']')?;
            break;
        }
        Some(match kind {
            'B' => Tag::ByteArray(nums.into_iter().map(|n| n as i8).collect()),
            'I' => Tag::IntArray(nums.into_iter().map(|n| n as i32).collect()),
            'L' => Tag::LongArray(nums),
            _ => return None,
        })
    }

    fn raw_token(&mut self) -> Option<String> {
        self.skip();
        let start = self.i;
        while let Some(c) = self.peek() {
            if c == ',' || c == ']' || c == '}' || c.is_whitespace() {
                break;
            }
            self.bump();
        }
        let token = &self.s[start..self.i];
        (!token.is_empty()).then(|| token.to_owned())
    }

    fn scalar(&mut self) -> Option<Tag> {
        if self.word("true") {
            return Some(Tag::Byte(1));
        }
        if self.word("false") {
            return Some(Tag::Byte(0));
        }
        let token = self.raw_token()?;
        Some(number_tag(&token).unwrap_or(Tag::String(token)))
    }

    fn word(&mut self, lit: &str) -> bool {
        self.skip();
        let rest = &self.s[self.i..];
        if let Some(tail) = rest.strip_prefix(lit) {
            let after = tail.chars().next();
            if after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_') {
                self.i += lit.len();
                return true;
            }
        }
        false
    }
}

fn number_tag(token: &str) -> Option<Tag> {
    let (body, suffix) = split_suffix(token);
    if let Ok(n) = body.parse::<i64>() {
        return Some(match suffix {
            b'b' | b'B' => Tag::Byte(n as i8),
            b's' | b'S' => Tag::Short(n as i16),
            b'l' | b'L' => Tag::Long(n),
            b'f' | b'F' => Tag::Float(n as f32),
            b'd' | b'D' => Tag::Double(n as f64),
            _ if (i32::MIN as i64..=i32::MAX as i64).contains(&n) => Tag::Int(n as i32),
            _ => Tag::Long(n),
        });
    }
    if let Ok(n) = body.parse::<f64>() {
        return Some(match suffix {
            b'f' | b'F' => Tag::Float(n as f32),
            _ => Tag::Double(n),
        });
    }
    None
}

fn split_suffix(token: &str) -> (&str, u8) {
    let Some(last) = token.as_bytes().last().copied() else {
        return (token, 0);
    };
    if token.len() > 1
        && matches!(
            last,
            b'b' | b'B' | b's' | b'S' | b'l' | b'L' | b'f' | b'F' | b'd' | b'D'
        )
        && token[..token.len() - 1]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '-' || c == '+' || c == '.')
    {
        (&token[..token.len() - 1], last)
    } else {
        (token, 0)
    }
}

/// SNBT целиком. Хвост после значения — ошибка.
pub fn parse_value(text: &str) -> Option<Tag> {
    // Стек не бесконечен: цепочка `{a:{a:{…}}}` из командной строки иначе
    // переполняет его раньше, чем закончится разбор.
    if nesting_depth(text) > MAX_SNBT_DEPTH {
        return None;
    }
    let mut p = P::new(text);
    let tag = p.value()?;
    p.skip();
    p.eof().then_some(tag)
}

/// Предел вложенности compound/list в SNBT.
const MAX_SNBT_DEPTH: usize = 64;

/// Максимальная вложенность `{`/`[` вне кавычек. Кавычки и `\` учитываются,
/// чтобы `"{a"` в строке не считался открывающей скобкой.
fn nesting_depth(text: &str) -> usize {
    let mut depth = 0usize;
    let mut max = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for c in text.chars() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '{' | '[' => {
                depth += 1;
                max = max.max(depth);
            }
            '}' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}

/// Путь `Foo.Bar[0]."baz"[]`. Пустая строка — корень.
pub fn parse_path(text: &str) -> Option<Vec<Step>> {
    let text = text.trim();
    if text.is_empty() {
        return Some(Vec::new());
    }
    let mut p = P::new(text);
    let mut steps = Vec::new();
    loop {
        p.skip();
        if p.eof() {
            break;
        }
        if p.eat_if('.') {
            continue;
        }
        if p.peek() == Some('[') {
            p.bump();
            p.skip();
            if p.eat_if(']') {
                steps.push(Step::All);
                continue;
            }
            if matches!(p.peek(), Some('"' | '\'')) {
                let key = p.quoted()?;
                p.eat(']')?;
                steps.push(Step::Key(key));
                continue;
            }
            if p.peek() == Some('{') {
                return None;
            }
            let token = p.raw_token()?;
            let index = token.parse::<i32>().ok()?;
            p.eat(']')?;
            steps.push(Step::Index(index));
            continue;
        }
        let key = if matches!(p.peek(), Some('"' | '\'')) {
            p.quoted()?
        } else {
            let start = p.i;
            while let Some(c) = p.peek() {
                if c == '.' || c == '[' || c.is_whitespace() {
                    break;
                }
                p.bump();
            }
            let key = &p.s[start..p.i];
            if key.is_empty() {
                return None;
            }
            key.to_owned()
        };
        steps.push(Step::Key(key));
    }
    (!steps.is_empty()).then_some(steps)
}

/// Число любого числового тега.
pub fn number(tag: &Tag) -> Option<f64> {
    Some(match *tag {
        Tag::Byte(v) => v as f64,
        Tag::Short(v) => v as f64,
        Tag::Int(v) => v as f64,
        Tag::Long(v) => v as f64,
        Tag::Float(v) => v as f64,
        Tag::Double(v) => v,
        _ => return None,
    })
}

/// Число для `execute store`: тип — литерал `byte|short|int|long|float|double`.
pub fn numeric_tag(kind: &str, value: f64) -> Option<Tag> {
    Some(match kind {
        "byte" => Tag::Byte(value as i8),
        "short" => Tag::Short(value as i16),
        "int" => Tag::Int(value as i32),
        "long" => Tag::Long(value as i64),
        "float" => Tag::Float(value as f32),
        "double" => Tag::Double(value),
        _ => return None,
    })
}

/// Значение по пути. `[]` собирает список.
pub fn get_path(root: &Tag, path: &[Step]) -> Option<Tag> {
    if path.is_empty() {
        return Some(root.clone());
    }
    walk_get(root, path)
}

fn walk_get(cur: &Tag, path: &[Step]) -> Option<Tag> {
    let (step, rest) = path.split_first()?;
    match step {
        Step::Key(key) => {
            let next = cur.as_compound()?.get(key)?;
            if rest.is_empty() {
                Some(next.clone())
            } else {
                walk_get(next, rest)
            }
        }
        Step::Index(index) => {
            let list = cur.as_list()?;
            let at = index_of(list.len(), *index, false)?;
            let next = list.get(at)?;
            if rest.is_empty() {
                Some(next.clone())
            } else {
                walk_get(next, rest)
            }
        }
        Step::All => {
            let mut out = Vec::new();
            for item in cur.as_list()? {
                if rest.is_empty() {
                    out.push(item.clone());
                } else if let Some(found) = walk_get(item, rest) {
                    match found {
                        Tag::List(inner) => out.extend(inner),
                        other => out.push(other),
                    }
                }
            }
            Some(Tag::List(out))
        }
    }
}

/// Пишет значение, создавая недостающие compound и ячейку в конце списка.
pub fn set_path(root: &mut Tag, path: &[Step], value: Tag) -> Result<(), EditError> {
    if path.is_empty() {
        *root = value;
        return Ok(());
    }
    write_at(root, path, value)
}

fn write_at(cur: &mut Tag, path: &[Step], value: Tag) -> Result<(), EditError> {
    let (step, rest) = path.split_first().ok_or(EditError::Path)?;
    if rest.is_empty() {
        return assign(cur, step, value);
    }
    if matches!(step, Step::All) {
        let Tag::List(list) = cur else {
            return Err(EditError::Type);
        };
        if list.is_empty() {
            return Err(EditError::Missing);
        }
        for item in list {
            write_at(item, rest, value.clone())?;
        }
        return Ok(());
    }
    let child = ensure(cur, step, &rest[0])?;
    write_at(child, rest, value)
}

fn ensure<'a>(cur: &'a mut Tag, step: &Step, next: &Step) -> Result<&'a mut Tag, EditError> {
    match step {
        Step::Key(key) => {
            if !matches!(cur, Tag::Compound(_)) {
                return Err(EditError::Type);
            }
            let Tag::Compound(map) = cur else {
                unreachable!()
            };
            if !map.contains_key(key) {
                map.insert(key.clone(), shell(next));
            }
            Ok(map.get_mut(key).unwrap())
        }
        Step::Index(index) => {
            if !matches!(cur, Tag::List(_)) {
                return Err(EditError::Type);
            }
            let Tag::List(list) = cur else { unreachable!() };
            let at = index_of(list.len(), *index, true).ok_or(EditError::Missing)?;
            if at == list.len() {
                list.push(shell(next));
            }
            list.get_mut(at).ok_or(EditError::Missing)
        }
        Step::All => Err(EditError::Type),
    }
}

fn shell(next: &Step) -> Tag {
    match next {
        Step::Key(_) => Tag::Compound(Compound::new()),
        Step::Index(_) | Step::All => Tag::List(Vec::new()),
    }
}

fn assign(cur: &mut Tag, step: &Step, value: Tag) -> Result<(), EditError> {
    match step {
        Step::Key(key) => {
            let Tag::Compound(map) = cur else {
                return Err(EditError::Type);
            };
            map.insert(key.clone(), value);
            Ok(())
        }
        Step::Index(index) => {
            let Tag::List(list) = cur else {
                return Err(EditError::Type);
            };
            let at = index_of(list.len(), *index, true).ok_or(EditError::Missing)?;
            if at == list.len() {
                list.push(value);
            } else if let Some(slot) = list.get_mut(at) {
                *slot = value;
            }
            Ok(())
        }
        Step::All => {
            let Tag::List(list) = cur else {
                return Err(EditError::Type);
            };
            if list.is_empty() {
                return Err(EditError::Missing);
            }
            for slot in list {
                *slot = value.clone();
            }
            Ok(())
        }
    }
}

fn index_of(len: usize, index: i32, allow_end: bool) -> Option<usize> {
    let at = if index < 0 { len as i32 + index } else { index };
    if at < 0 {
        return None;
    }
    let at = at as usize;
    if at < len || (allow_end && at == len) {
        Some(at)
    } else {
        None
    }
}

/// Накладывает compound: вложенные compound сливаются, остальное заменяется.
pub fn merge_compounds(dst: &mut Compound, src: &Compound) {
    for (key, value) in src {
        match (dst.get_mut(key), value) {
            (Some(Tag::Compound(into)), Tag::Compound(from)) => merge_compounds(into, from),
            _ => {
                dst.insert(key.clone(), value.clone());
            }
        }
    }
}

/// `set|merge|append|prepend|insert` по пути.
pub fn modify(root: &mut Tag, path: &[Step], mode: Modify, value: Tag) -> Result<(), EditError> {
    match mode {
        Modify::Set => set_path(root, path, value),
        Modify::Merge => {
            let Tag::Compound(mut dst) = get_path(root, path).ok_or(EditError::Missing)? else {
                return Err(EditError::Type);
            };
            let Tag::Compound(src) = value else {
                return Err(EditError::Type);
            };
            merge_compounds(&mut dst, &src);
            set_path(root, path, Tag::Compound(dst))
        }
        Modify::Append | Modify::Prepend | Modify::Insert(_) => {
            let Tag::List(mut list) = get_path(root, path).ok_or(EditError::Missing)? else {
                return Err(EditError::Type);
            };
            match mode {
                Modify::Append => list.push(value),
                Modify::Prepend => list.insert(0, value),
                Modify::Insert(index) => {
                    let at = if index < 0 {
                        list.len() as i32 + index
                    } else {
                        index
                    };
                    if at < 0 || at > list.len() as i32 {
                        return Err(EditError::Missing);
                    }
                    list.insert(at as usize, value);
                }
                Modify::Set | Modify::Merge => unreachable!(),
            }
            set_path(root, path, Tag::List(list))
        }
    }
}

/// SNBT для ответа команды. Целые double печатаются как `4.0d`.
pub fn format_tag(tag: &Tag) -> String {
    match tag {
        Tag::Byte(v) => format!("{v}b"),
        Tag::Short(v) => format!("{v}s"),
        Tag::Int(v) => v.to_string(),
        Tag::Long(v) => format!("{v}L"),
        Tag::Float(v) => format!("{v}f"),
        Tag::Double(v) => {
            if v.fract() == 0.0 && v.is_finite() {
                format!("{v:.1}d")
            } else {
                format!("{v}d")
            }
        }
        Tag::String(v) => format!("\"{}\"", escape(v)),
        Tag::ByteArray(v) => format!(
            "[B;{}]",
            v.iter()
                .map(|n| format!("{n}b"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Tag::IntArray(v) => format!(
            "[I;{}]",
            v.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        Tag::LongArray(v) => format!(
            "[L;{}]",
            v.iter()
                .map(|n| format!("{n}L"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Tag::List(v) => format!(
            "[{}]",
            v.iter().map(format_tag).collect::<Vec<_>>().join(",")
        ),
        Tag::Compound(c) => format!(
            "{{{}}}",
            c.iter()
                .map(|(k, v)| format!("{k}:{}", format_tag(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compound_and_path() {
        let tag = parse_value(r#"{CustomName:"Hi",Count:3b,Pos:[1.0d,2d]}"#).unwrap();
        let name = get_path(&tag, &parse_path("CustomName").unwrap()).unwrap();
        assert_eq!(name, Tag::String("Hi".into()));
        let y = get_path(&tag, &parse_path("Pos[1]").unwrap()).unwrap();
        assert_eq!(number(&y), Some(2.0));
        assert!(format_tag(&y).contains("2.0"));
    }

    #[test]
    fn nested_snbt_is_capped() {
        let shallow = format!("{}1{}", "{a:".repeat(8), "}".repeat(8));
        assert!(parse_value(&shallow).is_some());
        let deep = format!(
            "{}1{}",
            "{a:".repeat(MAX_SNBT_DEPTH + 1),
            "}".repeat(MAX_SNBT_DEPTH + 1)
        );
        assert!(parse_value(&deep).is_none());
    }

    #[test]
    fn brackets_inside_strings_do_not_count() {
        assert_eq!(nesting_depth(r#"{a:"[[[{{{",b:[1]}"#), 2);
    }

    #[test]
    fn set_merge_and_append() {
        let mut root = parse_value("{Items:[{Slot:0b}]}").unwrap();
        set_path(
            &mut root,
            &parse_path("CustomName").unwrap(),
            Tag::String("Hi".into()),
        )
        .unwrap();
        modify(
            &mut root,
            &parse_path("Items").unwrap(),
            Modify::Append,
            Tag::Compound(Compound::from([("Slot".into(), Tag::Byte(1))])),
        )
        .unwrap();
        let items = get_path(&root, &parse_path("Items").unwrap()).unwrap();
        assert_eq!(items.as_list().unwrap().len(), 2);
        modify(
            &mut root,
            &[],
            Modify::Merge,
            parse_value("{Lock:\"a\"}").unwrap(),
        )
        .unwrap();
        assert_eq!(
            get_path(&root, &parse_path("Lock").unwrap()).unwrap(),
            Tag::String("a".into())
        );
        assert!(get_path(&root, &parse_path("CustomName").unwrap()).is_some());
    }
}
