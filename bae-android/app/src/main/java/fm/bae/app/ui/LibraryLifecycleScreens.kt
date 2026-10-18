package fm.bae.app.ui

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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.PersonNameField
import fm.bae.app.ui.components.PrimaryButton
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeDeviceReset

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
    message: String,
    onReset: (BridgeDeviceReset) -> Unit,
) {
    val context = LocalContext.current
    var reset by remember(libraryId) { mutableStateOf<BridgeDeviceReset?>(null) }
    LifecycleMessage(message) {
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

/** Joining made this device a member but its profile wasn't written; the person names themselves again. */
@Composable
internal fun ProfileRequiredScreen(
    session: OpenLibrary,
    message: String,
    onCreated: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(message) }
    var saving by remember { mutableStateOf(false) }
    LifecycleMessage(stringResource(R.string.profile_required_title)) {
        PersonNameField(name) { name = it }
        error?.let { ErrorText(it) }
        PrimaryButton(
            onClick = {
                saving = true
                scope.launch {
                    var created = false
                    performBridgeAction(
                        logger = logger,
                        operation = "create the person's profile",
                        errors = LocaleErrorLines(context),
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

/**
 * Removing the open library stopped for this device's unsent changes: how many
 * would be lost, then removing anyway or keeping the library.
 */
@Composable
internal fun UnsentEditsScreen(
    writes: ULong,
    files: ULong,
    onRemove: () -> Unit,
    onKeep: () -> Unit,
) {
    val context = LocalContext.current
    LifecycleMessage(stringResource(R.string.unsent_edits_title)) {
        if (writes > 0u) {
            UnsentLine(context.coreString("core.library.removal.unsent_writes", mapOf("count" to writes.toLong())))
        }
        if (files > 0u) {
            UnsentLine(context.coreString("core.library.removal.unsent_files", mapOf("count" to files.toLong())))
        }
        UnsentLine(stringResource(R.string.unsent_edits_body))
        PrimaryButton(onClick = onRemove) { Text(stringResource(R.string.unsent_edits_remove)) }
        TextButton(onClick = onKeep) { Text(stringResource(R.string.cancel)) }
    }
}

@Composable
private fun UnsentLine(text: String) {
    Text(
        text = text,
        style = ThemeText.body.style,
        textAlign = TextAlign.Center,
    )
}

/** The library left its store, but deleting its preferences failed; finishing is retried here. */
@Composable
internal fun RemovalIncompleteScreen(
    message: String,
    onRetry: () -> Unit,
) {
    LifecycleMessage(message) {
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

@Preview(showBackground = true)
@Composable
private fun UnsentEditsScreenPreview() {
    BaeTheme {
        UnsentEditsScreen(writes = 3u, files = 2u, onRemove = {}, onKeep = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun RemovalIncompleteScreenPreview() {
    BaeTheme {
        RemovalIncompleteScreen(message = "Couldn't delete this library's settings.", onRetry = {})
    }
}
