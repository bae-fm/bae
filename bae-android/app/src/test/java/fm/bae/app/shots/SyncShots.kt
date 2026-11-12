package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.settings.PendingSection
import fm.bae.app.ui.settings.StorageSection
import fm.bae.app.ui.settings.SyncStatusBlock
import org.junit.Test
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingReason
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone

/** The shots of the android.settings.sync.* boards. */
class SyncShots : BoardShot() {
    /** The Sync page waiting on two records: a changed source file, and a cover whose cloud request failed. */
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
                                        lastSynced = true,
                                    ),
                                ),
                            onFix = {},
                        )
                        StorageSection(sceneDropbox)
                        PendingSection(records = pendingRecords, onAction = {})
                    }
                }
            }
        }

    private val pendingRecords =
        listOf(
            BridgePendingRecord(
                subject = BridgeMessage(key = "core.pending.subject.file", args = emptyList()),
                reason =
                    BridgePendingReason(
                        key = "core.pending.reason.file_lost",
                        detail = "the cloud holds nothing where the file should be",
                        causes = emptyList(),
                    ),
                action = BridgePendingAction.Review,
                actionKey = "core.pending.action.review",
                sendsFiles = false,
            ),
            BridgePendingRecord(
                subject = BridgeMessage(key = "core.pending.subject.local_changes", args = emptyList()),
                reason =
                    BridgePendingReason(
                        key = "core.pending.reason.storage",
                        detail = "request failed: connection refused",
                        causes = emptyList(),
                    ),
                action = BridgePendingAction.RetryOperation(id = 7L),
                actionKey = "core.pending.action.retry",
                sendsFiles = false,
            ),
        )
}
