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
struct ShotAccessibilityTree: Sendable {
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

    /// The window's elements, each before the ones it holds, in the tree's
    /// order. The window itself is not one.
    let nodes: [Node]

    /// The tree of the window whose frame, in the accessibility API's
    /// coordinates (points from the top-left of the main display), is
    /// `origin` and `size`.
    @MainActor
    static func read(
        windowAt origin: CGPoint,
        size: CGSize,
        sceneId: String
    ) throws -> Self {
        let application = AXUIElementCreateApplication(getpid())
        AXUIElementSetMessagingTimeout(application, 10)
        var windows: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(
            application,
            kAXWindowsAttribute as CFString,
            &windows
        )
        guard status == .success, let list = windows as? [AXUIElement] else {
            throw ShotError.accessibilityUnavailable(sceneId, status.rawValue)
        }
        guard
            let window = list.first(where: {
                guard let frame = Reader.frame(of: $0) else { return false }
                return abs(frame.minX - origin.x) <= 1
                    && abs(frame.minY - origin.y) <= 1
                    && abs(frame.width - size.width) <= 1
                    && abs(frame.height - size.height) <= 1
            })
        else { throw ShotError.windowNotInAccessibilityTree(sceneId) }
        var reader = Reader(origin: origin)
        reader.add(children: Reader.children(of: window), parent: nil)
        return Self(nodes: reader.nodes)
    }

    private struct Reader {
        let origin: CGPoint
        var nodes: [Node] = []

        mutating func add(children: [AXUIElement], parent: Int?) {
            for child in children {
                let index = nodes.count
                guard let node = node(of: child, parent: parent) else {
                    continue
                }
                nodes.append(node)
                add(children: Self.children(of: child), parent: index)
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

        static func children(of element: AXUIElement) -> [AXUIElement] {
            attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? []
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

        private static func string(
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
