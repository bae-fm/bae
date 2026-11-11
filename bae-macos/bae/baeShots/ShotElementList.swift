import AppKit
import Foundation

/// Every key of an element but `text` is written, null when the tree has no
/// answer.
private enum ElementKey: String, CodingKey {
    case role, native, label, value, bounds, enabled, selected, focused
    case text, parent
}

/// What a shot holds, as the hosted view's accessibility tree lists it: each
/// element's role, words, value and box, written beside the shot's PNG as
/// `<board>@macos.json` for the shot check to compare with the board. The
/// shape is the one both platforms write (plans/briefs/design-names.md, "The
/// element list each shot writes"): points from the shot's top-left, `scale`
/// PNG pixels per point, elements in the tree's order, no guesses for what
/// the tree lacks.
struct ShotElementList: Encodable {
    let board: String
    let platform = "macos"
    let size: Size
    let scale: Int
    let elements: [Element]

    struct Size: Encodable, Equatable {
        let width: Double
        let height: Double
    }

    struct Bounds: Encodable, Equatable {
        let x: Double
        let y: Double
        let width: Double
        let height: Double
    }

    /// The style a text element draws its words in.
    struct TextStyle: Encodable, Equatable {
        let size: Double
        /// 100 to 900.
        let weight: Int
        /// `#RRGGBBAA`, sRGB.
        let color: String
    }

    struct Element: Encodable {
        let role: String
        /// The platform's own role string, as read.
        let native: String
        let label: String?
        let value: String?
        let bounds: Bounds
        let enabled: Bool
        let selected: Bool?
        let focused: Bool?
        /// Absent when the tree gives no style for the element's words.
        let text: TextStyle?
        /// The index in the list of the element that holds this one.
        let parent: Int?

        func encode(to encoder: Encoder) throws {
            var container = encoder.container(keyedBy: ElementKey.self)
            try container.encode(role, forKey: .role)
            try container.encode(native, forKey: .native)
            try container.encode(label, forKey: .label)
            try container.encode(value, forKey: .value)
            try container.encode(bounds, forKey: .bounds)
            try container.encode(enabled, forKey: .enabled)
            try container.encode(selected, forKey: .selected)
            try container.encode(focused, forKey: .focused)
            try container.encodeIfPresent(text, forKey: .text)
            try container.encode(parent, forKey: .parent)
        }
    }

    /// The elements below `root`'s accessibility tree, boxed in points from
    /// the top-left of `frame`, the view the shot's pixels are drawn from.
    @MainActor
    static func read(
        board: String,
        root: NSView,
        frame: NSView,
        scale: Int
    ) -> Self {
        var reader = Reader(frame: frame)
        reader.add(children: topElements(below: root), parent: nil)
        return Self(
            board: board,
            size: Size(
                width: frame.bounds.width,
                height: frame.bounds.height
            ),
            scale: scale,
            elements: reader.elements
        )
    }

    /// The elements the tree starts with below `view`. The hosting view
    /// lists none itself; what SwiftUI draws hangs from the first views
    /// below it that are accessibility elements (its scroll view), whose
    /// own children are SwiftUI's elements.
    @MainActor
    private static func topElements(below view: NSView) -> [Any] {
        if let children = view.accessibilityChildren(), !children.isEmpty {
            return children
        }
        return view.subviews.flatMap { subview in
            subview.isAccessibilityElement()
                ? [subview] : topElements(below: subview)
        }
    }

    /// The JSON the list is written as.
    func json() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(self)
    }

    @MainActor
    private struct Reader {
        let frame: NSView
        var elements: [Element] = []

        mutating func add(children: [Any], parent: Int?) {
            for child in children {
                guard let object = child as? any NSAccessibilityProtocol else {
                    continue
                }
                let index = elements.count
                elements.append(element(of: object, parent: parent))
                add(
                    children: object.accessibilityChildren() ?? [],
                    parent: index
                )
            }
        }

        private func element(
            of object: any NSAccessibilityProtocol,
            parent: Int?
        ) -> Element {
            let native = object.accessibilityRole()?.rawValue ?? "none"
            let role = Self.role(
                native: native,
                subrole: object.accessibilitySubrole()
            )
            let label =
                Self.words(object.accessibilityLabel())
                ?? Self.words(object.accessibilityTitle())
            return Element(
                role: role,
                native: native,
                label: label,
                value: Self.valueText(object.accessibilityValue()),
                bounds: bounds(of: object.accessibilityFrame()),
                enabled: object.isAccessibilityEnabled(),
                selected: object.isAccessibilitySelected(),
                focused: object.isAccessibilityFocused(),
                text: role == "text"
                    ? Self.style(of: object, words: label) : nil,
                parent: parent
            )
        }

        /// A screen rectangle as points from `frame`'s top-left.
        private func bounds(of screen: NSRect) -> Bounds {
            let inWindow = frame.window?.convertFromScreen(screen) ?? screen
            var box = frame.convert(inWindow, from: nil)
            if !frame.isFlipped {
                box.origin.y = frame.bounds.height - box.maxY
            }
            return Bounds(
                x: box.minX,
                y: box.minY,
                width: box.width,
                height: box.height
            )
        }

        private static func words(_ text: String?) -> String? {
            guard let text, !text.isEmpty else { return nil }
            return text
        }

        private static func valueText(_ value: Any?) -> String? {
            switch value {
            case let text as String: words(text)
            case let number as NSNumber: number.stringValue
            default: nil
            }
        }

        private static func role(
            native: String,
            subrole: NSAccessibility.Subrole?
        )
            -> String
        {
            if subrole?.rawValue == "AXTabButton" { return "tab" }
            if subrole?.rawValue == "AXSwitch" { return "toggle" }
            return roles[native] ?? "other"
        }

        /// The element roles by the platform's own role.
        private static let roles: [String: String] = [
            NSAccessibility.Role.staticText.rawValue: "text",
            NSAccessibility.Role.button.rawValue: "button",
            NSAccessibility.Role.checkBox.rawValue: "toggle",
            NSAccessibility.Role.radioButton.rawValue: "radio",
            NSAccessibility.Role.textField.rawValue: "field",
            NSAccessibility.Role.textArea.rawValue: "field",
            NSAccessibility.Role.comboBox.rawValue: "field",
            NSAccessibility.Role.popUpButton.rawValue: "popup",
            NSAccessibility.Role.menuButton.rawValue: "popup",
            NSAccessibility.Role.link.rawValue: "link",
            NSAccessibility.Role.image.rawValue: "image",
            NSAccessibility.Role.slider.rawValue: "slider",
            NSAccessibility.Role.progressIndicator.rawValue: "progress",
        ]

        /// The style of `words` as the element's attributed string gives it
        /// over its whole range, or nil when it gives no font or colour.
        private static func style(
            of object: any NSAccessibilityProtocol,
            words: String?
        ) -> TextStyle? {
            guard let words,
                let attributed = object.accessibilityAttributedString(
                    for: NSRange(location: 0, length: words.utf16.count)
                ),
                attributed.length > 0
            else { return nil }
            let attributes = attributed.attributes(at: 0, effectiveRange: nil)
            guard let font = attributes[.font] as? NSFont,
                let color = attributes[.foregroundColor] as? NSColor,
                let hex = hex(of: color)
            else { return nil }
            return TextStyle(
                size: font.pointSize,
                weight: weight(of: font),
                color: hex
            )
        }

        private static func hex(of color: NSColor) -> String? {
            guard let rgb = color.usingColorSpace(.sRGB) else { return nil }
            func byte(_ value: CGFloat) -> Int { Int((value * 255).rounded()) }
            return String(
                format: "#%02X%02X%02X%02X",
                byte(rgb.redComponent),
                byte(rgb.greenComponent),
                byte(rgb.blueComponent),
                byte(rgb.alphaComponent)
            )
        }

        /// The font's weight on the 100 to 900 scale, from the descriptor's
        /// weight trait.
        private static func weight(of font: NSFont) -> Int {
            let traits =
                font.fontDescriptor.object(
                    forKey: .traits
                ) as? [NSFontDescriptor.TraitKey: Any]
            let trait =
                (traits?[.weight] as? NSNumber)
                .map { CGFloat($0.doubleValue) } ?? 0
            let scale: [(NSFont.Weight, Int)] = [
                (.ultraLight, 100), (.thin, 200), (.light, 300),
                (.regular, 400), (.medium, 500), (.semibold, 600),
                (.bold, 700), (.heavy, 800), (.black, 900),
            ]
            return scale.min(by: {
                abs($0.0.rawValue - trait) < abs($1.0.rawValue - trait)
            })?
            .1 ?? 400
        }
    }
}
