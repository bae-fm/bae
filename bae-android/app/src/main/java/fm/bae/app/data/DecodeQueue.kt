package fm.bae.app.data

import kotlinx.coroutines.CompletableDeferred
import uniffi.bae_bridge.BridgePictureTurn
import kotlin.coroutines.cancellation.CancellationException

/**
 * Runs at most [width] decodes at once. Among the decodes that wait, the one of the highest turn starts first, and
 * the oldest among equals. A request's turn is read each time a place is free, so it may change while it waits.
 */
internal class DecodeQueue(
    private val width: Int,
) {
    private class Waiting(
        val turn: () -> BridgePictureTurn,
    ) {
        val place = CompletableDeferred<Unit>()
    }

    private val lock = Any()

    /** Oldest first. Guarded by [lock]. */
    private val waiting = mutableListOf<Waiting>()

    /** The places taken. Guarded by [lock]. */
    private var running = 0

    /** What [work] returns, run once it has one of the queue's places. */
    suspend fun <T> run(
        turn: () -> BridgePictureTurn,
        work: suspend () -> T,
    ): T {
        enter(turn)
        try {
            return work()
        } finally {
            synchronized(lock) { passPlaceOn() }
        }
    }

    /** Returns once the request has a place. */
    private suspend fun enter(turn: () -> BridgePictureTurn) {
        val request =
            synchronized(lock) {
                if (running < width) {
                    running++
                    null
                } else {
                    Waiting(turn).also(waiting::add)
                }
            } ?: return
        try {
            request.place.await()
        } catch (e: CancellationException) {
            // A request no longer in line was given a place, which it passes on.
            synchronized(lock) { if (!waiting.remove(request)) passPlaceOn() }
            throw e
        }
    }

    /** Gives a place to the next request in line, or back to the queue. Called under [lock]. */
    private fun passPlaceOn() {
        // `maxByOrNull` takes the first of equals, which is the oldest.
        val next = waiting.maxByOrNull { it.turn() }
        if (next == null) {
            running--
        } else {
            waiting.remove(next)
            next.place.complete(Unit)
        }
    }
}
