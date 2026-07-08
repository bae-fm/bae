package fm.bae.app.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.availableCloudProviders

/**
 * The joining device's side of adding itself to an existing library. The flow
 * starts with a provider picker: picking the target library's provider drives
 * [JoinLauncher.selectProvider], which (for an OAuth provider) authenticates and
 * fetches the account email up front, then generates this device's join-request
 * with the email baked in. Once the code is ready this shows it (a QR plus the
 * copyable text and its fingerprint) for an existing member to approve, and
 * accepts the invite code that member hands back (scan or paste). The invite code
 * is decoded live into a Library / Provider / Owner preview, and Join enables only
 * once the code parses and this device can reach the library's provider. The
 * captured OAuth token is reused for the join — no second sign-in.
 */
@Composable
fun JoinLibraryScreen(
    joinLauncher: JoinLauncher,
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    onRequestScan: () -> Unit,
    onBack: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(32.dp).verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        JoinLibraryHeader()
        Spacer(modifier = Modifier.height(24.dp))

        if (joinLauncher.provider == null) {
            JoinProviderPicker(
                onSelect = { joinLauncher.selectProvider(it, oauthLinking, oauthLinkingError) },
            )
            Spacer(modifier = Modifier.height(16.dp))
            TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
        } else {
            JoinCodeExchange(
                joinLauncher = joinLauncher,
                onRequestScan = onRequestScan,
                // Back from the code step returns to the provider picker, dropping
                // the generated code and any captured token.
                onBack = { joinLauncher.resetProvider() },
            )
        }
    }
}

@Composable
private fun JoinLibraryHeader() {
    Text(
        text = stringResource(R.string.onboarding_join_title),
        style = MaterialTheme.typography.titleLarge,
        fontWeight = FontWeight.Bold,
    )
    Spacer(modifier = Modifier.height(8.dp))
    Text(
        text = stringResource(R.string.onboarding_join_instructions),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
    )
}

/** Step one: pick the cloud provider the target library uses. */
@Composable
private fun JoinProviderPicker(onSelect: (BridgeCloudProvider) -> Unit) {
    Text(
        text = stringResource(R.string.onboarding_join_pick_provider),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
    )
    Spacer(modifier = Modifier.height(16.dp))
    val buttonWidth = Modifier.width(220.dp)
    availableCloudProviders().forEach { provider ->
        Button(onClick = { onSelect(provider) }, modifier = buttonWidth) {
            Text(cloudProviderLabel(provider))
        }
        Spacer(modifier = Modifier.height(8.dp))
    }
}

/** Step two: show this device's code and take the invite code returned. */
@Composable
private fun JoinCodeExchange(
    joinLauncher: JoinLauncher,
    onRequestScan: () -> Unit,
    onBack: () -> Unit,
) {
    JoinRequestCode(
        requestCode = joinLauncher.requestCode,
        fingerprint = joinLauncher.fingerprint,
        generateError = joinLauncher.generateError,
        isAuthorizing = joinLauncher.isAuthorizing,
    )
    Spacer(modifier = Modifier.height(32.dp))
    JoinInviteEntry(
        input = joinLauncher.inviteInput,
        onInputChange = joinLauncher::updateInvite,
        preview = joinLauncher.invitePreview,
        error = joinLauncher.error,
        onScanInvite = onRequestScan,
    )
    Spacer(modifier = Modifier.height(16.dp))
    Button(
        onClick = { joinLauncher.join() },
        enabled = joinLauncher.joinReady,
        modifier = Modifier.width(220.dp),
    ) {
        Text(stringResource(R.string.onboarding_join_action))
    }
    Spacer(modifier = Modifier.height(8.dp))
    TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
}

/** The display name for a provider in the join picker. */
@Composable
private fun cloudProviderLabel(provider: BridgeCloudProvider): String =
    when (provider) {
        BridgeCloudProvider.GOOGLE_DRIVE -> stringResource(R.string.cloud_provider_google_drive)
        BridgeCloudProvider.DROPBOX -> stringResource(R.string.cloud_provider_dropbox)
        BridgeCloudProvider.ONE_DRIVE -> stringResource(R.string.cloud_provider_onedrive)
        BridgeCloudProvider.S3 -> stringResource(R.string.cloud_provider_s3)
        BridgeCloudProvider.CLOUD_KIT -> stringResource(R.string.cloud_provider_icloud)
    }

/**
 * The join-request code this device shows for approval: a QR plus its copyable
 * text and fingerprint once generated, the generate error if that failed, a
 * "signing in" spinner while authenticating with an OAuth provider, or a plain
 * spinner while the code is still being generated.
 */
@Composable
private fun JoinRequestCode(
    requestCode: String?,
    fingerprint: String?,
    generateError: String?,
    isAuthorizing: Boolean,
) {
    val clipboard = LocalClipboardManager.current
    if (requestCode != null) {
        QRCodeImage(
            text = requestCode,
            contentDescription = stringResource(R.string.onboarding_join_code_description),
            modifier = Modifier.size(220.dp),
        )
        Spacer(modifier = Modifier.height(12.dp))
        fingerprint?.let {
            Text(
                text = stringResource(R.string.onboarding_join_fingerprint, it),
                style = MaterialTheme.typography.bodyMedium,
                fontFamily = FontFamily.Monospace,
            )
        }
        Spacer(modifier = Modifier.height(4.dp))
        TextButton(onClick = { clipboard.setText(AnnotatedString(requestCode)) }) {
            Icon(Icons.Filled.ContentCopy, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(modifier = Modifier.width(6.dp))
            Text(stringResource(R.string.onboarding_join_copy_code))
        }
    } else if (generateError != null) {
        Text(text = generateError, color = MaterialTheme.colorScheme.error)
    } else if (isAuthorizing) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            CircularProgressIndicator(modifier = Modifier.size(20.dp))
            Spacer(modifier = Modifier.width(12.dp))
            Text(
                text = stringResource(R.string.onboarding_join_signing_in),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    } else {
        CircularProgressIndicator()
    }
}

/**
 * The invite-code field and its live preview. The code is decoded as it's typed
 * or scanned; a valid code shows the library, provider, and owner it points at,
 * and — when this device can't reach that provider — a mismatch notice. An
 * unparseable code shows an inline invalid-code notice.
 */
@Composable
private fun JoinInviteEntry(
    input: String,
    onInputChange: (String) -> Unit,
    preview: JoinInvitePreview,
    error: String?,
    onScanInvite: () -> Unit,
) {
    Text(
        text = stringResource(R.string.onboarding_join_enter_invite),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
    )
    Spacer(modifier = Modifier.height(12.dp))
    OutlinedTextField(
        value = input,
        onValueChange = onInputChange,
        placeholder = { Text(stringResource(R.string.onboarding_invite_code_placeholder)) },
        modifier = Modifier.fillMaxWidth(),
        textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
        singleLine = true,
    )
    Spacer(modifier = Modifier.height(8.dp))
    OutlinedButton(onClick = onScanInvite, modifier = Modifier.width(220.dp)) {
        Text(stringResource(R.string.onboarding_join_scan_invite))
    }
    Spacer(modifier = Modifier.height(12.dp))
    InvitePreviewContent(preview)
    error?.let {
        Spacer(modifier = Modifier.height(8.dp))
        Text(text = it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
    }
}

/** The rows shown under the invite field once a code is entered. */
@Composable
private fun InvitePreviewContent(preview: JoinInvitePreview) {
    when (preview) {
        JoinInvitePreview.Empty -> {
            Unit
        }

        JoinInvitePreview.Invalid -> {
            Text(
                text = stringResource(R.string.onboarding_invite_invalid),
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodySmall,
            )
        }

        is JoinInvitePreview.Decoded -> {
            InvitePreviewRow(
                label = stringResource(R.string.onboarding_invite_library),
                value = preview.info.libraryName,
            )
            InvitePreviewRow(
                label = stringResource(R.string.onboarding_invite_provider),
                value = cloudProviderLabel(preview.info.cloudProvider),
            )
            InvitePreviewRow(
                label = stringResource(R.string.onboarding_invite_owner),
                value = preview.info.ownerFingerprint,
                monospaceValue = true,
            )
            preview.mismatch?.let { mismatch ->
                val message =
                    when (mismatch) {
                        ProviderMismatch.PROVIDER_NOT_SELECTED -> {
                            stringResource(R.string.onboarding_invite_provider_mismatch)
                        }

                        ProviderMismatch.OAUTH_UNSUPPORTED -> {
                            stringResource(R.string.onboarding_invite_provider_unsupported)
                        }
                    }
                Spacer(modifier = Modifier.height(8.dp))
                Text(
                    text = message,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}

/** One label/value line in the invite preview. */
@Composable
private fun InvitePreviewRow(
    label: String,
    value: String,
    monospaceValue: Boolean = false,
) {
    Row(modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(modifier = Modifier.width(16.dp))
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            fontFamily = if (monospaceValue) FontFamily.Monospace else null,
            modifier = Modifier.weight(1f),
            textAlign = TextAlign.End,
        )
    }
}

/** The OAuth providers, for which the joiner authenticates before generating its code. */
internal fun providerUsesOauth(provider: BridgeCloudProvider): Boolean =
    when (provider) {
        BridgeCloudProvider.GOOGLE_DRIVE,
        BridgeCloudProvider.DROPBOX,
        BridgeCloudProvider.ONE_DRIVE,
        -> true

        BridgeCloudProvider.S3,
        BridgeCloudProvider.CLOUD_KIT,
        -> false
    }
