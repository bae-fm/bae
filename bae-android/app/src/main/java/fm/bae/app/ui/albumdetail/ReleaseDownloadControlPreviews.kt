package fm.bae.app.ui.albumdetail

import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.BridgeReleaseDownloadStatus

@Composable
private fun DownloadControlBodyStub(status: BridgeReleaseDownloadStatus) {
    DownloadControlBody(
        status = status,
        releaseId = "rel-1",
        downloads = PreviewData.downloadStore(),
        unpinning = false,
        onDownload = {},
        onCancel = {},
        onRetry = {},
        onRemove = {},
    )
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlAvailablePreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Available)
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlDownloadingPreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Downloading(wait = null))
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlQueuedPausedPreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Queued(ahead = 2u, wait = BridgeQueueWait.PAUSED))
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlQueuedOfflinePreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Queued(ahead = 0u, wait = BridgeQueueWait.OFFLINE))
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlDownloadedPreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Downloaded)
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlCopyingHerePreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.CopyingHere)
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlFailedPreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Failed("Network unreachable"))
    }
}

/** A release on another device names it; the line is core's message, built here as core builds it. */
@Preview(showBackground = true)
@Composable
private fun UnplayableOnAnotherDevicePreview() {
    BaeTheme {
        UnplayableLine(BridgeMessage("core.release.on_device", listOf(BridgeMessageArg("device", "Ana's Mac"))))
    }
}

@Preview(showBackground = true)
@Composable
private fun UnplayableOnARemovedDevicePreview() {
    BaeTheme {
        UnplayableLine(BridgeMessage("core.release.on_removed_device", listOf(BridgeMessageArg("device", "Ana's Mac"))))
    }
}
