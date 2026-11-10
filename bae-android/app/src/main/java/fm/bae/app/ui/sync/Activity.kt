package fm.bae.app.ui.sync

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.displayError
import fm.bae.app.queueWaitText
import fm.bae.app.releasesDoneText
import fm.bae.app.ui.ActivityProblemId
import fm.bae.app.ui.AppProblem
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ContentSheet
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.ErrorDetails
import fm.bae.app.ui.components.ErrorDetailsDialog
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.NoticeButtons
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.NoticeTextInset
import fm.bae.app.ui.components.StatusGlyph
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.TitledErrorNotice
import fm.bae.app.ui.components.WaitingIndicator
import fm.bae.app.uploadQueueWaitText
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeSyncActivity

/**
 * The Activity sheet's place in the library's top bar: a small spinner while bytes move, a warning
 * symbol while a problem the person hasn't dismissed is listed, and nothing otherwise. Tapping it opens the sheet.
 *
 * The sheet stays open once the last thing that showed the button is gone, saying everything is up to
 * date, until the person closes it.
 */
@Composable
internal fun ActivityButton(
    store: SyncActivityStore,
    app: List<AppProblem>,
    dismissed: Set<ActivityProblemId>,
    onDismissProblem: (ActivityProblemId) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
) {
    val activity by store.activity.collectAsState()
    val error by store.error.collectAsState()
    val problems = activityProblems(app, activity, error, dismissed)
    val moving = activity?.moving == true
    var listShown by rememberSaveable { mutableStateOf(false) }
    var details by remember { mutableStateOf<ErrorDetails?>(null) }
    if (moving || problems.needsAttention) {
        val label = LocalContext.current.coreString("core.activity.title")
        IconButton(
            onClick = { listShown = true },
            modifier = Modifier.semantics { contentDescription = label },
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (moving) {
                    WaitingIndicator(size = ThemeIcon.small)
                }
                if (problems.needsAttention) {
                    StatusGlyph(StatusTone.WARNING, size = ThemeIcon.large)
                }
            }
        }
    }
    if (listShown) {
        ActivitySheet(
            problems = problems,
            app = app,
            activity = activity,
            error = error,
            onDismiss = { listShown = false },
            onDismissProblem = onDismissProblem,
            onOpenDetails = {
                listShown = false
                details = it
            },
            onOpenSettings = {
                listShown = false
                onOpenSettings()
            },
            onOpenDownloads = {
                listShown = false
                onOpenDownloads()
            },
        )
    }
    // Outside the sheet: a dialog opened from inside it would close with it.
    details?.let { shown -> ErrorDetailsDialog(shown, onDismiss = { details = null }) }
}

@Composable
private fun ActivitySheet(
    problems: ActivityProblems,
    app: List<AppProblem>,
    activity: BridgeSyncActivity?,
    error: DisplayError?,
    onDismiss: () -> Unit,
    onDismissProblem: (ActivityProblemId) -> Unit,
    onOpenDetails: (ErrorDetails) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
) {
    ContentSheet(onDismiss = onDismiss) {
        ActivityList(
            problems = problems,
            app = app,
            activity = activity,
            error = error,
            onDismissProblem = onDismissProblem,
            onOpenDetails = onOpenDetails,
            onOpenSettings = onOpenSettings,
            onOpenDownloads = onOpenDownloads,
        )
    }
}

/**
 * What needs the person first, each problem a quiet row with an × that takes it out of the sheet and
 * a tap that opens its explanation, then what is moving, or that everything is up to date.
 */
@Composable
internal fun ActivityList(
    problems: ActivityProblems,
    app: List<AppProblem>,
    activity: BridgeSyncActivity?,
    error: DisplayError?,
    onDismissProblem: (ActivityProblemId) -> Unit,
    onOpenDetails: (ErrorDetails) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
) {
    val context = LocalContext.current
    LazyColumn(
        modifier = Modifier.fillMaxWidth(),
        contentPadding = PaddingValues(horizontal = ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        item {
            Text(text = context.coreString("core.activity.title"), style = ThemeText.heading.style)
        }
        if (problems.needsAttention) {
            item {
                NeedsAttention(problems, app, activity, error, onDismissProblem, onOpenDetails, onOpenSettings)
            }
        }
        activity?.let { transferRows(context, it, onOpenDownloads) }
        val transfers = activity != null && (activity.syncing || activity.uploads != null || activity.pins != null)
        if (!problems.needsAttention && !transfers) {
            item { SecondaryLine(context.coreString("core.activity.idle")) }
        }
    }
}

/**
 * The label, then a row for each problem not dismissed with a line between rows, the lines inset
 * to start where the words do, and the pending records after them.
 */
@Composable
private fun NeedsAttention(
    problems: ActivityProblems,
    app: List<AppProblem>,
    activity: BridgeSyncActivity?,
    error: DisplayError?,
    onDismissProblem: (ActivityProblemId) -> Unit,
    onOpenDetails: (ErrorDetails) -> Unit,
    onOpenSettings: () -> Unit,
) {
    val context = LocalContext.current
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Eyebrow(context.coreString("core.activity.attention"))
        Column {
            problems.shown.forEachIndexed { index, id ->
                if (index > 0) HorizontalDivider(modifier = Modifier.padding(start = NoticeTextInset))
                ProblemRow(
                    id = id,
                    app = app,
                    activity = activity,
                    error = error,
                    onDismiss = { onDismissProblem(id) },
                    onOpenDetails = onOpenDetails,
                    onOpenSettings = onOpenSettings,
                )
            }
        }
        if (activity?.attention?.isNotEmpty() == true) {
            PendingRecords(activity.attention, onOpenSettings)
        }
    }
}

/**
 * The quiet row for problem [id], with an × that calls [onDismiss] and a tap that hands its
 * explanation to [onOpenDetails].
 */
@Composable
private fun ProblemRow(
    id: ActivityProblemId,
    app: List<AppProblem>,
    activity: BridgeSyncActivity?,
    error: DisplayError?,
    onDismiss: () -> Unit,
    onOpenDetails: (ErrorDetails) -> Unit,
    onOpenSettings: () -> Unit,
) {
    val context = LocalContext.current
    val appProblem = app.firstOrNull { it.id == id }
    if (appProblem != null) {
        TitledErrorNotice(
            title = appProblem.title,
            advice = appProblem.advice,
            error = appProblem.error,
            look = NoticeLook.QUIET,
            onDismiss = onDismiss,
            onOpenDetails = onOpenDetails,
        )
    } else if (id == ActivityProblemId.SYNC_STATUS_UNREAD) {
        error?.let { unread ->
            TitledErrorNotice(
                title = stringResource(R.string.settings_sync_unreadable),
                advice = null,
                error = unread,
                look = NoticeLook.QUIET,
                onDismiss = onDismiss,
                onOpenDetails = onOpenDetails,
            )
        }
    } else {
        activity?.failure?.let(context::displayError)?.let { failure ->
            val open = ErrorAction(context.coreString("core.activity.open_settings"), onClick = onOpenSettings)
            ErrorNotice(
                error = failure,
                action = open,
                tone = StatusTone.WARNING,
                look = NoticeLook.QUIET,
                onDismiss = onDismiss,
                onOpenDetails = onOpenDetails,
                buttons = NoticeButtons.BELOW,
            )
        }
    }
}

/** The records sync can't send until the person acts, each its subject and why, then the way to act. */
@Composable
private fun PendingRecords(
    records: List<BridgePendingRecord>,
    onOpenSettings: () -> Unit,
) {
    val context = LocalContext.current
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        records.forEach { record ->
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
                Text(text = context.coreString(record.subject), style = ThemeText.strong.style)
                ErrorNotice(
                    error = DisplayError(context.coreString(record.reason.key), record.reason.detail),
                    tone = StatusTone.WARNING,
                )
            }
        }
        TextButton(onClick = onOpenSettings) {
            Text(context.coreString("core.activity.open_settings"))
        }
    }
}

private fun LazyListScope.transferRows(
    context: Context,
    activity: BridgeSyncActivity,
    onOpenDownloads: () -> Unit,
) {
    if (activity.syncing) {
        item { Text(context.coreString("core.activity.syncing")) }
    }
    activity.uploads?.let { uploads ->
        item {
            TransferLine(
                title = context.coreString("core.activity.uploading"),
                count = uploads.wait?.let(context::uploadQueueWaitText) ?: context.releasesDoneText(uploads.batch),
            )
        }
    }
    activity.pins?.let { pins ->
        item {
            TransferLine(
                title = context.coreString("core.activity.pinning"),
                count = pins.wait?.let(context::queueWaitText) ?: context.releasesDoneText(pins.batch),
                modifier = Modifier.clickable(onClick = onOpenDownloads),
            )
        }
    }
}

/** A transfer's line: its words, and at the end how many of its releases are done, or what holds its queue back. */
@Composable
private fun TransferLine(
    title: String,
    count: String,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text = title, modifier = Modifier.weight(1f))
        Text(text = count, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun SecondaryLine(text: String) {
    Text(text = text, style = ThemeText.detail.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Preview(showBackground = true)
@Composable
private fun ActivityListPreview() {
    BaeTheme {
        val activity = PreviewData.syncActivity(needsAttention = true)
        ActivityList(
            problems = activityProblems(emptyList(), activity, null, emptySet()),
            app = emptyList(),
            activity = activity,
            error = null,
            onDismissProblem = {},
            onOpenDetails = {},
            onOpenSettings = {},
            onOpenDownloads = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun ActivityButtonPreview() {
    val store = remember { SyncActivityStore().apply { apply(PreviewData.syncActivity(needsAttention = true)) } }
    BaeTheme {
        ActivityButton(
            store = store,
            app = emptyList(),
            dismissed = emptySet(),
            onDismissProblem = {},
            onOpenSettings = {},
            onOpenDownloads = {},
        )
    }
}
