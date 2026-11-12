#if canImport(AppKit)
    import SwiftUI

    /// The view a menu opens from, which knows where it is on the screen.
    @MainActor
    final class MacMenuAnchor {
        weak var view: NSView?

        var window: NSWindow? { view?.window }

        /// The view's frame in screen coordinates.
        var screenFrame: NSRect? {
            guard let view, let window = view.window else { return nil }
            return window.convertToScreen(view.convert(view.bounds, to: nil))
        }
    }

    private struct MacMenuAnchorView: NSViewRepresentable {
        let anchor: MacMenuAnchor

        private final class Passive: NSView {
            override func hitTest(_ point: NSPoint) -> NSView? { nil }
        }

        func makeNSView(context: Context) -> NSView {
            let view = Passive()
            anchor.view = view
            return view
        }

        func updateNSView(_ view: NSView, context: Context) {
            anchor.view = view
        }
    }

    /// The tone and accent the menu is drawn in, taken from where it opens.
    private struct MenuAppearance: ViewModifier {
        @Binding
        var appearance: HostedAppearance

        @Environment(\.surfaceTone)
        private var tone
        @Environment(\.accentChoice)
        private var accent

        func body(content: Content) -> some View {
            content.onChange(of: tone, initial: true) {
                appearance = HostedAppearance(tone: tone, accent: accent)
            }
            .onChange(of: accent) {
                appearance = HostedAppearance(tone: tone, accent: accent)
            }
        }
    }

    extension View {
        fileprivate func tracksMenuAppearance(
            _ appearance: Binding<HostedAppearance>
        ) -> some View {
            modifier(MenuAppearance(appearance: appearance))
        }
    }

    /// A button that opens bae's menu under itself. The label is whatever the
    /// caller draws; `bordered` draws it in a push button.
    public struct MacMenuButton<Label: View>: View {
        let bordered: Bool
        let content: () -> [MacMenuEntry]
        let label: Label

        @State
        private var anchor = MacMenuAnchor()
        @State
        private var appearance = HostedAppearance(
            tone: .standard,
            accent: .standard
        )

        public init(
            bordered: Bool = false,
            @MacMenuBuilder content: @escaping () -> [MacMenuEntry],
            @ViewBuilder label: () -> Label
        ) {
            self.bordered = bordered
            self.content = content
            self.label = label()
        }

        public var body: some View {
            Group {
                if bordered {
                    Button(action: open) { label }
                        .buttonStyle(.bordered)
                }
                else {
                    Button(action: open) { label }
                        .buttonStyle(.plain)
                }
            }
            .background(MacMenuAnchorView(anchor: anchor))
            .tracksMenuAppearance($appearance)
            .accessibilityAddTraits(.isButton)
        }

        private func open() {
            guard let window = anchor.window, let frame = anchor.screenFrame
            else { return }
            MacMenuSession.present(
                content(),
                over: window,
                placement: .below(anchor: frame),
                appearance: appearance,
                isPopUp: false
            )
        }
    }

    /// bae's pop-up button: the chosen value, then a round button in the
    /// control fill holding the up-and-down chevron. It opens a menu over
    /// itself with the chosen option on its line and checked.
    public struct MacPopUpButton<Value: Hashable>: View {
        let label: Text
        @Binding
        var selection: Value
        let options: [MacPopUpEntry<Value>]

        @State
        private var anchor = MacMenuAnchor()
        @State
        private var appearance = HostedAppearance(
            tone: .standard,
            accent: .standard
        )

        @Environment(\.isEnabled)
        private var isEnabled

        public init(
            _ title: LocalizedStringKey,
            selection: Binding<Value>,
            @MacPopUpBuilder<Value> options: () -> [MacPopUpEntry<Value>]
        ) {
            self.init(
                label: Text(title),
                selection: selection,
                options: options
            )
        }

        /// A pop-up whose name VoiceOver reads is a text of the caller's
        /// making, such as one from another string table.
        public init(
            label: Text,
            selection: Binding<Value>,
            @MacPopUpBuilder<Value> options: () -> [MacPopUpEntry<Value>]
        ) {
            self.label = label
            self._selection = selection
            self.options = options()
        }

        private var chosenTitle: String {
            for case .option(let option) in options
            where option.value == selection {
                return option.title
            }
            return ""
        }

        public var body: some View {
            Button(action: open) {
                HStack(spacing: ThemeSpace.related) {
                    Text(verbatim: chosenTitle)
                        .themeText(.body)
                        .foregroundStyle(Theme.textPrimary)
                        .lineLimit(1)
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(Theme.textPrimary)
                        .frame(width: 22, height: 22)
                        .background(Theme.tile, in: Circle())
                }
                .frame(height: 24)
                .opacity(isEnabled ? 1 : 0.4)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .background(MacMenuAnchorView(anchor: anchor))
            .tracksMenuAppearance($appearance)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label)
            .accessibilityValue(Text(verbatim: chosenTitle))
            .accessibilityAddTraits(.isButton)
        }

        private func open() {
            guard let window = anchor.window, let frame = anchor.screenFrame
            else { return }
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
                            ) { selection = option.value }
                        )
                    )
                }
            }
            MacMenuSession.present(
                entries,
                over: window,
                placement: .over(anchor: frame, chosen: chosen),
                appearance: appearance,
                isPopUp: true
            )
        }
    }

    /// The view over a region that takes a right click (or a control click)
    /// and nothing else: every other event goes through to what is below.
    private final class ContextClickView: NSView {
        var onClick: ((NSPoint) -> Void)?

        override func hitTest(_ point: NSPoint) -> NSView? {
            guard let event = NSApp.currentEvent else { return nil }
            let isContextClick =
                event.type == .rightMouseDown
                || (event.type == .leftMouseDown
                    && event.modifierFlags.contains(.control))
            return isContextClick ? super.hitTest(point) : nil
        }

        override func rightMouseDown(with event: NSEvent) {
            onClick?(NSEvent.mouseLocation)
        }

        override func mouseDown(with event: NSEvent) {
            onClick?(NSEvent.mouseLocation)
        }
    }

    private struct ContextClickRepresentable: NSViewRepresentable {
        let onClick: (NSWindow, NSPoint) -> Void

        func makeNSView(context: Context) -> NSView {
            let view = ContextClickView()
            view.onClick = { [weak view] point in
                guard let window = view?.window else { return }
                onClick(window, point)
            }
            return view
        }

        func updateNSView(_ view: NSView, context: Context) {
            (view as? ContextClickView)?.onClick = { [weak view] point in
                guard let window = view?.window else { return }
                onClick(window, point)
            }
        }
    }

    private struct MacContextMenu: ViewModifier {
        let entries: () -> [MacMenuEntry]

        @State
        private var appearance = HostedAppearance(
            tone: .standard,
            accent: .standard
        )

        func body(content: Content) -> some View {
            content
                .overlay(
                    ContextClickRepresentable { window, point in
                        MacMenuSession.present(
                            entries(),
                            over: window,
                            placement: .at(point),
                            appearance: appearance,
                            isPopUp: false
                        )
                    }
                )
                .tracksMenuAppearance($appearance)
        }
    }

    extension View {
        /// bae's menu at the point of a right click on this view.
        public func macContextMenu(
            @MacMenuBuilder _ content: @escaping () -> [MacMenuEntry]
        ) -> some View {
            modifier(MacContextMenu(entries: content))
        }
    }
#endif
