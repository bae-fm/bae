package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.downloads.AndroidDownloadsBar
import fm.bae.app.ui.downloads.DownloadConcurrencySelector
import fm.bae.app.ui.downloads.DownloadsEmpty
import fm.bae.app.ui.downloads.DownloadsList
import org.junit.Test
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadState

/** The shots of the android.storage.pinning.* boards: the Pinning screen with a queue, with a failure, and empty. */
class PinningBoardShots : BoardShot() {
    @Test
    fun androidStoragePinning() =
        shot("android.storage.pinning") {
            ScreenScene {
                Pinning(
                    failed = false,
                    summary = "1 pinning · 2 queued to pin",
                    ops =
                        listOf(
                            op("rel-1", "Late Arrivals", 10UL, 344_000_000UL, BridgeDownloadState.Active),
                            op("rel-2", "Winter Kitchen", 10UL, 331_000_000UL, BridgeDownloadState.Queued),
                            op("rel-3", "Glass Harbor", 9UL, 356_000_000UL, BridgeDownloadState.Queued),
                        ),
                )
            }
        }

    /** A failed pin beside a queued one whose release the library no longer has, so it has no title. */
    @Test
    fun androidStoragePinningFailed() =
        shot("android.storage.pinning.failed") {
            ScreenScene {
                Pinning(
                    failed = true,
                    summary = "1 failed · 1 queued to pin",
                    ops =
                        listOf(
                            op("rel-1", "Late Arrivals", 10UL, 344_000_000UL, BridgeDownloadState.Failed("Network unreachable")),
                            op("rel-2", null, 4UL, 120_000_000UL, BridgeDownloadState.Queued),
                        ),
                )
            }
        }

    @Test
    fun androidStoragePinningNone() =
        shot("android.storage.pinning.none") {
            ScreenScene { Pinning(failed = false, summary = "", ops = emptyList()) }
        }
}

private fun op(
    releaseId: String,
    title: String?,
    files: ULong,
    bytes: ULong,
    state: BridgeDownloadState,
) = PreviewData.downloadOp(releaseId = releaseId, state = state, title = title, fileCount = files, totalSize = bytes)

@Composable
private fun Pinning(
    failed: Boolean,
    summary: String,
    ops: List<BridgeDownloadOp>,
) {
    val downloads =
        PreviewData.downloadStore(
            downloadingReleaseId = "rel-1",
            transfer = PreviewData.downloadTransferProgress(bytesDone = 120_000_000UL, bytesTotal = 344_000_000UL, fraction = 0.35),
        )
    Column(modifier = Modifier.fillMaxSize()) {
        AndroidDownloadsBar(
            paused = false,
            hasDownloads = ops.isNotEmpty(),
            hasFailures = failed,
            onBack = {},
            onPauseToggle = {},
            onRetry = {},
        )
        DownloadConcurrencySelector(value = 3u, limit = 6u, onSelect = {})
        HorizontalDivider()
        Box(modifier = Modifier.weight(1f).fillMaxSize()) {
            if (ops.isEmpty()) {
                DownloadsEmpty(modifier = Modifier.align(Alignment.Center))
            } else {
                DownloadsList(
                    summary = summary,
                    count = ops.size,
                    row = { ops[it] },
                    downloads = downloads,
                    listState = rememberLazyListState(),
                    onCancel = {},
                    onRetry = {},
                )
            }
        }
    }
}
