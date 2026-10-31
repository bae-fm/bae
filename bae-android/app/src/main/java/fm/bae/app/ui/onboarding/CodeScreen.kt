package fm.bae.app.ui.onboarding

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.core.content.ContextCompat
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.PrimaryButton

private const val TAG = "bae.CodeScreen"
private val logger = BaeLogger(TAG)

/** What a code screen says above its actions, and where Back leaves the person. */
internal class CodeScreenText(
    val title: String,
    val message: String,
    val onBack: () -> Unit,
)

private class CodeScreenCallbacks(
    val onScan: () -> Unit,
    val onShowCodeDialog: () -> Unit,
    val onCodeInputChange: (String) -> Unit,
    val onEnterCode: (String) -> Unit,
    val onDismissCodeDialog: () -> Unit,
)

/**
 * What the code screen for [entry] says: nothing on a new device's welcome
 * screen, where the code itself says what it's for; the restore's title,
 * warning and way back for a restore over this device's copy.
 */
internal fun codeScreenText(entry: CodeEntry): CodeScreenText? =
    when (entry) {
        CodeEntry.NewDevice -> null
        is RestoreOver -> CodeScreenText(title = entry.title, message = entry.message, onBack = entry.onBack)
    }

/** Returns a callback that opens the scanner, asking for camera permission first when needed. */
@Composable
internal fun rememberScanRequest(
    onDenied: (String) -> Unit,
    onOpen: () -> Unit,
): () -> Unit {
    val context = LocalContext.current
    val permissionLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) {
                onOpen()
            } else {
                logger.info("camera permission denied; the code can still be typed")
                onDenied(context.getString(R.string.onboarding_camera_permission_required))
            }
        }
    return {
        val granted =
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
        if (granted) onOpen() else permissionLauncher.launch(Manifest.permission.CAMERA)
    }
}

@Composable
internal fun CodeScreen(
    flow: OnboardingFlow,
    step: OnboardingStep.EnterCode,
    text: CodeScreenText?,
    onScan: () -> Unit,
) {
    var showCodeDialog by remember { mutableStateOf(false) }
    var codeInput by remember { mutableStateOf("") }
    val savedCodes = flow.savedCodes
    CodeScreenContent(
        text = text,
        error = flow.error,
        savedCodes = {
            if (savedCodes != null) SavedCodesSection(codes = savedCodes, onOpen = flow::openSaved)
        },
        showCodeDialog = showCodeDialog,
        codeInput = codeInput,
        callbacks =
            CodeScreenCallbacks(
                onScan = {
                    flow.error = null
                    onScan()
                },
                onShowCodeDialog = {
                    flow.error = null
                    codeInput = ""
                    showCodeDialog = true
                },
                onCodeInputChange = { codeInput = it },
                onEnterCode = { code ->
                    showCodeDialog = false
                    flow.enter(step, code, CodeSource.TYPED)
                },
                onDismissCodeDialog = { showCodeDialog = false },
            ),
    )
}

/**
 * A code screen: scan a code or type it. A restore over this device's copy
 * says what it does first and offers Back; a new device's welcome lists the
 * restore codes saved on this phone above, in [savedCodes], and says nothing
 * more.
 */
@Composable
private fun CodeScreenContent(
    text: CodeScreenText?,
    error: DisplayError?,
    savedCodes: @Composable () -> Unit,
    showCodeDialog: Boolean,
    codeInput: String,
    callbacks: CodeScreenCallbacks,
) {
    OnboardingStartColumn(title = text?.title, message = text?.message) {
        savedCodes()
        PrimaryButton(onClick = callbacks.onScan, modifier = Modifier.width(actionWidth)) {
            Text(stringResource(R.string.onboarding_scan_code))
        }
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        OutlinedButton(onClick = callbacks.onShowCodeDialog, modifier = Modifier.width(actionWidth)) {
            Text(stringResource(R.string.onboarding_enter_code))
        }
        if (text != null) {
            Spacer(modifier = Modifier.height(ThemeSpace.related))
            TextButton(onClick = text.onBack, modifier = Modifier.width(actionWidth)) {
                Text(stringResource(R.string.back))
            }
        }
        if (error != null) {
            Spacer(modifier = Modifier.height(ThemeSpace.related))
            ErrorDetailDisclosure(error)
        }
    }
    if (showCodeDialog) {
        PasteCodeDialog(
            text =
                PasteDialogText(
                    title = stringResource(R.string.onboarding_enter_code),
                    instructions = stringResource(R.string.onboarding_enter_code_instructions),
                    placeholder = stringResource(R.string.onboarding_code_placeholder),
                    confirmLabel = stringResource(R.string.continue_action),
                ),
            pasteInput = codeInput,
            onInputChange = callbacks.onCodeInputChange,
            onConfirm = callbacks.onEnterCode,
            onDismiss = callbacks.onDismissCodeDialog,
        )
    }
}

/**
 * The first-run welcome screen, on a phone with no saved restore codes, in
 * the app chrome with inert callbacks, for the `welcome` screenshot scene.
 */
@Composable
internal fun WelcomeScene() {
    BaeAppChrome {
        CodeScreenContent(
            text = null,
            error = null,
            savedCodes = {},
            showCodeDialog = false,
            codeInput = "",
            callbacks =
                CodeScreenCallbacks(
                    onScan = {},
                    onShowCodeDialog = {},
                    onCodeInputChange = {},
                    onEnterCode = {},
                    onDismissCodeDialog = {},
                ),
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun WelcomeScenePreview() {
    WelcomeScene()
}
