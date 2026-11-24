package fm.bae.app.ui.components

import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import fm.bae.app.R

/**
 * An S3 access key, as a restore, a storage setup and an invite all ask for it. [secretNote] is the line
 * under the secret key's field; [refused] puts the notice that S3 storage did not accept the key above the fields;
 * [accessKeyIdModifier] reaches the ID field, such as to focus it. With
 * [focusesNext], the field the person types in next takes the focus as the fields show: the ID while it is empty,
 * else the secret. Typing doesn't move it.
 */
@Composable
internal fun AndroidS3KeyFields(
    accessKeyId: String,
    secretAccessKey: String,
    onAccessKeyIdChange: (String) -> Unit,
    onSecretAccessKeyChange: (String) -> Unit,
    secretNote: String? = null,
    refused: Boolean = false,
    accessKeyIdModifier: Modifier = Modifier,
    focusesNext: Boolean = false,
) {
    val focusesId = remember { accessKeyId.isEmpty() }
    if (refused) {
        AndroidNotice(
            title = stringResource(R.string.s3_key_refused_title),
            advice = stringResource(R.string.s3_key_refused),
            error = null,
            look = NoticeLook.QUIET,
        )
    }
    AndroidTextField(
        value = accessKeyId,
        onValueChange = onAccessKeyIdChange,
        label = stringResource(R.string.s3_access_key_id),
        modifier = accessKeyIdModifier.then(if (focusesNext && focusesId) Modifier.focusOnShow() else Modifier),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
    )
    AndroidTextField(
        value = secretAccessKey,
        onValueChange = onSecretAccessKeyChange,
        label = stringResource(R.string.s3_secret_access_key),
        modifier = if (focusesNext && !focusesId) Modifier.focusOnShow() else Modifier,
        kind = TextFieldKind.SECRET,
        helper = secretNote,
    )
}
