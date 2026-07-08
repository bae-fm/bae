package fm.bae.app.ui

import fm.bae.app.BridgeFixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeReleaseStorageAction

/**
 * The album-detail download control's state is a join of the release (`pinned` /
 * `storageActions`) and this release's download-queue entry. These cover the
 * join's precedence — a live queue entry outranks `pinned`, `pinned` outranks an
 * available `.pin` action, and no cloud home yields no control at all — plus the
 * snapshot lookup that feeds it.
 */
class ReleaseDownloadStatusTest {
    @Test
    fun pinnedWithNoQueueEntryIsDownloaded() {
        assertEquals(
            ReleaseDownloadStatus.Downloaded,
            releaseDownloadStatus(
                pinned = true,
                storageActions = listOf(BridgeReleaseStorageAction.UNPIN, BridgeReleaseStorageAction.MAKE_LOCAL),
                queueState = null,
            ),
        )
    }

    @Test
    fun queuedEntryWinsEvenWhenPinActionAvailable() {
        assertEquals(
            ReleaseDownloadStatus.Queued,
            releaseDownloadStatus(
                pinned = false,
                storageActions = listOf(BridgeReleaseStorageAction.PIN),
                queueState = BridgeDownloadState.Queued,
            ),
        )
    }

    @Test
    fun activeEntryCarriesItsExactProgress() {
        val progress = BridgeDownloadTransferProgress(bytesDone = 1_024u, bytesTotal = 4_096u, fraction = 0.25)
        assertEquals(
            ReleaseDownloadStatus.Downloading(progress),
            releaseDownloadStatus(
                pinned = false,
                storageActions = emptyList(),
                queueState = BridgeDownloadState.Active(progress),
            ),
        )
    }

    @Test
    fun failedEntryCarriesItsErrorMessage() {
        assertEquals(
            ReleaseDownloadStatus.Failed("read failed"),
            releaseDownloadStatus(
                pinned = false,
                storageActions = listOf(BridgeReleaseStorageAction.PIN),
                queueState = BridgeDownloadState.Failed("read failed"),
            ),
        )
    }

    @Test
    fun queueEntryOutranksPinnedInThePostSuccessRaceWindow() {
        val progress = BridgeDownloadTransferProgress(bytesDone = 4_096u, bytesTotal = 4_096u, fraction = 1.0)
        assertEquals(
            ReleaseDownloadStatus.Downloading(progress),
            releaseDownloadStatus(
                pinned = true,
                storageActions = emptyList(),
                queueState = BridgeDownloadState.Active(progress),
            ),
        )
    }

    @Test
    fun notPinnedWithPinActionIsAvailableAndDesktopActionsIgnored() {
        assertEquals(
            ReleaseDownloadStatus.Available,
            releaseDownloadStatus(
                pinned = false,
                storageActions = listOf(BridgeReleaseStorageAction.PIN, BridgeReleaseStorageAction.MAKE_LOCAL),
                queueState = null,
            ),
        )
    }

    @Test
    fun noCloudHomeYieldsNoControl() {
        assertNull(
            releaseDownloadStatus(
                pinned = false,
                storageActions = emptyList(),
                queueState = null,
            ),
        )
    }

    @Test
    fun unpinActionWithoutPinnedYieldsNoControl() {
        // State is driven by `pinned`, not the action list: without `pinned`
        // there is nothing downloaded to remove and no pin offered.
        assertNull(
            releaseDownloadStatus(
                pinned = false,
                storageActions = listOf(BridgeReleaseStorageAction.UNPIN, BridgeReleaseStorageAction.MAKE_LOCAL),
                queueState = null,
            ),
        )
    }

    @Test
    fun snapshotLookupFindsTheMatchingReleaseState() {
        val snapshot =
            BridgeFixtures.downloadSnapshot(
                downloads = listOf(BridgeFixtures.downloadOp("rel-1", BridgeDownloadState.Queued)),
            )
        assertEquals(BridgeDownloadState.Queued, snapshot.stateForRelease("rel-1"))
    }

    @Test
    fun snapshotLookupForAbsentReleaseIsNull() {
        val snapshot =
            BridgeFixtures.downloadSnapshot(
                downloads = listOf(BridgeFixtures.downloadOp("rel-1", BridgeDownloadState.Queued)),
            )
        assertNull(snapshot.stateForRelease("rel-2"))
    }

    @Test
    fun snapshotLookupPicksTheRightEntryAmongSeveral() {
        val progress = BridgeDownloadTransferProgress(bytesDone = 2u, bytesTotal = 8u, fraction = 0.25)
        val snapshot =
            BridgeFixtures.downloadSnapshot(
                downloads =
                    listOf(
                        BridgeFixtures.downloadOp("rel-1", BridgeDownloadState.Queued),
                        BridgeFixtures.downloadOp("rel-2", BridgeDownloadState.Active(progress)),
                        BridgeFixtures.downloadOp("rel-3", BridgeDownloadState.Failed("read failed")),
                    ),
            )
        assertEquals(BridgeDownloadState.Active(progress), snapshot.stateForRelease("rel-2"))
        assertEquals(BridgeDownloadState.Failed("read failed"), snapshot.stateForRelease("rel-3"))
    }
}
