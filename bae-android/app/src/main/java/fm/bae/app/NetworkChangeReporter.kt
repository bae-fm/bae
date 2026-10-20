package fm.bae.app

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Handler
import android.os.Looper
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner

/**
 * Tells the open library when its network may be back, so sync work that
 * failed for want of a network runs now instead of sitting out a backoff of up
 * to five minutes. While the phone is locked Android blocks bae's network, and
 * nothing in bae's own requests can tell when it returns.
 *
 * It reports when the default network becomes usable — validated, and not
 * blocked for bae — or a different network becomes usable, and each time bae
 * comes to the foreground. It reports transitions only: updates to a network
 * already reported, and losing it, report nothing.
 *
 * It lives with one open session: [start] when the session is wired up,
 * [stop] before its handle closes. Everything runs on the main thread, so no
 * report reaches a handle after [stop].
 */
internal class NetworkChangeReporter(
    private val connectivity: ConnectivityManager,
    private val lifecycle: Lifecycle,
    private val report: () -> Unit,
) {
    private val transitions = UsableNetworkTransitions<Network>()
    private var running = false

    private val network =
        object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                reportIf(transitions.available(network))
            }

            override fun onCapabilitiesChanged(
                network: Network,
                capabilities: NetworkCapabilities,
            ) {
                reportIf(
                    transitions.validated(
                        network,
                        capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED),
                    ),
                )
            }

            // Delivered from API 29; before it, a blocked network shows no
            // change here and the foreground report is the only signal.
            override fun onBlockedStatusChanged(
                network: Network,
                blocked: Boolean,
            ) {
                reportIf(transitions.blocked(network, blocked))
            }

            override fun onLost(network: Network) {
                transitions.lost(network)
            }
        }

    private val foreground =
        object : DefaultLifecycleObserver {
            override fun onStart(owner: LifecycleOwner) {
                reportIf(true)
            }
        }

    private fun reportIf(changed: Boolean) {
        if (running && changed) report()
    }

    /**
     * Start watching. The current state counts as a change: a usable network
     * already up, and bae already in the foreground, each report once, so a
     * backoff saved before the library opened ends now.
     */
    fun start() {
        check(!running) { "network reports already started" }
        running = true
        connectivity.registerDefaultNetworkCallback(network, Handler(Looper.getMainLooper()))
        lifecycle.addObserver(foreground)
    }

    /** Stop watching; does nothing if never started. */
    fun stop() {
        if (!running) return
        running = false
        lifecycle.removeObserver(foreground)
        connectivity.unregisterNetworkCallback(network)
    }
}

/**
 * Whether the default network just became usable for bae: validated, and not
 * blocked for it. The first usable network, and each different one after it,
 * is a change; anything about a network already reported is not, nor is
 * losing one. Updates about a network other than the current default are
 * ignored. [N] is the platform's network identity.
 */
internal class UsableNetworkTransitions<N : Any> {
    private var current: N? = null
    private var validated = false
    private var blocked = false
    private var reported: N? = null

    /** [network] became the default; its capabilities and blocked state follow. */
    fun available(network: N): Boolean {
        current = network
        validated = false
        blocked = false
        return settle()
    }

    fun validated(
        network: N,
        validated: Boolean,
    ): Boolean {
        if (network != current) return false
        this.validated = validated
        return settle()
    }

    fun blocked(
        network: N,
        blocked: Boolean,
    ): Boolean {
        if (network != current) return false
        this.blocked = blocked
        return settle()
    }

    fun lost(network: N) {
        if (network != current) return
        current = null
        settle()
    }

    private fun settle(): Boolean {
        val usable = current?.takeIf { validated && !blocked }
        val changed = usable != null && usable != reported
        reported = usable
        return changed
    }
}
