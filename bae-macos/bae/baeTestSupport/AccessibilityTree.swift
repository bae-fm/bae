import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

/// The accessibility tree of one of this process's own windows, read the way
/// an assistive client reads it: through the accessibility API.
///
/// SwiftUI builds its elements (texts, buttons, toggles, pop-ups) only for a
/// client of the accessibility API; the hosting view's own
/// `accessibilityChildren()` lists none of them. A process may query its own
/// windows without the accessibility permission. The API answers such a
/// request on the calling thread, so the read runs on the main actor, where
/// SwiftUI evaluates view bodies (a body that is main-actor isolated traps
/// when evaluated anywhere else).
struct AccessibilityTree: Sendable {
    /// The words' look as the tree gives it: the font's name (whose last part
    /// is the weight), its size in points and the colour as sRGB, 0 to 1.
    struct TextStyle: Sendable {
        let fontName: String
        let size: Double
        let red: Double
        let green: Double
        let blue: Double
        let alpha: Double
    }

    enum Value: Sendable {
        case text(String)
        case number(Double)
    }

    /// One element, with its box in points from the window's top-left.
    struct Node: Sendable {
        let role: String
        let subrole: String?
        let title: String?
        let description: String?
        let placeholder: String?
        let value: Value?
        let x: Double
        let y: Double
        let width: Double
        let height: Double
        let enabled: Bool
        let selected: Bool?
        let focused: Bool?
        let style: TextStyle?
        /// The index in `nodes` of the element that holds this one.
        let parent: Int?
    }

    /// The window's elements, each before the ones it holds, in the order an
    /// assistive client reads them. The window itself is not one.
    let nodes: [Node]

    /// The tree of `window`.
    @MainActor
    static func read(of window: NSWindow, subject: String) throws -> Self {
        let listed = try listedWindow(window, subject: subject)
        var reader = Reader(origin: listed.origin, subject: subject)
        try reader.add(
            children: Reader.children(of: listed.element, subject: subject),
            parent: nil
        )
        return Self(nodes: reader.nodes)
    }

    /// The element the accessibility API lists for `window`, and the
    /// window's top-left in the API's coordinates: points from the top-left
    /// of the main display.
    @MainActor
    private static func listedWindow(
        _ window: NSWindow,
        subject: String
    ) throws -> (element: AXUIElement, origin: CGPoint) {
        guard let main = NSScreen.screens.first else {
            throw ReadError.noScreen(subject)
        }
        let frame = window.frame
        let origin = CGPoint(x: frame.minX, y: main.frame.maxY - frame.maxY)
        let size = frame.size
        let application = AXUIElementCreateApplication(getpid())
        AXUIElementSetMessagingTimeout(application, 10)
        var windows: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(
            application,
            kAXWindowsAttribute as CFString,
            &windows
        )
        guard status == .success, let list = windows as? [AXUIElement] else {
            throw ReadError.unavailable(subject, status.rawValue)
        }
        // A locked screen lists no window of any app.
        guard !list.isEmpty else {
            throw ReadError.listsNoWindow(subject)
        }
        // A window that only takes clicks over another has the same frame
        // and holds nothing: the one that holds elements is the window.
        let there = list.filter {
            guard let frame = Reader.frame(of: $0) else { return false }
            return abs(frame.minX - origin.x) <= 1
                && abs(frame.minY - origin.y) <= 1
                && abs(frame.width - size.width) <= 1
                && abs(frame.height - size.height) <= 1
        }
        let holding = try there.first {
            try !Reader.children(of: $0, subject: subject).isEmpty
        }
        guard let element = holding ?? there.first else {
            throw ReadError.windowNotListed(
                subject,
                wanted: CGRect(origin: origin, size: size),
                listed: list.compactMap(Reader.frame(of:))
            )
        }
        return (element, origin)
    }

    fileprivate struct Reader {
        let origin: CGPoint
        /// What the tree is read for, which an error names.
        let subject: String
        var nodes: [Node] = []

        mutating func add(children: [AXUIElement], parent: Int?) throws {
            for child in children {
                // An element the tree gives an infinite position has no box
                // to list, and JSON has no infinity to write one as.
                // The elements it holds are listed under its parent.
                if let frame = Self.frame(of: child), !frame.isFinite {
                    try add(
                        children: Self.children(of: child, subject: subject),
                        parent: parent
                    )
                    continue
                }
                let index = nodes.count
                guard let node = node(of: child, parent: parent) else {
                    continue
                }
                nodes.append(node)
                try add(
                    children: Self.children(of: child, subject: subject),
                    parent: index
                )
            }
        }

        private func node(of element: AXUIElement, parent: Int?) -> Node? {
            guard let frame = Self.frame(of: element),
                let role = Self.string(element, kAXRoleAttribute)
            else { return nil }
            let value = Self.value(element)
            return Node(
                role: role,
                subrole: Self.string(element, kAXSubroleAttribute),
                title: Self.string(element, kAXTitleAttribute),
                description: Self.string(element, kAXDescriptionAttribute),
                placeholder: Self.string(element, kAXPlaceholderValueAttribute),
                value: value,
                x: frame.minX - origin.x,
                y: frame.minY - origin.y,
                width: frame.width,
                height: frame.height,
                enabled: Self.bool(element, kAXEnabledAttribute) ?? true,
                selected: Self.bool(element, kAXSelectedAttribute),
                focused: Self.bool(element, kAXFocusedAttribute),
                style: Self.style(of: element, value: value, role: role),
                parent: parent
            )
        }

        static func frame(of element: AXUIElement) -> CGRect? {
            var point = CGPoint.zero
            var size = CGSize.zero
            guard
                let position = attribute(element, kAXPositionAttribute),
                let extent = attribute(element, kAXSizeAttribute),
                CFGetTypeID(position) == AXValueGetTypeID(),
                CFGetTypeID(extent) == AXValueGetTypeID(),
                AXValueGetValue(
                    unsafeDowncast(position, to: AXValue.self),
                    .cgPoint,
                    &point
                ),
                AXValueGetValue(
                    unsafeDowncast(extent, to: AXValue.self),
                    .cgSize,
                    &size
                )
            else { return nil }
            return CGRect(origin: point, size: size)
        }

        private static func attribute(
            _ element: AXUIElement,
            _ name: String
        ) -> CFTypeRef? {
            var result: CFTypeRef?
            guard
                AXUIElementCopyAttributeValue(
                    element,
                    name as CFString,
                    &result
                )
                    == .success
            else { return nil }
            return result
        }

        static func string(
            _ element: AXUIElement,
            _ name: String
        ) -> String? {
            guard let text = attribute(element, name) as? String,
                !text.isEmpty
            else { return nil }
            return text
        }

        private static func bool(
            _ element: AXUIElement,
            _ name: String
        ) -> Bool? {
            attribute(element, name) as? Bool
        }

        private static func value(_ element: AXUIElement) -> Value? {
            switch attribute(element, kAXValueAttribute) {
            case let text as String: .text(text)
            case let number as NSNumber: .number(number.doubleValue)
            default: nil
            }
        }

        /// The look of a text element's words over their whole range.
        private static func style(
            of element: AXUIElement,
            value: Value?,
            role: String
        ) -> TextStyle? {
            guard role == kAXStaticTextRole, case .text(let words)? = value
            else { return nil }
            var range = CFRange(location: 0, length: words.utf16.count)
            var attributed: CFTypeRef?
            guard let rangeValue = AXValueCreate(.cfRange, &range),
                AXUIElementCopyParameterizedAttributeValue(
                    element,
                    kAXAttributedStringForRangeParameterizedAttribute
                        as CFString,
                    rangeValue,
                    &attributed
                ) == .success,
                let text = attributed as? NSAttributedString,
                text.length > 0
            else { return nil }
            let attributes = text.attributes(at: 0, effectiveRange: nil)
            guard
                let font = attributes[NSAttributedString.Key("AXFont")]
                    as? [String: Any],
                let name = font["AXFontName"] as? String,
                let size = (font["AXFontSize"] as? NSNumber)?.doubleValue,
                let color = attributes[
                    NSAttributedString.Key("AXForegroundColor")
                ],
                CFGetTypeID(color as CFTypeRef) == CGColor.typeID,
                let srgb = CGColorSpace(name: CGColorSpace.sRGB),
                let rgb = unsafeDowncast(color as AnyObject, to: CGColor.self)
                    .converted(to: srgb, intent: .defaultIntent, options: nil),
                let c = rgb.components, c.count == 4
            else { return nil }
            return TextStyle(
                fontName: name,
                size: size,
                red: c[0],
                green: c[1],
                blue: c[2],
                alpha: c[3]
            )
        }
    }
}

extension AccessibilityTree {
    /// Why a tree could not be read; `subject` names what it was read for.
    enum ReadError: Error, CustomStringConvertible {
        case noScreen(String)
        case unavailable(String, Int32)
        case listsNoWindow(String)
        case windowNotListed(String, wanted: CGRect, listed: [CGRect])
        case attributeFailed(
            String,
            element: String,
            attribute: String,
            status: Int32
        )

        var description: String {
            switch self {
            case .noScreen(let subject):
                "no display to place \(subject)'s window against"
            case .unavailable(let subject, let status):
                "the accessibility API listed no windows of this process for "
                    + "\(subject) (error \(status))"
            case .listsNoWindow(let subject):
                "the accessibility API lists no window of this process for "
                    + "\(subject); it lists none while the screen is locked"
            case .windowNotListed(let subject, let wanted, let listed):
                "the accessibility tree has no window where \(subject)'s is, "
                    + "at \(wanted); it lists \(listed)"
            case .attributeFailed(
                let subject,
                let element,
                let attribute,
                let status
            ):
                "the accessibility API failed to give \(attribute) of "
                    + "\(element) in \(subject)'s window (error \(status))"
            }
        }
    }
}

extension AccessibilityTree.Reader {
    /// The elements `element` holds, in the order VoiceOver reads them:
    /// the navigation order where the element gives one, which differs
    /// from the plain list of children for a titled window. A window lists
    /// its content first and its title bar's buttons, toolbar and title
    /// after it, and reads them title bar first. An element that has
    /// neither list holds nothing; any other failure to read one throws.
    static func children(
        of element: AXUIElement,
        subject: String
    ) throws -> [AXUIElement] {
        let navigation = NSAccessibility.Attribute
            .childrenInNavigationOrderAttribute.rawValue
        return try elementList(element, navigation, subject: subject)
            ?? elementList(element, kAXChildrenAttribute, subject: subject)
            ?? []
    }

    /// The elements `element`'s attribute `name` lists, or nil when the
    /// element has no such attribute or it holds no value. Any other
    /// failure, or a value that is not a list of elements, throws.
    private static func elementList(
        _ element: AXUIElement,
        _ name: String,
        subject: String
    ) throws -> [AXUIElement]? {
        var result: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(
            element,
            name as CFString,
            &result
        )
        switch status {
        case .success:
            if let list = result as? [AXUIElement] { return list }
        case .attributeUnsupported, .noValue:
            return nil
        default:
            break
        }
        throw AccessibilityTree.ReadError.attributeFailed(
            subject,
            element: described(element),
            attribute: name,
            status: status.rawValue
        )
    }

    /// The element as an error names it: its role and its box.
    private static func described(_ element: AXUIElement) -> String {
        let role = string(element, kAXRoleAttribute) ?? "an element"
        guard let frame = frame(of: element) else { return role }
        return "\(role) at \(frame)"
    }
}

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

extension CGRect {
    fileprivate var isFinite: Bool {
        origin.x.isFinite && origin.y.isFinite && size.width.isFinite
            && size.height.isFinite
    }
}

extension AccessibilityTree {
    /// This tree with `other`'s elements after its own, `other` read from a
    /// window that sits at `x`, `y` points from this tree's window's top-left.
    func adding(_ other: Self, atX x: Double, y: Double) -> Self {
        let base = nodes.count
        return Self(
            nodes: nodes
                + other.nodes.map {
                    $0.placed(dx: x, dy: y, parent: $0.parent.map { $0 + base })
                }
        )
    }

    /// This tree without the nodes `isLeftOut` picks and the nodes they hold.
    func leavingOut(_ isLeftOut: (Node) -> Bool) -> Self {
        // The new index of each node kept, by its old index.
        var kept: [Int: Int] = [:]
        var out: [Node] = []
        for (index, node) in nodes.enumerated() {
            var parent: Int?
            if let old = node.parent {
                // A node whose holder was left out goes with it.
                guard let new = kept[old] else { continue }
                parent = new
            }
            if isLeftOut(node) { continue }
            kept[index] = out.count
            out.append(node.placed(dx: 0, dy: 0, parent: parent))
        }
        return Self(nodes: out)
    }
}

extension AccessibilityTree.Node {
    /// This node moved `dx`, `dy` points, held by the node at `parent`.
    fileprivate func placed(dx: Double, dy: Double, parent: Int?) -> Self {
        Self(
            role: role,
            subrole: subrole,
            title: title,
            description: description,
            placeholder: placeholder,
            value: value,
            x: x + dx,
            y: y + dy,
            width: width,
            height: height,
            enabled: enabled,
            selected: selected,
            focused: focused,
            style: style,
            parent: parent
        )
    }
}
