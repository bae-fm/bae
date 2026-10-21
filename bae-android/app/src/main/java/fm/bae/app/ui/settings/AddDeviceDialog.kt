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
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.displayError
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.QRCodeImage
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRestoreCodeShare

private val logger = BaeLogger("bae.AddDeviceDialog")

private val qrSize = 220.dp

/**
 * Adds another of this person's devices: their recovery code, generated on
 * demand, as a QR the new device scans and as text to type there instead.
 */
@Composable
internal fun AddDeviceDialog(
    session: OpenLibrary,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    var share by remember { mutableStateOf<BridgeRestoreCodeShare?>(null) }
    var error by remember { mutableStateOf<DisplayError?>(null) }

    LaunchedEffect(Unit) {
        try {
            share = session.appHandle.generateRestoreCode()
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("Failed to generate the add-device code", e)
            error = context.displayError(e) ?: DisplayError(e.toString())
        }
    }

    AddDeviceDialogContent(share = share, error = error, onDismiss = onDismiss)
}

/** The dialog body: the code in [share] to scan or type, its [error], or a spinner. */
@Composable
internal fun AddDeviceDialogContent(
    share: BridgeRestoreCodeShare?,
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
                    share != null -> {
                        AddDeviceShareContent(share)
                    }

                    error != null -> {
                        ErrorDetailDisclosure(error)
                    }

                    else -> {
                        CircularProgressIndicator(modifier = Modifier.size(ThemeIcon.large))
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.done)) }
        },
    )
}

/**
 * The QR, which carries the S3 key beside the code when this device holds
 * one, then the code to type on the new device and copy.
 */
@Composable
private fun AddDeviceShareContent(share: BridgeRestoreCodeShare) {
    val clipboard = LocalClipboardManager.current
    QRCodeImage(
        text = share.qrText,
        contentDescription = stringResource(R.string.recovery_code_title),
        modifier = Modifier.size(qrSize),
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
                    qrText = "CVR1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567.key-preview",
                    qrCarriesS3Key = true,
                ),
            error = null,
            onDismiss = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun AddDeviceDialogContentLoadingPreview() {
    BaeTheme {
        AddDeviceDialogContent(share = null, error = null, onDismiss = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun AddDeviceDialogContentFailedPreview() {
    BaeTheme {
        AddDeviceDialogContent(
            share = null,
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
