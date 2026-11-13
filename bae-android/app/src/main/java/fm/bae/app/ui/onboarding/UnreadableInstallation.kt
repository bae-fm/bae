package fm.bae.app.ui.onboarding

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHostInterface

private val logger = BaeLogger("bae.UnreadableInstallation")

/**
 * The unfinished join or restore this phone can't read, which listing what is
 * under way failed on: the welcome lists it with Discard…, asks before
 * discarding it, and lists again afterwards, since another may be unreadable
 * too and a join that can resume goes on. [onFailed] hears a discard that
 * failed, which leaves the row in place; [listAgain] runs the listing.
 */
internal class UnreadableInstallation(
    private val scope: CoroutineScope,
    private val host: BridgeHostInterface,
    private val onFailed: (BridgeException) -> Unit,
    private val listAgain: () -> Unit,
) {
    /** The library whose installation can't be read; null when none is. */
    var libraryId by mutableStateOf<String?>(null)
        private set

    /** The library the person was asked about and hasn't answered; null when none is. */
    var asking by mutableStateOf<String?>(null)
        private set

    /** The listing failed on the installation of [libraryId]. */
    fun found(libraryId: String) {
        this.libraryId = libraryId
    }

    /** Ask whether to discard the installation of [libraryId]. */
    fun ask(libraryId: String) {
        asking = libraryId
    }

    /** The person kept the installation. */
    fun dismiss() {
        asking = null
    }

    /** Delete the installation of [libraryId] from this phone, then list again. */
    fun discard(libraryId: String) {
        asking = null
        scope.launch {
            try {
                host.discardUnfinishedInstallation(libraryId)
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("discarding the unfinished installation failed", e)
                onFailed(e)
                return@launch
            }
            this@UnreadableInstallation.libraryId = null
            listAgain()
        }
    }
}
