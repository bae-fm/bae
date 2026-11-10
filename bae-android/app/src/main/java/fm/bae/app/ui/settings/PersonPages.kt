package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CardRow
import fm.bae.app.ui.components.DestructiveTextButton
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.RowCard
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMemberRole

/**
 * A person's page: their role, which an admin changes, removing them from the library, and their devices, each
 * removable when allowed.
 */
@Composable
internal fun PersonPage(
    member: BridgeMember,
    selfIsAdmin: Boolean,
    name: String,
    callbacks: MemberCallbacks,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        item {
            RowCard {
                CardRow(
                    title = stringResource(R.string.people_role),
                    subtitle = stringResource(member.role.labelRes()),
                    trailing = if (selfIsAdmin && !member.isSelf) ({ RoleToggle(member, callbacks) }) else null,
                )
                if (member.canRemove) {
                    CardRow(
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
                Text(
                    text = LocalContext.current.coreString("core.access_key.id", mapOf("key" to key.accessKeyId)),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        item { Eyebrow(text = stringResource(R.string.people_devices)) }
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
    TextButton(onClick = { callbacks.onSetRole(member, nextRole) }) { Text(stringResource(label)) }
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
    if (member.formerDevices.isNotEmpty()) FormerDevices(devices = member.formerDevices)
}

@Composable
private fun DevicesCard(
    member: BridgeMember,
    callbacks: MemberCallbacks,
    addLast: Boolean,
) {
    RowCard {
        member.activeDevices.forEach { device ->
            CardRow(
                title = device.name,
                subtitle = if (device.isSelf) stringResource(R.string.members_this_device) else null,
                trailing = if (device.canRemove) ({ RemoveDeviceButton(device, member.isSelf, callbacks) }) else null,
            )
        }
        if (addLast && member.isSelf) {
            CardRow(
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
    own: Boolean,
    callbacks: MemberCallbacks,
) {
    DestructiveTextButton(onClick = { callbacks.onRemoveDevice(device, own) }) {
        Text(stringResource(R.string.members_remove))
    }
}

@Preview(showBackground = true)
@Composable
private fun DevicesPagePreview() {
    BaeTheme {
        Column(modifier = Modifier.padding(ThemeSpace.edge)) {
            DevicesCard(
                member = PreviewData.member(),
                callbacks = MemberCallbacks({}, { _, _ -> }, { _, _ -> }, { _, _ -> }, {}, {}),
                addLast = true,
            )
        }
    }
}
