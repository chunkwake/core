//! Страницы дашборда: основная и форма входа.
//!
//! HTML вшит в бинарник: сервер должен отдавать дашборд без Node, npm и
//! внешних CDN — страница работает и на машине без интернета, и из
//! `target/release` рядом с `server.properties`. Страница самодостаточна:
//! стили и скрипт внутри одного файла, данные берёт из `/api/events`.

use std::sync::OnceLock;

/// Исходная страница из файла: версия в ней ещё плейсхолдером.
const WEB_INDEX: &str = include_str!("web/index.html");

/// Версия Chunkwake из сборки: `CARGO_PKG_VERSION` крейта `chunkwake-metrics`
/// совпадает с версией workspace, то есть с версией сервера.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Плейсхолдер в HTML, который заменяется на [`VERSION`].
const VERSION_PLACEHOLDER: &str = "{{CHUNKWAKE_VERSION}}";

/// Вся страница: разметка, стили и скрипт в одном файле, с подставленной
/// версией сборки. `include_str!` статичен, поэтому версию подставляем здесь.
static INDEX: OnceLock<String> = OnceLock::new();

fn index() -> &'static str {
    INDEX.get_or_init(|| {
        // `replacen` с одним вхождением: в подписи это первое и единственное
        // место, а если версия содержит `$`, обычный `replace` его съел бы.
        WEB_INDEX.replacen(VERSION_PLACEHOLDER, VERSION, 1)
    })
}

/// Страница как текст: её видят тесты и `index_raw`.
pub fn index_text() -> &'static str {
    index()
}

/// Форма входа: показывается, когда дашборд открыт из сети без токена.
pub const LOGIN: &str = include_str!("web/login.html");

/// Сжатая страница дашборда: 76 КБ исходящих превращаются в 22.
///
/// Сжимаем один раз и держим результат: страница неизменна на всё время
/// работы, а запросов к ней может быть много (перезагрузки, несколько
/// админов). `OnceLock` вместо мьютекса — читатели не ждут друг друга.
static INDEX_GZIP: OnceLock<Vec<u8>> = OnceLock::new();

/// Страница как есть, байтами.
pub fn index_raw() -> &'static [u8] {
    index().as_bytes()
}

/// Страница, сжатая gzip.
pub fn index_gzip() -> Vec<u8> {
    INDEX_GZIP.get_or_init(|| gzip(index_raw())).clone()
}

/// gzip через `flate2`: та же библиотека, что в протоколе и мире.
fn gzip(body: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    match encoder.write_all(body).and_then(|()| encoder.finish()) {
        Ok(packed) => packed,
        Err(_) => body.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Страница не должна подгружать внешние ресурсы: иначе она сломается
    /// без интернета и потянет чужой код в бинарник. Исключение — Google
    /// Fonts: при недоступности сети остаётся системный fallback.
    /// `xmlns` у SVG — пространство имён, а не запрос в сеть.
    #[test]
    fn page_is_self_contained() {
        assert!(index_text().contains("<title>Chunkwake"));
        assert!(index_text().contains("/api/events"));
        for loader in ["src=\"http", "@import", "url(http", "url('http"] {
            assert!(!index_text().contains(loader), "внешний ресурс: {loader}");
        }
        for href in index_text().split("href=\"").skip(1) {
            if href.starts_with("https://") {
                assert!(
                    href.starts_with("https://fonts.googleapis.com")
                        || href.starts_with("https://fonts.gstatic.com"),
                    "внешний href: {href}"
                );
            }
        }
        assert!(index_text().contains("\"Segoe UI\""));
        assert!(index_text().contains("\"Cascadia Code\""));
        // Иконки вшиты спрайтом, а не подключаются как шрифт или CDN.
        assert!(
            index_text().contains("<symbol id=\"i-"),
            "спрайт иконок на месте"
        );
    }

    /// Группы на странице и в крейте должны совпадать по ключам: иначе
    /// подписи разъедутся с тем, что считает Rust, и «итого» не сойдётся.
    #[test]
    fn groups_match_the_crate() {
        for group in &crate::profile::GROUPS {
            let needle = format!("key:\"{}\"", group.key);
            assert!(index_text().contains(&needle), "нет группы {needle}");
            assert!(
                index_text().contains(group.label),
                "нет подписи {}",
                group.key
            );
        }
        assert_eq!(
            index_text().matches("key:\"").count(),
            crate::profile::GROUP_COUNT,
            "лишние или пропущенные группы"
        );
    }

    /// Внутренние названия фаз не должны попадать в обычный режим: их
    /// место — только в подробном, иначе снова «плиты и рассылка блоков».
    #[test]
    fn technical_phases_stay_behind_the_toggle() {
        assert!(index_text().contains("data-mode=\"groups\""));
        assert!(index_text().contains("data-mode=\"phases\""));
        // Подписи фаз живут в массиве PHASES и подставляются по режиму.
        assert!(index_text().contains("var PHASES = ["));
        assert!(index_text().contains("profSeries"));
    }

    /// Сжатие окупается: иначе незачем возиться с `Vary` и кешем.
    #[test]
    fn page_compresses_well() {
        let packed = index_gzip();
        assert!(
            packed.len() * 2 < index_text().len(),
            "gzip {} из {} — мало выигрыша",
            packed.len(),
            index_text().len()
        );
        // Повторный вызов отдаёт кеш, а не сжимает заново.
        assert_eq!(packed, index_gzip());
    }

    /// Версия сборки подставлена в шапку, а плейсхолдер не остался.
    #[test]
    fn build_version_is_substituted() {
        let text = index_text();
        assert!(!text.contains(VERSION_PLACEHOLDER), "плейсхолдер остался");
        assert!(
            text.contains(&format!("Chunkwake v{VERSION}")),
            "нет версии {VERSION}"
        );
        // Версия совпадает с той, что видит сервер: оба крейта из workspace.
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    /// Форма входа тоже без внешних ресурсов: она отдаётся до авторизации,
    /// то есть в самой недоверенной ситуации.
    #[test]
    fn login_page_is_self_contained() {
        assert!(LOGIN.contains("metrics-token"));
        assert!(LOGIN.contains("<form"));
        assert!(!LOGIN.contains("src=\"http"));
        assert!(!LOGIN.contains("@import"));
    }
}
