package fm.bae.app

import java.lang.reflect.Proxy
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.AppHandleInterface

class SyncRecoveryTest {
    @Test
    fun failureRecoveryReconnectsTheConfiguredProvider() {
        var reconnectCalls = 0
        val handle =
            Proxy.newProxyInstance(
                AppHandleInterface::class.java.classLoader,
                arrayOf(AppHandleInterface::class.java),
            ) { _, method, _ ->
                if (method.name == "reconnectSync") {
                    reconnectCalls++
                    Unit
                } else {
                    error("unexpected AppHandle call: ${method.name}")
                }
            } as AppHandleInterface

        runBlocking { reconnectFailedSync(handle) }

        assertEquals(1, reconnectCalls)
    }
}
