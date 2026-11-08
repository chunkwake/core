use std::path::PathBuf;
use std::process::Command;

const EXTRACTORS: &[&str] = &[
    "ChunkwakeLight",
    "ChunkwakeBiomes",
    "ChunkwakeWorldgen",
    "ChunkwakeShapes",
    "ChunkwakeInstruments",
    "ChunkwakeMapColors",
    "ChunkwakeEntities",
    "ChunkwakePathTypes",
];

fn main() {
    println!("cargo:rerun-if-env-changed=CHUNKWAKE_JAVAC");
    println!("cargo:rerun-if-env-changed=JAVA_HOME");

    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("extractor");
    std::fs::create_dir_all(&out).expect("create extractor output directory");

    let javac = std::env::var_os("CHUNKWAKE_JAVAC")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("JAVA_HOME").and_then(|home| {
                let path = PathBuf::from(home).join("bin").join(if cfg!(windows) {
                    "javac.exe"
                } else {
                    "javac"
                });
                path.is_file().then_some(path)
            })
        })
        .unwrap_or_else(|| PathBuf::from("javac"));

    let mut command = Command::new(&javac);
    command.arg("--release").arg("21").arg("-d").arg(&out);
    for name in EXTRACTORS {
        let source = manifest.join("extractor").join(format!("{name}.java"));
        println!("cargo:rerun-if-changed={}", source.display());
        command.arg(source);
    }
    let output = command.output().unwrap_or_else(|error| {
        panic!(
            "cannot start {}: {error}; install JDK 21+ or set CHUNKWAKE_JAVAC",
            javac.display()
        )
    });
    assert!(
        output.status.success(),
        "{} failed: {}",
        javac.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}
