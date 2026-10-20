package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.LabeledValueRow
import uniffi.bae_bridge.BridgeDevice
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeProviderSignOut
import uniffi.bae_bridge.bridgeProviderSignOutKey

/** A removal waiting on the person's confirmation. */
internal sealed interface PendingRemoval {
    data class Person(
        val member: BridgeMember,
        val name: String,
    ) : PendingRemoval

    data class Device(
        val device: BridgeDevice,
    ) : PendingRemoval
}

/** Which dialog the people screen shows, if any. */
internal class MembersDialogs {
    var inviting by mutableStateOf(false)
    var addingDevice by mutableStateOf(false)
    var pendingRemoval by mutableStateOf<PendingRemoval?>(null)
    var signOut by mutableStateOf<BridgeProviderSignOut?>(null)
}

@Composable
internal fun MembersDialogsShown(
    session: OpenLibrary,
    model: MembersModel,
    dialogs: MembersDialogs,
) {
    if (dialogs.inviting) {
        InviteDialog(session = session, storage = model.storage, onDismiss = { dialogs.inviting = false })
    }
    if (dialogs.addingDevice) {
        AddDeviceDialog(session = session, onDismiss = { dialogs.addingDevice = false })
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
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { Text(body) },
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
