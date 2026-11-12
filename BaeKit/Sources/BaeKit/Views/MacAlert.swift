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
    /// side by side at equal width with the default on the right; three or
    /// more stack with the default on top. Each button runs its action, then
    /// `finish`.
    public struct MacAlertView: View {
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
                    .fontWeight(.bold)
                    .fixedSize(horizontal: false, vertical: true)
                if let warning = words.warning {
                    Notice(
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
                        .lineSpacing(ThemeSpace.line / 2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let details = words.details {
                    HoverErrorDetails(
                        detail: details.copyText,
                        excerpt: details.causes
                    )
                }
                if let suppression = words.suppression {
                    Toggle(isOn: suppression.isOn) { suppression.label }
                        .toggleStyle(.checkbox)
                        .themeText(.detail)
                }
                buttonRow
                    .padding(.top, ThemeSpace.compact)
            }
            .padding(.top, ThemeSpace.edge + ThemeSpace.inline)
            .padding([.horizontal, .bottom], ThemeSpace.edge)
            .frame(width: Self.width, alignment: .leading)
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
                VStack(spacing: ThemeSpace.related) {
                    ForEach(buttons.indices, id: \.self) { index in
                        button(at: index)
                    }
                }
            }
            else {
                HStack(spacing: ThemeSpace.related) {
                    ForEach(buttons.indices.reversed(), id: \.self) { index in
                        button(at: index)
                    }
                }
            }
        }

        private func button(at index: Int) -> some View {
            let button = buttons[index]
            let isDefault = index == 0
            return MacAlertButtonView(
                button: button,
                isDefault: isDefault,
                run: { choose(button) }
            )
        }

        private func choose(_ button: MacAlertButton) {
            button.action()
            finish()
        }
    }

    /// A button filling its row at an alert's button height.
    private struct MacAlertButtonView: View {
        let button: MacAlertButton
        let isDefault: Bool
        let run: () -> Void

        var body: some View {
            let label = button.title
                .frame(maxWidth: .infinity)
            Group {
                if isDefault && button.role == .regular {
                    Button(action: run) { label }
                        .buttonStyle(PrimaryButtonStyle())
                }
                else {
                    Button(action: run) {
                        label.foregroundStyle(
                            button.role == .destructive
                                ? Theme.danger : Color.primary
                        )
                    }
                    .buttonStyle(SecondaryButtonStyle())
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

    /// A borderless clear panel that takes the keyboard, so an alert's
    /// buttons answer Return and Escape.
    private final class AlertPanel: NSPanel {
        override var canBecomeKey: Bool { true }
    }

    /// The alert's content in its panel: bae's face hung 36 points below the
    /// top of the window it sits on, with room around it for its shadow.
    private struct AlertPanelContent: View {
        static let hangs: CGFloat = 36
        static let shadowRoom: CGFloat = 24

        let build: () -> (MacAlertWords, [MacAlertButton])?
        let finish: @MainActor () -> Void

        var body: some View {
            if let built = build() {
                MacAlertView(built.0, buttons: built.1, finish: finish)
                    .elevatedFace()
                    .padding(.top, Self.hangs)
                    .padding([.horizontal, .bottom], Self.shadowRoom)
            }
        }
    }

    /// Shows alerts. An alert is a sheet of its own kind: a clear panel
    /// holding bae's face, attached to the window of the view that raised it,
    /// or to the sheet that is open on that window, so it is over everything the person can see and
    /// the window below it cannot be reached, by pointer, keyboard or
    /// VoiceOver, while it is up.
    @MainActor
    public final class AlertWindows {
        public static let shared = AlertWindows()

        private struct Shown {
            let id: UUID
            let panel: NSPanel
            let parent: NSWindow
        }

        private var shown: [Shown] = []

        /// The window an alert raised now sits over: `window`, or when the
        /// view that raised it is in none yet the key or main one, under
        /// whatever sheets are open on it.
        private static func target(over window: NSWindow?) -> NSWindow? {
            var window =
                window ?? NSApp.keyWindow ?? NSApp.mainWindow
                ?? NSApp.orderedWindows.first { $0.isVisible }
            while let sheet = window?.attachedSheet {
                window = sheet
            }
            return window
        }

        func present(
            id: UUID,
            over window: NSWindow?,
            build: @escaping () -> (MacAlertWords, [MacAlertButton])?,
            finish: @escaping @MainActor () -> Void
        ) {
            withdraw(id: id)
            guard let parent = Self.target(over: window) else {
                preconditionFailure("There is no window to show an alert over")
            }
            let host = NSHostingView(
                rootView: AlertPanelContent(build: build, finish: finish)
            )
            let panel = AlertPanel(
                contentRect: NSRect(origin: .zero, size: host.fittingSize),
                styleMask: [.borderless],
                backing: .buffered,
                defer: false
            )
            panel.isReleasedWhenClosed = false
            panel.isOpaque = false
            panel.backgroundColor = .clear
            panel.hasShadow = false
            panel.animationBehavior = .none
            panel.contentView = host
            shown.append(Shown(id: id, panel: panel, parent: parent))
            parent.beginSheet(panel)
        }

        func withdraw(id: UUID) {
            guard let index = shown.firstIndex(where: { $0.id == id }) else {
                return
            }
            let alert = shown.remove(at: index)
            alert.parent.endSheet(alert.panel)
            alert.panel.orderOut(nil)
            // Let go of the face, whose closures hold what presented it.
            alert.panel.contentView = nil
        }

        /// The panels showing now, the last over the others.
        public var panels: [NSPanel] { shown.map(\.panel) }
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
    }

    private struct WindowAnchorView: NSViewRepresentable {
        let anchor: WindowAnchor

        func makeNSView(context: Context) -> NSView {
            let view = NSView()
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
        let isPresented: Bool
        let build: () -> (MacAlertWords, [MacAlertButton])?
        let finish: @MainActor () -> Void

        @State
        private var id = UUID()
        @State
        private var anchor = WindowAnchor()

        func body(content: Content) -> some View {
            content
                .background(
                    WindowAnchorView(anchor: anchor).frame(width: 0, height: 0)
                )
                .onChange(of: isPresented, initial: true) { _, presented in
                    if presented {
                        AlertWindows.shared.present(
                            id: id,
                            over: anchor.view?.window,
                            build: build,
                            finish: finish
                        )
                    }
                    else {
                        AlertWindows.shared.withdraw(id: id)
                    }
                }
                .onDisappear { AlertWindows.shared.withdraw(id: id) }
        }
    }

    extension MacAlertView {
        /// The alert's face: elevated, 20 points rounded, with a shadow.
        public func elevatedFace() -> some View {
            background(Theme.surfaceElevated)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.panel))
                .shadow(color: Theme.shadow, radius: 15, y: 5)
        }
    }

    extension View {
        /// Shows bae's alert while `asker` has a question open.
        public func presentsAlerts(of asker: AlertAsker) -> some View {
            modifier(
                Presenting(
                    isPresented: asker.asked != nil,
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
                    isPresented: isPresented.wrappedValue,
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
                    isPresented: item.wrappedValue != nil,
                    build: { item.wrappedValue.map(alert) },
                    finish: { item.wrappedValue = nil }
                )
            )
        }
    }
#endif
