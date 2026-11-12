import SwiftUI

/// The play/pause toggle, shared by the now-playing bar and the queue, which
/// differ only in glyph size. While core loads a track it shows pause, and a
/// press lands the load paused.
public struct PlayPauseControl: View {
    public let isPlaying: Bool
    public let glyphFont: Font
    /// A fixed footprint for the glyph, which also pads the tappable region
    /// out past it. `nil` keeps the intrinsic size.
    public let targetSize: CGFloat?
    public let onToggle: () -> Void

    public init(
        isPlaying: Bool,
        glyphFont: Font,
        targetSize: CGFloat? = nil,
        onToggle: @escaping () -> Void
    ) {
        self.isPlaying = isPlaying
        self.glyphFont = glyphFont
        self.targetSize = targetSize
        self.onToggle = onToggle
    }

    public var body: some View {
        Button(action: onToggle) {
            Image(systemName: isPlaying ? "pause.fill" : "play.fill")
                .font(glyphFont)
                .frame(width: targetSize, height: targetSize)
                .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        .help(isPlaying ? "Pause" : "Play")
        .accessibilityLabel(isPlaying ? "Pause" : "Play")
    }
}
