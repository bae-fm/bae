package fm.bae.app.ui.settings

import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeApp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OAuthProviders
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.StorageSetupProgress
import fm.bae.app.deviceName
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.QRCodeImage
import fm.bae.app.ui.rememberScreenScope
import fm.bae.fastvalues.LatestSource
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeSetupCancellation
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.BridgeStorageSetupProgress
import uniffi.bae_bridge.availableCloudProviders

private val logger = BaeLogger("bae.StorageSetupScreen")

/** Where setting up storage stands. */
private sealed interface SetupStep {
    data object Choose : SetupStep

    data object S3 : SetupStep

    /** Signing in to [provider] before its folders can be listed. */
    data class SigningIn(
        val provider: BridgeCloudProvider,
    ) : SetupStep

    data object DropboxFolders : SetupStep

    data object OneDriveFolders : SetupStep

    /** Set up: the restore code the person keeps in case they lose every device. */
    data class Done(
        val setup: BridgeStorageSetup,
    ) : SetupStep
}

/**
 * Give a library that lives on this device only storage: an S3-compatible
 * bucket, or a folder in Dropbox or OneDrive after signing in. Once set up,
 * the person's restore code is shown for them to keep, and Done lands on the
 * Sync page ([onDone]).
 */
@Composable
internal fun StorageSetupScreen(
    session: OpenLibrary,
    oauthProviders: OAuthProviders?,
    onBack: () -> Unit,
    onDone: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val flow = remember(session) { StorageSetupFlow(session, oauthProviders, context, scope) }
    val runner = rememberStorageSetupRunner(session, onError = { flow.error = it }, onSetUp = flow::setUp)
    // Cancel takes the person back once coven has undone the setup.
    val cancel = { runner.cancel { flow.back(onBack) } }
    // While a setup runs, Back is its Cancel, and does nothing once Cancel
    // isn't offered: the screen stays until the setup's call ends.
    val back = {
        when {
            runner.cancelOffered -> {
                cancel()
            }

            runner.running -> {}

            else -> {
                flow.back(onBack)
            }
        }
    }
    BackHandler(onBack = back)

    Column(modifier = Modifier.fillMaxSize()) {
        SetupTopBar(onBack = back, backEnabled = !runner.running || runner.cancelOffered)
        Column(
            modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            flow.error?.let { ErrorNotice(it) }
            SetupStepContent(
                step = flow.step,
                session = session,
                oauthProviders = oauthProviders,
                runner = runner,
                callbacks =
                    SetupCallbacks(
                        onChoose = flow::choose,
                        onError = { flow.error = it },
                        onCancel = cancel,
                        onDone = onDone,
                    ),
            )
        }
    }
}

/** Where setting up storage stands, and the sign-in an account provider needs first. */
private class StorageSetupFlow(
    private val session: OpenLibrary,
    private val oauthProviders: OAuthProviders?,
    private val context: Context,
    private val scope: CoroutineScope,
) {
    var step by mutableStateOf<SetupStep>(SetupStep.Choose)
        private set
    var error by mutableStateOf<DisplayError?>(null)

    /** Back to choosing a provider, or out of setup from the choice or once set up. */
    fun back(leave: () -> Unit) {
        error = null
        when (step) {
            SetupStep.Choose, is SetupStep.Done -> leave()
            else -> step = SetupStep.Choose
        }
    }

    fun choose(provider: BridgeCloudProvider) {
        if (provider == BridgeCloudProvider.S3) {
            step = SetupStep.S3
            return
        }
        val providers = oauthProviders
        if (providers == null) {
            error = DisplayError(context.getString(R.string.oauth_unconfigured))
            return
        }
        error = null
        step = SetupStep.SigningIn(provider)
        scope.launch {
            var signedIn = false
            performBridgeAction(logger, "sign in to $provider", session.errors, { error = it }) {
                providers.signIn(session.appHandle, provider)
                signedIn = true
            }
            step =
                when {
                    !signedIn -> SetupStep.Choose
                    provider == BridgeCloudProvider.DROPBOX -> SetupStep.DropboxFolders
                    else -> SetupStep.OneDriveFolders
                }
        }
    }

    fun setUp(setup: BridgeStorageSetup) {
        error = null
        step = SetupStep.Done(setup)
    }
}

private class SetupCallbacks(
    val onChoose: (BridgeCloudProvider) -> Unit,
    val onError: (DisplayError?) -> Unit,
    val onCancel: () -> Unit,
    val onDone: () -> Unit,
)

/** What the setup shows at [step], its setup calls run by [runner]. */
@Composable
private fun ColumnScope.SetupStepContent(
    step: SetupStep,
    session: OpenLibrary,
    oauthProviders: OAuthProviders?,
    runner: StorageSetupRunner,
    callbacks: SetupCallbacks,
) {
    when (step) {
        SetupStep.Choose -> {
            ProviderChoice(onChoose = callbacks.onChoose)
        }

        SetupStep.S3 -> {
            S3SetupForm(
                session = session,
                setup = runner,
                onError = callbacks.onError,
                onCancel = callbacks.onCancel,
            )
        }

        is SetupStep.SigningIn -> {
            AndroidSpinner(text = stringResource(R.string.signing_in))
        }

        SetupStep.DropboxFolders -> {
            val providers = checkNotNull(oauthProviders) { "Dropbox setup signed in without sign-in" }
            FolderChoice(
                session = session,
                providers = providers,
                calls = FolderCalls.dropbox(session.appHandle, providers),
                words = DROPBOX_WORDS,
                setup = runner,
                onError = callbacks.onError,
                onCancel = callbacks.onCancel,
            )
        }

        SetupStep.OneDriveFolders -> {
            val providers = checkNotNull(oauthProviders) { "OneDrive setup signed in without sign-in" }
            FolderChoice(
                session = session,
                providers = providers,
                calls = FolderCalls.oneDrive(session.appHandle, providers),
                words = ONEDRIVE_WORDS,
                setup = runner,
                onError = callbacks.onError,
                onCancel = callbacks.onCancel,
            )
        }

        is SetupStep.Done -> {
            SetupDone(restoreCode = step.setup.restoreCode, onDone = callbacks.onDone)
        }
    }
}

@Composable
private fun SetupTopBar(
    onBack: () -> Unit,
    backEnabled: Boolean,
) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack, enabled = backEnabled) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(text = stringResource(R.string.storage_setup_title), style = ThemeText.title.style)
        }
    }
}

/** The providers this edition can store a library with. */
@Composable
private fun ProviderChoice(onChoose: (BridgeCloudProvider) -> Unit) {
    Text(stringResource(R.string.storage_setup_choose))
    availableCloudProviders().forEach { provider ->
        AndroidButton(
            kind = ButtonKind.OUTLINED,
            onClick = { onChoose(provider) },
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(cloudProviderLabel(provider))
        }
    }
}

/** Set up: the restore code, as a QR and as text, which gets the library back on a new device. */
@Composable
private fun ColumnScope.SetupDone(
    restoreCode: String,
    onDone: () -> Unit,
) {
    Text(stringResource(R.string.storage_setup_done))
    QRCodeImage(
        text = restoreCode,
        contentDescription = stringResource(R.string.restore_code_title),
    )
    SelectionContainer {
        Text(text = restoreCode, style = ThemeText.mono.style)
    }
    AndroidButton(onClick = onDone) { Text(stringResource(R.string.done)) }
}

/**
 * Runs a screen's storage setup calls: each one's steps are [steps] while it
 * runs, pulled in [scope]; its failure goes to [onError] and the setup it
 * ends in to [onSetUp]. Before its point of no return the person can
 * [cancel] it, which coven does between setup's steps rather than in the
 * middle of a request, so the cancel shows under way until the call ends.
 */
internal class StorageSetupRunner(
    private val context: Context,
    private val session: OpenLibrary,
    private val scope: CoroutineScope,
    private val onError: (DisplayError?) -> Unit,
    private val onSetUp: (BridgeStorageSetup) -> Unit,
) {
    /** The running setup's steps; null while none runs. */
    var steps by mutableStateOf<LatestSource<BridgeStorageSetupProgress>?>(null)
        private set

    /**
     * Whether the running setup is past its point of no return, creating the
     * library in storage or recording changed access: coven finishes it
     * rather than undoing it, so Cancel isn't offered.
     */
    var finishing by mutableStateOf(false)
        private set

    /** What follows the person's cancel once the setup's call ends; null while nobody cancelled. */
    private var afterCancel by mutableStateOf<(() -> Unit)?>(null)

    /** The coroutine the running setup's call runs in; null while none runs. */
    private var call: Job? = null

    val running: Boolean
        get() = steps != null

    /** Whether the person cancelled the running setup, which coven undoes before its call ends. */
    val cancelling: Boolean
        get() = afterCancel != null

    /** Whether Cancel can still stop the running setup. */
    val cancelOffered: Boolean
        get() = running && !finishing && !cancelling

    /** Run [setUp], named [operation] in the log, under this device's name. */
    suspend fun run(
        operation: String,
        setUp: suspend (deviceName: String, setup: BridgeLibrarySetup) -> BridgeStorageSetup,
    ) {
        run(operation, { false }, setUp)
    }

    /**
     * Run [setUp] as the other [run] does. A failure [claim] takes is shown by the caller where
     * it belongs, not as this runner's error.
     */
    suspend fun run(
        operation: String,
        claim: (BridgeException) -> Boolean,
        setUp: suspend (deviceName: String, setup: BridgeLibrarySetup) -> BridgeStorageSetup,
    ) {
        var result: BridgeStorageSetup? = null
        StorageSetupProgress((context.applicationContext as BaeApp).host, scope, session.bugs).use { progress ->
            steps = progress.steps
            call = currentCoroutineContext().job
            val following = scope.launch { progress.followFinishing { finishing = it } }
            try {
                performBridgeAction(logger, operation, session.errors, onError) {
                    result =
                        try {
                            setUp(deviceName(context), progress.setup)
                        } catch (e: BridgeException) {
                            if (!claim(e)) throw e
                            null
                        }
                }
            } finally {
                following.cancel()
                call = null
                steps = null
                finishing = false
                // The cancelled setup's call has ended: coven undid it.
                afterCancel?.let { after ->
                    afterCancel = null
                    after()
                }
            }
        }
        result?.let(onSetUp)
    }

    /**
     * The person cancelled the running setup: before its point of no return
     * coven undoes it and its call ends cancelled, and then [after] runs. A
     * setup coven isn't running yet is dropped with its call, which cancels
     * it. Past the point the setup finishes, and Cancel goes as it says so.
     */
    fun cancel(after: () -> Unit) {
        when (session.appHandle.cancelStorageSetup()) {
            BridgeSetupCancellation.UNDOING -> {
                afterCancel = after
            }

            BridgeSetupCancellation.NO_SETUP -> {
                afterCancel = after
                call?.cancel()
            }

            BridgeSetupCancellation.FINISHING -> {
                logger.info("Cancel came past storage setup's point of no return")
            }
        }
    }
}

/** A storage setup runner for [session]'s screen, reporting to the latest [onError] and [onSetUp]. */
@Composable
internal fun rememberStorageSetupRunner(
    session: OpenLibrary,
    onError: (DisplayError?) -> Unit,
    onSetUp: (BridgeStorageSetup) -> Unit,
): StorageSetupRunner {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val latestError by rememberUpdatedState(onError)
    val latestSetUp by rememberUpdatedState(onSetUp)
    return remember(session) {
        StorageSetupRunner(context, session, scope, { latestError(it) }, { latestSetUp(it) })
    }
}

/**
 * What [runner]'s setup shows while it runs: the step over its bar, with
 * Cancel, calling [onCancel], while it can still stop the setup, or the
 * person's cancel under way.
 */
@Composable
internal fun StorageSetupRunLine(
    runner: StorageSetupRunner,
    onCancel: () -> Unit,
) {
    if (runner.cancelling) {
        AndroidSpinner(text = stringResource(R.string.storage_setup_cancelling))
        return
    }
    runner.steps?.let { StorageSetupLine(it) }
    if (runner.cancelOffered) {
        AndroidButton(kind = ButtonKind.TEXT, onClick = onCancel) { Text(stringResource(R.string.cancel)) }
    }
}
