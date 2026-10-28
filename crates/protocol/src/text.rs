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
const NODE_CUSTOM_SUGGESTIONS: u8 = 0x10;
/// `StringArgumentType.greedyString()`.
const STRING_GREEDY: i32 = 2;

/// ID парсеров аргументов в реестре `command_argument_type`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandParsers {
    /// `brigadier:string`
    pub string: i32,
    /// `brigadier:integer`
    pub integer: i32,
    /// `minecraft:item_stack`: клиент сам подсказывает предметы из реестра.
    pub item_stack: i32,
}

/// Узел дерева Brigadier. `exec` — команду можно выполнить на этом узле.
#[derive(Clone, Debug)]
pub enum Node {
    Literal {
        name: String,
        exec: bool,
        children: Vec<Node>,
    },
    Argument {
        name: String,
        exec: bool,
        parser: Parser,
        children: Vec<Node>,
    },
}

/// Парсер аргумента со своими свойствами.
#[derive(Clone, Copy, Debug)]
pub enum Parser {
    GreedyString,
    /// Greedy-строка с подсказками от сервера (`minecraft:ask_server`).
    GreedyStringAsk,
    /// Целое с минимумом и максимумом.
    Integer(i32, i32),
    ItemStack,
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
            children,
        }
    }
    pub fn argument(name: &str, exec: bool, parser: Parser, children: Vec<Node>) -> Self {
        Node::Argument {
            name: name.into(),
            exec,
            parser,
            children,
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
    let mut w = Writer::new();
    w.var_int(id).var_int(flat.len() as i32);
    for (node, children) in &flat {
        let exec = |e: bool| if e { NODE_EXECUTABLE } else { 0 };
        match node {
            None => {
                w.u8(NODE_ROOT);
            }
            Some(Node::Literal { exec: e, .. }) => {
                w.u8(NODE_LITERAL | exec(*e));
            }
            Some(Node::Argument {
                exec: e, parser, ..
            }) => {
                let ask = if matches!(parser, Parser::GreedyStringAsk) {
                    NODE_CUSTOM_SUGGESTIONS
                } else {
                    0
                };
                w.u8(NODE_ARGUMENT | exec(*e) | ask);
            }
        }
        w.var_int(children.len() as i32);
        for &c in children {
            w.var_int(c);
        }
        match node {
            None => {}
            Some(Node::Literal { name, .. }) => {
                w.string(name);
            }
            Some(Node::Argument { name, parser, .. }) => {
                w.string(name);
                match *parser {
                    Parser::GreedyString => {
                        w.var_int(parsers.string).var_int(STRING_GREEDY);
                    }
                    Parser::GreedyStringAsk => {
                        w.var_int(parsers.string)
                            .var_int(STRING_GREEDY)
                            .string("minecraft:ask_server");
                    }
                    // Флаги: 1 — есть минимум, 2 — есть максимум.
                    Parser::Integer(min, max) => {
                        w.var_int(parsers.integer).u8(3).i32(min).i32(max);
                    }
                    Parser::ItemStack => {
                        w.var_int(parsers.item_stack);
                    }
                }
            }
        }
    }
    w.var_int(0);
    w.buf
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
                vec![Node::argument("args", true, Parser::GreedyString, vec![])],
            )
        })
        .collect();
    commands_tree(
        id,
        &roots,
        CommandParsers {
            string: string_parser,
            ..Default::default()
        },
    )
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
        let p = commands_tree(
            9,
            &roots,
            CommandParsers {
                string: 5,
                integer: 3,
                item_stack: 7,
            },
        );
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
}
