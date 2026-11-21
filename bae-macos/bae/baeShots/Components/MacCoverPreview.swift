import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macCoverPreviewScenes: [ShotScene] = [
        coverPreviewScene("mac.component.cover-preview.first", position: 1),
        coverPreviewScene("mac.component.cover-preview.third", position: 3),
    ]

    /// The preview of `cover.jpg`, the `position`th of six images, between
    /// the arrows that step to its neighbors.
    private static func coverPreviewScene(
        _ id: String,
        position: UInt32
    ) -> ShotScene {
        component(
            id,
            width: 360,
            height: 440,
            images: coverImages()
        ) {
            CoverPreviewStage(content: coverPreviewContent(position: position))
                .frame(width: 360)
        }
    }

    /// Six images with `cover.jpg` focused at `position`, between its
    /// neighbors.
    private static func coverPreviewContent(position: UInt32)
        -> CoverGalleryContent
    {
        var content = CoverGalleryContent.empty
        content.total = 6
        content.focus = BridgeGalleryFocus(
            cover: coverEntry("cover.jpg"),
            position: position - 1,
            neighbors: BridgeCoverNeighbors(
                previous: .file(fileId: "disc.jpg"),
                next: .file(fileId: "back.jpg")
            )
        )
        return content
    }
}

/// `MacCoverPreview` with the keyboard focus binding the gallery owns.
private struct CoverPreviewStage: View {
    let content: CoverGalleryContent

    @FocusState
    private var focused: Bool

    var body: some View {
        MacCoverPreview(
            content: content,
            isSaving: false,
            previewFocused: $focused,
            onOpenLightbox: {},
            onFocus: { _ in }
        )
    }
}
