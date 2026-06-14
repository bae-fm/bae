import SwiftUI

/// The play/pause toggle, replaced by a spinner while core is preparing or
/// buffering the track (initial load, or a seek to a position not yet
/// downloaded). Shared across the macOS now-playing bar and the iOS compact bar
/// / full-screen player, which differ only in glyph and spinner size.
struct PlayPauseControl: View {
    let isPlaying: Bool
    let isLoading: Bool
    let glyphFont: Font
    let spinnerControlSize: ControlSize
    let onToggle: () -> Void

    var body: some View {
        if isLoading {
            ProgressView()
                .controlSize(spinnerControlSize)
                .help("Loading")
                .accessibilityLabel("Loading")
        }
        else {
            Button(action: onToggle) {
                Image(systemName: isPlaying ? "pause.fill" : "play.fill")
                    .font(glyphFont)
            }
            .buttonStyle(.plain)
            .help(isPlaying ? "Pause" : "Play")
            .accessibilityLabel(isPlaying ? "Pause" : "Play")
        }
    }
}
