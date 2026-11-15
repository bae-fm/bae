package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import fm.bae.app.creditLineNamed
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AlbumDetailCallbacks
import fm.bae.app.ui.albumdetail.AlbumDetailContent
import fm.bae.app.ui.albumdetail.AlbumPlaybackState
import fm.bae.app.ui.library.AndroidDetailScreen
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeCreditRole
import uniffi.bae_bridge.BridgeCreditRow
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.BridgeTermLabel
import uniffi.bae_bridge.BridgeTrackWork

/** The shots of the album page's boards: the page with its rating, its works and credits, its catalog links and files. */
class AlbumBoardShots : BoardShot() {
    @Test
    fun androidAlbumRating() =
        shot("android.album.rating") {
            AlbumPage(
                boardAlbum(
                    rating =
                        BridgeAlbumRating(
                            canRate = true,
                            mine = BridgeStars.FOUR,
                            raters = listOf(BridgeRater.You, BridgeRater.Person("Ana", BridgeStars.FIVE)),
                        ),
                ),
            )
        }

    @Test
    fun androidAlbumCredits() =
        shot("android.album.credits") {
            val detail = boardAlbum()
            val release = detail.releases[0]
            val work = BridgeTrackWork("work-ebb", "Low Tide Suite: I. Ebb", "Ada Lindqvist")
            val groups =
                release.trackGroups.map { group ->
                    group.copy(tracks = group.tracks.map { if (it.id == "trk-4") it.copy(works = listOf(work)) else it })
                }
            val credits =
                listOf("Producer" to "Ines Varga", "Engineer" to "Pell Marsh", "Arranger" to "Ada Lindqvist").map { (role, name) ->
                    BridgeCreditRow(
                        role = BridgeCreditRole(BridgeTermLabel.Verbatim(role), emptyList()),
                        artists = creditLineNamed(name),
                        tracks = emptyList(),
                    )
                }
            val credited =
                release.copy(trackGroups = groups, tracks = groups.flatMap { it.tracks }, credits = credits)
            AlbumPage(detail.copy(releases = listOf(credited)))
        }
}

/** "Low Tide Hours" by Mara Quill with the six tracks the boards list, one disc, as rated by [rating]. */
internal fun boardAlbum(
    rating: BridgeAlbumRating = BridgeAlbumRating(canRate = false, mine = null, raters = emptyList()),
): BridgeAlbumDetail {
    val titles = listOf("Harbor Lights", "The Long Way Home", "Second Floor", "Low Tide Hours", "Tin Roof", "Margins")
    val tracks = titles.mapIndexed { index, title -> PreviewData.track("trk-${index + 1}", title, index + 1) }
    val album = PreviewData.album(title = "Low Tide Hours", year = 1992, artistNames = "Mara Quill")
    val release = PreviewData.release(albumId = album.id, trackGroups = listOf(PreviewData.trackGroup(tracks)))
    return PreviewData.albumDetail(album, listOf(release)).copy(rating = rating)
}

/** The album page as the boards draw it: its detail screen, with nothing playing. */
@Composable
internal fun AlbumPage(detail: BridgeAlbumDetail) {
    ScreenScene {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            AndroidDetailScreen(onBack = {}, backLabel = "Library", nowPlayingBar = {}) {
                AlbumDetailContent(
                    detail = detail,
                    selectedRelease = detail.releases[0],
                    cover = null,
                    playback = AlbumPlaybackState(currentTrackId = null, isPlaying = false),
                    callbacks =
                        AlbumDetailCallbacks(
                            onSelectRelease = {},
                            onTogglePlayPause = {},
                            onPlayTrack = {},
                            onPlayRelease = {},
                            onShuffleRelease = {},
                            onPlayReleaseNext = {},
                            onAddReleaseToQueue = {},
                            onPlayTrackNext = {},
                            onAddTrackToQueue = {},
                            onRateAlbum = {},
                            onSelectWork = {},
                        ),
                    releaseDownloadControl = {},
                )
            }
        }
    }
}
