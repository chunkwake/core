val paperApi: String by rootProject.extra

dependencies {
    "compileOnly"(paperApi)
}

tasks.named<Jar>("jar") {
    archiveFileName.set("chunkwake-perf-probe.jar")
}
