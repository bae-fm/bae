import AppKit
import SwiftUI
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

    @Test(
        "a menu button is a pop-up to VoiceOver, and pressing it opens the menu"
    )
    func aMenuButtonIsAPopUpThatPressOpens() async throws {
        let size = NSSize(width: 120, height: 44)
        try await SnapshotTestSupport.withHostedWindow(
            MacMenuButton {
                MacMenuItem(verbatim: "First") {}
            } label: {
                Text(verbatim: "Choose")
            },
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let control = try #require(
                Self.popUps(in: host).first,
                "the tree holds no pop-up"
            )
            #expect(control.accessibilityLabel() == "Choose")
            #expect(control.accessibilityPerformPress())
            let menu = NSApp.windows.compactMap(\.contentView)
                .first { $0.accessibilityRole() == .menu }
            #expect(menu != nil, "pressing the pop-up opens no menu")
            HostedMenus.close()
        }
    }

    /// Every element of the accessibility tree under `view` that VoiceOver
    /// reads as a pop-up or a pull-down.
    private static func popUps(in view: NSView) -> [NSAccessibilityProtocol] {
        func walk(_ element: Any) -> [NSAccessibilityProtocol] {
            guard let element = element as? NSAccessibilityProtocol else {
                return []
            }
            let own: [NSAccessibilityProtocol] =
                [.popUpButton, .menuButton]
                    .contains(element.accessibilityRole())
                ? [element] : []
            let children = element.accessibilityChildren() as? [Any] ?? []
            return own + children.flatMap(walk)
        }
        return walk(view)
    }
}
