#if canImport(AppKit)
    import SwiftUI

    /// One line of a bae menu, as the menu's builder collects it.
    public enum MacMenuEntry {
        case item(MacMenuItem)
        case divider
        /// A quiet title over the group of items under it.
        case header(String)
        case submenu(String, [MacMenuEntry], MacMenuAccessory?)

        /// Whether the highlight can rest on this line.
        var isChoosable: Bool {
            switch self {
            case .item(let item): !item.isDisabled
            case .submenu: true
            case .divider, .header: false
            }
        }

        public var title: String? {
            switch self {
            case .item(let item): item.title
            case .submenu(let title, _, _): title
            case .divider, .header: nil
            }
        }
    }

    /// A glyph at the end of a line, with the words a pointer resting on the
    /// line shows: a folder's scan state, say.
    public struct MacMenuAccessory {
        public enum Tone {
            case regular, secondary, danger
        }

        /// What the accessory draws: an SF Symbol, or the system spinner for
        /// a wait of no known length.
        enum Glyph {
            case symbol(String)
            case spinner
        }

        let glyph: Glyph
        public let tone: Tone
        public let help: String?

        public init(symbol: String, tone: Tone = .regular, help: String? = nil)
        {
            self.init(glyph: .symbol(symbol), tone: tone, help: help)
        }

        /// A spinner, for a line whose items are still being read.
        public static func spinner(tone: Tone = .regular, help: String? = nil)
            -> MacMenuAccessory
        {
            MacMenuAccessory(glyph: .spinner, tone: tone, help: help)
        }

        private init(glyph: Glyph, tone: Tone, help: String?) {
            self.glyph = glyph
            self.tone = tone
            self.help = help
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
        /// What the item says, and what accessibility reads when the item
        /// draws an image in its place.
        public let title: String
        /// What the menu draws in place of the title, such as a rating's
        /// stars; `nil` draws the title.
        public let image: AnyView?
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
            self.image = nil
            self.isChecked = checked
            self.isDisabled = disabled
            self.action = action
        }

        /// An item the menu draws as `image`, with a localized title that
        /// accessibility reads.
        public init(
            _ title: LocalizedStringResource,
            image: some View,
            checked: Bool = false,
            action: @escaping () -> Void
        ) {
            self.title = String(localized: title)
            self.image = AnyView(image)
            self.isChecked = checked
            self.isDisabled = false
            self.action = action
        }

        public var entries: [MacMenuEntry] { [.item(self)] }
    }

    extension MacMenuItem {
        /// The line between groups of items: `MacMenuItem.divider` stands
        /// among the items.
        public static var divider: MacMenuDivider { MacMenuDivider() }
    }

    /// A line between groups of items; `MacMenuItem.divider` makes one.
    public struct MacMenuDivider: MacMenuPart {
        fileprivate init() {}

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
        let accessory: MacMenuAccessory?
        let content: [MacMenuEntry]

        public init(
            _ title: LocalizedStringResource,
            @MacMenuBuilder content: () -> [MacMenuEntry]
        ) {
            self.init(
                verbatim: String(localized: title),
                content: content
            )
        }

        public init(
            verbatim title: String,
            accessory: MacMenuAccessory? = nil,
            @MacMenuBuilder content: () -> [MacMenuEntry]
        ) {
            self.title = title
            self.accessory = accessory
            self.content = content()
        }

        public var entries: [MacMenuEntry] {
            [.submenu(title, content, accessory)]
        }
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
        /// What the closed pop-up shows while this option is chosen, when
        /// that is shorter than the option's own words.
        public let closedTitle: String?

        public init(
            _ title: LocalizedStringResource,
            value: Value,
            disabled: Bool = false,
            closedTitle: String? = nil
        ) {
            self.init(
                verbatim: String(localized: title),
                value: value,
                disabled: disabled,
                closedTitle: closedTitle
            )
        }

        public init(
            verbatim title: String,
            value: Value,
            disabled: Bool = false,
            closedTitle: String? = nil
        ) {
            self.title = title
            self.value = value
            self.isDisabled = disabled
            self.closedTitle = closedTitle
        }
    }

    /// One line of a pop-up: an option to choose or a divider.
    public enum MacPopUpEntry<Value: Hashable> {
        case option(MacPopUpOption<Value>)
        case divider
    }

    /// The menu a pop-up opens: an item for each option, the one `selection`
    /// names checked, and a divider for each divider.
    public struct MacMenu<Value: Hashable> {
        public let entries: [MacMenuEntry]
        /// The line of the checked item, which the menu opens over the
        /// pop-up; `nil` when no option is `selection`.
        public let chosen: Int?

        public init(
            _ options: [MacPopUpEntry<Value>],
            selection: Value,
            select: @escaping (Value) -> Void
        ) {
            var entries: [MacMenuEntry] = []
            var chosen: Int?
            for entry in options {
                switch entry {
                case .divider:
                    entries.append(.divider)
                case .option(let option):
                    let isChosen = option.value == selection
                    if isChosen { chosen = entries.count }
                    entries.append(
                        .item(
                            MacMenuItem(
                                verbatim: option.title,
                                checked: isChosen,
                                disabled: option.isDisabled
                            ) { select(option.value) }
                        )
                    )
                }
            }
            self.entries = entries
            self.chosen = chosen
        }
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

        public static func buildExpression(_ entries: [MacPopUpEntry<Value>])
            -> [MacPopUpEntry<Value>]
        {
            entries
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
