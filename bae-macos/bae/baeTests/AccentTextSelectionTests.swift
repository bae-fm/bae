import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Accent text selection")
@MainActor
struct AccentTextSelectionTests {
    /// The field editor every field in a window shares selects in the chosen
    /// accent, not the Mac's: the selection and the caret follow the accent
    /// the window is drawn in.
    @Test(
        "the window's field editor selects and points in the chosen accent",
        arguments: [AccentChoice.coral, .mint]
    )
    func fieldEditorTakesTheAccent(accent: AccentChoice) async throws {
        try await SnapshotTestSupport.withHostedWindow(
            TextField("Name", text: .constant("Home music"))
                .appearance(mode: .dark, accent: accent, tone: .midnight),
            size: NSSize(width: 200, height: 40)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let editor = try #require(
                window.fieldEditor(true, for: nil) as? NSTextView
            )
            let selection = try #require(
                editor.selectedTextAttributes[.backgroundColor] as? NSColor
            )
            let expected = NSColor(accent.color(in: .dark))
            window.appearance = NSAppearance(named: .darkAqua)
            var picked = NSColor.clear
            var wanted = NSColor.clear
            window.appearance?
                .performAsCurrentDrawingAppearance {
                    picked = selection.usingColorSpace(.sRGB) ?? .clear
                    wanted = expected.usingColorSpace(.sRGB) ?? .clear
                }
            #expect(abs(picked.redComponent - wanted.redComponent) < 0.01)
            #expect(abs(picked.greenComponent - wanted.greenComponent) < 0.01)
            #expect(abs(picked.blueComponent - wanted.blueComponent) < 0.01)
            #expect(picked.alphaComponent < 1)
            #expect(editor.insertionPointColor != NSColor.controlAccentColor)
        }
    }
}
