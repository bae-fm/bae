import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacReleaseCell`, a release's cover and title.
    static let macReleaseCellScenes: [ShotScene] = [
        component(
            "mac.component.release-cell.default",
            page: true,
            height: 50,
            images: coverStore()
        ) {
            MacReleaseCell(
                release: PreviewData.storageRelease(
                    cover: cover("Low Tide Hours")
                ),
                album: BridgeAlbum(
                    id: "album-store-1",
                    title: "Low Tide Hours",
                    year: 1992,
                    isCompilation: false,
                    artistLine: .named("Mara Quill"),
                    releaseIds: [],
                    primaryReleaseId: "rel-store-1",
                    cover: cover("Low Tide Hours")
                )
            )
            .frame(width: 380, alignment: .leading)
        }
    ]
}
