package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AlbumMenuActions
import fm.bae.app.ui.albumdetail.AlbumOptionsMenu
import fm.bae.app.ui.albumdetail.AlbumRatingAction
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import fm.bae.app.ui.albumdetail.PinMenuItem
import fm.bae.app.ui.albumdetail.PlayMenuActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.library.AndroidDetailScreen
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail

/** The component shots of `AndroidDetailScreen`, one for each row of its preview. */
class AndroidDetailScreenShots : BoardShot() {
    @Test
    fun androidComponentDetailScreenResting() = component("android.component.detail-screen.resting") { Detail {} }

    @Test
    fun androidComponentDetailScreenMenu() =
        component("android.component.detail-screen.menu") {
            Detail {
                AlbumOptionsMenu(
                    AlbumMenuActions(
                        play = PlayMenuActions(onShuffle = {}, onPlayNext = {}, onAddToQueue = {}),
                        rating = { AlbumRatingAction.Rate {} },
                        pin = PinMenuItem(pinned = true) {},
                        onSetPrimaryRelease = null,
                        onDelete = {},
                    ),
                )
            }
        }

    @Test
    fun androidComponentDetailScreenFlush() = component("android.component.detail-screen.flush") { Detail(padded = false) {} }
}

/**
 * The detail screen over an album's header, with [actions] in its bar. The header sits in the page's padded column
 * unless [padded] is false, the flush row, where the page carries its own padding.
 */
@Composable
private fun Detail(
    padded: Boolean = true,
    actions: @Composable RowScope.() -> Unit,
) = Images {
    AndroidDetailScreen(
        onBack = {},
        backLabel = stringResource(R.string.back_to_library),
        nowPlayingBar = { Bar(playing) },
        modifier = Modifier.height(520.dp),
        actions = actions,
    ) {
        Box(Modifier.then(if (padded) Modifier.padding(ThemeSpace.edge) else Modifier).weight(1f)) {
            AndroidAlbumHeader(
                detail = plainDetail,
                cover = PreviewData.imageRef(),
                galleryItemCount = 0u,
                onShowGallery = {},
                onShowWhere = {},
                shownRating = { plainDetail.rating },
            )
        }
    }
}

/** Low Tide Hours as the detail screen's preview draws its header: the title, artist and year. */
private val plainDetail: BridgeAlbumDetail =
    PreviewData.albumDetail(
        harborDetail.album,
        releases =
            listOf(
                harborRelease.copy(
                    labels = emptyList(),
                    areaTerms = emptyList(),
                    mediaTerms = emptyList(),
                    totalDuration = null,
                ),
            ),
    )
