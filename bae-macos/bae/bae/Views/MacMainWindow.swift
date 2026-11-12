import SwiftUI

/// How the queue panel springs in beside the active section and out again.
enum QueuePanelMotion {
    static let spring = Animation.spring(duration: 0.24, bounce: 0.12)
}

/// The main window's layout: the title bar, the active section beside the
/// docked queue, and the now playing bar along the bottom. The caller supplies
/// each part; `MacMainWindowContainer` reads the stores for them.
struct MacMainWindow<
    TitleBar: View,
    Content: View,
    QueuePanel: View,
    NowPlayingBar: View
>: View {
    /// Whether the queue panel is docked beside the content.
    let showQueue: Bool
    @ViewBuilder
    let titleBar: () -> TitleBar
    @ViewBuilder
    let content: () -> Content
    @ViewBuilder
    let queuePanel: () -> QueuePanel
    @ViewBuilder
    let nowPlayingBar: () -> NowPlayingBar

    var body: some View {
        VStack(spacing: 0) {
            titleBar()

            HStack(spacing: 0) {
                content()
                    .frame(maxWidth: .infinity)

                // Docked so the content reflows beside it during drags.
                if showQueue {
                    queuePanel()
                        .transition(.move(edge: .trailing))
                }
            }
            .animation(QueuePanelMotion.spring, value: showQueue)

            Divider()
            nowPlayingBar()
        }
    }
}
