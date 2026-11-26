import AppKit

extension AccessibilityTree {
    /// The elements of `window`'s tree whose role is one of `roles`, each
    /// before the ones it holds: what a test presses or sets through the
    /// accessibility API, as an assistive client does.
    @MainActor
    static func elements(
        of window: NSWindow,
        roles: Set<String>,
        subject: String
    ) throws -> [AXUIElement] {
        func walk(_ element: AXUIElement) throws -> [AXUIElement] {
            let role = Reader.string(element, kAXRoleAttribute)
            let own = role.map(roles.contains) == true ? [element] : []
            return try own
                + Reader.children(of: element, subject: subject).flatMap(walk)
        }
        let listed = try listedWindow(window, subject: subject)
        return try Reader.children(of: listed.element, subject: subject)
            .flatMap(walk)
    }
}

extension AccessibilityTree.Node {
    /// The words of a static text; nil for any other element.
    var textWords: String? {
        guard role == NSAccessibility.Role.staticText.rawValue,
            case .text(let words)? = value
        else { return nil }
        return words
    }

    /// The centre of the element's frame in the window's coordinates.
    @MainActor
    func center(in window: NSWindow) -> NSPoint {
        NSPoint(x: x + width / 2, y: window.frame.height - y - height / 2)
    }
}
