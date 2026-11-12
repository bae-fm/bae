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

/** The non-empty [names] on one line, as the catalog separates a list of names ("Mara Quill, Low Tide Hours"). */
fun Context.namesLine(names: List<String?>): String =
    names.filter { !it.isNullOrEmpty() }.joinToString(coreString("core.label.list_separator"))

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
