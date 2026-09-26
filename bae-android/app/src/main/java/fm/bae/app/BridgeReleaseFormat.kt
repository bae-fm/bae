package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.bridgeAudioChannelsKey

fun BridgeRelease.compactMetadataText(context: Context): String = compactMetadataText(context, ::bridgeAudioChannelsKey)

/**
 * The release's facts on one line. The pressing lines come worded by core on
 * the release itself; the channel word comes in as a function, so the wording
 * here is exercised without the native bridge.
 */
internal fun BridgeRelease.compactMetadataText(
    context: Context,
    audioChannelsKey: (Long) -> String?,
): String =
    listOfNotNull(
        year?.toString(),
        factLine(context, pressingSummary).ifEmpty { null },
        label,
        catalogNumber,
        factLine(context, pressingDetails).ifEmpty { null },
        sourceAudio?.text(context, audioChannelsKey),
        context.durationUnitsText(totalDuration).ifEmpty { null },
    ).joinToString(context.coreString("core.audio.list_separator"))
