package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSide

// Locale rendering of the structured `Bridge*` shapes bae-core emits. The core
// owns the structure (the side discriminant); these compose and format them for
// the current locale, resolving any translatable word from the shared catalog
// via the key the core owns. Mirrors the macOS `Bridge*+*.swift` extensions.
// Source audio has its own file, `SourceAudioFormat.kt`.

/**
 * The localized track-group header ("Side A" / "Disc 2"), or empty for the flat
 * single-disc case (no header). bae-core decides the case and the side letter /
 * disc number, and hands over the header word's catalog key on the group
 * (`headerKey`); this resolves the word and substitutes the letter / number.
 * Mirrors macOS `TrackGroup.sideHeaderText`.
 */
fun BridgeTrackGroup.sideHeaderText(context: Context): String {
    val key = headerKey ?: return ""
    return when (val s = side) {
        is BridgeTrackSide.Sided -> {
            context.coreString(key, mapOf("letter" to s.sideLetter))
        }

        is BridgeTrackSide.Disc -> {
            context.coreString(key, mapOf("disc" to s.disc))
        }

        // Unreachable: headerKey is null for Flat, so the elvis above already
        // returned. Kept for exhaustiveness.
        is BridgeTrackSide.Flat -> {
            ""
        }
    }
}
