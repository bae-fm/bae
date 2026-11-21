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
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.NARROW_DIALOG_WIDTH
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
        width = NARROW_DIALOG_WIDTH,
        message = LocalContext.current.coreString(prepared.warningKey),
        action = DialogAction(stringResource(R.string.reset_device_confirm), onConfirm, destructive = true),
        dismiss = DialogAction(stringResource(R.string.cancel), onDismiss),
    )
}

/**
 * bae can't open this device's copy of the library, and only a device reset
 * fixes it; the screen offers the reset after its warning. After a failed
 * open, the notice says the library did not open and that its data is wrong,
 * and Details shows why. With no failure, as when the person cancelled a reset
 * or the Sync page offered it, the screen says the reset's line. When a reset
 * fails, the notice's title names the step that failed instead, and Retry
 * resets again without the warning the person already accepted. Back leaves
 * without resetting. Preparing the reset reads this device's state and can
 * fail; a failed preparation shows the same notice and Retry as a failed
 * reset.
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

        // After a failed open, the notice names the failed step, says the data is wrong and offers Reset from cloud.
        shown != null && shown.step == FailedStep.LIBRARY_OPEN -> {
            AndroidNotice(
                title = context.coreString(shown.step.titleKey),
                advice = stringResource(R.string.library_data_wrong),
                error = reason,
                look = NoticeLook.CENTERED,
                action = offer,
            )
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

/**
 * A library left its store, but deleting this device's files for it failed; this screen offers Retry when
 * core reports that a second attempt can succeed. The library already left its store, so there is no
 * library for Reset from cloud.
 */
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
            action = error.retryAction(onClick = onRetry),
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
