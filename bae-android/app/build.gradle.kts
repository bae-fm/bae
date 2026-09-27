plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("com.android.compose.screenshot")
}

// CI injects these for release builds; local builds get dev markers.
val baeVersionName = System.getenv("BAE_VERSION") ?: "0.0-dev"
val baeVersionCode = (System.getenv("BAE_VERSION_CODE") ?: "1").toInt()
val baeGitCommit = System.getenv("BAE_GIT_COMMIT") ?: "dev"
val baeCovenRev = System.getenv("BAE_COVEN_REV") ?: "dev"
val baeEnvironment = System.getenv("BAE_ENVIRONMENT")
val baeDatadogSite = System.getenv("BAE_DATADOG_SITE")
val baeDatadogClientToken = System.getenv("BAE_DATADOG_CLIENT_TOKEN")
val baeSentryDsn = System.getenv("BAE_SENTRY_DSN")
val releaseKeystore = System.getenv("ANDROID_KEYSTORE_FILE")

fun buildConfigString(value: String?): String =
    value?.let { "\"${it.replace("\\", "\\\\").replace("\"", "\\\"")}\"" } ?: "null"

android {
    namespace = "fm.bae.app"
    compileSdk = 35

    // The screenshotTest source set, which scripts/shots/android.sh renders.
    experimentalProperties["android.experimental.enableScreenshotTest"] = true

    defaultConfig {
        applicationId = "fm.bae.app"
        minSdk = 26
        targetSdk = 35
        versionCode = baeVersionCode
        versionName = baeVersionName

        buildConfigField("String", "BAE_GIT_COMMIT", "\"$baeGitCommit\"")
        buildConfigField("String", "BAE_COVEN_REV", "\"$baeCovenRev\"")
        buildConfigField("String", "BAE_ENVIRONMENT", buildConfigString(baeEnvironment))
        buildConfigField("String", "BAE_DATADOG_SITE", buildConfigString(baeDatadogSite))
        buildConfigField("String", "BAE_DATADOG_CLIENT_TOKEN", buildConfigString(baeDatadogClientToken))
        buildConfigField("String", "BAE_SENTRY_DSN", buildConfigString(baeSentryDsn))

        // The launcher label; baeium overrides it.
        manifestPlaceholders["appLabel"] = "bae"

        // The applicationId res/xml/shortcuts.xml targets, as a string resource since
        // res/xml can't read the manifest placeholder.
        resValue("string", "shortcut_target_package", "fm.bae.app")

        // run.sh passes -Pbae.abi to package only the connected device's ABI;
        // otherwise every ABI is kept.
        (project.findProperty("bae.abi") as String?)?.let { requestedAbi ->
            ndk { abiFilters += requestedAbi }
        }
    }

    flavorDimensions += "edition"
    productFlavors {
        // The complete app. Its OAuth redirect scheme is read from the gitignored
        // src/full/assets/oauth-creds.json, with an inert placeholder when it's absent.
        create("full") {
            dimension = "edition"
            buildConfigField("String", "BAE_EDITION", "\"bae\"")
            val oauthCreds = file("src/full/assets/oauth-creds.json")
            val redirectScheme =
                if (oauthCreds.exists()) {
                    Regex(""""redirect_uri"\s*:\s*"([^:"\s]+):""")
                        .find(oauthCreds.readText())
                        ?.groupValues
                        ?.getOrNull(1)
                } else {
                    null
                }
            manifestPlaceholders["oauthRedirectScheme"] =
                redirectScheme ?: "fm.bae.oauth.unconfigured"
        }
        // S3-only: its bindings lack the OAuth functions, src/baeium supplies a null
        // OAuthLinker, and no credentials are read.
        create("baeium") {
            dimension = "edition"
            buildConfigField("String", "BAE_EDITION", "\"baeium\"")
            manifestPlaceholders["oauthRedirectScheme"] = "fm.bae.oauth.unconfigured"
            // Installs beside the full build as its own app.
            applicationIdSuffix = ".baeium"
            manifestPlaceholders["appLabel"] = "baeium"
            resValue("string", "shortcut_target_package", "fm.bae.app.baeium")
        }
    }

    signingConfigs {
        // Signs a release only when CI supplies the keystore.
        if (releaseKeystore != null) {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (releaseKeystore != null) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    testOptions {
        // android.util.Log returns defaults so JVM tests don't need Robolectric.
        unitTests.isReturnDefaultValues = true
        // Robolectric needs the merged manifest and resources;
        // src/test/resources/robolectric.properties replaces the Application.
        unitTests.isIncludeAndroidResources = true
    }

    lint {
        abortOnError = true
        // Only errors fail the build.
        warningsAsErrors = false
    }

    sourceSets {
        // The theme build-android.sh generates from design/theme.toml.
        getByName("main") {
            java.srcDir("generated/theme/kotlin")
            res.srcDir("generated/theme/res")
        }
        // Each edition compiles against its own bindings, so a stray OAuth reference
        // fails baeium's build.
        getByName("full") {
            java.srcDir("../../bae-bridge/kotlin-bindings-full")
        }
        getByName("baeium") {
            java.srcDir("../../bae-bridge/kotlin-bindings-baeium")
        }
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2025.01.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.core:core-ktx:1.16.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-process:2.8.7")
    implementation("net.java.dev.jna:jna:5.15.0@aar")
    // The JVM half of the Rust TLS verifier, located through Cargo metadata by
    // settings.gradle.
    implementation(nativeDeps.rustls.platform.verifier)
    implementation("androidx.camera:camera-camera2:1.4.2")
    implementation("androidx.camera:camera-lifecycle:1.4.2")
    implementation("androidx.camera:camera-view:1.4.2")
    implementation("com.google.zxing:core:3.5.3")
    implementation("androidx.media3:media3-session:1.7.1")
    // The now-playing home-screen widget.
    implementation("androidx.glance:glance-appwidget:1.1.1")
    implementation("androidx.glance:glance-material3:1.1.1")
    // Reads the orientation tag of cover and booklet photos.
    implementation("androidx.exifinterface:exifinterface:1.4.2")
    implementation("sh.calvin.reorderable:reorderable:2.4.0")
    implementation("androidx.browser:browser:1.8.0")
    implementation("io.sentry:sentry-android-ndk:8.16.0")
    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.15.1")
    testImplementation("androidx.compose.ui:ui-test-junit4")
    // The screenshotTest source set renders previews with the tooling.
    screenshotTestImplementation(composeBom)
    screenshotTestImplementation("androidx.compose.ui:ui-tooling")
}
