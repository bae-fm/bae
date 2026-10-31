package fm.bae.app.ui.onboarding

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.LabeledValueRow
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.storageLocationLabel
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeS3Credentials

/**
 * What a restore says on its form and while it runs: a code entered on a new
 * device adds this device to the person's own library; a code saved on this
 * phone, and a restore over this device's copy, restore the library.
 */
internal class RestoreText(
    @StringRes val title: Int,
    @StringRes val action: Int,
    @StringRes val running: Int,
) {
    companion object {
        val ADD =
            RestoreText(
                title = R.string.onboarding_add_device,
                action = R.string.onboarding_add_action,
                running = R.string.onboarding_adding_device,
            )

        val RESTORE =
            RestoreText(
                title = R.string.onboarding_restore_title,
                action = R.string.onboarding_restore_action,
                running = R.string.onboarding_restoring_title,
            )

        /** What a code entered for [entry] says. */
        fun of(entry: CodeEntry): RestoreText =
            when (entry) {
                CodeEntry.NewDevice -> ADD
                is RestoreOver -> RESTORE
            }
    }
}

/**
 * The person's own restore code, read: S3 asks for the bucket's access key;
 * the account providers sign in as the restore runs.
 */
@Composable
internal fun RestoreForm(
    step: OnboardingStep.Restore,
    error: DisplayError?,
    onRestore: (BridgeS3Credentials?) -> Unit,
    onBack: () -> Unit,
) {
    val text = RestoreText.of(step.entry)
    OnboardingForm(title = stringResource(text.title), error = error, onBack = onBack) {
        LabeledValueRow(stringResource(R.string.settings_stored_in), storageLocationLabel(step.metadata.location))
        if (step.metadata.provider == BridgeCloudProvider.S3 && step.metadata.carriesS3Key) {
            Text(
                text = stringResource(R.string.onboarding_s3_key_scanned),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        RestoreAction(needsKey = step.needsS3Key, action = text.action, onRestore = onRestore)
    }
}

/**
 * A restore code saved on this phone, picked from the welcome's list: the
 * library's saved name and where it is stored, with the S3 key fields when no
 * key is saved beside the code. A failed restore returns here, to try again.
 */
@Composable
internal fun SavedRestoreForm(
    step: OnboardingStep.RestoreSaved,
    error: DisplayError?,
    onRestore: (BridgeS3Credentials?) -> Unit,
    onBack: () -> Unit,
) {
    val info = step.saved.info
    val text = RestoreText.RESTORE
    OnboardingForm(title = stringResource(text.title), error = error, onBack = onBack) {
        LabeledValueRow(stringResource(R.string.onboarding_saved_library), info.name)
        LabeledValueRow(stringResource(R.string.settings_stored_in), storageLocationLabel(info.location))
        RestoreAction(needsKey = step.saved.needsS3Key, action = text.action, onRestore = onRestore)
    }
}

/**
 * A restore form's action, under the S3 access key fields when the restore
 * needs the key typed in.
 */
@Composable
private fun RestoreAction(
    needsKey: Boolean,
    @StringRes action: Int,
    onRestore: (BridgeS3Credentials?) -> Unit,
) {
    var accessKeyId by remember { mutableStateOf("") }
    var secretAccessKey by remember { mutableStateOf("") }
    if (needsKey) {
        Text(
            text = stringResource(R.string.onboarding_s3_key_explanation),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        S3KeyFields(
            accessKeyId = accessKeyId,
            secretAccessKey = secretAccessKey,
            onAccessKeyIdChange = { accessKeyId = it.trim() },
            onSecretAccessKeyChange = { secretAccessKey = it.trim() },
        )
    }
    PrimaryButton(
        onClick = {
            onRestore(if (needsKey) BridgeS3Credentials(accessKeyId, secretAccessKey) else null)
        },
        enabled = !needsKey || (accessKeyId.isNotEmpty() && secretAccessKey.isNotEmpty()),
        modifier = Modifier.width(actionWidth),
    ) {
        Text(stringResource(action))
    }
}

@Composable
private fun OnboardingForm(
    title: String,
    error: DisplayError?,
    onBack: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.page).verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.group),
    ) {
        Text(text = title, style = ThemeText.title.style, textAlign = TextAlign.Center)
        content()
        error?.let { ErrorDetailDisclosure(it) }
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}
