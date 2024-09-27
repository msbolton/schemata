import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

plugins {
    id("org.jetbrains.kotlin.jvm") version "1.9.25"
    id("org.jetbrains.kotlin.kapt") version "1.9.25"
    id("org.jetbrains.kotlin.plugin.allopen") version "1.9.25"
    id("com.github.johnrengelman.shadow") version "8.1.1"
    id("io.micronaut.application") version "4.4.2"
    antlr
}

version = "0.1"
group = "com.github"

val kotlinVersion=project.properties.get("kotlinVersion")
repositories {
    mavenCentral()
}

dependencies {
    kapt("info.picocli:picocli-codegen")
    kapt("io.micronaut.serde:micronaut-serde-processor")
    antlr("org.antlr:antlr4:4.13.2")
    implementation("org.freemarker:freemarker:2.3.31")
    implementation("info.picocli:picocli")
    implementation("io.micronaut.kotlin:micronaut-kotlin-runtime")
    implementation("io.micronaut.picocli:micronaut-picocli")
    implementation("io.micronaut.serde:micronaut-serde-jackson")
    implementation("org.jetbrains.kotlin:kotlin-reflect:${kotlinVersion}")
    implementation("org.jetbrains.kotlin:kotlin-stdlib-jdk8:${kotlinVersion}")
    runtimeOnly("ch.qos.logback:logback-classic")
    runtimeOnly("com.fasterxml.jackson.module:jackson-module-kotlin")
}

tasks.generateGrammarSource {
    arguments = arguments + listOf("-visitor", "-long-messages", "-package", "com.github.msbolton.schemata.parsing")
    outputDirectory = file("build/generated-src/antlr/main/com/github/msbolton/schemata/parsing")
}

sourceSets {
    main {
        antlr.srcDir("src/main/antlr") // Default location for ANTLR grammar files
        kotlin.srcDirs("build/generated-src/antlr/main")
    }
}

tasks.withType<KotlinCompile>().configureEach {
    dependsOn(tasks.withType<AntlrTask>())
}

tasks.withType<Jar>().configureEach {
    dependsOn(tasks.withType<AntlrTask>())
}

application {
    mainClass = "com.github.msbolton.schemata.SchemataCommand"
}
java {
    sourceCompatibility = JavaVersion.toVersion("17")
}

kotlin {
    jvmToolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
}

micronaut {
    testRuntime("junit5")
    processing {
        incremental(true)
        annotations("com.github.*")
    }
}



