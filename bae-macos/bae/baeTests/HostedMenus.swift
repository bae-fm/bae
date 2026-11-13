import AppKit
import Testing

@testable import BaeKit

/// bae's menu controls in a hosted view tree. A menu control is a SwiftUI
/// button with no AppKit control behind it, so the one thing the AppKit tree
/// holds for it is its anchor view, which sits where the control is drawn.
/// A test finds the controls by their anchors, opens one with a click, and
/// reads or presses the lines of the menu that opens as VoiceOver does.
@MainActor
enum HostedMenus {
    /// Every menu control `host` holds, top to bottom and left to right.
    static func controls(in host: NSView) -> [NSView] {
        SnapshotTestSupport.descendants(of: host)
            .filter { $0 is MacMenuAnchorNSView }
            .sorted { first, second in
                let a = first.convert(first.bounds, to: nil)
                let b = second.convert(second.bounds, to: nil)
                if abs(a.midY - b.midY) > 4 { return a.midY > b.midY }
                return a.minX < b.minX
            }
    }

    /// The lines of the menu a click on `control` opens, or nil when the click
    /// opens none, as one on a disabled control does. The menu is closed again.
    static func titles(of control: NSView) throws -> [String]? {
        guard let lines = try open(control) else { return nil }
        let titles = lines.compactMap { $0.accessibilityLabel() }
        close()
        return titles
    }

    /// Whether a click on `control` opens its menu.
    static func opens(_ control: NSView) throws -> Bool {
        try titles(of: control) != nil
    }

    /// Open `control`'s menu and press its line titled `title`, which runs the
    /// line's action and closes the menu. False when the menu has no such
    /// line, or does not open; the menu is closed again.
    @discardableResult
    static func choose(_ title: String, from control: NSView) throws -> Bool {
        guard let lines = try open(control) else { return false }
        guard
            let line = lines.first(where: { $0.accessibilityLabel() == title })
        else {
            close()
            return false
        }
        return line.accessibilityPerformPress()
    }

    private static func open(
        _ control: NSView
    ) throws -> [NSAccessibilityElement]? {
        try HostedInput.click(
            at: NSPoint(x: control.bounds.midX, y: control.bounds.midY),
            in: control
        )
        return openMenu()
    }

    /// The lines of the menu open now, if one is.
    private static func openMenu() -> [NSAccessibilityElement]? {
        for window in NSApp.windows {
            guard let content = window.contentView,
                content.accessibilityRole() == .menu
            else { continue }
            let lines = content.accessibilityChildren() as? [Any] ?? []
            return lines.compactMap { $0 as? NSAccessibilityElement }
        }
        return nil
    }

    /// Close the open menu with Escape, which the menu reads from the
    /// application's events as a person's key press arrives.
    private static func close() {
        guard
            let panel = NSApp.windows.first(where: {
                $0.contentView?.accessibilityRole() == .menu
            }),
            let escape = NSEvent.keyEvent(
                with: .keyDown,
                location: .zero,
                modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: panel.windowNumber,
                context: nil,
                characters: "\u{1b}",
                charactersIgnoringModifiers: "\u{1b}",
                isARepeat: false,
                keyCode: 53
            )
        else { return }
        autoreleasepool { NSApp.sendEvent(escape) }
    }
}
