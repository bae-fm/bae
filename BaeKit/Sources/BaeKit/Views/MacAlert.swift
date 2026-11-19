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
                    .fontWeight(.bold)
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
                    MacCheckbox(isOn: suppression.isOn) { suppression.label }
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
        let run: () -> Void

        var body: some View {
            let label = button.title
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

    /// A borderless clear panel that takes the keyboard, so an alert's
    /// buttons answer Return and Escape.
    private final class AlertPanel: NSPanel {
        override var canBecomeKey: Bool { true }
    }

    /// A clear window over all of the window an alert sits on, which takes
    /// every click aimed at it and does nothing with them. bae fills the
    /// window with 1% black, because the window server passes clicks through
    /// a window that is fully clear.
    private final class AlertShield: NSWindow {
        override var canBecomeKey: Bool { false }
        override var canBecomeMain: Bool { false }

        init(over parent: NSWindow) {
            super
                .init(
                    contentRect: parent.frame,
                    styleMask: [.borderless],
                    backing: .buffered,
                    defer: false
                )
            isReleasedWhenClosed = false
            isOpaque = false
            backgroundColor = NSColor.black.withAlphaComponent(0.01)
            hasShadow = false
            animationBehavior = .none
            ignoresMouseEvents = false
            setAccessibilityElement(false)
        }

        override func sendEvent(_ event: NSEvent) {}
    }

    /// The tone, accent and light or dark scheme of the window an alert sits
    /// on. The panel is a separate window and inherits nothing from its
    /// parent, so the presenting view passes it the parent's look, and
    /// updates it when the parent's look changes.
    @MainActor
    @Observable
    private final class PanelLook {
        var appearance: HostedAppearance
        var scheme: ColorScheme

        init(appearance: HostedAppearance, scheme: ColorScheme) {
            self.appearance = appearance
            self.scheme = scheme
        }
    }

    /// The alert's face, 36 points below the top of the parent window, with
    /// room around it for its shadow, drawn in the parent's look.
    private struct AlertPanelContent: View {
        static let hangs: CGFloat = 36
        static let shadowRoom: CGFloat = 24

        let build: () -> (MacAlertWords, [MacAlertButton])?
        let finish: @MainActor () -> Void
        let look: PanelLook

        var body: some View {
            if let built = build() {
                MacAlert(built.0, buttons: built.1, finish: finish)
                    .elevatedFace()
                    .padding(.top, Self.hangs)
                    .padding([.horizontal, .bottom], Self.shadowRoom)
                    .hostedAppearance(look.appearance)
                    .environment(\.colorScheme, look.scheme)
            }
        }
    }

    /// Shows alerts. An alert is a clear panel holding bae's face, a child of
    /// the window of the view that raised it, or of the alert or sheet that is
    /// over that window. AlertWindows places the panel at the top centre of
    /// its parent. A shield window between them takes the pointer,
    /// AlertWindows makes the panel key again whenever something makes its
    /// parent key, and the panel is the only modal element of the
    /// accessibility tree, so the person cannot reach the window below with
    /// the pointer, the keyboard or VoiceOver while the alert is up.
    @MainActor
    public final class AlertWindows {
        public static let shared = AlertWindows()

        private struct Shown {
            let id: UUID
            let panel: NSPanel
            let shield: NSWindow
            let parent: NSWindow
            let observers: [NSObjectProtocol]
        }

        private var shown: [Shown] = []

        /// The window an alert raised now sits over: `window`, or when the
        /// view that raised it is in none yet the key or main one, under
        /// whatever sheets and alerts are open on it.
        private func target(over window: NSWindow?) -> NSWindow? {
            var window =
                window ?? NSApp.keyWindow ?? NSApp.mainWindow
                ?? NSApp.orderedWindows.first { $0.isVisible }
            while let current = window {
                if let sheet = current.attachedSheet {
                    window = sheet
                }
                else if let alert = shown.last(where: { $0.parent === current })
                {
                    window = alert.panel
                }
                else {
                    break
                }
            }
            return window
        }

        /// Whether an alert raised now over `window` has a window to sit on.
        func canPresent(over window: NSWindow?) -> Bool {
            target(over: window) != nil
        }

        /// Places `panel` at the top centre of `parent` and `shield` over it,
        /// and keeps them there as `parent` moves or resizes. When something
        /// makes the parent key, the panel makes itself key again. Returns the
        /// closure that positions the panel and the shield, and the
        /// observers.
        private static func keepWith(
            parent: NSWindow,
            panel: NSPanel,
            shield: NSWindow
        ) -> (@MainActor () -> Void, [NSObjectProtocol]) {
            let center = NotificationCenter.default
            let place: @MainActor () -> Void = {
                [weak panel, weak shield, weak parent] in
                guard let panel, let shield, let parent else { return }
                shield.setFrame(parent.frame, display: false)
                panel.setFrameOrigin(
                    NSPoint(
                        x: parent.frame.midX - panel.frame.width / 2,
                        y: parent.frame.maxY - panel.frame.height
                    )
                )
            }
            var observers: [NSObjectProtocol] = []
            for name in [
                NSWindow.didResizeNotification, NSWindow.didMoveNotification,
            ] {
                observers.append(
                    center.addObserver(
                        forName: name,
                        object: parent,
                        queue: .main
                    ) { _ in MainActor.assumeIsolated { place() } }
                )
            }
            observers.append(
                center.addObserver(
                    forName: NSWindow.didBecomeKeyNotification,
                    object: parent,
                    queue: .main
                ) { [weak panel] _ in
                    MainActor.assumeIsolated {
                        if let panel, panel.isVisible { panel.makeKey() }
                    }
                }
            )
            return (place, observers)
        }

        fileprivate func present(
            id: UUID,
            over window: NSWindow?,
            build: @escaping () -> (MacAlertWords, [MacAlertButton])?,
            look: PanelLook,
            finish: @escaping @MainActor () -> Void
        ) {
            withdraw(id: id)
            guard let parent = target(over: window) else {
                preconditionFailure("There is no window to show an alert over")
            }
            let host = NSHostingView(
                rootView: AlertPanelContent(
                    build: build,
                    finish: finish,
                    look: look
                )
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
            panel.appearance = parent.effectiveAppearance
            panel.contentView = host
            panel.setAccessibilityModal(true)
            host.setAccessibilityModal(true)
            let shield = AlertShield(over: parent)

            let (place, observers) = Self.keepWith(
                parent: parent,
                panel: panel,
                shield: shield
            )

            shown.append(
                Shown(
                    id: id,
                    panel: panel,
                    shield: shield,
                    parent: parent,
                    observers: observers
                )
            )
            parent.addChildWindow(shield, ordered: .above)
            parent.addChildWindow(panel, ordered: .above)
            place()
            panel.makeKeyAndOrderFront(nil)
        }

        /// Draws the alert `id` in the light or dark appearance its parent
        /// window has now, after the parent's appearance changed.
        func matchParentAppearance(id: UUID) {
            guard let alert = shown.first(where: { $0.id == id }) else {
                return
            }
            alert.panel.appearance = alert.parent.effectiveAppearance
        }

        func withdraw(id: UUID) {
            guard let index = shown.firstIndex(where: { $0.id == id }) else {
                return
            }
            let alert = shown.remove(at: index)
            alert.observers.forEach(NotificationCenter.default.removeObserver)
            let hadKey = alert.panel.isKeyWindow
            alert.parent.removeChildWindow(alert.panel)
            alert.parent.removeChildWindow(alert.shield)
            alert.panel.orderOut(nil)
            alert.shield.orderOut(nil)
            // Let go of the face, whose closures hold what presented it.
            alert.panel.contentView = nil
            // The keyboard goes back to the window the alert was over.
            if hadKey, alert.parent.isVisible {
                alert.parent.makeKey()
            }
        }

        /// The panels showing now, the last over the others.
        public var panels: [NSPanel] { shown.map(\.panel) }

        /// The windows that take the pointer for the panels showing now, in
        /// the same order.
        public var shields: [NSWindow] { shown.map(\.shield) }

        /// The window each of the panels showing now sits over.
        public func parent(of panel: NSPanel) -> NSWindow? {
            shown.first { $0.panel === panel }?.parent
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
        let isPresented: Bool
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
                .onChange(of: isPresented, initial: true) { _, presented in
                    if presented {
                        raise()
                    }
                    else {
                        anchor.arrived = nil
                        AlertWindows.shared.withdraw(id: id)
                    }
                }
                .onChange(of: hostedAppearance) { _, appearance in
                    look?.appearance = appearance
                }
                .onChange(of: scheme) { _, scheme in
                    look?.scheme = scheme
                    AlertWindows.shared.matchParentAppearance(id: id)
                }
                .onDisappear {
                    anchor.arrived = nil
                    AlertWindows.shared.withdraw(id: id)
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
            let anchor = anchor
            let id = id
            let build = build
            let finish = finish
            let look = PanelLook(appearance: hostedAppearance, scheme: scheme)
            self.look = look
            let show: @MainActor () -> Void = {
                AlertWindows.shared.present(
                    id: id,
                    over: anchor.view?.window,
                    build: build,
                    look: look,
                    finish: finish
                )
            }
            if AlertWindows.shared.canPresent(over: anchor.view?.window) {
                show()
            }
            else {
                anchor.arrived = show
            }
        }
    }

    extension MacAlert {
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
