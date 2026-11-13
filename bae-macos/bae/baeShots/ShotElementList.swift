import AppKit
import Foundation

/// Every key of an element but `text` and `placeholder` is written, null when the tree has no
/// answer.
private enum ElementKey: String, CodingKey {
    case role, native, label, value, placeholder, bounds, enabled, selected,
        focused
    case text, parent
}

/// What a shot holds, as its window's accessibility tree lists it: each
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
        /// The words a field shows while it is empty; absent when the tree
        /// gives none.
        let placeholder: String?
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
            try container.encodeIfPresent(placeholder, forKey: .placeholder)
            try container.encode(bounds, forKey: .bounds)
            try container.encode(enabled, forKey: .enabled)
            try container.encode(selected, forKey: .selected)
            try container.encode(focused, forKey: .focused)
            try container.encodeIfPresent(text, forKey: .text)
            try container.encode(parent, forKey: .parent)
        }
    }

    /// The elements of `window`'s accessibility tree, boxed in points from the
    /// top-left of the window, which is the shot's top-left. The view the
    /// pixels come from is `size` points big.
    @MainActor
    static func read(
        board: String,
        window: NSWindow,
        size: CGSize,
        scale: Int
    ) throws -> Self {
        let tree = try AccessibilityTree.read(of: window, subject: board)
        return Self(
            board: board,
            size: Size(width: size.width, height: size.height),
            scale: scale,
            elements: tree.nodes.map(Element.init(node:))
        )
    }

    /// The JSON the list is written as.
    func json() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(self)
    }
}

extension ShotElementList.Element {
    /// The element a tree node is: its role from the platform's own, its
    /// words, value and look as the tree gives them.
    init(node: AccessibilityTree.Node) {
        let role = Self.role(native: node.role, subrole: node.subrole)
        self.init(
            role: role,
            native: node.role,
            label: role == "text"
                ? Self.words(node.value) : node.description ?? node.title,
            value: role == "text"
                ? nil : Self.valueText(node.value, role: role),
            placeholder: node.placeholder,
            bounds: ShotElementList.Bounds(
                x: node.x,
                y: node.y,
                width: node.width,
                height: node.height
            ),
            enabled: node.enabled,
            selected: node.selected,
            focused: node.focused,
            text: role == "text" ? node.style.flatMap(Self.textStyle) : nil,
            parent: node.parent
        )
    }

    private static func words(_ value: AccessibilityTree.Value?) -> String? {
        guard case .text(let text)? = value, !text.isEmpty else { return nil }
        return text
    }

    /// A toggle's value is `"on"` or `"off"`; any other number is written as
    /// the tree gives it.
    private static func valueText(
        _ value: AccessibilityTree.Value?,
        role: String
    ) -> String? {
        switch value {
        case .text(let text)?: text.isEmpty ? nil : text
        case .number(let number)?:
            role == "toggle"
                ? (number == 0 ? "off" : "on")
                : (number == number.rounded()
                    ? String(Int(number)) : String(number))
        case nil: nil
        }
    }

    private static func role(native: String, subrole: String?) -> String {
        if subrole == "AXTabButton" { return "tab" }
        if subrole == "AXSwitch" { return "toggle" }
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

    /// The look the tree reports, nil when its font's weight is not one of
    /// the nine named weights.
    private static func textStyle(
        _ style: AccessibilityTree.TextStyle
    ) -> ShotElementList.TextStyle? {
        let part = style.fontName.split(separator: "-").last.map(String.init)
        let named = (part ?? "").replacingOccurrences(of: "Italic", with: "")
        guard let weight = weights[named.isEmpty ? "Regular" : named] else {
            return nil
        }
        func byte(_ value: Double) -> Int { Int((value * 255).rounded()) }
        return ShotElementList.TextStyle(
            size: style.size,
            weight: weight,
            color: String(
                format: "#%02X%02X%02X%02X",
                byte(style.red),
                byte(style.green),
                byte(style.blue),
                byte(style.alpha)
            )
        )
    }

    /// The weight names a font name ends in, on the 100 to 900 scale.
    private static let weights: [String: Int] = [
        "Ultralight": 100, "Thin": 200, "Light": 300, "Regular": 400,
        "Medium": 500, "Semibold": 600, "Bold": 700, "Heavy": 800,
        "Black": 900,
    ]
}
