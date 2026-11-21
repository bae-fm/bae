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
            width: 700,
            height: 360,
            images: coverStore()
        ) {
            MacBrowseSplit {
                VStack(spacing: 0) {
                    MacLibraryRow(
                        title: "Mara Quill",
                        subtitle: "12 albums",
                        selected: true
                    )
                    MacLibraryRow(title: "Odile Fen", subtitle: "3 albums")
                }
                .frame(maxHeight: .infinity, alignment: .top)
            } detail: {
                MacDetailHeader(
                    summary: artistSummary("Mara Quill", albums: 12)
                )
                .frame(maxHeight: .infinity, alignment: .top)
                .padding(ThemeSpace.edge)
            }
        }
    ]
}
