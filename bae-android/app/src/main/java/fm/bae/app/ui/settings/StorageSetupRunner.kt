package fm.bae.app.ui.settings

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeApp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.data.StorageSetupProgress
import fm.bae.app.performBridgeAction
import fm.bae.fastvalues.LatestSource
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.BridgeStorageSetupProgress

private val logger = BaeLogger("bae.StorageSetupRunner")

/**
 * Runs the library's storage setup calls that reconnect it (signing in to its
 * provider again, or a new S3 access key): each one's steps are [steps] while
 * it runs, pulled in [scope]; its failure goes to [onError].
 */
internal class StorageSetupRunner(
    private val context: Context,
    private val session: OpenLibrary,
    private val scope: CoroutineScope,
    private val onError: (DisplayError?) -> Unit,
) {
    /** The running setup's steps; null while none runs. */
    var steps by mutableStateOf<LatestSource<BridgeStorageSetupProgress>?>(null)
        private set

    /** Run [setUp], named [operation] in the log. */
    suspend fun run(
        operation: String,
        setUp: suspend (setup: BridgeLibrarySetup) -> BridgeStorageSetup,
    ) {
        StorageSetupProgress((context.applicationContext as BaeApp).host, scope, session.bugs).use { progress ->
            steps = progress.steps
            try {
                performBridgeAction(logger, operation, session.errors, onError) {
                    setUp(progress.setup)
                }
            } finally {
                steps = null
            }
        }
    }
}
