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
internal fun AndroidS3KeyFields(
    accessKeyId: String,
    secretAccessKey: String,
    onAccessKeyIdChange: (String) -> Unit,
    onSecretAccessKeyChange: (String) -> Unit,
    secretNote: String? = null,
    accessKeyIdModifier: Modifier = Modifier,
    secretAccessKeyModifier: Modifier = Modifier,
) {
    AndroidTextField(
        value = accessKeyId,
        onValueChange = onAccessKeyIdChange,
        label = stringResource(R.string.s3_access_key_id),
        modifier = accessKeyIdModifier,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
    )
    AndroidTextField(
        value = secretAccessKey,
        onValueChange = onSecretAccessKeyChange,
        label = stringResource(R.string.s3_secret_access_key),
        modifier = secretAccessKeyModifier,
        kind = TextFieldKind.SECRET,
        helper = secretNote,
    )
}
