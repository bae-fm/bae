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
    @MainActor
    struct LayerPlacement {
        weak var view: NSView?
        let cover: MacLayer.Cover

        /// The radius of the shape the layer covers: the window's corners, or
        /// the sheet face's.
        var cornerRadius: CGFloat {
            switch cover {
            case .window: windowRadius
            case .presentingView: ThemeRadius.panel
            }
        }

        /// The screen frame the layer covers, when `parent` is the window
        /// it sits over. A layer is withdrawn before the view that presents
        /// it leaves its window.
        func frame(over parent: NSWindow) -> NSRect {
            switch cover {
            case .window:
                return parent.frame
            case .presentingView:
                guard let view, let window = view.window else {
                    preconditionFailure(
                        "A layer is up over a view that is in no window"
                    )
                }
                return window.convertToScreen(
                    view.convert(view.bounds, to: nil)
                )
            }
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
        let modals: ModalWindows

        var body: some View {
            model.environment.install(on: model.content())
                // A layer or alert that this layer's content presents sits
                // on the same stack.
                .modalWindows(modals)
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
        /// Whether layers fade in and out. A host that captures a layer
        /// partway through its fade turns this off.
        public let fades: Bool

        public init(fades: Bool = true) {
            self.fades = fades
        }

        /// What an entry is.
        private enum Kind {
            /// An alert, over the window that dims and shields its parent,
            /// which replaces the alert of its replacement group.
            case alert(shield: ScrimWindow, replacementGroup: UUID?)
            /// A layer, whose panel draws `model` over what `cover` names.
            case layer(model: LayerModel, cover: MacLayer.Cover)
        }

        private struct Shown {
            let id: UUID
            let panel: NSPanel
            let parent: NSWindow
            let observers: [NSObjectProtocol]
            let kind: Kind

            /// The group whose alert this alert replaces; a layer is in none.
            var replacementGroup: UUID? {
                guard case .alert(_, let group) = kind else { return nil }
                return group
            }
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
            let observers =
                keepWith(parent: parent, panel: panel, place: place)
                + [dropWhenClosed(parent: parent, id: id)]

            shown.append(
                Shown(
                    id: id,
                    panel: panel,
                    parent: parent,
                    observers: observers,
                    kind: .alert(
                        shield: shield,
                        replacementGroup: replacementGroup
                    )
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
            let host = NSHostingView(
                rootView: LayerRoot(model: model, modals: self)
            )
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
            let observers =
                keepWith(parent: parent, panel: panel, place: place)
                + [dropWhenClosed(parent: parent, id: id)]
            shown.append(
                Shown(
                    id: id,
                    panel: panel,
                    parent: parent,
                    observers: observers,
                    kind: .layer(model: model, cover: placement.cover)
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

        /// Withdraws the entry `id` when its parent window closes, since a
        /// closed window gives the keyboard to nothing and an entry over it
        /// would stay in the stack for good.
        private func dropWhenClosed(
            parent: NSWindow,
            id: UUID
        ) -> NSObjectProtocol {
            NotificationCenter.default.addObserver(
                forName: NSWindow.willCloseNotification,
                object: parent,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    _ = self?.withdraw(id: id, returningKey: false)
                }
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
                preconditionFailure("The layer \(id) is not showing")
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
        /// keyboard back to the window under it unless `returningKey` is
        /// false, as when that window is closing. Returns whether the
        /// keyboard went back.
        @discardableResult
        func withdraw(id: UUID, returningKey: Bool = true) -> Bool {
            guard let index = shown.firstIndex(where: { $0.id == id }) else {
                return false
            }
            var hadKey = false
            let panel = shown[index].panel
            for above in shown.reversed() where above.parent === panel {
                if withdraw(id: above.id, returningKey: returningKey) {
                    hadKey = true
                }
            }
            guard let index = shown.firstIndex(where: { $0.id == id }) else {
                return hadKey
            }
            let entry = shown.remove(at: index)
            entry.observers.forEach(NotificationCenter.default.removeObserver)
            hadKey = hadKey || entry.panel.isKeyWindow
            if case .alert(let shield, _) = entry.kind { shield.hide() }
            if case .layer(let model, _) = entry.kind, model.fades, fades,
                model.shown
            {
                // Fade out, then take the panel away: it takes no click or key
                // meanwhile.
                entry.panel.ignoresMouseEvents = true
                let parent = entry.parent
                withAnimation(.easeInOut(duration: 0.2)) {
                    model.shown = false
                } completion: {
                    MainActor.assumeIsolated {
                        parent.removeChildWindow(panel)
                        panel.orderOut(nil)
                        panel.contentView = nil
                    }
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
            if returningKey, hadKey, entry.parent.isVisible {
                entry.parent.makeKey()
                return true
            }
            return hadKey
        }
    }

    // MARK: - What the stack shows

    extension ModalWindows {
        /// The panels showing now, the last over the others.
        public var panels: [NSPanel] { shown.map(\.panel) }

        /// The panels of layers showing now, in the same order.
        public var layerPanels: [NSPanel] {
            shown.compactMap { entry in
                guard case .layer = entry.kind else { return nil }
                return entry.panel
            }
        }

        /// The windows that take the pointer for the alerts showing now, in
        /// the same order.
        public var shields: [NSWindow] {
            shown.compactMap { entry in
                guard case .alert(let shield, _) = entry.kind else {
                    return nil
                }
                return shield
            }
        }

        /// What the layer whose panel is `panel` covers: the whole window, or
        /// only the frame of the view that presents it, as the lightbox over
        /// the gallery of the Change cover sheet does.
        public func cover(ofLayer panel: NSPanel) -> MacLayer.Cover {
            guard
                let entry = shown.first(where: { $0.panel === panel }),
                case .layer(_, let cover) = entry.kind
            else {
                preconditionFailure("The panel is no layer this stack shows")
            }
            return cover
        }

        /// The window each of the panels showing now sits over.
        public func parent(of panel: NSPanel) -> NSWindow? {
            shown.first { $0.panel === panel }?.parent
        }
    }
#endif
