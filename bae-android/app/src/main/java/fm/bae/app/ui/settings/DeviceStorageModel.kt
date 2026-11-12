package fm.bae.app.ui.settings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.rememberScreenScope
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeDeviceStorage

private val logger = BaeLogger("bae.DeviceStorageModel")

/**
 * What this phone keeps for the open library, as core measures it: the latest value, or that the last read
 * failed. Null before the first value; a failure replaces the value, and the next value replaces the failure.
 */
internal class DeviceStorageModel(
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) {
    var read by mutableStateOf<LiveValue<BridgeDeviceStorage>?>(null)
        private set

    private var subscription: ScreenSubscription<BridgeDeviceStorage>? = null

    /** Follow what the phone keeps, replacing a subscription already running. */
    fun watch() {
        stop()
        subscription =
            ScreenSubscription(
                appHandle.subscribeDeviceStorage().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { read = LiveValue.Current(it) },
                onError = { error ->
                    logger.error("reading what this phone keeps failed", error)
                    // A cancelled read shows nothing and leaves the value as it was.
                    errors.display(error)?.let { read = LiveValue.Failed(it) }
                },
            )
    }

    fun stop() {
        subscription?.stop()
        subscription = null
    }
}

/** A [DeviceStorageModel] following the library's measurement while the caller shows. */
@Composable
internal fun rememberDeviceStorageModel(session: OpenLibrary): DeviceStorageModel {
    val scope = rememberScreenScope()
    val model = remember(session) { DeviceStorageModel(session.appHandle, session.errors, scope) }
    DisposableEffect(model) {
        model.watch()
        onDispose { model.stop() }
    }
    return model
}
