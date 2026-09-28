package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.bridgeAudioChannelsKey

fun BridgeRelease.pressingLineText(context: Context): String = pressingLineText(context, ::bridgeAudioChannelsKey)

/**
 * The pressing on one line: year, where and what it is, the source audio and
 * the play time. The pressing lines come worded by core on the release itself;
 * the channel word comes in as a function, so the wording here is exercised
 * without the native bridge.
 */
internal fun BridgeRelease.pressingLineText(
    context: Context,
    audioChannelsKey: (Long) -> String?,
): String =
    listOf(
        year?.toString(),
        factLine(context, pressingSummary).ifEmpty { null },
        factLine(context, pressingDetails).ifEmpty { null },
        sourceAudio?.text(context, audioChannelsKey),
        context.durationUnitsText(totalDuration).ifEmpty { null },
    ).filterNotNull().joinToString(context.coreString("core.audio.list_separator"))

/**
 * The labels on one line, as core grouped them: each line's names, then the
 * catalog numbers they share.
 */
fun BridgeRelease.labelsLineText(context: Context): String = labelsLineText(context, labels)

internal fun labelsLineText(
    context: Context,
    lines: List<BridgeLabelLine>,
): String {
    val separator = context.coreString("core.audio.list_separator")
    val within = context.coreString("core.label.list_separator")
    return lines
        .map { line ->
            listOf(line.names.joinToString(within), line.catalogNumbers.joinToString(within))
                .filter { it.isNotEmpty() }
                .joinToString(separator)
        }.filter { it.isNotEmpty() }
        .joinToString(separator)
}
