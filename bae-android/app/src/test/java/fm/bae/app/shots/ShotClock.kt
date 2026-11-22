package fm.bae.app.shots

import java.time.Duration
import java.time.Instant

/**
 * The clock the shots read: a fixed moment, so a countdown or "2 hours ago" draws the same words whenever the shot
 * is taken. [settle] provides it to every shot as `LocalNowMs`, and a shot builds each moment it shows from it.
 */
internal object ShotClock {
    /** The moment every shot is drawn at. */
    val now: Instant = Instant.ofEpochSecond(1_800_000_000)

    /** [now] in Unix epoch milliseconds. */
    fun nowMs(): Long = now.toEpochMilli()

    /** The moment a countdown that has [secondsLeft] seconds left ends. */
    fun moment(secondsLeft: Long): Instant = now.plusSeconds(secondsLeft)

    /** The moment [ago] before [now]. */
    fun before(ago: Duration): Instant = now.minus(ago)
}
