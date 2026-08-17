package fm.bae.app

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch

internal class ConflatedProgressDelivery<Value>(
    scope: CoroutineScope,
    apply: suspend (Value) -> Unit,
) {
    private val values = Channel<Value>(Channel.CONFLATED)
    private val delivery =
        scope.launch {
            for (value in values) {
                apply(value)
            }
        }

    fun offer(value: Value) {
        check(values.trySend(value).isSuccess) { "progress delivery is closed" }
    }

    fun close() {
        values.close()
    }

    suspend fun closeAndJoin() {
        close()
        delivery.join()
    }
}
