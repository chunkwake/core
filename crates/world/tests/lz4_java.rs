//! Фикстуры настоящего LZ4BlockOutputStream проходят через чтение .mca и .mcc.
use chunkwake_world::{anvil::RegionStore, nbt};
use std::fs;

#[test]
fn java_lz4_region_and_external_chunk() {
    let packed = include_bytes!("fixtures/lz4/mixed.lz4");
    let expected = nbt::read_root(include_bytes!("fixtures/lz4/mixed.nbt")).unwrap();
    for external in [false, true] {
        let dir = std::env::temp_dir().join(format!(
            "chunkwake-lz4-java-{}-{external}",
            std::process::id()
        ));
        fs::create_dir(&dir).unwrap();
        let mut payload = vec![4 | if external { 128 } else { 0 }];
        if external {
            fs::write(dir.join("c.0.0.mcc"), packed).unwrap();
        } else {
            payload.extend_from_slice(packed);
        }
        let sectors = (4 + payload.len()).div_ceil(4096);
        let mut region = vec![0; (2 + sectors) * 4096];
        region[..4].copy_from_slice(&(0x200u32 | sectors as u32).to_be_bytes());
        region[8192..8196].copy_from_slice(&(payload.len() as u32).to_be_bytes());
        region[8196..8196 + payload.len()].copy_from_slice(&payload);
        let path = dir.join("r.0.0.mca");
        fs::write(&path, &region).unwrap();
        let store = RegionStore::new(&dir);
        assert_eq!(
            store.load_region(0, 0).unwrap(),
            vec![((0, 0), expected.clone())]
        );

        // Неверная checksum терминатора должна дойти до вызывающего кода
        // как ошибка с именем региона и индексом чанка.
        if external {
            let mut corrupt = packed.to_vec();
            *corrupt.last_mut().unwrap() = 1;
            fs::write(dir.join("c.0.0.mcc"), corrupt).unwrap();
        } else {
            region[8196 + payload.len() - 1] = 1;
            fs::write(&path, &region).unwrap();
        }
        let error = store.load_region(0, 0).unwrap_err();
        assert!(
            error.contains("r.0.0.mca: chunk 0: lz4: bad final checksum"),
            "{error}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
