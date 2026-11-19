import BaeKit
import SwiftUI

/// The transport row: shuffle, previous, play and pause, next, repeat. While
/// core has not said the repeat mode, repeat's place stays empty.
struct MacTransport: View {
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

            navButton(
                "backward.fill",
                help: "Previous",
                action: onPrevious
            )

            MacPlayPause(
                isPlaying: isPlaying,
                waitsOnStorage: waitsOnStorage,
                onToggle: onPlayPause
            )

            navButton(
                "forward.fill",
                help: "Next",
                action: onNext
            )

            if let repeatMode {
                repeatButton(repeatMode)
            }
            else {
                Color.clear
                    .frame(
                        width: ThemeSize.hitTarget,
                        height: ThemeSize.hitTarget
                    )
            }
        }
    }

    private var shuffleButton: some View {
        Button {
            onSetShuffle(!shuffled)
        } label: {
            toggleLabel("shuffle")
        }
        .buttonStyle(IconHoverButtonStyle(isOn: shuffled).accentFocus())
        .help(shuffled ? "Turn off shuffle" : "Shuffle")
        .accessibilityLabel(shuffled ? "Turn off shuffle" : "Shuffle")
    }

    private func repeatButton(_ repeatMode: BridgeRepeatMode) -> some View {
        let active = repeatMode != .off
        let glyph = repeatMode == .track ? "repeat.1" : "repeat"
        return Button {
            onSetRepeatMode(bridgeNextRepeatMode(mode: repeatMode))
        } label: {
            toggleLabel(glyph)
        }
        .buttonStyle(IconHoverButtonStyle(isOn: active).accentFocus())
        .help(Self.repeatHelp(repeatMode))
        .accessibilityLabel("Repeat")
        .accessibilityValue(Self.repeatValue(repeatMode))
    }

    /// The slot shuffle and repeat share; `IconHoverButtonStyle` draws it
    /// accent on an accent tint when the button is on.
    private func toggleLabel(_ systemName: String) -> some View {
        Image(systemName: systemName)
            .themeIcon(.large)
            .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
            .contentShape(Rectangle())
    }

    private func navButton(
        _ systemName: String,
        help: LocalizedStringKey,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemName)
                .themeIcon(.large)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle().accentFocus())
        .help(help)
        .accessibilityLabel(help)
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
