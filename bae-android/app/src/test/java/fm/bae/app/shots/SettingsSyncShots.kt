package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.settings.AndroidSettingsSync
import fm.bae.app.ui.settings.RestoreCodeState
import fm.bae.app.ui.settings.S3KeyReplacement
import org.junit.Test
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeFolderLabel
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeRestoreCodeShare
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

    /** Sync over a Dropbox folder with the restore code shown. */
    @Test
    fun androidSettingsSyncCodeShown() =
        shot("android.settings.sync.code-shown") { Sync(synced, folder, RestoreCodeState.Shown(SHOWN_RESTORE_CODE)) }

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

    /** Sync over an S3 bucket, synced, with the key the library records for the person. */
    @Test
    fun androidSettingsSyncS3() = shot("android.settings.sync.s3") { Sync(synced, s3) }

    /** The line after a saved S3 key, naming the old key to delete. */
    @Test
    fun androidSettingsSyncS3KeyReplaced() =
        shot("android.settings.sync.s3-key-replaced") {
            Sync(synced, s3, key = "AKIA4EXAMPLENEW", keySaved = S3KeyReplacement.Saved("AKIA4EXAMPLEOLD"))
        }

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
                        lastSyncedAgo = threeDaysAgo,
                    ),
                ),
                folder,
            )
        }

    /** Sync over an S3 bucket whose storage refuses the key. */
    @Test
    fun androidSettingsSyncSignedOutS3() =
        shot("android.settings.sync.signed-out-s3") {
            Sync(
                SyncLine.Read(
                    syncShown(
                        BridgeSyncStatusWord.SIGNED_OUT_OF_S3,
                        "core.sync.status.signed_out_s3",
                        BridgeSyncTone.WARNING,
                        cause = BridgeSyncCause.ChangesStayOnDevice,
                        fix = BridgeSyncFix.EnterAccessKey,
                        lastSyncedAgo = threeDaysAgo,
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

/** The Sync page over [location], an S3 library recording [key] for the person, with [keySaved]'s line under it. */
@Composable
private fun Sync(
    line: SyncLine,
    location: BridgeStorageLocation,
    restoreCode: RestoreCodeState = RestoreCodeState.Masked(RESTORE_CODE),
    key: String = "AKIA4EXAMPLEOLD",
    keySaved: S3KeyReplacement.Saved? = null,
) = ScreenScene {
    AndroidSettingsSync(
        line = line,
        notices = {},
        location = location,
        access = BridgeMemberAccess.S3AccessKey(key),
        keySaved = keySaved,
        onReplaceKey = {},
        onDismissKeySaved = {},
        restoreCode = restoreCode,
        onBack = {},
        onFix = {},
        onShowRestoreCode = {},
        onHideRestoreCode = {},
        onPrintRestoreCode = {},
    )
}

private val RESTORE_CODE =
    BridgeRestoreCodeShare(
        code = "CVR1 UJZD EGXD NCF3 2EPF 3DHO DZDO CIS2 JHTL GMXG EDN7 3U55 XTPL PFT7 3MNB",
        maskedGroups = listOf("CVR1", "UJZD") + List(12) { "••••" } + "3MNB",
        qrText = "CVR1-UJZDEGXDNCF32EPF3DHODZDOCIS2JHTLGMXGEDN73U55XTPLPFT73MNB",
    )

private val SHOWN_RESTORE_CODE =
    BridgeRestoreCodeShare(
        code = SHOWN_RESTORE_CODE_TEXT,
        maskedGroups = listOf("CVR1", "UJZD") + List(60) { "••••" } + "B",
        qrText = SHOWN_RESTORE_CODE_TEXT,
    )

private const val SHOWN_RESTORE_CODE_TEXT =
    "CVR1 UJZD EGXD NCF3 2EPF 3DHO DZDO CIS2 JHTL GMXG EDN7 3U55 XTPL PFT7 V4SE H2KV J72C EUVW 75EF R6ED T4SY WB5W KH7D NSIP " +
        "ZZ7F K4ZR I3R2 WYOJ FLJO OA7L QSAJ 2XUI D5ZZ ZZG6 ZDME N4KH VDGA JGXB ENYJ QWX6 HH75 66TF JGVQ 6KBN XJBT FQXK WOVO MPZO " +
        "M7WB BR6Q MW4W XFOG O6MV N6A6 WFHY M6L3 VFZ5 ZFKK IBJ5 J6WJ IBAG I3MN B"
