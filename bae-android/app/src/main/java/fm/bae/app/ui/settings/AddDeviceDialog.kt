package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
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
import androidx.compose.runtime.DisposableEffect
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.displayError
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.QRCodeImage
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AddedDeviceCallback
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRestoreCodeShare

private val logger = BaeLogger("bae.AddDeviceDialog")

/**
 * Adds another of this person's devices: their recovery code, generated on
 * demand, as a QR the new device scans and as text to type there instead.
 * While it shows, it watches the person's devices, and once a new one joins it
 * says so in place of the code.
 */
@Composable
internal fun AddDeviceDialog(
    appHandle: AppHandleInterface,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var share by remember { mutableStateOf<BridgeRestoreCodeShare?>(null) }
    var added by remember { mutableStateOf<BridgeActiveDevice?>(null) }
    var error by remember { mutableStateOf<DisplayError?>(null) }

    fun show(failure: BridgeException) {
        error = context.displayError(failure) ?: DisplayError(failure.toString())
    }

    DisposableEffect(appHandle) {
        val watch =
            appHandle.subscribeAddedDevice(
                object : AddedDeviceCallback {
                    override fun onValue(device: BridgeActiveDevice?) {
                        scope.launch(Dispatchers.Main.immediate) { added = device }
                    }

                    override fun onError(error: BridgeException) {
                        logger.error("Watching for the added device failed", error)
                        scope.launch(Dispatchers.Main.immediate) { show(error) }
                    }
                },
            )
        onDispose { watch.cancel() }
    }

    LaunchedEffect(Unit) {
        try {
            share = appHandle.generateRestoreCode()
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("Failed to generate the add-device code", e)
            show(e)
        }
    }

    AddDeviceDialogContent(share = share, added = added, error = error, onDismiss = onDismiss)
}

/**
 * The dialog body: the device that joined, [added], once one has; until then
 * the code in [share] to scan or type, a spinner while it generates, and any
 * [error].
 */
@Composable
internal fun AddDeviceDialogContent(
    share: BridgeRestoreCodeShare?,
    added: BridgeActiveDevice?,
    error: DisplayError?,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.add_device_title)) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                when {
                    added != null -> {
                        Text(
                            text = stringResource(R.string.add_device_added, added.name),
                            style = ThemeText.heading.style,
                            textAlign = TextAlign.Center,
                        )
                    }

                    share != null -> {
                        AddDeviceShareContent(share)
                    }

                    error == null -> {
                        CircularProgressIndicator(modifier = Modifier.size(ThemeIcon.large))
                    }
                }
                error?.let {
                    Spacer(modifier = Modifier.height(ThemeSpace.related))
                    ErrorDetailDisclosure(it)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.done)) }
        },
    )
}

/**
 * The QR, whose code holds the S3 key when this device has one, then the
 * code to type on the new device and copy.
 */
@Composable
private fun AddDeviceShareContent(share: BridgeRestoreCodeShare) {
    val clipboard = LocalClipboardManager.current
    QRCodeImage(
        text = share.qrText,
        contentDescription = stringResource(R.string.recovery_code_title),
    )
    Spacer(modifier = Modifier.height(ThemeSpace.group))
    Text(
        text = stringResource(R.string.add_device_instructions),
        style = ThemeText.body.style,
        textAlign = TextAlign.Center,
    )
    Spacer(modifier = Modifier.height(ThemeSpace.related))
    SelectionContainer {
        Text(text = share.code, style = ThemeText.mono.style, textAlign = TextAlign.Center)
    }
    TextButton(onClick = { clipboard.setText(AnnotatedString(share.code)) }) {
        Icon(
            Icons.Filled.ContentCopy,
            contentDescription = null,
            modifier = Modifier.size(ThemeIcon.medium),
        )
        Spacer(modifier = Modifier.width(ThemeSpace.compact))
        Text(stringResource(R.string.copy))
    }
    if (share.qrCarriesS3Key) {
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        Text(
            text = stringResource(R.string.restore_code_carries_s3_key),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun AddDeviceDialogContentReadyPreview() {
    BaeTheme {
        AddDeviceDialogContent(
            share =
                BridgeRestoreCodeShare(
                    code = "CVR1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
                    qrText = "CVR1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOP",
                    qrCarriesS3Key = true,
                ),
            added = null,
            error = null,
            onDismiss = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun AddDeviceDialogContentAddedPreview() {
    BaeTheme {
        AddDeviceDialogContent(
            share = null,
            added = BridgeActiveDevice(id = 7u, name = "New Phone", isSelf = false, canRemove = true),
            error = null,
            onDismiss = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun AddDeviceDialogContentLoadingPreview() {
    BaeTheme {
        AddDeviceDialogContent(share = null, added = null, error = null, onDismiss = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun AddDeviceDialogContentFailedPreview() {
    BaeTheme {
        AddDeviceDialogContent(
            share = null,
            added = null,
            error =
                DisplayError(
                    "Couldn’t reach your storage.",
                    "S3: Network: dispatch failure: io error: connection refused",
                ),
            onDismiss = {
            },
        )
    }
}
