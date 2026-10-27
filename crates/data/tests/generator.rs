//! Запуск настоящего генератора: нужны `CHUNKWAKE_TEST_JAR` и `CHUNKWAKE_JAVA`.

use chunkwake_data::generator::ensure_generated;
use chunkwake_data::packets::PacketTable;
use std::path::PathBuf;

#[test]
fn generates_and_caches_reports_if_available() {
    let (Ok(jar), Ok(java)) = (
        std::env::var("CHUNKWAKE_TEST_JAR"),
        std::env::var("CHUNKWAKE_JAVA"),
    ) else {
        return;
    };
    let cache: PathBuf =
        std::env::temp_dir().join(format!("chunkwake-datagen-{}", std::process::id()));
    let dir = ensure_generated(java.as_ref(), jar.as_ref(), "26.3", &cache).unwrap();
    let table = PacketTable::load(&dir.join("reports/packets.json")).unwrap();
    table.verify_hardcoded().unwrap();

    // Второй вызов берёт кэш и не запускает java (несуществующий путь не мешает).
    let again = ensure_generated("no-such-java".as_ref(), jar.as_ref(), "26.3", &cache).unwrap();
    assert_eq!(again, dir);
    std::fs::remove_dir_all(cache).unwrap();
}
