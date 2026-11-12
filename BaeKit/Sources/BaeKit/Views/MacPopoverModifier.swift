#if canImport(AppKit)
    import SwiftUI

    /// What presents a bae popover from a view while `isPresented`: the
    /// stand-in for SwiftUI's `popover`, which macOS draws in a system
    /// colour. The content keeps the environment it would have had in a
    /// system popover.
    private struct MacPopover<Popover: View>: ViewModifier {
        @Binding
        var isPresented: Bool
        let arrowEdge: Edge
        let takesKeyboard: Bool
        let popover: () -> Popover

        @Environment(\.self)
        private var environment
        @State
        private var anchor = MacMenuAnchor()
        @State
        private var session: MacPopoverSession?

        func body(content: Content) -> some View {
            content
                .background(MacMenuAnchorView(anchor: anchor))
                .background(refreshed())
                .onChange(of: isPresented, initial: true) { _, presented in
                    if presented {
                        present()
                    }
                    else {
                        session?.close()
                    }
                }
                .onDisappear { session?.close() }
        }

        /// Hands an open popover the content for the state this view has now.
        private func refreshed() -> some View {
            session?.update(shown())
            return Color.clear
        }

        private func shown() -> AnyView {
            AnyView(
                popover().transformEnvironment(\.self) { $0 = environment }
            )
        }

        private func present() {
            guard session == nil else { return }
            guard let window = anchor.window else {
                isPresented = false
                return
            }
            session = MacPopoverSession(
                content: shown(),
                over: window,
                anchor: anchor,
                edge: arrowEdge,
                takesKeyboard: takesKeyboard
            ) {
                session = nil
                isPresented = false
            }
        }
    }

    extension View {
        /// bae's popover, in bae's colours with macOS's shape and arrow, open
        /// on `arrowEdge` of this view while `isPresented`. A popover that
        /// opens on hover leaves the keyboard where it is, with
        /// `takesKeyboard` false.
        public func macPopover<Content: View>(
            isPresented: Binding<Bool>,
            arrowEdge: Edge = .bottom,
            takesKeyboard: Bool = true,
            @ViewBuilder content: @escaping () -> Content
        ) -> some View {
            modifier(
                MacPopover(
                    isPresented: isPresented,
                    arrowEdge: arrowEdge,
                    takesKeyboard: takesKeyboard,
                    popover: content
                )
            )
        }
    }
#endif
