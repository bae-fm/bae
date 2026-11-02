import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The cover picker's remote lookup in each state that says something of
    /// itself, and the picker saving a cover.
    static let coverPickerScenes: [ShotScene] = [
        scene("cover-picker-fetching", .loading([])),
        scene("cover-picker-none-found", .linked([])),
        scene(
            "cover-picker-lookup-failed",
            .failed([], failure: DisplayError(line: "Artwork lookup failed"))
        ),
        scene("cover-picker-saving", .linked([]), isSaving: true),
    ]

    private static func scene(
        _ id: String,
        _ remote: RemoteCoverItems,
        isSaving: Bool = false
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 1_148, height: 868)) {
            AnyView(
                CoverPickerPreviewScene.lookup(remote, isSaving: isSaving)
            )
        }
    }
}
