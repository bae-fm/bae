package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.bridgeAudioChannelsKey

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
