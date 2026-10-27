package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.LabeledValueRow
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeProviderSignOut
import uniffi.bae_bridge.bridgeProviderSignOutKey

/** A removal waiting on the person's confirmation. */
internal sealed interface PendingRemoval {
    data class Person(
        val member: BridgeMember,
        val name: String,
    ) : PendingRemoval

    data class Device(
        val device: BridgeActiveDevice,
    ) : PendingRemoval
}

/** Which dialog the people screen shows, if any. */
internal class MembersDialogs {
    var inviting by mutableStateOf(false)
    var addingDevice by mutableStateOf(false)
    var pendingRemoval by mutableStateOf<PendingRemoval?>(null)
    var signOut by mutableStateOf<BridgeProviderSignOut?>(null)

    /** The open invite shown again, by id; it closes when the invite ends. */
    var shownInvite by mutableStateOf<String?>(null)

    /** Sent invites whose request dialog the person closed; the invite's row opens it again. */
    var closedRequests by mutableStateOf<Set<String>>(emptySet())

    /** Open an invite from its row: its request to decide, or its code. */
    fun open(invite: BridgeInvite) {
        if (invite.requestToDecide != null) {
            closedRequests = closedRequests - invite.id
        } else {
            shownInvite = invite.id
        }
    }
}

@Composable
internal fun MembersDialogsShown(
    session: OpenLibrary,
    model: MembersModel,
    dialogs: MembersDialogs,
) {
    if (dialogs.inviting) {
        InviteDialog(
            session = session,
            storage = model.storage,
            onCreated = { id ->
                dialogs.inviting = false
                dialogs.shownInvite = id
            },
            onDismiss = { dialogs.inviting = false },
        )
    }
    model.invites.firstOrNull { it.id == dialogs.shownInvite }?.let { invite ->
        InviteShownDialog(session = session, invite = invite, onDismiss = { dialogs.shownInvite = null })
    }
    model.invites
        .firstOrNull { it.requestToDecide != null && it.id !in dialogs.closedRequests }
        ?.let { invite ->
            JoinRequestDialog(
                invite = invite,
                model = model,
                session = session,
                onClose = { dialogs.closedRequests = dialogs.closedRequests + invite.id },
            )
        }
    if (dialogs.addingDevice) {
        AddDeviceDialog(appHandle = session.appHandle, onDismiss = { dialogs.addingDevice = false })
    }
    dialogs.pendingRemoval?.let { removal ->
        RemovalConfirmDialog(
            removal = removal,
            onConfirm = {
                dialogs.pendingRemoval = null
                when (removal) {
                    is PendingRemoval.Person -> {
                        model.perform("remove a person") { session.appHandle.removeMember(removal.member.id) }
                    }

                    is PendingRemoval.Device -> {
                        model.perform("remove a device") {
                            dialogs.signOut = session.appHandle.removeDevice(removal.device.id)
                        }
                    }
                }
            },
            onDismiss = { dialogs.pendingRemoval = null },
        )
    }
    dialogs.signOut?.let { instruction ->
        SignOutDialog(instruction = instruction, onDismiss = { dialogs.signOut = null })
    }
}

@Composable
private fun RemovalConfirmDialog(
    removal: PendingRemoval,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val (title, body) =
        when (removal) {
            is PendingRemoval.Person -> {
                stringResource(R.string.members_remove_person_confirm_title, removal.name) to
                    stringResource(R.string.members_remove_person_confirm_body)
            }

            is PendingRemoval.Device -> {
                stringResource(R.string.members_remove_confirm_title) to
                    stringResource(R.string.members_remove_confirm_body, removal.device.name)
            }
        }
    // Removing a person ends their S3 key's use; bae keeps no record of it.
    val forgottenKey = ((removal as? PendingRemoval.Person)?.member?.access as? BridgeMemberAccess.S3AccessKey)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                Text(body)
                forgottenKey?.let { key ->
                    Text(context.coreString("core.access_key.forget", mapOf("key" to key.accessKeyId)))
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(stringResource(R.string.members_remove)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/** After a device is removed: what to do at the provider so it can't reach the storage any more. */
@Composable
private fun SignOutDialog(
    instruction: BridgeProviderSignOut,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.device_removed_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                if (instruction is BridgeProviderSignOut.RemoveAppAccess) {
                    LabeledValueRow(
                        stringResource(R.string.settings_provider),
                        cloudProviderLabel(instruction.provider),
                    )
                }
                Text(context.coreString(bridgeProviderSignOutKey(instruction)))
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}

/**
 * A sent invite's request: the person's name and the number their device
 * shows, to compare before approving. Closing it keeps the request; the
 * invite's row opens it again.
 */
@Composable
private fun JoinRequestDialog(
    invite: BridgeInvite,
    model: MembersModel,
    session: OpenLibrary,
    onClose: () -> Unit,
) {
    val request = invite.requestToDecide ?: return
    val info = remember(request) { request.info() }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text(invite.invitee ?: info.deviceName) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                Text(stringResource(R.string.join_request_confirmation), style = ThemeText.detail.style)
                Text(info.confirmation, style = ThemeText.title.style)
            }
        },
        confirmButton = {
            TextButton(
                onClick = { model.perform("approve a join request") { session.appHandle.approveJoinRequest(request) } },
            ) {
                Text(stringResource(R.string.join_request_approve))
            }
        },
        dismissButton = {
            TextButton(
                onClick = { model.perform("decline a join request") { session.appHandle.declineJoinRequest(request) } },
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) {
                Text(stringResource(R.string.join_request_decline))
            }
        },
    )
}
