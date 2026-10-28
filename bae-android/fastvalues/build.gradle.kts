plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

// The one place the app reads the bridge's KeyedValueStream: its own module,
// so what it keeps `internal` the compiler keeps from the app.
fun toolchain(name: String): String = providers.gradleProperty("bae.$name").get()
val javaVersion = toolchain("javaVersion")

android {
    namespace = "fm.bae.fastvalues"
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
    api(project(":bridge"))
    implementation("androidx.annotation:annotation:1.9.1")
}
