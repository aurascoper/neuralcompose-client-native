// Compose shell for the NeuralCompose native client. Consumes the Rust core
// through the same generated UniFFI bindings the host-JVM harness tests
// (sourceSets includes ../core/src/main/kotlin). Native .so built by
// cargo-ndk into src/main/jniLibs (see android/README.md).
//
// Version matrix: AGP 8.13 requires Gradle 8.13–8.14 (NOT the system
// Gradle 9) — always build with ./gradlew (wrapper pinned to 8.14.3).

plugins {
    id("com.android.application") version "8.13.0"
    id("org.jetbrains.kotlin.android") version "2.2.0"
    id("org.jetbrains.kotlin.plugin.compose") version "2.2.0"
}

kotlin {
    jvmToolchain(21)
}

android {
    namespace = "org.neuralcompose.client"
    compileSdk = 35

    defaultConfig {
        applicationId = "org.neuralcompose.client"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    buildFeatures {
        compose = true
    }

    sourceSets["main"].kotlin.srcDir("../core/src/main/kotlin")
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2025.06.01")
    implementation(composeBom)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.1")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.1")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    // @aar is mandatory: it bundles Android libjnidispatch.so; the plain jar
    // (used by the host-JVM harness) fails at startup on-device.
    implementation("net.java.dev.jna:jna:5.17.0@aar")
}
