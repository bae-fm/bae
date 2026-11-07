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

    testOptions {
        // Robolectric reads the merged manifest and resources.
        unitTests.isIncludeAndroidResources = true
    }
}

dependencies {
    api(project(":bridge"))
    // A fed stream's key changes are pulled like every other screen value.
    implementation(project(":screensubscription"))
    implementation("androidx.annotation:annotation:1.9.1")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.0")
    testImplementation("org.robolectric:robolectric:4.15.1")
}
