import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The identify bar at the pane's narrowest width, 489 points.
@MainActor
@Suite("The identify bar's wrap")
struct MacIdentifyBarWrapTests {
    private static func height(atWidth width: CGFloat) -> CGFloat {
        let bar = MacIdentifyBar(
            spacing: ThemeSpace.related,
            identifyDisabled: false,
            onAutomatic: {},
            onSearch: {},
            onMetadataCommand: { _ in },
            trailing: {
                HStack(spacing: ThemeSpace.related) {
                    Text("2 tracks have no title yet, so bae can't name them")
                    Button("Import") {}
                }
            }
        )
        return NSHostingView(rootView: bar.frame(width: width)).fittingSize
            .height
    }

    @Test(
        "the right-hand group wraps onto a second line when the pane has no room"
    )
    func theTrailingGroupWraps() {
        let wide = Self.height(atWidth: 1_000)
        let narrow = Self.height(atWidth: MacImportWindow.paneFloor)
        #expect(narrow > wide)
    }
}
