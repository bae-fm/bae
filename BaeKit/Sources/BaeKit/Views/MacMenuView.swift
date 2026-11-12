#if canImport(AppKit)
    import SwiftUI

    /// Sizes of a bae menu, from the design system's `MacMenu`: a face 12
    /// points rounded with 6 points of padding, items 24 points tall, a
    /// divider 11 points tall.
    enum MacMenuMetrics {
        static let padding: CGFloat = 6
        static let cornerRadius: CGFloat = 12
        static let itemCornerRadius: CGFloat = 6
        static let itemHeight: CGFloat = 24
        static let headerHeight: CGFloat = 22
        static let dividerHeight: CGFloat = 11
        static let markWidth: CGFloat = 11
        static let minWidth: CGFloat = 190
        /// Room around the face, for its shadow.
        static let shadowRoom: CGFloat = 24
        /// From the face's left edge to its items' words.
        static let wordsInset: CGFloat =
            padding + ThemeSpace.related + markWidth + ThemeSpace.compact

        static func height(of entry: MacMenuEntry) -> CGFloat {
            switch entry {
            case .item, .submenu: itemHeight
            case .header: headerHeight
            case .divider: dividerHeight
            }
        }

        /// Each line's frame in the face, measured from its top left.
        static func frames(of entries: [MacMenuEntry], width: CGFloat)
            -> [CGRect]
        {
            var top = padding
            return entries.map { entry in
                let frame = CGRect(
                    x: padding,
                    y: top,
                    width: width - 2 * padding,
                    height: height(of: entry)
                )
                top += frame.height
                return frame
            }
        }

        static func faceHeight(of entries: [MacMenuEntry]) -> CGFloat {
            2 * padding + entries.reduce(0) { $0 + height(of: $1) }
        }
    }

    /// What one open menu shows and which line the highlight is on.
    @MainActor
    @Observable
    final class MacMenuLevel {
        var entries: [MacMenuEntry]
        var highlighted: Int?

        init(entries: [MacMenuEntry], highlighted: Int? = nil) {
            self.entries = entries
            self.highlighted = highlighted
        }
    }

    /// A menu's face: bae's raised colour with a hairline edge, the
    /// highlighted line in the accent the person chose.
    struct MacMenuFace: View {
        let level: MacMenuLevel
        let isPopUp: Bool
        let hover: (Int, Bool) -> Void
        let pick: (Int) -> Void

        @Environment(\.accentChoice)
        private var accent

        var body: some View {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(level.entries.indices, id: \.self) { index in
                    line(index)
                }
            }
            .padding(MacMenuMetrics.padding)
            .frame(
                minWidth: isPopUp ? 0 : MacMenuMetrics.minWidth,
                alignment: .leading
            )
            .fixedSize()
            // The shadow belongs to the face's shape alone: on the whole
            // face it would blur the highlighted line's shadow into the
            // lines around it.
            .background(
                RoundedRectangle(cornerRadius: MacMenuMetrics.cornerRadius)
                    .fill(Theme.surfaceElevated)
                    .shadow(color: Theme.shadow, radius: 12, y: 8)
            )
            .overlay(
                RoundedRectangle(cornerRadius: MacMenuMetrics.cornerRadius)
                    .strokeBorder(Theme.hairlineStrong, lineWidth: 1)
            )
            .accessibilityHidden(true)
        }

        private func accessoryColor(
            _ accessory: MacMenuAccessory,
            _ highlighted: Bool
        ) -> Color {
            if highlighted { return Theme.onFill }
            switch accessory.tone {
            case .regular: return Theme.textPrimary
            case .secondary: return Theme.textSecondary
            case .danger: return Theme.danger
            }
        }

        @ViewBuilder
        private func line(_ index: Int) -> some View {
            switch level.entries[index] {
            case .divider:
                Rectangle()
                    .fill(Theme.hairline)
                    .frame(height: 1)
                    .padding(.horizontal, 7)
                    .frame(height: MacMenuMetrics.dividerHeight)
            case .header(let title):
                Text(title)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .padding(.leading, ThemeSpace.related)
                    .frame(
                        maxWidth: .infinity,
                        minHeight: MacMenuMetrics.headerHeight,
                        maxHeight: MacMenuMetrics.headerHeight,
                        alignment: .leading
                    )
            case .item(let item):
                row(
                    index,
                    title: item.title,
                    checked: item.isChecked,
                    disabled: item.isDisabled,
                    submenu: false
                )
            case .submenu(let title, _, let accessory):
                row(
                    index,
                    title: title,
                    checked: false,
                    disabled: false,
                    submenu: true,
                    accessory: accessory
                )
            }
        }

        private func row(
            _ index: Int,
            title: String,
            checked: Bool,
            disabled: Bool,
            submenu: Bool,
            accessory: MacMenuAccessory? = nil
        ) -> some View {
            let highlighted = level.highlighted == index && !disabled
            return HStack(spacing: ThemeSpace.compact) {
                Group {
                    if checked {
                        Image(systemName: "checkmark")
                            .font(.system(size: 10, weight: .bold))
                    }
                }
                .frame(width: MacMenuMetrics.markWidth)
                Text(verbatim: title)
                    .lineLimit(1)
                if submenu {
                    Spacer(minLength: ThemeSpace.edge)
                    if let accessory {
                        Image(systemName: accessory.symbol)
                            .font(.system(size: 11))
                            .foregroundStyle(
                                accessoryColor(accessory, highlighted)
                            )
                    }
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                }
            }
            .themeText(.body)
            .padding(.leading, ThemeSpace.related)
            .padding(.trailing, isPopUp ? ThemeSpace.edge : ThemeSpace.related)
            .frame(
                maxWidth: .infinity,
                minHeight: MacMenuMetrics.itemHeight,
                maxHeight: MacMenuMetrics.itemHeight,
                alignment: .leading
            )
            .foregroundStyle(
                disabled
                    ? Theme.textTertiary
                    : highlighted ? Theme.onFill : Theme.textPrimary
            )
            .background(
                highlighted ? accent.buttonColor : Color.clear,
                in: RoundedRectangle(
                    cornerRadius: MacMenuMetrics.itemCornerRadius
                )
            )
            .contentShape(Rectangle())
            .help(accessory?.help ?? "")
            .onHover { hover(index, $0) }
            .onTapGesture { if !disabled { pick(index) } }
        }
    }

    /// The face in its panel, drawn in the tone, accent and appearance of
    /// the window it opened from.
    struct MacMenuPanelContent: View {
        let level: MacMenuLevel
        let isPopUp: Bool
        let appearance: HostedAppearance
        let colorScheme: ColorScheme
        let hover: (Int, Bool) -> Void
        let pick: (Int) -> Void

        var body: some View {
            MacMenuFace(
                level: level,
                isPopUp: isPopUp,
                hover: hover,
                pick: pick
            )
            .padding(MacMenuMetrics.shadowRoom)
            .hostedAppearance(appearance)
            .environment(\.colorScheme, colorScheme)
        }
    }
#endif

#if canImport(AppKit)
    /// A menu's face drawn in place, as it is when open, for captures and
    /// previews; the app's menus draw it in a panel.
    public struct MacMenuPreview: View {
        let entries: [MacMenuEntry]
        let highlighted: Int?
        let isPopUp: Bool

        public init(
            @MacMenuBuilder _ entries: () -> [MacMenuEntry],
            highlighted: Int? = nil,
            isPopUp: Bool = false
        ) {
            self.entries = entries()
            self.highlighted = highlighted
            self.isPopUp = isPopUp
        }

        public var body: some View {
            MacMenuFace(
                level: MacMenuLevel(entries: entries, highlighted: highlighted),
                isPopUp: isPopUp,
                hover: { _, _ in },
                pick: { _ in }
            )
        }
    }
#endif
