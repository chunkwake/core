val paperApi: String by rootProject.extra

dependencies {
    "implementation"(paperApi)
    // Провайдер SLF4J 2.x. Без него SLF4J пишет в stderr три строки
    // «No SLF4J providers were found»: API плагинов (и библиотеки, которые
    // плагины объявляют в `libraries:`) логируют через SLF4J, а реализации
    // в classpath не было. Бэкенд — log4j-api, он уже едет с paper-api.
    "runtimeOnly"("org.apache.logging.log4j:log4j-slf4j2-impl:2.26.0")
}

tasks.named<Jar>("jar") {
    archiveFileName.set("chunkwake-api.jar")
}

// --- Генерация провайдеров ServiceLoader -----------------------------------
//
// Часть paper-api ищется через ServiceLoader, а сами интерфейсы объявлены
// package-private: реализовать их можно только классом в пакете paper-api.
// Пишем такие классы генератором, а не руками: их сотня методов, и они
// меняются с каждой версией paper-api. Источник правды — paper-api.jar,
// поэтому при обновлении версии всё перегенерируется само.

val toolsDir = rootProject.layout.projectDirectory.dir("tools")
val generatedDir = layout.buildDirectory.dir("generated/providers")
val toolsClasses = layout.buildDirectory.dir("generated/provider-tools")

/** Компиляция самого генератора: ему нужен paper-api в classpath для рефлексии. */
val compileProviderTools by tasks.registering(JavaCompile::class) {
    description = "Компилирует генератор провайдеров"
    source = fileTree(toolsDir) { include("GenProviders.java") }
    classpath = configurations.named("compileClasspath").get() + files(toolsDir)
    destinationDirectory.set(toolsClasses)
    options.encoding = "UTF-8"
    // Генератор разбирает paper-api рефлексией, сам он пишется на том же Java.
    options.release.set(21)
}

val generateProviders by tasks.registering(JavaExec::class) {
    group = "build"
    description = "Генерирует реализации ServiceLoader-провайдеров paper-api"
    dependsOn(compileProviderTools)
    classpath = files(toolsClasses, configurations.named("compileClasspath"))
    mainClass.set("GenProviders")
    argumentProviders.add(CommandLineArgumentProvider {
        listOf(
            configurations.named("compileClasspath").get()
                .first { it.name.startsWith("paper-api") }.absolutePath,
            generatedDir.get().dir("java").asFile.absolutePath,
            generatedDir.get().dir("resources").asFile.absolutePath,
        )
    })
    inputs.files(configurations.named("compileClasspath"))
    inputs.files(fileTree(toolsDir) { include("GenProviders.java") })
    outputs.dir(generatedDir)
}

sourceSets.named("main") {
    java.srcDir(generatedDir.map { it.dir("java") })
    resources.srcDir(generatedDir.map { it.dir("resources") })
}

tasks.named<JavaCompile>("compileJava") {
    dependsOn(generateProviders)
}

tasks.named<ProcessResources>("processResources") {
    dependsOn(generateProviders)
}

// Готовый к запуску каталог: сервер берёт из него classpath JVM.
tasks.register<Sync>("dist") {
    from(tasks.named("jar"))
    from(configurations.named("runtimeClasspath")) {
        into("libs")
    }
    into(rootProject.layout.projectDirectory.dir("dist"))
}
