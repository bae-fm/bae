package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.settings.AndroidSyncStatus
import org.junit.Test
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone

/** The component shots of `AndroidSyncStatus`, one for each row of its preview. */
class AndroidSyncStatusShots : BoardShot() {
    @Test
    fun androidComponentSyncStatusStatus() = component("android.component.sync-status.status") { SignedOutOfS3(BridgeSyncTone.WARNING) }

    @Test
    fun androidComponentSyncStatusNoTone() = component("android.component.sync-status.no-tone") { SignedOutOfS3(BridgeSyncTone.NEUTRAL) }

    @Test
    fun androidComponentSyncStatusCause() =
        component("android.component.sync-status.cause") {
            SignedOutOfS3(BridgeSyncTone.WARNING, cause = BridgeSyncCause.ChangesStayOnDevice)
        }

    @Test
    fun androidComponentSyncStatusFix() =
        component("android.component.sync-status.fix") {
            SignedOutOfS3(BridgeSyncTone.WARNING, fix = BridgeSyncFix.EnterAccessKey)
        }

    @Test
    fun androidComponentSyncStatusLastSynced() =
        component("android.component.sync-status.last-synced") {
            SignedOutOfS3(BridgeSyncTone.WARNING, lastSynced = true)
        }

    @Test
    fun androidComponentSyncStatusToneSuccess() =
        component("android.component.sync-status.tone-success") { SignedOutOfS3(BridgeSyncTone.SUCCESS) }

    @Test
    fun androidComponentSyncStatusCountdown() =
        component("android.component.sync-status.countdown") {
            InsideScreenEdges(top = ThemeSpace.related) {
                AndroidSyncStatus(
                    line =
                        SyncLine.Read(
                            syncShown(
                                BridgeSyncStatusWord.WAITING_TO_SYNC,
                                "core.sync.status.waiting_to_sync",
                                BridgeSyncTone.WARNING,
                                cause = dropboxAskedToWait(),
                            ),
                        ),
                    onFix = {},
                )
            }
        }
}

/** The status the preview shows, Signed out of S3, in [tone], with the [cause], [fix] and last sync it is given. */
@Composable
private fun SignedOutOfS3(
    tone: BridgeSyncTone,
    cause: BridgeSyncCause? = null,
    fix: BridgeSyncFix? = null,
    lastSynced: Boolean = false,
) = InsideScreenEdges(top = ThemeSpace.related) {
    AndroidSyncStatus(
        line =
            SyncLine.Read(
                syncShown(
                    BridgeSyncStatusWord.SIGNED_OUT_OF_S3,
                    "core.sync.status.signed_out_s3",
                    tone,
                    cause = cause,
                    fix = fix,
                    lastSyncedAgo = if (lastSynced) threeDaysAgo else null,
                ),
            ),
        onFix = {},
    )
}
