package fm.bae.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import kotlinx.coroutines.delay

private const val MILLIS_PER_SECOND = 1_000L

/**
 * Whole seconds from [nowMs] until [deadlineMs], rounded up so a countdown never
 * reads 0 before its deadline, and never below 0.
 */
internal fun countdownSecondsLeft(
    deadlineMs: Long,
    nowMs: Long,
): Long = (maxOf(0L, deadlineMs - nowMs) + MILLIS_PER_SECOND - 1) / MILLIS_PER_SECOND

/**
 * The time to count down from, read from [nowMs] again as each whole second
 * before [deadlineMs] passes, until the deadline.
 */
@Composable
internal fun rememberCountdownNow(
    deadlineMs: Long,
    nowMs: () -> Long,
): Long {
    var now by remember(deadlineMs) { mutableLongStateOf(nowMs()) }
    LaunchedEffect(deadlineMs) {
        while (now < deadlineMs) {
            // Wake as the next whole second before the deadline passes.
            val untilNextSecond = (deadlineMs - now) % MILLIS_PER_SECOND
            delay(if (untilNextSecond == 0L) MILLIS_PER_SECOND else untilNextSecond)
            now = nowMs()
        }
    }
    return now
}
