import AppKit
import SwiftUI
import Testing

@testable import BaeKit

/// A menu taller than the screen: its panel fits the screen, and its lines
/// scroll.
@MainActor
@Suite("Mac menu taller than the screen")
struct MacMenuScrollTests {
    private static func lines(_ count: Int) -> [MacMenuEntry] {
        (0..<count).map { .item(MacMenuItem(verbatim: "Line \($0)") {}) }
    }

    @Test("the face is as tall as its lines until they pass the screen")
    func faceHeightFollowsTheScreen() {
        let entries = Self.lines(100)
        let all = MacMenuMetrics.faceHeight(of: entries)
        #expect(
            MacMenuMetrics.faceHeight(of: entries, tallest: all + 50) == all
        )
        #expect(MacMenuMetrics.faceHeight(of: entries, tallest: 500) == 500)
        #expect(MacMenuMetrics.faceHeight(of: entries, tallest: nil) == all)
        #expect(MacMenuMetrics.tallestFace(onScreenOf: 800) == 792)
    }

    @Test("the lines scroll by what the face hides, no further")
    func scrollRangeIsWhatTheFaceHides() {
        let entries = Self.lines(100)
        let all = MacMenuMetrics.faceHeight(of: entries)
        #expect(
            MacMenuMetrics.scrollRange(of: entries, faceHeight: all) == 0
        )
        #expect(
            MacMenuMetrics.scrollRange(of: entries, faceHeight: 500)
                == all - 500
        )
        #expect(
            MacMenuMetrics.clampedScroll(
                -20,
                of: entries,
                faceHeight: 500
            ) == 0
        )
        #expect(
            MacMenuMetrics.clampedScroll(
                1e6,
                of: entries,
                faceHeight: 500
            ) == all - 500
        )
    }

    @Test("showing a line scrolls only as far as the line needs")
    func showingALineMovesLeastFar() {
        let entries = Self.lines(100)
        let face: CGFloat = 500
        let shown = face - 2 * MacMenuMetrics.padding
        let height = MacMenuMetrics.itemHeight
        // A line already showing leaves the offset alone.
        #expect(
            MacMenuMetrics.scroll(
                toShow: 3,
                of: entries,
                faceHeight: face,
                from: 0
            ) == 0
        )
        // A line below the face scrolls its bottom to the face's bottom.
        #expect(
            MacMenuMetrics.scroll(
                toShow: 50,
                of: entries,
                faceHeight: face,
                from: 0
            ) == 51 * height - shown
        )
        // A line above the face scrolls its top to the face's top.
        #expect(
            MacMenuMetrics.scroll(
                toShow: 2,
                of: entries,
                faceHeight: face,
                from: 600
            ) == 2 * height
        )
        // The last line leaves no gap under it.
        #expect(
            MacMenuMetrics.scroll(
                toShow: 99,
                of: entries,
                faceHeight: face,
                from: 0
            ) == MacMenuMetrics.scrollRange(of: entries, faceHeight: face)
        )
    }

    @Test("the panel fits the screen and arrow keys scroll the highlight in")
    func panelFitsAndHighlightScrollsIn() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 200, y: 200, width: 400, height: 300),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.animationBehavior = .none
        window.makeKeyAndOrderFront(nil)
        let session = try #require(
            MacMenuSession.present(
                Self.lines(400),
                over: window,
                placement: .at(NSPoint(x: 300, y: 450)),
                appearance: HostedAppearance(
                    tone: .standard,
                    accent: .standard
                ),
                isPopUp: false
            )
        )
        defer {
            session.close()
            window.close()
        }
        let content = try #require(
            NSApp.windows.compactMap(\.contentView)
                .first { $0.accessibilityRole() == .menu }
        )
        let panel = try #require(content.window)
        let room = MacMenuMetrics.shadowRoom
        let face = panel.frame.insetBy(dx: room, dy: room)
        #expect(
            NSScreen.screens.contains { $0.visibleFrame.contains(face) },
            "the face \(face), panel \(panel.frame), room \(room), is on none of \(NSScreen.screens.map(\.visibleFrame))"
        )

        for _ in 0..<200 { try pressDown(in: panel) }
        let lines = try #require(
            content.accessibilityChildren() as? [NSAccessibilityElement]
        )
        let highlighted = try #require(
            lines.first { $0.isAccessibilitySelected() }
        )
        #expect(highlighted.accessibilityLabel() == "Line 199")
        let frame = highlighted.accessibilityFrame()
        #expect(frame.minY >= face.minY - 0.5 && frame.maxY <= face.maxY + 0.5)
    }

    private func pressDown(in panel: NSWindow) throws {
        let event = try #require(
            NSEvent.keyEvent(
                with: .keyDown,
                location: .zero,
                modifierFlags: [],
                timestamp: 0,
                windowNumber: panel.windowNumber,
                context: nil,
                characters: "",
                charactersIgnoringModifiers: "",
                isARepeat: false,
                keyCode: 125
            )
        )
        NSApp.sendEvent(event)
    }
}
