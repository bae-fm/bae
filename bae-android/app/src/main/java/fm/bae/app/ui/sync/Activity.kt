package fm.bae.app.ui.sync

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
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
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ActivityLine
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.ActivityProblemId
import fm.bae.app.ui.AppProblem
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidEyebrow
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.ErrorDetails
import fm.bae.app.ui.components.ErrorDetailsDialog
import fm.bae.app.ui.components.NoticeButtons
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.NoticeTextInset
import fm.bae.app.ui.components.StatusGlyph
import fm.bae.app.ui.components.StatusTone
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeSyncActivity

/**
 * The Activity sheet's place in the library's top bar: a small spinner while bytes move, a pause
 * symbol while a queue is held and nothing moves, a warning symbol while a problem the person hasn't
 * dismissed is listed, and nothing otherwise. Tapping it opens the sheet.
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
    val moving = activity?.moving == true
    val held = activity?.held == true
    var listShown by rememberSaveable { mutableStateOf(false) }
    var details by remember { mutableStateOf<ErrorDetails?>(null) }
    if (moving || held || problems.needsAttention) {
        val label = LocalContext.current.coreString("core.activity.title")
        IconButton(
            onClick = { listShown = true },
            modifier = Modifier.semantics { contentDescription = label },
        ) {
            ActivityGlyph(moving = moving, held = held, needsAttention = problems.needsAttention)
        }
    }
    if (listShown) {
        ActivitySheet(
            problems = problems,
            app = app,
            activity = activity,
            words = activityWords(LocalContext.current, activity),
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

/**
 * The spinner or the pause symbol, the warning, or both. Core never holds a queue while bytes move, so
 * the spinner and the pause never show together.
 */
@Composable
private fun ActivityGlyph(
    moving: Boolean,
    held: Boolean,
    needsAttention: Boolean,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (moving) {
            AndroidSpinner(size = ThemeIcon.small)
        }
        if (held) {
            Icon(imageVector = Icons.Filled.Pause, contentDescription = null, modifier = Modifier.size(ThemeIcon.large))
        }
        if (needsAttention) {
            StatusGlyph(StatusTone.WARNING, size = ThemeIcon.large)
        }
    }
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
    AndroidSheet(onDismiss = onDismiss) {
        AndroidActivity(
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
internal fun AndroidActivity(
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
        AndroidEyebrow(context.coreString("core.activity.attention"))
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
        AndroidNotice(
            title = appProblem.title,
            advice = appProblem.advice,
            error = appProblem.error,
            look = NoticeLook.QUIET,
            onDismiss = onDismiss,
            onOpenDetails = onOpenDetails,
        )
    } else if (id == ActivityProblemId.SYNC_STATUS_UNREAD) {
        error?.let { unread ->
            AndroidNotice(
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
            AndroidNotice(
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
                AndroidNotice(
                    error = DisplayError(context.coreString(record.reason.key), record.reason.detail),
                    tone = StatusTone.WARNING,
                )
            }
        }
        AndroidButton(kind = ButtonKind.TEXT, onClick = onOpenSettings) {
            Text(context.coreString("core.activity.open_settings"))
        }
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
        AndroidActivity(
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
    val store =
        remember {
            SyncActivityStore().apply { apply(PreviewData.syncActivity(needsAttention = true), BugReports()) }
        }
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
