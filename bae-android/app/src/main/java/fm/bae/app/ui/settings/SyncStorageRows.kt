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
import fm.bae.app.ui.components.CardRow
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.RowCard
import uniffi.bae_bridge.BridgeFolderLabel
import uniffi.bae_bridge.BridgeStorageLocation

private val logger = BaeLogger("bae.SyncStorageRows")

/**
 * The library's storage: its provider, then for S3 the endpoint, region and bucket it names, and for Dropbox and
 * OneDrive the signed-in account and the folder coven recorded, with Open in browser, which opens the folder's page
 * at the provider. A row shows a name or isn't there.
 */
@Composable
internal fun StorageSection(location: BridgeStorageLocation) {
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        Eyebrow(text = stringResource(R.string.sync_cloud_home))
        RowCard { StorageRows(location) }
    }
}

/** The rows of [StorageSection]'s card, which the welcome's library details sheet shows too. */
@Composable
internal fun StorageRows(location: BridgeStorageLocation) {
    CardRow(
        title = stringResource(R.string.settings_provider),
        subtitle = cloudProviderLabel(storageProvider(location)),
    )
    when (location) {
        is BridgeStorageLocation.S3 -> {
            location.endpoint?.let { CardRow(title = stringResource(R.string.settings_endpoint), subtitle = it) }
            CardRow(title = stringResource(R.string.settings_region), subtitle = location.region)
            CardRow(title = stringResource(R.string.settings_bucket), subtitle = location.bucket)
        }

        is BridgeStorageLocation.Dropbox -> {
            location.accountLabel?.let { CardRow(title = stringResource(R.string.settings_account), subtitle = it) }
            location.folderLabel?.let { FolderRows(it) }
        }

        is BridgeStorageLocation.OneDrive -> {
            location.accountLabel?.let { CardRow(title = stringResource(R.string.settings_account), subtitle = it) }
            location.folderLabel?.let { FolderRows(it) }
        }

        is BridgeStorageLocation.CloudKit -> {
            Unit
        }
    }
}

/** A Dropbox shared folder's or a OneDrive folder's name, and the row that opens its page at the provider. */
@Composable
private fun FolderRows(folder: BridgeFolderLabel) {
    val context = LocalContext.current
    var openFailure by remember { mutableStateOf<DisplayError?>(null) }
    CardRow(title = stringResource(R.string.settings_folder), subtitle = folder.name)
    CardRow(
        title = stringResource(R.string.settings_open_in_browser),
        onClick = { openFailure = openInBrowser(context, folder.webUrl) },
    )
    openFailure?.let { ErrorNotice(it) }
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
