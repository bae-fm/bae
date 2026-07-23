plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.0.21" apply false
    // Renders @Preview composables to PNGs on the JVM (layoutlib, no emulator);
    // scripts/shots/android.sh drives it to capture the screenshot scenes.
    // alpha09 is the line that pairs with AGP 8.7.x (later alphas require AGP
    // 8.13+ and the @PreviewTest opt-in annotation).
    id("com.android.compose.screenshot") version "0.0.1-alpha09" apply false
}
