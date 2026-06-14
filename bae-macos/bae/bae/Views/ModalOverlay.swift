import SwiftUI

struct ModalOverlay<Content: View>: View {
    let onDismiss: () -> Void
    @ViewBuilder
    let content: () -> Content
    @FocusState
    private var focused: Bool

    var body: some View {
        ZStack {
            Color.black.opacity(0.3)
                .ignoresSafeArea()
                .onTapGesture { onDismiss() }
            content()
                .clipShape(RoundedRectangle(cornerRadius: 10))
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
