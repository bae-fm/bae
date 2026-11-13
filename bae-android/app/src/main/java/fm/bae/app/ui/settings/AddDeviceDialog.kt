package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.displayError
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidMatchCode
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidScanCode
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.rememberScreenScope
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAddDevice
import uniffi.bae_bridge.BridgeAddDeviceEnd
import uniffi.bae_bridge.BridgeAddDeviceStatus
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.AddDeviceDialog")

/** What Add device shows of a pairing's status, as plain values. */
internal sealed interface AddDeviceFace {
    /** The code to scan or type: no request has come. */
    data object Code : AddDeviceFace

    /** A new device asks to be added, showing [confirmation]. */
    data class Request(
        val deviceName: String,
        val confirmation: String,
    ) : AddDeviceFace

    /** The approved device is on its way in. */
    data class Adding(
        val name: String,
    ) : AddDeviceFace

    /** The device is in. */
    data class Added(
        val name: String,
    ) : AddDeviceFace

    /** The person declined the request: the dialog closes. */
    data object Declined : AddDeviceFace

    /** The code's time passed with no device added: the dialog says so. */
    data object Expired : AddDeviceFace

    /** coven refused the request, so no device was added: the dialog says so. */
    data object Refused : AddDeviceFace
}

internal fun BridgeAddDeviceStatus.face(): AddDeviceFace =
    when (this) {
        is BridgeAddDeviceStatus.Waiting -> {
            request?.info()?.let { AddDeviceFace.Request(it.deviceName, it.confirmation) } ?: AddDeviceFace.Code
        }

        is BridgeAddDeviceStatus.Approved -> {
            AddDeviceFace.Adding(deviceName)
        }

        is BridgeAddDeviceStatus.Ended -> {
            when (val end = this.end) {
                is BridgeAddDeviceEnd.Added -> AddDeviceFace.Added(end.name)
                BridgeAddDeviceEnd.Declined -> AddDeviceFace.Declined
                BridgeAddDeviceEnd.Expired -> AddDeviceFace.Expired
                BridgeAddDeviceEnd.Refused -> AddDeviceFace.Refused
            }
        }
    }

/**
 * Adds another of this person's devices: a pairing, started on demand, whose
 * code shows as a QR the new device scans and as text to type there instead.
 * While it shows the code, it follows the pairing's status: a new device's
 * request replaces the code with the number to compare, and once the device
 * is in the dialog says so. Closing it ends the pairing.
 */
@Composable
internal fun AddDeviceDialog(
    appHandle: AppHandleInterface,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var adding by remember { mutableStateOf<BridgeAddDevice?>(null) }
    var status by remember { mutableStateOf<BridgeAddDeviceStatus?>(null) }
    var error by remember { mutableStateOf<DisplayError?>(null) }

    // A cancellation shows nothing: the dialog stays on its spinner or code.
    fun show(failure: BridgeException) {
        val shown = context.displayError(failure)
        if (shown == null) {
            logger.debug("adding a device was cancelled; nothing shows")
        } else {
            error = shown
        }
    }

    LaunchedEffect(Unit) {
        val started =
            try {
                appHandle.addDevice()
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("Failed to start the pairing", e)
                show(e)
                return@LaunchedEffect
            }
        adding = started
        val following =
            ScreenSubscription(
                appHandle.followAddDevice(started.id).let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value -> status = value },
                onError = { failure ->
                    logger.error("Following the pairing failed", failure)
                    show(failure)
                },
            )
        try {
            awaitCancellation()
        } finally {
            following.stop()
            // The dialog closed: the pairing ends, whatever its status.
            withContext(NonCancellable) {
                try {
                    appHandle.endAddDevice(started.id)
                } catch (e: BridgeException) {
                    logger.error("Ending the pairing failed", e)
                }
            }
        }
    }

    val face = status?.face() ?: AddDeviceFace.Code
    val request = (status as? BridgeAddDeviceStatus.Waiting)?.request

    fun decide(decision: suspend () -> Unit) {
        error = null
        scope.launch {
            try {
                decision()
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("Deciding on the request failed", e)
                show(e)
            }
        }
    }
    LaunchedEffect(face) { if (face == AddDeviceFace.Declined) onDismiss() }

    AddDeviceDialogContent(
        adding = adding,
        face = face,
        error = error,
        onApprove = { request?.let { decide { appHandle.approveAddDevice(it) } } },
        onDecline = { request?.let { decide { appHandle.declineAddDevice(it) } } },
        onDismiss = onDismiss,
    )
}

/**
 * The dialog body, by where the pairing is: a new device's request, to add
 * when its screen shows the number; the name of the device that was added;
 * or until then the code in [adding] to scan or type, a spinner while it
 * starts, and any [error].
 */
@Composable
internal fun AddDeviceDialogContent(
    adding: BridgeAddDevice?,
    face: AddDeviceFace,
    error: DisplayError?,
    onApprove: () -> Unit,
    onDecline: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts =
            addDeviceParts(
                adding = adding,
                face = face,
                error = error,
                onApprove = onApprove,
                onDecline = onDecline,
                onDismiss = onDismiss,
            ),
    )
}

@Composable
internal fun addDeviceParts(
    adding: BridgeAddDevice?,
    face: AddDeviceFace,
    error: DisplayError?,
    onApprove: () -> Unit,
    onDecline: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    if (face is AddDeviceFace.Request) {
        addDeviceRequestParts(face, error, onApprove, onDecline)
    } else {
        DialogParts(
            title = stringResource(R.string.add_device_title),
            content = { AddDeviceBody(adding = adding, face = face, error = error) },
            confirmButton = {
                AndroidButton(kind = ButtonKind.TEXT, onClick = onDismiss) { Text(stringResource(R.string.done)) }
            },
        )
    }

/** A new device's request: Decline in the text colour, Add the one coloured button. */
@Composable
private fun addDeviceRequestParts(
    request: AddDeviceFace.Request,
    error: DisplayError?,
    onApprove: () -> Unit,
    onDecline: () -> Unit,
): DialogParts =
    DialogParts(
        title = stringResource(R.string.join_request_title, request.deviceName),
        message = stringResource(R.string.add_device_request_match),
        content = {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                AndroidMatchCode(request.confirmation, dialog = true)
                error?.let {
                    Spacer(modifier = Modifier.height(ThemeSpace.related))
                    AndroidNotice(it, look = NoticeLook.QUIET)
                }
            }
        },
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = onApprove) {
                Text(stringResource(R.string.onboarding_add_action))
            }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDecline) {
                Text(stringResource(R.string.join_request_decline))
            }
        },
    )

/** The sheet's body when no request is showing: the code, a line saying where the pairing ended, or a spinner. */
@Composable
private fun AddDeviceBody(
    adding: BridgeAddDevice?,
    face: AddDeviceFace,
    error: DisplayError?,
) {
    Column(
        modifier = Modifier.verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        when {
            face is AddDeviceFace.Added -> {
                BodyLine(stringResource(R.string.add_device_added, face.name))
            }

            face is AddDeviceFace.Expired -> {
                BodyLine(stringResource(R.string.add_device_expired))
            }

            face is AddDeviceFace.Refused -> {
                BodyLine(stringResource(R.string.add_device_refused))
            }

            face is AddDeviceFace.Adding -> {
                AndroidSpinner(text = stringResource(R.string.add_device_adding, face.name))
            }

            adding != null -> {
                // Copy and Share hand out the code to type; the QR code holds the key
                // on S3, so a device that scans it asks for none.
                AndroidScanCode(
                    instruction = stringResource(R.string.add_device_instructions),
                    qr = adding.qrText,
                    code = adding.code,
                )
            }

            error == null -> {
                AndroidSpinner(text = stringResource(R.string.add_device_making_code))
            }
        }
        error?.let {
            Spacer(modifier = Modifier.height(ThemeSpace.related))
            AndroidNotice(it, look = NoticeLook.QUIET)
        }
    }
}

@Composable
private fun BodyLine(text: String) {
    Text(text = text, style = ThemeText.body.style, color = MaterialTheme.colorScheme.onSurface)
}
