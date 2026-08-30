package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeAudioFormat
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSourceAudioDescriptor
import uniffi.bae_bridge.BridgeSourceAudioLayout
import uniffi.bae_bridge.BridgeSourceAudioSummary
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
fun BridgeAudioFormat.text(context: Context): String = text(context, ::bridgeAudioChannelsKey)

internal fun BridgeAudioFormat.text(
    context: Context,
    audioChannelsKey: (Long) -> String?,
): String {
    val parts = mutableListOf(codec)
    if (bitsPerSample == null) {
        bitrateKbps?.let {
            parts.add(context.coreString("core.audio.bitrate_kbps", mapOf("value" to it)))
        }
    }
    parts.add(sampleRateText(context))
    bitsPerSample?.let {
        parts.add(context.coreString("core.audio.bit_depth", mapOf("value" to it)))
    }
    parts.add(channelsText(context, audioChannelsKey))
    return parts.joinToString(context.coreString("core.audio.list_separator"))
}

private fun BridgeAudioFormat.sampleRateText(context: Context): String {
    val khz = sampleRateHz / HZ_PER_KHZ
    val nf =
        NumberFormat.getNumberInstance(context.currentLocale()).apply {
            maximumFractionDigits = 1
            minimumFractionDigits = 0
        }
    return context.coreString(
        "core.audio.sample_rate_khz",
        mapOf("value" to nf.format(khz)),
    )
}

private fun BridgeAudioFormat.channelsText(
    context: Context,
    audioChannelsKey: (Long) -> String?,
): String {
    // 1 and 2 channels have a localized word (mono/stereo); any other count
    // has no special word and renders as "Nch" — this is the multichannel
    // case, not a missing catalog key.
    val key = audioChannelsKey(channels)
    return if (key != null) {
        context.coreString(key)
    } else {
        context.coreString("core.audio.channels.count", mapOf("value" to channels))
    }
}

fun BridgeSourceAudioDescriptor.text(context: Context): String = text(context, ::bridgeAudioChannelsKey)

internal fun BridgeSourceAudioDescriptor.text(
    context: Context,
    audioChannelsKey: (Long) -> String?,
): String {
    val parts = mutableListOf<String>()
    if (layout == BridgeSourceAudioLayout.CUE) {
        parts.add(context.coreString("core.audio.layout.cue"))
    }
    parts.add(format.text(context, audioChannelsKey))
    return parts.joinToString(context.coreString("core.audio.list_separator"))
}

fun BridgeSourceAudioSummary.text(context: Context): String = text(context, ::bridgeAudioChannelsKey)

internal fun BridgeSourceAudioSummary.text(
    context: Context,
    audioChannelsKey: (Long) -> String?,
): String =
    when (this) {
        is BridgeSourceAudioSummary.Uniform -> {
            descriptor.text(context, audioChannelsKey)
        }

        is BridgeSourceAudioSummary.Mixed -> {
            (
                listOf(context.coreString("core.audio.mixed")) +
                    descriptors.map { it.text(context, audioChannelsKey) }
            ).joinToString(context.coreString("core.audio.list_separator"))
        }
    }

fun BridgeRelease.compactMetadataText(context: Context): String = compactMetadataText(context, ::bridgeAudioChannelsKey)

internal fun BridgeRelease.compactMetadataText(
    context: Context,
    audioChannelsKey: (Long) -> String?,
): String =
    listOfNotNull(
        year?.toString(),
        format,
        label,
        catalogNumber,
        country,
        sourceAudio?.text(context, audioChannelsKey),
        context.durationUnitsText(totalDuration).ifEmpty { null },
    ).joinToString(context.coreString("core.audio.list_separator"))
