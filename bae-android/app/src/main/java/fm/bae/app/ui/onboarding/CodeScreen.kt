package fm.bae.app.ui.onboarding

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.Logo
import uniffi.bae_bridge.codeAsEntered
import uniffi.bae_bridge.codeInGroups

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
 * What the code screen for [entry] says above its actions: nothing on a new
 * device's welcome screen, which has its own words; the restore's title,
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
    val resources = LocalResources.current
    val permissionLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) {
                onOpen()
            } else {
                logger.info("camera permission denied; the code can still be typed")
                onDenied(resources.getString(R.string.onboarding_camera_permission_required))
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
    libraries: WelcomeLibrariesView?,
    showsError: Boolean = true,
) {
    var showCodeDialog by remember { mutableStateOf(false) }
    var codeInput by remember { mutableStateOf("") }
    val savedCodes = flow.savedCodes
    CodeScreenContent(
        text = text,
        error = if (showsError) flow.error else null,
        libraries = libraries,
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
                onCodeInputChange = { codeInput = codeInGroups(it) },
                onEnterCode = { code ->
                    showCodeDialog = false
                    flow.enter(step, codeAsEntered(code), CodeSource.TYPED)
                },
                onDismissCodeDialog = { showCodeDialog = false },
            ),
    )
}

/**
 * A code screen: scan a code or type it. A restore over this device's copy
 * says what it does first and offers Back; a new device's welcome is the
 * [WelcomeColumn].
 */
@Composable
private fun CodeScreenContent(
    text: CodeScreenText?,
    error: DisplayError?,
    libraries: WelcomeLibrariesView?,
    savedCodes: @Composable () -> Unit,
    showCodeDialog: Boolean,
    codeInput: String,
    callbacks: CodeScreenCallbacks,
) {
    if (text != null) {
        OnboardingStartColumn(title = text.title, message = text.message) {
            CodeButtons(callbacks)
            Spacer(modifier = Modifier.height(ThemeSpace.related))
            AndroidButton(kind = ButtonKind.TEXT, onClick = text.onBack, modifier = Modifier.width(actionWidth)) {
                Text(stringResource(R.string.back))
            }
            error?.let {
                Spacer(modifier = Modifier.height(ThemeSpace.related))
                AndroidNotice(it)
            }
        }
    } else {
        WelcomeColumn(
            listed = libraries?.listed == true,
            callbacks = callbacks,
            error = error,
            savedCodes = savedCodes,
        ) { onAdd ->
            libraries?.content?.invoke(onAdd)
        }
    }
    if (showCodeDialog) {
        PasteCodeDialog(
            text =
                PasteDialogText(
                    title = stringResource(R.string.onboarding_enter_code),
                    instructions = stringResource(R.string.onboarding_enter_code_instructions),
                    label = stringResource(R.string.onboarding_code_label),
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
 * The Scan code and Enter code buttons at [actionWidth], one over the other.
 */
@Composable
private fun CodeButtons(callbacks: CodeScreenCallbacks) {
    AndroidButton(onClick = callbacks.onScan, modifier = Modifier.width(actionWidth)) {
        Text(stringResource(R.string.onboarding_scan_code))
    }
    Spacer(modifier = Modifier.height(ThemeSpace.related))
    AndroidButton(
        kind = ButtonKind.OUTLINED,
        onClick = callbacks.onShowCodeDialog,
        modifier = Modifier.width(actionWidth),
    ) {
        Text(stringResource(R.string.onboarding_enter_code))
    }
}

/**
 * A new device's welcome: bae's sheep and name floating over "Your music,
 * everywhere", then [libraries] and the saved restore codes. With libraries
 * [listed] the logo is at the top over the list, whose last row, Add library,
 * opens a sheet with the actions; with none the logo is centered in the space
 * above the actions, which sit at the foot. Scrolled when it doesn't fit, such
 * as on a phone held sideways.
 */
@Composable
private fun WelcomeColumn(
    listed: Boolean,
    callbacks: CodeScreenCallbacks,
    error: DisplayError?,
    savedCodes: @Composable () -> Unit,
    libraries: @Composable (onAdd: () -> Unit) -> Unit,
) {
    if (listed) {
        ListedWelcome(callbacks, error, savedCodes, libraries)
    } else {
        EmptyWelcome(callbacks, error, savedCodes) { libraries {} }
    }
}

/**
 * The welcome over a list of libraries, whose Add library row raises the sheet of actions. The
 * logo, tagline and list sit together, centered top to bottom, and scroll when they don't fit.
 */
@Composable
private fun ListedWelcome(
    callbacks: CodeScreenCallbacks,
    error: DisplayError?,
    savedCodes: @Composable () -> Unit,
    libraries: @Composable (onAdd: () -> Unit) -> Unit,
) {
    var adding by remember { mutableStateOf(false) }
    BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .heightIn(min = maxHeight)
                    .padding(top = ThemeSpace.section, bottom = LIST_BOTTOM_PADDING),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Logo(size = LIST_LOGO_SIZE, bob = true, glow = true)
            Tagline(modifier = Modifier.padding(LIST_TAGLINE_PADDING))
            Column(
                modifier = Modifier.padding(horizontal = ThemeSpace.edge),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                libraries { adding = true }
                savedCodes()
                error?.let { AndroidNotice(it) }
            }
        }
    }
    if (adding) {
        AndroidSheet(title = stringResource(R.string.welcome_add_library), onDismiss = { adding = false }) {
            WelcomeActions(
                onScan = {
                    adding = false
                    callbacks.onScan()
                },
                onEnter = {
                    adding = false
                    callbacks.onShowCodeDialog()
                },
            )
        }
    }
}

/** The welcome with no list: the logo centered over the space the actions leave, which sit at the foot. */
@Composable
private fun EmptyWelcome(
    callbacks: CodeScreenCallbacks,
    error: DisplayError?,
    savedCodes: @Composable () -> Unit,
    libraries: @Composable () -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier =
                Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = ThemeSpace.page),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Logo(size = EMPTY_LOGO_SIZE, bob = true, glow = true)
            Tagline(modifier = Modifier.padding(top = TAGLINE_TOP))
            libraries()
            savedCodes()
            error?.let {
                Spacer(modifier = Modifier.height(ThemeSpace.related))
                AndroidNotice(it)
            }
        }
        WelcomeActions(onScan = callbacks.onScan, onEnter = callbacks.onShowCodeDialog)
        Spacer(modifier = Modifier.height(ThemeSpace.section))
    }
}

@Composable
private fun Tagline(modifier: Modifier = Modifier) {
    Text(
        text = stringResource(R.string.onboarding_tagline),
        style = ThemeText.androidBodyLarge.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
        modifier = modifier,
    )
}

private val LIST_LOGO_SIZE = 96.dp
private val EMPTY_LOGO_SIZE = 150.dp
private val TAGLINE_TOP = 14.dp
private val LIST_BOTTOM_PADDING = 48.dp
private val LIST_TAGLINE_PADDING =
    PaddingValues(start = ThemeSpace.page, top = TAGLINE_TOP, end = ThemeSpace.page, bottom = ThemeSpace.edge)

/**
 * The welcome screen, on a phone with no saved restore codes, with inert
 * callbacks, for the `welcome` screenshot scene, and for
 * the one that lists the libraries on the phone, in [libraries], and the one
 * that lists restore codes saved for others, drawn by [savedCodes].
 */
@Composable
internal fun WelcomeScene(
    libraries: WelcomeLibrariesView? = null,
    savedCodes: @Composable () -> Unit = {},
) {
    CodeScreenContent(
        text = null,
        error = null,
        libraries = libraries,
        savedCodes = savedCodes,
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

@Preview(showBackground = true)
@Composable
private fun WelcomeScenePreview() {
    PreviewTheme { BaeAppChrome { WelcomeScene() } }
}
