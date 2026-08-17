package fm.bae.app.ui.settings

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.components.QRCodeImage
import fm.bae.app.ui.components.QRScannerScreen
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.bae_bridge.decodeJoinRequest
import java.util.Base64

private const val TAG = "bae.AddDeviceFlow"
private val logger = BaeLogger(TAG)

/** The owner's actions on the add-a-device dialog, dispatched by step. */
private class AddDeviceActions(
    val onScan: () -> Unit,
    val onPaste: () -> Unit,
    val onConfirm: (AddDeviceStep.Confirm) -> Unit,
    val onRetry: () -> Unit,
    val onDismiss: () -> Unit,
)

/** Where the add-a-device flow is in its sequence. */
private sealed interface AddDeviceStep {
    /** Capture the joining device's request code (scan or paste). */
    data object Capture : AddDeviceStep

    /** Confirm the fingerprint of the device about to be invited. */
    data class Confirm(
        val joinRequestCode: String,
        val fingerprint: String,
        val providerAccountEmail: String?,
    ) : AddDeviceStep

    /** Approving — calling the bridge to mint the invite code. */
    data object Inviting : AddDeviceStep

    /** Show the invite code to hand back to the joining device. */
    data class Invited(
        val invitationCode: String,
    ) : AddDeviceStep
}

/**
 * Drives the owner's approve-a-device flow: decode the captured request, create
 * its sealed invitation, and keep the owner-side join driver alive until the
 * new device finishes joining. The member list refreshes only after that join
 * completes.
 */
private class AddDeviceModel(
    private val session: OpenLibrary,
    private val appContext: Context,
    private val onInvited: () -> Unit,
) {
    var step by mutableStateOf<AddDeviceStep>(AddDeviceStep.Capture)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    private var invitation: ByteArray? = null
    private var dismissed = false

    fun clearError() {
        error = null
    }

    fun fail(message: String) {
        error = message
    }

    fun capture(code: String) {
        error = null
        try {
            val request = decodeJoinRequest(code)
            step = AddDeviceStep.Confirm(code, request.fingerprint, request.email)
        } catch (e: Exception) {
            logger.error("Failed to decode join request", e)
            error = e.message ?: appContext.getString(R.string.members_add_decode_failed)
        }
    }

    suspend fun approve(device: AddDeviceStep.Confirm) {
        step = AddDeviceStep.Inviting
        val created =
            try {
                session.appHandle.beginDeviceInvite(device.joinRequestCode)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.error("Failed to invite member ${device.fingerprint}", e)
                error = e.message ?: appContext.getString(R.string.members_invite_failed)
                step = AddDeviceStep.Capture
                return
            }
        invitation = created
        if (dismissed) {
            cancelInvitation(created)
            return
        }
        step = AddDeviceStep.Invited(Base64.getEncoder().encodeToString(created))
        drive(created)
    }

    suspend fun retry() {
        val created = invitation ?: return
        error = null
        drive(created)
    }

    suspend fun dismiss() {
        dismissed = true
        invitation?.let { cancelInvitation(it) }
    }

    private suspend fun drive(created: ByteArray) {
        try {
            session.appHandle.driveDeviceJoin(created)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            if (!dismissed) {
                logger.error("Failed while waiting for the invited device to join", e)
                error = e.message ?: appContext.getString(R.string.members_invite_failed)
            }
            return
        }
        if (!dismissed) onInvited()
    }

    private suspend fun cancelInvitation(created: ByteArray) {
        try {
            session.appHandle.cancelDeviceInvite(created)
        } catch (e: Exception) {
            logger.error("Failed to cancel device invitation", e)
        }
    }
}

/**
 * The owner's side of approving a device: scan or paste the joining device's
 * request code, confirm the two devices show the same fingerprint, approve it,
 * and present the sealed invitation while the owner waits for the joiner.
 * Reuses the shared [QRScannerScreen] for the capture step and the
 * camera-permission pattern from onboarding.
 */
@Composable
fun AddDeviceSheet(
    session: OpenLibrary,
    onDismiss: () -> Unit,
    onInvited: () -> Unit,
) {
    val appContext = LocalContext.current.applicationContext
    val scope = rememberCoroutineScope()
    val model = remember {
        AddDeviceModel(session, appContext) {
            onInvited()
            onDismiss()
        }
    }
    var showScanner by remember { mutableStateOf(false) }
    var showPasteDialog by remember { mutableStateOf(false) }
    var pasteInput by remember { mutableStateOf("") }

    val requestScan =
        rememberCameraScan(
            onError = { model.fail(it) },
            onRequest = {
                model.clearError()
                showScanner = true
            },
        )

    if (showScanner) {
        QRScannerScreen(
            onScanned = { code ->
                showScanner = false
                model.capture(code)
            },
            onDismiss = { showScanner = false },
            instructions = stringResource(R.string.qr_scanner_join_request_instructions),
        )
        return
    }

    AddDeviceDialog(
        step = model.step,
        error = model.error,
        actions =
            AddDeviceActions(
                onScan = requestScan,
                onPaste = {
                    model.clearError()
                    pasteInput = ""
                    showPasteDialog = true
                },
                onConfirm = { device -> scope.launch { model.approve(device) } },
                onRetry = { scope.launch { model.retry() } },
                onDismiss = {
                    scope.launch {
                        model.dismiss()
                        onDismiss()
                    }
                },
            ),
    )

    if (showPasteDialog) {
        PasteJoinRequestDialog(
            pasteInput = pasteInput,
            onInputChange = { pasteInput = it },
            onConfirm = { code ->
                showPasteDialog = false
                model.capture(code)
            },
            onDismiss = { showPasteDialog = false },
        )
    }
}

/**
 * Returns a callback that opens the QR scanner, first requesting camera
 * permission if it isn't already granted. [onRequest] runs once the camera is
 * available (synchronously when permission is already held, or after the grant);
 * [onError] receives the camera-permission-required message on denial.
 */
@Composable
private fun rememberCameraScan(
    onError: (String) -> Unit,
    onRequest: () -> Unit,
): () -> Unit {
    val context = LocalContext.current
    val permissionLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) {
                onRequest()
            } else {
                onError(context.getString(R.string.onboarding_camera_permission_required))
            }
        }
    val hasCameraPermission =
        ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
            PackageManager.PERMISSION_GRANTED
    return {
        if (hasCameraPermission) onRequest() else permissionLauncher.launch(Manifest.permission.CAMERA)
    }
}

@Composable
private fun AddDeviceDialog(
    step: AddDeviceStep,
    error: String?,
    actions: AddDeviceActions,
) {
    AlertDialog(
        onDismissRequest = actions.onDismiss,
        title = { Text(stringResource(R.string.members_add_device)) },
        text = { AddDeviceDialogContent(step = step, error = error) },
        confirmButton = { AddDeviceConfirmButton(step = step, error = error, actions = actions) },
        dismissButton = {
            TextButton(onClick = actions.onDismiss) {
                Text(stringResource(if (step is AddDeviceStep.Invited) R.string.close else R.string.cancel))
            }
        },
    )
}

@Composable
private fun AddDeviceDialogContent(
    step: AddDeviceStep,
    error: String?,
) {
    Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
        when (step) {
            AddDeviceStep.Capture -> {
                Text(
                    text = stringResource(R.string.members_add_capture_instructions),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            is AddDeviceStep.Confirm -> {
                Text(
                    text = stringResource(R.string.members_add_confirm_instructions),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(modifier = Modifier.height(12.dp))
                Text(
                    text = step.fingerprint,
                    style = MaterialTheme.typography.titleMedium,
                    fontFamily = FontFamily.Monospace,
                )
                // OAuth join-requests carry the account address the folder will be
                // shared to; S3 requests have no email, so this row is absent.
                step.providerAccountEmail?.let { email ->
                    Spacer(modifier = Modifier.height(4.dp))
                    Text(
                        text = email,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            AddDeviceStep.Inviting -> {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(modifier = Modifier.size(20.dp))
                    Spacer(modifier = Modifier.width(12.dp))
                    Text(stringResource(R.string.members_add_inviting))
                }
            }

            is AddDeviceStep.Invited -> {
                InviteCodeBlock(inviteCode = step.invitationCode)
            }
        }
        error?.let {
            Spacer(modifier = Modifier.height(8.dp))
            Text(text = it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun InviteCodeBlock(inviteCode: String) {
    val clipboard = LocalClipboardManager.current
    Text(
        text = stringResource(R.string.members_add_invited_instructions),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Spacer(modifier = Modifier.height(12.dp))
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        QRCodeImage(
            text = inviteCode,
            contentDescription = stringResource(R.string.members_add_invite_code_description),
            modifier = Modifier.size(200.dp),
        )
        Spacer(modifier = Modifier.height(8.dp))
        TextButton(onClick = { clipboard.setText(AnnotatedString(inviteCode)) }) {
            Icon(
                Icons.Filled.ContentCopy,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
            )
            Spacer(modifier = Modifier.width(6.dp))
            Text(stringResource(R.string.members_add_copy_invite))
        }
    }
}

@Composable
private fun AddDeviceConfirmButton(
    step: AddDeviceStep,
    error: String?,
    actions: AddDeviceActions,
) {
    when (step) {
        AddDeviceStep.Capture -> {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = actions.onPaste) { Text(stringResource(R.string.members_add_paste)) }
                TextButton(onClick = actions.onScan) { Text(stringResource(R.string.members_add_scan)) }
            }
        }

        is AddDeviceStep.Confirm -> {
            TextButton(onClick = { actions.onConfirm(step) }) {
                Text(stringResource(R.string.members_add_approve))
            }
        }

        AddDeviceStep.Inviting -> {}

        is AddDeviceStep.Invited ->
            if (error != null) {
                TextButton(onClick = actions.onRetry) { Text(stringResource(R.string.retry)) }
            }
    }
}

@Composable
private fun PasteJoinRequestDialog(
    pasteInput: String,
    onInputChange: (String) -> Unit,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.members_add_paste)) },
        text = {
            Column {
                Text(
                    text = stringResource(R.string.members_add_paste_instructions),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(modifier = Modifier.height(12.dp))
                OutlinedTextField(
                    value = pasteInput,
                    onValueChange = onInputChange,
                    placeholder = { Text(stringResource(R.string.members_add_paste_placeholder)) },
                    modifier = Modifier.fillMaxWidth(),
                    textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
                    singleLine = false,
                    maxLines = 3,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(pasteInput.trim()) }, enabled = pasteInput.trim().isNotEmpty()) {
                Text(stringResource(R.string.members_add_approve))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

private val previewAddDeviceActions =
    AddDeviceActions(onScan = {}, onPaste = {}, onConfirm = {}, onRetry = {}, onDismiss = {})

@Preview(showBackground = true)
@Composable
private fun AddDeviceDialogCapturePreview() {
    BaeTheme {
        AddDeviceDialog(step = AddDeviceStep.Capture, error = null, actions = previewAddDeviceActions)
    }
}

@Preview(showBackground = true)
@Composable
private fun AddDeviceDialogInvitedPreview() {
    BaeTheme {
        AddDeviceDialog(
            step = AddDeviceStep.Invited(invitationCode = "device-invitation-preview"),
            error = null,
            actions = previewAddDeviceActions,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun PasteJoinRequestDialogPreview() {
    BaeTheme {
        PasteJoinRequestDialog(
            pasteInput = "placeholder-request-code",
            onInputChange = {},
            onConfirm = {},
            onDismiss = {},
        )
    }
}
