package fm.bae.app.ui.sync

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
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
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ActivityLine
import fm.bae.app.data.SyncActivityStore
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
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeQueueWait
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
    onSetDownloadsPaused: (Boolean) -> Unit,
) {
    val activity by store.activity.collectAsState()
    val error by store.error.collectAsState()
    val problems = activityProblems(app, activity, error, dismissed)
    val words = activityWords(LocalContext.current, activity)
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
            words = words,
            error = error,
            bar = store::bar,
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
            onSetDownloadsPaused = onSetDownloadsPaused,
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
    words: ActivityWords,
    error: DisplayError?,
    bar: (ActivityLine) -> LatestSource<Double>,
    onDismiss: () -> Unit,
    onDismissProblem: (ActivityProblemId) -> Unit,
    onOpenDetails: (ErrorDetails) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
    onSetDownloadsPaused: (Boolean) -> Unit,
) {
    ContentSheet(onDismiss = onDismiss) {
        ActivityList(
            problems = problems,
            app = app,
            activity = activity,
            words = words,
            error = error,
            bar = bar,
            onDismissProblem = onDismissProblem,
            onOpenDetails = onOpenDetails,
            onOpenSettings = onOpenSettings,
            onOpenDownloads = onOpenDownloads,
            onSetDownloadsPaused = onSetDownloadsPaused,
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
    words: ActivityWords,
    error: DisplayError?,
    bar: (ActivityLine) -> LatestSource<Double>,
    onDismissProblem: (ActivityProblemId) -> Unit,
    onOpenDetails: (ErrorDetails) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
    onSetDownloadsPaused: (Boolean) -> Unit,
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
                NeedsAttention(
                    problems = problems,
                    app = app,
                    activity = activity,
                    failure = words.failure,
                    error = error,
                    onDismissProblem = onDismissProblem,
                    onOpenDetails = onOpenDetails,
                    onOpenSettings = onOpenSettings,
                )
            }
        }
        activity?.let { transferRows(it, words, bar, onOpenDownloads, onSetDownloadsPaused) }
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
    failure: DisplayError?,
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
                    failure = failure,
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
    failure: DisplayError?,
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
        failure?.let {
            val open = ErrorAction(context.coreString("core.activity.open_settings"), onClick = onOpenSettings)
            ErrorNotice(
                error = it,
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
    activity: BridgeSyncActivity,
    words: ActivityWords,
    bar: (ActivityLine) -> LatestSource<Double>,
    onOpenDownloads: () -> Unit,
    onSetDownloadsPaused: (Boolean) -> Unit,
) {
    if (!(activity.syncing || activity.uploads != null || activity.pins != null)) return
    item { Eyebrow(LocalContext.current.coreString("core.activity.transfers")) }
    if (activity.syncing) {
        item {
            TransferLine(title = LocalContext.current.coreString("core.activity.syncing"), count = null, share = null)
        }
    }
    activity.uploads?.let { uploads ->
        item {
            val context = LocalContext.current
            TransferLine(
                title = context.coreString("core.activity.uploading"),
                count = words.uploadsWait ?: context.releasesDoneText(uploads.batch),
                share = bar(ActivityLine.UPLOADS),
            )
        }
    }
    activity.pins?.let { pins ->
        item {
            val context = LocalContext.current
            val paused = pins.wait == BridgeQueueWait.PAUSED
            TransferLine(
                title = context.coreString("core.activity.pinning"),
                count = words.pinsWait ?: context.releasesDoneText(pins.batch),
                share = bar(ActivityLine.PINS),
                modifier = Modifier.clickable(onClick = onOpenDownloads),
                end = { PausePinning(paused = paused, onClick = { onSetDownloadsPaused(!paused) }) },
            )
        }
    }
}

/**
 * A transfer's line: its words, and at the end how many of its releases are done, or what holds its
 * queue back, then [end], and under them its bar, which is [share] of the batch's bytes, or moving
 * when there is none.
 */
@Composable
private fun TransferLine(
    title: String,
    count: String?,
    share: LatestSource<Double>?,
    modifier: Modifier = Modifier,
    end: (@Composable () -> Unit)? = null,
) {
    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = title, modifier = Modifier.weight(1f))
            count?.let { Text(text = it, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            end?.invoke()
        }
        ShareBar(share)
    }
}

/**
 * The Pinning line's tonal Pause, or Resume while pinning is paused: a [PAUSE_CONTAINER] disc whose
 * touch area is the usual 48dp.
 */
@Composable
private fun PausePinning(
    paused: Boolean,
    onClick: () -> Unit,
) {
    val colors = IconButtonDefaults.filledTonalIconButtonColors()
    IconButton(onClick = onClick, modifier = Modifier.size(PAUSE_CONTAINER).wrapContentSize(unbounded = true)) {
        Box(
            modifier = Modifier.size(PAUSE_CONTAINER).background(colors.containerColor, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                imageVector = if (paused) Icons.Filled.PlayArrow else Icons.Filled.Pause,
                contentDescription = stringResource(if (paused) R.string.resume else R.string.pause),
                tint = colors.contentColor,
                modifier = Modifier.size(PAUSE_GLYPH),
            )
        }
    }
}

private val PAUSE_CONTAINER = 32.dp
private val PAUSE_GLYPH = 18.dp

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
            words = ActivityWords(failure = null, uploadsWait = null, pinsWait = null),
            error = null,
            bar = SyncActivityStore()::bar,
            onDismissProblem = {},
            onOpenDetails = {},
            onOpenSettings = {},
            onOpenDownloads = {},
            onSetDownloadsPaused = {},
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
            onSetDownloadsPaused = {},
        )
    }
}
