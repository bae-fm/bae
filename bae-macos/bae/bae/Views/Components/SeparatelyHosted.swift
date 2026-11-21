import AppKit
import SwiftUI

/// Draws `content` in a `NSHostingView` of its own, which the window's hosting
/// view lists after the native views drawn before it, with the whole
/// environment of the place it sits in. The now playing bar hosts its right
/// controls this way, so VoiceOver reads them after the seek bar, which is a
/// native view.
struct SeparatelyHosted<Content: View>: NSViewRepresentable {
    @Environment(\.self)
    private var environment

    let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    func makeNSView(context _: Context) -> NSHostingView<AnyView> {
        let view = NSHostingView(rootView: shown)
        view.sizingOptions = []
        view.safeAreaRegions = []
        return view
    }

    func updateNSView(_ view: NSHostingView<AnyView>, context _: Context) {
        view.rootView = shown
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView view: NSHostingView<AnyView>,
        context _: Context
    ) -> CGSize? {
        view.sizeThatFits(proposal)
    }

    private var shown: AnyView {
        AnyView(content.transformEnvironment(\.self) { $0 = environment })
    }
}
