import BaeKit
import SwiftUI

struct ModalOverlay<Content: View>: View {
    let onDismiss: () -> Void
    @ViewBuilder
    let content: () -> Content
    @FocusState
    private var focused: Bool

    var body: some View {
        ZStack {
            Theme.scrim
                .ignoresSafeArea()
                .onTapGesture { onDismiss() }
            content()
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
                .shadow(radius: 20)
                .focusable()
                .focusEffectDisabled()
                .focused($focused)
                .onKeyPress(.escape) {
                    onDismiss()
                    return .handled
                }
                .onAppear { focused = true }
        }
    }
}
