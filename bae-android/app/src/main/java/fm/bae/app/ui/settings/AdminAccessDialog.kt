package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.S3KeyFields
import uniffi.bae_bridge.BridgeAdminAccess

/** The kinds of administrator credential a phone can paste. */
private enum class AdminCredential(
    @StringRes val label: Int,
) {
    ACCESS_KEY(R.string.admin_access_key),
    TOKEN(R.string.admin_access_token),
}

/** What the person typed for an administrator's credentials. */
private class AdminAccessInput {
    var credential by mutableStateOf(AdminCredential.ACCESS_KEY)
    var accessKeyId by mutableStateOf("")
    var secretAccessKey by mutableStateOf("")
    var sessionToken by mutableStateOf("")
    var iamEndpoint by mutableStateOf("")
    var token by mutableStateOf("")

    /** The credentials as typed, or null while what the chosen kind needs is incomplete. */
    fun access(): BridgeAdminAccess? =
        when (credential) {
            AdminCredential.ACCESS_KEY -> {
                if (accessKeyId.isEmpty() || secretAccessKey.isEmpty()) {
                    null
                } else {
                    BridgeAdminAccess.AccessKey(
                        accessKeyId = accessKeyId,
                        secretAccessKey = secretAccessKey,
                        sessionToken = sessionToken.takeIf { it.isNotEmpty() },
                        iamEndpoint = iamEndpoint.takeIf { it.isNotEmpty() },
                    )
                }
            }

            AdminCredential.TOKEN -> {
                token.takeIf { it.isNotEmpty() }?.let { BridgeAdminAccess.BearerToken(it) }
            }
        }
}

/**
 * Credentials that can manage the storage's access keys, for one operation
 * that needs them. bae hands them to that operation only and keeps none.
 */
@Composable
internal fun AdminAccessDialog(
    title: String,
    onSubmit: (BridgeAdminAccess) -> Unit,
    onDismiss: () -> Unit,
) {
    val input = remember { AdminAccessInput() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                Text(
                    text = stringResource(R.string.admin_access_explanation),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                AdminAccessFields(input)
            }
        },
        confirmButton = {
            val access = input.access()
            TextButton(onClick = { access?.let(onSubmit) }, enabled = access != null) {
                Text(stringResource(R.string.continue_action))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Composable
private fun AdminAccessFields(input: AdminAccessInput) {
    SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
        AdminCredential.entries.forEachIndexed { index, choice ->
            SegmentedButton(
                selected = input.credential == choice,
                onClick = { input.credential = choice },
                shape = SegmentedButtonDefaults.itemShape(index, AdminCredential.entries.size),
            ) {
                Text(stringResource(choice.label))
            }
        }
    }
    when (input.credential) {
        AdminCredential.ACCESS_KEY -> {
            S3KeyFields(
                accessKeyId = input.accessKeyId,
                secretAccessKey = input.secretAccessKey,
                onAccessKeyIdChange = { input.accessKeyId = it.trim() },
                onSecretAccessKeyChange = { input.secretAccessKey = it.trim() },
            )
            CredentialField(input.sessionToken, R.string.admin_access_session_token, secret = true) {
                input.sessionToken = it.trim()
            }
            CredentialField(input.iamEndpoint, R.string.admin_access_iam_endpoint, secret = false) {
                input.iamEndpoint = it.trim()
            }
        }

        AdminCredential.TOKEN -> {
            CredentialField(input.token, R.string.admin_access_token, secret = true) { input.token = it.trim() }
        }
    }
}

@Composable
private fun CredentialField(
    value: String,
    @StringRes label: Int,
    secret: Boolean,
    onValueChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(stringResource(label)) },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions =
            KeyboardOptions(
                keyboardType = if (secret) KeyboardType.Password else KeyboardType.Uri,
                autoCorrectEnabled = false,
            ),
    )
}

/**
 * An access key a removed device used still exists: delete it here with
 * administrator access, or say it was deleted in the provider's console.
 */
@Composable
internal fun DeleteAccessKeyDialog(
    accessKeyId: String,
    onDeleteWith: (BridgeAdminAccess) -> Unit,
    onDeletedElsewhere: () -> Unit,
    onDismiss: () -> Unit,
) {
    var askingForAccess by remember { mutableStateOf(false) }
    if (askingForAccess) {
        AdminAccessDialog(
            title = stringResource(R.string.delete_key_title),
            onSubmit = onDeleteWith,
            onDismiss = onDismiss,
        )
        return
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.delete_key_title)) },
        text = { Text(stringResource(R.string.delete_key_body, accessKeyId)) },
        confirmButton = {
            TextButton(onClick = { askingForAccess = true }) { Text(stringResource(R.string.delete_key_with_admin)) }
        },
        dismissButton = {
            TextButton(onClick = onDeletedElsewhere) { Text(stringResource(R.string.delete_key_done)) }
        },
    )
}

@Preview
@Composable
private fun AdminAccessDialogPreview() {
    BaeTheme {
        AdminAccessDialog(title = "Administrator access", onSubmit = {}, onDismiss = {})
    }
}
