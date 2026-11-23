import BaeKit
import SwiftUI

/// A scrim over the window with the content in the sheet face. A sheet that
/// sizes itself to the window takes the window's size from `sizedContent`.
struct MacModal<Content: View>: View {
    let onDismiss: () -> Void
    private let content: (CGSize) -> Content
    @FocusState
    private var focused: Bool

    init(
        onDismiss: @escaping () -> Void,
        @ViewBuilder sizedContent: @escaping (CGSize) -> Content
    ) {
        self.onDismiss = onDismiss
        content = sizedContent
    }

    init(
        onDismiss: @escaping () -> Void,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.init(onDismiss: onDismiss) { _ in content() }
    }

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                MacScrim()
                    .onTapGesture { onDismiss() }
                content(geometry.size)
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
