package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import uniffi.bae_bridge.BridgeCloudProvider

/** Join by scanning the one pairing code displayed on an existing device. */
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
        Text(
            text = stringResource(R.string.onboarding_join_title),
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.Bold,
        )
        Spacer(modifier = Modifier.height(8.dp))
        Text(
            text = stringResource(R.string.onboarding_join_pairing_instructions),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(24.dp))

        OutlinedTextField(
            value = joinLauncher.pairingCode,
            onValueChange = {
                joinLauncher.updatePairingCode(it, oauthLinking, oauthLinkingError)
            },
            label = { Text(stringResource(R.string.pairing_code)) },
            placeholder = { Text(stringResource(R.string.pairing_code_placeholder)) },
            modifier = Modifier.fillMaxWidth(),
            textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
            minLines = 3,
            maxLines = 5,
        )
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedButton(onClick = onRequestScan, modifier = Modifier.width(220.dp)) {
            Text(stringResource(R.string.pairing_scan_code))
        }

        val decoded = joinLauncher.decodedOffer
        if (decoded != null) {
            Spacer(modifier = Modifier.height(20.dp))
            decoded.fold(
                onSuccess = { offer ->
                    PairingOfferRow(
                        label = stringResource(R.string.pairing_library),
                        value = offer.libraryName,
                    )
                    PairingOfferRow(
                        label = stringResource(R.string.pairing_provider),
                        value = cloudProviderLabel(offer.cloudProvider),
                    )
                },
                onFailure = {
                    Text(
                        text = stringResource(R.string.pairing_code_invalid),
                        color = MaterialTheme.colorScheme.error,
                    )
                },
            )
        }

        if (joinLauncher.isAuthorizing) {
            Spacer(modifier = Modifier.height(16.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                CircularProgressIndicator(modifier = Modifier.width(20.dp))
                Spacer(modifier = Modifier.width(12.dp))
                Text(stringResource(R.string.pairing_authorizing))
            }
        }
        if (joinLauncher.isJoining) {
            Spacer(modifier = Modifier.height(16.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                CircularProgressIndicator(modifier = Modifier.width(20.dp))
                Spacer(modifier = Modifier.width(12.dp))
                Text(
                    stringResource(
                        if (joinLauncher.joiningFingerprint == null) {
                            R.string.pairing_starting
                        } else {
                            R.string.pairing_waiting_for_approval
                        },
                    ),
                )
            }
            joinLauncher.joiningFingerprint?.let {
                Text(
                    text = stringResource(R.string.onboarding_join_fingerprint, it),
                    fontFamily = FontFamily.Monospace,
                )
            }
        }
        joinLauncher.error?.let {
            Spacer(modifier = Modifier.height(12.dp))
            Text(text = it, color = MaterialTheme.colorScheme.error)
        }

        Spacer(modifier = Modifier.height(24.dp))
        Button(
            onClick = joinLauncher::join,
            enabled = joinLauncher.joinReady,
            modifier = Modifier.width(220.dp),
        ) {
            Text(stringResource(R.string.onboarding_join_action))
        }
        Spacer(modifier = Modifier.height(8.dp))
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}

@Composable
private fun PairingOfferRow(
    label: String,
    value: String,
) {
    Row(modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
        Text(
            text = label,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(modifier = Modifier.width(16.dp))
        Text(text = value, modifier = Modifier.weight(1f), textAlign = TextAlign.End)
    }
}

@Composable
private fun cloudProviderLabel(provider: BridgeCloudProvider): String =
    when (provider) {
        BridgeCloudProvider.S3 -> stringResource(R.string.cloud_provider_s3)
        BridgeCloudProvider.CLOUD_KIT -> stringResource(R.string.cloud_provider_icloud)
        BridgeCloudProvider.GOOGLE_DRIVE -> stringResource(R.string.cloud_provider_google_drive)
        BridgeCloudProvider.DROPBOX -> stringResource(R.string.cloud_provider_dropbox)
        BridgeCloudProvider.ONE_DRIVE -> stringResource(R.string.cloud_provider_onedrive)
    }
