plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

// The generated uniffi bindings, apart from the app so the fast-values module
// can compile against them; each edition compiles against its own.
fun toolchain(name: String): String = providers.gradleProperty("bae.$name").get()
val javaVersion = toolchain("javaVersion")

android {
    namespace = "fm.bae.bridge"
    compileSdk = toolchain("compileSdk").toInt()
    buildToolsVersion = toolchain("buildToolsVersion")

    defaultConfig {
        minSdk = toolchain("minSdk").toInt()
    }

    flavorDimensions += "edition"
    productFlavors {
        create("full") { dimension = "edition" }
        create("baeium") { dimension = "edition" }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.toVersion(javaVersion)
        targetCompatibility = JavaVersion.toVersion(javaVersion)
    }

    kotlinOptions {
        jvmTarget = javaVersion
    }

    sourceSets {
        // Each edition compiles against its own bindings, so a stray OAuth
        // reference fails baeium's build.
        getByName("full") {
            java.srcDir("../../bae-bridge/kotlin-bindings-full")
        }
        getByName("baeium") {
            java.srcDir("../../bae-bridge/kotlin-bindings-baeium")
        }
    }
}

dependencies {
    api("net.java.dev.jna:jna:5.15.0@aar")
    api("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.0")
}
