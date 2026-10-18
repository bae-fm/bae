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
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.localizedLine
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.QRCodeImage
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRestoreCodeShare

private val logger = BaeLogger("bae.RestoreCodeDialog")

private val qrSize = 220.dp

/**
 * This person's restore code, generated on demand, as a QR another of their
 * devices scans and as text to type. It holds the person's keys: anyone
 * holding it can add a device as them.
 */
@Composable
internal fun RestoreCodeDialog(
    session: OpenLibrary,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    var share by remember { mutableStateOf<BridgeRestoreCodeShare?>(null) }
    var error by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) {
        try {
            share = session.appHandle.generateRestoreCode()
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("Failed to generate the restore code", e)
            error = context.localizedLine(e) ?: e.toString()
        }
    }

    RestoreCodeDialogContent(share = share, error = error, onDismiss = onDismiss)
}

/**
 * The dialog body: the secret warning plus the code in [share], its [error],
 * or a spinner. The QR carries the S3 key beside the code when this device
 * holds one.
 */
@Composable
internal fun RestoreCodeDialogContent(
    share: BridgeRestoreCodeShare?,
    error: String?,
    onDismiss: () -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_restore_code)) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    text = stringResource(R.string.restore_code_secret_warning),
                    style = ThemeText.body.style,
                    color = BaeTheme.colors.warning,
                )
                Spacer(modifier = Modifier.height(ThemeSpace.group))
                when {
                    share != null -> {
                        RestoreCodeShareContent(share)
                    }

                    error != null -> {
                        ErrorText(error)
                    }

                    else -> {
                        CircularProgressIndicator(modifier = Modifier.size(ThemeIcon.large))
                    }
                }
            }
        },
        confirmButton = {
            if (share != null) {
                TextButton(onClick = { clipboard.setText(AnnotatedString(share.code)) }) {
                    Icon(
                        Icons.Filled.ContentCopy,
                        contentDescription = null,
                        modifier = Modifier.size(ThemeIcon.medium),
                    )
                    Spacer(modifier = Modifier.width(ThemeSpace.compact))
                    Text(stringResource(R.string.copy_code))
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}

/** The QR, which carries the S3 key beside the code when this device holds one, and the code to type. */
@Composable
private fun RestoreCodeShareContent(share: BridgeRestoreCodeShare) {
    QRCodeImage(
        text = share.qrText,
        contentDescription = stringResource(R.string.settings_restore_code),
        modifier = Modifier.size(qrSize),
    )
    Spacer(modifier = Modifier.height(ThemeSpace.group))
    SelectionContainer {
        Text(text = share.code, style = ThemeText.mono.style)
    }
    if (share.qrCarriesS3Key) {
        Spacer(modifier = Modifier.height(ThemeSpace.group))
        Text(
            text = stringResource(R.string.restore_code_carries_s3_key),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun RestoreCodeDialogContentReadyPreview() {
    BaeTheme {
        RestoreCodeDialogContent(
            share =
                BridgeRestoreCodeShare(
                    code = "restore-code-preview",
                    qrText = "restore-code-preview.key-preview",
                    qrCarriesS3Key = true,
                ),
            error = null,
            onDismiss = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun RestoreCodeDialogContentLoadingPreview() {
    BaeTheme {
        RestoreCodeDialogContent(share = null, error = null, onDismiss = {})
    }
}
