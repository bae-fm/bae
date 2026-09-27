import SwiftUI

/// The prompt core raises when playback pauses at the end of a side or disc,
/// drawn as a card because a system alert cannot hold its checkbox.
public struct SidePausePromptAlert: ViewModifier {
    @Environment(PlaybackStore.self)
    private var playbackStore

    /// Shows a failed write of the pause-between-sides setting.
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
    private static let width: CGFloat = 500

    let prompt: BridgeSidePausePrompt
    let showError: @MainActor (any Error) -> Void

    @Environment(Playback.self)
    private var playback
    @Environment(PlaybackStore.self)
    private var playbackStore

    /// Starts checked because the prompt only appears while the setting is on;
    /// written only when the prompt is answered.
    @State
    private var keepPausing = true

    #if os(macOS)
        @FocusState
        private var focused: Bool
    #endif

    var body: some View {
        ZStack {
            Theme.scrim
                .ignoresSafeArea()
                .onTapGesture { answer(play: false) }

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
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let countdown = prompt.countdown {
                SidePauseCountdownLine(prompt: prompt, countdown: countdown)
            }

            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Toggle(isOn: $keepPausing) {
                    Text(verbatim: prompt.keepPausingLabel())
                }
                #if os(macOS)
                    .toggleStyle(.checkbox)
                #endif
                // Unchecking turns the setting off, which also ends these
                // prompts, so the card says where to turn it back on.
                if !keepPausing {
                    Text("You can turn this back on in Settings › Playback.")
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            HStack(spacing: ThemeSpace.related) {
                Spacer()
                Button("Close") { answer(play: false) }
                    .buttonStyle(.bordered)
                    .keyboardShortcut(.cancelAction)

                Button("Play") { answer(play: true) }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.top, ThemeSpace.related)
        }
        .padding(ThemeSpace.section)
        #if os(macOS)
            .frame(width: Self.width, alignment: .leading)
        #else
            .frame(maxWidth: Self.width, alignment: .leading)
        #endif
        .background(Theme.surfaceElevated)
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.panel))
        .overlay {
            RoundedRectangle(cornerRadius: ThemeRadius.panel)
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
            .padding(.horizontal, ThemeSpace.edge)
        #endif
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

/// The countdown line, redrawn as each second before core's deadline runs out;
/// core, not this view, starts the next side or disc.
private struct SidePauseCountdownLine: View {
    let prompt: BridgeSidePausePrompt
    let countdown: BridgeSideCountdown

    var body: some View {
        TimelineView(.periodic(from: tickAnchor, by: 1)) { context in
            Text(verbatim: prompt.countdownLine(countdown, at: context.date))
                .themeText(.strong)
                .monospacedDigit()
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.updatesFrequently)
        }
    }

    /// An hour before the deadline, so ticks land on whole seconds before it
    /// and the anchor is in the past for any countdown the settings offer.
    private var tickAnchor: Date {
        countdown.resumesAt.addingTimeInterval(-3600)
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
