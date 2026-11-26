#if canImport(AppKit)
    import AppKit
    import SwiftUI

    /// A modal presentation that bae draws itself over a view: a sheet, the
    /// document viewer or a lightbox. It is a window over the presenting view
    /// (`ModalWindows`), so Tab, VoiceOver and the pointer reach only what it
    /// holds.
    public struct MacLayer {
        /// What a layer covers.
        public enum Cover {
            /// The whole window of the presenting view, title bar included.
            case window
            /// The frame of the presenting view.
            case presentingView
        }

        let id: AnyHashable
        let cover: Cover
        let fades: Bool
        let content: () -> AnyView

        /// A layer named `id`, which stays the same layer while the id stays.
        public init<ID: Hashable, Content: View>(
            id: ID,
            cover: Cover = .window,
            fades: Bool = false,
            @ViewBuilder content: @escaping () -> Content
        ) {
            self.id = AnyHashable(id)
            self.cover = cover
            self.fades = fades
            self.content = { AnyView(content()) }
        }
    }

    /// The values that a layer's content draws with, which whoever hosts the
    /// window installs: the app's stores, or a test's or shot's own. A
    /// layer's panel is a window of its own, so its content inherits nothing
    /// from the view that presents it.
    struct LayerEnvironment {
        /// Each host's installation, the outermost host's first.
        fileprivate var installations: [@MainActor (AnyView) -> AnyView] = []

        /// `content` with every host's values, the nearest host's winning.
        @MainActor
        func install(on content: AnyView) -> AnyView {
            installations.reversed().reduce(content) { $1($0) }
        }
    }

    extension EnvironmentValues {
        @Entry
        var layerEnvironment = LayerEnvironment()
    }

    extension View {
        /// Shows `layers` over this view, the first of the ones that are new
        /// first, each over the one before. A layer is withdrawn when it
        /// leaves `layers` or this view disappears. Each draws with the values
        /// the window's host installs (`layerEnvironment(_:)`), in this view's
        /// tone, accent and colour scheme, which it follows while it is up.
        public func macLayers(_ layers: [MacLayer]) -> some View {
            background(LayerAnchor(layers: layers))
        }

        /// This view with what `install` sets, which every layer that this
        /// view's content presents draws with too.
        public func layerEnvironment<Installed: View>(
            _ install: @escaping @MainActor (AnyView) -> Installed
        ) -> some View {
            install(
                AnyView(
                    transformEnvironment(\.layerEnvironment) {
                        $0.installations.append { AnyView(install($0)) }
                    }
                )
            )
        }
    }

    /// The view the layers are presented from, which knows its window and
    /// frame.
    private final class LayerAnchorView: NSView {
        weak var coordinator: LayerCoordinator?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard window != nil else { return }
            MainActor.assumeIsolated { coordinator?.schedule() }
        }
    }

    @MainActor
    private final class LayerCoordinator {
        private struct Live {
            let id: UUID
            let model: LayerModel
        }

        weak var view: NSView?
        /// What the presenting view asked for last: its layers, the values
        /// they draw with and the look of the window they sit on.
        private struct Wanted {
            let layers: [MacLayer]
            let environment: LayerEnvironment
            let appearance: HostedAppearance
            let scheme: ColorScheme
        }

        private var latest: Wanted?
        private var live: [AnyHashable: Live] = [:]
        private var order: [AnyHashable] = []
        private var scheduled = false
        private var gone = false

        func update(
            layers: [MacLayer],
            environment: LayerEnvironment,
            appearance: HostedAppearance,
            scheme: ColorScheme
        ) {
            latest = Wanted(
                layers: layers,
                environment: environment,
                appearance: appearance,
                scheme: scheme
            )
            schedule()
        }

        /// Shows what `latest` lists after the view update that asked.
        func schedule() {
            guard !scheduled else { return }
            scheduled = true
            Task { @MainActor [weak self] in
                self?.scheduled = false
                self?.flush()
            }
        }

        private func flush() {
            guard !gone, let latest, let view, view.window != nil else {
                return
            }
            let modals = ModalWindows.shared
            let wanted = Set(latest.layers.map(\.id))
            for key in order.reversed() where !wanted.contains(key) {
                if let layer = live.removeValue(forKey: key) {
                    modals.withdraw(id: layer.id)
                }
            }
            order.removeAll { !wanted.contains($0) }
            for layer in latest.layers {
                let placement = LayerPlacement(
                    view: view,
                    wholeWindow: layer.cover == .window
                )
                if let shown = live[layer.id], modals.isShowing(id: shown.id) {
                    shown.model.content = layer.content
                    shown.model.environment = latest.environment
                    shown.model.look.appearance = latest.appearance
                    shown.model.look.scheme = latest.scheme
                    modals.refreshLayer(id: shown.id, placement: placement)
                    continue
                }
                let model = LayerModel(
                    content: layer.content,
                    environment: latest.environment,
                    look: PanelLook(
                        appearance: latest.appearance,
                        scheme: latest.scheme
                    ),
                    fades: layer.fades,
                    shown: !(layer.fades && modals.fades)
                )
                let id = live[layer.id]?.id ?? UUID()
                live[layer.id] = Live(id: id, model: model)
                if !order.contains(layer.id) { order.append(layer.id) }
                modals.presentLayer(
                    id: id,
                    over: view.window,
                    placement: placement,
                    model: model
                )
            }
        }

        func withdrawAll() {
            gone = true
            for key in order.reversed() {
                if let layer = live[key] {
                    ModalWindows.shared.withdraw(id: layer.id)
                }
            }
            live = [:]
            order = []
        }
    }

    private struct LayerAnchor: NSViewRepresentable {
        let layers: [MacLayer]
        @Environment(\.layerEnvironment)
        private var environment
        @Environment(\.surfaceTone)
        private var tone
        @Environment(\.accentChoice)
        private var accent
        @Environment(\.colorScheme)
        private var scheme

        func makeCoordinator() -> LayerCoordinator { LayerCoordinator() }

        func makeNSView(context: Context) -> NSView {
            let view = LayerAnchorView()
            view.coordinator = context.coordinator
            context.coordinator.view = view
            // Out of the key-view loop and the view tree's drawing: it only
            // knows its window and frame.
            view.isHidden = true
            return view
        }

        func updateNSView(_ view: NSView, context: Context) {
            context.coordinator.view = view
            context.coordinator.update(
                layers: layers,
                environment: environment,
                appearance: HostedAppearance(tone: tone, accent: accent),
                scheme: scheme
            )
        }

        static func dismantleNSView(
            _ view: NSView,
            coordinator: LayerCoordinator
        ) {
            MainActor.assumeIsolated { coordinator.withdrawAll() }
        }
    }
#endif
