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

/** An S3 access key, as a restore and a storage setup both ask for it. */
@Composable
internal fun S3KeyFields(
    accessKeyId: String,
    secretAccessKey: String,
    onAccessKeyIdChange: (String) -> Unit,
    onSecretAccessKeyChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = accessKeyId,
        onValueChange = onAccessKeyIdChange,
        label = { Text(stringResource(R.string.s3_access_key_id)) },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
    )
    OutlinedTextField(
        value = secretAccessKey,
        onValueChange = onSecretAccessKeyChange,
        label = { Text(stringResource(R.string.s3_secret_access_key)) },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
    )
}

/** The name everyone in the library sees for the person. */
@Composable
internal fun PersonNameField(
    name: String,
    onNameChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = name,
        onValueChange = onNameChange,
        label = { Text(stringResource(R.string.onboarding_person_name)) },
        supportingText = { Text(stringResource(R.string.name_shown_to_everyone)) },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
    )
}
