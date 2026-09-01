import SwiftUI

public struct SidePausePromptAlert: ViewModifier {
    @Environment(PlaybackStore.self)
    private var playbackStore

    #if os(iOS)
        @Environment(Playback.self)
        private var playback
    #endif

    public func body(content: Content) -> some View {
        #if os(macOS)
            content
                .disabled(playbackStore.presentedSidePausePrompt != nil)
                .overlay {
                    SidePausePromptAlertContent()
                }
        #else
            content.alert(
                item: Binding<BridgeSidePausePrompt?>(
                    get: { playbackStore.presentedSidePausePrompt },
                    set: { nextPrompt in
                        if nextPrompt == nil,
                            let prompt = playbackStore.presentedSidePausePrompt
                        {
                            playbackStore.dismissSidePausePrompt(prompt)
                        }
                    }
                )
            ) { prompt in
                Alert(
                    title: Text(prompt.title()),
                    message: Text(prompt.message()),
                    primaryButton: .default(Text("Play")) {
                        playbackStore.dismissSidePausePrompt(prompt)
                        playback.resume()
                    },
                    secondaryButton: .cancel(Text("Close")) {
                        playbackStore.dismissSidePausePrompt(prompt)
                    }
                )
            }
        #endif
    }
}

#if os(macOS)
    private struct SidePausePromptAlertContent: View {
        @Environment(Playback.self)
        private var playback
        @Environment(PlaybackStore.self)
        private var playbackStore
        @FocusState
        private var focused: Bool

        var body: some View {
            if let prompt = playbackStore.presentedSidePausePrompt {
                ZStack {
                    Color.black.opacity(0.3)
                        .ignoresSafeArea()
                        .onTapGesture {
                            playbackStore.dismissSidePausePrompt(prompt)
                        }

                    VStack(alignment: .leading, spacing: 14) {
                        Text(verbatim: prompt.title())
                            .font(.title2.weight(.semibold))
                        Text(verbatim: prompt.message())
                            .font(.body)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)

                        HStack(spacing: 10) {
                            Spacer()
                            Button("Close") {
                                playbackStore.dismissSidePausePrompt(prompt)
                            }
                            .buttonStyle(.bordered)
                            .keyboardShortcut(.cancelAction)

                            Button("Play") {
                                playbackStore.dismissSidePausePrompt(prompt)
                                playback.resume()
                            }
                            .buttonStyle(.borderedProminent)
                            .keyboardShortcut(.defaultAction)
                        }
                        .padding(.top, 8)
                    }
                    .padding(28)
                    .frame(width: 500, alignment: .leading)
                    .background(Theme.surfaceElevated)
                    .clipShape(RoundedRectangle(cornerRadius: 22))
                    .overlay {
                        RoundedRectangle(cornerRadius: 22)
                            .stroke(.white.opacity(0.12), lineWidth: 1)
                    }
                    .shadow(radius: 20)
                    .focusable()
                    .focusEffectDisabled()
                    .focused($focused)
                    .onKeyPress(.escape) {
                        playbackStore.dismissSidePausePrompt(prompt)
                        return .handled
                    }
                    .onAppear { focused = true }
                }
                .accessibilityAddTraits(.isModal)
            }
        }
    }
#else
    extension BridgeSidePausePrompt: Identifiable {}
#endif

extension View {
    public func sidePausePromptAlert() -> some View {
        modifier(SidePausePromptAlert())
    }
}
