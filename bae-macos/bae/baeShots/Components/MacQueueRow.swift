import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueueRow`, at rest and under the pointer.
    static let macQueueRowScenes: [ShotScene] = [
        queueRow("mac.component.queue-row.resting", hover: false),
        queueRow("mac.component.queue-row.hover", hover: true),
    ]

    private static func queueRow(_ id: String, hover: Bool) -> ShotScene {
        let images = ImageStore(
            fetchLibraryImageBytes: { try BoardArtwork.data(named: $0.id) }
        )
        return component(id, height: 90, images: images) {
            MacQueueRow(
                item: BridgeQueueEntry(
                    entryId: "e-harbor-lights",
                    trackId: "t-harbor-lights",
                    display: BridgeTrackDisplay(
                        title: "Harbor Lights",
                        artistLine: .named("Mara Quill"),
                        albumId: "a-01",
                        releaseId: "r-01",
                        albumTitle: "Low Tide Hours",
                        coverImage: BridgeImageRef(
                            id: "Low Tide Hours",
                            version: "1",
                            imageType: .cover
                        )
                    ),
                    durationClock: BridgeDurationClock.length(ms: 272_000)
                ),
                isHovered: { hover },
                onHoverChanged: { _ in },
                onSkipTo: { _ in },
                onRemove: { _ in }
            )
            .frame(width: 380, alignment: .leading)
            .environment(images)
        }
    }
}
