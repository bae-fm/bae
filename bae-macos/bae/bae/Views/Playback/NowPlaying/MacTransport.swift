import BaeKit
import SwiftUI

/// The transport row: shuffle, previous, play and pause, next, repeat. While
/// core has not said the repeat mode, repeat's place stays empty.
struct MacTransport: View {
    /// The room one icon control takes, holding a control's place while
    /// there is nothing to show in it.
    static var controlSpace: some View {
        Color.clear
            .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
    }

    let isPlaying: Bool
    /// Whether the loading track waits on storage.
    let waitsOnStorage: Bool
    /// Whether shuffle is on, with or without a playing context.
    let shuffled: Bool
    /// The repeat mode; `nil` until core says it.
    let repeatMode: BridgeRepeatMode?
    let onPlayPause: () -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onSetShuffle: (Bool) -> Void
    /// The repeat mode the repeat button steps to.
    let onSetRepeatMode: (BridgeRepeatMode) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.section) {
            shuffleButton

            MacIconButton(
                symbol: "backward.fill",
                label: "Previous",
                action: onPrevious
            )

            MacPlayPause(
                isPlaying: isPlaying,
                waitsOnStorage: waitsOnStorage,
                onToggle: onPlayPause
            )

            MacIconButton(
                symbol: "forward.fill",
                label: "Next",
                action: onNext
            )

            if let repeatMode {
                repeatButton(repeatMode)
            }
            else {
                Self.controlSpace
            }
        }
    }

    private var shuffleButton: some View {
        MacIconButton(
            symbol: "shuffle",
            label: shuffled ? "Turn off shuffle" : "Shuffle",
            active: shuffled
        ) {
            onSetShuffle(!shuffled)
        }
    }

    private func repeatButton(_ repeatMode: BridgeRepeatMode) -> some View {
        MacIconButton(
            symbol: repeatMode == .track ? "repeat.1" : "repeat",
            label: "Repeat",
            help: Self.repeatHelp(repeatMode),
            active: repeatMode != .off
        ) {
            onSetRepeatMode(bridgeNextRepeatMode(mode: repeatMode))
        }
        .accessibilityValue(Self.repeatValue(repeatMode))
    }

    /// The repeat button's tooltip: one whole string per mode, so each
    /// language orders its own words.
    private static func repeatHelp(
        _ repeatMode: BridgeRepeatMode
    ) -> LocalizedStringKey {
        switch repeatMode {
        case .off:
            "Repeat: off"
        case .context:
            "Repeat: all"
        case .track:
            "Repeat: one"
        }
    }

    /// The mode as the button's accessibility value, which VoiceOver reads
    /// after the name Repeat and announces when it changes.
    private static func repeatValue(
        _ repeatMode: BridgeRepeatMode
    ) -> LocalizedStringKey {
        switch repeatMode {
        case .off:
            "Off"
        case .context:
            "All"
        case .track:
            "One"
        }
    }
}
