import SwiftUI

struct SidePausePromptAlert: ViewModifier {
    @Environment(PlaybackStore.self)
    private var playbackStore

    @State
    private var dismissedPromptId: String?

    private var prompt: BridgeSidePausePrompt? {
        guard case .paused(_, let reason) = playbackStore.nowPlaying,
            let prompt = reason.sidePausePrompt,
            dismissedPromptId != prompt.id
        else {
            return nil
        }
        return prompt
    }

    func body(content: Content) -> some View {
        content
            .alert(
                item: Binding<BridgeSidePausePrompt?>(
                    get: { prompt },
                    set: { newPrompt in
                        if newPrompt == nil {
                            dismissedPromptId = prompt?.id
                        }
                    }
                )
            ) { prompt in
                Alert(
                    title: Text(prompt.title()),
                    message: Text(prompt.message()),
                    dismissButton: .default(Text("Close")) {
                        dismissedPromptId = prompt.id
                    }
                )
            }
    }
}

extension View {
    func sidePausePromptAlert() -> some View {
        modifier(SidePausePromptAlert())
    }
}
