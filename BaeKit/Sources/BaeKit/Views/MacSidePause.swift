import SwiftUI

/// The prompt core raises when playback pauses at the end of a side or disc,
/// drawn as a card because a system alert cannot hold its checkbox.
public struct MacSidePauseContainer: ViewModifier {
    @Environment(PlaybackStore.self)
    private var playbackStore

    /// Shows a failed write of the pause-between-sides setting.
    let showError: @MainActor (any Error) -> Void

    public func body(content: Content) -> some View {
        content
            .disabled(playbackStore.presentedSidePausePrompt != nil)
            .overlay(alignment: .top) {
                if let prompt = playbackStore.presentedSidePausePrompt {
                    SidePausePromptCard(
                        prompt: prompt,
                        showError: showError
                    )
                    .id(prompt.id)
                }
            }
    }
}

/// The card's state and its answer: the box starts checked because the prompt
/// only appears while the setting is on, and the setting is written only when
/// the prompt is answered.
private struct SidePausePromptCard: View {
    let prompt: BridgeSidePausePrompt
    let showError: @MainActor (any Error) -> Void

    @Environment(Playback.self)
    private var playback
    @Environment(PlaybackStore.self)
    private var playbackStore

    @State
    private var keepPausing = true

    var body: some View {
        MacSidePause(
            prompt: prompt,
            keepPausing: $keepPausing,
            onAnswer: answer
        )
    }

    private func answer(play: Bool) {
        playbackStore.dismissSidePausePrompt(prompt)
        let keepPausing = keepPausing
        Task {
            do {
                try await playback.answerSidePausePrompt(
                    keepPausing: keepPausing,
                    play: play
                )
            }
            catch {
                showError(error)
            }
        }
    }
}

/// The prompt as drawn: the card over the scrim, with the box's state and the
/// answer given to it.
public struct MacSidePause: View {
    private static let width: CGFloat = 500

    let prompt: BridgeSidePausePrompt
    @Binding
    var keepPausing: Bool
    /// Answers the prompt: `true` plays, `false` closes it.
    let onAnswer: (_ play: Bool) -> Void

    @FocusState
    private var focused: Bool

    public init(
        prompt: BridgeSidePausePrompt,
        keepPausing: Binding<Bool>,
        onAnswer: @escaping (_ play: Bool) -> Void
    ) {
        self.prompt = prompt
        _keepPausing = keepPausing
        self.onAnswer = onAnswer
    }

    public var body: some View {
        // The card hangs from the top of the window over the scrim, which
        // takes the tap that closes it.
        ZStack(alignment: .top) {
            MacScrim()
                .contentShape(Rectangle())
                .onTapGesture { onAnswer(false) }

            card
        }
        .accessibilityAddTraits(.isModal)
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text(verbatim: prompt.title())
                .themeText(.heading)
            Text(verbatim: localizedCoreString("core.playback.pause.message"))
                .themeText(.body)
                .foregroundStyle(Theme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
            if let countdown = prompt.countdown {
                SidePauseCountdownLine(countdown: countdown)
            }

            MacCheckbox(isOn: $keepPausing) {
                // The board stretches the box's row across the card.
                Text(verbatim: prompt.keepPausingLabel())
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            // Unchecking turns the setting off, which also ends these
            // prompts, so the card says where to turn it back on.
            if !keepPausing {
                Text("You can turn this back on in Settings › Playback.")
                    .themeText(.body)
                    .foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack(spacing: ThemeSpace.related) {
                Spacer()
                MacButton("Close") { onAnswer(false) }
                    .keyboardShortcut(.cancelAction)

                MacButton("Play", kind: .primary) { onAnswer(true) }
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.top, ThemeSpace.related)
        }
        .padding(ThemeSpace.section)
        .frame(width: Self.width, alignment: .leading)
        .sheetFace()
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.escape) {
            onAnswer(false)
            return .handled
        }
        .onAppear { focused = true }
    }
}

/// The countdown line, redrawn as each second before core's deadline runs out;
/// core, not this view, starts the next side or disc.
private struct SidePauseCountdownLine: View {
    let countdown: BridgeCountdown

    var body: some View {
        CountdownTimeline(countdown) { line in
            Text(verbatim: line)
                .themeText(.strong)
                .monospacedDigit()
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.updatesFrequently)
        }
    }
}

extension View {
    public func sidePausePromptAlert(
        showError: @escaping @MainActor (any Error) -> Void
    ) -> some View {
        modifier(MacSidePauseContainer(showError: showError))
    }
}
