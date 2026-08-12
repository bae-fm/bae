package fm.bae.app.playback

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking

internal data class NotificationRecipient<Owner>(
    val owner: Owner,
    val identity: Any,
)

internal class OwnerNotificationCoordinator<Owner>(
    private val lock: Any,
) {
    internal data class InFlight<Owner>(
        val recipient: NotificationRecipient<Owner>,
        val invokingThread: Thread,
        val completed: CompletableDeferred<Unit>,
    )

    internal data class Invocation<Owner>(
        val callback: () -> Unit,
        val inFlight: List<InFlight<Owner>>,
    )

    private val inFlight = mutableListOf<InFlight<Owner>>()

    fun begin(
        recipients: List<NotificationRecipient<Owner>>,
        callback: () -> Unit,
    ): Invocation<Owner> =
        synchronized(lock) {
            val completed = CompletableDeferred<Unit>()
            val accepted =
                recipients.map { recipient ->
                    InFlight(recipient, Thread.currentThread(), completed).also(inFlight::add)
                }
            Invocation(callback, accepted)
        }

    fun invoke(invocation: Invocation<Owner>) {
        try {
            invocation.callback()
        } finally {
            val completed =
                synchronized(lock) {
                    invocation.inFlight.forEach(inFlight::remove)
                    invocation.inFlight.firstOrNull()?.completed
                }
            completed?.complete(Unit)
        }
    }

    fun matching(
        owner: Owner,
        identity: Any,
    ): List<InFlight<Owner>> =
        synchronized(lock) {
            inFlight.filter { invocation ->
                invocation.recipient.owner == owner && invocation.recipient.identity === identity
            }
        }

    fun all(): List<InFlight<Owner>> = synchronized(lock) { inFlight.toList() }

    suspend fun await(invocations: List<InFlight<Owner>>) {
        invocations
            .filterNot { it.invokingThread === Thread.currentThread() }
            .map { it.completed }
            .distinct()
            .forEach { it.await() }
    }

    fun awaitBlocking(invocations: List<InFlight<Owner>>) {
        runBlocking { await(invocations) }
    }
}
