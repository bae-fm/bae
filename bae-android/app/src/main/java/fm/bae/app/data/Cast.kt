package fm.bae.app.data

import android.content.Context
import android.net.wifi.WifiManager
import uniffi.bae_bridge.AppHandle

/**
 * Cast transport: browse for devices, and move playback to or from one. Reads
 * flow through the [CastStore]; this is the write side.
 *
 * Browsing needs a multicast lock. Android drops multicast and broadcast packets
 * that aren't addressed to the device while Wi-Fi power-save is on, which is
 * every packet mDNS and SSDP answer with — so a browse without the lock finds
 * nothing on most phones. The lock is held for exactly as long as the browse.
 */
class Cast(
    private val appHandle: AppHandle,
    context: Context,
) {
    private val multicastLock =
        (context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
            .createMulticastLock("bae-cast-discovery")
            .apply { setReferenceCounted(false) }

    /** Begin browsing for devices (the picker opened). */
    fun startDiscovery() {
        if (!multicastLock.isHeld) {
            multicastLock.acquire()
        }
        appHandle.startCastDiscovery()
    }

    /** Stop browsing for devices (the picker closed). */
    fun stopDiscovery() {
        appHandle.stopCastDiscovery()
        if (multicastLock.isHeld) {
            multicastLock.release()
        }
    }

    /** Switch playback to the device with this id. */
    fun castTo(deviceId: String) = appHandle.castTo(deviceId)

    /** Stop casting and return playback to local output. */
    fun stopCasting() = appHandle.stopCasting()

    /**
     * Whether casting is available at all. Turning it off is what stops
     * discovery and ends a session in flight — core does both off the write, so
     * the settings toggle only has to make this call.
     */
    fun setEnabled(enabled: Boolean) = appHandle.setCastEnabled(enabled)
}
