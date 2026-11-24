import AppKit
import ApplicationServices
import SwiftUI
import Testing

@testable import bae

/// The Save as panel's format line, read through the accessibility API as
/// VoiceOver reads it.
@MainActor
@Suite("Save as format line accessibility")
struct MacSavePanelFormatAccessibilityTests {
    private struct FormatLine: NSViewRepresentable {
        func makeNSView(context _: Context) -> NSView {
            let popup = NSPopUpButton(
                frame: MacSavePanelFormat.popupFrame,
                pullsDown: false
            )
            popup.addItems(withTitles: ["FLAC", "MP3"])
            return MacSavePanelFormat(popup: popup)
        }

        func updateNSView(_: NSView, context _: Context) {}
    }

    @Test(
        "the format pop-up is named by the Format label beside it, without its colon"
    )
    func popUpIsNamedByItsLabel() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            FormatLine().frame(width: 250, height: 34),
            size: NSSize(width: 250, height: 34)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "the Save as format line"
            )
            let popUps = tree.nodes.filter { $0.role == kAXPopUpButtonRole }
            #expect(popUps.map(\.controlName) == ["Format"])
        }
    }
}
