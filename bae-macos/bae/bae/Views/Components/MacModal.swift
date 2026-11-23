import BaeKit
import SwiftUI

extension EnvironmentValues {
    /// The size of the window a modal opens over, for the sheets that size
    /// themselves to it; nil outside a `MacModal`.
    @Entry
    var modalHost: CGSize?
}

/// The window a modal opens over, for the sheet `sheet` that sizes itself to
/// it. A sheet shown outside a `MacModal` is a bug: it is reported, and the
/// sheet draws at no size.
func modalHostSize(_ host: CGSize?, for sheet: String) -> CGSize {
    if let host {
        return host
    }
    let detail = "\(sheet) is shown outside a MacModal"
    baeAppLogger.error("\(detail)")
    CoreBugReporter().report(operation: "size a modal sheet", detail: detail)
    return .zero
}

/// A scrim over the window with the content in the sheet face.
struct MacModal<Content: View>: View {
    let onDismiss: () -> Void
    @ViewBuilder
    let content: () -> Content
    @FocusState
    private var focused: Bool

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Theme.scrim
                    .ignoresSafeArea()
                    .onTapGesture { onDismiss() }
                content()
                    .environment(\.modalHost, geometry.size)
                    .sheetFace()
                    .focusable()
                    .focusEffectDisabled()
                    .focused($focused)
                    .onKeyPress(.escape) {
                        onDismiss()
                        return .handled
                    }
                    .onAppear { focused = true }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
        }
    }
}
