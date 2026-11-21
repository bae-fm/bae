import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacDetailHeader`: the artist's picture, name and count.
    static let macDetailHeaderScenes: [ShotScene] = [
        component(
            "mac.component.detail-header.default",
            page: true,
            height: 100,
            images: coverStore()
        ) {
            MacDetailHeader(summary: artistSummary("Mara Quill", albums: 12))
                .frame(width: 380, alignment: .leading)
        }
    ]
}
