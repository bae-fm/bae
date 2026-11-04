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
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.OAuthProviders
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup

/**
 * Sign In to the library's recorded storage, for a device that no longer holds its credentials for it: a new access
 * key for its S3 bucket, or the provider's sign-in for Dropbox and OneDrive. It connects that location again and never
 * sets up other storage. iCloud isn't reachable from Android, so an iCloud library offers nothing here.
 */
@Composable
internal fun StorageSignIn(
    session: OpenLibrary,
    location: BridgeStorageLocation,
    oauthProviders: OAuthProviders?,
) {
    val context = LocalContext.current
    val resources = LocalResources.current
    val scope = rememberScreenScope()
    var error by remember(session) { mutableStateOf<DisplayError?>(null) }
    var askingForKey by remember(session) { mutableStateOf(false) }
    var busy by remember(session) { mutableStateOf(false) }
    // The status follows the reconnect on its own: Settings shows the line again once it lands.
    val runner = rememberStorageSetupRunner(session, onError = { error = it }, onSetUp = {})

    fun signIn(
        operation: String,
        setUp: suspend (deviceName: String, setup: BridgeLibrarySetup) -> BridgeStorageSetup,
    ) {
        busy = true
        error = null
        scope.launch {
            runner.run(operation, setUp)
            busy = false
        }
    }

    val start: () -> Unit =
        when (location) {
            is BridgeStorageLocation.S3 -> {
                { askingForKey = true }
            }

            is BridgeStorageLocation.Dropbox, is BridgeStorageLocation.OneDrive -> {
                {
                    if (oauthProviders == null) {
                        error = DisplayError(resources.getString(R.string.oauth_unconfigured))
                    } else {
                        signIn("sign in to storage again") { deviceName, setup ->
                            oauthProviders.signInAgain(session.appHandle, deviceName, setup)
                        }
                    }
                }
            }

            is BridgeStorageLocation.CloudKit -> {
                return
            }
        }
    PrimaryButton(enabled = !busy, onClick = start) {
        Text(context.coreString("core.pending.action.sign_in"))
    }
    runner.steps?.let { StorageSetupLine(it) }
    error?.let { ErrorDetailDisclosure(it) }
    if (askingForKey && location is BridgeStorageLocation.S3) {
        S3KeyDialog(
            location = location,
            onSignIn = { config ->
                askingForKey = false
                signIn("sign in to S3 with a new access key") { deviceName, setup ->
                    session.appHandle.saveSyncConfig(config, deviceName, setup)
                }
            },
            onDismiss = { askingForKey = false },
        )
    }
}

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
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
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
        keyPrefix = prefix,
        accessKey = accessKeyId,
        secretKey = secretAccessKey,
        profile = profile,
    )
