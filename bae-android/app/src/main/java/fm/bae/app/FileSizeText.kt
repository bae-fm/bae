package fm.bae.app

import android.content.Context
import java.text.NumberFormat
import java.util.Locale

/** The names of the units above bytes, smallest first; bytes have a plural of their own. */
private val LARGER_UNITS =
    listOf(
        R.string.file_size_kilobytes,
        R.string.file_size_megabytes,
        R.string.file_size_gigabytes,
        R.string.file_size_terabytes,
    )
private const val UNIT_STEP = 1000L
private const val TENTHS = 10L
private const val TENTHS_PER_THOUSAND = UNIT_STEP * TENTHS

/**
 * A byte count as the person's language prints it: "3.9 GB", "420 MB", "1 GB",
 * and "3,9 Go" in French.
 */
fun Context.formatFileSize(bytes: Long): String {
    val size = fileSizeParts(bytes, currentLocale())
    return if (size.unit == 0) {
        resources.getQuantityString(R.plurals.file_size_bytes, bytes.toInt(), size.number)
    } else {
        getString(LARGER_UNITS[size.unit - 1], size.number)
    }
}

internal class FileSizeParts(
    val number: String,
    val unit: Int,
)

/**
 * A byte count in decimal units (1 KB is 1000 bytes), the largest unit that
 * keeps the number at 1 or more, with at most one decimal and none when the
 * rounded number is whole. The number's separator and digits come from [locale]; the
 * unit is 0 for bytes and counts up from there. A value
 * that rounds up to 1000 moves to the next unit, so 999,960 bytes is "1 MB".
 */
internal fun fileSizeParts(
    bytes: Long,
    locale: Locale,
): FileSizeParts {
    require(bytes >= 0) { "a byte count is not negative" }
    val tenths = Math.multiplyExact(bytes, TENTHS)
    var unit = 0
    var divisor = 1L
    while (unit < LARGER_UNITS.size && (tenths + divisor / 2) / divisor >= TENTHS_PER_THOUSAND) {
        divisor *= UNIT_STEP
        unit += 1
    }
    val rounded = (tenths + divisor / 2) / divisor
    val format = NumberFormat.getNumberInstance(locale)
    format.minimumFractionDigits = 0
    format.maximumFractionDigits = 1
    return FileSizeParts(format.format(rounded / TENTHS.toDouble()), unit)
}
