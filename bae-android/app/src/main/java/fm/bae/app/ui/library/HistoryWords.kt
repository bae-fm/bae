package fm.bae.app.ui.library

import android.content.Context
import fm.bae.app.R
import fm.bae.app.currentLocale
import fm.bae.app.data.ImageContent
import fm.bae.app.ui.components.AvatarFace
import uniffi.bae_bridge.BridgeDay
import uniffi.bae_bridge.BridgePersonRef
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Date
import java.util.TimeZone

// How History words a play's time and day, in the person's language.

/** The face of the person whose play it is. */
internal fun BridgePersonRef.face(): AvatarFace =
    AvatarFace(initials, avatarVersion?.let { ImageContent.Avatar(person = person, version = it) })

/** The time a play began, as the person's clock shows times. */
internal fun timeOf(
    context: Context,
    startedAt: Long,
    zone: TimeZone,
): String =
    android.text.format.DateFormat
        .getTimeFormat(context)
        .apply { timeZone = zone }
        .format(Date(startedAt))

/** A day's heading: Today, Yesterday, or the weekday and date. */
internal fun dayHeading(
    context: Context,
    day: BridgeDay,
    today: LocalDate,
): String {
    val date = LocalDate.of(day.year, day.month.toInt(), day.day.toInt())
    return when (date) {
        today -> context.getString(R.string.history_today)
        today.minusDays(1) -> context.getString(R.string.history_yesterday)
        else -> date.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.FULL).withLocale(context.currentLocale()))
    }
}
