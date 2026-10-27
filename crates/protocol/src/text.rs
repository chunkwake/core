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
/// `StringArgumentType.greedyString()`.
const STRING_GREEDY: i32 = 2;

/// Пакет `commands`: корень → литерал каждой команды (исполняемый) → greedy-строка
/// аргументов. `string_parser` — ID `brigadier:string` в `command_argument_type`.
pub fn commands(id: i32, names: &[String], string_parser: i32) -> Vec<u8> {
    let mut w = Writer::new();
    w.var_int(id);
    // Узлы: 0 — корень, затем пары (литерал, аргумент).
    w.var_int(1 + 2 * names.len() as i32);
    w.u8(NODE_ROOT).var_int(names.len() as i32);
    for i in 0..names.len() {
        w.var_int(1 + 2 * i as i32);
    }
    for (i, n) in names.iter().enumerate() {
        w.u8(NODE_LITERAL | NODE_EXECUTABLE)
            .var_int(1)
            .var_int(2 + 2 * i as i32)
            .string(n);
        w.u8(NODE_ARGUMENT | NODE_EXECUTABLE)
            .var_int(0)
            .string("args")
            .var_int(string_parser)
            .var_int(STRING_GREEDY);
    }
    w.var_int(0);
    w.buf
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
}
