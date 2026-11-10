package fm.bae.app.ui.sync

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ActivityLine
import fm.bae.app.releasesDoneText
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.Eyebrow
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.BridgeSyncActivity

internal fun LazyListScope.transferRows(
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
