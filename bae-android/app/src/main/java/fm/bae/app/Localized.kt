package fm.bae.app

import android.content.Context
import android.icu.text.DecimalFormatSymbols
import android.icu.text.MeasureFormat
import android.icu.text.MessageFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeDurationUnits
import uniffi.bae_bridge.BridgeLibrary
import java.util.Locale

private const val HOURS_KEY = "core.duration.hours"
private const val MINUTES_KEY = "core.duration.minutes"

// Renders bae's shared `core.*` catalog strings for the current locale.
//
// The locale never crosses the bridge: bae-library / bae-bridge emit raw numbers,
// typed enums, and stable `core.*` catalog keys; this resolves a key against the
// generated `core_strings.xml` (the loc-gen Android emit) and formats numbers
// with Android's locale formatters. Mirrors macOS's `NSLocalizedString(key,
// tableName: "Core")` rendering — the key comes from a `bridge_*_key()` uniffi
// function at runtime, so it's looked up by name.

/**
 * Resolve a `core.*` catalog key to its localized string. The dotted key maps to
 * the resource name loc-gen sanitizes it to (`.`/`-` → `_`); the key is chosen
 * at runtime by a `bridge_*_key()` function, so it's resolved by name rather
 * than a compile-time `R.string` reference (the Android equivalent of macOS's
 * dynamic `NSLocalizedString(key, ...)`).
 */
fun Context.coreString(key: String): String {
    val name = key.replace('.', '_').replace('-', '_')
    val id = resources.getIdentifier(name, "string", packageName)
    require(id != 0) { "no core_strings.xml entry for catalog key `$key` (resource `$name`)" }
    return getString(id)
}

/**
 * Resolve a `core.*` catalog key whose message takes ICU MessageFormat
 * arguments (named placeholders or a `plural`), formatting [args] for the
 * current locale. Uses the OS-bundled [MessageFormat] (minSdk 26), which parses
 * the MF1 string loc-gen stored verbatim — so plural category selection and
 * number formatting follow the resolved locale's CLDR rules.
 */
fun Context.coreString(
    key: String,
    args: Map<String, Any>,
): String = MessageFormat(coreString(key), currentLocale()).format(args)

/** Core's count parts, such as "2 uploading" and "3 queued", each localized and
 *  joined with the catalog's list separator. Core decides which parts appear and in what order. */
fun Context.countLabelsText(parts: List<BridgeCountLabel>): String = factsLine(parts.map { coreString(it) })

/** The name a library is listed under: its own, or for one whose name can't be read, a fixed title. */
@Composable
fun BridgeLibrary.title(): String = name ?: LocalContext.current.coreString("core.library.name_unreadable")

/** The configuration's primary locale, for number / duration / byte formatting. */
fun Context.currentLocale(): Locale = resources.configuration.locales[0]

/**
 * The clock label for a pre-computed clock (e.g. "3:07"). Core rendered the
 * clock's fields onto the row at conversion; this renders them for the current
 * locale.
 */
fun Context.durationClockLabel(clock: BridgeDurationClock): String = clockText(clock, currentLocale())

/**
 * A total playing time in words — "39 min", "3 hr", "3 hr, 42 min". Core decides
 * which words the label has (an hour of music is "1 hr", never "1 hr, 0 min");
 * the `core.duration.*` catalog messages own the words and the pattern that joins
 * them, so every platform says the same thing. The hours word is a plural —
 * Hebrew's dual "שעתיים" drops the numeral entirely — so the two halves are
 * formatted separately and then joined.
 */
fun Context.durationUnitsText(units: BridgeDurationUnits?): String =
    when (units) {
        null -> {
            ""
        }

        is BridgeDurationUnits.HoursOnly -> {
            durationWord(HOURS_KEY, "hours", units.hours)
        }

        is BridgeDurationUnits.MinutesOnly -> {
            durationWord(MINUTES_KEY, "minutes", units.minutes)
        }

        is BridgeDurationUnits.HoursAndMinutes -> {
            coreString(
                "core.duration.hours_minutes",
                mapOf(
                    "hours" to durationWord(HOURS_KEY, "hours", units.hours),
                    "minutes" to durationWord(MINUTES_KEY, "minutes", units.minutes),
                ),
            )
        }
    }

/** One `core.duration.*` unit word for a count — the count is the message's plural argument. */
private fun Context.durationWord(
    key: String,
    arg: String,
    count: ULong,
): String = coreString(key, mapOf(arg to count.toLong()))

/**
 * Render a clock's fields in the locale's own time format (ICU's numeric
 * duration pattern): its digits, its separator and field order, so an
 * Arabic-Indic locale reads "٣:٠٧" where "en" reads "3:07". A countdown takes
 * the locale's minus sign. The spread passes at most three measures to ICU's varargs entry point.
 */
@Suppress("SpreadOperator")
internal fun clockText(
    clock: BridgeDurationClock,
    locale: Locale,
): String {
    val parts =
        listOfNotNull(
            clock.hours?.let { Measure(it.toLong(), MeasureUnit.HOUR) },
            Measure(clock.minutes.toLong(), MeasureUnit.MINUTE),
            Measure(clock.seconds.toLong(), MeasureUnit.SECOND),
        )
    val format = MeasureFormat.getInstance(locale, MeasureFormat.FormatWidth.NUMERIC)
    val time = format.formatMeasures(*parts.toTypedArray())
    return if (clock.negative) DecimalFormatSymbols.getInstance(locale).minusSignString + time else time
}

fun ULong.requireDisplayableByteCount(): Long {
    require(this <= Long.MAX_VALUE.toULong()) { "byte count exceeds display range" }
    return toLong()
}
