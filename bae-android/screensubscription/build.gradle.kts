plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

// The one place the app pulls a value from a bridge subscription: its own
// module, so every pull loop lives here and nowhere in the app.
fun toolchain(name: String): String = providers.gradleProperty("bae.$name").get()
val javaVersion = toolchain("javaVersion")

android {
    namespace = "fm.bae.screensubscription"
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
}

dependencies {
    // A pull's failure is a BridgeException, and a cancelled pull ends it.
    api(project(":bridge"))
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.0")
}
