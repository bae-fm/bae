package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeMediaCount
import uniffi.bae_bridge.BridgePressingFacts
import uniffi.bae_bridge.BridgeReleaseName
import uniffi.bae_bridge.BridgeTermLabel
import uniffi.bae_bridge.BridgeWorkReleaseSummary
import uniffi.bae_bridge.bridgeMediaTerms
import uniffi.bae_bridge.bridgePressingDetails
import uniffi.bae_bridge.bridgePressingSummary
import java.util.Locale

// What a pressing is, worded for the current locale. Core decides which parts
// a line has and in what order; this words each part and joins them, so every
// catalog's row reads the same shape: "Japan · 2×CD" over "Promo · Reissue".

/** Where the pressing was released and what it is made of. */
fun BridgePressingFacts.summaryText(context: Context): String = factLine(context, bridgePressingSummary(this))

/** Its status where that sets it apart, its packaging, and Discogs's details. */
fun BridgePressingFacts.detailsText(context: Context): String = factLine(context, bridgePressingDetails(this))

/** Media alone, each carrier with its count: "2×CD · DVD". */
fun List<BridgeMediaCount>.mediaText(context: Context): String = factLine(context, bridgeMediaTerms(this))

/** The parts, worded and joined with the catalog's list separator. */
fun factLine(
    context: Context,
    terms: List<BridgeFactTerm>,
): String = terms.joinToString(context.coreString("core.audio.list_separator")) { it.text(context) }

/** A country's name in the current locale, from its ISO 3166-1 code. */
fun countryName(
    context: Context,
    code: String,
): String =
    Locale
        .Builder()
        .setRegion(code)
        .build()
        .getDisplayCountry(context.currentLocale())
        .ifEmpty { code }

fun BridgeTermLabel.text(context: Context): String =
    when (this) {
        is BridgeTermLabel.Localized -> context.coreString(key)
        is BridgeTermLabel.Verbatim -> text
    }

fun BridgeFactTerm.text(context: Context): String =
    when (this) {
        is BridgeFactTerm.Country -> {
            countryName(context, code)
        }

        is BridgeFactTerm.Label -> {
            label.text(context)
        }

        is BridgeFactTerm.Counted -> {
            context.coreString(
                "core.pressing.media_count",
                mapOf("count" to count.toLong(), "medium" to label.text(context)),
            )
        }
    }

/** What a list of an album's releases calls this one. */
fun BridgeReleaseName.text(context: Context): String =
    when (this) {
        is BridgeReleaseName.Named -> {
            name
        }

        is BridgeReleaseName.Described -> {
            listOfNotNull(year?.toString(), media.mediaText(context).ifEmpty { null })
                .joinToString(" ")
        }

        is BridgeReleaseName.Numbered -> {
            context.coreString("core.release.numbered", mapOf("number" to number))
        }
    }

/**
 * The release's name, and its media where the name does not already say them:
 * a release named by its year and media is not followed by them a second time.
 */
fun BridgeWorkReleaseSummary.metadataText(context: Context): String =
    when (name) {
        is BridgeReleaseName.Described -> {
            name.text(context)
        }

        else -> {
            listOfNotNull(name.text(context), media.mediaText(context).ifEmpty { null })
                .joinToString(context.coreString("core.audio.list_separator"))
        }
    }
