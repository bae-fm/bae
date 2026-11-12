package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.R
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.StatusChip
import fm.bae.app.ui.components.StatusTone
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeAdminChangeKind
import uniffi.bae_bridge.BridgeAdminChangeState
import uniffi.bae_bridge.BridgeDeviceStop
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeOperationRejection

/** Where an admin change shows: next to what it changes. */
internal sealed interface AdminChangePlace {
    /** A rename, under the library's name. */
    data object Library : AdminChangePlace

    /** A role change, on that person's page. */
    data class Person(
        val memberId: String,
    ) : AdminChangePlace

    /** Removing a person or a device, and letting a person in, on People and devices. */
    data object People : AdminChangePlace
}

internal fun BridgeAdminChange.place(): AdminChangePlace =
    when (val kind = change) {
        is BridgeAdminChangeKind.RenameLibrary -> AdminChangePlace.Library

        is BridgeAdminChangeKind.SetRole -> AdminChangePlace.Person(kind.member)

        is BridgeAdminChangeKind.RemovePerson,
        is BridgeAdminChangeKind.RemoveDevice,
        is BridgeAdminChangeKind.Approve,
        -> AdminChangePlace.People
    }

/** The admin changes that show in [place]. */
internal fun List<BridgeAdminChange>.at(place: AdminChangePlace) = filter { it.place() == place }

/**
 * The admin changes this phone asked for that show in one place: renaming the library, changing a role, removing a
 * person or a device. Each shows waiting until sync publishes it, then done, refused with why, or stopped with why
 * this phone stopped syncing for good before it was made, until the person dismisses it. Nothing shows while there is
 * none.
 */
@Composable
internal fun AdminChangesList(
    changes: List<BridgeAdminChange>,
    onDismiss: (BridgeAdminChange) -> Unit,
) {
    if (changes.isEmpty()) return
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        AndroidSection(text = stringResource(R.string.admin_changes_title))
        AndroidCard {
            changes.forEach { change -> AdminChangeRow(change = change, onDismiss = { onDismiss(change) }) }
        }
    }
}

@Composable
private fun AdminChangeRow(
    change: BridgeAdminChange,
    onDismiss: () -> Unit,
) {
    AndroidRow(
        title = change.change.description(),
        subtitle = change.state.whyRes()?.let { stringResource(it) },
        trailing = {
            when (change.state) {
                BridgeAdminChangeState.Waiting -> {
                    StatusChip(stringResource(R.string.admin_change_waiting))
                }

                BridgeAdminChangeState.Done -> {
                    StatusChip(stringResource(R.string.done), tone = StatusTone.SUCCESS)
                }

                is BridgeAdminChangeState.Refused -> {
                    StatusChip(stringResource(R.string.admin_change_refused), tone = StatusTone.DANGER)
                }

                is BridgeAdminChangeState.Stopped -> {
                    StatusChip(stringResource(R.string.admin_change_stopped), tone = StatusTone.DANGER)
                }
            }
            if (change.state != BridgeAdminChangeState.Waiting) {
                AndroidButton(kind = ButtonKind.TEXT, onClick = onDismiss) { Text(stringResource(R.string.dismiss)) }
            }
        },
    )
}

/** Why a change ended without being made; null while it waits or once done. */
private fun BridgeAdminChangeState.whyRes(): Int? =
    when (this) {
        BridgeAdminChangeState.Waiting, BridgeAdminChangeState.Done -> null
        is BridgeAdminChangeState.Refused -> reason.labelRes()
        is BridgeAdminChangeState.Stopped -> stop.labelRes()
    }

@Composable
private fun BridgeAdminChangeKind.description(): String {
    val unnamed = stringResource(R.string.members_unnamed)
    return when (this) {
        is BridgeAdminChangeKind.RenameLibrary -> {
            stringResource(R.string.admin_change_rename, name)
        }

        is BridgeAdminChangeKind.SetRole -> {
            when (role) {
                BridgeMemberRole.ADMIN -> stringResource(R.string.admin_change_make_admin, person ?: unnamed)
                BridgeMemberRole.MEMBER -> stringResource(R.string.admin_change_make_member, person ?: unnamed)
            }
        }

        is BridgeAdminChangeKind.RemovePerson -> {
            stringResource(R.string.admin_change_remove, person ?: unnamed)
        }

        is BridgeAdminChangeKind.RemoveDevice -> {
            // A device this phone hasn't read yet has no name to show.
            name?.let { stringResource(R.string.admin_change_remove, it) }
                ?: stringResource(R.string.admin_change_remove_unread_device)
        }

        is BridgeAdminChangeKind.Approve -> {
            stringResource(R.string.admin_change_approve, person ?: unnamed)
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun AdminChangesListPreview() {
    PreviewTheme {
        AdminChangesList(
            changes =
                listOf(
                    BridgeAdminChange(
                        operation = 1,
                        change = BridgeAdminChangeKind.RenameLibrary(name = "Home"),
                        state = BridgeAdminChangeState.Waiting,
                    ),
                    BridgeAdminChange(
                        operation = 2,
                        change = BridgeAdminChangeKind.RemovePerson(member = "member", person = "Ben"),
                        state = BridgeAdminChangeState.Refused(BridgeOperationRejection.NO_ADMIN_LEFT),
                    ),
                    BridgeAdminChange(
                        operation = 3,
                        change = BridgeAdminChangeKind.RemoveDevice(device = 7uL, name = "Tablet"),
                        state = BridgeAdminChangeState.Stopped(BridgeDeviceStop.REMOVED),
                    ),
                ),
            onDismiss = {},
        )
    }
}
