#if canImport(AppKit)
    import SwiftUI

    /// One line of a bae menu, as the menu's builder collects it.
    public enum MacMenuEntry {
        case item(MacMenuItem)
        case divider
        /// A quiet title over the group of items under it.
        case header(String)
        case submenu(String, [MacMenuEntry])

        /// Whether the highlight can rest on this line.
        var isChoosable: Bool {
            switch self {
            case .item(let item): !item.isDisabled
            case .submenu: true
            case .divider, .header: false
            }
        }

        var title: String? {
            switch self {
            case .item(let item): item.title
            case .submenu(let title, _): title
            case .divider, .header: nil
            }
        }
    }

    /// What can stand in a menu's builder: an item, a divider, a section, a
    /// submenu.
    public protocol MacMenuPart {
        var entries: [MacMenuEntry] { get }
    }

    /// An item that does something when picked. `checked` draws the mark a
    /// pop-up puts on its chosen item and a menu on an option that is on.
    public struct MacMenuItem: MacMenuPart {
        public let title: String
        public let isChecked: Bool
        public let isDisabled: Bool
        public let action: () -> Void

        /// A title that is localized.
        public init(
            _ title: LocalizedStringResource,
            checked: Bool = false,
            disabled: Bool = false,
            action: @escaping () -> Void
        ) {
            self.init(
                verbatim: String(localized: title),
                checked: checked,
                disabled: disabled,
                action: action
            )
        }

        /// A title that is a name or a value, shown as it is.
        public init(
            verbatim title: String,
            checked: Bool = false,
            disabled: Bool = false,
            action: @escaping () -> Void
        ) {
            self.title = title
            self.isChecked = checked
            self.isDisabled = disabled
            self.action = action
        }

        public var entries: [MacMenuEntry] { [.item(self)] }
    }

    /// A line between groups of items.
    public struct MacMenuDivider: MacMenuPart {
        public init() {}

        public var entries: [MacMenuEntry] { [.divider] }
    }

    /// Items under a quiet title.
    public struct MacMenuSection: MacMenuPart {
        let title: String
        let content: [MacMenuEntry]

        public init(
            _ title: LocalizedStringResource,
            @MacMenuBuilder content: () -> [MacMenuEntry]
        ) {
            self.init(verbatim: String(localized: title), content: content)
        }

        public init(
            verbatim title: String,
            @MacMenuBuilder content: () -> [MacMenuEntry]
        ) {
            self.title = title
            self.content = content()
        }

        public var entries: [MacMenuEntry] { [.header(title)] + content }
    }

    /// A line that opens more items to its side.
    public struct MacMenuSubmenu: MacMenuPart {
        let title: String
        let content: [MacMenuEntry]

        public init(
            _ title: LocalizedStringResource,
            @MacMenuBuilder content: () -> [MacMenuEntry]
        ) {
            self.init(verbatim: String(localized: title), content: content)
        }

        public init(
            verbatim title: String,
            @MacMenuBuilder content: () -> [MacMenuEntry]
        ) {
            self.title = title
            self.content = content()
        }

        public var entries: [MacMenuEntry] { [.submenu(title, content)] }
    }

    @resultBuilder
    public enum MacMenuBuilder {
        public static func buildExpression(_ part: some MacMenuPart)
            -> [MacMenuEntry]
        {
            part.entries
        }

        public static func buildExpression(_ entries: [MacMenuEntry])
            -> [MacMenuEntry]
        {
            entries
        }

        public static func buildBlock(_ parts: [MacMenuEntry]...)
            -> [MacMenuEntry]
        {
            parts.flatMap { $0 }
        }

        public static func buildOptional(_ part: [MacMenuEntry]?)
            -> [MacMenuEntry]
        {
            part ?? []
        }

        public static func buildEither(first part: [MacMenuEntry])
            -> [MacMenuEntry]
        {
            part
        }

        public static func buildEither(second part: [MacMenuEntry])
            -> [MacMenuEntry]
        {
            part
        }

        public static func buildArray(_ parts: [[MacMenuEntry]])
            -> [MacMenuEntry]
        {
            parts.flatMap { $0 }
        }
    }

    /// What can stand in a pop-up's builder: an option, a divider.
    public struct MacPopUpOption<Value: Hashable> {
        public let title: String
        public let value: Value
        public let isDisabled: Bool

        public init(
            _ title: LocalizedStringResource,
            value: Value,
            disabled: Bool = false
        ) {
            self.init(
                verbatim: String(localized: title),
                value: value,
                disabled: disabled
            )
        }

        public init(
            verbatim title: String,
            value: Value,
            disabled: Bool = false
        ) {
            self.title = title
            self.value = value
            self.isDisabled = disabled
        }
    }

    /// One line of a pop-up: an option to choose or a divider.
    public enum MacPopUpEntry<Value: Hashable> {
        case option(MacPopUpOption<Value>)
        case divider
    }

    @resultBuilder
    public enum MacPopUpBuilder<Value: Hashable> {
        public static func buildExpression(_ option: MacPopUpOption<Value>)
            -> [MacPopUpEntry<Value>]
        {
            [.option(option)]
        }

        public static func buildExpression(_ divider: MacMenuDivider)
            -> [MacPopUpEntry<Value>]
        {
            [.divider]
        }

        public static func buildBlock(_ parts: [MacPopUpEntry<Value>]...)
            -> [MacPopUpEntry<Value>]
        {
            parts.flatMap { $0 }
        }

        public static func buildOptional(_ part: [MacPopUpEntry<Value>]?)
            -> [MacPopUpEntry<Value>]
        {
            part ?? []
        }

        public static func buildEither(first part: [MacPopUpEntry<Value>])
            -> [MacPopUpEntry<Value>]
        {
            part
        }

        public static func buildEither(second part: [MacPopUpEntry<Value>])
            -> [MacPopUpEntry<Value>]
        {
            part
        }

        public static func buildArray(_ parts: [[MacPopUpEntry<Value>]])
            -> [MacPopUpEntry<Value>]
        {
            parts.flatMap { $0 }
        }
    }
#endif
