package fm.bae.app.ui.sync

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.coreString
import fm.bae.app.countLabelsText
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.data.syncActivityVisible
import fm.bae.app.displayError
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.color
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import uniffi.bae_bridge.BridgeAbandonedAttachment
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePendingRecords
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeUploadBar
import uniffi.bae_bridge.BridgeUploadTransfer

/**
 * Sync's place in the library's top bar: a small spinner while bytes move, a
 * warning symbol while something needs the person, nothing otherwise. Tapping
 * it opens what sync is doing in a sheet, whose upload line shows the bytes
 * [uploadTransfer] says the uploads have moved.
 */
@Composable
internal fun SyncActivityButton(
    store: SyncActivityStore,
    uploadTransfer: StateFlow<BridgeUploadTransfer?>,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
) {
    val activity by store.activity.collectAsState()
    val error by store.error.collectAsState()
    if (!syncActivityVisible(activity, error)) return
    var listShown by rememberSaveable { mutableStateOf(false) }
    val label = LocalContext.current.coreString("core.activity.title")
    IconButton(
        onClick = { listShown = true },
        modifier = Modifier.semantics { contentDescription = label },
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (activity?.moving == true) {
                CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
            }
            if (activity?.needsAttention == true || error != null) {
                Icon(
                    imageVector = Icons.Filled.Warning,
                    contentDescription = null,
                    tint = StatusTone.WARNING.color,
                    modifier = Modifier.size(16.dp),
                )
            }
        }
    }
    if (listShown) {
        SyncActivitySheet(
            store = store,
            uploadTransfer = uploadTransfer,
            onDismiss = { listShown = false },
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
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SyncActivitySheet(
    store: SyncActivityStore,
    uploadTransfer: StateFlow<BridgeUploadTransfer?>,
    onDismiss: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
) {
    val activity by store.activity.collectAsState()
    val error by store.error.collectAsState()
    ModalBottomSheet(onDismissRequest = onDismiss) {
        SyncActivityList(
            activity = activity,
            uploadTransfer = uploadTransfer,
            error = error,
            onOpenSettings = onOpenSettings,
            onOpenDownloads = onOpenDownloads,
            onRetryNow = store::retryNow,
            onDismissAbandoned = store::dismissAbandoned,
        )
    }
}

/**
 * What sync is doing: what needs the person first, then what is moving, and
 * Retry Now. The uploads' line shows the bytes [uploadTransfer] says they have
 * moved, which change apart from [activity].
 */
@Composable
internal fun SyncActivityList(
    activity: BridgeSyncActivity?,
    uploadTransfer: StateFlow<BridgeUploadTransfer?>,
    error: DisplayError?,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
    onRetryNow: () -> Unit,
    onDismissAbandoned: (BridgeAbandonedAttachment) -> Unit,
) {
    val context = LocalContext.current
    LazyColumn(
        modifier = Modifier.fillMaxWidth(),
        contentPadding = PaddingValues(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        item {
            Text(text = context.coreString("core.activity.title"), style = ThemeText.heading.style)
        }
        if (activity != null) {
            attentionSection(context, activity, onOpenSettings, onDismissAbandoned)
            transferRows(context, activity, uploadTransfer, onOpenDownloads)
            if (!activity.moving && !activity.needsAttention && !activity.syncing) {
                item { SecondaryLine(context.coreString("core.activity.idle")) }
            }
        }
        error?.let { shown -> item { ErrorDetailDisclosure(shown) } }
        item {
            OutlinedButton(onClick = onRetryNow) {
                Text(context.coreString("core.activity.retry_now"))
            }
        }
    }
}

private fun LazyListScope.attentionSection(
    context: Context,
    activity: BridgeSyncActivity,
    onOpenSettings: () -> Unit,
    onDismissAbandoned: (BridgeAbandonedAttachment) -> Unit,
) {
    if (!activity.needsAttention) return
    item { SectionHeading(context.coreString("core.activity.attention")) }
    activity.failure?.let { failure ->
        item { FailureDisclosure(context, failure) }
    }
    when (val attention = activity.attention) {
        BridgePendingRecords.Loading -> {
            Unit
        }

        is BridgePendingRecords.Failed -> {
            item { FailureDisclosure(context, attention.error) }
        }

        is BridgePendingRecords.Ready -> {
            items(attention.records) { record ->
                Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
                    Text(text = context.coreString(record.subject), style = ThemeText.strong.style)
                    ErrorDetailDisclosure(
                        error =
                            DisplayError(
                                context.coreString(record.reason.key),
                                record.reason.detail,
                            ),
                        lineColor = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
    abandonedRows(context, activity.abandoned, onDismissAbandoned)
    item {
        TextButton(onClick = onOpenSettings) {
            Text(context.coreString("core.activity.open_settings"))
        }
    }
    item { HorizontalDivider() }
}

private fun LazyListScope.transferRows(
    context: Context,
    activity: BridgeSyncActivity,
    uploadTransfer: StateFlow<BridgeUploadTransfer?>,
    onOpenDownloads: () -> Unit,
) {
    if (activity.syncing) {
        item { Text(context.coreString("core.activity.syncing")) }
    }
    activity.uploads?.let { uploads ->
        item {
            val transfer by uploadTransfer.collectAsState()
            TransferLine(context.countLabelsText(uploads.summaryParts), transfer?.bar)
        }
    }
    activity.downloads?.let { downloads ->
        item {
            Text(
                text = context.countLabelsText(downloads.summaryParts),
                modifier = Modifier.fillMaxWidth().clickable(onClick = onOpenDownloads),
            )
        }
    }
}

/** A transfer's line, and the bytes it has moved when [bar] counts any. */
@Composable
private fun TransferLine(
    title: String,
    bar: BridgeUploadBar?,
) {
    val context = LocalContext.current
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Text(title)
        if (bar != null && bar.bytesTotal > 0uL) {
            SecondaryLine(
                context.coreString(
                    "core.download.bytes_progress",
                    mapOf(
                        "done" to context.formatFileSize(bar.bytesDone.requireDisplayableByteCount()),
                        "total" to context.formatFileSize(bar.bytesTotal.requireDisplayableByteCount()),
                    ),
                ),
            )
        }
    }
}

/** A failure sync reported, in the warning tone, with its diagnostic behind it. */
@Composable
internal fun FailureDisclosure(
    context: Context,
    failure: BridgeException,
) {
    ErrorDetailDisclosure(
        error = context.displayError(failure) ?: DisplayError(failure.toString()),
        lineColor = StatusTone.WARNING.color,
    )
}

@Composable
private fun SectionHeading(text: String) {
    Spacer(modifier = Modifier.padding(top = ThemeSpace.inline))
    Text(text = text, style = ThemeText.strong.style)
}

@Composable
internal fun SecondaryLine(text: String) {
    Text(text = text, style = ThemeText.detail.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Preview(showBackground = true)
@Composable
private fun SyncActivityListPreview() {
    BaeTheme {
        SyncActivityList(
            activity = PreviewData.syncActivity(needsAttention = true),
            uploadTransfer = MutableStateFlow(PreviewData.uploadTransfer()),
            error = null,
            onOpenSettings = {},
            onOpenDownloads = {},
            onRetryNow = {},
            onDismissAbandoned = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun SyncActivityButtonPreview() {
    val store = remember { SyncActivityStore({}, {}).apply { apply(PreviewData.syncActivity(needsAttention = true)) } }
    BaeTheme {
        SyncActivityButton(
            store = store,
            uploadTransfer = MutableStateFlow(PreviewData.uploadTransfer()),
            onOpenSettings = {},
            onOpenDownloads = {},
        )
    }
}
