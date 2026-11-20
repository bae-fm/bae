package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeSideHeading
import uniffi.bae_bridge.BridgeTrackSide

// Locale rendering of the structured `Bridge*` shapes bae-library emits. The core
// owns the structure (the side discriminant); these compose and format them for
// the current locale, resolving each translatable word from the shared catalog.
// Mirrors the macOS `Bridge*+*.swift` extensions.
// Source audio has its own file, `SourceAudioFormat.kt`.

/**
 * The localized heading of a side or disc ("Side A", "Disc 2"). Every screen,
 * prompt and error that names a side or disc words it here, with no call into
 * the bridge, so a view under a JVM test draws it. Mirrors macOS
 * `BridgeSideHeading.text`.
 */
fun BridgeSideHeading.text(context: Context): String =
    when (this) {
        is BridgeSideHeading.Sided -> context.coreString("core.track.side", mapOf("letter" to sideLetter))
        is BridgeSideHeading.Disc -> context.coreString("core.track.disc", mapOf("disc" to disc.toLong()))
    }

/** The localized heading of a side or disc, or empty for a flat side, which has no heading. */
fun BridgeTrackSide.headingText(context: Context): String =
    when (this) {
        is BridgeTrackSide.Sided -> BridgeSideHeading.Sided(sideLetter).text(context)
        is BridgeTrackSide.Disc -> BridgeSideHeading.Disc(disc).text(context)
        is BridgeTrackSide.Flat -> ""
    }
