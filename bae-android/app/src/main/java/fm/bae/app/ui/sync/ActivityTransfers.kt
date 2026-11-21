package fm.bae.app.ui.sync

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.releasesDoneText
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidEyebrow
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.IconTone
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeActivityLine
import uniffi.bae_bridge.BridgeSyncActivity

internal fun LazyListScope.transferRows(
    activity: BridgeSyncActivity,
    words: ActivityWords,
    bar: (BridgeActivityLine) -> LatestSource<Double>,
    onOpenDownloads: () -> Unit,
    onSetDownloadsPaused: (Boolean) -> Unit,
) {
    if (!(activity.syncing || activity.uploads != null || activity.pins != null)) return
    item { AndroidEyebrow(LocalContext.current.coreString("core.activity.transfers")) }
    if (activity.syncing) {
        item {
            AndroidTransferLine(
                title = LocalContext.current.coreString("core.activity.syncing"),
                batch = null,
            )
        }
    }
    activity.uploads?.let { uploads ->
        item {
            val context = LocalContext.current
            AndroidTransferLine(
                title = context.coreString("core.activity.uploading"),
                batch =
                    TransferBatch(
                        count = words.uploadsWait ?: context.releasesDoneText(uploads.batch),
                        share = bar(BridgeActivityLine.UPLOADS),
                    ),
            )
        }
    }
    activity.pins?.let { pins ->
        item {
            val context = LocalContext.current
            val paused = pins.paused
            AndroidTransferLine(
                title = context.coreString("core.activity.pinning"),
                batch =
                    TransferBatch(
                        count = words.pinsWait ?: context.releasesDoneText(pins.batch),
                        share = bar(BridgeActivityLine.PINS),
                    ),
                modifier = Modifier.clickable(onClick = onOpenDownloads),
                end = { PausePinning(paused = paused, onClick = { onSetDownloadsPaused(!paused) }) },
            )
        }
    }
}

/**
 * A transfer that moves a batch of releases: how many of them are done, or what holds its queue back, and
 * its [share] of the batch's bytes.
 */
internal class TransferBatch(
    val count: String,
    val share: LatestSource<Double>,
)

/**
 * A transfer's line: its words, and at the end its [batch]'s count, then [end], and under them a bar of
 * the batch's share, or a moving bar for a transfer with no batch, such as syncing changes.
 */
@Composable
internal fun AndroidTransferLine(
    title: String,
    batch: TransferBatch?,
    modifier: Modifier = Modifier,
    end: (@Composable () -> Unit)? = null,
) {
    Column(
        modifier = modifier.fillMaxWidth().padding(vertical = ThemeSpace.related),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = title, style = ThemeText.body.style, modifier = Modifier.weight(1f))
            batch?.let {
                Text(text = it.count, style = ThemeText.body.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            end?.invoke()
        }
        ShareBar(batch?.share, description = title)
    }
}

/**
 * The Pinning line's tonal Pause, or Resume while pinning is paused: a 32dp disc whose
 * touch area is the usual 48dp.
 */
@Composable
internal fun PausePinning(
    paused: Boolean,
    onClick: () -> Unit,
) {
    AndroidIconButton(
        icon = if (paused) SymbolIcons.PlayArrow else SymbolIcons.Pause,
        contentDescription = stringResource(if (paused) R.string.resume else R.string.pause),
        onClick = onClick,
        tone = IconTone.ACCENT,
        tonal = true,
    )
}
