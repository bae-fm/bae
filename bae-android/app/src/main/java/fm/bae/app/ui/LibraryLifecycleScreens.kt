package fm.bae.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.AppScreen
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.FailedStep
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.PreparedReset
import fm.bae.app.R
import fm.bae.app.ScreenFailure
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.ScreenFailureNotice
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.retryAction
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRecoveryLines

private val logger = BaeLogger("bae.LibraryLifecycleScreens")

/**
 * The warning a device reset carries, before the person confirms it: edits on
 * this device that haven't synced are discarded.
 */
@Composable
internal fun DeviceResetDialog(
    prepared: PreparedReset,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = stringResource(R.string.reset_device_confirm_title),
        message = LocalContext.current.coreString(prepared.warningKey),
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.DESTRUCTIVE, onClick = onConfirm) {
                Text(stringResource(R.string.reset_device_confirm))
            }
        },
        dismissButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                tone = ButtonTone.NEUTRAL,
                onClick = onDismiss,
            ) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/**
 * Opening failed in a way only a device reset fixes; the reset is offered
 * after its warning. The screen says why: [error]'s own line, or with none,
 * after a reset was cancelled, the reset's line. A reset that failed is titled
 * by what failed instead, and Retry resets again without the warning the
 * person already accepted. Back leaves without resetting. Preparing the reset
 * can itself fail (it reads this device's state); that shows as a failed reset
 * does, with the same Retry.
 */
@Composable
internal fun ResetOfferedScreen(
    libraryId: String,
    error: ScreenFailure?,
    lines: BridgeRecoveryLines,
    prepare: () -> PreparedReset,
    errors: ErrorDisplays,
    onReset: (BridgeDeviceReset) -> Unit,
    onLeave: () -> Unit,
) {
    val context = LocalContext.current
    var offered by remember(libraryId) { mutableStateOf<PreparedReset?>(null) }
    var prepareFailure by remember(libraryId, error) { mutableStateOf<ScreenFailure?>(null) }
    BackHandler(onBack = onLeave)

    // Runs [then] on the prepared reset; a failed preparation replaces the screen's failure.
    fun prepared(then: (PreparedReset) -> Unit) {
        try {
            prepareFailure = null
            then(prepare())
        } catch (e: BridgeException) {
            logger.error("preparing the device reset for $libraryId failed", e)
            prepareFailure = errors.display(e)?.let { ScreenFailure(FailedStep.DEVICE_RESET, it) }
        }
    }
    val offer = ErrorAction(stringResource(R.string.reset_device)) { prepared { offered = it } }
    val shown = prepareFailure ?: error
    val reason = shown?.error
    when {
        shown != null && shown.step == FailedStep.DEVICE_RESET -> {
            AndroidNotice(
                title = context.coreString(shown.step.titleKey),
                advice = null,
                error = reason,
                look = NoticeLook.CENTERED,
                action = shown.error.retryAction { prepared { onReset(it.reset) } },
            )
        }

        reason != null -> {
            AndroidNotice(reason, look = NoticeLook.CENTERED, action = offer)
        }

        else -> {
            AndroidNotice(
                title = context.coreString(lines.deviceResetRequired),
                advice = null,
                error = null,
                look = NoticeLook.CENTERED,
                action = offer,
            )
        }
    }
    offered?.let { prepared ->
        DeviceResetDialog(
            prepared = prepared,
            onConfirm = {
                offered = null
                onReset(prepared.reset)
            },
            onDismiss = { offered = null },
        )
    }
}

/** [ResetOfferedScreen] for [screen], whose reset a failure shows this screen again for, with why. */
@Composable
internal fun ResetOfferedRecovery(
    screen: AppScreen.ResetOffered,
    lines: BridgeRecoveryLines,
    onScreen: (AppScreen) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val libraryId = screen.libraryId
    ResetOfferedScreen(
        libraryId = libraryId,
        error = screen.error,
        lines = lines,
        prepare = { AppSessionHolder.prepareDeviceReset(context, libraryId) },
        errors = remember(context) { LocaleErrorDisplays(context) },
        onReset = { scope.launch { AppSessionHolder.resetOffered(context, libraryId, it, onScreen) } },
        onLeave = { AppSessionHolder.leaveRecovery(onScreen) },
    )
}

/**
 * A reset of this device's copy stopped partway, and finishing it failed, as
 * [error] says; finishing tries again from what core kept, without the
 * warning the person already accepted. With no [error], the screen says the
 * reset didn't finish. Back leaves, keeping the library waiting for it.
 */
@Composable
internal fun ResetUnfinishedScreen(
    error: ScreenFailure?,
    lines: BridgeRecoveryLines,
    onFinish: () -> Unit,
    onLeave: () -> Unit,
) {
    val context = LocalContext.current
    BackHandler(onBack = onLeave)
    val finish = ErrorAction(stringResource(R.string.finish_reset), onClick = onFinish)
    if (error == null) {
        AndroidNotice(
            title = context.coreString(lines.resetUnfinished),
            advice = null,
            error = null,
            tone = StatusTone.WARNING,
            look = NoticeLook.CENTERED,
            action = finish,
        )
    } else {
        AndroidNotice(
            title = context.coreString(error.step.titleKey),
            advice = null,
            error = error.error,
            look = NoticeLook.CENTERED,
            action = finish,
        )
    }
}

/**
 * Removing a library from this device stopped partway, after this device
 * began leaving it; finishing removes it. Leaving keeps the library waiting
 * for it.
 */
@Composable
internal fun DeletionUnfinishedScreen(
    error: ScreenFailure?,
    lines: BridgeRecoveryLines,
    onFinish: () -> Unit,
    onLeave: () -> Unit,
) {
    val context = LocalContext.current
    BackHandler(onBack = onLeave)
    LifecycleMessage(context.coreString(lines.deletionUnfinished)) {
        error?.let { ScreenFailureNotice(it) }
        AndroidButton(onClick = onFinish) { Text(stringResource(R.string.finish_removal)) }
        AndroidButton(kind = ButtonKind.TEXT, onClick = onLeave) { Text(stringResource(R.string.cancel)) }
    }
}

/** A library left its store, but deleting this device's files for it failed; finishing is retried here. */
@Composable
internal fun RemovalIncompleteScreen(
    error: DisplayError?,
    onRetry: () -> Unit,
) {
    val context = LocalContext.current
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        AndroidNotice(
            title = context.coreString("ui.load_error.removal.title"),
            advice = null,
            error = error,
            look = NoticeLook.CENTERED,
            action = ErrorAction(stringResource(R.string.retry), onClick = onRetry),
        )
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
private fun RemovalIncompleteScreenPreview() {
    BaeTheme {
        RemovalIncompleteScreen(
            error = DisplayError("Couldn't delete this library's settings.", "permission denied"),
            onRetry = {},
        )
    }
}
