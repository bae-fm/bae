import AppKit
import Testing

@testable import BaeKit

/// How bae's menu answers an assistive technology that points at it.
@MainActor
@Suite("bae's menu to assistive technologies")
struct MacMenuAccessibilityTests {
    @Test("a point on a line answers with that line, which makes it hittable")
    func aPointOnALineIsTheLine() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 200, y: 200, width: 400, height: 300),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.makeKeyAndOrderFront(nil)
        let session = try #require(
            MacMenuSession.present(
                [
                    .item(MacMenuItem(verbatim: "First") {}),
                    .item(MacMenuItem(verbatim: "Second") {}),
                ],
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
        let lines = try #require(
            content.accessibilityChildren() as? [NSAccessibilityElement]
        )
        #expect(lines.count == 2)
        for line in lines {
            let frame = line.accessibilityFrame()
            let hit = content.accessibilityHitTest(
                NSPoint(x: frame.midX, y: frame.midY)
            )
            #expect(
                hit as AnyObject === line,
                "\(line.accessibilityLabel() ?? "") is what a point on it finds"
            )
        }
    }
}
