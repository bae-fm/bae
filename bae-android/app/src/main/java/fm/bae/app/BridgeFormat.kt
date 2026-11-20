package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeTrackSide
import uniffi.bae_bridge.bridgeTrackSideHeadingKey

// Locale rendering of the structured `Bridge*` shapes bae-library emits. The core
// owns the structure (the side discriminant); these compose and format them for
// the current locale, resolving any translatable word from the shared catalog
// via the key the core owns. Mirrors the macOS `Bridge*+*.swift` extensions.
// Source audio has its own file, `SourceAudioFormat.kt`.

/**
 * The localized heading of a side or disc ("Side A", "Disc 2"), or empty for a
 * flat side, which has no heading. Core names the catalog key; every screen,
 * prompt and error that names a side or disc words it here. Mirrors macOS
 * `BridgeTrackSide.headingText`.
 */
fun BridgeTrackSide.headingText(context: Context): String {
    val key = bridgeTrackSideHeadingKey(this) ?: return ""
    return when (this) {
        is BridgeTrackSide.Sided -> context.coreString(key, mapOf("letter" to sideLetter))
        is BridgeTrackSide.Disc -> context.coreString(key, mapOf("disc" to disc.toLong()))
        is BridgeTrackSide.Flat -> ""
    }
}
