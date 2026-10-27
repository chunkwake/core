val paperApi: String by rootProject.extra

dependencies {
    "compileOnly"(paperApi)
}

tasks.named<Jar>("jar") {
    archiveFileName.set("chunkwake-test-plugin.jar")
}

// Плагин рядом с API: `java/dist/plugins/`.
tasks.register<Copy>("dist") {
    from(tasks.named("jar"))
    into(rootProject.layout.projectDirectory.dir("dist/plugins"))
}
