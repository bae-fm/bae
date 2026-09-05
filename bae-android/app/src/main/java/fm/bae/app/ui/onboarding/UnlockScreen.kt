package fm.bae.app.ui.onboarding

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.components.PrimaryButton
import kotlinx.coroutines.launch

private const val HEX_KEY_LENGTH = 64

/**
 * Fallback unlock flow when the encryption key isn't in the keyring. The v1
 * happy path stores the key during `restoreFromCode`, so this rarely triggers,
 * but it's the recovery path when the keyring is wiped.
 */
private data class UnlockCallbacks(
    val onKeyHexChange: (String) -> Unit,
    val onUnlock: () -> Unit,
    val onCancel: () -> Unit,
)

@Composable
fun UnlockScreen(
    libraryName: String,
    onUnlock: suspend (String) -> Unit,
    onCancel: () -> Unit,
) {
    var keyHex by remember { mutableStateOf("") }
    var isUnlocking by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val appContext = LocalContext.current
    BackHandler { onCancel() }
    UnlockForm(
        libraryName = libraryName,
        keyHex = keyHex,
        isUnlocking = isUnlocking,
        error = error,
        callbacks =
            UnlockCallbacks(
                onKeyHexChange = { keyHex = it.trim() },
                onUnlock = {
                    isUnlocking = true
                    error = null
                    scope.launch {
                        try {
                            onUnlock(keyHex)
                            isUnlocking = false
                        } catch (e: Exception) {
                            isUnlocking = false
                            error = e.message ?: appContext.getString(R.string.unlock_failed)
                        }
                    }
                },
                onCancel = onCancel,
            ),
    )
}

@Composable
private fun UnlockForm(
    libraryName: String,
    keyHex: String,
    isUnlocking: Boolean,
    error: String?,
    callbacks: UnlockCallbacks,
) {
    val isValidHex = keyHex.length == HEX_KEY_LENGTH && keyHex.all { it.isDigit() || it in 'a'..'f' || it in 'A'..'F' }
    Column(
        modifier = Modifier.fillMaxSize().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(stringResource(R.string.unlock_title), style = MaterialTheme.typography.headlineSmall)
        Spacer(modifier = Modifier.height(8.dp))
        Text(
            libraryName,
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(modifier = Modifier.height(24.dp))
        Text(
            text = stringResource(R.string.unlock_explanation),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(24.dp))
        OutlinedTextField(
            value = keyHex,
            onValueChange = callbacks.onKeyHexChange,
            label = { Text(stringResource(R.string.unlock_key_label)) },
            modifier = Modifier.fillMaxWidth(),
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
        )
        Spacer(modifier = Modifier.height(16.dp))
        PrimaryButton(onClick = callbacks.onUnlock, enabled = isValidHex && !isUnlocking) {
            if (isUnlocking) {
                CircularProgressIndicator(modifier = Modifier.height(20.dp))
            } else {
                Text(stringResource(R.string.unlock_action))
            }
        }
        Spacer(modifier = Modifier.height(8.dp))
        TextButton(onClick = callbacks.onCancel) {
            Text(stringResource(R.string.cancel))
        }
        if (error != null) {
            Spacer(modifier = Modifier.height(16.dp))
            Text(text = error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun UnlockFormPreview() {
    BaeTheme {
        UnlockForm(
            libraryName = "Library Name",
            keyHex = "",
            isUnlocking = false,
            error = null,
            callbacks = UnlockCallbacks(onKeyHexChange = {}, onUnlock = {}, onCancel = {}),
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun UnlockFormErrorPreview() {
    BaeTheme {
        UnlockForm(
            libraryName = "Library Name",
            keyHex = "abc123",
            isUnlocking = false,
            error = "That key didn't unlock the library.",
            callbacks = UnlockCallbacks(onKeyHexChange = {}, onUnlock = {}, onCancel = {}),
        )
    }
}
