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
    /// A fixed footprint for BOTH states (glyph and loading spinner), so the
    /// swap between them can't re-lay-out the surrounding transport row —
    /// without it, a seek's loading flash makes the neighbor buttons jump.
    /// Also pads the tappable region out past the glyph. `nil` keeps the
    /// intrinsic sizes.
    public let targetSize: CGFloat?
    public let onToggle: () -> Void

    public init(
        isPlaying: Bool,
        isLoading: Bool,
        glyphFont: Font,
        spinnerControlSize: ControlSize,
        targetSize: CGFloat? = nil,
        onToggle: @escaping () -> Void
    ) {
        self.isPlaying = isPlaying
        self.isLoading = isLoading
        self.glyphFont = glyphFont
        self.spinnerControlSize = spinnerControlSize
        self.targetSize = targetSize
        self.onToggle = onToggle
    }

    public var body: some View {
        if isLoading {
            ProgressView()
                .controlSize(spinnerControlSize)
                .frame(width: targetSize, height: targetSize)
                .help("Loading")
                .accessibilityLabel("Loading")
        }
        else {
            Button(action: onToggle) {
                Image(systemName: isPlaying ? "pause.fill" : "play.fill")
                    .font(glyphFont)
                    .frame(width: targetSize, height: targetSize)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(isPlaying ? "Pause" : "Play")
            .accessibilityLabel(isPlaying ? "Pause" : "Play")
        }
    }
}
