// Платформа ViaVersion для Chunkwake: `java/dist/via/chunkwake-via.jar`.
//
// Не лежит в classpath JVM: мост грузит её одним загрузчиком с jar-ами Via из
// `plugins/` (см. `ViaLoader`). Via здесь только для компиляции — версию
// выбирает пользователь, положив свои jar-ы.

val paperApi: String by rootProject.extra

repositories {
    maven("https://repo.viaversion.com")
}

dependencies {
    "compileOnly"(project(":api-impl"))
    "compileOnly"(paperApi)
    "compileOnly"("com.viaversion:viaversion-common:5.12.2")
    "compileOnly"("com.viaversion:viabackwards-common:5.12.2")
    "compileOnly"("com.viaversion:viarewind-common:4.2.0")
    val netty = "4.2.16.Final"
    "compileOnly"("io.netty:netty-buffer:$netty")
    "compileOnly"("io.netty:netty-common:$netty")
    "compileOnly"("io.netty:netty-transport:$netty")
    "compileOnly"("io.netty:netty-codec-base:$netty")
}

tasks.named<Jar>("jar") {
    archiveFileName.set("chunkwake-via.jar")
}

tasks.register<Copy>("dist") {
    from(tasks.named("jar"))
    into(rootProject.layout.projectDirectory.dir("dist/via"))
}
