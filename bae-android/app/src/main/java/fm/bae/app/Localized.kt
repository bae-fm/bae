package fm.bae.app

import android.content.Context
import android.icu.text.MessageFormat
import android.text.format.DateUtils
import android.text.format.Formatter
import java.util.Locale

private const val MS_PER_SECOND = 1000L

// Renders bae's shared `core.*` catalog strings for the current locale.
//
// The locale never crosses the bridge: bae-core / bae-bridge emit raw numbers,
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

/** The configuration's primary locale, for number / duration / byte formatting. */
fun Context.currentLocale(): Locale = resources.configuration.locales[0]

/**
 * Format a millisecond duration as a locale-aware clock label (e.g. "3:07"),
 * or empty when absent or negative. Mirrors macOS's `DurationClock.text`.
 * [DateUtils.formatElapsedTime] renders "M:SS" / "H:MM:SS" using locale digits.
 */
fun formatDurationMs(ms: Long?): String {
    val msVal = ms?.takeIf { it >= 0 } ?: return ""
    return DateUtils.formatElapsedTime(msVal / MS_PER_SECOND)
}

/** Format the time remaining as "-M:SS", clamped at zero. */
fun formatRemainingMs(
    positionMs: Long,
    durationMs: Long,
): String {
    val remaining = (durationMs - positionMs).coerceAtLeast(0)
    return "-" + formatDurationMs(remaining)
}

/** Format a byte count for the current locale (e.g. "35 MB" / "35 Mo"). */
fun Context.formatFileSize(bytes: Long): String = Formatter.formatFileSize(this, bytes)
