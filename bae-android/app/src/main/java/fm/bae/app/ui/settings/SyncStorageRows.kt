package fm.bae.app.ui.settings

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import uniffi.bae_bridge.BridgeFolderLabel
import uniffi.bae_bridge.BridgeStorageLocation

private val logger = BaeLogger("bae.SyncStorageRows")

/**
 * The library's storage: its provider, then for S3 the endpoint, region and bucket it names, and for Dropbox and
 * OneDrive the signed-in account and the folder coven recorded, then Open in browser, which opens the folder's page
 * at the provider. A row shows a name or isn't there.
 */
@Composable
internal fun StorageSection(location: BridgeStorageLocation) {
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        AndroidSection(text = stringResource(R.string.sync_cloud_home))
        AndroidCard {
            StorageRows(location)
            storageFolder(location)?.let { OpenFolderRow(it) }
        }
    }
}

/** The rows of [StorageSection]'s card, which the welcome's library details sheet shows too, as they are. */
@Composable
internal fun StorageRows(location: BridgeStorageLocation) {
    AndroidRow(
        title = stringResource(R.string.settings_provider),
        subtitle = cloudProviderLabel(storageProvider(location)),
    )
    when (location) {
        is BridgeStorageLocation.S3 -> {
            location.endpoint?.let { AndroidRow(title = stringResource(R.string.settings_endpoint), subtitle = it) }
            AndroidRow(title = stringResource(R.string.settings_region), subtitle = location.region)
            AndroidRow(title = stringResource(R.string.settings_bucket), subtitle = location.bucket)
        }

        is BridgeStorageLocation.Dropbox -> {
            location.accountLabel?.let { AndroidRow(title = stringResource(R.string.settings_account), subtitle = it) }
            location.folderLabel?.let { FolderRow(it) }
        }

        is BridgeStorageLocation.OneDrive -> {
            location.accountLabel?.let { AndroidRow(title = stringResource(R.string.settings_account), subtitle = it) }
            location.folderLabel?.let { FolderRow(it) }
        }

        is BridgeStorageLocation.CloudKit -> {
            Unit
        }
    }
}

@Composable
private fun FolderRow(folder: BridgeFolderLabel) {
    AndroidRow(title = stringResource(R.string.settings_folder), subtitle = folder.name)
}

private fun storageFolder(location: BridgeStorageLocation): BridgeFolderLabel? =
    when (location) {
        is BridgeStorageLocation.Dropbox -> location.folderLabel
        is BridgeStorageLocation.OneDrive -> location.folderLabel
        is BridgeStorageLocation.S3, is BridgeStorageLocation.CloudKit -> null
    }

/** The row that opens a Dropbox shared folder's or a OneDrive folder's page at the provider. */
@Composable
private fun OpenFolderRow(folder: BridgeFolderLabel) {
    val context = LocalContext.current
    var openFailure by remember { mutableStateOf<DisplayError?>(null) }
    AndroidRow(
        title = stringResource(R.string.settings_open_in_browser),
        onClick = { openFailure = openInBrowser(context, folder.webUrl) },
    )
    openFailure?.let { AndroidNotice(it) }
}

/** Open `webUrl` in the browser; the error to show when no app on this device opens it. */
internal fun openInBrowser(
    context: Context,
    webUrl: String,
): DisplayError? =
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(webUrl)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        null
    } catch (e: ActivityNotFoundException) {
        logger.warning("no app opened the folder's web address $webUrl", e)
        DisplayError(context.getString(R.string.settings_folder_no_app))
    }
