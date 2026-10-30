package fm.bae.app.ui.sync

import android.content.Context
import android.util.TypedValue
import android.widget.TextView
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
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
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
import fm.bae.fastvalues.FrameFollowing
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePendingRecords
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeUploadBar
import uniffi.bae_bridge.BridgeUploadTransfer

/**
 * Sync's place in the library's top bar: a small spinner while bytes move, a
 * warning symbol while something needs the person, nothing otherwise. Tapping
 * it opens what sync is doing in a sheet, whose upload line shows the bytes
 * the uploads have moved, read from [uploadTransfer] each display frame.
 */
@Composable
internal fun SyncActivityButton(
    store: SyncActivityStore,
    uploadTransfer: LatestSource<BridgeUploadTransfer>,
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
    uploadTransfer: LatestSource<BridgeUploadTransfer>,
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
        )
    }
}

/**
 * What sync is doing: what needs the person first, then what is moving, and
 * Retry Now while core offers it, when a retry can clear a failure. The
 * uploads' line shows the bytes [uploadTransfer] says they have moved, which
 * change apart from [activity].
 */
@Composable
internal fun SyncActivityList(
    activity: BridgeSyncActivity?,
    uploadTransfer: LatestSource<BridgeUploadTransfer>,
    error: DisplayError?,
    onOpenSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
    onRetryNow: () -> Unit,
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
            attentionSection(context, activity, onOpenSettings)
            transferRows(context, activity, uploadTransfer, onOpenDownloads)
            if (!activity.moving && !activity.needsAttention && !activity.syncing) {
                item { SecondaryLine(context.coreString("core.activity.idle")) }
            }
        }
        error?.let { shown -> item { ErrorDetailDisclosure(shown) } }
        if (activity?.retryOffered == true) {
            item {
                OutlinedButton(onClick = onRetryNow) {
                    Text(context.coreString("core.activity.retry_now"))
                }
            }
        }
    }
}

private fun LazyListScope.attentionSection(
    context: Context,
    activity: BridgeSyncActivity,
    onOpenSettings: () -> Unit,
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
    uploadTransfer: LatestSource<BridgeUploadTransfer>,
    onOpenDownloads: () -> Unit,
) {
    if (activity.syncing) {
        item { Text(context.coreString("core.activity.syncing")) }
    }
    activity.uploads?.let { uploads ->
        item { TransferLine(context.countLabelsText(uploads.summaryParts), uploadTransfer) }
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

/**
 * A transfer's line, and the bytes the files being sent have moved together,
 * which a View reads from [transfer]'s latest value each display frame while
 * files are being sent: moving bytes recompose nothing.
 */
@Composable
private fun TransferLine(
    title: String,
    transfer: LatestSource<BridgeUploadTransfer>,
) {
    val color = MaterialTheme.colorScheme.onSurfaceVariant.toArgb()
    val size = ThemeText.detail.style.fontSize.value
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Text(title)
        AndroidView(
            factory = { context -> TransferBytesView(context) },
            update = { view ->
                view.setTextColor(color)
                view.setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
                view.follow(transfer)
            },
            onRelease = { view -> view.stopFollowing() },
        )
    }
}

/** The bytes the files being sent have moved together, as "3 MB of 221.2 MB", gone while none counts any. */
internal class TransferBytesView(
    context: Context,
) : TextView(context) {
    private val following = FrameFollowing<BridgeUploadTransfer>(this) { show(it?.bar) }

    fun follow(transfer: LatestSource<BridgeUploadTransfer>) = following.follow(transfer)

    fun stopFollowing() = following.stop()

    private fun show(bar: BridgeUploadBar?) {
        val shown = bar?.takeIf { it.bytesTotal > 0uL }
        visibility = if (shown == null) GONE else VISIBLE
        val text =
            shown
                ?.let {
                    context.coreString(
                        "core.download.bytes_progress",
                        mapOf(
                            "done" to context.formatFileSize(it.bytesDone.requireDisplayableByteCount()),
                            "total" to context.formatFileSize(it.bytesTotal.requireDisplayableByteCount()),
                        ),
                    )
                }.orEmpty()
        if (this.text.toString() != text) this.text = text
    }
}

/** A failure sync reported, in the warning tone, with its diagnostic behind it; nothing for a cancellation. */
@Composable
internal fun FailureDisclosure(
    context: Context,
    failure: BridgeException,
) {
    val shown = context.displayError(failure) ?: return
    ErrorDetailDisclosure(error = shown, lineColor = StatusTone.WARNING.color)
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
            uploadTransfer = PreviewData.uploadTransferSource(),
            error = null,
            onOpenSettings = {},
            onOpenDownloads = {},
            onRetryNow = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun SyncActivityButtonPreview() {
    val store = remember { SyncActivityStore({}).apply { apply(PreviewData.syncActivity(needsAttention = true)) } }
    BaeTheme {
        SyncActivityButton(
            store = store,
            uploadTransfer = PreviewData.uploadTransferSource(),
            onOpenSettings = {},
            onOpenDownloads = {},
        )
    }
}
