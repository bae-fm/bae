package fm.bae.app.data

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgePictureTurn

/** What the decode queue promises: the highest turn starts first, and no place is lost to a request that leaves. */
class DecodeQueueTest {
    @Test
    fun aWaitingDecodeOfAHigherTurnStartsFirst() =
        runBlocking {
            val queue = DecodeQueue(1)
            val order = mutableListOf<String>()
            val finishBlocker = CompletableDeferred<Unit>()
            val chosen = ImageTurn(BridgePictureTurn.PREFETCH)

            // Each request runs on this thread until it waits, so they join the line in this order.
            fun request(
                name: String,
                turn: ImageTurn,
                work: suspend () -> Unit = {},
            ) = launch(Dispatchers.Unconfined, start = CoroutineStart.UNDISPATCHED) {
                queue.run({ turn.asked }) {
                    order += name
                    work()
                }
            }
            val requests =
                listOf(
                    request("blocker", ImageTurn()) { finishBlocker.await() },
                    request("prefetch", ImageTurn(BridgePictureTurn.PREFETCH)),
                    request("visible", ImageTurn(BridgePictureTurn.VISIBLE)),
                    request("chosen", chosen),
                )
            // The person looks at the picture of the last request now.
            chosen.asked = BridgePictureTurn.FOCUSED

            finishBlocker.complete(Unit)
            requests.joinAll()

            assertEquals(listOf("blocker", "chosen", "visible", "prefetch"), order)
        }

    @Test
    fun aRequestThatLeavesTheLineTakesNoPlace() =
        runBlocking {
            val queue = DecodeQueue(1)
            val order = mutableListOf<String>()
            val finishBlocker = CompletableDeferred<Unit>()

            fun request(
                name: String,
                work: suspend () -> Unit = {},
            ) = launch(Dispatchers.Unconfined, start = CoroutineStart.UNDISPATCHED) {
                queue.run({ BridgePictureTurn.VISIBLE }) {
                    order += name
                    work()
                }
            }
            val blocker = request("blocker") { finishBlocker.await() }
            val leaving = request("leaving")
            val staying = request("staying")

            leaving.cancel()
            finishBlocker.complete(Unit)
            joinAll(blocker, leaving, staying)

            assertEquals(listOf("blocker", "staying"), order)
        }
}
