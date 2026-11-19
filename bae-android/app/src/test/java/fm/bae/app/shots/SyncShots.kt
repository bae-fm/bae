package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.settings.StorageSection
import fm.bae.app.ui.settings.SyncStatusBlock
import org.junit.Test
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeFolderLabel
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone

/** The shots of the android.settings.sync.* boards. */
class SyncShots : BoardShot() {
    /** The Sync page while sync waits, over the library's storage. */
    @Test
    fun androidSettingsSyncPending() =
        shot("android.settings.sync.pending") {
            ScreenScene {
                AndroidScreen(title = "Sync", onBack = {}, modifier = Modifier.fillMaxSize()) {
                    Column(
                        modifier = Modifier.padding(ThemeSpace.edge),
                        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
                    ) {
                        SyncStatusBlock(
                            line =
                                SyncLine.Read(
                                    syncShown(
                                        BridgeSyncStatusWord.WAITING_TO_SYNC,
                                        "core.sync.status.waiting_to_sync",
                                        BridgeSyncTone.NEUTRAL,
                                        lastSyncedAgo = twoHoursAgo,
                                    ),
                                ),
                            onFix = {},
                        )
                        StorageSection(
                            location = accountDropbox,
                            access = null,
                            keyLine = { null },
                            onReplaceKey = {},
                            onDismissKeyLine = {},
                        )
                    }
                }
            }
        }
}

private val accountDropbox =
    BridgeStorageLocation.Dropbox(
        location = BridgeDropboxLocation.AppFolder,
        accountLabel = "name@example.com",
        folderLabel = BridgeFolderLabel(name = "Music", webUrl = "https://www.dropbox.com/home/Music"),
    )
