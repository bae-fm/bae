package fm.bae.app.ui.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import fm.bae.app.R

/**
 * An S3 access key, as a restore, a storage setup and an invite all ask for it. [secretNote] is the line
 * under the secret key's field; [accessKeyIdModifier] and [secretAccessKeyModifier] reach each field, such as to
 * focus it.
 */
@Composable
internal fun S3KeyFields(
    accessKeyId: String,
    secretAccessKey: String,
    onAccessKeyIdChange: (String) -> Unit,
    onSecretAccessKeyChange: (String) -> Unit,
    secretNote: String? = null,
    accessKeyIdModifier: Modifier = Modifier,
    secretAccessKeyModifier: Modifier = Modifier,
) {
    OutlinedTextField(
        value = accessKeyId,
        onValueChange = onAccessKeyIdChange,
        label = { Text(stringResource(R.string.s3_access_key_id)) },
        modifier = accessKeyIdModifier.fillMaxWidth(),
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
    )
    OutlinedTextField(
        value = secretAccessKey,
        onValueChange = onSecretAccessKeyChange,
        label = { Text(stringResource(R.string.s3_secret_access_key)) },
        supportingText = secretNote?.let { note -> { Text(note) } },
        modifier = secretAccessKeyModifier.fillMaxWidth(),
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
    )
}
