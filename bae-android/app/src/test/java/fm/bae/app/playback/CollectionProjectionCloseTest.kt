package fm.bae.app.playback

import fm.bae.app.data.CollectionBrowseQuery
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeLiveQueryCause

class CollectionProjectionCloseTest {
    @Test
    fun closeStopsConsumerWhenNativeCancellationFails() =
        runBlocking {
            val query = FailingCancelQuery()
            val projection =
                CollectionProjection(
                    CoroutineScope(SupervisorJob() + Dispatchers.Default),
                    query,
                    CollectionSnapshotReader(
                        windows = { emptyMap() },
                        totalCount = { 0 },
                        cause = { BridgeLiveQueryCause.INITIAL },
                    ),
                    onChanged = {},
                    onError = {},
                )
            query.started.await()

            assertThrows(IllegalStateException::class.java) {
                runBlocking { projection.close() }
            }

            withTimeout(1_000) { query.stopped.await() }
        }

    private data object Snapshot

    private class FailingCancelQuery : CollectionBrowseQuery<Snapshot> {
        val started = CompletableDeferred<Unit>()
        val stopped = CompletableDeferred<Unit>()

        override fun setWindows(windows: List<BridgeLibraryPageWindow>) {}

        override suspend fun next(): Snapshot {
            started.complete(Unit)
            try {
                awaitCancellation()
            } finally {
                stopped.complete(Unit)
            }
        }

        override suspend fun cancel(): Unit = throw IllegalStateException("native cancellation failed")
    }
}
