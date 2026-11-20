package fm.bae.app.playback

import uniffi.bae_bridge.BridgeLocalTimeZone
import java.util.TimeZone

/**
 * The time zone the device is in, which the app hands core so that a play's day is the day the device's calendar
 * showed when the play began.
 */
internal class DeviceTimeZone : BridgeLocalTimeZone {
    override fun utcOffsetSeconds(unixMillis: Long): Int {
        val offsetMillis = TimeZone.getDefault().getOffset(unixMillis)
        return offsetMillis / MILLIS_PER_SECOND
    }

    private companion object {
        const val MILLIS_PER_SECOND = 1_000
    }
}
