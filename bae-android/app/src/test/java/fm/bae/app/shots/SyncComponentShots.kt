package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.settings.AndroidInviteFields
import fm.bae.app.ui.settings.AndroidSyncStatus
import fm.bae.app.ui.settings.InviteAccessInput
import fm.bae.app.ui.settings.S3KeyReplacement
import fm.bae.app.ui.settings.StorageSection
import org.junit.Test
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone

/** The component shots of Sync and sharing in Settings: the sync status, an S3 library's cloud home and the invite form's fields. */
class SyncComponentShots : BoardShot() {
    @Test
    fun androidComponentSyncStatusStatus() =
        component("android.component.sync-status.status", wide = true) { SignedOutOfS3(BridgeSyncTone.WARNING) }

    @Test
    fun androidComponentSyncStatusNoTone() =
        component("android.component.sync-status.no-tone", wide = true) { SignedOutOfS3(BridgeSyncTone.NEUTRAL) }

    @Test
    fun androidComponentSyncStatusCause() =
        component("android.component.sync-status.cause", wide = true) {
            SignedOutOfS3(BridgeSyncTone.WARNING, cause = BridgeSyncCause.ChangesStayOnDevice)
        }

    @Test
    fun androidComponentSyncStatusFix() =
        component("android.component.sync-status.fix", wide = true) {
            SignedOutOfS3(BridgeSyncTone.WARNING, fix = BridgeSyncFix.EnterAccessKey)
        }

    @Test
    fun androidComponentSyncStatusLastSynced() =
        component("android.component.sync-status.last-synced", wide = true) {
            SignedOutOfS3(BridgeSyncTone.WARNING, lastSynced = true)
        }

    @Test
    fun androidComponentCloudHomeResting() =
        component("android.component.cloud-home.resting", wide = true) { CloudHome(key = "AKIA4EXAMPLEOLD") }

    @Test
    fun androidComponentCloudHomeKeyReplaced() =
        component("android.component.cloud-home.key-replaced", wide = true) {
            CloudHome(key = "AKIA4EXAMPLENEW", keyLine = S3KeyReplacement.Line.Saved("AKIA4EXAMPLEOLD"))
        }

    @Test
    fun androidComponentInviteFieldsS3TheirKey() =
        component("android.component.invite-fields.s3-their-key", wide = true) {
            Fields(sceneS3, InviteAccessInput(accessKeyId = "AKIAEXAMPLEKEY", secretAccessKey = "••••••••••••••••"))
        }

    @Test
    fun androidComponentInviteFieldsS3MyKey() =
        component("android.component.invite-fields.s3-my-key", wide = true) {
            Fields(sceneS3, InviteAccessInput().apply { usesOwnKey = true })
        }

    @Test
    fun androidComponentInviteFieldsDropbox() =
        component("android.component.invite-fields.dropbox", wide = true) {
            Fields(sceneDropbox, InviteAccessInput(email = "dan@example.com"))
        }

    @Test
    fun androidComponentInviteFieldsOnedrive() =
        component("android.component.invite-fields.onedrive", wide = true) {
            Fields(
                BridgeStorageLocation.OneDrive("drive", "folder", "name@example.com", null),
                InviteAccessInput(email = "dan@example.com"),
            )
        }
}

/** The status the preview shows, Signed out of S3, in [tone], with the [cause], [fix] and last sync it is given. */
@Composable
private fun SignedOutOfS3(
    tone: BridgeSyncTone,
    cause: BridgeSyncCause? = null,
    fix: BridgeSyncFix? = null,
    lastSynced: Boolean = false,
) = AndroidSyncStatus(
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

/** The S3 library's storage as Settings shows it, recording [key] for the person, with [keyLine] under it. */
@Composable
private fun CloudHome(
    key: String,
    keyLine: S3KeyReplacement.Line? = null,
) = StorageSection(
    location = sceneS3,
    access = BridgeMemberAccess.S3AccessKey(key),
    keyLine = { keyLine },
    onReplaceKey = {},
    onDismissKeyLine = {},
)

@Composable
private fun Fields(
    storage: BridgeStorageLocation,
    input: InviteAccessInput,
) = AndroidInviteFields(storage = storage, input = input, error = null)
