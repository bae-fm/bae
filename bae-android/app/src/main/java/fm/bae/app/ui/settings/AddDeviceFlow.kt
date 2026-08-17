package fm.bae.app.ui.settings

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.components.QRCodeImage
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeDevicePairingSession
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePairingDevice

private const val TAG = "bae.AddDeviceFlow"
private val logger = BaeLogger(TAG)

private sealed interface AddDeviceStep {
    data object Starting : AddDeviceStep

    data class Waiting(
        val session: BridgeDevicePairingSession,
        val code: String,
    ) : AddDeviceStep

    data class Confirm(
        val session: BridgeDevicePairingSession,
        val device: BridgePairingDevice,
    ) : AddDeviceStep

    data object Approving : AddDeviceStep

    data object Cancelling : AddDeviceStep
}

private class AddDeviceModel(
    private val session: OpenLibrary,
    private val appContext: Context,
    private val onAdded: () -> Unit,
) {
    var step by mutableStateOf<AddDeviceStep>(AddDeviceStep.Starting)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    private var pairing: BridgeDevicePairingSession? = null
    private var completed = false

    suspend fun start() {
        try {
            val started = session.appHandle.startDevicePairing()
            pairing = started
            step = AddDeviceStep.Waiting(started, started.code())
            val device = started.waitForDevice()
            step = AddDeviceStep.Confirm(started, device)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to start device pairing", e)
            error = e.message ?: appContext.getString(R.string.members_pairing_failed)
        }
    }

    suspend fun retryWait() {
        val started = pairing ?: return
        error = null
        try {
            step = AddDeviceStep.Waiting(started, started.code())
            step = AddDeviceStep.Confirm(started, started.waitForDevice())
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed while waiting for pairing device", e)
            error = e.message ?: appContext.getString(R.string.members_pairing_failed)
        }
    }

    suspend fun approve(device: AddDeviceStep.Confirm) {
        error = null
        step = AddDeviceStep.Approving
        try {
            device.session.approve()
            completed = true
            pairing = null
            onAdded()
        } catch (_: BridgeException.Cancelled) {
            completed = true
            pairing = null
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to approve paired device", e)
            error = e.message ?: appContext.getString(R.string.members_pairing_failed)
            step = device
        }
    }

    suspend fun dismiss(): Boolean {
        if (completed) return true
        val active = pairing ?: return true
        val previousStep = step
        step = AddDeviceStep.Cancelling
        try {
            active.cancel()
            pairing = null
            return true
        } catch (e: Exception) {
            logger.error("Failed to cancel device pairing", e)
            error = e.message ?: appContext.getString(R.string.members_pairing_failed)
            step = previousStep
            return false
        }
    }
}

@Composable
fun AddDeviceSheet(
    session: OpenLibrary,
    onDismiss: () -> Unit,
    onInvited: () -> Unit,
) {
    val appContext = LocalContext.current.applicationContext
    val scope = rememberCoroutineScope()
    val model =
        remember {
            AddDeviceModel(session, appContext) {
                onInvited()
                onDismiss()
            }
        }

    LaunchedEffect(model) { model.start() }
    AlertDialog(
        onDismissRequest = {
            scope.launch {
                if (model.dismiss()) {
                    onDismiss()
                }
            }
        },
        title = { Text(stringResource(R.string.members_add_device)) },
        text = {
            AddDeviceContent(
                step = model.step,
                error = model.error,
                onRetry = { scope.launch { model.retryWait() } },
            )
        },
        confirmButton = {
            val confirm = model.step as? AddDeviceStep.Confirm
            if (confirm != null) {
                TextButton(onClick = { scope.launch { model.approve(confirm) } }) {
                    Text(stringResource(R.string.members_pairing_approve))
                }
            }
        },
        dismissButton = {
            TextButton(
                enabled = model.step != AddDeviceStep.Cancelling,
                onClick = {
                    scope.launch {
                        if (model.dismiss()) {
                            onDismiss()
                        }
                    }
                },
            ) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}

@Composable
private fun AddDeviceContent(
    step: AddDeviceStep,
    error: String?,
    onRetry: () -> Unit,
) {
    Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
        when (step) {
            AddDeviceStep.Starting -> PairingProgress(R.string.pairing_starting)
            is AddDeviceStep.Waiting -> PairingCode(step.code)
            is AddDeviceStep.Confirm -> PairingDevice(step.device)
            AddDeviceStep.Approving -> PairingProgress(R.string.members_pairing_approving)
            AddDeviceStep.Cancelling -> PairingProgress(R.string.core_pairing_cancelling)
        }
        error?.let {
            Spacer(modifier = Modifier.height(8.dp))
            Text(text = it, color = MaterialTheme.colorScheme.error)
            if (step is AddDeviceStep.Waiting) {
                TextButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
            }
        }
    }
}

@Composable
private fun PairingProgress(
    @StringRes label: Int,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        CircularProgressIndicator(modifier = Modifier.size(20.dp))
        Spacer(modifier = Modifier.width(12.dp))
        Text(stringResource(label))
    }
}

@Composable
private fun PairingCode(code: String) {
    val clipboard = LocalClipboardManager.current
    Text(
        text = stringResource(R.string.members_pairing_scan_instructions),
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Spacer(modifier = Modifier.height(12.dp))
    Column(modifier = Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        QRCodeImage(
            text = code,
            contentDescription = stringResource(R.string.members_pairing_scan_instructions),
            modifier = Modifier.size(220.dp),
        )
        TextButton(onClick = { clipboard.setText(AnnotatedString(code)) }) {
            Icon(Icons.Filled.ContentCopy, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(modifier = Modifier.width(6.dp))
            Text(stringResource(R.string.pairing_copy_code))
        }
        PairingProgress(R.string.members_pairing_waiting)
    }
}

@Composable
private fun PairingDevice(device: BridgePairingDevice) {
    Text(stringResource(R.string.members_pairing_confirm))
    Spacer(modifier = Modifier.height(12.dp))
    Text(device.fingerprint, style = MaterialTheme.typography.titleMedium, fontFamily = FontFamily.Monospace)
    device.email?.let {
        Spacer(modifier = Modifier.height(4.dp))
        Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
