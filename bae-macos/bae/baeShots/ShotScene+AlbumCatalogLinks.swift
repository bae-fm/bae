import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The library window with Northbound open, as the release editor's
    /// scenes open it, its release holding a MusicBrainz record and the
    /// popover of its catalog links open under the facts button.
    static let albumCatalogLinksScene = ShotScene(
        id: "mac.library.albums.album-catalog-links",
        size: CGSize(width: 1_200, height: 1_020),
        frame: .window(title: "bae", titleBar: .hidden)
    ) {
        albumCatalogLinksStage()
    }

    @MainActor
    private static func albumCatalogLinksStage() -> ShotStage {
        let uiStore = UiStore()
        let detailAnswered = AlbumDetailAnswered()
        var detail = northboundAlbumDetail
        let records = [
            PreviewData.record(.musicBrainz, "northbound")
        ]
        detail.shownRelease.records = records
        detail.shownRelease.hasRecords = true
        detail.releases[0].records = records
        detail.releases[0].hasRecords = true
        let backing = boardLibraryBacking(
            uiStore: uiStore,
            albumCount: 4,
            albums: boardAlbums(first: ("Northbound", 1974)),
            openAlbum: true,
            detailAnswered: detailAnswered,
            detail: detail
        )
        let images = coverGalleryImages(of: "Northbound")
        let window =
            ShotScenes.libraryWindow(
                uiStore: uiStore,
                backing: backing,
                playbackStore: boardPlayback(),
                images: images,
                size: CGSize(width: 1_200, height: 1_020)
            )
        let opened = ZStack(alignment: .topLeading) {
            window
            catalogLinksPopover(records, buttonX: 420, buttonTop: 580)
        }
        return ShotStage(opened, images: images) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
                && detailAnswered.answered
        }
    }

    /// The popover of the catalog links drawn in place under the facts
    /// button, whose left edge is `buttonX` and top `buttonTop` in the
    /// scene, as the boards draw it: centred on the button, its body 9
    /// points under the button's bottom edge, its arrow on the button.
    static func catalogLinksPopover(
        _ records: [BridgeReleaseRecord],
        buttonX: CGFloat,
        buttonTop: CGFloat
    ) -> some View {
        let buttonWidth: CGFloat = 185.484375
        // The button holds the edition line and the copy line, each a
        // detail line, a line space apart.
        guard let detailLine = ThemeText.detail.macOSLine else {
            preconditionFailure("the detail text role has no line height")
        }
        let buttonHeight = 2 * detailLine + ThemeSpace.line
        let width = ReleaseRecordsCard.width
        return MacPopoverInPlace(arrowSide: .top, arrowOffset: width / 2) {
            ReleaseRecordsCard(records: records)
        }
        .padding(
            .leading,
            buttonX + buttonWidth / 2 - width / 2 - popoverShadowRoom
        )
        .padding(
            .top,
            buttonTop + buttonHeight + 9 - popoverArrowLength
                - popoverShadowRoom
        )
    }
}
