val paperApi: String by rootProject.extra

dependencies {
    "implementation"(paperApi)
}

tasks.named<Jar>("jar") {
    archiveFileName.set("chunkwake-api.jar")
}

// Готовый к запуску каталог: сервер берёт из него classpath JVM.
tasks.register<Sync>("dist") {
    from(tasks.named("jar"))
    from(configurations.named("runtimeClasspath")) {
        into("libs")
    }
    into(rootProject.layout.projectDirectory.dir("dist"))
}
