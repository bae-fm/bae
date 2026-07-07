import SwiftUI

/// The play/pause toggle, replaced by a spinner while core is preparing or
/// buffering the track (initial load, or a seek to a position not yet
/// downloaded). Shared across the macOS now-playing bar and the iOS compact bar
/// / full-screen player, which differ only in glyph and spinner size.
public struct PlayPauseControl: View {
    public let isPlaying: Bool
    public let isLoading: Bool
    public let glyphFont: Font
    public let spinnerControlSize: ControlSize
    public let onToggle: () -> Void

    public init(
        isPlaying: Bool,
        isLoading: Bool,
        glyphFont: Font,
        spinnerControlSize: ControlSize,
        onToggle: @escaping () -> Void
    ) {
        self.isPlaying = isPlaying
        self.isLoading = isLoading
        self.glyphFont = glyphFont
        self.spinnerControlSize = spinnerControlSize
        self.onToggle = onToggle
    }

    public var body: some View {
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
