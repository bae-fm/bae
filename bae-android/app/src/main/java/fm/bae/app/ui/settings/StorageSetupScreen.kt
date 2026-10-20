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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
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
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OAuthProviders
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.deviceName
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.QRCodeImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.availableCloudProviders

private val logger = BaeLogger("bae.StorageSetupScreen")

private val qrSize = 220.dp

/** Where setting up a cloud home stands. */
private sealed interface SetupStep {
    data object Choose : SetupStep

    data object S3 : SetupStep

    /** Signing in to [provider] before its folders can be listed. */
    data class SigningIn(
        val provider: BridgeCloudProvider,
    ) : SetupStep

    data object DropboxFolders : SetupStep

    data object OneDriveFolders : SetupStep

    /** Set up: the recovery code the person keeps in case they lose every device. */
    data class Done(
        val setup: BridgeStorageSetup,
    ) : SetupStep
}

/**
 * Give a library that lives on this device only a cloud home: an S3-compatible
 * bucket, or a folder in Dropbox or OneDrive after signing in. Once set up,
 * the person's recovery code is shown for them to keep.
 */
@Composable
internal fun StorageSetupScreen(
    session: OpenLibrary,
    oauthProviders: OAuthProviders?,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val flow = remember(session) { StorageSetupFlow(session, oauthProviders, context, scope) }
    val back = { flow.back(onBack) }
    BackHandler(onBack = back)

    Column(modifier = Modifier.fillMaxSize()) {
        SetupTopBar(onBack = back)
        Column(
            modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            flow.error?.let { ErrorText(it) }
            SetupStepContent(
                step = flow.step,
                session = session,
                oauthProviders = oauthProviders,
                callbacks =
                    SetupCallbacks(
                        onChoose = flow::choose,
                        onError = { flow.error = it },
                        onSetUp = flow::setUp,
                        onDone = onBack,
                    ),
            )
        }
    }
}

/** Where setting up a cloud home stands, and the sign-in an account provider needs first. */
private class StorageSetupFlow(
    private val session: OpenLibrary,
    private val oauthProviders: OAuthProviders?,
    private val context: Context,
    private val scope: CoroutineScope,
) {
    var step by mutableStateOf<SetupStep>(SetupStep.Choose)
        private set
    var error by mutableStateOf<String?>(null)

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
            error = context.getString(R.string.oauth_unconfigured)
            return
        }
        error = null
        step = SetupStep.SigningIn(provider)
        scope.launch {
            var signedIn = false
            performBridgeAction(logger, "sign in to $provider", LocaleErrorLines(context), { error = it }) {
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
    val onError: (String?) -> Unit,
    val onSetUp: (BridgeStorageSetup) -> Unit,
    val onDone: () -> Unit,
)

/** What the setup shows at [step]. */
@Composable
private fun ColumnScope.SetupStepContent(
    step: SetupStep,
    session: OpenLibrary,
    oauthProviders: OAuthProviders?,
    callbacks: SetupCallbacks,
) {
    when (step) {
        SetupStep.Choose -> {
            ProviderChoice(onChoose = callbacks.onChoose)
        }

        SetupStep.S3 -> {
            S3SetupForm(session = session, onError = callbacks.onError, onSetUp = callbacks.onSetUp)
        }

        is SetupStep.SigningIn -> {
            Row(
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircularProgressIndicator(modifier = Modifier.size(ThemeIcon.medium))
                Text(stringResource(R.string.signing_in))
            }
        }

        SetupStep.DropboxFolders -> {
            DropboxFolderChoice(
                session = session,
                providers = checkNotNull(oauthProviders) { "Dropbox setup signed in without sign-in" },
                onError = callbacks.onError,
                onSetUp = callbacks.onSetUp,
            )
        }

        SetupStep.OneDriveFolders -> {
            OneDriveFolderChoice(
                session = session,
                providers = checkNotNull(oauthProviders) { "OneDrive setup signed in without sign-in" },
                onError = callbacks.onError,
                onSetUp = callbacks.onSetUp,
            )
        }

        is SetupStep.Done -> {
            SetupDone(restoreCode = step.setup.restoreCode, onDone = callbacks.onDone)
        }
    }
}

@Composable
private fun SetupTopBar(onBack: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
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
        OutlinedButton(onClick = { onChoose(provider) }, modifier = Modifier.fillMaxWidth()) {
            Text(cloudProviderLabel(provider))
        }
    }
}

/** Set up: the recovery code, as a QR and as text, which gets the library back on a new device. */
@Composable
private fun ColumnScope.SetupDone(
    restoreCode: String,
    onDone: () -> Unit,
) {
    Text(stringResource(R.string.storage_setup_done))
    QRCodeImage(
        text = restoreCode,
        contentDescription = stringResource(R.string.recovery_code_title),
        modifier = Modifier.size(qrSize).align(Alignment.CenterHorizontally),
    )
    SelectionContainer {
        Text(text = restoreCode, style = ThemeText.mono.style)
    }
    PrimaryButton(onClick = onDone) { Text(stringResource(R.string.done)) }
}

/** Run a setup step that ends in a storage setup, reporting its failure through [onError]. */
internal suspend fun runSetup(
    context: Context,
    operation: String,
    onError: (String?) -> Unit,
    onSetUp: (BridgeStorageSetup) -> Unit,
    setUp: suspend (deviceName: String) -> BridgeStorageSetup,
) {
    var result: BridgeStorageSetup? = null
    performBridgeAction(logger, operation, LocaleErrorLines(context), onError) {
        result = setUp(deviceName(context))
    }
    result?.let(onSetUp)
}
