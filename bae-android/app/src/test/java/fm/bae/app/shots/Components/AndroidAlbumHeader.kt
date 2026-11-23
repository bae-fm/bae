package fm.bae.app.shots

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import fm.bae.app.ui.components.AndroidButton
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeDurationUnits
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeHearter
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeMediaCount
import uniffi.bae_bridge.BridgeMedium
import uniffi.bae_bridge.BridgeOtherDevice
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseArea
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.BridgeTermLabel

/** The component shots of `AndroidAlbumHeader`, one for each row of its preview. */
class AndroidAlbumHeaderShots : BoardShot() {
    @Test
    fun androidComponentAlbumHeaderDefault() = component("android.component.album-header.default") { Header(harborDetail) }

    @Test
    fun androidComponentAlbumHeaderRated() =
        component("android.component.album-header.rated") {
            Header(harborDetail.copy(rating = BridgeAlbumRating.Stars(mine = BridgeStars.FOUR, raters = listOf(BridgeRater.You))))
        }

    @Test
    fun androidComponentAlbumHeaderWithPlay() =
        component("android.component.album-header.with-play") {
            Header(harborDetail) { AndroidButton(onClick = {}) { Text(stringResource(R.string.play)) } }
        }

    @Test
    fun androidComponentAlbumHeaderHeartSystem() =
        component("android.component.album-header.heart-system") {
            Header(harborDetail.copy(rating = BridgeAlbumRating.Heart(hearted = true, hearters = listOf(BridgeHearter.You))))
        }

    @Test
    fun androidComponentAlbumHeaderNoneSystem() =
        component("android.component.album-header.none-system") {
            Header(harborDetail.copy(rating = BridgeAlbumRating.Hidden))
        }

    @Test
    fun androidComponentAlbumHeaderAReissue() =
        component("android.component.album-header.a-reissue") {
            Header(
                harborDetail.copy(
                    shownRelease =
                        harborRelease.copy(distinctYear = 2001),
                ),
            )
        }

    @Test
    fun androidComponentAlbumHeaderNotOnThisDevice() =
        component("android.component.album-header.not-on-this-device") {
            Header(
                harborDetail.copy(
                    shownRelease =
                        harborRelease.copy(
                            playability = BridgeReleasePlayability.OnOtherDevice(BridgeOtherDevice.Active("Ana’s Mac")),
                        ),
                ),
            )
        }
}

@Composable
private fun Header(
    detail: BridgeAlbumDetail,
    actions: @Composable () -> Unit = {},
) = Covers {
    AndroidAlbumHeader(
        detail = detail,
        cover = detail.shownRelease.cover,
        galleryItemCount = 0u,
        onShowGallery = {},
        onShowWhere = {},
        actions = actions,
    )
}

private val us = BridgeFactTerm.Worded(BridgeTermLabel.Localized("core.pressing.country.us"))
private val cd = BridgeFactTerm.Worded(BridgeTermLabel.Verbatim("CD"))

/** Low Tide Hours as the design system's previews pressed it: a 1992 CD on Harbor Records. */
internal val harborRelease: BridgeRelease =
    PreviewData.release().let { release ->
        release.copy(
            cover = PreviewData.imageRef("Low Tide Hours"),
            distinctYear = null,
            labels = listOf(BridgeLabelLine(names = listOf("Harbor Records"), catalogNumbers = listOf("HR-1042"))),
            facts = release.facts.copy(area = BridgeReleaseArea.Country("US"), media = listOf(BridgeMediaCount(BridgeMedium.CD, 1u))),
            pressingSummary = listOf(us, cd),
            areaTerms = listOf(us),
            mediaTerms = listOf(cd),
            totalDuration = BridgeDurationUnits.MinutesOnly(52u),
        )
    }

internal val harborDetail: BridgeAlbumDetail =
    PreviewData.albumDetail(
        PreviewData.album(title = "Low Tide Hours", year = 1992, artistNames = "Mara Quill"),
        releases = listOf(harborRelease),
    )
