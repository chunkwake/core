//! Текстовые компоненты (JSON Adventure → сетевой NBT) и дерево команд Brigadier.

use crate::Writer;
use crate::configuration::{modified_utf8, write_nbt_string_root};
use serde_json::Value;

const TAG_END: u8 = 0;
const TAG_BYTE: u8 = 1;
const TAG_INT: u8 = 3;
const TAG_LONG: u8 = 4;
const TAG_DOUBLE: u8 = 6;
const TAG_STRING: u8 = 8;
const TAG_LIST: u8 = 9;
const TAG_COMPOUND: u8 = 10;

/// `system_chat` с компонентом в JSON (как его отдаёт `GsonComponentSerializer`).
/// Неразборный JSON уходит как простой текст.
pub fn system_chat_json(id: i32, json: &str, overlay: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    match serde_json::from_str::<Value>(json) {
        Ok(v) => write_component(&mut w, &v),
        Err(_) => write_nbt_string_root(&mut w, json),
    }
    w.bool(overlay);
    w.buf
}

/// Компонент как безымянный корень сетевого NBT.
pub fn write_component(w: &mut Writer, v: &Value) {
    match v {
        Value::String(s) => write_nbt_string_root(w, s),
        Value::Object(_) => {
            w.u8(TAG_COMPOUND);
            compound_body(w, v);
        }
        // Массив — первый элемент с остальными в `extra`.
        Value::Array(items) => {
            w.u8(TAG_COMPOUND);
            str_entry(w, "text", "");
            named(w, "extra", &Value::Array(items.clone()));
            w.u8(TAG_END);
        }
        other => write_nbt_string_root(w, &scalar_text(other)),
    }
}

/// Безымянный корень сетевого NBT → JSON (для текстовых компонентов от клиента).
/// Байт становится `bool` при 0/1 (флаги стиля), массивы — списками чисел.
pub fn read_component(r: &mut crate::Reader) -> crate::Result<Value> {
    let tag = r.u8()?;
    read_payload(r, tag, 0)
}

fn read_nbt_string(r: &mut crate::Reader) -> crate::Result<String> {
    let len = r.u16()? as usize;
    Ok(String::from_utf8_lossy(r.bytes(len)?).into_owned())
}

fn read_payload(r: &mut crate::Reader, tag: u8, depth: u32) -> crate::Result<Value> {
    if depth > 64 {
        return Err(crate::ProtocolError::BadLength(depth as i32));
    }
    let i32_of = |r: &mut crate::Reader| -> crate::Result<i32> {
        let b = r.bytes(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    Ok(match tag {
        1 => match r.u8()? as i8 {
            0 => Value::Bool(false),
            1 => Value::Bool(true),
            b => Value::from(b),
        },
        2 => Value::from(r.u16()? as i16),
        3 => Value::from(i32_of(r)?),
        4 => Value::from(r.i64()?),
        5 => Value::from(r.f32()? as f64),
        6 => Value::from(r.f64()?),
        7 => {
            let n = i32_of(r)?.max(0) as usize;
            Value::from(
                r.bytes(n)?
                    .iter()
                    .map(|&b| b as i8 as i64)
                    .collect::<Vec<_>>(),
            )
        }
        8 => Value::String(read_nbt_string(r)?),
        9 => {
            let inner = r.u8()?;
            let n = i32_of(r)?.max(0);
            let mut out = Vec::new();
            for _ in 0..n {
                out.push(read_payload(r, inner, depth + 1)?);
            }
            Value::Array(out)
        }
        10 => {
            let mut map = serde_json::Map::new();
            loop {
                let t = r.u8()?;
                if t == TAG_END {
                    break;
                }
                let k = read_nbt_string(r)?;
                map.insert(k, read_payload(r, t, depth + 1)?);
            }
            Value::Object(map)
        }
        11 => {
            let n = i32_of(r)?.max(0);
            let mut out = Vec::new();
            for _ in 0..n {
                out.push(Value::from(i32_of(r)?));
            }
            Value::Array(out)
        }
        12 => {
            let n = i32_of(r)?.max(0);
            let mut out = Vec::new();
            for _ in 0..n {
                out.push(Value::from(r.i64()?));
            }
            Value::Array(out)
        }
        t => return Err(crate::ProtocolError::BadLength(t as i32)),
    })
}

fn scalar_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn tag_of(v: &Value) -> u8 {
    match v {
        Value::Bool(_) => TAG_BYTE,
        Value::Number(n) if n.is_i64() => {
            if i32::try_from(n.as_i64().unwrap_or(0)).is_ok() {
                TAG_INT
            } else {
                TAG_LONG
            }
        }
        Value::Number(_) => TAG_DOUBLE,
        Value::String(_) | Value::Null => TAG_STRING,
        Value::Array(_) => TAG_LIST,
        Value::Object(_) => TAG_COMPOUND,
    }
}

fn name(w: &mut Writer, s: &str) {
    let b = modified_utf8(s);
    w.u16(b.len() as u16);
    w.buf.extend_from_slice(&b);
}

fn str_entry(w: &mut Writer, key: &str, s: &str) {
    w.u8(TAG_STRING);
    name(w, key);
    name(w, s);
}

fn named(w: &mut Writer, key: &str, v: &Value) {
    w.u8(tag_of(v));
    name(w, key);
    payload(w, v);
}

fn compound_body(w: &mut Writer, v: &Value) {
    if let Value::Object(map) = v {
        for (k, x) in map {
            if !x.is_null() {
                named(w, k, x);
            }
        }
    }
    w.u8(TAG_END);
}

fn payload(w: &mut Writer, v: &Value) {
    match v {
        Value::Bool(b) => {
            w.u8(*b as u8);
        }
        Value::Number(n) => match tag_of(v) {
            TAG_INT => {
                w.i32(n.as_i64().unwrap_or(0) as i32);
            }
            TAG_LONG => {
                w.i64(n.as_i64().unwrap_or(0));
            }
            _ => {
                w.f64(n.as_f64().unwrap_or(0.0));
            }
        },
        Value::String(s) => name(w, s),
        Value::Null => name(w, ""),
        Value::Object(_) => compound_body(w, v),
        Value::Array(items) => list(w, items),
    }
}

/// Текст компонента для лога: `text` и `extra`; у `translate` — ключ и аргументы.
pub fn plain_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(plain_text).collect(),
        Value::Object(o) => {
            let mut out = match (o.get("text"), o.get("translate")) {
                (Some(t), _) => plain_text(t),
                (None, Some(Value::String(key))) => {
                    let args: Vec<String> = o
                        .get("with")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().map(plain_text).collect())
                        .unwrap_or_default();
                    if args.is_empty() {
                        key.clone()
                    } else {
                        format!("{key} {}", args.join(" "))
                    }
                }
                _ => String::new(),
            };
            if let Some(extra) = o.get("extra") {
                out += &plain_text(extra);
            }
            out
        }
        other => other.to_string(),
    }
}

/// Текст для консоли сервера: ключ `translate` подставляется в строку из `en_us`.
///
/// Клиенту по-прежнему уходит ключ. В логе оператор видит фразу.
/// Нет строки в словаре — ключ и аргументы через запятую, не слипшаяся строка.
pub fn console_line(v: &Value, lang: &std::collections::HashMap<String, String>) -> String {
    match v {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| console_line(item, lang))
            .collect::<Vec<_>>()
            .join(""),
        Value::Object(object) => {
            let mut out = if let Some(Value::String(key)) = object.get("translate") {
                let args = object
                    .get("with")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| console_line(item, lang))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if let Some(template) = lang.get(key) {
                    apply_format(template, &args)
                } else if let Some(text) = object.get("text") {
                    console_line(text, lang)
                } else if args.is_empty() {
                    key.clone()
                } else {
                    format!("{key}: {}", args.join(", "))
                }
            } else if let Some(text) = object.get("text") {
                console_line(text, lang)
            } else {
                String::new()
            };
            if let Some(extra) = object.get("extra") {
                out.push_str(&console_line(extra, lang));
            }
            out
        }
        other => other.to_string(),
    }
}

/// Подстановка `%s`, `%1$s` и `%%`, как в строках перевода Minecraft.
pub fn apply_format(template: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    let mut next = 0usize;
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some(digit) if digit.is_ascii_digit() => {
                let mut index = 0usize;
                while let Some(digit) = chars.peek().copied().filter(|ch| ch.is_ascii_digit()) {
                    chars.next();
                    index = index * 10 + (digit as usize - '0' as usize);
                }
                if chars.peek() == Some(&'$') {
                    chars.next();
                }
                if chars.peek() == Some(&'s') {
                    chars.next();
                }
                if let Some(arg) = index.checked_sub(1).and_then(|index| args.get(index)) {
                    out.push_str(arg);
                }
            }
            Some('s') => {
                chars.next();
                if let Some(arg) = args.get(next) {
                    out.push_str(arg);
                }
                next += 1;
            }
            _ => out.push('%'),
        }
    }
    out
}

/// Список NBT однороден: одинаковые скаляры — как есть, иначе каждый элемент —
/// компонент-компаунд (строка → `{"text": ...}`).
fn list(w: &mut Writer, items: &[Value]) {
    let first = items.first().map(tag_of).unwrap_or(TAG_END);
    let same = items.iter().all(|x| tag_of(x) == first);
    if same && first != TAG_LIST {
        w.u8(first).i32(items.len() as i32);
        for x in items {
            payload(w, x);
        }
        return;
    }
    w.u8(TAG_COMPOUND).i32(items.len() as i32);
    for x in items {
        match x {
            Value::Object(_) => compound_body(w, x),
            other => {
                str_entry(w, "text", &scalar_text(other));
                w.u8(TAG_END);
            }
        }
    }
}

const NODE_ROOT: u8 = 0;
const NODE_LITERAL: u8 = 1;
const NODE_ARGUMENT: u8 = 2;
const NODE_EXECUTABLE: u8 = 0x04;
/// У узла есть `redirect`: после детей идёт индекс узла, куда переходит разбор.
const NODE_REDIRECT: u8 = 0x08;
const NODE_CUSTOM_SUGGESTIONS: u8 = 0x10;
/// `StringArgumentType.greedyString()`.
const STRING_GREEDY: i32 = 2;
/// `StringArgumentType.singleWord()`.
const STRING_SINGLE_WORD: i32 = 0;
/// `StringArgumentType.quotablePhrase()`.
const STRING_QUOTABLE: i32 = 1;

/// Все имена парсеров аргументов в реестре `command_argument_type`.
///
/// Порядок совпадает с ванильным реестром: так проще сверять с
/// `reports/registries.json` и не забыть ни одного типа.
pub const ARGUMENT_TYPES: &[&str] = &[
    "brigadier:bool",
    "brigadier:double",
    "brigadier:float",
    "brigadier:integer",
    "brigadier:long",
    "brigadier:string",
    "minecraft:angle",
    "minecraft:block_pos",
    "minecraft:block_predicate",
    "minecraft:block_state",
    "minecraft:column_pos",
    "minecraft:component",
    "minecraft:context_float_provider",
    "minecraft:context_int_provider",
    "minecraft:dialog",
    "minecraft:dimension",
    "minecraft:entity",
    "minecraft:entity_anchor",
    "minecraft:feature",
    "minecraft:float_range",
    "minecraft:function",
    "minecraft:game_profile",
    "minecraft:gamemode",
    "minecraft:heightmap",
    "minecraft:hex_color",
    "minecraft:int_range",
    "minecraft:item_predicate",
    "minecraft:item_slot",
    "minecraft:item_slots",
    "minecraft:item_stack",
    "minecraft:loot_modifier",
    "minecraft:loot_predicate",
    "minecraft:loot_table",
    "minecraft:message",
    "minecraft:nbt_compound_tag",
    "minecraft:nbt_path",
    "minecraft:nbt_tag",
    "minecraft:objective",
    "minecraft:objective_criteria",
    "minecraft:operation",
    "minecraft:particle",
    "minecraft:resource",
    "minecraft:resource_key",
    "minecraft:resource_location",
    "minecraft:resource_or_tag",
    "minecraft:resource_or_tag_key",
    "minecraft:resource_selector",
    "minecraft:rotation",
    "minecraft:score_holder",
    "minecraft:scoreboard_slot",
    "minecraft:slot_source",
    "minecraft:style",
    "minecraft:swing_animation",
    "minecraft:swizzle",
    "minecraft:team",
    "minecraft:team_color",
    "minecraft:template_mirror",
    "minecraft:template_rotation",
    "minecraft:time",
    "minecraft:uuid",
    "minecraft:vec2",
    "minecraft:vec3",
];

/// ID парсеров аргументов в реестре `command_argument_type`.
///
/// Раньше это была структура из четырёх полей: остальные типы приходилось
/// выдавать за `brigadier:string`, и клиент не подсвечивал ни селекторы,
/// ни координаты, ни реестровые ключи.
#[derive(Clone, Debug, Default)]
pub struct CommandParsers {
    ids: std::collections::HashMap<&'static str, i32>,
}

impl CommandParsers {
    /// Собирает таблицу из замыкания «имя → ID»; отсутствующий тип — ошибка.
    pub fn load<E>(mut lookup: impl FnMut(&'static str) -> Result<i32, E>) -> Result<Self, E> {
        let mut ids = std::collections::HashMap::with_capacity(ARGUMENT_TYPES.len());
        for name in ARGUMENT_TYPES {
            ids.insert(*name, lookup(name)?);
        }
        Ok(Self { ids })
    }

    /// ID типа; 0 — таблица не заполнена (тесты, сервер без данных).
    pub fn id(&self, name: &str) -> i32 {
        self.ids.get(name).copied().unwrap_or(0)
    }

    /// Подставляет ID вручную: нужно тестам и минимальным сборкам.
    pub fn set(&mut self, name: &'static str, id: i32) {
        self.ids.insert(name, id);
    }
}

/// Узел дерева Brigadier. `exec` — команду можно выполнить на этом узле.
///
/// `redirect` — имя корневой команды, куда узел передаёт разбор (ванильные
/// алиасы `tell` → `msg` и цикл `execute`). Клиент по нему подсказывает
/// аргументы чужой ветки, а сервер — продолжает разбор.
#[derive(Clone, Debug)]
pub enum Node {
    Literal {
        name: String,
        exec: bool,
        redirect: Option<String>,
        children: Vec<Node>,
    },
    Argument {
        name: String,
        exec: bool,
        parser: Parser,
        redirect: Option<String>,
        children: Vec<Node>,
    },
}

/// `brigadier:string`: как читать слово.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringMode {
    /// `SINGLE_WORD`: одно слово без пробелов.
    Word,
    /// `QUOTABLE_PHRASE`: слово или строка в кавычках.
    Quotable,
    /// `GREEDY_PHRASE`: весь остаток строки.
    Greedy,
}

/// Парсер аргумента со своими свойствами.
///
/// Клиент по типу аргумента выбирает подсветку и подсказки: селекторы,
/// координаты, режимы игры, ключи реестров. `Raw` — тип, у которого свойств
/// нет (или он нам пока не нужен): пишется только ID.
#[derive(Clone, Debug)]
pub enum Parser {
    Bool,
    Double(f64, f64),
    Float(f32, f32),
    /// Целое с минимумом и максимумом.
    Integer(i32, i32),
    Long(i64, i64),
    /// `brigadier:string`.
    String(StringMode),
    /// Greedy-строка с подсказками от сервера (`minecraft:ask_server`).
    GreedyStringAsk,
    Angle,
    BlockPos,
    ColumnPos,
    BlockState,
    BlockPredicate,
    Vec2,
    Vec3,
    Rotation,
    Anchor,
    /// `minecraft:time`: минимальное значение в тиках.
    Time(i32),
    Gamemode,
    /// `minecraft:game_profile`: имя игрока (в т.ч. офлайн).
    GameProfile,
    /// `minecraft:entity` / `minecraft:game_profile`: только игроки и/или одна цель.
    Entity {
        players_only: bool,
        single: bool,
    },
    /// `minecraft:score_holder`: `multiple` разрешает `*`.
    ScoreHolder {
        multiple: bool,
    },
    Component,
    Message,
    ItemStack,
    ItemPredicate,
    ItemSlot,
    ItemSlots,
    /// Ключ реестра (`minecraft:resource`), например `minecraft:mob_effect`.
    Resource(&'static str),
    ResourceKey(&'static str),
    ResourceOrTag(&'static str),
    ResourceOrTagKey(&'static str),
    ResourceLocation,
    ResourceSelector(&'static str),
    Objective,
    ObjectiveCriteria,
    Operation,
    Particle,
    ScoreboardSlot,
    Team,
    TeamColor,
    Heightmap,
    Dimension,
    Swizzle,
    TemplateMirror,
    TemplateRotation,
    Uuid,
    IntRange,
    FloatRange,
    Style,
    NbtCompoundTag,
    NbtTag,
    NbtPath,
    LootTable,
    LootPredicate,
    LootModifier,
    Feature,
    Function,
    Dialog,
    HexColor,
    /// Тип без свойств: пишется только ID.
    Raw(&'static str),
}

impl Parser {
    /// Имя типа в реестре `command_argument_type`.
    pub fn kind(&self) -> &'static str {
        match self {
            Parser::Bool => "brigadier:bool",
            Parser::Double(..) => "brigadier:double",
            Parser::Float(..) => "brigadier:float",
            Parser::Integer(..) => "brigadier:integer",
            Parser::Long(..) => "brigadier:long",
            Parser::String(_) | Parser::GreedyStringAsk => "brigadier:string",
            Parser::Angle => "minecraft:angle",
            Parser::BlockPos => "minecraft:block_pos",
            Parser::ColumnPos => "minecraft:column_pos",
            Parser::BlockState => "minecraft:block_state",
            Parser::BlockPredicate => "minecraft:block_predicate",
            Parser::Vec2 => "minecraft:vec2",
            Parser::Vec3 => "minecraft:vec3",
            Parser::Rotation => "minecraft:rotation",
            Parser::Anchor => "minecraft:entity_anchor",
            Parser::Time(_) => "minecraft:time",
            Parser::Gamemode => "minecraft:gamemode",
            Parser::GameProfile => "minecraft:game_profile",
            Parser::Entity { .. } => "minecraft:entity",
            Parser::ScoreHolder { .. } => "minecraft:score_holder",
            Parser::Component => "minecraft:component",
            Parser::Message => "minecraft:message",
            Parser::ItemStack => "minecraft:item_stack",
            Parser::ItemPredicate => "minecraft:item_predicate",
            Parser::ItemSlot => "minecraft:item_slot",
            Parser::ItemSlots => "minecraft:item_slots",
            Parser::Resource(_) => "minecraft:resource",
            Parser::ResourceKey(_) => "minecraft:resource_key",
            Parser::ResourceOrTag(_) => "minecraft:resource_or_tag",
            Parser::ResourceOrTagKey(_) => "minecraft:resource_or_tag_key",
            Parser::ResourceLocation => "minecraft:resource_location",
            Parser::ResourceSelector(_) => "minecraft:resource_selector",
            Parser::Objective => "minecraft:objective",
            Parser::ObjectiveCriteria => "minecraft:objective_criteria",
            Parser::Operation => "minecraft:operation",
            Parser::Particle => "minecraft:particle",
            Parser::ScoreboardSlot => "minecraft:scoreboard_slot",
            Parser::Team => "minecraft:team",
            Parser::TeamColor => "minecraft:team_color",
            Parser::Heightmap => "minecraft:heightmap",
            Parser::Dimension => "minecraft:dimension",
            Parser::Swizzle => "minecraft:swizzle",
            Parser::TemplateMirror => "minecraft:template_mirror",
            Parser::TemplateRotation => "minecraft:template_rotation",
            Parser::Uuid => "minecraft:uuid",
            Parser::IntRange => "minecraft:int_range",
            Parser::FloatRange => "minecraft:float_range",
            Parser::Style => "minecraft:style",
            Parser::NbtCompoundTag => "minecraft:nbt_compound_tag",
            Parser::NbtTag => "minecraft:nbt_tag",
            Parser::NbtPath => "minecraft:nbt_path",
            Parser::LootTable => "minecraft:loot_table",
            Parser::LootPredicate => "minecraft:loot_predicate",
            Parser::LootModifier => "minecraft:loot_modifier",
            Parser::Feature => "minecraft:feature",
            Parser::Function => "minecraft:function",
            Parser::Dialog => "minecraft:dialog",
            Parser::HexColor => "minecraft:hex_color",
            Parser::Raw(name) => name,
        }
    }

    /// Строка с подсказками от сервера (`minecraft:ask_server`).
    pub fn asks_server(&self) -> bool {
        matches!(self, Parser::GreedyStringAsk)
    }
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Node::Literal { name, .. } | Node::Argument { name, .. } => name,
        }
    }

    pub fn literal(name: &str, exec: bool, children: Vec<Node>) -> Self {
        Node::Literal {
            name: name.into(),
            exec,
            redirect: None,
            children,
        }
    }
    pub fn argument(name: &str, exec: bool, parser: Parser, children: Vec<Node>) -> Self {
        Node::Argument {
            name: name.into(),
            exec,
            parser,
            redirect: None,
            children,
        }
    }

    /// Тот же узел, передающий разбор другой корневой команде (ванильные
    /// алиасы и цикл `/execute`).
    pub fn redirect_to(mut self, target: &str) -> Self {
        match &mut self {
            Node::Literal { redirect, .. } | Node::Argument { redirect, .. } => {
                *redirect = Some(target.into());
            }
        }
        self
    }

    fn redirect(&self) -> Option<&str> {
        match self {
            Node::Literal { redirect, .. } | Node::Argument { redirect, .. } => redirect.as_deref(),
        }
    }

    fn children(&self) -> &[Node] {
        match self {
            Node::Literal { children, .. } | Node::Argument { children, .. } => children,
        }
    }
}

/// Пакет `commands`: корень с узлами `roots`. Узлы пишутся в порядке обхода в
/// ширину, дети ссылаются на индексы.
pub fn commands_tree(id: i32, roots: &[Node], parsers: CommandParsers) -> Vec<u8> {
    // Плоский список: индекс 0 — корень; у каждого узла — индексы детей.
    let mut flat: Vec<(Option<&Node>, Vec<i32>)> = vec![(None, Vec::new())];
    let mut queue: Vec<(usize, &Node)> = roots.iter().map(|n| (0, n)).collect();
    let mut i = 0;
    while i < queue.len() {
        let (parent, node) = queue[i];
        let index = flat.len();
        flat.push((Some(node), Vec::new()));
        flat[parent].1.push(index as i32);
        queue.extend(node.children().iter().map(|c| (index, c)));
        i += 1;
    }
    // Корни идут первыми и в порядке `roots`: индекс узла — позиция в очереди.
    let root_index = |name: &str| -> Option<i32> {
        roots
            .iter()
            .position(|n| n.name() == name)
            .map(|p| p as i32 + 1)
    };
    let mut w = Writer::new();
    w.var_int(id).var_int(flat.len() as i32);
    for (node, children) in &flat {
        let exec = |e: bool| if e { NODE_EXECUTABLE } else { 0 };
        let redirect = node.and_then(|n| n.redirect());
        let flag = |base: u8| {
            let mut flags = base;
            if redirect.is_some() {
                flags |= NODE_REDIRECT;
            }
            flags
        };
        match node {
            None => {
                w.u8(NODE_ROOT);
            }
            Some(Node::Literal { exec: e, .. }) => {
                w.u8(flag(NODE_LITERAL | exec(*e)));
            }
            Some(Node::Argument {
                exec: e, parser, ..
            }) => {
                let ask = if parser.asks_server() {
                    NODE_CUSTOM_SUGGESTIONS
                } else {
                    0
                };
                w.u8(flag(NODE_ARGUMENT | exec(*e) | ask));
            }
        }
        w.var_int(children.len() as i32);
        for &c in children {
            w.var_int(c);
        }
        // `redirect` идёт между детьми и именем: индекс узла, куда переходит
        // разбор. Неизвестная цель — ошибка сборки, а не молчаливое 0.
        if let Some(target) = redirect {
            let index = root_index(target)
                .unwrap_or_else(|| panic!("redirect на неизвестную команду {target}"));
            w.var_int(index);
        }
        match node {
            None => {}
            Some(Node::Literal { name, .. }) => {
                w.string(name);
            }
            Some(Node::Argument { name, parser, .. }) => {
                w.string(name);
                write_parser(&mut w, parser, &parsers);
            }
        }
    }
    w.var_int(0);
    w.buf
}

/// Свойства парсера после его ID (`ArgumentTypeInfo` ванилы в кодировке сети).
fn write_parser(w: &mut Writer, parser: &Parser, parsers: &CommandParsers) {
    w.var_int(parsers.id(parser.kind()));
    match parser {
        Parser::Bool => {}
        // Флаги диапазона: 1 — есть минимум, 2 — есть максимум.
        Parser::Double(min, max) => {
            w.u8(3).f64(*min).f64(*max);
        }
        Parser::Float(min, max) => {
            w.u8(3).f32(*min).f32(*max);
        }
        Parser::Long(min, max) => {
            w.u8(3).i64(*min).i64(*max);
        }
        Parser::Integer(min, max) => {
            w.u8(3).i32(*min).i32(*max);
        }
        Parser::String(mode) => {
            let mode = match mode {
                StringMode::Word => STRING_SINGLE_WORD,
                StringMode::Quotable => STRING_QUOTABLE,
                StringMode::Greedy => STRING_GREEDY,
            };
            w.var_int(mode);
        }
        Parser::GreedyStringAsk => {
            w.var_int(STRING_GREEDY).string("minecraft:ask_server");
        }
        Parser::Time(min) => {
            w.i32(*min);
        }
        Parser::Entity {
            players_only,
            single,
        } => {
            // `EntityArgument.Info.serializeToNetwork`: один байт флагов,
            // бит 0 — одна цель, бит 1 — только игроки. Две строки здесь
            // сдвигали весь пакет, и клиент не мог его декодировать.
            let mut flags = 0u8;
            if *single {
                flags |= 1;
            }
            if *players_only {
                flags |= 2;
            }
            w.u8(flags);
        }
        Parser::ScoreHolder { multiple } => {
            w.u8(u8::from(*multiple));
        }
        Parser::Resource(registry)
        | Parser::ResourceKey(registry)
        | Parser::ResourceOrTag(registry)
        | Parser::ResourceOrTagKey(registry)
        | Parser::ResourceSelector(registry) => {
            w.string(registry);
        }
        Parser::Raw(_)
        | Parser::Angle
        | Parser::BlockPos
        | Parser::ColumnPos
        | Parser::BlockState
        | Parser::BlockPredicate
        | Parser::Vec2
        | Parser::Vec3
        | Parser::Rotation
        | Parser::Anchor
        | Parser::Gamemode
        | Parser::GameProfile
        | Parser::Component
        | Parser::Message
        | Parser::ItemStack
        | Parser::ItemPredicate
        | Parser::ItemSlot
        | Parser::ItemSlots
        | Parser::ResourceLocation
        | Parser::Objective
        | Parser::ObjectiveCriteria
        | Parser::Operation
        | Parser::Particle
        | Parser::ScoreboardSlot
        | Parser::Team
        | Parser::TeamColor
        | Parser::Heightmap
        | Parser::Dimension
        | Parser::Swizzle
        | Parser::TemplateMirror
        | Parser::TemplateRotation
        | Parser::Uuid
        | Parser::IntRange
        | Parser::FloatRange
        | Parser::Style
        | Parser::NbtCompoundTag
        | Parser::NbtTag
        | Parser::NbtPath
        | Parser::LootTable
        | Parser::LootPredicate
        | Parser::LootModifier
        | Parser::Feature
        | Parser::Function
        | Parser::Dialog
        | Parser::HexColor => {}
    }
}

/// `ClientboundCommandSuggestionsPacket`: заменяемый участок и варианты без подсказок.
pub fn command_suggestions(id: i32, tx: i32, start: i32, len: i32, matches: &[&str]) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id)
        .var_int(tx)
        .var_int(start)
        .var_int(len)
        .var_int(matches.len() as i32);
    for m in matches {
        w.string(m).bool(false);
    }
    w.buf
}

/// Команды плагинов: литерал → greedy-строка аргументов.
pub fn commands(id: i32, names: &[String], string_parser: i32) -> Vec<u8> {
    let roots: Vec<Node> = names
        .iter()
        .map(|n| {
            Node::literal(
                n,
                true,
                vec![Node::argument(
                    "args",
                    true,
                    Parser::String(StringMode::Greedy),
                    vec![],
                )],
            )
        })
        .collect();
    let mut parsers = CommandParsers::default();
    parsers.set("brigadier:string", string_parser);
    commands_tree(id, &roots, parsers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_to_nbt() {
        let p = system_chat_json(
            7,
            r#"{"color":"gold","text":"Hi","bold":true,"extra":["a",{"text":"b"}]}"#,
            false,
        );
        let mut want = vec![7, TAG_COMPOUND];
        let s = |o: &mut Vec<u8>, t: &str| {
            o.extend((t.len() as u16).to_be_bytes());
            o.extend(t.as_bytes());
        };
        // serde_json сортирует ключи.
        want.push(TAG_BYTE);
        s(&mut want, "bold");
        want.push(1);
        want.push(TAG_STRING);
        s(&mut want, "color");
        s(&mut want, "gold");
        want.push(TAG_LIST);
        s(&mut want, "extra");
        want.push(TAG_COMPOUND);
        want.extend(2i32.to_be_bytes());
        want.push(TAG_STRING);
        s(&mut want, "text");
        s(&mut want, "a");
        want.push(TAG_END);
        want.push(TAG_STRING);
        s(&mut want, "text");
        s(&mut want, "b");
        want.push(TAG_END);
        want.push(TAG_STRING);
        s(&mut want, "text");
        s(&mut want, "Hi");
        want.push(TAG_END);
        want.push(0); // overlay
        assert_eq!(p, want);
    }

    #[test]
    fn plain_string_component() {
        let p = system_chat_json(1, r#""hey""#, true);
        assert_eq!(p, [1, TAG_STRING, 0, 3, b'h', b'e', b'y', 1]);
    }

    #[test]
    fn command_tree() {
        let p = commands(0x10, &["hello".into()], 5);
        let mut want = vec![0x10, 3, NODE_ROOT, 1, 1];
        want.extend([NODE_LITERAL | NODE_EXECUTABLE, 1, 2, 5]);
        want.extend(b"hello");
        want.extend([NODE_ARGUMENT | NODE_EXECUTABLE, 0, 4]);
        want.extend(b"args");
        want.extend([5, 2, 0]);
        assert_eq!(p, want);
    }

    #[test]
    fn command_tree_breadth_first() {
        let roots = [Node::literal(
            "give",
            false,
            vec![Node::argument(
                "item",
                true,
                Parser::ItemStack,
                vec![Node::argument(
                    "count",
                    true,
                    Parser::Integer(1, 64),
                    vec![],
                )],
            )],
        )];
        let mut parsers = CommandParsers::default();
        for (name, id) in [
            ("brigadier:string", 5),
            ("brigadier:integer", 3),
            ("minecraft:item_stack", 7),
            ("minecraft:resource", 8),
        ] {
            parsers.set(name, id);
        }
        let p = commands_tree(9, &roots, parsers);
        let want: Vec<u8> = [
            vec![9, 4],
            vec![NODE_ROOT, 1, 1],
            vec![NODE_LITERAL, 1, 2, 4],
            b"give".to_vec(),
            vec![NODE_ARGUMENT | NODE_EXECUTABLE, 1, 3, 4],
            b"item".to_vec(),
            vec![7],
            vec![NODE_ARGUMENT | NODE_EXECUTABLE, 0, 5],
            b"count".to_vec(),
            vec![3, 3, 0, 0, 0, 1, 0, 0, 0, 64],
            vec![0],
        ]
        .concat();
        assert_eq!(p, want);
    }

    /// Каждый ванильный тип аргумента есть в таблице: ID не подменяется нулём.
    #[test]
    fn all_argument_types_have_ids() {
        let parsers = CommandParsers::load(|name| {
            Ok::<_, ()>(ARGUMENT_TYPES.iter().position(|n| *n == name).unwrap() as i32 + 1)
        })
        .unwrap();
        for (index, name) in ARGUMENT_TYPES.iter().enumerate() {
            assert_eq!(parsers.id(name), index as i32 + 1, "{name}");
        }
    }

    /// Селектор пишется по ванильному формату: `minecraft:entity` — один
    /// байт флагов. Раньше здесь были две строки («players», «multiple»),
    /// и клиент не мог декодировать весь пакет `commands`.
    #[test]
    fn entity_parser_writes_flag_byte() {
        let roots = [Node::literal(
            "kill",
            false,
            vec![Node::argument(
                "targets",
                true,
                Parser::Entity {
                    players_only: true,
                    single: true,
                },
                vec![],
            )],
        )];
        let mut parsers = CommandParsers::default();
        parsers.set("minecraft:entity", 20);
        let p = commands_tree(1, &roots, parsers);
        let want: Vec<u8> = [
            vec![1, 3],
            vec![NODE_ROOT, 1, 1],
            vec![NODE_LITERAL, 1, 2, 4],
            b"kill".to_vec(),
            vec![NODE_ARGUMENT | NODE_EXECUTABLE, 0, 7],
            b"targets".to_vec(),
            vec![20, 0b11],
            vec![0],
        ]
        .concat();
        assert_eq!(p, want);
    }

    /// Читает пакет так, как это делает клиент: любое расхождение в
    /// свойствах парсера сдвигает поток, и узлы перестают разбираться.
    mod decode {
        use super::*;

        /// Ожидаемые свойства парсера в формате сети.
        struct Reader<'a> {
            data: &'a [u8],
            at: usize,
        }

        impl<'a> Reader<'a> {
            fn new(data: &'a [u8]) -> Self {
                Self { data, at: 0 }
            }

            fn u8(&mut self) -> u8 {
                let v = self.data[self.at];
                self.at += 1;
                v
            }

            fn var_int(&mut self) -> i32 {
                let mut value = 0u32;
                for shift in 0..5 {
                    let byte = self.u8();
                    value |= u32::from(byte & 0x7f) << (7 * shift);
                    if byte & 0x80 == 0 {
                        break;
                    }
                }
                value as i32
            }

            fn string(&mut self) -> String {
                let len = self.var_int() as usize;
                let text = String::from_utf8(self.data[self.at..self.at + len].to_vec()).unwrap();
                self.at += len;
                text
            }

            fn f32(&mut self) -> f32 {
                let v = f32::from_be_bytes(self.data[self.at..self.at + 4].try_into().unwrap());
                self.at += 4;
                v
            }

            fn f64(&mut self) -> f64 {
                let v = f64::from_be_bytes(self.data[self.at..self.at + 8].try_into().unwrap());
                self.at += 8;
                v
            }

            fn i32(&mut self) -> i32 {
                let v = i32::from_be_bytes(self.data[self.at..self.at + 4].try_into().unwrap());
                self.at += 4;
                v
            }

            fn i64(&mut self) -> i64 {
                let v = i64::from_be_bytes(self.data[self.at..self.at + 8].try_into().unwrap());
                self.at += 8;
                v
            }
        }

        /// Свойства парсера по его имени — как их читает `ArgumentTypeInfo`.
        fn skip_properties(reader: &mut Reader<'_>, kind: &str, names: &[&str]) {
            let id = reader.var_int();
            let name = names
                .get(id as usize - 1)
                .copied()
                .unwrap_or_else(|| panic!("парсер #{id} вне таблицы"));
            assert_eq!(name, kind, "парсер #{id}");
            match kind {
                "brigadier:bool" => {}
                "brigadier:double" => {
                    let flags = reader.u8();
                    if flags & 1 != 0 {
                        reader.f64();
                    }
                    if flags & 2 != 0 {
                        reader.f64();
                    }
                }
                "brigadier:float" => {
                    let flags = reader.u8();
                    if flags & 1 != 0 {
                        reader.f32();
                    }
                    if flags & 2 != 0 {
                        reader.f32();
                    }
                }
                "brigadier:integer" => {
                    let flags = reader.u8();
                    if flags & 1 != 0 {
                        reader.i32();
                    }
                    if flags & 2 != 0 {
                        reader.i32();
                    }
                }
                "brigadier:long" => {
                    let flags = reader.u8();
                    if flags & 1 != 0 {
                        reader.i64();
                    }
                    if flags & 2 != 0 {
                        reader.i64();
                    }
                }
                "brigadier:string" => {
                    let _ = reader.var_int();
                }
                "minecraft:entity" => {
                    // Один байт: бит 0 — одна цель, бит 1 — только игроки.
                    let _ = reader.u8();
                }
                "minecraft:score_holder" => {
                    let _ = reader.u8();
                }
                "minecraft:time" => {
                    let _ = reader.i32();
                }
                "minecraft:resource"
                | "minecraft:resource_key"
                | "minecraft:resource_or_tag"
                | "minecraft:resource_or_tag_key"
                | "minecraft:resource_selector" => {
                    let _ = reader.string();
                }
                // У остальных типов свойств нет: только ID.
                _ => {}
            }
        }

        /// Полный разбор пакета `commands`; паникует на любом сдвиге.
        pub fn decode(data: &[u8], names: &[&str]) {
            let mut reader = Reader::new(data);
            let _packet_id = reader.var_int();
            let count = reader.var_int();
            assert!(count > 0, "пустой пакет");
            for _ in 0..count {
                let flags = reader.u8();
                let children = reader.var_int();
                for _ in 0..children {
                    let index = reader.var_int();
                    assert!(index < count, "ребёнок {index} вне пакета из {count} узлов");
                }
                // `redirect` читается до имени: индекс узла-цели.
                if flags & NODE_REDIRECT != 0 {
                    let index = reader.var_int();
                    assert!(
                        index < count,
                        "redirect {index} вне пакета из {count} узлов"
                    );
                }
                let node = flags & 0x03;
                if node == NODE_ROOT {
                    continue;
                }
                assert!(
                    node == NODE_LITERAL || node == NODE_ARGUMENT,
                    "неизвестный тип узла {node}"
                );
                let _name = reader.string();
                if node == NODE_ARGUMENT {
                    let kind = names
                        .get(reader.var_int() as usize - 1)
                        .copied()
                        .unwrap_or_else(|| panic!("парсер вне таблицы"));
                    // `skip_properties` перечитает ID; отдаём поток ему.
                    reader.at -= 1;
                    skip_properties(&mut reader, kind, names);
                    if flags & NODE_CUSTOM_SUGGESTIONS != 0 {
                        let _ = reader.string();
                    }
                }
            }
            assert_eq!(reader.var_int(), 0, "нет завершающего корня");
            assert_eq!(reader.at, data.len(), "лишние байты после пакета");
        }
    }

    /// Все парсеры, которые встречаются в нашем дереве, читаются клиентским
    /// разбором: это защита от сдвига свойств (как было с `entity`).
    #[test]
    fn every_parser_decodes_like_the_client() {
        let parsers = CommandParsers::load(|name| {
            Ok::<_, ()>(ARGUMENT_TYPES.iter().position(|n| *n == name).unwrap_or(0) as i32 + 1)
        })
        .unwrap();
        // По одному узлу на каждый тип парсера.
        let mut roots = Vec::new();
        for (index, name) in ARGUMENT_TYPES.iter().enumerate() {
            let parser = match *name {
                "brigadier:bool" => Parser::Bool,
                "brigadier:double" => Parser::Double(0.0, 1.0),
                "brigadier:float" => Parser::Float(0.0, 1.0),
                "brigadier:integer" => Parser::Integer(0, 10),
                "brigadier:long" => Parser::Long(0, 10),
                "brigadier:string" => Parser::String(StringMode::Greedy),
                "minecraft:entity" => Parser::Entity {
                    players_only: true,
                    single: false,
                },
                "minecraft:score_holder" => Parser::ScoreHolder { multiple: true },
                "minecraft:time" => Parser::Time(0),
                "minecraft:resource"
                | "minecraft:resource_key"
                | "minecraft:resource_or_tag"
                | "minecraft:resource_or_tag_key"
                | "minecraft:resource_selector" => Parser::Resource("minecraft:test"),
                other => Parser::Raw(Box::leak(other.to_string().into_boxed_str())),
            };
            roots.push(Node::literal(
                &format!("cmd{index}"),
                false,
                vec![Node::argument("arg", true, parser, vec![])],
            ));
        }
        let packet = commands_tree(0x42, &roots, parsers);
        decode::decode(&packet, ARGUMENT_TYPES);
    }

    #[test]
    fn console_line_uses_the_english_template() {
        let mut lang = std::collections::HashMap::new();
        lang.insert("particle.notFound".into(), "Unknown particle: %s".into());
        lang.insert(
            "commands.function.success.single.result".into(),
            "Function '%2$s' returned %1$s".into(),
        );
        let missing = serde_json::json!({
            "translate": "particle.notFound",
            "with": ["minecraft:nope"],
            "color": "red",
        });
        assert_eq!(
            console_line(&missing, &lang),
            "Unknown particle: minecraft:nope"
        );
        let indexed = serde_json::json!({
            "translate": "commands.function.success.single.result",
            "with": ["3", {"text": "foo"}],
        });
        assert_eq!(console_line(&indexed, &lang), "Function 'foo' returned 3");
        assert_eq!(apply_format("100%% done", &[]), "100% done");
        let bare = serde_json::json!({"translate": "missing.key", "with": ["a"]});
        assert_eq!(console_line(&bare, &lang), "missing.key: a");
    }
}
