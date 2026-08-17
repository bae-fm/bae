package fm.bae.app

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class ConflatedProgressDeliveryTest {
    @Test
    fun blockedRendererReceivesTheNewestPendingProgress() =
        runBlocking {
            val firstStarted = CompletableDeferred<Unit>()
            val releaseFirst = CompletableDeferred<Unit>()
            val received = mutableListOf<Int>()
            val delivery =
                ConflatedProgressDelivery<Int>(this) { value ->
                    received += value
                    if (value == 1) {
                        firstStarted.complete(Unit)
                        releaseFirst.await()
                    }
                }

            delivery.offer(1)
            firstStarted.await()
            delivery.offer(2)
            delivery.offer(3)
            releaseFirst.complete(Unit)
            delivery.closeAndJoin()

            assertEquals(listOf(1, 3), received)
        }
}
