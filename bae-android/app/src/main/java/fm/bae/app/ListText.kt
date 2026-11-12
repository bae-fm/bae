package fm.bae.app

import android.content.Context
import android.icu.text.ListFormatter
import java.util.Locale

/** The catalog's separator between facts on one line ("Japan · 2×CD"). */
fun Context.factSeparator(): String = coreString("core.audio.list_separator")

/** The non-empty [facts] on one line, in the catalog's separator. */
fun Context.factsLine(facts: List<String?>): String {
    val shown = facts.filter { !it.isNullOrEmpty() }
    return shown.joinToString(factSeparator())
}

/**
 * Items read aloud as a list in [locale] ("a, b, and c" in English, "a, b et c" in French), for the
 * description a screen reader gives a chart.
 */
fun spokenListText(
    items: List<String>,
    locale: Locale,
): String =
    ListFormatter
        .getInstance(locale, ListFormatter.Type.AND, ListFormatter.Width.WIDE)
        .format(items)
