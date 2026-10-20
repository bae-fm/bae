package fm.bae.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowNetwork
import org.robolectric.shadows.ShadowNetworkCapabilities

/**
 * Android blocks bae's network while the phone is locked. The reporter tells
 * the open library once the default network is usable again, or bae comes to
 * the foreground — on the transition only, never per update.
 */
class UsableNetworkTransitionsTest {
    @Test
    fun aNetworkIsReportedOnceItIsValidated() {
        val transitions = UsableNetworkTransitions<String>()
        assertFalse(transitions.available("wifi"))
        assertTrue(transitions.validated("wifi", true))
        assertFalse(transitions.validated("wifi", true))
        assertFalse(transitions.blocked("wifi", false))
    }

    @Test
    fun aNetworkUnblockedAfterTheLockIsReportedAgain() {
        val transitions = UsableNetworkTransitions<String>()
        transitions.available("wifi")
        assertTrue(transitions.validated("wifi", true))
        assertFalse(transitions.blocked("wifi", true))
        assertFalse(transitions.blocked("wifi", true))
        assertTrue(transitions.blocked("wifi", false))
    }

    @Test
    fun movingToAnotherNetworkIsReported() {
        val transitions = UsableNetworkTransitions<String>()
        transitions.available("wifi")
        transitions.validated("wifi", true)
        assertFalse(transitions.available("cell"))
        assertTrue(transitions.validated("cell", true))
    }

    @Test
    fun aNetworkBackAfterBeingLostIsReported() {
        val transitions = UsableNetworkTransitions<String>()
        transitions.available("wifi")
        transitions.validated("wifi", true)
        transitions.lost("wifi")
        transitions.available("wifi")
        assertTrue(transitions.validated("wifi", true))
    }

    @Test
    fun updatesAboutAnotherNetworkAreIgnored() {
        val transitions = UsableNetworkTransitions<String>()
        transitions.available("cell")
        assertFalse(transitions.validated("wifi", true))
        transitions.lost("wifi")
        assertTrue(transitions.validated("cell", true))
    }
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class NetworkChangeReporterTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val owner =
        object : LifecycleOwner {
            override val lifecycle = LifecycleRegistry.createUnsafe(this)
        }
    private var reports = 0
    private val reporter = NetworkChangeReporter(connectivity, owner.lifecycle) { reports++ }

    private fun callback(): ConnectivityManager.NetworkCallback = shadowOf(connectivity).networkCallbacks.single()

    private fun validated(): NetworkCapabilities =
        ShadowNetworkCapabilities.newInstance().also {
            shadowOf(it).addCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        }

    @Test
    fun aNetworkUnblockedAfterTheLockIsReported() {
        reporter.start()
        val network: Network = ShadowNetwork.newInstance(1)
        callback().onAvailable(network)
        callback().onCapabilitiesChanged(network, validated())
        assertEquals(1, reports)
        callback().onBlockedStatusChanged(network, true)
        callback().onCapabilitiesChanged(network, validated())
        assertEquals(1, reports)
        callback().onBlockedStatusChanged(network, false)
        assertEquals(2, reports)
    }

    @Test
    fun comingToTheForegroundIsReported() {
        reporter.start()
        owner.lifecycle.handleLifecycleEvent(Lifecycle.Event.ON_START)
        owner.lifecycle.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)
        assertEquals(1, reports)
        owner.lifecycle.handleLifecycleEvent(Lifecycle.Event.ON_STOP)
        owner.lifecycle.handleLifecycleEvent(Lifecycle.Event.ON_START)
        assertEquals(2, reports)
    }

    @Test
    fun stoppingUnregistersAndReportsNothingMore() {
        reporter.start()
        val network = callback()
        reporter.stop()
        assertTrue(shadowOf(connectivity).networkCallbacks.isEmpty())
        assertEquals(0, owner.lifecycle.observerCount)
        network.onAvailable(ShadowNetwork.newInstance(1))
        network.onCapabilitiesChanged(ShadowNetwork.newInstance(1), validated())
        assertEquals(0, reports)
    }

    @Test
    fun stoppingWhatNeverStartedDoesNothing() {
        reporter.stop()
        assertEquals(0, reports)
    }
}
