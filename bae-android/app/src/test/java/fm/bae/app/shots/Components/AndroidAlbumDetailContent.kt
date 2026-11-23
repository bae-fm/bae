package fm.bae.app.shots

import fm.bae.app.ui.albumdetail.AlbumPlaybackState
import fm.bae.app.ui.albumdetail.AndroidAlbumDetailContent
import org.junit.Test

/** The component shots of `AndroidAlbumDetailContent`, one for each row of its preview. */
class AndroidAlbumDetailContentShots : BoardShot() {
    @Test
    fun androidComponentAlbumDetailContentResting() =
        component("android.component.album-detail-content.resting") {
            Images {
                AndroidAlbumDetailContent(
                    detail = harborDetail,
                    cover = harborDetail.shownRelease.cover,
                    playback = AlbumPlaybackState(currentTrackId = null, isPlaying = false),
                    callbacks = noAlbumCallbacks,
                    releaseDownloadControl = {},
                    shownRating = { harborDetail.rating },
                )
            }
        }
}
