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

#if DEBUG
    #Preview("Modal Overlay") {
        ModalOverlay(onDismiss: {}) {
            VStack(spacing: 12) {
                Text(verbatim: "Sample Modal")
                    .font(.headline)
                Text(
                    verbatim:
                        "Any content hosts inside the dimmed, dismissible overlay."
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                Button("Done") {}
                    .buttonStyle(.borderedProminent)
            }
            .padding(32)
            .frame(width: 320)
            .background(Theme.surface)
        }
        .frame(width: 620, height: 420)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
