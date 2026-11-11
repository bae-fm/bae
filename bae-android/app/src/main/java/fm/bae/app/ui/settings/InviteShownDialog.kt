package fm.bae.app.ui.settings

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
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
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.currentLocale
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.QRCodeImage
import fm.bae.app.ui.components.rememberCopyText
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgePendingWait
import java.text.DateFormat
import java.time.Instant
import java.util.Date

private val logger = BaeLogger("bae.InviteShownDialog")

/**
 * One of this device's open invites, shown again: while sync grants its
 * access, that it is doing so, and why coven holds it when it does; then the
 * QR code with Copy and Share; and Cancel invite.
 */
@Composable
internal fun InviteShownDialog(
    session: OpenLibrary,
    invite: BridgeInvite,
    onDismiss: () -> Unit,
) {
    val scope = rememberScreenScope()
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var busy by remember { mutableStateOf(false) }
    var confirmingCancel by remember { mutableStateOf(false) }
    val errors = session.errors
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
private fun CancelInviteConfirmDialog(
    invite: BridgeInvite,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val forgetLine =
        (invite.access as? BridgeMemberAccess.S3AccessKey)?.let { key ->
            context.coreString("core.access_key.forget", mapOf("key" to key.accessKeyId))
        }
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = context.coreString("core.invite.cancel_confirm.title"),
        message = forgetLine,
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

/** When an invite expires, as the device's locale writes a date and time. */
internal fun Context.inviteExpiry(expiresAt: Instant): String =
    DateFormat
        .getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, currentLocale())
        .format(Date.from(expiresAt))

/**
 * The invite to give the person joining: a QR code to scan in bae, and the
 * code to copy or share.
 */
@Composable
private fun InviteCodeDialog(
    invite: BridgeInvite,
    error: DisplayError?,
    busy: Boolean,
    onCancelInvite: () -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val copy = rememberCopyText()
    val title = invite.invitee ?: stringResource(R.string.invite_title)
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = title,
        content = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                when (val shown = invite.shown) {
                    is BridgeInviteShown.Granting -> {
                        InviteGranting(invite.held)
                    }

                    is BridgeInviteShown.Code -> {
                        InviteCode(
                            code = shown.code,
                            title = title,
                            onCopy = { copy(shown.code) },
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

/** Sync hasn't granted the invite's access yet; a refused check or a failed grant says why. */
@Composable
private fun InviteGranting(held: BridgePendingWait?) {
    val context = LocalContext.current
    when (held) {
        null -> {
            AndroidSpinner(text = stringResource(R.string.invite_granting))
        }

        else -> {
            Text(
                text = context.coreString(held.reason.key),
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.error,
            )
            held.reason.detail?.let { detail ->
                SelectionContainer { Text(text = detail, style = ThemeText.mono.style) }
            }
        }
    }
}

/**
 * The code to give the person joining: a QR code for one beside the admin to
 * scan in bae, and the code to copy or share for one elsewhere.
 */
@Composable
private fun InviteCode(
    code: String,
    title: String,
    onCopy: () -> Unit,
) {
    val context = LocalContext.current
    QRCodeImage(text = code, contentDescription = title)
    Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        AndroidButton(kind = ButtonKind.TEXT, onClick = onCopy) { Text(stringResource(R.string.copy_code)) }
        AndroidButton(kind = ButtonKind.TEXT, onClick = { context.shareInviteCode(code) }) {
            Text(stringResource(R.string.invite_share))
        }
    }
}

/** Hand an invite's code to the app the person picks: a message, an email. */
private fun Context.shareInviteCode(code: String) {
    val send =
        Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, code)
        }
    startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
}

@Composable
private fun InviteNote(text: String) {
    Text(text = text, style = ThemeText.detail.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
}
