// Host-JVM harness for the UniFFI Kotlin bindings — proves the Kotlin FFI
// surface without any Android SDK. The Compose shell (deferred until the SDK
// is installed) will consume the same generated bindings with .so jniLibs
// built via cargo-ndk; see android/README.md.

plugins {
    kotlin("jvm") version "2.2.0"
}

kotlin {
    jvmToolchain(21)
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("net.java.dev.jna:jna:5.17.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    testImplementation(kotlin("test"))
}

tasks.test {
    useJUnitPlatform()
    // Host cdylib built by `cargo build --features uniffi` (scripts/gen-bindings.sh).
    systemProperty("jna.library.path", "${rootDir}/../../target/debug")
}
