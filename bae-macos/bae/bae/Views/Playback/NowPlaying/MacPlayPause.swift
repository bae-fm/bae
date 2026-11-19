import BaeKit
import SwiftUI

/// The 48pt circle on the tile colour that plays and pauses; while the track
/// arrives from storage an accent arc turns round it.
struct MacPlayPause: View {
    let isPlaying: Bool
    /// Whether the loading track waits on storage.
    let waitsOnStorage: Bool
    let onToggle: () -> Void

    var body: some View {
        PlayPauseControl(
            isPlaying: isPlaying,
            glyphFont: .system(size: 20, weight: .medium),
            targetSize: 48,
            onToggle: onToggle
        )
        .foregroundStyle(.primary)
        .background(Circle().fill(Theme.tile))
        .overlay {
            StorageWaitArc(waits: waitsOnStorage)
        }
    }
}
