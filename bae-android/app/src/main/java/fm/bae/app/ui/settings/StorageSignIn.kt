package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.NeutralTextButton
import fm.bae.app.ui.components.S3KeyFields
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeStorageLocation

/**
 * A new access key for the library's S3 bucket at [location], handed to [onSignIn] as that location's connection with
 * the key: for a device that lost its key, and for a pending record whose S3 key stopped working.
 */
@Composable
internal fun S3KeyDialog(
    location: BridgeStorageLocation.S3,
    onSignIn: (BridgeSaveSyncConfig) -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    var accessKeyId by remember { mutableStateOf("") }
    var secretAccessKey by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(context.coreString("core.pending.action.sign_in")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                Text(stringResource(R.string.onboarding_s3_key_explanation))
                S3KeyFields(
                    accessKeyId = accessKeyId,
                    secretAccessKey = secretAccessKey,
                    onAccessKeyIdChange = { accessKeyId = it.trim() },
                    onSecretAccessKeyChange = { secretAccessKey = it.trim() },
                )
            }
        },
        confirmButton = {
            TextButton(
                enabled = accessKeyId.isNotEmpty() && secretAccessKey.isNotEmpty(),
                onClick = { onSignIn(location.withKey(accessKeyId, secretAccessKey)) },
            ) {
                Text(context.coreString("core.pending.action.sign_in"))
            }
        },
        dismissButton = {
            NeutralTextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/** This S3 location's connection with a new access key. Setup checks it against the recorded location. */
private fun BridgeStorageLocation.S3.withKey(
    accessKeyId: String,
    secretAccessKey: String,
): BridgeSaveSyncConfig =
    BridgeSaveSyncConfig(
        bucket = bucket,
        region = region,
        endpoint = endpoint,
        accessKey = accessKeyId,
        secretKey = secretAccessKey,
        profile = profile,
    )
