package fm.bae.app.ui.settings

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.currentLocale
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.QRCodeImage
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteDeliveryKind
import uniffi.bae_bridge.BridgeMemberAccess
import java.text.DateFormat
import java.time.Instant
import java.util.Date

private val logger = BaeLogger("bae.InviteShownDialog")

/**
 * One of this device's open invites, shown again: the QR code of an in-person
 * invite, or the code of a sent one with Copy and Share, and Cancel invite.
 */
@Composable
internal fun InviteShownDialog(
    session: OpenLibrary,
    invite: BridgeInvite,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var busy by remember { mutableStateOf(false) }
    var confirmingCancel by remember { mutableStateOf(false) }
    val errors = LocaleErrorDisplays(context)
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
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(context.coreString("core.invite.cancel_confirm.title")) },
        text = forgetLine?.let { line -> { Text(line) } },
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(stringResource(R.string.invite_cancel)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(context.coreString("core.invite.cancel_confirm.keep")) }
        },
    )
}

/** When an invite expires, as the device's locale writes a date and time. */
internal fun Context.inviteExpiry(expiresAt: Instant): String =
    DateFormat
        .getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, currentLocale())
        .format(Date.from(expiresAt))

/**
 * The invite to give the person joining: in person, a QR code to scan in
 * bae; sent, the code to copy or share. Either says when it expires.
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
    val clipboard = LocalClipboardManager.current
    val expires = remember(invite.expiresAt) { context.inviteExpiry(invite.expiresAt) }
    val title = invite.invitee ?: stringResource(R.string.invite_title)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                when (invite.delivery) {
                    BridgeInviteDeliveryKind.IN_PERSON -> {
                        QRCodeImage(
                            text = invite.code,
                            contentDescription = title,
                        )
                    }

                    BridgeInviteDeliveryKind.SENT -> {
                        SelectionContainer { Text(text = invite.code, style = ThemeText.mono.style) }
                        Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                            TextButton(onClick = { clipboard.setText(AnnotatedString(invite.code)) }) {
                                Text(stringResource(R.string.copy_code))
                            }
                            TextButton(onClick = { context.shareInviteCode(invite.code) }) {
                                Text(stringResource(R.string.invite_share))
                            }
                        }
                    }
                }
                InviteNote(stringResource(R.string.invite_expires, expires))
                (invite.access as? BridgeMemberAccess.S3AccessKey)?.let { key ->
                    SelectionContainer {
                        InviteNote(context.coreString("core.access_key.id", mapOf("key" to key.accessKeyId)))
                    }
                    InviteNote(context.coreString("core.access_key.unused_invite", mapOf("key" to key.accessKeyId)))
                }
                error?.let { ErrorDetailDisclosure(it) }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.done)) }
        },
        dismissButton = {
            TextButton(onClick = onCancelInvite, enabled = !busy) { Text(stringResource(R.string.invite_cancel)) }
        },
    )
}

/** Hand a sent invite's code to the app the person picks: a message, an email. */
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
