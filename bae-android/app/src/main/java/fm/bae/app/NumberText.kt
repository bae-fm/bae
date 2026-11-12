package fm.bae.app

import android.content.Context
import android.icu.text.NumberFormat
import java.util.Locale

/** A whole number in [locale]'s digits; [grouping] false for a year, which has no thousands separator. */
internal fun integerText(
    value: Long,
    locale: Locale,
    grouping: Boolean = true,
): String =
    NumberFormat
        .getIntegerInstance(locale)
        .apply { isGroupingUsed = grouping }
        .format(value)

/** A count, such as a pin count or a page number, in the app's locale. */
fun Context.countText(value: Number): String = integerText(value.toLong(), currentLocale())

/** A year in the app's locale's digits, with no thousands separator. */
fun Context.yearText(year: Number): String = integerText(year.toLong(), currentLocale(), grouping = false)

/** A count held as an unsigned number, such as the pin count. */
fun Context.countText(value: UInt): String = integerText(value.toLong(), currentLocale())
