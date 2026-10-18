package fm.bae.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.settings.storageProvider
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeStorageLocation

/** The provider's name as the person knows it. */
@Composable
internal fun cloudProviderLabel(provider: BridgeCloudProvider): String =
    when (provider) {
        BridgeCloudProvider.S3 -> stringResource(R.string.cloud_provider_s3)
        BridgeCloudProvider.CLOUD_KIT -> stringResource(R.string.cloud_provider_icloud)
        BridgeCloudProvider.DROPBOX -> stringResource(R.string.cloud_provider_dropbox)
        BridgeCloudProvider.ONE_DRIVE -> stringResource(R.string.cloud_provider_onedrive)
    }

/**
 * The provider with the account or bucket a library is in, as restore and
 * library rows show it: "Dropbox: ana@example.com", "S3-compatible: bucket
 * (endpoint)", "iCloud". The endpoint is the one the person entered; Amazon S3
 * has none, so its region names the place instead. A Dropbox or OneDrive
 * location coven hasn't set up has no account yet.
 */
@Composable
internal fun storageLocationLabel(location: BridgeStorageLocation): String {
    val provider = cloudProviderLabel(storageProvider(location))
    return when (location) {
        is BridgeStorageLocation.S3 -> {
            val place = location.endpoint ?: location.region
            stringResource(R.string.storage_location_bucket, provider, location.bucket, place)
        }

        is BridgeStorageLocation.Dropbox -> {
            location.accountLabel?.let { stringResource(R.string.storage_location_account, provider, it) } ?: provider
        }

        is BridgeStorageLocation.OneDrive -> {
            location.accountLabel?.let { stringResource(R.string.storage_location_account, provider, it) } ?: provider
        }

        is BridgeStorageLocation.CloudKit -> {
            provider
        }
    }
}
