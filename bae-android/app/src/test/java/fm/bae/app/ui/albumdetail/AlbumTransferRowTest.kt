package fm.bae.app.ui.albumdetail

import android.app.Application
import androidx.compose.material3.Text
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.shots.noAlbumCallbacks
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeOtherDevice
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleasePlayability

/**
 * The album page draws its transfer row when core says the release has transfer lines, and plays
 * from the release only when core says it offers playback: the page decides neither from the
 * release's playability.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AlbumTransferRowTest {
    @get:Rule
    val compose = createComposeRule()

    private val album = BridgeFixtures.album(id = "alb-1")

    private val onAnotherDevice =
        BridgeFixtures.release(id = album.primaryReleaseId, albumId = album.id).copy(
            playability = BridgeReleasePlayability.OnOtherDevice(BridgeOtherDevice.Active("Ana’s Mac")),
            offersPlayback = false,
        )

    private fun showPage(release: BridgeRelease) {
        val detail = BridgeFixtures.albumDetail(album, listOf(release))
        compose.setContent {
            TestTheme {
                AlbumDetailPage(
                    detail = detail,
                    cover = null,
                    playback = AlbumPlaybackState(currentTrackId = null, isPlaying = false),
                    callbacks = noAlbumCallbacks,
                    releaseDownloadControl = { Text("Transfer row") },
                    shownRating = { detail.rating },
                )
            }
        }
    }

    @Test
    fun aReleaseWithNoTransferLinesDrawsNoTransferRow() {
        showPage(BridgeFixtures.release(id = album.primaryReleaseId, albumId = album.id))

        compose.onNodeWithText("Transfer row").assertDoesNotExist()
    }

    @Test
    fun aReleaseWithTransferLinesDrawsTheTransferRow() {
        showPage(BridgeFixtures.release(id = album.primaryReleaseId, albumId = album.id).copy(showsTransferLines = true))

        compose.onNodeWithText("Transfer row").assertExists()
    }

    @Test
    fun aReleaseOnAnotherDeviceWithNoUploadDrawsNoTransferRow() {
        showPage(onAnotherDevice)
        compose.onNodeWithText("Transfer row").assertDoesNotExist()
    }

    @Test
    fun aReleaseOnAnotherDeviceKeepsItsUploadLine() {
        showPage(onAnotherDevice.copy(showsTransferLines = true))

        compose.onNodeWithText("Transfer row").assertExists()
    }

    @Test
    fun theBarOffersPlaybackActionsOnlyWhenCoreOffersPlayback() {
        val playable = BridgeFixtures.release(id = album.primaryReleaseId, albumId = album.id)

        assertNotNull(playMenuActions(playable, noAlbumCallbacks))
        assertNull(playMenuActions(onAnotherDevice, noAlbumCallbacks))
    }
}
