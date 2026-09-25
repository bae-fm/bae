import SwiftUI

/// Presents the prompt core raises when playback pauses at the end of a side or
/// disc. A card, not a system alert: it carries the pause-between-sides
/// checkbox, which an alert cannot hold. macOS draws it over the window; iOS
/// presents it full screen over a clear background.
public struct SidePausePromptAlert: ViewModifier {
    @Environment(PlaybackStore.self)
    private var playbackStore

    /// Where a failed pause-between-sides write is shown. Takes the error, not a
    /// rendered line, like `PauseBetweenSidesToggle`.
    let showError: @MainActor (any Error) -> Void

    public func body(content: Content) -> some View {
        #if os(macOS)
            content
                .disabled(playbackStore.presentedSidePausePrompt != nil)
                .overlay {
                    if let prompt = playbackStore.presentedSidePausePrompt {
                        SidePausePromptCard(
                            prompt: prompt,
                            showError: showError
                        )
                        .id(prompt.id)
                    }
                }
        #else
            content
                .fullScreenCover(
                    item: Binding<BridgeSidePausePrompt?>(
                        get: { playbackStore.presentedSidePausePrompt },
                        set: { nextPrompt in
                            if nextPrompt == nil,
                                let prompt = playbackStore
                                    .presentedSidePausePrompt
                            {
                                playbackStore.dismissSidePausePrompt(prompt)
                            }
                        }
                    )
                ) { prompt in
                    SidePausePromptCard(prompt: prompt, showError: showError)
                        .presentationBackground(.clear)
                }
                // Appear and leave in place like an alert, instead of the
                // cover's slide from the bottom edge.
                .transaction(
                    value: playbackStore.presentedSidePausePrompt?.id,
                    Self.disableAnimations
                )
        #endif
    }

    #if os(iOS)
        private static func disableAnimations(
            _ transaction: inout Transaction
        ) {
            transaction.disablesAnimations = true
        }
    #endif
}

private struct SidePausePromptCard: View {
    let prompt: BridgeSidePausePrompt
    let showError: @MainActor (any Error) -> Void

    @Environment(Playback.self)
    private var playback
    @Environment(PlaybackStore.self)
    private var playbackStore

    /// The checkbox. The prompt only appears while the setting is on, so it
    /// starts checked; nothing is written until the prompt is answered.
    @State
    private var keepPausing = true

    #if os(macOS)
        @FocusState
        private var focused: Bool
    #endif

    var body: some View {
        ZStack {
            Color.black.opacity(0.3)
                .ignoresSafeArea()
                .onTapGesture { answer(play: false) }

            card
        }
        .accessibilityAddTraits(.isModal)
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(verbatim: prompt.title())
                .font(.title2.weight(.semibold))
            Text(verbatim: localizedCoreString("core.playback.pause.message"))
                .font(.body)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            Toggle("Pause between sides and discs", isOn: $keepPausing)
                #if os(macOS)
                    .toggleStyle(.checkbox)
                #endif

            HStack(spacing: 10) {
                Spacer()
                Button("Close") { answer(play: false) }
                    .buttonStyle(.bordered)
                    .keyboardShortcut(.cancelAction)

                Button("Play") { answer(play: true) }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.top, 8)
        }
        .padding(28)
        #if os(macOS)
            .frame(width: 500, alignment: .leading)
        #else
            .frame(maxWidth: 500, alignment: .leading)
        #endif
        .background(Theme.surfaceElevated)
        .clipShape(RoundedRectangle(cornerRadius: 22))
        .overlay {
            RoundedRectangle(cornerRadius: 22)
                .stroke(Theme.hairline, lineWidth: 1)
        }
        .shadow(radius: 20)
        #if os(macOS)
            .focusable()
            .focusEffectDisabled()
            .focused($focused)
            .onKeyPress(.escape) {
                answer(play: false)
                return .handled
            }
            .onAppear { focused = true }
        #else
            .padding(.horizontal, 16)
        #endif
    }

    private func answer(play: Bool) {
        playbackStore.dismissSidePausePrompt(prompt)
        do {
            try playback.answerSidePausePrompt(
                keepPausing: keepPausing,
                play: play
            )
        }
        catch {
            showError(error)
        }
    }
}

#if os(iOS)
    extension BridgeSidePausePrompt: Identifiable {}
#endif

extension View {
    public func sidePausePromptAlert(
        showError: @escaping @MainActor (any Error) -> Void
    ) -> some View {
        modifier(SidePausePromptAlert(showError: showError))
    }
}
