#if canImport(AppKit)
    import SwiftUI

    /// What a Mac alert says: a bold title, a small secondary message, and
    /// under it, when there is one, a quiet warning, an error's causes, and
    /// the box that stops the alert asking again. The buttons are given apart,
    /// since a caller that waits for an answer builds them differently.
    public struct MacAlertWords {
        public struct Suppression {
            public let label: Text
            public let isOn: Binding<Bool>

            public init(_ label: Text, isOn: Binding<Bool>) {
                self.label = label
                self.isOn = isOn
            }
        }

        public struct Details {
            public let copyText: String
            public let causes: String

            public init(copyText: String, causes: String) {
                self.copyText = copyText
                self.causes = causes
            }
        }

        public let title: Text
        public let message: Text?
        /// What the choice costs, as a quiet warning between the title and
        /// the message.
        public let warning: String?
        public let details: Details?
        public let suppression: Suppression?

        public init(
            _ title: Text,
            message: Text? = nil,
            warning: String? = nil,
            details: Details? = nil,
            suppression: Suppression? = nil
        ) {
            self.title = title
            self.message = message
            self.warning = warning
            self.details = details
            self.suppression = suppression
        }
    }

    /// One button of a Mac alert. The first button listed is the default,
    /// which Return presses and which is drawn in the accent unless it is
    /// `destructive`; a `cancel` button is what Escape presses.
    public struct MacAlertButton {
        public enum Role {
            case regular, destructive, cancel
        }

        public let title: Text
        public let role: Role
        public let action: @MainActor () -> Void

        public init(
            _ title: Text,
            role: Role = .regular,
            action: @escaping @MainActor () -> Void = {}
        ) {
            self.title = title
            self.role = role
            self.action = action
        }
    }

    /// bae's alert: no app icon, an elevated face, the title in bold, the
    /// message small and secondary, and bae's buttons below. Two buttons sit
    /// side by side at equal width with the default on the right, unless a
    /// label is wider than half the row; then, as with three or more, they
    /// stack with the default on top. Each button runs its action, then
    /// `finish`.
    public struct MacAlert: View {
        public static let width: CGFloat = 260

        let words: MacAlertWords
        let buttons: [MacAlertButton]
        let finish: @MainActor () -> Void

        @FocusState
        private var focused: Bool

        public init(
            _ words: MacAlertWords,
            buttons: [MacAlertButton],
            finish: @escaping @MainActor () -> Void = {}
        ) {
            self.words = words
            self.buttons = buttons
            self.finish = finish
        }

        public var body: some View {
            VStack(
                alignment: .leading,
                spacing: ThemeSpace.related + ThemeSpace.line
            ) {
                words.title
                    .themeText(.strong)
                    .fixedSize(horizontal: false, vertical: true)
                if let warning = words.warning {
                    MacNotice(
                        title: warning,
                        advice: nil,
                        error: nil,
                        tone: .warning,
                        look: .quiet
                    )
                }
                if let message = words.message {
                    message
                        .themeText(.alertMessage)
                        .foregroundStyle(Theme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let details = words.details {
                    HoverErrorDetails(
                        detail: details.copyText,
                        excerpt: details.causes
                    )
                }
                if let suppression = words.suppression {
                    // The board's checkbox is in the detail size and is as
                    // wide as its label.
                    MacCheckbox(isOn: suppression.isOn, detail: true) {
                        suppression.label
                    }
                }
                buttonRow
                    .padding(.top, ThemeSpace.compact)
            }
            .padding(.top, ThemeSpace.edge + ThemeSpace.inline)
            .padding([.horizontal, .bottom], ThemeSpace.edge)
            .frame(width: Self.width, alignment: .leading)
            // The board's alert is a dialog named by its title.
            .accessibilityPanel(words.title)
            .focusable()
            .focused($focused)
            .focusEffectDisabled()
            .onAppear { focused = true }
            // A lone button takes Escape too; with several, the cancel button
            // does, and an alert with none ignores it.
            .onExitCommand {
                if buttons.count == 1 { choose(buttons[0]) }
            }
        }

        @ViewBuilder
        private var buttonRow: some View {
            if buttons.count > 2 {
                stackedButtons
            }
            else {
                // Side by side while the widest label fits half the row, as
                // macOS lays out an alert's two buttons; otherwise stacked,
                // the default on top, so no label is cut short.
                ViewThatFits(in: .horizontal) {
                    // The default button sits on the right, and VoiceOver
                    // reads the buttons in the order they were listed, the
                    // default first, as it reads a system alert's. Each
                    // button is as wide as the widest label wants, so the
                    // row fits only when both halves hold their words.
                    HStack(spacing: ThemeSpace.related) {
                        ForEach(buttons.indices.reversed(), id: \.self) {
                            index in
                            button(at: index, sizedBy: buttons.map(\.title))
                                .accessibilitySortPriority(
                                    Double(buttons.count - index)
                                )
                        }
                    }
                    .accessibilityElement(children: .contain)
                    stackedButtons
                }
            }
        }

        /// The buttons one above another, the default on top.
        private var stackedButtons: some View {
            VStack(spacing: ThemeSpace.related) {
                ForEach(buttons.indices, id: \.self) { index in
                    button(at: index)
                }
            }
        }

        private func button(at index: Int, sizedBy titles: [Text] = [])
            -> some View
        {
            let button = buttons[index]
            let isDefault = index == 0
            return MacAlertButtonView(
                button: button,
                isDefault: isDefault,
                sizingTitles: titles,
                run: { choose(button) }
            )
        }

        private func choose(_ button: MacAlertButton) {
            button.action()
            finish()
        }
    }

    /// The corner radius of an alert's buttons: a regular button's, not the
    /// large button's capsule, as the board draws them at the alert's height.
    private let alertButtonRadius: CGFloat = 5

    /// An alert's default button: the accent's button fill under white text, 28
    /// points high with the corners of a regular button, as the board draws it.
    private struct MacAlertPrimaryButtonStyle: ButtonStyle {
        @Environment(\.accentChoice)
        private var accent
        @Environment(\.isEnabled)
        private var isEnabled

        func makeBody(configuration: Configuration) -> some View {
            let shape = RoundedRectangle(cornerRadius: alertButtonRadius)
            configuration.label
                .foregroundStyle(Color.white)
                .padding(.horizontal, 12)
                .frame(height: 28)
                .background(accent.buttonColor, in: shape)
                .overlay {
                    shape.fill(Theme.pressed)
                        .opacity(configuration.isPressed ? 1 : 0)
                }
                .contentShape(shape)
                .opacity(isEnabled ? 1 : ThemeOpacity.off)
        }
    }

    /// A button filling its row at an alert's button height.
    private struct MacAlertButtonView: View {
        let button: MacAlertButton
        let isDefault: Bool
        /// Labels the button takes the width of, hidden, so that buttons
        /// side by side all want the widest one's width.
        let sizingTitles: [Text]
        let run: () -> Void

        var body: some View {
            let label = ZStack {
                ForEach(sizingTitles.indices, id: \.self) { index in
                    sizingTitles[index].hidden()
                }
                button.title
            }
            .frame(maxWidth: .infinity)
            Group {
                if isDefault && button.role == .regular {
                    Button(action: run) { label }
                        .buttonStyle(
                            MacAlertPrimaryButtonStyle()
                                .accentFocus(cornerRadius: alertButtonRadius)
                        )
                }
                else {
                    Button(action: run) {
                        label.foregroundStyle(
                            button.role == .destructive
                                ? Theme.danger : Theme.textPrimary
                        )
                    }
                    .buttonStyle(
                        SecondaryButtonStyle(cornerRadius: alertButtonRadius)
                            .accentFocus(cornerRadius: alertButtonRadius)
                    )
                }
            }
            .controlSize(.large)
            .keyboardShortcut(shortcut)
        }

        /// Return presses the default button and Escape the cancel button.
        private var shortcut: KeyboardShortcut? {
            if isDefault { return .defaultAction }
            return button.role == .cancel ? .cancelAction : nil
        }
    }

    /// The alert's face, 36 points below the top of the parent window, with
    /// room around it for its shadow, drawn in the parent's look.
    struct AlertPanelContent: View {
        static let hangs: CGFloat = 36
        static let shadowRoom = SheetFaceMetrics.shadowRoom

        let build: () -> (MacAlertWords, [MacAlertButton])?
        let finish: @MainActor () -> Void
        let look: PanelLook

        var body: some View {
            if let built = build() {
                MacAlert(built.0, buttons: built.1, finish: finish)
                    .sheetFace()
                    .padding(.top, Self.hangs)
                    .padding([.horizontal, .bottom], Self.shadowRoom)
                    .hostedAppearance(look.appearance)
                    .environment(\.colorScheme, look.scheme)
            }
        }
    }

    /// An alert that waits for an answer: `ask` shows the alert and returns the
    /// index of the button chosen, in the order the buttons were listed.
    @MainActor
    @Observable
    public final class AlertAsker {
        struct Asked {
            let words: MacAlertWords
            let buttons: [MacAlertButton]
        }

        private(set) var asked: Asked?
        private var answer: CheckedContinuation<Int, Never>?

        public init() {}

        /// Shows `alert` and answers with the index of the button chosen,
        /// after that button's own action has run.
        public func ask(_ alert: (MacAlertWords, [MacAlertButton])) async -> Int
        {
            precondition(asked == nil, "An alert is already asking")
            return await withCheckedContinuation { continuation in
                answer = continuation
                asked = Asked(
                    words: alert.0,
                    buttons: alert.1.enumerated()
                        .map { index, button in
                            MacAlertButton(button.title, role: button.role) {
                                [weak self] in
                                button.action()
                                self?.answered(index)
                            }
                        }
                )
            }
        }

        private func answered(_ index: Int) {
            answer?.resume(returning: index)
            answer = nil
        }

        fileprivate func close() {
            asked = nil
        }
    }

    /// The view an alert is raised from, which knows its window.
    private final class WindowAnchor {
        weak var view: NSView?
        /// What to do once the view is in a window, for an alert asked for
        /// before there is one to raise it over.
        var arrived: (@MainActor () -> Void)?
    }

    private final class AnchorNSView: NSView {
        weak var anchor: WindowAnchor?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard window != nil, let arrived = anchor?.arrived else { return }
            anchor?.arrived = nil
            MainActor.assumeIsolated { arrived() }
        }
    }

    private struct WindowAnchorView: NSViewRepresentable {
        let anchor: WindowAnchor

        func makeNSView(context: Context) -> NSView {
            let view = AnchorNSView()
            view.anchor = anchor
            // Out of the key-view loop and the view tree's drawing: it only
            // knows its window.
            view.isHidden = true
            anchor.view = view
            return view
        }

        func updateNSView(_ view: NSView, context: Context) {
            anchor.view = view
        }
    }

    /// What presents an alert in its window while something is set.
    private struct Presenting: ViewModifier {
        enum Presentation: Equatable {
            case hidden, shown
            case replacing(item: UUID, group: UUID)

            init(_ isPresented: Bool) {
                self = isPresented ? .shown : .hidden
            }

            var replacementGroup: UUID? {
                guard case .replacing(_, let group) = self else { return nil }
                return group
            }
        }

        let presentation: Presentation
        let build: () -> (MacAlertWords, [MacAlertButton])?
        let finish: @MainActor () -> Void

        @State
        private var id = UUID()
        @State
        private var anchor = WindowAnchor()
        /// The look the alert shown now draws in, which follows the
        /// window's tone, accent and scheme while the alert is up.
        @State
        private var look: PanelLook?
        @Environment(\.modalWindows)
        private var modals
        @Environment(\.surfaceTone)
        private var tone
        @Environment(\.accentChoice)
        private var accent
        @Environment(\.colorScheme)
        private var scheme

        func body(content: Content) -> some View {
            content
                .background(
                    WindowAnchorView(anchor: anchor).frame(width: 0, height: 0)
                )
                .onChange(of: presentation, initial: true) { _, presentation in
                    if presentation != .hidden {
                        raise()
                    }
                    else {
                        anchor.arrived = nil
                        modals?.withdraw(id: id)
                    }
                }
                .onChange(of: hostedAppearance) { _, appearance in
                    look?.appearance = appearance
                }
                .onChange(of: scheme) { _, scheme in
                    look?.scheme = scheme
                    modals?.matchParentAppearance(id: id)
                }
                .onDisappear {
                    anchor.arrived = nil
                    modals?.withdraw(id: id)
                }
        }

        /// The window's tone and accent, which the alert draws in.
        private var hostedAppearance: HostedAppearance {
            HostedAppearance(tone: tone, accent: accent)
        }

        /// Shows the alert over the anchor's window, or when there is none
        /// yet (an alert asked for as its view is first built), as soon as
        /// the view is in one.
        private func raise() {
            guard let modals else {
                reportMissingModalStack(presenting: "an alert")
                return
            }
            let anchor = anchor
            let id = id
            let replacementGroup = presentation.replacementGroup
            let look = PanelLook(appearance: hostedAppearance, scheme: scheme)
            self.look = look
            let content = AlertPanelContent(
                build: build,
                finish: finish,
                look: look
            )
            let show: @MainActor () -> Void = {
                modals.present(
                    id: id,
                    over: anchor.view?.window,
                    replacementGroup: replacementGroup,
                    content: content
                )
            }
            let canPresent =
                if replacementGroup != nil {
                    anchor.view?.window != nil
                }
                else {
                    modals.canPresent(over: anchor.view?.window)
                }
            if canPresent {
                show()
            }
            else {
                anchor.arrived = show
            }
        }
    }

    extension View {
        /// Shows bae's alert while `asker` has a question open.
        public func presentsAlerts(of asker: AlertAsker) -> some View {
            modifier(
                Presenting(
                    presentation: .init(asker.asked != nil),
                    build: { asker.asked.map { ($0.words, $0.buttons) } },
                    finish: { asker.close() }
                )
            )
        }

        /// Shows bae's alert while `isPresented`; any button closes it.
        public func macAlert(
            isPresented: Binding<Bool>,
            _ alert: @escaping () -> (MacAlertWords, [MacAlertButton])
        ) -> some View {
            modifier(
                Presenting(
                    presentation: .init(isPresented.wrappedValue),
                    build: { alert() },
                    finish: { isPresented.wrappedValue = false }
                )
            )
        }

        /// Shows bae's alert for `item` while it is set; any button closes it
        /// by clearing `item`, after the button's own action.
        public func macAlert<Item>(
            item: Binding<Item?>,
            _ alert: @escaping (Item) -> (MacAlertWords, [MacAlertButton])
        ) -> some View {
            modifier(
                Presenting(
                    presentation: .init(item.wrappedValue != nil),
                    build: { item.wrappedValue.map(alert) },
                    finish: { item.wrappedValue = nil }
                )
            )
        }

        /// Shows an item in this view's window, replacing the previous alert
        /// in the same group even if it was shown over another window.
        public func macAlert<Item: Identifiable>(
            item: Item?,
            replacingIn group: UUID,
            dismiss: @escaping @MainActor (UUID) -> Void,
            _ alert: @escaping (Item) -> (MacAlertWords, [MacAlertButton])
        ) -> some View where Item.ID == UUID {
            modifier(
                Presenting(
                    presentation: item.map {
                        .replacing(item: $0.id, group: group)
                    } ?? .hidden,
                    build: { item.map(alert) },
                    finish: { if let item { dismiss(item.id) } }
                )
            )
        }
    }
#endif
