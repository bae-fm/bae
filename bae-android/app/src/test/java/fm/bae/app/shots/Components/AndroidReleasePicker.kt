package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidReleasePicker
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeReleaseName

/** The component shots of `AndroidReleasePicker`, one for each row of its preview. */
class AndroidReleasePickerShots : BoardShot() {
    @Test
    fun androidComponentReleasePickerTwoReleases() = component("android.component.release-picker.two-releases") { Picker(shown = 0) }

    @Test
    fun androidComponentReleasePickerSecondShown() = component("android.component.release-picker.second-shown") { Picker(shown = 1) }
}

@Composable
private fun Picker(shown: Int) {
    val releases =
        listOf("CD", "LP · 2014 reissue").mapIndexed { index, name ->
            PreviewData.release(id = "rel-$index").copy(name = BridgeReleaseName.Named(name))
        }
    val detail: BridgeAlbumDetail =
        PreviewData.albumDetail(releases = releases).copy(shownRelease = releases[shown], shownReleaseIndex = shown.toUInt())
    AndroidReleasePicker(detail, onSelect = {})
}
