package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
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
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.QRCodeImage
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeStorageLocation
import java.text.DateFormat
import java.util.Date

private val logger = BaeLogger("bae.InviteDialog")

private val qrSize = 220.dp

/**
 * Invite a person: their role, and the access they reach the storage with —
 * an S3 access key, or the provider account they sign in with. The invite
 * carries the library's name and expires within a day; it can be cancelled
 * while shown.
 */
@Composable
internal fun InviteDialog(
    session: OpenLibrary,
    storage: BridgeStorageLocation?,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var invite by remember { mutableStateOf<BridgeInvite?>(null) }
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var busy by remember { mutableStateOf(false) }
    var confirmingCancel by remember { mutableStateOf(false) }
    val errors = LocaleErrorDisplays(context)

    val created = invite
    if (created == null) {
        InviteFormDialog(
            storage = storage,
            error = error,
            busy = busy,
            onCreate = { role, access ->
                busy = true
                error = null
                scope.launch {
                    performBridgeAction(logger, "invite a person", errors, { error = it }) {
                        invite = session.appHandle.invitePerson(role, access)
                    }
                    busy = false
                }
            },
            onDismiss = onDismiss,
        )
        return
    }
    if (confirmingCancel) {
        CancelInviteConfirmDialog(
            invite = created,
            onConfirm = {
                confirmingCancel = false
                busy = true
                scope.launch {
                    var cancelled = false
                    performBridgeAction(logger, "cancel an invite", errors, { error = it }) {
                        session.appHandle.cancelInvite(created.id)
                        cancelled = true
                    }
                    busy = false
                    if (cancelled) onDismiss()
                }
            },
            onDismiss = { confirmingCancel = false },
        )
        return
    }
    InviteCreatedDialog(
        invite = created,
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

/** What the person typed for the invited person's access, as the storage asks for it. */
private class InviteAccessInput {
    var email by mutableStateOf("")
    var accessKeyId by mutableStateOf("")
    var secretAccessKey by mutableStateOf("")

    /** The access for [storage], or null while what it needs is incomplete. */
    fun access(storage: BridgeStorageLocation): BridgeInviteAccess? =
        if (storage is BridgeStorageLocation.S3) {
            if (accessKeyId.isEmpty() || secretAccessKey.isEmpty()) {
                null
            } else {
                BridgeInviteAccess.S3AccessKey(accessKeyId, secretAccessKey)
            }
        } else {
            email.takeIf { it.isNotEmpty() }?.let { BridgeInviteAccess.ProviderAccount(it) }
        }
}

/** The invite's role and the access it carries, asked for as the library's storage needs it. */
@Composable
private fun InviteFormDialog(
    storage: BridgeStorageLocation?,
    error: DisplayError?,
    busy: Boolean,
    onCreate: (BridgeMemberRole, BridgeInviteAccess) -> Unit,
    onDismiss: () -> Unit,
) {
    var role by remember { mutableStateOf(BridgeMemberRole.MEMBER) }
    val input = remember { InviteAccessInput() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.members_invite)) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                RoleChoice(role) { role = it }
                if (storage == null) {
                    CircularProgressIndicator()
                } else {
                    InviteAccessFields(storage, input)
                }
                error?.let { ErrorDetailDisclosure(it) }
            }
        },
        confirmButton = {
            val access = storage?.let(input::access)
            TextButton(enabled = access != null && !busy, onClick = { access?.let { onCreate(role, it) } }) {
                Text(stringResource(R.string.invite_create))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/** The fields for the invited person's access to [storage]: an S3 access key, or their provider account. */
@Composable
private fun InviteAccessFields(
    storage: BridgeStorageLocation,
    input: InviteAccessInput,
) {
    if (storage is BridgeStorageLocation.S3) {
        InviteNote(stringResource(R.string.invite_s3_explanation))
        S3KeyFields(
            accessKeyId = input.accessKeyId,
            secretAccessKey = input.secretAccessKey,
            onAccessKeyIdChange = { input.accessKeyId = it.trim() },
            onSecretAccessKeyChange = { input.secretAccessKey = it.trim() },
        )
    } else {
        InviteNote(stringResource(R.string.invite_email_explanation, cloudProviderLabel(storageProvider(storage))))
        OutlinedTextField(
            value = input.email,
            onValueChange = { input.email = it.trim() },
            label = { Text(stringResource(R.string.invite_email)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, autoCorrectEnabled = false),
        )
    }
}

@Composable
private fun InviteNote(text: String) {
    Text(text = text, style = ThemeText.detail.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun RoleChoice(
    role: BridgeMemberRole,
    onChoose: (BridgeMemberRole) -> Unit,
) {
    val roles = listOf(BridgeMemberRole.MEMBER, BridgeMemberRole.ADMIN)
    SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
        roles.forEachIndexed { index, choice ->
            SegmentedButton(
                selected = role == choice,
                onClick = { onChoose(choice) },
                shape = SegmentedButtonDefaults.itemShape(index, roles.size),
            ) {
                Text(
                    stringResource(
                        when (choice) {
                            BridgeMemberRole.MEMBER -> R.string.members_role_member
                            BridgeMemberRole.ADMIN -> R.string.members_role_admin
                        },
                    ),
                )
            }
        }
    }
}

/** The invite to show the person joining: a QR to scan in bae, the code to type, and when it expires. */
@Composable
private fun InviteCreatedDialog(
    invite: BridgeInvite,
    error: DisplayError?,
    busy: Boolean,
    onCancelInvite: () -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val expires =
        remember(invite) {
            DateFormat
                .getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, context.currentLocale())
                .format(Date.from(invite.expiresAt))
        }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.invite_title)) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                InviteNote(stringResource(R.string.invite_instructions))
                QRCodeImage(
                    text = invite.code,
                    contentDescription = stringResource(R.string.invite_title),
                    modifier = Modifier.size(qrSize),
                )
                SelectionContainer { Text(text = invite.code, style = ThemeText.mono.style) }
                InviteNote(stringResource(R.string.invite_expires, expires))
                (invite.access as? BridgeMemberAccess.S3AccessKey)?.let { key ->
                    SelectionContainer {
                        InviteNote(context.coreString("core.access_key.id", mapOf("key" to key.accessKeyId)))
                    }
                    InviteNote(context.coreString("core.access_key.unused_invite", mapOf("key" to key.accessKeyId)))
                }
                TextButton(onClick = { clipboard.setText(AnnotatedString(invite.code)) }) {
                    Text(stringResource(R.string.copy_code))
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
