package fm.bae.app

/** Format a millisecond duration as "M:SS" (e.g. "3:07"). Returns empty string for null. */
fun formatDurationMs(ms: Long?): String {
    val msVal = ms ?: return ""
    val totalSeconds = msVal / 1000
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}

/** Format the time remaining as "-M:SS" (e.g. "-3:07"). */
fun formatRemainingMs(positionMs: Long, durationMs: Long): String {
    val remaining = (durationMs - positionMs).coerceAtLeast(0)
    return "-" + formatDurationMs(remaining)
}
