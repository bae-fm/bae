package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.BridgeFixtures
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.library.AndroidPlaylistHeader
import org.junit.Test
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgePersonRef
import uniffi.bae_bridge.BridgePlaylistCover
import uniffi.bae_bridge.BridgePlaylistSummary
import uniffi.bae_bridge.BridgeSharing

/** The component shots of `AndroidPlaylistHeader`, one for each row of its preview. */
class AndroidPlaylistHeaderShots : BoardShot() {
    @Test
    fun androidComponentPlaylistHeaderPlaylist() =
        component("android.component.playlist-header.playlist") {
            Header(
                BridgeFixtures.playlistSummary(name = "Late night", sharing = BridgeSharing.HOUSEHOLD).copy(
                    owner = BridgePersonRef(person = "person-1", name = "Ana", initials = "A", avatarVersion = null),
                    cover = BridgePlaylistCover.Grid(covers = List(4) { PreviewData.imageRef("img-$it") }),
                ),
            )
        }

    @Test
    fun androidComponentPlaylistHeaderLongName() =
        component("android.component.playlist-header.long-name") {
            Header(
                BridgeFixtures.playlistSummary(name = "Sunday mornings in the kitchen").copy(
                    owner = BridgePersonRef(person = "person-2", name = "Ben", initials = "B", avatarVersion = null),
                    trackCount = BridgeCountLabel(key = "core.library.track_count", count = 1uL),
                    cover = BridgePlaylistCover.Single(PreviewData.imageRef("img-1")),
                ),
            )
        }
}

@Composable
private fun Header(summary: BridgePlaylistSummary) = Images { AndroidPlaylistHeader(summary, onPlay = {}, onShuffle = {}) }
