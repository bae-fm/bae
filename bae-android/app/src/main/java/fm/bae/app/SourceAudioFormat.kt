package fm.bae.app

import android.content.Context
import android.icu.text.ListFormatter
import uniffi.bae_bridge.BridgeAudioFormat
import uniffi.bae_bridge.BridgeSourceAudioDescriptor
import uniffi.bae_bridge.BridgeSourceAudioDifference
import uniffi.bae_bridge.BridgeSourceAudioLayout
import uniffi.bae_bridge.BridgeSourceAudioSummary
import uniffi.bae_bridge.bridgeAudioChannelsKey
import java.text.NumberFormat

private const val HZ_PER_KHZ = 1000.0

// Locale rendering of a release's source audio: the format parts bae-core
// reads at scan time, composed and formatted for the current locale. Mirrors
// macOS `BridgeAudioFormat+Format.swift`.

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
    parts.add(sampleRateText(context, sampleRateHz))
    bitsPerSample?.let { parts.add(bitDepthText(context, it)) }
    parts.add(channelsText(context, channels, audioChannelsKey))
    return parts.joinToString(context.coreString("core.audio.list_separator"))
}

private fun bitDepthText(
    context: Context,
    bitsPerSample: Long,
): String = context.coreString("core.audio.bit_depth", mapOf("value" to bitsPerSample))

private fun sampleRateText(
    context: Context,
    sampleRateHz: Long,
): String {
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

private fun channelsText(
    context: Context,
    channels: Long,
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
            differences.joinToString(context.coreString("core.audio.list_separator")) {
                it.text(context, audioChannelsKey)
            }
        }
    }

/**
 * One fact a release's files disagree on: its values joined the way the
 * locale lists things ("FLAC and MP3"). Mirrors macOS
 * `BridgeSourceAudioDifference.text`.
 */
private fun BridgeSourceAudioDifference.text(
    context: Context,
    audioChannelsKey: (Long) -> String?,
): String {
    val values =
        when (this) {
            is BridgeSourceAudioDifference.Layout -> {
                layouts.map {
                    when (it) {
                        BridgeSourceAudioLayout.CUE -> context.coreString("core.audio.layout.cue")
                        BridgeSourceAudioLayout.FILE -> context.coreString("core.audio.layout.file")
                    }
                }
            }

            is BridgeSourceAudioDifference.Codec -> {
                codecs
            }

            is BridgeSourceAudioDifference.SampleRate -> {
                sampleRatesHz.map { sampleRateText(context, it) }
            }

            is BridgeSourceAudioDifference.BitDepth -> {
                bitsPerSample.map { bitDepthText(context, it) }
            }

            is BridgeSourceAudioDifference.Channels -> {
                channels.map { channelsText(context, it, audioChannelsKey) }
            }
        }
    return ListFormatter.getInstance(context.currentLocale()).format(values)
}
