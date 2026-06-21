package fm.bae.app.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import fm.bae.app.pubkeyFingerprint
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.decodeJoinRequest
import uniffi.bae_bridge.generateJoinRequest

private const val TAG = "bae.JoinLibraryScreen"
private val logger = BaeLogger(TAG)

/**
 * The joining device's side of adding itself to an existing library: it shows
 * this device's join-request code (a QR plus the copyable text and its
 * fingerprint) for an existing member to approve, and accepts the invite code
 * that member hands back (scan or paste). The join-request code is generated
 * once when the screen appears.
 */
@Composable
fun JoinLibraryScreen(
    joinLauncher: JoinLauncher,
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    onRequestScan: () -> Unit,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    var requestCode by remember { mutableStateOf<String?>(null) }
    var fingerprint by remember { mutableStateOf<String?>(null) }
    var generateError by remember { mutableStateOf<String?>(null) }
    var showPasteDialog by remember { mutableStateOf(false) }
    var pasteInput by remember { mutableStateOf("") }

    LaunchedEffect(Unit) {
        try {
            val code = withContext(Dispatchers.IO) { generateJoinRequest() }
            requestCode = code
            fingerprint = pubkeyFingerprint(withContext(Dispatchers.IO) { decodeJoinRequest(code) }.pubkey)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to generate join request", e)
            generateError = e.message ?: context.getString(R.string.onboarding_join_generate_failed)
        }
    }

    Column(
        modifier = Modifier.fillMaxSize().padding(32.dp).verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        JoinLibraryHeader()
        Spacer(modifier = Modifier.height(24.dp))
        JoinRequestCode(requestCode = requestCode, fingerprint = fingerprint, generateError = generateError)
        Spacer(modifier = Modifier.height(32.dp))
        JoinInviteEntry(
            error = joinLauncher.error,
            onScanInvite = onRequestScan,
            onPasteInvite = {
                joinLauncher.error = null
                pasteInput = ""
                showPasteDialog = true
            },
        )
        Spacer(modifier = Modifier.height(16.dp))
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }

    if (showPasteDialog) {
        PasteCodeDialog(
            text =
                PasteDialogText(
                    title = stringResource(R.string.onboarding_join_paste_invite),
                    instructions = stringResource(R.string.onboarding_join_paste_invite_instructions),
                    placeholder = stringResource(R.string.onboarding_invite_code_placeholder),
                    confirmLabel = stringResource(R.string.onboarding_join_action),
                ),
            pasteInput = pasteInput,
            onInputChange = { pasteInput = it },
            onConfirm = { code ->
                showPasteDialog = false
                joinLauncher.join(code, oauthLinking, oauthLinkingError)
            },
            onDismiss = { showPasteDialog = false },
        )
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

/**
 * The join-request code this device shows for approval: a QR plus its copyable
 * text and fingerprint once generated, the generate error if that failed, or a
 * spinner while it's still being generated.
 */
@Composable
private fun JoinRequestCode(
    requestCode: String?,
    fingerprint: String?,
    generateError: String?,
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
    } else {
        CircularProgressIndicator()
    }
}

/** Prompt and buttons for entering the invite code the approving member returns. */
@Composable
private fun JoinInviteEntry(
    error: String?,
    onScanInvite: () -> Unit,
    onPasteInvite: () -> Unit,
) {
    Text(
        text = stringResource(R.string.onboarding_join_enter_invite),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
    )
    Spacer(modifier = Modifier.height(12.dp))
    val buttonWidth = Modifier.width(220.dp)
    Button(onClick = onScanInvite, modifier = buttonWidth) {
        Text(stringResource(R.string.onboarding_join_scan_invite))
    }
    Spacer(modifier = Modifier.height(8.dp))
    OutlinedButton(onClick = onPasteInvite, modifier = buttonWidth) {
        Text(stringResource(R.string.onboarding_join_paste_invite))
    }
    error?.let {
        Spacer(modifier = Modifier.height(8.dp))
        Text(text = it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
    }
}
