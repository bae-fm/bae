package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.DestructiveTextButton
import fm.bae.app.ui.components.LabeledValueRow
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeDeviceCutOff
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeProviderSignOut
import uniffi.bae_bridge.bridgeProviderSignOutKey

private val logger = BaeLogger("bae.MembersDialogs")

/** A removal waiting on the person's confirmation. */
internal sealed interface PendingRemoval {
    data class Person(
        val member: BridgeMember,
        val name: String,
    ) : PendingRemoval

    data class Device(
        val device: BridgeActiveDevice,
        /** Whether it is this person's own device. */
        val own: Boolean,
    ) : PendingRemoval
}

/**
 * The provider step a removed device needs; its own person, who did the step, tells the library it is done when
 * their account holds the library's storage.
 */
internal data class CutOffStep(
    val device: ULong,
    val instruction: BridgeProviderSignOut,
    val offersCutOff: Boolean,
)

/** A removed device whose cut-off confirmation changed nothing, and why. */
internal data class CutOffRetry(
    val device: ULong,
    val reason: BridgeDeviceCutOff,
)

/** Which dialog the people screen shows, if any. */
internal class MembersDialogs {
    var inviting by mutableStateOf(false)
    var addingDevice by mutableStateOf(false)
    var pendingRemoval by mutableStateOf<PendingRemoval?>(null)
    var signOut by mutableStateOf<CutOffStep?>(null)

    /** The removed device whose cut-off confirmation changed nothing: confirming again works later. */
    var cutOffRetry by mutableStateOf<CutOffRetry?>(null)

    /** The open invite shown again, by id; it closes when the invite ends. */
    var shownInvite by mutableStateOf<String?>(null)

    /** Invites whose request dialog the person closed; the invite's row opens it again. */
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
                remove(model, session, dialogs, removal)
            },
            onDismiss = { dialogs.pendingRemoval = null },
        )
    }
    dialogs.signOut?.let { step ->
        SignOutDialog(
            step = step,
            onConfirm = {
                dialogs.signOut = null
                confirmCutOff(model, session, dialogs, step.device)
            },
            onDismiss = { dialogs.signOut = null },
        )
    }
    dialogs.cutOffRetry?.let { retry ->
        CutOffRetryDialog(
            reason = retry.reason,
            onTryAgain = {
                dialogs.cutOffRetry = null
                confirmCutOff(model, session, dialogs, retry.device)
            },
            onDismiss = { dialogs.cutOffRetry = null },
        )
    }
}

private fun remove(
    model: MembersModel,
    session: OpenLibrary,
    dialogs: MembersDialogs,
    removal: PendingRemoval,
) {
    when (removal) {
        is PendingRemoval.Person -> {
            model.perform("remove a person") { session.appHandle.removeMember(removal.member.id) }
        }

        is PendingRemoval.Device -> {
            model.perform("remove a device") {
                val instruction = session.appHandle.removeDevice(removal.device.id)
                val offersCutOff = removal.own && holdsStore(session)
                dialogs.signOut = instruction?.let { CutOffStep(removal.device.id, it, offersCutOff) }
            }
        }
    }
}

/**
 * Whether this person's account holds the library's storage. When the read fails nothing is offered, since confirming
 * would only be refused.
 */
private suspend fun holdsStore(session: OpenLibrary): Boolean =
    try {
        session.appHandle.holdsStore()
    } catch (e: CancellationException) {
        throw e
    } catch (e: BridgeException) {
        logger.error("reading whether this account holds the storage failed", e)
        false
    }

/** The cut-off confirmation changed nothing: a request to the provider ran, or the removal hasn't arrived yet. */
@Composable
private fun CutOffRetryDialog(
    reason: BridgeDeviceCutOff,
    onTryAgain: () -> Unit,
    onDismiss: () -> Unit,
) {
    val notGone = reason == BridgeDeviceCutOff.DEVICE_NOT_GONE
    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                stringResource(
                    if (notGone) R.string.device_cut_off_not_gone_title else R.string.device_cut_off_running_title,
                ),
            )
        },
        text = {
            Text(
                stringResource(
                    if (notGone) R.string.device_cut_off_not_gone_body else R.string.device_cut_off_running_body,
                ),
            )
        },
        confirmButton = { TextButton(onClick = onTryAgain) { Text(stringResource(R.string.retry)) } },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.device_cut_off_not_yet)) }
        },
    )
}

/** Tell the library the person cut a removed device off at the provider. */
private fun confirmCutOff(
    model: MembersModel,
    session: OpenLibrary,
    dialogs: MembersDialogs,
    device: ULong,
) {
    model.perform("confirm a device cut off") {
        val outcome = session.appHandle.confirmDeviceCutOff(device)
        when (outcome) {
            BridgeDeviceCutOff.CONFIRMED -> {}

            BridgeDeviceCutOff.PROVIDER_REQUEST_RUNNING,
            BridgeDeviceCutOff.DEVICE_NOT_GONE,
            -> {
                dialogs.cutOffRetry = CutOffRetry(device, outcome)
            }
        }
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
    step: CutOffStep,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val instruction = step.instruction
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
            if (step.offersCutOff) {
                TextButton(onClick = onConfirm) { Text(stringResource(R.string.device_cut_off_confirm)) }
            } else {
                TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
            }
        },
        dismissButton =
            if (step.offersCutOff) {
                { TextButton(onClick = onDismiss) { Text(stringResource(R.string.device_cut_off_not_yet)) } }
            } else {
                null
            },
    )
}

/**
 * An invite's request: the person's name and the number their device
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
            DestructiveTextButton(
                onClick = { model.perform("decline a join request") { session.appHandle.declineJoinRequest(request) } },
            ) {
                Text(stringResource(R.string.join_request_decline))
            }
        },
    )
}
