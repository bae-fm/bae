plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Release builds (CI) inject these; local dev builds fall back to dev markers.
// versionName is the app's own `0.N` release line, versionCode the monotonic
// store build number. BAE_GIT_COMMIT / BAE_COVEN_REV stamp the exact bae and
// coven (sync library) commits the binary carries, for crash triage and
// sync-compat debugging.
val baeVersionName = System.getenv("BAE_VERSION") ?: "0.0-dev"
val baeVersionCode = (System.getenv("BAE_VERSION_CODE") ?: "1").toInt()
val baeGitCommit = System.getenv("BAE_GIT_COMMIT") ?: "dev"
val baeCovenRev = System.getenv("BAE_COVEN_REV") ?: "dev"
val baeEnvironment = System.getenv("BAE_ENVIRONMENT") ?: ""
val baeDatadogSite = System.getenv("BAE_DATADOG_SITE") ?: ""
val baeDatadogClientToken = System.getenv("BAE_DATADOG_CLIENT_TOKEN") ?: ""
val releaseKeystore = System.getenv("ANDROID_KEYSTORE_FILE")

fun buildConfigString(value: String): String =
    "\"${value.replace("\\", "\\\\").replace("\"", "\\\"")}\""

android {
    namespace = "fm.bae.app"
    compileSdk = 35

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

        // The launcher label. The baeium flavor overrides it to "baeium" so the two
        // editions are distinguishable once both are installed.
        manifestPlaceholders["appLabel"] = "bae"

        // Package native libs for one ABI only when run.sh passes
        // -Pbae.abi=<abi> for the connected device. This filters every native
        // source — our libbae_bridge.so plus AAR libs like JNA's
        // libjnidispatch.so — so the APK carries no other-ABI dead weight. With no
        // property set (CI, Android Studio, release bundles) all ABIs are kept.
        (project.findProperty("bae.abi") as String?)?.let { requestedAbi ->
            ndk { abiFilters += requestedAbi }
        }
    }

    flavorDimensions += "edition"
    productFlavors {
        // full: the complete bae app. Its bridge bindings are built with the
        // oauth-providers feature, so the OAuth functions exist and the OAuth
        // sign-in flow (src/full) compiles. The redirect scheme for the
        // system-browser callback is read from the gitignored
        // src/full/assets/oauth-creds.json (the scheme of the first provider's
        // redirect_uri); absent → an inert placeholder, so the build works
        // without credentials. OAuthRedirectActivity binds it in the manifest.
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
        // baeium: S3-only. Its bridge bindings are built with no
        // features, so the OAuth functions are absent — src/full is excluded and
        // src/baeium supplies an always-null OAuthLinker. No OAuthRedirectActivity
        // and no scheme intent-filter ship (that manifest is src/full only), and
        // no credentials are read. The placeholder is set to the inert value so
        // the build never depends on an oauth-creds.json.
        create("baeium") {
            dimension = "edition"
            buildConfigField("String", "BAE_EDITION", "\"baeium\"")
            manifestPlaceholders["oauthRedirectScheme"] = "fm.bae.oauth.unconfigured"
            // baeium installs alongside the full bae build as a distinct app:
            // applicationId fm.bae.app.baeium (the namespace / R package stays
            // fm.bae.app), labelled "baeium" on the launcher.
            applicationIdSuffix = ".baeium"
            manifestPlaceholders["appLabel"] = "baeium"
        }
    }

    signingConfigs {
        // Only wire the release signing config when CI supplies the keystore;
        // local `assembleRelease` then produces an unsigned APK rather than
        // failing, and debug installs are unaffected.
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
        // The data-layer unit tests exercise code paths that call android.util.Log
        // (e.g. the dropped-release skip log). Return defaults instead of throwing
        // so the JVM tests don't need Robolectric just to no-op a log line.
        unitTests.isReturnDefaultValues = true
    }

    lint {
        abortOnError = true
        // Only errors fail the build; warnings are visible in the report but
        // don't block CI. Avoids noise from rules that don't apply (missing
        // translations, etc.).
        warningsAsErrors = false
    }

    sourceSets {
        // Each edition compiles against its own uniffi bindings: the full
        // bindings (built --features oauth-providers) carry the OAuth functions;
        // the baeium bindings (built with no features) lack them, so any stray
        // OAuth reference in shared code fails to compile. build-android.sh
        // writes each set under its own dir keyed by the feature set.
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
    implementation("androidx.camera:camera-camera2:1.4.2")
    implementation("androidx.camera:camera-lifecycle:1.4.2")
    implementation("androidx.camera:camera-view:1.4.2")
    implementation("com.google.zxing:core:3.5.3")
    implementation("androidx.media3:media3-session:1.7.1")
    implementation("io.coil-kt.coil3:coil-compose:3.0.4")
    implementation("sh.calvin.reorderable:reorderable:2.4.0")
    implementation("androidx.browser:browser:1.8.0")
    debugImplementation("androidx.compose.ui:ui-tooling")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.15.1")
}
