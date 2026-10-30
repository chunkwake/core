//! Диспетчер команд: разбор строки по дереву, права, единый ответ.
//!
//! Ванила держит дерево Brigadier и выполняет найденный узел. Здесь дерево
//! объявлено декларативно ([`super::registry`]), из него же собирается пакет
//! `commands` для клиента и подсказки `command_suggestions`. Диспетчер идёт
//! по литералам, отсекая узлы по уровню прав, и отдаёт обработчику путь и
//! остаток строки — поэтому права объявлены в одном месте, а не в трёх
//! (`builtin_level`, `is_builtin_name`, ветки сессии).

use chunkwake_protocol::text::{Node, Parser};
use serde_json::Value;

use super::args::ArgError;

/// Кто выполнил команду.
#[derive(Clone, Debug)]
pub struct Source {
    pub uuid: Option<uuid::Uuid>,
    pub name: String,
    /// Уровень прав: 0 — игрок, 2 — гейм-мастер, 3 — админ, 4 — владелец.
    pub level: i32,
}

impl Source {
    pub fn console() -> Self {
        Self {
            uuid: None,
            name: "Server".into(),
            level: 4,
        }
    }

    pub fn player(uuid: uuid::Uuid, name: impl Into<String>, level: i32) -> Self {
        Self {
            uuid: Some(uuid),
            name: name.into(),
            level,
        }
    }

    pub fn is_console(&self) -> bool {
        self.uuid.is_none()
    }
}

/// Что делать с результатом.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// Сообщение только выполнившему (или в лог консоли).
    Feedback(Value),
    /// Сообщение всем игрокам.
    Broadcast(Value),
    /// Ничего не выводить (отмена плагином, `gamerule` без значения).
    Silent,
}

impl Outcome {
    pub fn text(text: impl Into<String>) -> Self {
        Outcome::Feedback(text_component(&text.into()))
    }

    pub fn error(text: impl Into<String>) -> Self {
        Outcome::Feedback(error_component(&text.into()))
    }

    pub fn translate(key: &str) -> Self {
        Outcome::Feedback(translate(key, Vec::new()))
    }

    pub fn from_result(result: Result<Value, ArgError>) -> Self {
        match result {
            Ok(v) if v.is_null() => Outcome::Silent,
            Ok(v) => Outcome::Feedback(v),
            Err(e) => Outcome::Feedback(e.component()),
        }
    }
}

/// Обычный текст как компонент.
pub fn text_component(text: &str) -> Value {
    serde_json::json!({"text": text})
}

/// Текст ошибки: красный, как ванильные `sendFailure`.
pub fn error_component(text: &str) -> Value {
    serde_json::json!({"text": text, "color": "red"})
}

/// Перевод с аргументами.
pub fn translate(key: &str, with: Vec<Value>) -> Value {
    if with.is_empty() {
        serde_json::json!({"translate": key})
    } else {
        serde_json::json!({"translate": key, "with": with})
    }
}

/// Обработчик команды: источник, путь литералов и остаток строки.
pub type Handler = fn(&super::Context<'_>, &Call<'_>) -> Outcome;

/// Что пришло обработчику.
pub struct Call<'a> {
    pub source: &'a Source,
    /// Совпавшие литералы после имени команды (`["set"]` для `/time set`).
    pub path: &'a [String],
    /// Аргументы после литералов, как их ввёл игрок.
    pub rest: &'a str,
}

impl Call<'_> {
    ///
    /// Диспетчер передаёт строку после имени команды целиком, поэтому здесь
    /// пропускаются совпавшие литералы (`set`, `rain`, `objectives`). Так
    /// обработчик видит только то, что осталось разобрать.
    pub fn t(&self) -> super::args::Input<'_> {
        let mut input = super::args::Input::new(self.rest);
        for literal in self.path {
            if input.word() != Some(literal.as_str()) {
                // Литерал не в начале (например путь из двух слов): начинаем
                // разбор заново, чтобы не потерять ввод.
                return super::args::Input::new(self.rest);
            }
        }
        input
    }

    /// Позиция источника или центра мира, если это консоль.
    pub fn position(&self, ctx: &super::Context<'_>) -> [f64; 3] {
        if let Some((pos, _, _)) = ctx.execution_origin() {
            return pos;
        }
        self.source
            .uuid
            .and_then(|uuid| ctx.pose(uuid))
            .map(|(pos, _, _)| pos)
            .unwrap_or(ctx.spawn())
    }
}

/// Объявление узла дерева.
#[derive(Clone)]
pub struct Spec {
    pub name: String,
    /// Аргумент или литерал.
    pub parser: Option<Parser>,
    /// Можно ли выполнить команду на этом узле.
    pub exec: bool,
    /// Минимальный уровень прав.
    pub level: i32,
    /// Команда, которой узел передаёт разбор (ванильный алиас `tell` → `msg`,
    /// цикл `/execute … run`, `xp` → `experience`).
    pub redirect: Option<String>,
    pub handler: Option<Handler>,
    pub children: Vec<Spec>,
}

impl Spec {
    /// Промежуточный литерал (`/time`).
    pub fn group(name: &str, level: i32, children: Vec<Spec>) -> Self {
        Self {
            name: name.into(),
            parser: None,
            exec: false,
            level,
            redirect: None,
            handler: None,
            children,
        }
    }

    /// Исполняемый литерал-лист (`/list`).
    pub fn leaf(name: &str, level: i32, handler: Handler) -> Self {
        Self {
            name: name.into(),
            parser: None,
            exec: true,
            level,
            redirect: None,
            handler: Some(handler),
            children: Vec::new(),
        }
    }

    /// Исполняемый литерал с детьми: `/time` и `/time set …`.
    pub fn exec(name: &str, level: i32, handler: Handler, children: Vec<Spec>) -> Self {
        Self {
            name: name.into(),
            parser: None,
            exec: true,
            level,
            redirect: None,
            handler: Some(handler),
            children,
        }
    }

    /// Алиас: литерал без своих аргументов, разбор уходит в другую команду
    /// (`/tell` → `/msg`). Обработчика нет — вызов идёт через цель.
    pub fn alias(name: &str, level: i32, target: &str) -> Self {
        Self {
            name: name.into(),
            parser: None,
            exec: false,
            level,
            redirect: Some(target.into()),
            handler: None,
            children: Vec::new(),
        }
    }

    /// Аргумент с детьми (`/execute as <targets>`).
    pub fn arg(name: &str, level: i32, parser: Parser, children: Vec<Spec>) -> Self {
        Self {
            name: name.into(),
            parser: Some(parser),
            exec: false,
            level,
            redirect: None,
            handler: None,
            children,
        }
    }

    /// Аргумент, возвращающий разбор в корень другой команды
    /// (`/execute as <targets> …`).
    pub fn arg_redirect(
        name: &str,
        level: i32,
        parser: Parser,
        target: &str,
        children: Vec<Spec>,
    ) -> Self {
        Self {
            name: name.into(),
            parser: Some(parser),
            exec: false,
            level,
            redirect: Some(target.into()),
            handler: None,
            children,
        }
    }

    /// Тот же узел с детьми: объявление остаётся линейным.
    pub fn with_children(mut self, children: Vec<Spec>) -> Self {
        self.children = children;
        self
    }

    /// Обработчик без `executable`: корень с ветками (`/teleport`, `/give`)
    /// принимает строку целиком, но клиент не считает команду законченной.
    pub fn with_handler(mut self, handler: Handler) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Тот же узел, возвращающий разбор в другую команду (`/execute …`).
    pub fn redirect_to(mut self, target: &str) -> Self {
        self.redirect = Some(target.into());
        self
    }

    /// Исполняемый литерал-лист без обработчика-листа (для подкоманд).
    pub fn literal_leaf(name: &str, level: i32, handler: Handler) -> Self {
        Spec::leaf(name, level, handler)
    }

    /// Аргумент с обработчиком, но не `executable`: строка продолжается
    /// дальше (`/setblock <pos> <block>`).
    pub fn arg_group(name: &str, level: i32, parser: Parser, handler: Handler) -> Self {
        Self {
            name: name.into(),
            parser: Some(parser),
            exec: false,
            level,
            redirect: None,
            handler: Some(handler),
            children: Vec::new(),
        }
    }

    /// Исполняемый аргумент-лист.
    pub fn arg_leaf(name: &str, level: i32, parser: Parser, handler: Handler) -> Self {
        Self {
            name: name.into(),
            parser: Some(parser),
            exec: true,
            level,
            redirect: None,
            handler: Some(handler),
            children: Vec::new(),
        }
    }

    /// Узел протокола; дети фильтруются по уровню прав.
    pub fn node(&self, level: i32) -> Node {
        let children = self
            .children
            .iter()
            .filter(|c| level >= c.level)
            .map(|c| c.node(level))
            .collect();
        let node = match &self.parser {
            Some(parser) => Node::argument(&self.name, self.exec, parser.clone(), children),
            None => Node::literal(&self.name, self.exec, children),
        };
        match &self.redirect {
            Some(target) => node.redirect_to(target),
            None => node,
        }
    }
}

/// Имя команды и остаток строки.
fn split_head(line: &str) -> (&str, &str) {
    match line.trim().split_once(char::is_whitespace) {
        Some((head, rest)) => (head, rest.trim_start()),
        None => (line.trim(), ""),
    }
}

/// Совпадение пути с объявленным деревом.
pub struct Resolved<'a> {
    /// Узел, на котором остановился разбор.
    pub spec: &'a Spec,
    /// Совпавшие литералы после имени команды.
    pub path: Vec<String>,
    /// Все аргументы после имени команды, как их ввёл игрок.
    ///
    /// Литералы из `path` здесь тоже остаются: обработчику проще разобрать
    /// строку целиком, а путь нужен для прав и подсказок.
    pub args: String,
    /// Уровень, требуемый всем путём: максимум по пройденным узлам.
    pub level: i32,
}

/// Таблица объявленных команд.
pub struct Tree {
    specs: Vec<Spec>,
}

impl Tree {
    pub fn new(specs: Vec<Spec>) -> Self {
        Self { specs }
    }

    pub fn specs(&self) -> &[Spec] {
        &self.specs
    }

    /// Дерево для пакета `commands`: только доступное по уровню.
    pub fn nodes_for(&self, level: i32) -> Vec<Node> {
        self.specs
            .iter()
            .filter(|s| level >= s.level)
            .map(|s| s.node(level))
            .collect()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.specs.iter().map(|s| s.name.as_str())
    }

    pub fn spec(&self, name: &str) -> Option<&Spec> {
        self.specs.iter().find(|s| s.name == name)
    }

    pub fn level_of(&self, name: &str) -> Option<i32> {
        self.spec(name).map(|s| s.level)
    }

    /// Разбор строки без `/`.
    ///
    /// `None` — команда не наша: сессия отдаёт её плагинам и отвечает
    /// `Unknown command`.
    pub fn parse(&self, line: &str) -> Option<Resolved<'_>> {
        // Алиас (`/tell` → `/msg`, `/tp` → `/teleport`, `/xp` →
        // `/experience`): имя заменяется на целевое, и разбор идёт заново.
        // Редиректы внутри веток (`/execute as …`) нужны только клиенту.
        let mut line = line.trim().to_owned();
        let mut level = 0;
        for _ in 0..4 {
            let (head, rest) = split_head(&line);
            let root = self.spec(head)?;
            level = level.max(root.level);
            match root.redirect.clone() {
                Some(target) => {
                    line = if rest.is_empty() {
                        target
                    } else {
                        format!("{target} {rest}")
                    };
                }
                None => return Some(self.descend(root, rest, level)),
            }
        }
        None
    }

    /// Разбор после имени команды: литералы в `path`, остальное — аргументы.
    fn descend<'a>(&self, root: &'a Spec, rest: &str, level: i32) -> Resolved<'a> {
        let mut spec = root;
        let mut level = level;
        let mut path = Vec::new();
        // Аргументы целиком: обработчику проще разобрать всю строку.
        let args = rest.to_owned();
        let mut rest = rest;
        loop {
            let Some((word, tail)) = rest.split_once(char::is_whitespace) else {
                break;
            };
            let tail = tail.trim_start();
            // Продолжаем путь только по литералам: аргумент забирает обработчик.
            let Some(child) = spec
                .children
                .iter()
                .find(|c| c.parser.is_none() && c.name == word)
            else {
                break;
            };
            level = level.max(child.level);
            path.push(word.to_owned());
            spec = child;
            rest = tail;
        }
        // Последнее слово без хвоста тоже может быть литералом.
        if !rest.is_empty()
            && !rest.contains(char::is_whitespace)
            && let Some(child) = spec
                .children
                .iter()
                .find(|c| c.parser.is_none() && c.name == rest)
        {
            level = level.max(child.level);
            path.push(rest.to_owned());
            spec = child;
        }
        // Глубокий узел без обработчика (`/time query`, `/execute as @a run`)
        // выполняет ближайшего предка с обработчиком — он разбирает хвост сам.
        // Сверху вниз запоминаем последний исполнимый узел пути.
        if spec.handler.is_none() {
            let mut found = if root.handler.is_some() { Some(root) } else { None };
            let mut walk = root;
            for literal in &path {
                match walk
                    .children
                    .iter()
                    .find(|c| c.parser.is_none() && c.name == *literal)
                {
                    Some(child) => {
                        if child.handler.is_some() {
                            found = Some(child);
                        }
                        walk = child;
                    }
                    None => break,
                }
            }
            if let Some(ancestor) = found {
                spec = ancestor;
            }
        }
        // Группа без обработчика (`/weather`): ищем единственного исполнителя
        // среди потомков, чтобы вызов нашёл обработчик и не потерял права.
        while spec.handler.is_none() {
            let mut executable: Vec<&Spec> =
                spec.children.iter().filter(|c| c.handler.is_some()).collect();
            let only = match executable.len() {
                1 => executable.pop(),
                _ => None,
            };
            match only {
                Some(child) => {
                    level = level.max(child.level);
                    spec = child;
                }
                None => break,
            }
        }
        Resolved {
            spec,
            path,
            args,
            level,
        }
    }

    /// Подсказки для tab-complete на текущей глубине.
    ///
    /// Литералы клиент подсказывает сам по дереву; сюда приходит то, что
    /// требует сервера (`ask_server`) и верхний уровень. Алиас разворачивается
    /// в цель, чтобы `/tell <tab>` показывал аргументы `/msg`.
    pub fn suggest(&self, line: &str, level: i32) -> Vec<String> {
        let trimmed = line.trim_start();
        let (body, prefix) = match trimmed.rfind(char::is_whitespace) {
            Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
            None => ("", trimmed),
        };
        let words: Vec<&str> = body.split_whitespace().collect();
        let children: &[Spec] = match words.split_first() {
            None => &self.specs,
            Some((first, rest)) => match self.spec(first) {
                Some(root) => match self.walk_from(root, rest) {
                    Some(node) => &node.children,
                    None => return Vec::new(),
                },
                None => return Vec::new(),
            },
        };
        let mut out: Vec<String> = children
            .iter()
            .filter(|child| level >= child.level && child.name.starts_with(prefix))
            .map(|child| child.name.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Узел после имени команды: алиас ведёт в цель, дальше — литералы.
    fn walk_from<'a>(&'a self, root: &'a Spec, words: &[&str]) -> Option<&'a Spec> {
        let mut spec = root;
        let mut hops = 0;
        while let Some(target) = spec.redirect.as_deref() {
            if hops == 4 {
                return None;
            }
            hops += 1;
            spec = self.spec(target)?;
        }
        for word in words {
            spec = spec
                .children
                .iter()
                .find(|c| c.parser.is_none() && c.name == *word)?;
        }
        Some(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::commands::registry;

    #[test]
    fn parses_known_commands() {
        let tree = registry::tree();
        let resolved = tree.parse("time set day").unwrap();
        assert_eq!(resolved.path, ["set"]);
        assert_eq!(resolved.args, "set day");
        let resolved = tree.parse("give").unwrap();
        assert!(resolved.path.is_empty());
        assert!(tree.parse("nosuch").is_none());
    }

    #[test]
    fn deepest_literal_wins_over_argument() {
        let tree = registry::tree();
        // `/tp` не имеет литералов после себя: имя игрока уходит в аргументы.
        let resolved = tree.parse("tp Alice 1 2 3").unwrap();
        assert!(resolved.path.is_empty());
        assert_eq!(resolved.args, "Alice 1 2 3");
    }

    #[test]
    fn hidden_by_level() {
        let tree = registry::tree();
        assert!(tree.level_of("op").unwrap() >= 3);
        let nodes = tree.nodes_for(0);
        let names: Vec<&str> = nodes.iter().map(Node::name).collect();
        assert!(!names.contains(&"op"));
        assert!(names.contains(&"list"));
        assert!(names.contains(&"tps"));
    }

    #[test]
    fn suggests_subcommands() {
        let tree = registry::tree();
        let out = tree.suggest("time ", 4);
        assert!(out.contains(&"set".to_owned()), "{out:?}");
        let out = tree.suggest("ti", 4);
        assert!(out.contains(&"time".to_owned()));
        let out = tree.suggest("op ", 0);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn every_root_is_a_literal_with_a_level() {
        for spec in registry::commands() {
            assert!(spec.parser.is_none(), "{}", spec.name);
        }
    }
}
