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
import fm.bae.app.ui.albumdetail.AlbumOptionsMenu
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.library.AndroidDetailScreen
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail

/** The component shots of `AndroidDetailScreen`, one for each row of its preview. */
class AndroidDetailScreenShots : BoardShot() {
    @Test
    fun androidComponentDetailScreenResting() = detail("android.component.detail-screen.resting") {}

    @Test
    fun androidComponentDetailScreenMenu() =
        detail("android.component.detail-screen.menu") { AlbumOptionsMenu(onSetPrimaryRelease = null, onDelete = {}) }

    private fun detail(
        id: String,
        actions: @Composable RowScope.() -> Unit,
    ) = component(id, wide = true) {
        Images {
            AndroidDetailScreen(
                onBack = {},
                backLabel = stringResource(R.string.back_to_library),
                nowPlayingBar = { Bar(playing) },
                modifier = Modifier.height(480.dp),
                actions = actions,
            ) {
                Box(Modifier.padding(ThemeSpace.edge).weight(1f)) {
                    AndroidAlbumHeader(
                        detail = plainDetail,
                        cover = PreviewData.imageRef(),
                        galleryItemCount = 0u,
                        onShowGallery = {},
                    )
                }
            }
        }
    }
}

/** Low Tide Hours as the detail screen's preview draws its header: the title, artist and year. */
private val plainDetail: BridgeAlbumDetail =
    PreviewData.albumDetail(
        harborDetail.album,
        releases = listOf(harborRelease.copy(labels = emptyList(), pressingSummary = emptyList())),
    )
