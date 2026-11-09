plugins {
    // Скачивает JDK 25 для компиляции, если локально его нет.
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

rootProject.name = "chunkwake-java"
include("api-impl", "test-plugin", "via")
