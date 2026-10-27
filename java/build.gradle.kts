// Java-слой Chunkwake: реализация Bukkit/Paper API (`api-impl`) и тестовый плагин.
// Сборка: `./gradlew dist` → `java/dist/` (chunkwake-api.jar + libs/).

val paperApi by extra("io.papermc.paper:paper-api:26.3.build.209-stable")

subprojects {
    apply(plugin = "java")

    repositories {
        mavenCentral()
        maven("https://repo.papermc.io/repository/maven-public/")
    }

    extensions.configure<JavaPluginExtension> {
        toolchain.languageVersion.set(JavaLanguageVersion.of(25))
    }

    tasks.withType<JavaCompile>().configureEach {
        options.encoding = "UTF-8"
    }
}
