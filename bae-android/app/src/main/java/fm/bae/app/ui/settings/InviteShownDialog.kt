package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
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
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidScanCode
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.sync.ShareBar
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeStorageLocation

private val logger = BaeLogger("bae.InviteShownDialog")

/**
 * One of this device's open invites, shown again. While it shows, this device
 * syncs every few seconds, so the invite's request arrives soon after it is
 * sent. [storage] is where the library is stored, which names the provider
 * the invite shares through.
 */
@Composable
internal fun InviteShownDialog(
    session: OpenLibrary,
    invite: BridgeInvite,
    storage: BridgeStorageLocation?,
    onDismiss: () -> Unit,
) {
    val scope = rememberScreenScope()
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var busy by remember { mutableStateOf(false) }
    var confirmingCancel by remember { mutableStateOf(false) }
    val errors = session.errors
    val line by session.syncStatusStore.line.collectAsState()
    val offline = line.isOffline
    // The hold ends as the dialog leaves the screen.
    val hold = remember { session.appHandle.holdInviteScreenSync() }
    DisposableEffect(hold) { onDispose { hold.release() } }
    if (confirmingCancel) {
        CancelInviteConfirmDialog(
            invite = invite,
            onConfirm = {
                confirmingCancel = false
                busy = true
                error = null
                scope.launch {
                    // A cancelled invite leaves the list, which closes this dialog.
                    performBridgeAction(logger, "cancel an invite", errors, { error = it }) {
                        session.appHandle.cancelInvite(invite.id)
                    }
                    busy = false
                }
            },
            onDismiss = { confirmingCancel = false },
        )
        return
    }
    InviteCodeDialog(
        invite = invite,
        providerName = storage?.let { cloudProviderLabel(storageProvider(it)) }.orEmpty(),
        offline = offline,
        error = error,
        busy = busy,
        onCancelInvite = { confirmingCancel = true },
        onDismiss = onDismiss,
    )
}

/**
 * Asks before cancelling a shown invite. On S3 it names the invite's key: bae
 * keeps no record of it afterwards, so the person deletes it in the
 * provider's console.
 */
@Composable
internal fun CancelInviteConfirmDialog(
    invite: BridgeInvite,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts = cancelInviteParts(invite = invite, onConfirm = onConfirm, onDismiss = onDismiss),
    )
}

@Composable
internal fun cancelInviteParts(
    invite: BridgeInvite,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts {
    val context = LocalContext.current
    val forgetLine =
        (invite.access as? BridgeMemberAccess.S3AccessKey)?.let { key ->
            context.coreString("core.access_key.forget", mapOf("key" to key.accessKeyId))
        }
    val wontUse = stringResource(R.string.invite_cancel_message, invite.label)
    return DialogParts(
        title = context.coreString("core.invite.cancel_confirm.title"),
        message = if (forgetLine == null) wontUse else "$wontUse\n\n$forgetLine",
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = onConfirm) { Text(stringResource(R.string.invite_cancel)) }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(context.coreString("core.invite.cancel_confirm.keep"))
            }
        },
    )
}

/**
 * The invite's dialog: while sync shares the library with the invited address,
 * a bar saying so, or what holds it; while this device is offline, that the
 * code shows once it is online and the library is shared; then the code to
 * scan or enter on their device. Cancel invite sits beside Done.
 */
@Composable
internal fun InviteCodeDialog(
    invite: BridgeInvite,
    providerName: String,
    offline: Boolean,
    error: DisplayError?,
    busy: Boolean,
    onCancelInvite: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts =
            inviteCodeParts(
                invite = invite,
                providerName = providerName,
                offline = offline,
                error = error,
                busy = busy,
                onCancelInvite = onCancelInvite,
                onDismiss = onDismiss,
            ),
    )
}

@Composable
internal fun inviteCodeParts(
    invite: BridgeInvite,
    providerName: String,
    offline: Boolean,
    error: DisplayError?,
    busy: Boolean,
    onCancelInvite: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts {
    val context = LocalContext.current
    val title = stringResource(R.string.invite_title)
    // Offline with nothing held: the dialog says the code waits, in place of a bar.
    val waitingOffline = offline && invite.shown is BridgeInviteShown.Granting && invite.held == null
    return DialogParts(
        title = title,
        message =
            if (waitingOffline) {
                stringResource(R.string.invite_offline, providerName, invite.label)
            } else {
                null
            },
        content = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                when (val shown = invite.shown) {
                    is BridgeInviteShown.Granting -> {
                        InviteGranting(invite, offline)
                    }

                    is BridgeInviteShown.Code -> {
                        AndroidScanCode(
                            instruction = stringResource(R.string.invite_scan_instruction),
                            qr = shown.code,
                            code = shown.code,
                        )
                    }
                }
                (invite.access as? BridgeMemberAccess.S3AccessKey)?.let { key ->
                    SelectionContainer {
                        InviteNote(context.coreString("core.access_key.id", mapOf("key" to key.accessKeyId)))
                    }
                    InviteNote(context.coreString("core.access_key.unused_invite", mapOf("key" to key.accessKeyId)))
                }
                error?.let { AndroidNotice(it, look = NoticeLook.QUIET) }
            }
        },
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = onDismiss) { Text(stringResource(R.string.done)) }
        },
        dismissButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                tone = ButtonTone.NEUTRAL,
                onClick = onCancelInvite,
                enabled = !busy,
            ) {
                Text(stringResource(R.string.invite_cancel))
            }
        },
    )
}

/**
 * Sync hasn't shared the library with the invited address yet: what holds it when something does, as a
 * refused check or a failed grant; nothing while this device is offline, which the dialog's message says;
 * else a bar saying it is being shared.
 */
@Composable
private fun InviteGranting(
    invite: BridgeInvite,
    offline: Boolean,
) {
    val context = LocalContext.current
    val held = invite.held
    when {
        held != null -> {
            Text(
                text = context.coreString(held.reason.key),
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.error,
            )
            held.reason.detail?.let { detail ->
                SelectionContainer { Text(text = detail, style = ThemeText.mono.style) }
            }
        }

        offline -> {}

        else -> {
            Column(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                val sharing = stringResource(R.string.invite_granting, invite.label)
                Text(text = sharing, style = ThemeText.body.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
                ShareBar(share = null, description = sharing)
            }
        }
    }
}

@Composable
private fun InviteNote(text: String) {
    Text(text = text, style = ThemeText.detail.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
}
