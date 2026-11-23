import BaeKit
import SwiftUI

extension EnvironmentValues {
    /// The size of the window a modal opens over, for the sheets that size
    /// themselves to it.
    @Entry
    var modalHost = CGSize.zero
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
