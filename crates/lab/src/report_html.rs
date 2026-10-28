//! Offline HTML presentation of the machine-readable lab report.

use super::Report;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const LOG_LIMIT: u64 = 5 * 1024 * 1024;
const TEMPLATE: &str = include_str!("report_template.html");

#[derive(Serialize)]
struct EmbeddedLog {
    text: String,
    truncated_bytes: u64,
    missing: bool,
}

fn embedded_log(path: &Path) -> EmbeddedLog {
    let Ok(mut file) = File::open(path) else {
        return EmbeddedLog {
            text: String::new(),
            truncated_bytes: 0,
            missing: true,
        };
    };
    let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
        return EmbeddedLog {
            text: String::new(),
            truncated_bytes: 0,
            missing: true,
        };
    };
    let offset = length.saturating_sub(LOG_LIMIT);
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return EmbeddedLog {
            text: String::new(),
            truncated_bytes: 0,
            missing: true,
        };
    }
    let mut bytes = Vec::with_capacity((length - offset) as usize);
    if file.read_to_end(&mut bytes).is_err() {
        return EmbeddedLog {
            text: String::new(),
            truncated_bytes: 0,
            missing: true,
        };
    }
    EmbeddedLog {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        truncated_bytes: offset,
        missing: false,
    }
}

fn script_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value)
        .map(|json| {
            json.replace('&', "\\u0026")
                .replace('<', "\\u003c")
                .replace('>', "\\u003e")
        })
        .map_err(|error| error.to_string())
}

pub(super) fn write(root: &Path, report: &Report) -> Result<(), String> {
    let logs = report
        .runs
        .iter()
        .map(|run| (run.log.clone(), embedded_log(&root.join(&run.log))))
        .collect::<BTreeMap<_, _>>();
    let html = TEMPLATE
        .replace("__REPORT_DATA__", &script_json(report)?)
        .replace("__LOG_DATA__", &script_json(&logs)?);
    fs::write(root.join("report.html"), html).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_cannot_close_script_or_inject_html() {
        let encoded = script_json(&"</script><img src=x onerror=alert(1)>&").unwrap();
        assert!(!encoded.contains("</script>"));
        assert!(!encoded.contains("<img"));
        assert!(encoded.contains("\\u003c"));
        assert!(encoded.contains("\\u0026"));
    }

    #[test]
    fn missing_log_is_reported_without_failing_report() {
        let log = embedded_log(Path::new("this-log-does-not-exist.chunkwake-test"));
        assert!(log.missing);
        assert!(log.text.is_empty());
    }
}
