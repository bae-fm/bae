package fm.bae.app.data

import android.content.Context
import android.net.wifi.WifiManager
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeBrowsingFor
import uniffi.bae_bridge.BridgeCastTranscodeFormat

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
    private val appHandle: AppHandleInterface,
    context: Context,
) {
    private val multicastLock =
        (context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
            .createMulticastLock("bae-cast-discovery")
            .apply { setReferenceCounted(false) }

    /** The places open that list receivers: discovery runs exactly while one is. */
    private var browsingFor: Set<BridgeBrowsingFor> = emptySet()

    /**
     * Whether the picker is open. A cast that ends while it is open shows in
     * the picker; otherwise the person is told wherever they are.
     */
    val pickerOpen: Boolean
        get() = BridgeBrowsingFor.PICKER in browsingFor

    /** Begin browsing for devices for [asker], the place that opened. */
    fun startDiscovery(asker: BridgeBrowsingFor) {
        browsingFor = browsingFor + asker
        if (!multicastLock.isHeld) {
            multicastLock.acquire()
        }
        appHandle.startCastDiscovery(asker)
    }

    /** [asker] closed: browsing, and the lock, end with the last place open. */
    fun stopDiscovery(asker: BridgeBrowsingFor) {
        browsingFor = browsingFor - asker
        appHandle.stopCastDiscovery(asker)
        if (browsingFor.isEmpty() && multicastLock.isHeld) {
            multicastLock.release()
        }
    }

    /** Switch playback to the device with this id. */
    suspend fun castTo(deviceId: String) = appHandle.castTo(deviceId)

    /** Stop casting and return playback to local output. */
    fun stopCasting() = appHandle.stopCasting()

    /**
     * Whether casting is available at all. Turning it off is what stops
     * discovery and ends a session in flight — core does both off the write, so
     * the settings toggle only has to make this call.
     */
    suspend fun setEnabled(enabled: Boolean) = appHandle.setCastEnabled(enabled)

    /** What a track a device can't play directly is converted to; core reads it as each track loads. */
    suspend fun setTranscodeFormat(format: BridgeCastTranscodeFormat) = appHandle.setCastTranscodeFormat(format)
}
