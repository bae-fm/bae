package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidMoreMenu
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.ButtonKind
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMemberRole

/** A person's page with its bar, titled with their [name]. */
@Composable
internal fun AndroidSettingsPerson(
    member: BridgeMember,
    selfIsAdmin: Boolean,
    name: String,
    adminChanges: List<BridgeAdminChange>,
    callbacks: MemberCallbacks,
    error: DisplayError?,
    onBack: () -> Unit,
) {
    PeopleFrame(title = name, error = error, onBack = onBack) {
        PersonPage(
            member = member,
            selfIsAdmin = selfIsAdmin,
            name = name,
            adminChanges = adminChanges,
            callbacks = callbacks,
        )
    }
}

/** This person's own devices with their bar. */
@Composable
internal fun AndroidSettingsDevices(
    member: BridgeMember,
    callbacks: MemberCallbacks,
    error: DisplayError?,
    onBack: () -> Unit,
) {
    PeopleFrame(title = stringResource(R.string.people_your_devices), error = error, onBack = onBack) {
        DevicesPage(member = member, callbacks = callbacks)
    }
}

/**
 * A person's page: the role changes this device asked for, their role, which an admin changes, removing them from the
 * library, and their devices, each removable when allowed.
 */
@Composable
internal fun PersonPage(
    member: BridgeMember,
    selfIsAdmin: Boolean,
    name: String,
    adminChanges: List<BridgeAdminChange>,
    callbacks: MemberCallbacks,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        val shownChanges = adminChanges.at(AdminChangePlace.Person(member.id))
        if (shownChanges.isNotEmpty()) {
            item { AdminChangesList(changes = shownChanges, onDismiss = callbacks.onDismissChange) }
        }
        item {
            AndroidCard {
                AndroidRow(
                    title = stringResource(R.string.people_role),
                    subtitle = stringResource(member.role.labelRes()),
                    trailing = if (selfIsAdmin && !member.isSelf) ({ RoleToggle(member, callbacks) }) else null,
                )
                if (member.canRemove) {
                    AndroidRow(
                        title = stringResource(R.string.members_remove_person),
                        action = true,
                        destructive = true,
                        onClick = { callbacks.onRemovePerson(member, name) },
                    )
                }
            }
        }
        (member.access as? BridgeMemberAccess.S3AccessKey)?.let { key ->
            item {
                AndroidNote(LocalContext.current.coreString("core.access_key.id", mapOf("key" to key.accessKeyId)))
            }
        }
        item { AndroidSection(text = stringResource(R.string.people_devices)) }
        item { DevicesCard(member = member, callbacks = callbacks, addLast = false) }
        item { FormerDevicesIfAny(member) }
    }
}

/** Makes an admin a member, or a member an admin. */
@Composable
private fun RoleToggle(
    member: BridgeMember,
    callbacks: MemberCallbacks,
) {
    val (nextRole, label) =
        when (member.role) {
            BridgeMemberRole.ADMIN -> BridgeMemberRole.MEMBER to R.string.members_make_member
            BridgeMemberRole.MEMBER -> BridgeMemberRole.ADMIN to R.string.members_make_admin
        }
    AndroidButton(
        kind = ButtonKind.TEXT,
        onClick = { callbacks.onSetRole(member, nextRole) },
    ) { Text(stringResource(label)) }
}

/** This person's own devices: this one reads "This device" with no Remove, and Add device is last. */
@Composable
internal fun DevicesPage(
    member: BridgeMember,
    callbacks: MemberCallbacks,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        item { DevicesCard(member = member, callbacks = callbacks, addLast = true) }
        item { FormerDevicesIfAny(member) }
    }
}

@Composable
private fun FormerDevicesIfAny(member: BridgeMember) {
    if (member.formerDevices.isNotEmpty()) {
        // The design sets the removed devices a gap of their own below the devices card.
        Box(modifier = Modifier.padding(top = ThemeSpace.related)) { FormerDevices(devices = member.formerDevices) }
    }
}

@Composable
private fun DevicesCard(
    member: BridgeMember,
    callbacks: MemberCallbacks,
    addLast: Boolean,
) {
    AndroidCard {
        member.activeDevices.forEach { device ->
            AndroidRow(
                title = device.name,
                subtitle = if (device.isSelf) stringResource(R.string.members_this_device) else null,
                trailing = if (device.canRemove) ({ RemoveDeviceButton(device, callbacks) }) else null,
            )
        }
        if (addLast && member.isSelf) {
            AndroidRow(
                title = stringResource(R.string.members_add_device),
                action = true,
                leadingIcon = Icons.Filled.Add,
                onClick = callbacks.onAddDevice,
            )
        }
    }
}

@Composable
private fun RemoveDeviceButton(
    device: BridgeActiveDevice,
    callbacks: MemberCallbacks,
) {
    AndroidMoreMenu { close ->
        DropdownMenuItem(
            text = { Text(stringResource(R.string.members_remove_menu)) },
            onClick = {
                close()
                callbacks.onRemoveDevice(device)
            },
        )
    }
}
