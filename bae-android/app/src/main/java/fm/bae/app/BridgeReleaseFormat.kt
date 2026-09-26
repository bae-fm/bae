package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgePressingFacts
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.bridgeAudioChannelsKey
import uniffi.bae_bridge.bridgePressingDetails
import uniffi.bae_bridge.bridgePressingSummary

fun BridgeRelease.compactMetadataText(context: Context): String =
    compactMetadataText(context, ::bridgeAudioChannelsKey, ::bridgePressingSummary, ::bridgePressingDetails)

/**
 * The release's facts on one line. The parts core decides — the channel word,
 * and which pressing facts a line has — come in as functions, so the wording
 * here is exercised without the native bridge.
 */
internal fun BridgeRelease.compactMetadataText(
    context: Context,
    audioChannelsKey: (Long) -> String?,
    pressingSummary: (BridgePressingFacts) -> List<BridgeFactTerm>,
    pressingDetails: (BridgePressingFacts) -> List<BridgeFactTerm>,
): String =
    listOfNotNull(
        year?.toString(),
        factLine(context, pressingSummary(facts)).ifEmpty { null },
        label,
        catalogNumber,
        factLine(context, pressingDetails(facts)).ifEmpty { null },
        sourceAudio?.text(context, audioChannelsKey),
        context.durationUnitsText(totalDuration).ifEmpty { null },
    ).joinToString(context.coreString("core.audio.list_separator"))
