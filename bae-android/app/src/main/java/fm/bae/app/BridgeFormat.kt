package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeAudioFormat
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSide
import uniffi.bae_bridge.bridgeAudioChannelsKey
import java.text.NumberFormat

private const val HZ_PER_KHZ = 1000.0

// Locale rendering of the structured `Bridge*` shapes bae-core emits. The core
// owns the structure (the side discriminant, the audio parts); these compose
// and format them for the current locale, resolving any translatable word from
// the shared catalog via the key the core owns. Mirrors the macOS
// `Bridge*+*.swift` extensions.

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

/**
 * One-line audio descriptor for the current locale, e.g.
 * "FLAC · 44.1 kHz · 16-bit · stereo" (lossless) or
 * "MP3 · 320 kbps · 44.1 kHz · stereo" (lossy). The codec is a proper noun; the
 * channel word is localized; numbers use the locale formatter. bae-core owns the
 * parts and the lossy/lossless split (`bitsPerSample == null`); this is the UI's
 * locale rendering. Mirrors macOS `BridgeAudioFormat.text`.
 */
fun BridgeAudioFormat.text(context: Context): String {
    val nf = NumberFormat.getIntegerInstance(context.currentLocale())
    val parts = mutableListOf(codec)
    if (bitsPerSample == null) {
        bitrateKbps?.let { parts.add("${nf.format(it)} kbps") }
    }
    parts.add(sampleRateText(context))
    bitsPerSample?.let { parts.add("${nf.format(it)}-bit") }
    parts.add(channelsText(context, nf))
    return parts.joinToString(" · ")
}

private fun BridgeAudioFormat.sampleRateText(context: Context): String {
    val khz = sampleRateHz / HZ_PER_KHZ
    val nf =
        NumberFormat.getNumberInstance(context.currentLocale()).apply {
            maximumFractionDigits = 1
            minimumFractionDigits = 0
        }
    return "${nf.format(khz)} kHz"
}

private fun BridgeAudioFormat.channelsText(
    context: Context,
    nf: NumberFormat,
): String {
    // 1 and 2 channels have a localized word (mono/stereo); any other count
    // has no special word and renders as "Nch" — this is the multichannel
    // case, not a missing catalog key.
    val key = bridgeAudioChannelsKey(channels)
    return if (key != null) context.coreString(key) else "${nf.format(channels)}ch"
}
