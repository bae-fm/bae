package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BaeLogger
import fm.bae.app.OAuthProviders
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.LabeledValueRow
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.library.LibraryDestination
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeStorageLocation

private val logger = BaeLogger("bae.SettingsSyncSection")

/**
 * Where the library is stored and how its sync stands: the cloud home's
 * details, the one status line and upload pause — or, when this device no
 * longer holds its credentials for that storage, signing in to it again; or,
 * without a cloud home, the way to set one up. Under either, the last sync
 * failure, such as a start that failed on a locked keychain.
 */
@Composable
internal fun SettingsSyncSection(
    onResetDevice: (BridgeDeviceReset) -> Unit,
    session: OpenLibrary,
    cloudHome: CloudHome,
    oauthProviders: OAuthProviders?,
    onPush: (LibraryDestination) -> Unit,
) {
    val line by session.syncStatusStore.line.collectAsState()
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_sync))
        when (cloudHome) {
            CloudHome.Loading -> {
                Unit
            }

            is CloudHome.Failed -> {
                ErrorDetailDisclosure(cloudHome.error)
            }

            CloudHome.None -> {
                Text(stringResource(R.string.settings_local_only))
                Text(
                    text = stringResource(R.string.settings_set_up_sync_explanation),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                PrimaryButton(onClick = { onPush(LibraryDestination.StorageSetup) }) {
                    Text(stringResource(R.string.storage_setup_title))
                }
            }

            is CloudHome.At -> {
                CloudHomeRows(cloudHome.location)
                if (line == SyncLine.NoStorage) {
                    StorageSignIn(session = session, location = cloudHome.location, oauthProviders = oauthProviders)
                    SyncFailureOffers(session = session, onResetDevice = onResetDevice)
                } else {
                    SyncStatusLine(line = line, onOpenPending = { onPush(LibraryDestination.SyncPending) })
                    SyncFailureOffers(session = session, onResetDevice = onResetDevice)
                    SyncUploadPauseControl(session)
                }
            }
        }
    }
}

/**
 * The last sync failure, with Reconnect where a start failed with storage set
 * up, or Reset device, after its warning, where sync found another copy of this
 * device.
 */
@Composable
private fun SyncFailureOffers(
    session: OpenLibrary,
    onResetDevice: (BridgeDeviceReset) -> Unit,
) {
    val scope = rememberScreenScope()
    val error by session.syncStatusStore.error.collectAsState()
    SyncFailureNotice(
        error = error,
        onReconnect = {
            scope.launch {
                try {
                    session.appHandle.reconnectSync()
                } catch (e: BridgeException) {
                    // Core records a start that fails again as the status's failure, which the notice shows.
                    logger.error("Reconnect failed", e)
                }
            }
        },
    )
    DeviceResetOffer(session = session, error = error, onResetDevice = onResetDevice)
}

/**
 * The cloud home's provider and, for S3, the bucket it names; Dropbox and
 * OneDrive show the signed-in account, since their folder has no name to show.
 */
@Composable
private fun CloudHomeRows(location: BridgeStorageLocation) {
    LabeledValueRow(stringResource(R.string.settings_provider), cloudProviderLabel(storageProvider(location)))
    when (location) {
        is BridgeStorageLocation.S3 -> {
            LabeledValueRow(stringResource(R.string.settings_bucket), location.bucket)
            LabeledValueRow(stringResource(R.string.settings_region), location.region)
            location.endpoint?.let { LabeledValueRow(stringResource(R.string.settings_endpoint), it) }
        }

        is BridgeStorageLocation.Dropbox -> {
            location.accountLabel?.let { LabeledValueRow(stringResource(R.string.settings_account), it) }
        }

        is BridgeStorageLocation.OneDrive -> {
            location.accountLabel?.let { LabeledValueRow(stringResource(R.string.settings_account), it) }
        }

        is BridgeStorageLocation.CloudKit -> {
            Unit
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun CloudHomeRowsPreview() {
    BaeTheme {
        Column {
            CloudHomeRows(
                BridgeStorageLocation.S3(
                    bucket = "household-music",
                    region = "auto",
                    endpoint = "https://account-id.r2.cloudflarestorage.com",
                    profile = null,
                ),
            )
        }
    }
}
