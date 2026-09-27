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

#if DEBUG
    #Preview("Modal Overlay") {
        ModalOverlay(onDismiss: {}) {
            VStack(spacing: ThemeSpace.group) {
                Text(verbatim: "Sample Modal")
                    .themeText(.heading)
                Text(
                    verbatim:
                        "Any content hosts inside the dimmed, dismissible overlay."
                )
                .themeText(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                Button("Done") {}
                    .buttonStyle(PrimaryButtonStyle())
            }
            .padding(ThemeSpace.section)
            .frame(width: 320)
            .background(Theme.surface)
        }
        .frame(width: 620, height: 420)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
