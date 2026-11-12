package fm.bae.app.ui.settings

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ErrorDetails
import fm.bae.app.ui.components.ErrorDetailsDialog
import fm.bae.app.ui.components.SomethingWentWrongNotice
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingRecord

/**
 * What sync waits on: each pending record as a row in a card, its subject over its reason, with the record's action as
 * a text button at the right. A tap on a row with something behind its reason opens that in a dialog; the note under
 * the card says so.
 *
 * [recordsDropped] says a record a phone can't have was left out of [records] and reported as a bug
 * ([fm.bae.app.data.PhonePendingRecords]); "Something went wrong." shows in its place.
 */
@Composable
internal fun PendingSection(
    records: List<BridgePendingRecord>,
    onAction: (BridgePendingAction) -> Unit,
    recordsDropped: Boolean = false,
) {
    val context = LocalContext.current
    var shown by remember { mutableStateOf<ErrorDetails?>(null) }
    val details = records.map { pendingDetails(context, it) }
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        AndroidSection(text = stringResource(R.string.sync_pending))
        if (recordsDropped) SomethingWentWrongNotice()
        if (records.isNotEmpty()) {
            AndroidCard {
                records.forEachIndexed { index, record ->
                    PendingRow(
                        record = record,
                        details = details[index],
                        onAction = onAction,
                        onOpenDetails = { shown = it },
                    )
                }
            }
        }
        if (details.any { it != null }) AndroidNote(stringResource(R.string.sync_pending_tap))
    }
    shown?.let { ErrorDetailsDialog(it, onDismiss = { shown = null }) }
}

/**
 * What a record's reason has behind it: the reasons an undo that could not finish is made of, then the error coven
 * kept. Null when there is nothing behind it.
 */
private fun pendingDetails(
    context: Context,
    record: BridgePendingRecord,
): ErrorDetails? {
    val lines = record.reason.causes.map { context.coreString(it.key) } + listOfNotNull(record.reason.detail)
    if (lines.isEmpty()) return null
    return ErrorDetails.of(DisplayError(context.coreString(record.reason.key), lines.joinToString("\n")))
}

/** One record: what it is about over why it waits, and its action. */
@Composable
private fun PendingRow(
    record: BridgePendingRecord,
    details: ErrorDetails?,
    onAction: (BridgePendingAction) -> Unit,
    onOpenDetails: (ErrorDetails) -> Unit,
) {
    val context = LocalContext.current
    AndroidRow(
        title = context.coreString(record.subject),
        subtitle = context.coreString(record.reason.key),
        onClick = details?.let { { onOpenDetails(it) } },
        onClickLabel = stringResource(R.string.details),
        trailing = {
            val actionKey = record.actionKey
            when (record.action) {
                // The reason's details show what a review is of: a tap on the row shows them.
                BridgePendingAction.Wait, BridgePendingAction.Review -> {
                    Unit
                }

                else -> {
                    AndroidButton(kind = ButtonKind.TEXT, onClick = { onAction(record.action) }) {
                        Text(context.coreString(checkNotNull(actionKey) { "${record.action} has a label" }))
                    }
                }
            }
        },
    )
}
