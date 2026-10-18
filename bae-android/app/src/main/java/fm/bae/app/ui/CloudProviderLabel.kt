package fm.bae.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import uniffi.bae_bridge.BridgeCloudProvider

/** The provider's name as the person knows it. */
@Composable
internal fun cloudProviderLabel(provider: BridgeCloudProvider): String =
    when (provider) {
        BridgeCloudProvider.S3 -> stringResource(R.string.cloud_provider_s3)
        BridgeCloudProvider.CLOUD_KIT -> stringResource(R.string.cloud_provider_icloud)
        BridgeCloudProvider.DROPBOX -> stringResource(R.string.cloud_provider_dropbox)
        BridgeCloudProvider.ONE_DRIVE -> stringResource(R.string.cloud_provider_onedrive)
    }
