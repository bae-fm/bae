package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.bridgeAudioChannelsKey

// A release's three lines, worded for the current locale. Core carries the parts as terms and the
// release's year only when it differs from its album's; these words and joins them, so the lines
// read the same shape as the Mac's.

/**
 * The edition line: what is printed on the release. Its year when it differs from the album's,
 * each label with its catalog numbers, then its country.
 */
fun BridgeRelease.editionLineText(context: Context): String =
    listOf(
        distinctYear?.let(context::yearText),
        labelsLineText(context).ifEmpty { null },
        factLine(context, areaTerms).ifEmpty { null },
    ).filterNotNull().joinToString(context.factSeparator())

/** The copy line: the media and details, then the play time. */
fun BridgeRelease.copyLineText(context: Context): String =
    listOf(
        factLine(context, mediaTerms).ifEmpty { null },
        factLine(context, pressingDetails).ifEmpty { null },
        context.durationUnitsText(totalDuration).ifEmpty { null },
    ).filterNotNull().joinToString(context.factSeparator())

/** The audio line: the files' audio as bae read it, or an empty line when no file was read. */
fun BridgeRelease.audioLineText(
    context: Context,
    audioChannelsKey: (Long) -> String? = ::bridgeAudioChannelsKey,
): String = sourceAudio?.text(context, audioChannelsKey).orEmpty()

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
