package fm.bae.app.shots

import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeMediaCount
import uniffi.bae_bridge.BridgeMedium
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeTermLabel

/** The component shots of `AndroidAlbumHeader`, one for each row of its preview. */
class AndroidAlbumHeaderShots : BoardShot() {
    @Test
    fun androidComponentAlbumHeaderDefault() =
        component("android.component.album-header.default", wide = true) {
            Images {
                AndroidAlbumHeader(
                    detail = harborDetail,
                    cover = PreviewData.imageRef(),
                    galleryItemCount = 0u,
                    onShowGallery = {},
                )
            }
        }

    @Test
    fun androidComponentAlbumHeaderAReissue() =
        component("android.component.album-header.a-reissue", wide = true) {
            Images {
                AndroidAlbumHeader(
                    detail = harborDetail.copy(shownRelease = harborRelease.copy(distinctYear = 2001)),
                    cover = PreviewData.imageRef(),
                    galleryItemCount = 0u,
                    onShowGallery = {},
                )
            }
        }
}

/** Low Tide Hours as the design system's previews pressed it: a 1992 CD on Harbor Records. */
internal val harborRelease: BridgeRelease =
    PreviewData.release().let { release ->
        release.copy(
            distinctYear = null,
            labels = listOf(BridgeLabelLine(names = listOf("Harbor Records"), catalogNumbers = listOf("HR-1042"))),
            facts = release.facts.copy(area = null, media = listOf(BridgeMediaCount(BridgeMedium.CD, 1u))),
            pressingSummary = listOf(BridgeFactTerm.Worded(BridgeTermLabel.Verbatim("CD"))),
        )
    }

internal val harborDetail: BridgeAlbumDetail =
    PreviewData.albumDetail(
        PreviewData.album(title = "Low Tide Hours", year = 1992, artistNames = "Mara Quill"),
        releases = listOf(harborRelease),
    )
