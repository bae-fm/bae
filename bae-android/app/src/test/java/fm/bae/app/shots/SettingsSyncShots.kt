package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.settings.AndroidSettingsSync
import fm.bae.app.ui.settings.RestoreCodeState
import org.junit.Test
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeFolderLabel
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone

/** The shots of the ready android.settings.sync.* boards: the Sync page over each storage and some states. */
class SettingsSyncShots : BoardShot() {
    /** Sync over a Dropbox folder, synced. */
    @Test
    fun androidSettingsSyncDropbox() = shot("android.settings.sync.dropbox") { Sync(synced, folder) }

    /** Sync over a OneDrive folder, synced. */
    @Test
    fun androidSettingsSyncOnedrive() =
        shot("android.settings.sync.onedrive") {
            Sync(
                synced,
                BridgeStorageLocation.OneDrive(
                    "drive",
                    "folder",
                    "name@example.com",
                    BridgeFolderLabel("Music Library", "https://onedrive.live.com/"),
                ),
            )
        }

    /** Sync over an S3 bucket, synced. */
    @Test
    fun androidSettingsSyncS3() = shot("android.settings.sync.s3") { Sync(synced, s3) }

    /** Sync over a Dropbox folder after Dropbox signed this device out. */
    @Test
    fun androidSettingsSyncSignedOutDropbox() =
        shot("android.settings.sync.signed-out-dropbox") {
            Sync(
                SyncLine.Read(
                    syncShown(
                        BridgeSyncStatusWord.SIGNED_OUT_OF_DROPBOX,
                        "core.sync.status.signed_out_dropbox",
                        BridgeSyncTone.WARNING,
                        fix = BridgeSyncFix.SignIn(BridgeCloudProvider.DROPBOX),
                        lastSynced = true,
                    ),
                ),
                folder,
            )
        }

    /** Sync over an S3 bucket whose key was not accepted. */
    @Test
    fun androidSettingsSyncSignedOutS3() =
        shot("android.settings.sync.signed-out-s3") {
            Sync(
                SyncLine.Read(
                    syncShown(
                        BridgeSyncStatusWord.ACCESS_KEY_NOT_ACCEPTED,
                        "core.sync.status.access_key_not_accepted",
                        BridgeSyncTone.WARNING,
                        cause = BridgeSyncCause.Message(BridgeMessage("core.sync.cause.enter_key", emptyList()), null),
                        fix = BridgeSyncFix.EnterAccessKey,
                        lastSynced = true,
                    ),
                ),
                s3,
            )
        }
}

private val synced = SyncLine.Read(syncShown(BridgeSyncStatusWord.SYNCED, "core.sync.synced", BridgeSyncTone.SUCCESS))

private val folder =
    BridgeStorageLocation.Dropbox(
        location = BridgeDropboxLocation.SharedFolder("namespace"),
        accountLabel = "name@example.com",
        folderLabel = BridgeFolderLabel(name = "Music", webUrl = "https://www.dropbox.com/home/Music"),
    )

private val s3 = BridgeStorageLocation.S3("my-bucket", "us-east-1", "https://s3.home.lan", null)

@Composable
private fun Sync(
    line: SyncLine,
    location: BridgeStorageLocation,
) = ScreenScene {
    AndroidSettingsSync(
        line = line,
        notices = {},
        location = location,
        restoreCode = RestoreCodeState.Masked(RESTORE_CODE),
        localOnly = false,
        pending = emptyList(),
        onBack = {},
        onFix = {},
        onShowRestoreCode = {},
        onHideRestoreCode = {},
        onPrintRestoreCode = {},
        onSetUp = {},
        onPendingAction = {},
    )
}

private const val RESTORE_CODE = "CVR1 UJZD EGXD NCF3 2EPF 3DHO DZDO CIS2 JHTL GMXG EDN7 3U55 XTPL PFT7 V4SE H2KV"
