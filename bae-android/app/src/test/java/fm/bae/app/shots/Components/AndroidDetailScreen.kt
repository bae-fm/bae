package fm.bae.app.shots

import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.albumdetail.AlbumMenuActions
import fm.bae.app.ui.albumdetail.AlbumOptionsMenu
import fm.bae.app.ui.albumdetail.AlbumRatingAction
import fm.bae.app.ui.albumdetail.PinMenuItem
import fm.bae.app.ui.albumdetail.PlayMenuActions
import fm.bae.app.ui.library.AndroidDetailScreen
import org.junit.Test

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
}

/** The detail screen with [actions] in its bar and the preview's dashed stand-in for the page. */
@Composable
private fun Detail(actions: @Composable RowScope.() -> Unit) =
    Images {
        AndroidDetailScreen(
            onBack = {},
            backLabel = stringResource(R.string.back_to_library),
            nowPlayingBar = { Bar(playing) },
            modifier = Modifier.height(520.dp),
            actions = actions,
        ) {
            PreviewSlot("The page")
        }
    }
