package fm.bae.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.PersonNameField
import fm.bae.app.ui.components.PrimaryButton
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeReplacedBy
import uniffi.bae_bridge.bridgeErrorCategoryKey

private val logger = BaeLogger("bae.LibraryLifecycleScreens")

/**
 * The warning a device reset carries, before the person confirms it: edits on
 * this device that haven't synced are discarded.
 */
@Composable
internal fun DeviceResetDialog(
    reset: BridgeDeviceReset,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.reset_device_confirm_title)) },
        text = { Text(LocalContext.current.coreString(reset.warningKey())) },
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(stringResource(R.string.reset_device_confirm)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/** Opening failed in a way only a device reset fixes; the reset is offered after its warning. */
@Composable
internal fun ResetOfferedScreen(
    libraryId: String,
    error: DisplayError,
    onReset: (BridgeDeviceReset) -> Unit,
) {
    val context = LocalContext.current
    var reset by remember(libraryId) { mutableStateOf<BridgeDeviceReset?>(null) }
    LifecycleError(error) {
        PrimaryButton(onClick = { reset = AppSessionHolder.prepareDeviceReset(context, libraryId) }) {
            Text(stringResource(R.string.reset_device))
        }
    }
    reset?.let { prepared ->
        DeviceResetDialog(
            reset = prepared,
            onConfirm = {
                reset = null
                onReset(prepared)
            },
            onDismiss = { reset = null },
        )
    }
}

/**
 * A reset of this device's copy stopped partway; finishing resets again,
 * without the warning the person already accepted. Leaving keeps the library
 * waiting for it.
 */
@Composable
internal fun ResetUnfinishedScreen(
    error: DisplayError?,
    onFinish: () -> Unit,
    onLeave: () -> Unit,
) {
    val context = LocalContext.current
    BackHandler(onBack = onLeave)
    LifecycleMessage(
        context.coreString(bridgeErrorCategoryKey(BridgeErrorCategory.ReplacementUnfinished(BridgeReplacedBy.RESET))),
    ) {
        error?.let { ErrorDetailDisclosure(it) }
        PrimaryButton(onClick = onFinish) { Text(stringResource(R.string.finish_reset)) }
        TextButton(onClick = onLeave) { Text(stringResource(R.string.cancel)) }
    }
}

/** Joining made this device a member but its profile wasn't written; the person names themselves again. */
@Composable
internal fun ProfileRequiredScreen(
    session: OpenLibrary,
    failure: DisplayError,
    onCreated: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var name by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<DisplayError?>(failure) }
    var saving by remember { mutableStateOf(false) }
    LifecycleMessage(stringResource(R.string.profile_required_title)) {
        PersonNameField(name) { name = it }
        error?.let { ErrorDetailDisclosure(it) }
        PrimaryButton(
            onClick = {
                saving = true
                scope.launch {
                    var created = false
                    performBridgeAction(
                        logger = logger,
                        operation = "create the person's profile",
                        errors = LocaleErrorDisplays(context),
                        showError = { error = it },
                    ) {
                        session.appHandle.createProfile(name.trim())
                        created = true
                    }
                    saving = false
                    if (created) onCreated()
                }
            },
            enabled = name.isNotBlank() && !saving,
        ) {
            Text(stringResource(R.string.save))
        }
    }
}

/** A library left its store, but deleting this device's files for it failed; finishing is retried here. */
@Composable
internal fun RemovalIncompleteScreen(
    error: DisplayError,
    onRetry: () -> Unit,
) {
    LifecycleError(error) {
        PrimaryButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
    }
}

@Composable
private fun LifecycleMessage(
    message: String,
    content: @Composable () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.page),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.group, Alignment.CenterVertically),
    ) {
        Text(
            text = message,
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        content()
    }
}

/** A failure in place of the message, with its details, above what the screen offers. */
@Composable
private fun LifecycleError(
    error: DisplayError,
    content: @Composable () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.page),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.group, Alignment.CenterVertically),
    ) {
        ErrorDetailDisclosure(error = error, lineColor = MaterialTheme.colorScheme.onSurfaceVariant)
        content()
    }
}

@Preview(showBackground = true)
@Composable
private fun RemovalIncompleteScreenPreview() {
    BaeTheme {
        RemovalIncompleteScreen(
            error = DisplayError("Couldn't delete this library's settings.", "permission denied"),
            onRetry = {},
        )
    }
}
