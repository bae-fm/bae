package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidScreen
import uniffi.bae_bridge.BridgeCacheLimitChoice
import uniffi.bae_bridge.BridgeDeviceStorage

/** The dialog the Storage page has open over it. */
private enum class StorageDialog { KeepUpTo, DeleteBeforeReset }

/**
 * What this phone keeps for the open library, with Clear cache, Keep up to, Ask before unpinning and, while a reset's
 * data is here, its deletion. It follows core's measurement while it shows.
 */
@Composable
internal fun StorageScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val model = rememberDeviceStorageModel(session)
    val title = stringResource(R.string.settings_storage)
    when (val read = model.read) {
        // Nothing has been measured yet.
        null -> {
            AndroidScreen(title = title, onBack = onBack, modifier = Modifier.fillMaxSize()) {}
        }

        is LiveValue.Failed -> {
            AndroidScreen(title = title, onBack = onBack, modifier = Modifier.fillMaxSize()) {
                AndroidNotice(read.error, modifier = Modifier.padding(ThemeSpace.edge))
            }
        }

        is LiveValue.Current -> {
            LoadedStorage(session = session, storage = read.value, onBack = onBack)
        }
    }
}

@Composable
private fun LoadedStorage(
    session: OpenLibrary,
    storage: BridgeDeviceStorage,
    onBack: () -> Unit,
) {
    val config by session.configStore.config.collectAsState()
    var dialog by remember { mutableStateOf<StorageDialog?>(null) }
    AndroidSettingsStorage(
        storage = storage,
        asksBeforeUnpinning = config.askBeforeUnpinning,
        onBack = onBack,
        onClearCache = { session.messages.perform("clear cache") { session.appHandle.clearCache() } },
        onKeepUpTo = { dialog = StorageDialog.KeepUpTo },
        onAskBeforeUnpinning = { asks ->
            session.messages.perform("set ask before unpinning") { session.appHandle.setAskBeforeUnpinning(asks) }
        },
        onDeleteBeforeReset = { dialog = StorageDialog.DeleteBeforeReset },
    )
    val dismiss = { dialog = null }
    when (dialog) {
        StorageDialog.KeepUpTo -> {
            val choose: (BridgeCacheLimitChoice) -> Unit = { choice ->
                dialog = null
                session.messages.perform("set cache limit") { session.appHandle.setCacheLimit(choice) }
            }
            AndroidDialog(
                onDismissRequest = dismiss,
                parts = keepUpToParts(storage.cacheLimits, storage.cacheLimit.choice, choose, dismiss),
            )
        }

        StorageDialog.DeleteBeforeReset -> {
            // The data is gone once it is deleted, and the question with it.
            storage.beforeReset?.let { bytes ->
                val confirm = {
                    dialog = null
                    session.messages.perform("delete data from before the reset") {
                        session.appHandle.deleteDataFromBeforeReset()
                    }
                }
                AndroidDialog(
                    onDismissRequest = dismiss,
                    parts = deleteBeforeResetParts(bytes, confirm, dismiss),
                )
            }
        }

        null -> {}
    }
}
