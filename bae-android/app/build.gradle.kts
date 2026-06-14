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
val releaseKeystore = System.getenv("ANDROID_KEYSTORE_FILE")

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

        // OAuth redirect scheme for the system-browser callback, read from the
        // gitignored oauth-creds.json (the scheme of the first provider's
        // redirect_uri). Absent → an inert placeholder, so the build works
        // without credentials. OAuthRedirectActivity binds this in the manifest.
        val oauthCreds = file("src/main/assets/oauth-creds.json")
        val redirectScheme = if (oauthCreds.exists()) {
            Regex(""""redirect_uri"\s*:\s*"([^:"\s]+):""")
                .find(oauthCreds.readText())
                ?.groupValues
                ?.getOrNull(1)
        } else {
            null
        }
        manifestPlaceholders["oauthRedirectScheme"] =
            redirectScheme ?: "fm.bae.oauth.unconfigured"

        // Package native libs for one ABI only when run.sh passes
        // -Pbae.abi=<abi> for the connected device. This filters every native
        // source — our libbae_bridge.so plus AAR libs like ML Kit's
        // libbarhopper.so — so the APK carries no other-ABI dead weight. With no
        // property set (CI, Android Studio, release bundles) all ABIs are kept.
        (project.findProperty("bae.abi") as String?)?.let { requestedAbi ->
            ndk { abiFilters += requestedAbi }
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

    sourceSets {
        getByName("main") {
            java.srcDir("../../bae-bridge/kotlin-bindings")
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
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-process:2.8.7")
    implementation("net.java.dev.jna:jna:5.15.0@aar")
    implementation("androidx.camera:camera-camera2:1.4.2")
    implementation("androidx.camera:camera-lifecycle:1.4.2")
    implementation("androidx.camera:camera-view:1.4.2")
    implementation("com.google.mlkit:barcode-scanning:17.3.0")
    implementation("androidx.media3:media3-session:1.7.1")
    implementation("io.coil-kt.coil3:coil-compose:3.0.4")
    implementation("sh.calvin.reorderable:reorderable:2.4.0")
    implementation("androidx.browser:browser:1.8.0")
    debugImplementation("androidx.compose.ui:ui-tooling")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.15.1")
}
