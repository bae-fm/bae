package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.CancellationException

private val logger = BaeLogger("bae.RecoveryCodeDialog")

/**
 * Reveals the library's recovery code on demand. The code is a bearer secret —
 * anyone holding it can restore the whole library — so it is generated only when
 * the user asks and labelled as a secret, never offered as an add-a-device step
 * (devices are added through the membership chain in the Devices screen).
 */
@Composable
internal fun RecoveryCodeDialog(
    session: OpenLibrary,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    var code by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) {
        try {
            code = session.appHandle.generateRestoreCode()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to generate recovery code", e)
            error = e.message ?: context.getString(R.string.settings_recovery_code_failed)
        }
    }

    RecoveryCodeDialogContent(code = code, error = error, onDismiss = onDismiss)
}

/**
 * The dialog body: the secret warning plus the resolved [code], its [error], or a
 * spinner while it is being generated. Prop-driven so each state renders without a
 * session.
 */
@Composable
private fun RecoveryCodeDialogContent(
    code: String?,
    error: String?,
    onDismiss: () -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_recovery_code)) },
        text = {
            Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                Text(
                    text = stringResource(R.string.settings_recovery_code_secret_warning),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
                Spacer(modifier = Modifier.height(12.dp))
                val current = code
                when {
                    current != null -> {
                        Text(
                            text = current,
                            style = MaterialTheme.typography.bodyMedium,
                            fontFamily = FontFamily.Monospace,
                        )
                    }

                    error != null -> {
                        Text(text = error!!, color = MaterialTheme.colorScheme.error)
                    }

                    else -> {
                        CircularProgressIndicator(modifier = Modifier.size(24.dp))
                    }
                }
            }
        },
        confirmButton = {
            val current = code
            if (current != null) {
                TextButton(onClick = { clipboard.setText(AnnotatedString(current)) }) {
                    Icon(Icons.Filled.ContentCopy, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(stringResource(R.string.settings_copy_recovery_code))
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}

@Preview(showBackground = true)
@Composable
private fun RecoveryCodeDialogContentReadyPreview() {
    BaeTheme {
        RecoveryCodeDialogContent(code = "AAAA-BBBB-CCCC-DDDD-EEEE", error = null, onDismiss = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun RecoveryCodeDialogContentLoadingPreview() {
    BaeTheme {
        RecoveryCodeDialogContent(code = null, error = null, onDismiss = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun RecoveryCodeDialogContentErrorPreview() {
    BaeTheme {
        RecoveryCodeDialogContent(code = null, error = "Could not generate a recovery code.", onDismiss = {})
    }
}
