package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.LocalBugReports
import fm.bae.app.R
import fm.bae.app.data.LibraryWithoutStorage
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.SomethingWentWrongNotice
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.settings.StorageRows
import fm.bae.app.ui.settings.statusTone
import fm.bae.app.ui.settings.syncLastSyncedText
import fm.bae.app.ui.settings.syncStatusText
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeStandingState
import uniffi.bae_bridge.BridgeStorageLocation

/**
 * A library on the welcome in a sheet titled by its name: how it stands with
 * its storage, when it last synced, and where it is stored.
 */
@Composable
internal fun LibraryDetailsSheet(
    details: LibraryDetails,
    onDismiss: () -> Unit,
) {
    AndroidSheet(title = details.library.name, onDismiss = onDismiss) {
        val storage = (details.library.state as? BridgeLibraryState.Openable)?.storage
        if (storage == null) {
            val bugs = LocalBugReports.current
            LaunchedEffect(details.library.id) {
                bugs.reportShownInPlace("show a library's details", LibraryWithoutStorage())
            }
            SomethingWentWrongNotice()
        } else {
            LibraryDetailsRows(storage, details.standing)
        }
    }
}

/**
 * The details' rows: its Status and when it last synced come from [standing], as sync's
 * state is worded on the Sync page or as unsent changes, with the storage's
 * rows after them. A failed read shows its error in place of those two rows.
 */
@Composable
internal fun LibraryDetailsRows(
    storage: BridgeStorageLocation,
    standing: StandingRead,
) {
    val context = LocalContext.current
    when (standing) {
        StandingRead.Loading -> {
            AndroidSpinner(modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group))
        }

        is StandingRead.Failed -> {
            AndroidNotice(standing.error, modifier = Modifier.padding(horizontal = ThemeSpace.edge))
        }

        is StandingRead.Loaded -> {
            val loaded = standing.standing
            when (val state = loaded.state) {
                BridgeStandingState.UnsentChanges -> {
                    AndroidRow(
                        title = stringResource(R.string.sync_status),
                        subtitle = stringResource(R.string.library_unsent_changes),
                        tone = StatusTone.WARNING,
                    )
                }

                is BridgeStandingState.Sync -> {
                    AndroidRow(
                        title = stringResource(R.string.sync_status),
                        subtitle = syncStatusText(context, state.shown),
                        tone = state.shown.tone.statusTone(),
                    )
                }
            }
            loaded.lastSynced?.let { moment ->
                AndroidRow(
                    title = stringResource(R.string.library_last_synced),
                    subtitle = syncLastSyncedText(moment).toString(),
                )
            }
        }
    }
    StorageRows(storage)
}
