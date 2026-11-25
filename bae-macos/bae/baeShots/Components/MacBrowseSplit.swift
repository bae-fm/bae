import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacBrowseSplit`: two artists in the list, the first selected, and its
    /// header in the detail pane.
    static let macBrowseSplitScenes: [ShotScene] = [
        component(
            "mac.component.browse-split.default",
            width: 1_000,
            height: 360,
            images: coverStore()
        ) {
            MacBrowseSplit {
                VStack(spacing: ThemeSpace.hairline) {
                    MacLibraryRowButton(
                        picture: cover("Mara Quill"),
                        title: "Mara Quill",
                        subtitle: "12 albums",
                        selected: true,
                        action: {}
                    )
                    MacLibraryRowButton(
                        picture: cover("Odile Fen"),
                        title: "Odile Fen",
                        subtitle: "3 albums",
                        action: {}
                    )
                }
            } detail: {
                MacDetailHeader(
                    summary: artistSummary("Mara Quill", albums: 12)
                )
            }
        }
    ]
}
