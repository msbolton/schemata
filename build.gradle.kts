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
    arguments = arguments + listOf("-visitor", "-long-messages")
    outputDirectory = file("build/generated-src/antlr/main")
}

sourceSets {
    main {
        antlr.srcDir("src/main/antlr") // Default location for ANTLR grammar files
        java.srcDirs("build/generated-src/antlr/main") // Where ANTLR generates Java code
    }
}

tasks.compileKotlin {
    dependsOn(tasks.generateGrammarSource) // Ensure parser is generated before compiling Kotlin
}

tasks.compileJava {
    dependsOn(tasks.generateGrammarSource)
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



