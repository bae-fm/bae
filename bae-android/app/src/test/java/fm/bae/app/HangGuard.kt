package fm.bae.app

import kotlin.time.Duration.Companion.seconds

/**
 * How long a test waits for something it knows is coming before it calls the
 * hang a failure. It bounds a hang and paces nothing: a wait ends on its own
 * event, as soon as the event comes.
 */
val HANG_GUARD = 300.seconds

/** [HANG_GUARD] for the waits that take milliseconds. */
val HANG_GUARD_MS = HANG_GUARD.inWholeMilliseconds
