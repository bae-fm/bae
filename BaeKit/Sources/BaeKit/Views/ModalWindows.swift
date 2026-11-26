#if canImport(AppKit)
    import AppKit
    import SwiftUI

    /// A borderless clear panel that takes the keyboard, so an alert's
    /// buttons answer Return and Escape. It does not activate the app: an
    /// alert only sits over a window of the app, and a panel that activates
    /// can be key only while the app is the active one.
    final class AlertPanel: NSPanel {
        override var canBecomeKey: Bool { true }
    }

    /// The tone, accent and light or dark scheme of the window an alert sits
    /// on. The panel is a separate window and inherits nothing from its
    /// parent, so the presenting view passes it the parent's look, and
    /// updates it when the parent's look changes.
    @MainActor
    @Observable
    final class PanelLook {
        var appearance: HostedAppearance
        var scheme: ColorScheme

        init(appearance: HostedAppearance, scheme: ColorScheme) {
            self.appearance = appearance
            self.scheme = scheme
        }
    }

    /// Where a layer sits: over the whole window it is presented in, or over
    /// the frame of the view that presents it.
    struct LayerPlacement {
        weak var view: NSView?
        let wholeWindow: Bool

        /// The radius of the shape the layer covers: the window's corners, or
        /// the sheet face's.
        var cornerRadius: CGFloat {
            wholeWindow ? windowRadius : ThemeRadius.panel
        }

        /// The screen frame the layer covers, when `parent` is the window
        /// it sits over.
        func frame(over parent: NSWindow) -> NSRect {
            guard !wholeWindow, let view, let window = view.window else {
                return parent.frame
            }
            return window.convertToScreen(view.convert(view.bounds, to: nil))
        }
    }

    /// What a layer's panel draws: the layer's content with the values that
    /// the window's host installs (`layerEnvironment`), in the look of the
    /// window the layer sits on. The presenting view updates both while the
    /// layer is up.
    @MainActor
    @Observable
    final class LayerModel {
        var content: () -> AnyView
        var environment: LayerEnvironment
        let look: PanelLook
        /// Whether the layer fades in and out.
        let fades: Bool
        /// Whether the layer is faded in; a layer that fades starts false.
        var shown: Bool

        init(
            content: @escaping () -> AnyView,
            environment: LayerEnvironment,
            look: PanelLook,
            fades: Bool,
            shown: Bool
        ) {
            self.content = content
            self.environment = environment
            self.look = look
            self.fades = fades
            self.shown = shown
        }
    }

    private struct LayerRoot: View {
        let model: LayerModel

        var body: some View {
            model.environment.install(on: model.content())
                // A layer that this layer's content presents takes the same
                // values.
                .environment(\.layerEnvironment, model.environment)
                .hostedAppearance(model.look.appearance)
                .environment(\.colorScheme, model.look.scheme)
                .opacity(model.shown ? 1 : 0)
        }
    }

    /// A borderless clear panel over the view a layer covers. It takes the
    /// keyboard and, as the window it covers does, a click on it from another
    /// app activates bae. It stays up while bae is in the background.
    private final class LayerPanel: NSPanel {
        override var canBecomeKey: Bool { true }

        override func sendEvent(_ event: NSEvent) {
            switch event.type {
            case .leftMouseDown, .rightMouseDown, .otherMouseDown:
                if !NSApp.isActive {
                    NSApp.activate(ignoringOtherApps: true)
                }
            default:
                break
            }
            super.sendEvent(event)
        }
    }

    /// Keeps `panel` where `place` puts it as `parent` moves or resizes.
    /// When something makes the parent key, the panel makes itself key
    /// again. Returns the observers.
    @MainActor
    private func keepWith(
        parent: NSWindow,
        panel: NSPanel,
        place: @escaping @MainActor () -> Void
    ) -> [NSObjectProtocol] {
        let center = NotificationCenter.default
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
        return observers
    }

    /// Shows what modal presentations of bae's own draw as windows over a
    /// window: alerts and layers. Each entry is a child panel of the window
    /// under it, which is the top entry over the presenting window, or the
    /// sheet attached to it. The top panel is key and the only modal element
    /// of the accessibility tree, so the person cannot reach the window below
    /// with the pointer, the keyboard or VoiceOver while it is up.
    ///
    /// - An alert's panel sits at the top centre of its parent, over a scrim
    ///   window that dims the whole parent, title bar included, and takes the
    ///   pointer.
    /// - A layer's panel covers the frame of the view that presents it,
    ///   follows that frame, and draws the layer's whole content, scrim
    ///   included.
    ///
    /// ModalWindows makes a panel key again whenever something makes its
    /// parent key. Withdrawing an entry first withdraws every entry above it.
    @MainActor
    public final class ModalWindows {
        public static let shared = ModalWindows()

        /// Whether layers fade in and out.
        public var fades = true

        private struct Shown {
            let id: UUID
            let replacementGroup: UUID?
            let panel: NSPanel
            /// The window that dims and shields the parent, for an alert.
            let shield: ScrimWindow?
            let parent: NSWindow
            let observers: [NSObjectProtocol]
            /// For a layer, what its panel draws.
            let model: LayerModel?
            /// Whether the entry covers only the frame of the view that
            /// presents it, not the whole window.
            var coversPartOfWindow = false
        }

        private var shown: [Shown] = []

        /// The window an entry raised now sits over: `window`, or when the
        /// view that raised it is in none yet the key or main one, under
        /// whatever sheets and entries are open on it.
        private func target(over window: NSWindow?) -> NSWindow? {
            var window =
                window ?? NSApp.keyWindow ?? NSApp.mainWindow
                ?? NSApp.orderedWindows.first { $0.isVisible }
            while let current = window {
                if let sheet = current.attachedSheet {
                    window = sheet
                }
                else if let above = shown.last(where: { $0.parent === current })
                {
                    window = above.panel
                }
                else {
                    break
                }
            }
            return window
        }

        /// Whether an entry raised now over `window` has a window to sit on.
        func canPresent(over window: NSWindow?) -> Bool {
            target(over: window) != nil
        }

        /// Makes `panel` a clear, modal panel that shows `host` and draws like
        /// `parent`.
        private static func configure(
            _ panel: NSPanel,
            showing host: NSView,
            over parent: NSWindow
        ) {
            panel.isReleasedWhenClosed = false
            panel.isOpaque = false
            panel.backgroundColor = .clear
            panel.hasShadow = false
            panel.animationBehavior = .none
            panel.appearance = parent.effectiveAppearance
            panel.contentView = host
        }

        func present(
            id: UUID,
            over window: NSWindow?,
            replacementGroup: UUID?,
            content: AlertPanelContent
        ) {
            withdraw(id: id)
            if let replacementGroup,
                let previous = shown.first(where: {
                    $0.replacementGroup == replacementGroup
                })
            {
                withdraw(id: previous.id)
            }
            guard let parent = target(over: window) else {
                preconditionFailure("There is no window to show an alert over")
            }
            let host = NSHostingView(rootView: content)
            let panel = AlertPanel(
                contentRect: NSRect(origin: .zero, size: host.fittingSize),
                styleMask: [.borderless, .nonactivatingPanel],
                backing: .buffered,
                defer: false
            )
            Self.configure(panel, showing: host, over: parent)
            let shield = ScrimWindow(over: parent)

            let place: @MainActor () -> Void = {
                [weak panel, weak parent] in
                guard let panel, let parent else { return }
                panel.setFrameOrigin(
                    NSPoint(
                        x: parent.frame.midX - panel.frame.width / 2,
                        y: parent.frame.maxY - panel.frame.height
                    )
                )
            }
            let observers = keepWith(
                parent: parent,
                panel: panel,
                place: place
            )

            shown.append(
                Shown(
                    id: id,
                    replacementGroup: replacementGroup,
                    panel: panel,
                    shield: shield,
                    parent: parent,
                    observers: observers,
                    model: nil
                )
            )
            shield.show()
            parent.addChildWindow(panel, ordered: .above)
            markTopModal()
            place()
            panel.makeKeyAndOrderFront(nil)
        }

        /// Shows a layer over `window`: a panel over the frame `placement`
        /// names, drawing `model`.
        func presentLayer(
            id: UUID,
            over window: NSWindow?,
            placement: LayerPlacement,
            model: LayerModel
        ) {
            withdraw(id: id)
            guard let parent = target(over: window) else {
                preconditionFailure("There is no window to show a layer over")
            }
            let host = NSHostingView(rootView: LayerRoot(model: model))
            host.sizingOptions = []
            host.wantsLayer = true
            host.layer?.cornerRadius = placement.cornerRadius
            host.layer?.cornerCurve = .continuous
            host.layer?.masksToBounds = true
            let panel = LayerPanel(
                contentRect: placement.frame(over: parent),
                styleMask: [.borderless, .nonactivatingPanel],
                backing: .buffered,
                defer: false
            )
            // A layer stays up while bae is in the background.
            panel.hidesOnDeactivate = false
            Self.configure(panel, showing: host, over: parent)

            let place: @MainActor () -> Void = {
                [weak panel, weak parent] in
                guard let panel, let parent else { return }
                panel.setFrame(placement.frame(over: parent), display: true)
            }
            let observers = keepWith(
                parent: parent,
                panel: panel,
                place: place
            )
            shown.append(
                Shown(
                    id: id,
                    replacementGroup: nil,
                    panel: panel,
                    shield: nil,
                    parent: parent,
                    observers: observers,
                    model: model,
                    coversPartOfWindow: !placement.wholeWindow
                )
            )
            parent.addChildWindow(panel, ordered: .above)
            markTopModal()
            place()
            panel.makeKeyAndOrderFront(nil)
            if !model.shown {
                withAnimation(.easeInOut(duration: 0.2)) { model.shown = true }
            }
        }

        /// Makes the top entry's panel the one modal element of the
        /// accessibility tree: a panel under it is not modal, since a second
        /// modal element hides the one under it from assistive clients.
        private func markTopModal() {
            for (index, entry) in shown.enumerated() {
                let isTop = index == shown.count - 1
                entry.panel.setAccessibilityModal(isTop)
                entry.panel.contentView?.setAccessibilityModal(isTop)
            }
        }

        /// Whether the entry `id` is showing.
        func isShowing(id: UUID) -> Bool {
            shown.contains { $0.id == id }
        }

        /// Puts the layer `id` over what it covers now, in the light or dark
        /// appearance its parent has now, after either changed.
        func refreshLayer(id: UUID, placement: LayerPlacement) {
            guard let layer = shown.first(where: { $0.id == id }) else {
                return
            }
            layer.panel.appearance = layer.parent.effectiveAppearance
            layer.panel.setFrame(
                placement.frame(over: layer.parent),
                display: true
            )
        }

        /// Draws the alert `id` in the light or dark appearance its parent
        /// window has now, after the parent's appearance changed.
        func matchParentAppearance(id: UUID) {
            guard let alert = shown.first(where: { $0.id == id }) else {
                return
            }
            alert.panel.appearance = alert.parent.effectiveAppearance
        }

        /// Withdraws the entry `id` after every entry above it, and gives the
        /// keyboard back to the window under it. Returns whether the keyboard
        /// went back.
        @discardableResult
        func withdraw(id: UUID) -> Bool {
            guard let index = shown.firstIndex(where: { $0.id == id }) else {
                return false
            }
            var hadKey = false
            let panel = shown[index].panel
            for above in shown.reversed() where above.parent === panel {
                if withdraw(id: above.id) { hadKey = true }
            }
            guard let index = shown.firstIndex(where: { $0.id == id }) else {
                return hadKey
            }
            let entry = shown.remove(at: index)
            entry.observers.forEach(NotificationCenter.default.removeObserver)
            hadKey = hadKey || entry.panel.isKeyWindow
            entry.shield?.hide()
            if let model = entry.model, model.fades, fades, model.shown {
                // Fade out, then take the panel away: it takes no click or key
                // meanwhile.
                entry.panel.ignoresMouseEvents = true
                withAnimation(.easeInOut(duration: 0.2)) { model.shown = false }
                let parent = entry.parent
                Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(220))
                    parent.removeChildWindow(panel)
                    panel.orderOut(nil)
                    panel.contentView = nil
                }
            }
            else {
                entry.parent.removeChildWindow(entry.panel)
                entry.panel.orderOut(nil)
                // Let go of the face, whose closures hold what presented it.
                entry.panel.contentView = nil
            }
            markTopModal()
            // The keyboard goes back to the window the entry was over.
            if hadKey, entry.parent.isVisible {
                entry.parent.makeKey()
                return true
            }
            return hadKey
        }

        /// The panels showing now, the last over the others.
        public var panels: [NSPanel] { shown.map(\.panel) }

        /// The panels of layers showing now, in the same order.
        public var layerPanels: [NSPanel] {
            shown.filter { $0.model != nil }.map(\.panel)
        }

        /// The windows that take the pointer for the alerts showing now, in
        /// the same order.
        public var shields: [NSWindow] { shown.compactMap(\.shield) }

        /// Whether `panel` is a layer that covers only the frame of the view
        /// that presents it, such as the lightbox over the gallery of the
        /// Change cover sheet.
        public func coversPartOfWindow(_ panel: NSPanel) -> Bool {
            shown.first { $0.panel === panel }?.coversPartOfWindow ?? false
        }

        /// The window each of the panels showing now sits over.
        public func parent(of panel: NSPanel) -> NSWindow? {
            shown.first { $0.panel === panel }?.parent
        }
    }
#endif
