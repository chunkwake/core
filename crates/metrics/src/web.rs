//! Страница дашборда метрик.
//!
//! HTML вшит в бинарник: сервер должен отдавать дашборд без Node, npm и
//! внешних CDN — страница работает и на машине без интернета, и из
//! `target/release` рядом с `server.properties`. Страница самодостаточна:
//! стили и скрипт внутри одного файла, данные берёт из `/api/events`.

/// Вся страница: разметка, стили и скрипт в одном файле.
pub const INDEX: &str = include_str!("web/index.html");

#[cfg(test)]
mod tests {
    use super::*;

    /// Страница не должна ссылаться на внешние ресурсы: иначе она сломается
    /// без интернета и потянет чужой код в бинарник.
    #[test]
    fn page_is_self_contained() {
        assert!(INDEX.contains("<title>Chunkwake"));
        assert!(INDEX.contains("/api/events"));
        assert!(!INDEX.contains("http://"), "нет внешних ссылок");
        assert!(!INDEX.contains("https://"), "нет внешних ссылок");
    }
}
